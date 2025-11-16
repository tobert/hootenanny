//! MCP Musical Extensions
//!
//! This module defines the musical extensions to the Model-Context-Protocol.

use crate::domain::context::MusicalContext;
use crate::domain::messages::JamMessage;
use crate::domain::Event;
use crate::conversation::{AgentId, BranchId, NodeId};
use resonode::MusicalTime;
use rmcp::rpc;

/// A stream of events.
pub type EventStream = Vec<Event>;
pub type ConversationId = String;

#[rpc]
pub trait MusicalMCP {
    /// Create a new conversation with a given musical context.
    async fn create_conversation(&self, context: MusicalContext) -> ConversationId;

    /// Join an existing conversation.
    async fn join_conversation(&self, id: ConversationId, agent: AgentId) -> anyhow::Result<()>;

    /// Fork a branch in the conversation tree.
    async fn fork_branch(&self, from: NodeId, reason: String) -> BranchId;

    /// Merge two branches in the conversation tree.
    async fn merge_branches(&self, from: BranchId, into: BranchId) -> NodeId;

    /// Prune a branch from the conversation tree.
    async fn prune_branch(&self, branch: BranchId) -> anyhow::Result<()>;

    /// Add a musical event to a branch.
    async fn add_event(&self, branch: BranchId, event: Event) -> NodeId;

    /// Evaluate a branch.
    async fn evaluate_branch(&self, branch: BranchId) -> f32;

    /// Get the musical context at a specific time.
    async fn get_context(&self, at_time: MusicalTime) -> MusicalContext;

    /// Subscribe to a stream of events.
    async fn subscribe_events(&self) -> EventStream;

    /// Broadcast a message to all agents in the conversation.
    async fn broadcast_message(&self, msg: JamMessage) -> anyhow::Result<()>;
}
