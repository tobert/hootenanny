use chrono::{DateTime, Utc};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::types::{OutputChunk, SessionId, SessionStatus};

/// An agent session - a managed conversation with an LLM
pub struct AgentSession {
    pub id: SessionId,
    pub backend_id: String,
    pub conversation_id: String,
    pub status: SessionStatus,
    pub enable_tools: bool,
    pub max_tool_iterations: u32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,

    /// Output chunks for polling
    output_chunks: Vec<OutputChunk>,

    /// Current generation config
    pub temperature: Option<f32>,
}

impl AgentSession {
    pub fn new(
        backend_id: &str,
        conversation_id: &str,
        enable_tools: bool,
        max_tool_iterations: u32,
    ) -> Self {
        Self {
            id: SessionId::new(),
            backend_id: backend_id.to_string(),
            conversation_id: conversation_id.to_string(),
            status: SessionStatus::Idle,
            enable_tools,
            max_tool_iterations,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            output_chunks: Vec::new(),
            temperature: None,
        }
    }

    /// Add an output chunk
    pub fn push_chunk(&mut self, chunk: OutputChunk) {
        self.output_chunks.push(chunk);
        self.updated_at = Utc::now();
    }

    /// Get chunks since index
    pub fn chunks_since(&self, index: usize) -> &[OutputChunk] {
        if index < self.output_chunks.len() {
            &self.output_chunks[index..]
        } else {
            &[]
        }
    }

    /// Total chunk count
    pub fn chunk_count(&self) -> usize {
        self.output_chunks.len()
    }

    /// Mark session as generating
    pub fn set_generating(&mut self) {
        self.status = SessionStatus::Generating;
        self.updated_at = Utc::now();
    }

    /// Mark session as executing tools
    pub fn set_executing_tools(&mut self) {
        self.status = SessionStatus::ExecutingTools;
        self.updated_at = Utc::now();
    }

    /// Mark session as finished
    pub fn set_finished(&mut self) {
        self.status = SessionStatus::Finished;
        self.updated_at = Utc::now();
    }

    /// Mark session as failed
    pub fn set_failed(&mut self, error: &str) {
        self.status = SessionStatus::Failed;
        self.push_chunk(OutputChunk::Error {
            message: error.to_string(),
            index: self.output_chunks.len(),
        });
        self.updated_at = Utc::now();
    }

    /// Mark session as cancelled
    pub fn set_cancelled(&mut self) {
        self.status = SessionStatus::Cancelled;
        self.updated_at = Utc::now();
    }

    /// Reset to idle for new turn
    pub fn set_idle(&mut self) {
        self.status = SessionStatus::Idle;
        self.updated_at = Utc::now();
    }

    /// Clear output chunks for a new turn
    pub fn clear_chunks(&mut self) {
        self.output_chunks.clear();
        self.updated_at = Utc::now();
    }
}

/// Thread-safe session handle
pub type SessionHandle = Arc<RwLock<AgentSession>>;
