//! HOOT01 Frame Protocol
//!
//! A hybrid frame-based protocol inspired by MDP (Majordomo Protocol) for ZMQ messaging.
//! Enables routing without deserialization, efficient heartbeats, and native binary payloads.
//!
//! ## Wire Format
//!
//! A HOOT01 message is a 7-frame ZMQ multipart message:
//!
//! ```text
//! Frame 0: Protocol version    "HOOT01" (6 bytes)
//! Frame 1: Command             2 bytes (big-endian u16)
//! Frame 2: Content-Type        2 bytes (big-endian u16)
//! Frame 3: Request ID          16 bytes (UUID)
//! ─── fixed-width above / variable-width below ───
//! Frame 4: Service name        UTF-8 string (variable)
//! Frame 5: Traceparent         UTF-8 string (variable, or empty)
//! Frame 6: Body                bytes (interpretation per Content-Type)
//! ```
//!
//! ## ROUTER Socket Handling
//!
//! When using ROUTER sockets, ZMQ prepends identity frame(s). We scan for "HOOT01"
//! to find frame 0, preserving identity frames for reply routing.

use bytes::{BufMut, Bytes, BytesMut};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use uuid::Uuid;

/// Protocol version - bump on breaking changes
pub const PROTOCOL_VERSION: &[u8] = b"HOOT01";

/// Number of frames in a HOOT01 message (excluding identity prefix)
pub const FRAME_COUNT: usize = 7;

/// Command types for the HOOT01 protocol (2 bytes, big-endian)
///
/// Inspired by MDP but simplified for our use case:
/// - No Partial/Final streaming (single Reply only)
/// - Unified protocol ID (no client/worker distinction)
/// - Request ID in frame for correlation without body deserialization
#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// Worker announces availability and capabilities (worker -> broker)
    Ready = 0x0001,
    /// Request from client or broker (broker -> worker)
    Request = 0x0002,
    /// Reply from worker (worker -> broker)
    Reply = 0x0003,
    /// Bidirectional liveness check
    Heartbeat = 0x0004,
    /// Graceful shutdown notification
    Disconnect = 0x0005,
}

impl Command {
    /// Parse a u16 into a Command
    pub fn from_u16(value: u16) -> Result<Self, FrameError> {
        match value {
            0x0001 => Ok(Command::Ready),
            0x0002 => Ok(Command::Request),
            0x0003 => Ok(Command::Reply),
            0x0004 => Ok(Command::Heartbeat),
            0x0005 => Ok(Command::Disconnect),
            other => Err(FrameError::InvalidCommand(other)),
        }
    }

    /// Convert Command to u16
    pub fn to_u16(self) -> u16 {
        self as u16
    }
}

/// Content type for body interpretation (2 bytes, big-endian)
///
/// Explicit content type field (not magic byte detection) for clarity and extensibility.
#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentType {
    /// No body (heartbeats, simple acks)
    Empty = 0x0000,
    /// MsgPack-encoded payload
    MsgPack = 0x0001,
    /// Raw binary (MIDI, audio, etc.)
    RawBinary = 0x0002,
    /// JSON (for debugging, future)
    Json = 0x0003,
}

impl ContentType {
    /// Parse a u16 into a ContentType
    pub fn from_u16(value: u16) -> Result<Self, FrameError> {
        match value {
            0x0000 => Ok(ContentType::Empty),
            0x0001 => Ok(ContentType::MsgPack),
            0x0002 => Ok(ContentType::RawBinary),
            0x0003 => Ok(ContentType::Json),
            other => Err(FrameError::InvalidContentType(other)),
        }
    }

    /// Convert ContentType to u16
    pub fn to_u16(self) -> u16 {
        self as u16
    }
}

/// A parsed HOOT01 multipart ZMQ message
#[derive(Debug, Clone)]
pub struct HootFrame {
    pub command: Command,
    pub content_type: ContentType,
    pub request_id: Uuid,
    pub service: String,
    pub traceparent: Option<String>,
    pub body: Bytes,
}

/// Payload for Ready command - worker announces capabilities
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReadyPayload {
    /// Protocol version (should be "HOOT01")
    pub protocol: String,
    /// List of tool names this worker provides
    pub tools: Vec<String>,
    /// Whether worker can handle ContentType::RawBinary
    pub accepts_binary: bool,
}

