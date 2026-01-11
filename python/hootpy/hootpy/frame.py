"""
HOOT01 Frame Protocol

A hybrid frame-based protocol for ZMQ messaging. Enables routing without
deserialization, efficient heartbeats, and native binary payloads.

Wire Format:
    Frame 0: Protocol version    "HOOT01" (6 bytes)
    Frame 1: Command             2 bytes (big-endian u16)
    Frame 2: Content-Type        2 bytes (big-endian u16)
    Frame 3: Request ID          16 bytes (UUID)
    Frame 4: Service name        UTF-8 string (variable)
    Frame 5: Traceparent         UTF-8 string (variable, or empty)
    Frame 6: Body                bytes (interpretation per Content-Type)

When using ROUTER sockets, ZMQ prepends identity frame(s). We scan for
"HOOT01" to find frame 0, preserving identity frames for reply routing.
"""

from dataclasses import dataclass
from enum import IntEnum
import struct
import uuid

PROTOCOL_VERSION = b"HOOT01"
FRAME_COUNT = 7


class Command(IntEnum):
    """Command types for HOOT01 protocol (2 bytes, big-endian)"""

    READY = 0x0001  # Worker announces availability
    REQUEST = 0x0002  # Request from client or broker
    REPLY = 0x0003  # Reply from worker
    HEARTBEAT = 0x0004  # Bidirectional liveness check
    DISCONNECT = 0x0005  # Graceful shutdown


class ContentType(IntEnum):
    """Content type for body interpretation (2 bytes, big-endian)"""

    EMPTY = 0x0000  # No body (heartbeats, simple acks)
    CAPNPROTO = 0x0001  # Cap'n Proto-encoded payload
    RAW_BINARY = 0x0002  # Raw binary (MIDI, audio, etc.)
    JSON = 0x0003  # JSON (for debugging, future)


class FrameError(Exception):
    """Error during frame parsing"""

    pass


@dataclass(slots=True)
class HootFrame:
    """A parsed HOOT01 multipart ZMQ message"""

    command: Command
    content_type: ContentType
    request_id: uuid.UUID
    service: str
    traceparent: str
    body: bytes
    identity: list[bytes] | None = None

    def to_frames(self) -> list[bytes]:
        """Serialize to ZMQ multipart message (7 frames)"""
        return [
            PROTOCOL_VERSION,
            struct.pack(">H", self.command),
            struct.pack(">H", self.content_type),
            self.request_id.bytes,
            self.service.encode("utf-8"),
            self.traceparent.encode("utf-8"),
            self.body,
        ]

    def to_frames_with_identity(self) -> list[bytes]:
        """Serialize with identity prefix (for ROUTER socket replies)"""
        if self.identity:
            return self.identity + self.to_frames()
        return self.to_frames()

    @classmethod
    def from_frames(cls, frames: list[bytes]) -> "HootFrame":
        """Parse ZMQ multipart message, discarding identity prefix"""
        _, frame = cls.from_frames_with_identity(frames)
        return frame

    @classmethod
    def from_frames_with_identity(
        cls, frames: list[bytes]
    ) -> tuple[list[bytes] | None, "HootFrame"]:
        """Parse frames, returning identity frames separately (for ROUTER replies)"""
        # Scan for HOOT01 to find start of protocol
        proto_idx = None
        for i, f in enumerate(frames):
            if f == PROTOCOL_VERSION:
                proto_idx = i
                break

        if proto_idx is None:
            raise FrameError("Invalid protocol version: expected HOOT01")

        # Identity frames are everything before protocol frame
        identity = frames[:proto_idx] if proto_idx > 0 else None

        # Ensure we have enough frames after protocol
        hoot_frames = frames[proto_idx:]
        if len(hoot_frames) < FRAME_COUNT:
            raise FrameError(
                f"Missing frame: insufficient frames after HOOT01 "
                f"(got {len(hoot_frames)}, need {FRAME_COUNT})"
            )

        # Frame 1: Command (2 bytes, big-endian)
        cmd_frame = hoot_frames[1]
        if len(cmd_frame) < 2:
            raise FrameError(f"Frame too short: command needs 2 bytes, got {len(cmd_frame)}")
        try:
            command = Command(struct.unpack(">H", cmd_frame[:2])[0])
        except ValueError as e:
            raise FrameError(f"Invalid command: {e}")

        # Frame 2: Content-Type (2 bytes, big-endian)
        ctype_frame = hoot_frames[2]
        if len(ctype_frame) < 2:
            raise FrameError(f"Frame too short: content-type needs 2 bytes, got {len(ctype_frame)}")
        try:
            content_type = ContentType(struct.unpack(">H", ctype_frame[:2])[0])
        except ValueError as e:
            raise FrameError(f"Invalid content type: {e}")

        # Frame 3: Request ID (16 bytes UUID)
        reqid_frame = hoot_frames[3]
        if len(reqid_frame) < 16:
            raise FrameError(f"Frame too short: request ID needs 16 bytes, got {len(reqid_frame)}")
        try:
            request_id = uuid.UUID(bytes=reqid_frame[:16])
        except ValueError as e:
            raise FrameError(f"Invalid UUID in request ID: {e}")

        # Frame 4: Service name (UTF-8)
        try:
            service = hoot_frames[4].decode("utf-8")
        except UnicodeDecodeError as e:
            raise FrameError(f"Invalid UTF-8 in service name: {e}")

        # Frame 5: Traceparent (UTF-8, may be empty)
        try:
            traceparent = hoot_frames[5].decode("utf-8")
        except UnicodeDecodeError as e:
            raise FrameError(f"Invalid UTF-8 in traceparent: {e}")

        # Frame 6: Body
        body = hoot_frames[6]

        return (
            identity,
            cls(
                command=command,
                content_type=content_type,
                request_id=request_id,
                service=service,
                traceparent=traceparent,
                body=body,
                identity=identity,
            ),
        )

    @classmethod
    def heartbeat(cls, service: str) -> "HootFrame":
        """Create a heartbeat frame"""
        return cls(
            command=Command.HEARTBEAT,
            content_type=ContentType.EMPTY,
            request_id=uuid.uuid4(),
            service=service,
            traceparent="",
            body=b"",
        )

    @classmethod
    def request(
        cls,
        service: str,
        body: bytes,
        content_type: ContentType = ContentType.CAPNPROTO,
        traceparent: str = "",
    ) -> "HootFrame":
        """Create a request frame"""
        return cls(
            command=Command.REQUEST,
            content_type=content_type,
            request_id=uuid.uuid4(),
            service=service,
            traceparent=traceparent,
            body=body,
        )

    @classmethod
    def reply(
        cls,
        request_id: uuid.UUID,
        body: bytes,
        content_type: ContentType = ContentType.CAPNPROTO,
    ) -> "HootFrame":
        """Create a reply frame"""
        return cls(
            command=Command.REPLY,
            content_type=content_type,
            request_id=request_id,
            service="",
            traceparent="",
            body=body,
        )

    @classmethod
    def disconnect(cls, service: str) -> "HootFrame":
        """Create a disconnect frame"""
        return cls(
            command=Command.DISCONNECT,
            content_type=ContentType.EMPTY,
            request_id=uuid.uuid4(),
            service=service,
            traceparent="",
            body=b"",
        )

    def is_heartbeat(self) -> bool:
        """Check if this is a heartbeat message"""
        return self.command == Command.HEARTBEAT

    def indicates_liveness(self) -> bool:
        """Check if this message indicates liveness (any command except DISCONNECT)"""
        return self.command != Command.DISCONNECT
