"""
ZMQ ROUTER service base class for hootpy

Provides the foundation for building model services that speak HOOT01 protocol.
Handles ZMQ socket setup, message routing, heartbeats, and error handling.
"""

import asyncio
import json
import logging
import signal
import time
from abc import ABC, abstractmethod
from contextlib import asynccontextmanager
from dataclasses import dataclass, field
from threading import Lock
from typing import Any

import zmq
import zmq.asyncio

from .errors import ServiceError, ToolError
from .frame import Command, ContentType, HootFrame
from .protocol import decode_tool_request, encode_error_response, encode_tool_response

log = logging.getLogger(__name__)


@dataclass
class ServiceConfig:
    """Configuration for a model service"""

    name: str
    endpoint: str = "tcp://127.0.0.1:5591"
    reconnect_ivl_ms: int = 1000
    reconnect_ivl_max_ms: int = 60000
    linger_ms: int = 0
    heartbeat_ivl_ms: int = 30000
    heartbeat_timeout_ms: int = 90000


class SingleJobGuard:
    """
    Ensures only one inference runs at a time.

    Model services should use this to prevent concurrent GPU access
    which could cause OOM or corrupted results.
    """

    def __init__(self):
        self._lock = Lock()
        self._busy = False

    def try_acquire(self) -> bool:
        """Try to acquire the guard. Returns False if already busy."""
        with self._lock:
            if self._busy:
                return False
            self._busy = True
            return True

    def release(self):
        """Release the guard."""
        with self._lock:
            self._busy = False

    @asynccontextmanager
    async def acquire_or_raise(self):
        """Context manager that raises ServiceError if busy."""
        if not self.try_acquire():
            raise ServiceError(
                message="Service is busy processing another request",
                service_name="",
                code="service_busy",
                retryable=True,
            )
        try:
            yield
        finally:
            self.release()