impl ReadyPayload {
    /// Create a new ReadyPayload with default protocol version
    pub fn new(tools: Vec<String>) -> Self {
        Self {
            protocol: String::from_utf8_lossy(PROTOCOL_VERSION).to_string(),
            tools,
            accepts_binary: true,
        }
    }
}

/// Errors during frame parsing
#[derive(Debug, thiserror::Error)]
pub enum FrameError {
    #[error("Invalid protocol version: expected HOOT01")]
    InvalidProtocol,
    #[error("Missing frame: {0}")]
    MissingFrame(&'static str),
    #[error("Invalid command: {0:#06x}")]
    InvalidCommand(u16),
    #[error("Invalid content type: {0:#06x}")]
    InvalidContentType(u16),
    #[error("Invalid UTF-8 in {0}")]
    InvalidUtf8(&'static str),
    #[error("Invalid UUID in request ID")]
    InvalidUuid,
    #[error("MsgPack decode error: {0}")]
    MsgPackDecode(#[from] rmp_serde::decode::Error),
    #[error("MsgPack encode error: {0}")]
    MsgPackEncode(#[from] rmp_serde::encode::Error),
    #[error("Content type mismatch: expected {expected:?}, got {actual:?}")]
    ContentTypeMismatch {
        expected: ContentType,
        actual: ContentType,
    },
    #[error("Frame too short: expected {expected} bytes, got {actual}")]
    FrameTooShort { expected: usize, actual: usize },
}

impl HootFrame {
    /// Parse a list of frame bytes into a HootFrame
    ///
    /// Scans for HOOT01 to find the protocol frame, handling any identity prefix
    /// that ROUTER sockets may add.
    pub fn from_frames(frames: &[Bytes]) -> Result<Self, FrameError> {
        let (_, frame) = Self::from_frames_with_identity(frames)?;
        Ok(frame)
    }

    /// Parse frames, returning identity frames separately (for ROUTER socket replies)
    ///
    /// Returns (identity_frames, parsed_frame) where identity_frames are any frames
    /// before the HOOT01 protocol marker.
    pub fn from_frames_with_identity(frames: &[Bytes]) -> Result<(Vec<Bytes>, Self), FrameError> {
        // Scan for HOOT01 to find start of our protocol
        let proto_idx = frames
            .iter()
            .position(|f| f.as_ref() == PROTOCOL_VERSION)
            .ok_or(FrameError::InvalidProtocol)?;

        // Identity frames are everything before protocol frame
        let identity: Vec<Bytes> = frames[..proto_idx].to_vec();

        // Ensure we have enough frames after protocol
        let hoot_frames = &frames[proto_idx..];
        if hoot_frames.len() < FRAME_COUNT {
            return Err(FrameError::MissingFrame("insufficient frames after HOOT01"));
        }

        // Frame 1: Command (2 bytes, big-endian)
        let cmd_frame = &hoot_frames[1];
        if cmd_frame.len() < 2 {
            return Err(FrameError::FrameTooShort {
                expected: 2,
                actual: cmd_frame.len(),
            });
        }
        let command = Command::from_u16(u16::from_be_bytes([cmd_frame[0], cmd_frame[1]]))?;

        // Frame 2: Content-Type (2 bytes, big-endian)
        let ctype_frame = &hoot_frames[2];
        if ctype_frame.len() < 2 {
            return Err(FrameError::FrameTooShort {
                expected: 2,
                actual: ctype_frame.len(),
            });
        }
        let content_type =
            ContentType::from_u16(u16::from_be_bytes([ctype_frame[0], ctype_frame[1]]))?;

        // Frame 3: Request ID (16 bytes UUID)
        let reqid_frame = &hoot_frames[3];
        if reqid_frame.len() < 16 {
            return Err(FrameError::FrameTooShort {
                expected: 16,
                actual: reqid_frame.len(),
            });
        }
        let request_id =
            Uuid::from_slice(&reqid_frame[..16]).map_err(|_| FrameError::InvalidUuid)?;

        // Frame 4: Service name (UTF-8)
        let service = std::str::from_utf8(&hoot_frames[4])
            .map_err(|_| FrameError::InvalidUtf8("service"))?
            .to_string();

        // Frame 5: Traceparent (UTF-8, may be empty)
        let trace_bytes = &hoot_frames[5];
        let traceparent = if trace_bytes.is_empty() {
            None
        } else {
            Some(
                std::str::from_utf8(trace_bytes)
                    .map_err(|_| FrameError::InvalidUtf8("traceparent"))?
                    .to_string(),
            )
        };

        // Frame 6: Body
        let body = hoot_frames[6].clone();

        Ok((
            identity,
            HootFrame {
                command,
                content_type,
                request_id,
                service,
                traceparent,
                body,
            },
        ))
    }

    /// Serialize to a list of frame bytes (7 frames)
    pub fn to_frames(&self) -> Vec<Bytes> {
        let mut frames = Vec::with_capacity(FRAME_COUNT);

        // Frame 0: Protocol version
        frames.push(Bytes::from_static(PROTOCOL_VERSION));

        // Frame 1: Command (2 bytes, big-endian)
        let mut cmd_buf = BytesMut::with_capacity(2);
        cmd_buf.put_u16(self.command.to_u16());
        frames.push(cmd_buf.freeze());

        // Frame 2: Content-Type (2 bytes, big-endian)
        let mut ctype_buf = BytesMut::with_capacity(2);
        ctype_buf.put_u16(self.content_type.to_u16());
        frames.push(ctype_buf.freeze());

        // Frame 3: Request ID (16 bytes)
        frames.push(Bytes::copy_from_slice(self.request_id.as_bytes()));

        // Frame 4: Service name
        frames.push(Bytes::from(self.service.clone()));

        // Frame 5: Traceparent (empty if None)
        frames.push(
            self.traceparent
                .as_ref()
                .map(|t| Bytes::from(t.clone()))
                .unwrap_or_else(Bytes::new),
        );

        // Frame 6: Body
        frames.push(self.body.clone());

        frames
    }

    /// Serialize with identity prefix (for ROUTER socket replies)
    pub fn to_frames_with_identity(&self, identity: &[Bytes]) -> Vec<Bytes> {
        let mut frames = identity.to_vec();
        frames.extend(self.to_frames());
        frames
    }

    /// Create a heartbeat frame (ContentType::Empty, no body)
    pub fn heartbeat(service: &str) -> Self {
        Self {
            command: Command::Heartbeat,
            content_type: ContentType::Empty,
            request_id: Uuid::new_v4(),
            service: service.to_string(),
            traceparent: None,
            body: Bytes::new(),
        }
    }

    /// Create a ready frame (worker registration)
    pub fn ready(service: &str, capabilities: &ReadyPayload) -> Result<Self, FrameError> {
        let body = rmp_serde::to_vec(capabilities)?;
        Ok(Self {
            command: Command::Ready,
            content_type: ContentType::MsgPack,
            request_id: Uuid::new_v4(),
            service: service.to_string(),
            traceparent: None,
            body: Bytes::from(body),
        })
    }

    /// Create a request frame with MsgPack payload
    pub fn request<T: Serialize>(service: &str, payload: &T) -> Result<Self, FrameError> {
        let body = rmp_serde::to_vec(payload)?;
        Ok(Self {
            command: Command::Request,
            content_type: ContentType::MsgPack,
            request_id: Uuid::new_v4(),
            service: service.to_string(),
            traceparent: None,
            body: Bytes::from(body),
        })
    }

    /// Create a request frame with raw binary body
    pub fn request_binary(service: &str, request_id: Uuid, data: Bytes) -> Self {
        Self {
            command: Command::Request,
            content_type: ContentType::RawBinary,
            request_id,
            service: service.to_string(),
            traceparent: None,
            body: data,
        }
    }

    /// Create a reply frame with MsgPack payload
    pub fn reply<T: Serialize>(request_id: Uuid, payload: &T) -> Result<Self, FrameError> {
        let body = rmp_serde::to_vec(payload)?;
        Ok(Self {
            command: Command::Reply,
            content_type: ContentType::MsgPack,
            request_id,
            service: String::new(), // Service not needed for replies
            traceparent: None,
            body: Bytes::from(body),
        })
    }

    /// Create a reply frame with raw binary body
    pub fn reply_binary(request_id: Uuid, data: Bytes) -> Self {
        Self {
            command: Command::Reply,
            content_type: ContentType::RawBinary,
            request_id,
            service: String::new(),
            traceparent: None,
            body: data,
        }
    }

    /// Create a disconnect frame
    pub fn disconnect(service: &str) -> Self {
        Self {
            command: Command::Disconnect,
            content_type: ContentType::Empty,
            request_id: Uuid::new_v4(),
            service: service.to_string(),
            traceparent: None,
            body: Bytes::new(),
        }
    }

    /// Set traceparent for distributed tracing
    pub fn with_traceparent(mut self, traceparent: impl Into<String>) -> Self {
        self.traceparent = Some(traceparent.into());
        self
    }

    /// Extract typed payload from MsgPack body (checks content_type)
    pub fn payload<T: DeserializeOwned>(&self) -> Result<T, FrameError> {
        if self.content_type != ContentType::MsgPack {
            return Err(FrameError::ContentTypeMismatch {
                expected: ContentType::MsgPack,
                actual: self.content_type,
            });
        }
        let payload: T = rmp_serde::from_slice(&self.body)?;
        Ok(payload)
    }

    /// Get raw body bytes (checks for RawBinary content type)
    pub fn raw_body(&self) -> Result<&Bytes, FrameError> {
        if self.content_type != ContentType::RawBinary {
            return Err(FrameError::ContentTypeMismatch {
                expected: ContentType::RawBinary,
                actual: self.content_type,
            });
        }
        Ok(&self.body)
    }

    /// Check if this is a heartbeat message (any command acts as heartbeat for liveness)
    pub fn is_heartbeat(&self) -> bool {
        self.command == Command::Heartbeat
    }

    /// Check if this is a liveness-indicating message (for Paranoid Pirate pattern)
    ///
    /// Per MDP spec: "Any received command except DISCONNECT acts as a heartbeat"
    pub fn indicates_liveness(&self) -> bool {
        self.command != Command::Disconnect
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_roundtrip() {
        assert_eq!(Command::Ready.to_u16(), 0x0001);
        assert_eq!(Command::Request.to_u16(), 0x0002);
        assert_eq!(Command::Reply.to_u16(), 0x0003);
        assert_eq!(Command::Heartbeat.to_u16(), 0x0004);
        assert_eq!(Command::Disconnect.to_u16(), 0x0005);

        assert_eq!(Command::from_u16(0x0001).unwrap(), Command::Ready);
        assert_eq!(Command::from_u16(0x0004).unwrap(), Command::Heartbeat);
        assert!(Command::from_u16(0xFFFF).is_err());
    }

    #[test]
    fn content_type_roundtrip() {
        assert_eq!(ContentType::Empty.to_u16(), 0x0000);
        assert_eq!(ContentType::MsgPack.to_u16(), 0x0001);
        assert_eq!(ContentType::RawBinary.to_u16(), 0x0002);
        assert_eq!(ContentType::Json.to_u16(), 0x0003);

        assert_eq!(ContentType::from_u16(0x0000).unwrap(), ContentType::Empty);
        assert_eq!(ContentType::from_u16(0x0002).unwrap(), ContentType::RawBinary);
        assert!(ContentType::from_u16(0xFFFF).is_err());
    }

    #[test]
    fn heartbeat_roundtrip() {
        let frame = HootFrame::heartbeat("hootenanny");
        let frames = frame.to_frames();

        assert_eq!(frames.len(), FRAME_COUNT);
        assert_eq!(frames[0].as_ref(), PROTOCOL_VERSION);

        let parsed = HootFrame::from_frames(&frames).unwrap();
        assert_eq!(parsed.command, Command::Heartbeat);
        assert_eq!(parsed.content_type, ContentType::Empty);
        assert_eq!(parsed.service, "hootenanny");
        assert!(parsed.body.is_empty());
    }

    #[test]
    fn request_with_payload_roundtrip() {
        use crate::Payload;

        let payload = Payload::Ping;
        let frame = HootFrame::request("hootenanny", &payload).unwrap();
        let frames = frame.to_frames();

        assert_eq!(frames.len(), FRAME_COUNT);

        let parsed = HootFrame::from_frames(&frames).unwrap();
        assert_eq!(parsed.command, Command::Request);
        assert_eq!(parsed.content_type, ContentType::MsgPack);

        let recovered: Payload = parsed.payload().unwrap();
        assert_eq!(recovered, Payload::Ping);
    }

    #[test]
    fn raw_binary_body() {
        let midi_bytes = b"MThd\x00\x00\x00\x06";
        let frame = HootFrame::reply_binary(Uuid::new_v4(), Bytes::from_static(midi_bytes));
        let frames = frame.to_frames();

        assert_eq!(frames.len(), FRAME_COUNT);

        let parsed = HootFrame::from_frames(&frames).unwrap();
        assert_eq!(parsed.content_type, ContentType::RawBinary);
        assert_eq!(parsed.raw_body().unwrap().as_ref(), midi_bytes);
    }

    #[test]
    fn content_type_explicit() {
        use crate::Payload;

        // MsgPack frame
        let frame = HootFrame::request("hootenanny", &Payload::Ping).unwrap();
        assert_eq!(frame.content_type, ContentType::MsgPack);

        // Empty frame (heartbeat)
        let hb = HootFrame::heartbeat("hootenanny");
        assert_eq!(hb.content_type, ContentType::Empty);
        assert!(hb.body.is_empty());

        // Binary frame
        let bin = HootFrame::reply_binary(Uuid::new_v4(), Bytes::from_static(b"MIDI"));
        assert_eq!(bin.content_type, ContentType::RawBinary);
    }

    #[test]
    fn ready_with_capabilities() {
        let caps = ReadyPayload {
            protocol: "HOOT01".into(),
            tools: vec!["orpheus_generate".into(), "cas_store".into()],
            accepts_binary: true,
        };
        let frame = HootFrame::ready("hootenanny", &caps).unwrap();
        assert_eq!(frame.command, Command::Ready);
        assert_eq!(frame.content_type, ContentType::MsgPack);

        let frames = frame.to_frames();
        let parsed = HootFrame::from_frames(&frames).unwrap();

        let recovered: ReadyPayload = parsed.payload().unwrap();
        assert_eq!(recovered.tools.len(), 2);
        assert!(recovered.accepts_binary);
    }

    #[test]
    fn identity_prefix_handling() {
        let frame = HootFrame::heartbeat("hootenanny");

        // Simulate ROUTER socket adding identity frames
        let identity1 = Bytes::from_static(b"\x00\x01\x02\x03\x04");
        let identity2 = Bytes::from_static(b"client-123");

        let frames_with_id = frame.to_frames_with_identity(&[identity1.clone(), identity2.clone()]);
        assert_eq!(frames_with_id.len(), FRAME_COUNT + 2);

        let (recovered_id, parsed) = HootFrame::from_frames_with_identity(&frames_with_id).unwrap();
        assert_eq!(recovered_id.len(), 2);
        assert_eq!(recovered_id[0], identity1);
        assert_eq!(recovered_id[1], identity2);
        assert_eq!(parsed.command, Command::Heartbeat);
        assert_eq!(parsed.service, "hootenanny");
    }

    #[test]
    fn traceparent_roundtrip() {
        let traceparent = "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01";
        let frame = HootFrame::heartbeat("hootenanny").with_traceparent(traceparent);

        let frames = frame.to_frames();
        let parsed = HootFrame::from_frames(&frames).unwrap();

        assert_eq!(parsed.traceparent.as_deref(), Some(traceparent));
    }

    #[test]
    fn content_type_mismatch_error() {
        let frame = HootFrame::heartbeat("hootenanny"); // ContentType::Empty
        let result: Result<ReadyPayload, _> = frame.payload();
        assert!(matches!(
            result,
            Err(FrameError::ContentTypeMismatch {
                expected: ContentType::MsgPack,
                actual: ContentType::Empty
            })
        ));
    }

    #[test]
    fn liveness_indication() {
        assert!(HootFrame::heartbeat("test").indicates_liveness());
        assert!(HootFrame::request::<()>("test", &()).unwrap().indicates_liveness());
        assert!(HootFrame::reply::<()>(Uuid::new_v4(), &()).unwrap().indicates_liveness());
        assert!(!HootFrame::disconnect("test").indicates_liveness());
    }

    #[test]
    fn disconnect_frame() {
        let frame = HootFrame::disconnect("hootenanny");
        assert_eq!(frame.command, Command::Disconnect);
        assert_eq!(frame.content_type, ContentType::Empty);
        assert_eq!(frame.service, "hootenanny");

        let frames = frame.to_frames();
        let parsed = HootFrame::from_frames(&frames).unwrap();
        assert_eq!(parsed.command, Command::Disconnect);
    }
}
