//! Wire format serialization for ZMQ messages
//!
//! Supports MessagePack (production) and JSON (debugging).

use anyhow::{Context, Result};
use serde::{de::DeserializeOwned, Serialize};

use crate::ipc::Message;

/// Wire format for serializing messages
pub trait WireFormat {
    fn serialize<T: Serialize>(msg: &Message<T>) -> Result<Vec<u8>>;
    fn deserialize<T: DeserializeOwned>(data: &[u8]) -> Result<Message<T>>;
}

/// MessagePack format - fast and compact for production
pub struct MsgPackFormat;

impl WireFormat for MsgPackFormat {
    fn serialize<T: Serialize>(msg: &Message<T>) -> Result<Vec<u8>> {
        rmp_serde::to_vec(msg).context("failed to serialize message to MessagePack")
    }

    fn deserialize<T: DeserializeOwned>(data: &[u8]) -> Result<Message<T>> {
        rmp_serde::from_slice(data).context("failed to deserialize MessagePack message")
    }
}

/// JSON format - readable for debugging
pub struct JsonFormat;

impl WireFormat for JsonFormat {
    fn serialize<T: Serialize>(msg: &Message<T>) -> Result<Vec<u8>> {
        serde_json::to_vec(msg).context("failed to serialize message to JSON")
    }

    fn deserialize<T: DeserializeOwned>(data: &[u8]) -> Result<Message<T>> {
        serde_json::from_slice(data).context("failed to deserialize JSON message")
    }
}

/// Helper functions for the default format (MessagePack)
pub fn serialize<T: Serialize>(msg: &Message<T>) -> Result<Vec<u8>> {
    MsgPackFormat::serialize(msg)
}

pub fn deserialize<T: DeserializeOwned>(data: &[u8]) -> Result<Message<T>> {
    MsgPackFormat::deserialize(data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::{Beat, ShellRequest};
    use uuid::Uuid;

    #[test]
    fn test_msgpack_roundtrip() {
        let session = Uuid::new_v4();
        let msg = Message::new(
            session,
            "shell_request",
            ShellRequest::Seek { beat: Beat(16.0) },
        );

        let bytes = MsgPackFormat::serialize(&msg).unwrap();
        let decoded: Message<ShellRequest> = MsgPackFormat::deserialize(&bytes).unwrap();

        assert_eq!(decoded.header.session, session);
        match decoded.content {
            ShellRequest::Seek { beat } => assert_eq!(beat.0, 16.0),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_json_roundtrip() {
        let session = Uuid::new_v4();
        let msg = Message::new(
            session,
            "shell_request",
            ShellRequest::SetTempo { bpm: 120.0 },
        );

        let bytes = JsonFormat::serialize(&msg).unwrap();
        let decoded: Message<ShellRequest> = JsonFormat::deserialize(&bytes).unwrap();

        assert_eq!(decoded.header.session, session);
        match decoded.content {
            ShellRequest::SetTempo { bpm } => assert_eq!(bpm, 120.0),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_msgpack_is_compact() {
        let session = Uuid::new_v4();
        let msg = Message::new(session, "shell_request", ShellRequest::Play);

        let msgpack_bytes = MsgPackFormat::serialize(&msg).unwrap();
        let json_bytes = JsonFormat::serialize(&msg).unwrap();

        assert!(msgpack_bytes.len() < json_bytes.len());
    }
}