class ModelService(ABC):
    """
    Base class for HOOT01 model services.

    Subclasses implement:
    - load_model(): Load the ML model
    - handle_request(tool_name, params): Process a request
    - TOOLS: List of tool names this service handles

    Example:
        class MyService(ModelService):
            TOOLS = ["my_tool"]

            async def load_model(self):
                self.model = load_my_model()

            async def handle_request(self, tool_name: str, params: dict) -> dict:
                return {"result": self.model.predict(params["input"])}
    """

    TOOLS: list[str] = []

    def __init__(self, config: ServiceConfig):
        self.config = config
        self.ctx: zmq.asyncio.Context | None = None
        self.socket: zmq.asyncio.Socket | None = None
        self.job_guard = SingleJobGuard()
        self._running = False
        self._last_heartbeat = 0.0

    async def start(self):
        """Initialize and run the service"""
        log.info(f"Starting {self.config.name} service...")

        # Setup ZMQ
        self.ctx = zmq.asyncio.Context()
        self.socket = self.ctx.socket(zmq.ROUTER)
        self._configure_socket()
        self.socket.bind(self.config.endpoint)
        log.info(f"Bound to {self.config.endpoint}")

        # Load model
        log.info("Loading model...")
        await self.load_model()
        log.info("Model loaded")

        # Setup signal handlers
        loop = asyncio.get_event_loop()
        for sig in (signal.SIGINT, signal.SIGTERM):
            loop.add_signal_handler(sig, lambda: asyncio.create_task(self._shutdown()))

        # Enter event loop
        self._running = True
        log.info(f"🎵 {self.config.name} ready, listening for requests")
        await self._event_loop()

    async def _shutdown(self):
        """Graceful shutdown"""
        log.info("Shutting down...")
        self._running = False

    def _configure_socket(self):
        """Apply standard socket options (matches Rust socket_config.rs)"""
        self.socket.setsockopt(zmq.RECONNECT_IVL, self.config.reconnect_ivl_ms)
        self.socket.setsockopt(zmq.RECONNECT_IVL_MAX, self.config.reconnect_ivl_max_ms)
        self.socket.setsockopt(zmq.LINGER, self.config.linger_ms)
        self.socket.setsockopt(zmq.ROUTER_MANDATORY, 1)

    async def _event_loop(self):
        """Main event loop - receive frames, dispatch, reply"""
        while self._running:
            try:
                # Poll with timeout to allow shutdown checks
                if await self.socket.poll(timeout=1000):
                    frames = await self.socket.recv_multipart()
                    identity, frame = HootFrame.from_frames_with_identity(
                        [bytes(f) for f in frames]
                    )

                    if frame.command == Command.HEARTBEAT:
                        await self._handle_heartbeat(frame, identity)
                    elif frame.command == Command.REQUEST:
                        # Handle request - don't await to avoid blocking heartbeats
                        asyncio.create_task(
                            self._handle_request_safe(frame, identity)
                        )

            except zmq.ZMQError as e:
                if e.errno == zmq.ETERM:
                    break  # Context terminated
                log.error(f"ZMQ error: {e}")
            except Exception as e:
                log.exception(f"Event loop error: {e}")

        # Cleanup
        if self.socket:
            self.socket.close()
        if self.ctx:
            self.ctx.term()

    async def _handle_heartbeat(
        self, frame: HootFrame, identity: list[bytes] | None
    ):
        """Reply to heartbeat immediately"""
        reply = HootFrame.heartbeat(self.config.name)
        reply.request_id = frame.request_id
        reply.identity = identity
        await self._send_frame(reply)
        self._last_heartbeat = time.time()

    async def _handle_request_safe(
        self, frame: HootFrame, identity: list[bytes] | None
    ):
        """Handle request with error catching"""
        try:
            await self._handle_request(frame, identity)
        except Exception as e:
            log.exception(f"Request handler error: {e}")
            await self._send_error(
                frame, identity, "internal_error", str(e)
            )

    async def _handle_request(
        self, frame: HootFrame, identity: list[bytes] | None
    ):
        """Dispatch request based on content type"""
        if frame.content_type != ContentType.CAPNPROTO:
            await self._send_error(
                frame, identity, "invalid_content_type",
                f"Expected CapnProto, got {frame.content_type.name}"
            )
            return

        # Decode the request
        try:
            tool_name, params = decode_tool_request(frame.body)
        except Exception as e:
            await self._send_error(
                frame, identity, "decode_error", str(e)
            )
            return

        # Check if we handle this tool
        if tool_name not in self.TOOLS:
            await self._send_error(
                frame, identity, "unknown_tool",
                f"Unknown tool: {tool_name}"
            )
            return

        # Execute with job guard
        try:
            async with self.job_guard.acquire_or_raise():
                log.info(f"Handling {tool_name}")
                start_time = time.time()
                response = await self.handle_request(tool_name, params)
                elapsed = time.time() - start_time
                log.info(f"Completed {tool_name} in {elapsed:.2f}s")

                # Send response
                await self._send_response(frame, identity, tool_name, response)

        except ToolError as e:
            await self._send_error(frame, identity, e.category.value, e.message)
        except Exception as e:
            log.exception(f"Handler error for {tool_name}: {e}")
            await self._send_error(frame, identity, "internal_error", str(e))

    async def _send_frame(self, frame: HootFrame):
        """Send a frame, handling identity for ROUTER socket"""
        frames = frame.to_frames_with_identity()
        await self.socket.send_multipart(frames)

    async def _send_response(
        self,
        request: HootFrame,
        identity: list[bytes] | None,
        response_type: str,
        response_data: dict[str, Any],
    ):
        """Send a successful response"""
        body = encode_tool_response(
            request.request_id.bytes,
            response_type + "_response",  # e.g., rave_encode -> rave_encode_response
            response_data,
        )
        reply = HootFrame.reply(request.request_id, body)
        reply.identity = identity
        await self._send_frame(reply)

    async def _send_error(
        self,
        request: HootFrame,
        identity: list[bytes] | None,
        code: str,
        message: str,
    ):
        """Send an error response"""
        body = encode_error_response(request.request_id.bytes, code, message)
        reply = HootFrame.reply(request.request_id, body)
        reply.identity = identity
        await self._send_frame(reply)

    @abstractmethod
    async def load_model(self):
        """Load the ML model. Called once at startup."""
        ...

    @abstractmethod
    async def handle_request(self, tool_name: str, params: dict[str, Any]) -> dict[str, Any]:
        """
        Handle a tool request.

        Args:
            tool_name: Name of the tool being invoked
            params: Request parameters as a dict

        Returns:
            Response data as a dict

        Raises:
            ToolError: On expected errors (validation, not found, etc.)
            Exception: On unexpected errors (will be wrapped as internal error)
        """
        ...
