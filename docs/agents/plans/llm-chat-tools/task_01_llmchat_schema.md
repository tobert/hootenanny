# Task 01: Create llmchat crate with SQLite schema

## Goal

Establish the conversation persistence layer following audio-graph-mcp patterns.

## Files to Create

- `crates/llmchat/Cargo.toml`
- `crates/llmchat/src/lib.rs`
- `crates/llmchat/src/types.rs`
- `crates/llmchat/src/db.rs`

## Dependencies

```toml
[package]
name = "llmchat"
version = "0.1.0"
edition = "2021"

[dependencies]
rusqlite = { version = "0.32", features = ["bundled"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
anyhow = "1"
uuid = { version = "1", features = ["v4"] }
chrono = { version = "0.4", features = ["serde"] }

[dev-dependencies]
tempfile = "3"
```

## Key Types (types.rs)

```rust
use serde::{Deserialize, Serialize};

/// Unique identifier for a conversation
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConversationId(pub String);

impl ConversationId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }
}

/// Unique identifier for a message
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MessageId(pub String);

impl MessageId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }
}

/// Unique identifier for a tool call (matches OpenAI format)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ToolCallId(pub String);

/// Message role in conversation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

impl Role {
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::Tool => "tool",
        }
    }
}

/// A conversation with an LLM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: ConversationId,
    pub backend: String,
    pub model: Option<String>,
    pub system_prompt: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub metadata: serde_json::Value,
}

/// A message in a conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: MessageId,
    pub conversation_id: ConversationId,
    pub role: Role,
    pub content: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub token_count: Option<i64>,
}

/// A tool call made by the assistant
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: ToolCallId,
    pub message_id: MessageId,
    pub tool_name: String,
    pub arguments: serde_json::Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Result of a tool call
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub id: String,
    pub tool_call_id: ToolCallId,
    pub result_message_id: MessageId,
    pub output: String,
    pub is_error: bool,
    pub duration_ms: Option<i64>,
}

/// Parse SQLite datetime string to chrono DateTime
/// Shared helper for all db operations
pub fn parse_datetime(s: &str) -> chrono::DateTime<chrono::Utc> {
    chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
        .map(|dt| dt.and_utc())
        .unwrap_or_else(|_| chrono::Utc::now())
}
```

## SQLite Schema (db.rs)

```rust
use std::path::{Path, PathBuf};
use anyhow::{Context, Result};
use rusqlite::{Connection, OpenFlags};

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS conversations (
    id TEXT PRIMARY KEY,
    backend TEXT NOT NULL,
    model TEXT,
    system_prompt TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    metadata JSON NOT NULL DEFAULT '{}'
);
CREATE INDEX IF NOT EXISTS idx_conversations_updated ON conversations(updated_at DESC);

CREATE TABLE IF NOT EXISTS messages (
    id TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    role TEXT NOT NULL,
    content TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    token_count INTEGER
);
CREATE INDEX IF NOT EXISTS idx_messages_conversation ON messages(conversation_id, created_at);

CREATE TABLE IF NOT EXISTS tool_calls (
    id TEXT PRIMARY KEY,
    message_id TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    tool_name TEXT NOT NULL,
    arguments JSON NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_tool_calls_message ON tool_calls(message_id);

CREATE TABLE IF NOT EXISTS tool_results (
    id TEXT PRIMARY KEY,
    tool_call_id TEXT NOT NULL REFERENCES tool_calls(id) ON DELETE CASCADE,
    result_message_id TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    output TEXT NOT NULL,
    is_error BOOLEAN NOT NULL DEFAULT FALSE,
    duration_ms INTEGER
);
CREATE INDEX IF NOT EXISTS idx_tool_results_call ON tool_results(tool_call_id);
"#;

pub struct ConversationDb {
    path: PathBuf,
}

impl ConversationDb {
    /// Open database at path, creating if necessary
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref().to_path_buf();

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .context("Failed to create database directory")?;
        }

        let db = Self { path };
        db.initialize()?;
        Ok(db)
    }

    /// Create in-memory database for testing
    pub fn in_memory() -> Result<Self> {
        let db = Self {
            path: PathBuf::from(":memory:"),
        };
        db.initialize()?;
        Ok(db)
    }

    /// Get a connection (connection-per-call pattern)
    pub fn conn(&self) -> Result<Connection> {
        let conn = if self.path.to_str() == Some(":memory:") {
            Connection::open_in_memory()?
        } else {
            Connection::open_with_flags(
                &self.path,
                OpenFlags::SQLITE_OPEN_READ_WRITE
                    | OpenFlags::SQLITE_OPEN_CREATE
                    | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )?
        };

        // Enable WAL mode and set busy timeout
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "busy_timeout", 5000)?;
        conn.pragma_update(None, "foreign_keys", "ON")?;

        Ok(conn)
    }

    fn initialize(&self) -> Result<()> {
        let conn = self.conn()?;
        conn.execute_batch(SCHEMA)
            .context("Failed to initialize database schema")?;
        Ok(())
    }
}
```

## Reference Files

- `crates/audio-graph-mcp/src/db.rs` - SQLite + WAL pattern to follow
- `crates/audio-graph-mcp/src/types.rs` - Rich type patterns (newtypes for IDs)

## Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_open_in_memory() {
        let db = ConversationDb::in_memory().unwrap();
        let conn = db.conn().unwrap();

        // Verify tables exist
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='conversations'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_role_serialization() {
        assert_eq!(Role::System.as_str(), "system");
        assert_eq!(Role::User.as_str(), "user");
        assert_eq!(Role::Assistant.as_str(), "assistant");
        assert_eq!(Role::Tool.as_str(), "tool");
    }

    #[test]
    fn test_id_generation() {
        let id1 = ConversationId::new();
        let id2 = ConversationId::new();
        assert_ne!(id1, id2);
    }
}
```

## Acceptance Criteria

- [ ] Crate compiles and is added to workspace
- [ ] In-memory database works for tests
- [ ] WAL mode enabled by default
- [ ] Foreign keys enforced
- [ ] All four tables created with indexes
- [ ] Types implement Debug, Clone, Serialize, Deserialize
