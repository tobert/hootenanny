use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rusqlite::{Connection, OpenFlags};

use crate::types::*;

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

/// Database for conversation persistence with connection-per-call pattern.
/// Each method creates a fresh connection with WAL mode enabled.
pub struct ConversationDb {
    path: PathBuf,
}

impl ConversationDb {
    /// Open database at path, creating if necessary
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref().to_path_buf();

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).context("Failed to create database directory")?;
        }

        let db = Self { path };
        db.initialize()?;
        Ok(db)
    }

    /// Create a temporary database file with a unique name.
    /// Each call creates a new database - suitable for tests.
    pub fn in_memory() -> Result<Self> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);

        let temp_dir = std::env::temp_dir();
        let unique_id = COUNTER.fetch_add(1, Ordering::SeqCst);
        let db_name = format!("llmchat_{}_{}.db", std::process::id(), unique_id);
        let path = temp_dir.join(db_name);
        Self::open(path)
    }

    /// Get a connection with WAL mode and busy timeout
    pub fn conn(&self) -> Result<Connection> {
        let conn = Connection::open_with_flags(
            &self.path,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .with_context(|| format!("Failed to open database: {:?}", self.path))?;

        conn.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA busy_timeout = 5000;
            PRAGMA foreign_keys = ON;
        ",
        )?;

        Ok(conn)
    }

    fn initialize(&self) -> Result<()> {
        let conn = self.conn()?;
        conn.execute_batch(SCHEMA)
            .context("Failed to initialize database schema")?;
        Ok(())
    }

    /// Create a new conversation
    pub fn create_conversation(
        &self,
        backend: &str,
        model: Option<&str>,
        system_prompt: Option<&str>,
    ) -> Result<Conversation> {
        let id = ConversationId::new();
        let conn = self.conn()?;

        conn.execute(
            "INSERT INTO conversations (id, backend, model, system_prompt) VALUES (?1, ?2, ?3, ?4)",
            (&id.0, backend, model, system_prompt),
        )?;

        self.get_conversation(&id.0)?
            .ok_or_else(|| anyhow::anyhow!("Failed to retrieve created conversation"))
    }

    /// Get a conversation by ID
    pub fn get_conversation(&self, id: &str) -> Result<Option<Conversation>> {
        let conn = self.conn()?;

        let mut stmt = conn.prepare(
            "SELECT id, backend, model, system_prompt, created_at, updated_at, metadata
             FROM conversations WHERE id = ?1",
        )?;

        let result = stmt.query_row([id], |row| {
            Ok(Conversation {
                id: ConversationId(row.get(0)?),
                backend: row.get(1)?,
                model: row.get(2)?,
                system_prompt: row.get(3)?,
                created_at: parse_datetime(&row.get::<_, String>(4)?),
                updated_at: parse_datetime(&row.get::<_, String>(5)?),
                metadata: serde_json::from_str(&row.get::<_, String>(6)?).unwrap_or_default(),
            })
        });

        match result {
            Ok(conv) => Ok(Some(conv)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// List recent conversations
    pub fn list_recent(&self, limit: usize) -> Result<Vec<Conversation>> {
        let conn = self.conn()?;

        let mut stmt = conn.prepare(
            "SELECT id, backend, model, system_prompt, created_at, updated_at, metadata
             FROM conversations ORDER BY updated_at DESC LIMIT ?1",
        )?;

        let rows = stmt.query_map([limit], |row| {
            Ok(Conversation {
                id: ConversationId(row.get(0)?),
                backend: row.get(1)?,
                model: row.get(2)?,
                system_prompt: row.get(3)?,
                created_at: parse_datetime(&row.get::<_, String>(4)?),
                updated_at: parse_datetime(&row.get::<_, String>(5)?),
                metadata: serde_json::from_str(&row.get::<_, String>(6)?).unwrap_or_default(),
            })
        })?;

        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Update conversation's updated_at timestamp
    pub fn touch_conversation(&self, id: &str) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            "UPDATE conversations SET updated_at = datetime('now') WHERE id = ?1",
            [id],
        )?;
        Ok(())
    }

    /// Delete a conversation and all related data
    pub fn delete_conversation(&self, id: &str) -> Result<bool> {
        let conn = self.conn()?;
        let rows = conn.execute("DELETE FROM conversations WHERE id = ?1", [id])?;
        Ok(rows > 0)
    }

    /// Append a message to a conversation
    pub fn append_message(
        &self,
        conversation_id: &str,
        role: Role,
        content: Option<&str>,
    ) -> Result<Message> {
        let id = MessageId::new();
        let conn = self.conn()?;

        conn.execute(
            "INSERT INTO messages (id, conversation_id, role, content) VALUES (?1, ?2, ?3, ?4)",
            (&id.0, conversation_id, role.as_str(), content),
        )?;

        self.touch_conversation(conversation_id)?;

        self.get_message(&id.0)?
            .ok_or_else(|| anyhow::anyhow!("Failed to retrieve created message"))
    }

    /// Get a message by ID
    pub fn get_message(&self, id: &str) -> Result<Option<Message>> {
        let conn = self.conn()?;

        let mut stmt = conn.prepare(
            "SELECT id, conversation_id, role, content, created_at, token_count
             FROM messages WHERE id = ?1",
        )?;

        let result = stmt.query_row([id], |row| {
            Ok(Message {
                id: MessageId(row.get(0)?),
                conversation_id: ConversationId(row.get(1)?),
                role: row.get::<_, String>(2)?.parse().unwrap_or(Role::User),
                content: row.get(3)?,
                created_at: parse_datetime(&row.get::<_, String>(4)?),
                token_count: row.get(5)?,
            })
        });

        match result {
            Ok(msg) => Ok(Some(msg)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Get all messages in a conversation
    pub fn get_messages(&self, conversation_id: &str) -> Result<Vec<Message>> {
        let conn = self.conn()?;

        let mut stmt = conn.prepare(
            "SELECT id, conversation_id, role, content, created_at, token_count
             FROM messages WHERE conversation_id = ?1 ORDER BY created_at ASC",
        )?;

        let rows = stmt.query_map([conversation_id], |row| {
            Ok(Message {
                id: MessageId(row.get(0)?),
                conversation_id: ConversationId(row.get(1)?),
                role: row.get::<_, String>(2)?.parse().unwrap_or(Role::User),
                content: row.get(3)?,
                created_at: parse_datetime(&row.get::<_, String>(4)?),
                token_count: row.get(5)?,
            })
        })?;

        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Get messages fitting within a token budget (most recent first)
    pub fn get_context_window(
        &self,
        conversation_id: &str,
        max_tokens: usize,
    ) -> Result<Vec<Message>> {
        let all_messages = self.get_messages(conversation_id)?;

        let mut total_tokens = 0usize;
        let mut context = Vec::new();

        for msg in all_messages.into_iter().rev() {
            let msg_tokens = msg.token_count.unwrap_or(0) as usize;
            if total_tokens + msg_tokens > max_tokens && !context.is_empty() {
                break;
            }
            total_tokens += msg_tokens;
            context.push(msg);
        }

        context.reverse();
        Ok(context)
    }

    /// Update token count for a message
    pub fn update_token_count(&self, message_id: &str, token_count: i64) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            "UPDATE messages SET token_count = ?1 WHERE id = ?2",
            (token_count, message_id),
        )?;
        Ok(())
    }

    /// Record a tool call from an assistant message
    pub fn record_tool_call(&self, message_id: &str, call: NewToolCall) -> Result<ToolCall> {
        let conn = self.conn()?;

        conn.execute(
            "INSERT INTO tool_calls (id, message_id, tool_name, arguments) VALUES (?1, ?2, ?3, ?4)",
            (
                &call.id.0,
                message_id,
                &call.tool_name,
                serde_json::to_string(&call.arguments)?,
            ),
        )?;

        self.get_tool_call(&call.id.0)?
            .ok_or_else(|| anyhow::anyhow!("Failed to retrieve created tool call"))
    }

    /// Get a tool call by ID
    pub fn get_tool_call(&self, id: &str) -> Result<Option<ToolCall>> {
        let conn = self.conn()?;

        let mut stmt = conn.prepare(
            "SELECT id, message_id, tool_name, arguments, created_at
             FROM tool_calls WHERE id = ?1",
        )?;

        let result = stmt.query_row([id], |row| {
            Ok(ToolCall {
                id: ToolCallId(row.get(0)?),
                message_id: MessageId(row.get(1)?),
                tool_name: row.get(2)?,
                arguments: serde_json::from_str(&row.get::<_, String>(3)?).unwrap_or_default(),
                created_at: parse_datetime(&row.get::<_, String>(4)?),
            })
        });

        match result {
            Ok(tc) => Ok(Some(tc)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Record a tool result
    pub fn record_tool_result(
        &self,
        tool_call_id: &str,
        result_message_id: &str,
        output: &str,
        is_error: bool,
        duration_ms: Option<i64>,
    ) -> Result<()> {
        let id = uuid::Uuid::new_v4().to_string();
        let conn = self.conn()?;

        conn.execute(
            "INSERT INTO tool_results (id, tool_call_id, result_message_id, output, is_error, duration_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            (&id, tool_call_id, result_message_id, output, is_error, duration_ms),
        )?;

        Ok(())
    }

    /// Get tool calls for a message
    pub fn get_tool_calls_for_message(&self, message_id: &str) -> Result<Vec<ToolCall>> {
        let conn = self.conn()?;

        let mut stmt = conn.prepare(
            "SELECT id, message_id, tool_name, arguments, created_at
             FROM tool_calls WHERE message_id = ?1 ORDER BY created_at ASC",
        )?;

        let rows = stmt.query_map([message_id], |row| {
            Ok(ToolCall {
                id: ToolCallId(row.get(0)?),
                message_id: MessageId(row.get(1)?),
                tool_name: row.get(2)?,
                arguments: serde_json::from_str(&row.get::<_, String>(3)?).unwrap_or_default(),
                created_at: parse_datetime(&row.get::<_, String>(4)?),
            })
        })?;

        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Get result for a specific tool call
    pub fn get_tool_result(&self, tool_call_id: &str) -> Result<Option<ToolResult>> {
        let conn = self.conn()?;

        let mut stmt = conn.prepare(
            "SELECT id, tool_call_id, result_message_id, output, is_error, duration_ms
             FROM tool_results WHERE tool_call_id = ?1",
        )?;

        let result = stmt.query_row([tool_call_id], |row| {
            Ok(ToolResult {
                id: row.get(0)?,
                tool_call_id: ToolCallId(row.get(1)?),
                result_message_id: MessageId(row.get(2)?),
                output: row.get(3)?,
                is_error: row.get(4)?,
                duration_ms: row.get(5)?,
            })
        });

        match result {
            Ok(tr) => Ok(Some(tr)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Get tool_call_id for a tool result message
    pub fn get_tool_call_id_for_result_message(&self, result_message_id: &str) -> Result<Option<String>> {
        let conn = self.conn()?;

        let mut stmt = conn.prepare(
            "SELECT tool_call_id FROM tool_results WHERE result_message_id = ?1",
        )?;

        let result = stmt.query_row([result_message_id], |row| row.get::<_, String>(0));

        match result {
            Ok(id) => Ok(Some(id)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Get pending tool calls (calls without results) for a conversation
    pub fn get_pending_tool_calls(&self, conversation_id: &str) -> Result<Vec<ToolCall>> {
        let conn = self.conn()?;

        let mut stmt = conn.prepare(
            "SELECT tc.id, tc.message_id, tc.tool_name, tc.arguments, tc.created_at
             FROM tool_calls tc
             JOIN messages m ON tc.message_id = m.id
             LEFT JOIN tool_results tr ON tc.id = tr.tool_call_id
             WHERE m.conversation_id = ?1 AND tr.id IS NULL
             ORDER BY tc.created_at ASC",
        )?;

        let rows = stmt.query_map([conversation_id], |row| {
            Ok(ToolCall {
                id: ToolCallId(row.get(0)?),
                message_id: MessageId(row.get(1)?),
                tool_name: row.get(2)?,
                arguments: serde_json::from_str(&row.get::<_, String>(3)?).unwrap_or_default(),
                created_at: parse_datetime(&row.get::<_, String>(4)?),
            })
        })?;

        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_open_in_memory() {
        let db = ConversationDb::in_memory().unwrap();
        let conn = db.conn().unwrap();

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

    #[test]
    fn test_conversation_lifecycle() {
        let db = ConversationDb::in_memory().unwrap();

        let conv = db
            .create_conversation("deepseek", Some("v2-lite"), None)
            .unwrap();
        assert_eq!(conv.backend, "deepseek");

        let fetched = db.get_conversation(&conv.id.0).unwrap().unwrap();
        assert_eq!(fetched.id, conv.id);

        let recent = db.list_recent(10).unwrap();
        assert_eq!(recent.len(), 1);

        assert!(db.delete_conversation(&conv.id.0).unwrap());
        assert!(db.get_conversation(&conv.id.0).unwrap().is_none());
    }

    #[test]
    fn test_message_operations() {
        let db = ConversationDb::in_memory().unwrap();
        let conv = db.create_conversation("test", None, None).unwrap();

        let _m1 = db
            .append_message(&conv.id.0, Role::User, Some("Hello"))
            .unwrap();
        let _m2 = db
            .append_message(&conv.id.0, Role::Assistant, Some("Hi there!"))
            .unwrap();

        let messages = db.get_messages(&conv.id.0).unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, Role::User);
        assert_eq!(messages[1].role, Role::Assistant);
    }

    #[test]
    fn test_tool_call_tracking() {
        let db = ConversationDb::in_memory().unwrap();
        let conv = db.create_conversation("test", None, None).unwrap();

        let msg = db
            .append_message(&conv.id.0, Role::Assistant, None)
            .unwrap();

        let tc = db
            .record_tool_call(
                &msg.id.0,
                NewToolCall {
                    id: ToolCallId("call_123".to_string()),
                    tool_name: "orpheus_generate".to_string(),
                    arguments: serde_json::json!({"model": "base"}),
                },
            )
            .unwrap();

        let pending = db.get_pending_tool_calls(&conv.id.0).unwrap();
        assert_eq!(pending.len(), 1);

        let result_msg = db
            .append_message(&conv.id.0, Role::Tool, Some("generated"))
            .unwrap();
        db.record_tool_result(&tc.id.0, &result_msg.id.0, "generated", false, Some(1500))
            .unwrap();

        let pending = db.get_pending_tool_calls(&conv.id.0).unwrap();
        assert_eq!(pending.len(), 0);
    }

    #[test]
    fn test_cascade_delete() {
        let db = ConversationDb::in_memory().unwrap();
        let conv = db.create_conversation("test", None, None).unwrap();

        let msg = db
            .append_message(&conv.id.0, Role::Assistant, None)
            .unwrap();
        db.record_tool_call(
            &msg.id.0,
            NewToolCall {
                id: ToolCallId("call_456".to_string()),
                tool_name: "test_tool".to_string(),
                arguments: serde_json::json!({}),
            },
        )
        .unwrap();

        db.delete_conversation(&conv.id.0).unwrap();

        let messages = db.get_messages(&conv.id.0).unwrap();
        assert!(messages.is_empty());
    }

    #[test]
    fn test_context_window() {
        let db = ConversationDb::in_memory().unwrap();
        let conv = db.create_conversation("test", None, None).unwrap();

        for i in 0..5 {
            let msg = db
                .append_message(&conv.id.0, Role::User, Some(&format!("Message {}", i)))
                .unwrap();
            db.update_token_count(&msg.id.0, 100).unwrap();
        }

        let context = db.get_context_window(&conv.id.0, 250).unwrap();
        assert!(context.len() <= 3);
    }
}
