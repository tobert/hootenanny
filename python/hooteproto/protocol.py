"""
HOOT01 ZMQ Protocol Implementation.

This module handles the wire protocol for communicating with Hootenanny
over ZeroMQ. It's stable and doesn't depend on schema generation.
"""
from __future__ import annotations

import uuid
from dataclasses import dataclass
from enum import IntEnum
from typing import Any

import msgpack
import zmq
import zmq.asyncio


PROTOCOL_VERSION = b"HOOT01"
FRAME_COUNT = 7


class Command(IntEnum):
    """HOOT01 command types."""
    READY = 0x0001
    REQUEST = 0x0002
    REPLY = 0x0003
    HEARTBEAT = 0x0004
    DISCONNECT = 0x0005


class ContentType(IntEnum):
    """HOOT01 content types."""
    EMPTY = 0x0000
    MSGPACK = 0x0001
    RAW_BINARY = 0x0002
    JSON = 0x0003


class ProtocolError(Exception):
    """Error in HOOT01 protocol handling."""
    pass


class HootError(Exception):
    """Error returned from a Hootenanny tool."""
    def __init__(self, code: str, message: str, details: Any = None):
        self.code = code
        self.message = message
        self.details = details
        super().__init__(f"[{code}] {message}")


@dataclass
class HootFrame:
    """A parsed HOOT01 frame."""
    command: Command
    content_type: ContentType
    request_id: bytes
    service: str
    traceparent: str | None
    body: bytes

    @classmethod
    def from_frames(cls, frames: list[bytes]) -> tuple[list[bytes], "HootFrame"]:
        """Parse ZMQ multipart message, returning (identity_frames, hoot_frame)."""
        # Find HOOT01 marker (skip identity frames from ROUTER)
        try:
            idx = next(i for i, f in enumerate(frames) if f == PROTOCOL_VERSION)
        except StopIteration:
            raise ProtocolError("No HOOT01 marker found in frames")

        identity = frames[:idx]
        hoot = frames[idx:]

        if len(hoot) < FRAME_COUNT:
            raise ProtocolError(f"Expected {FRAME_COUNT} frames, got {len(hoot)}")

        return identity, cls(
            command=Command(int.from_bytes(hoot[1], "big")),
            content_type=ContentType(int.from_bytes(hoot[2], "big")),
            request_id=hoot[3],
            service=hoot[4].decode("utf-8"),
            traceparent=hoot[5].decode("utf-8") or None,
            body=hoot[6],
        )

    def to_frames(self) -> list[bytes]:
        """Serialize to ZMQ multipart message."""
        return [
            PROTOCOL_VERSION,
            self.command.to_bytes(2, "big"),
            self.content_type.to_bytes(2, "big"),
            self.request_id,
            self.service.encode("utf-8"),
            (self.traceparent or "").encode("utf-8"),
            self.body,
        ]

    def to_frames_with_identity(self, identity: list[bytes]) -> list[bytes]:
        """Serialize with identity prefix for ROUTER replies."""
        return identity + self.to_frames()

    @classmethod
    def request(cls, service: str, tool: str, params: dict[str, Any]) -> "HootFrame":
        """Create a request frame."""
        payload = {"type": tool, **{k: v for k, v in params.items() if v is not None}}
        return cls(
            command=Command.REQUEST,
            content_type=ContentType.MSGPACK,
            request_id=uuid.uuid4().bytes,
            service=service,
            traceparent=None,
            body=msgpack.packb(payload),
        )

    @classmethod
    def heartbeat(cls, service: str) -> "HootFrame":
        """Create a heartbeat frame."""
        return cls(
            command=Command.HEARTBEAT,
            content_type=ContentType.EMPTY,
            request_id=uuid.uuid4().bytes,
            service=service,
            traceparent=None,
            body=b"",
        )

    def payload(self) -> dict[str, Any]:
        """Decode MsgPack body."""
        if self.content_type != ContentType.MSGPACK:
            raise ProtocolError(f"Expected MsgPack, got {self.content_type}")
        return msgpack.unpackb(self.body, raw=False)


class Connection:
    """Low-level ZMQ connection to Hootenanny."""

    def __init__(self, broker_url: str = "tcp://localhost:5555"):
        self.broker_url = broker_url
        self._ctx: zmq.asyncio.Context | None = None
        self._socket: zmq.asyncio.Socket | None = None

    async def connect(self) -> None:
        """Establish connection."""
        self._ctx = zmq.asyncio.Context()
        self._socket = self._ctx.socket(zmq.DEALER)
        self._socket.setsockopt(zmq.RCVTIMEO, 60000)
        self._socket.setsockopt(zmq.SNDTIMEO, 10000)
        self._socket.setsockopt(zmq.LINGER, 0)
        self._socket.connect(self.broker_url)

    async def close(self) -> None:
        """Close connection."""
        if self._socket:
            self._socket.close()
            self._socket = None
        if self._ctx:
            self._ctx.term()
            self._ctx = None

    async def call(self, tool: str, **kwargs) -> dict[str, Any]:
        """Send request, await reply, return result."""
        if not self._socket:
            raise ProtocolError("Not connected")

        frame = HootFrame.request("hootenanny", tool, kwargs)
        await self._socket.send_multipart(frame.to_frames())

        reply_frames = await self._socket.recv_multipart()
        _, reply = HootFrame.from_frames(reply_frames)

        result = reply.payload()

        # Handle ["type", data] format from hootenanny
        if isinstance(result, list) and len(result) >= 2 and isinstance(result[0], str):
            msg_type = result[0]
            data = result[1]

            if msg_type == "error":
                if isinstance(data, dict):
                    raise HootError(
                        data.get("code", "unknown"),
                        data.get("message", "Unknown error"),
                        data.get("details"),
                    )
                raise HootError("error", str(data))

            # Return the data payload directly
            return data

        # Handle dict error responses
        if isinstance(result, dict) and result.get("type") == "error":
            raise HootError(
                result.get("code", "unknown"),
                result.get("message", "Unknown error"),
                result.get("details"),
            )

        # Unwrap if wrapped in result/data, otherwise return as-is
        if isinstance(result, dict):
            return result.get("result", result.get("data", result))
        return result

    async def __aenter__(self) -> "Connection":
        await self.connect()
        return self

    async def __aexit__(self, *args) -> None:
        await self.close()
