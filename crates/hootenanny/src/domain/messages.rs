//! Agent communication protocol for musical collaboration.

use crate::conversation::{AgentId, BranchId, ForkReason, NodeId};
use crate::domain::Event;
use rmcp::schemars;
use serde::{Deserialize, Serialize};

/// Timing intention for a planned event.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub enum TimingIntention {
    Now,
    NextBeat,
    NextBar,
    AfterNode(NodeId),
}

/// Musical interpretation of a heard event.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub enum MusicalInterpretation {
    Understood,
    Confused,
    Inspired,
    Dissonant,
}

/// Type of response to a heard event.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub enum ResponseType {
    WillComplement,
    WillContrast,
    WillImitate,
    WillDevelop,
    WillRest,
}

/// Core message types for agent communication.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub enum JamMessage {
    /// Announce a planned musical event.
    Intention {
        agent: AgentId,
        planned: Event,
        confidence: f32,
        timing: TimingIntention,
    },

    /// Acknowledge a heard event.
    Acknowledgment {
        agent: AgentId,
        heard: NodeId,
        interpretation: MusicalInterpretation,
        response_type: ResponseType,
    },

    /// Suggest a musical event to another agent.
    Suggestion {
        agent: AgentId,
        suggestion: Event,
        rationale: String,
        target_branch: Option<BranchId>,
    },

    /// Request to fork the conversation tree.
    ForkRequest {
        agent: AgentId,
        from_node: NodeId,
        reason: ForkReason,
    },

    /// Evaluate a branch.
    BranchEvaluation {
        agent: AgentId,
        branch: BranchId,
        score: f32,
        continue_branch: bool,
    },
}
