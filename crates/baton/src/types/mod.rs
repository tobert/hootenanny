//! MCP Protocol Types
//!
//! This module contains all type definitions for the MCP 2025-06-18 specification.
//! Types are organized by their role in the protocol:
//!
//! - `jsonrpc` - JSON-RPC 2.0 base types (requests, responses, errors)
//! - `error` - MCP error types with standard error codes
//! - `protocol` - Initialize handshake and capability negotiation
//! - `tool` - Tool definitions and call results
//! - `content` - Content types (text, image, audio, resource)
//! - `resource` - Resource definitions and contents
//! - `prompt` - Prompt templates and messages
//! - `progress` - Progress notifications for long-running operations

pub mod content;
pub mod error;
pub mod jsonrpc;
pub mod progress;
pub mod prompt;
pub mod protocol;
pub mod resource;
pub mod tool;

// Common types used across modules
use serde::{Deserialize, Serialize};

/// Optional annotations for content and resources.
/// Per MCP 2025-06-18 schema lines 4-26.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Annotations {
    /// Who the intended audience is: "user", "assistant", or both.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audience: Option<Vec<Role>>,

    /// Priority from 0.0 (least important) to 1.0 (most important).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<f64>,

    /// ISO 8601 timestamp of last modification.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_modified: Option<String>,
}

/// Role in a conversation - user or assistant.
/// Per MCP 2025-06-18 schema lines 1996-2003.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
}
