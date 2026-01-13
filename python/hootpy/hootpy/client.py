"""
ZMQ DEALER client with Lazy Pirate reliability pattern

Provides reliable request/reply communication with automatic retry
and reconnection on failures.
"""

import asyncio
import logging
import time
from dataclasses import dataclass
from typing import Any

import zmq
import zmq.asyncio

from .errors import ServiceError, TimeoutError, ToolError
from .frame import Command, ContentType, HootFrame
from .protocol import decode_envelope, encode_tool_request

log = logging.getLogger(__name__)


@dataclass
class ClientConfig:
    """Configuration for a HOOT01 client"""

    name: str
    endpoint: str
    timeout_ms: int = 30000
    max_retries: int = 3
    reconnect_ivl_ms: int = 1000
    reconnect_ivl_max_ms: int = 60000
    linger_ms: int = 0


class HootClient:
    """
    DEALER socket client with Lazy Pirate reliability pattern.

    Features:
    - Automatic retry on timeout
    - Exponential backoff between retries
    - Health tracking
    - Async request/reply

    Example:
        client = HootClient(ClientConfig(
            name="rave",
            endpoint="tcp://127.0.0.1:5591",
            timeout_ms=60000,
        ))
        await client.connect()

        response = await client.request("rave_encode", {"audio_hash": "abc123"})
    """

    def __init__(self, config: ClientConfig):
        self.config = config
        self.ctx: zmq.asyncio.Context | None = None
        self.socket: zmq.asyncio.Socket | None = None
        self._consecutive_failures = 0
        self._last_success = 0.0

    async def connect(self):
        """
        Connect to the service.

        Note: ZMQ connect() is non-blocking. The peer doesn't need to exist.
        This just sets up the socket for communication.
        """
        self.ctx = zmq.asyncio.Context()
        self.socket = self.ctx.socket(zmq.DEALER)
        self._configure_socket()
        self.socket.connect(self.config.endpoint)
        log.info(f"Connected to {self.config.endpoint}")

    def _configure_socket(self):
        """Apply standard socket options"""
        self.socket.setsockopt(zmq.RECONNECT_IVL, self.config.reconnect_ivl_ms)
        self.socket.setsockopt(zmq.RECONNECT_IVL_MAX, self.config.reconnect_ivl_max_ms)
        self.socket.setsockopt(zmq.LINGER, self.config.linger_ms)

    async def close(self):
        """Close the connection"""
        if self.socket:
            self.socket.close()
            self.socket = None
        if self.ctx:
            self.ctx.term()
            self.ctx = None

    async def request(
        self,
        tool_name: str,
        params: dict[str, Any],
        timeout_ms: int | None = None,
    ) -> dict[str, Any]:
        """
        Send a request and wait for response.

        Args:
            tool_name: Name of the tool to invoke
            params: Request parameters
            timeout_ms: Override default timeout (optional)

        Returns:
            Response data as a dict

        Raises:
            TimeoutError: If all retries exhausted
            ServiceError: If service returns an error
            ToolError: For other categorized errors
        """
        if not self.socket:
            raise ServiceError(
                message="Client not connected",
                service_name=self.config.name,
                code="not_connected",
            )

        timeout = timeout_ms or self.config.timeout_ms
        retry_delay = self.config.reconnect_ivl_ms / 1000.0  # Start delay in seconds

        for attempt in range(self.config.max_retries + 1):
            try:
                return await self._request_once(tool_name, params, timeout)
            except asyncio.TimeoutError:
                self._consecutive_failures += 1
                if attempt < self.config.max_retries:
                    log.warning(
                        f"Request to {self.config.name} timed out (attempt {attempt + 1}), "
                        f"retrying in {retry_delay:.1f}s..."
                    )
                    await asyncio.sleep(retry_delay)
                    retry_delay = min(
                        retry_delay * 2,
                        self.config.reconnect_ivl_max_ms / 1000.0,
                    )
                    # Reconnect socket for clean state
                    await self._reconnect()
                else:
                    raise TimeoutError(
                        message=f"Request to {self.config.name} timed out after {self.config.max_retries + 1} attempts",
                        timeout_ms=timeout,
                    )

        # Should not reach here, but just in case
        raise TimeoutError(
            message=f"Request to {self.config.name} failed",
            timeout_ms=timeout,
        )

    async def _request_once(
        self,
        tool_name: str,
        params: dict[str, Any],
        timeout_ms: int,
    ) -> dict[str, Any]:
        """Send a single request with timeout"""
        # Build request frame (body will be set after we have request ID)
        frame = HootFrame.request(
            service=self.config.name,
            body=b"",  # Placeholder
            content_type=ContentType.CAPNPROTO,
        )

        # Encode the tool request with the real request ID
        frame.body = encode_tool_request(
            request_id_bytes=frame.request_id.bytes,
            tool_name=tool_name,
            params=params,
        )

        # Send
        await self.socket.send_multipart(frame.to_frames())

        # Wait for reply with timeout
        if await self.socket.poll(timeout=timeout_ms):
            frames = await self.socket.recv_multipart()
            reply = HootFrame.from_frames([bytes(f) for f in frames])

            # Check for matching request ID
            if reply.request_id != frame.request_id:
                log.warning(
                    f"Request ID mismatch: expected {frame.request_id}, got {reply.request_id}"
                )

            # Decode response
            response_type, response_data = decode_envelope(reply.body)

            # Check for error
            if response_type == "error":
                raise ServiceError(
                    message=response_data.get("message", "Unknown error"),
                    service_name=self.config.name,
                    code=response_data.get("code", "unknown"),
                )

            # Success
            self._consecutive_failures = 0
            self._last_success = time.time()
            return response_data

        else:
            # Timeout
            raise asyncio.TimeoutError()

    async def _reconnect(self):
        """Reconnect the socket for clean state"""
        if self.socket:
            self.socket.close()
        self.socket = self.ctx.socket(zmq.DEALER)
        self._configure_socket()
        self.socket.connect(self.config.endpoint)
        log.debug(f"Reconnected to {self.config.endpoint}")

    @property
    def is_healthy(self) -> bool:
        """Check if the client is considered healthy"""
        # Unhealthy if too many consecutive failures
        return self._consecutive_failures < 5

    @property
    def consecutive_failures(self) -> int:
        """Number of consecutive failed requests"""
        return self._consecutive_failures


async def request(
    endpoint: str,
    tool_name: str,
    params: dict[str, Any],
    timeout_ms: int = 30000,
) -> dict[str, Any]:
    """
    Convenience function for one-shot requests.

    Creates a temporary client, sends request, and cleans up.

    Example:
        response = await hootpy.client.request(
            "tcp://127.0.0.1:5591",
            "rave_encode",
            {"audio_hash": "abc123"},
        )
    """
    client = HootClient(ClientConfig(
        name="oneshot",
        endpoint=endpoint,
        timeout_ms=timeout_ms,
    ))
    await client.connect()
    try:
        return await client.request(tool_name, params, timeout_ms)
    finally:
        await client.close()
