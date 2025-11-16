use crate::conversation::{ConversationTree, ForkReason};
use crate::domain::{EmotionalVector, Event, AbstractEvent, IntentionEvent, ConcreteEvent};
use crate::persistence::conversation_store::ConversationStore;
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, Content, ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router, ErrorData as McpError, ServerHandler,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Shared state for the conversation tree and persistence.
#[derive(Debug)]
pub struct ConversationState {
    tree: ConversationTree,
    store: ConversationStore,
    current_branch: String,
}

impl ConversationState {
    pub fn new(state_dir: PathBuf) -> anyhow::Result<Self> {
        let store = ConversationStore::new(&state_dir)?;

        // Try to load existing tree, or create new one
        let tree = match store.load_tree()? {
            Some(tree) => {
                tracing::info!("✅ Loaded existing conversation tree with {} nodes", tree.nodes.len());
                tree
            }
            None => {
                tracing::info!("🌱 Creating new conversation tree");
                let root_event = Event::Abstract(AbstractEvent::Intention(IntentionEvent {
                    what: "root".to_string(),
                    how: "gently".to_string(),
                    emotion: EmotionalVector::neutral(),
                }));
                ConversationTree::new(
                    root_event,
                    "system".to_string(),
                    EmotionalVector::neutral(),
                )
            }
        };

        let current_branch = "main".to_string();

        Ok(Self {
            tree,
            store,
            current_branch,
        })
    }

    pub fn save(&mut self) -> anyhow::Result<()> {
        self.store.store_tree(&self.tree)?;
        self.store.flush()?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct EventDualityServer {
    tool_router: ToolRouter<Self>,
    state: Arc<Mutex<ConversationState>>,
}

/// Request to add a node to the conversation tree.
/// Flattened for easier MCP usage - construct AbstractEvent internally.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AddNodeRequest {
    #[schemars(description = "Note to play (C, D, E, F, G, A, B)")]
    pub what: String,

    #[schemars(description = "How to play it (softly, normally, boldly, questioning)")]
    pub how: String,

    #[schemars(description = "Valence: joy-sorrow axis, -1.0 to 1.0")]
    pub valence: f32,

    #[schemars(description = "Arousal: energy-stillness axis, 0.0 to 1.0")]
    pub arousal: f32,

    #[schemars(description = "Agency: initiative-responsiveness axis, -1.0 to 1.0")]
    pub agency: f32,

    #[schemars(description = "Agent ID (your identifier)")]
    pub agent_id: String,

    #[schemars(description = "Optional branch ID (defaults to current branch)")]
    pub branch_id: Option<String>,

    #[schemars(description = "Optional description of this musical contribution")]
    pub description: Option<String>,
}

/// Request to fork a conversation branch.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ForkRequest {
    #[schemars(description = "Name for the new branch")]
    pub branch_name: String,

    #[schemars(description = "Node ID to fork from (defaults to current head)")]
    pub from_node: Option<u64>,

    #[schemars(description = "Reason for forking")]
    pub reason_description: String,

    #[schemars(description = "Agent IDs participating in this fork")]
    pub participants: Vec<String>,
}

use crate::domain::context::MusicalContext;
use crate::domain::messages::JamMessage;
use resonode::MusicalTime;

pub type ConversationId = String;
pub type EventStream = Vec<Event>;

/// Request to merge two branches.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MergeRequest {
    #[schemars(description = "Branch to merge from")]
    pub from: String,
    #[schemars(description = "Branch to merge into")]
    pub into: String,
}

/// Request to prune a branch.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PruneRequest {
    #[schemars(description = "Branch to prune")]
    pub branch: String,
}

/// Request to evaluate a branch.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct EvaluateRequest {
    #[schemars(description = "Branch to evaluate")]
    pub branch: String,
}

/// Request to get the musical context.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GetContextRequest {
    #[schemars(description = "Time to get the context at")]
    pub at_time: String,
}

/// Request to broadcast a message.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct BroadcastMessageRequest {
    #[schemars(description = "Message to broadcast")]
    pub msg: String,
}

#[tool_router]
impl EventDualityServer {
    pub fn new_with_state(state: Arc<Mutex<ConversationState>>) -> Self {
        Self {
            tool_router: Self::tool_router(),
            state,
        }
    }

    pub fn new() -> Self {
        // Default stateless mode for backward compatibility
        let state_dir = std::env::temp_dir().join("hrmcp_default");
        std::fs::create_dir_all(&state_dir).expect("Failed to create temp dir");

        let state = ConversationState::new(state_dir).expect("Failed to create conversation state");
        Self::new_with_state(Arc::new(Mutex::new(state)))
    }

    #[tool(description = "Merge two branches in the conversation tree")]
    fn merge_branches(
        &self,
        Parameters(request): Parameters<MergeRequest>,
    ) -> Result<CallToolResult, McpError> {
        Ok(CallToolResult::success(vec![Content::text(
            "not implemented".to_string(),
        )]))
    }

    #[tool(description = "Prune a branch from the conversation tree")]
    fn prune_branch(
        &self,
        Parameters(request): Parameters<PruneRequest>,
    ) -> Result<CallToolResult, McpError> {
        Ok(CallToolResult::success(vec![Content::text(
            "not implemented".to_string(),
        )]))
    }

    #[tool(description = "Evaluate a branch")]
    fn evaluate_branch(
        &self,
        Parameters(request): Parameters<EvaluateRequest>,
    ) -> Result<CallToolResult, McpError> {
        Ok(CallToolResult::success(vec![Content::text(
            "not implemented".to_string(),
        )]))
    }

    #[tool(description = "Get the musical context at a specific time")]
    fn get_context(
        &self,
        Parameters(request): Parameters<GetContextRequest>,
    ) -> Result<CallToolResult, McpError> {
        Ok(CallToolResult::success(vec![Content::text(
            "not implemented".to_string(),
        )]))
    }

    #[tool(description = "Subscribe to a stream of events")]
    fn subscribe_events(&self) -> Result<CallToolResult, McpError> {
        Ok(CallToolResult::success(vec![Content::text(
            "not implemented".to_string(),
        )]))
    }

    #[tool(description = "Broadcast a message to all agents in the conversation")]
    fn broadcast_message(
        &self,
        Parameters(request): Parameters<BroadcastMessageRequest>,
    ) -> Result<CallToolResult, McpError> {
        Ok(CallToolResult::success(vec![Content::text(
            "not implemented".to_string(),
        )]))
    }

    #[tool(description = "Transform an intention into sound - the Alchemical transmutation of emotion to music")]
    #[tracing::instrument(
        name = "mcp.tool.play",
        skip(self, request),
        fields(
            music.note = %request.what,
            music.expression = %request.how,
            emotion.valence = request.valence,
            emotion.arousal = request.arousal,
            emotion.agency = request.agency,
            agent.id = %request.agent_id,
            sound.pitch = tracing::field::Empty,
            sound.velocity = tracing::field::Empty,
            sound.duration_ms = tracing::field::Empty,
        )
    )]
    fn play(
        &self,
        Parameters(request): Parameters<AddNodeRequest>,
    ) -> Result<CallToolResult, McpError> {
        // Reuse the flattened AddNodeRequest structure for simplicity
        let intention = AbstractEvent::Intention(IntentionEvent {
            what: request.what,
            how: request.how,
            emotion: EmotionalVector {
                valence: request.valence,
                arousal: request.arousal,
                agency: request.agency,
            },
        });

        let sound = intention.realize();

        // Record sound output in span
        let span = tracing::Span::current();
        if let ConcreteEvent::Note(note_event) = &sound {
            span.record("sound.pitch", note_event.note.pitch.midi_note_number);
            span.record("sound.velocity", note_event.note.velocity.0);
            if let resonode::Duration::Absolute(duration) = &note_event.duration {
                span.record("sound.duration_ms", duration.0);
            }
        }

        let result = serde_json::to_value(sound).unwrap();

        Ok(CallToolResult::success(vec![Content::text(
            result.to_string(),
        )]))
    }

    #[tool(description = "Add a musical intention to the conversation tree")]
    #[tracing::instrument(
        name = "mcp.tool.add_node",
        skip(self, request),
        fields(
            conversation.branch_id = tracing::field::Empty,
            conversation.node_id = tracing::field::Empty,
            music.note = %request.what,
            music.expression = %request.how,
            emotion.valence = request.valence,
            emotion.arousal = request.arousal,
            emotion.agency = request.agency,
            agent.id = %request.agent_id,
            has_description = request.description.is_some(),
            tree.nodes_before = tracing::field::Empty,
            tree.nodes_after = tracing::field::Empty,
        )
    )]
    fn add_node(
        &self,
        Parameters(request): Parameters<AddNodeRequest>,
    ) -> Result<CallToolResult, McpError> {
        let mut state = self.state.lock().unwrap();

        let branch_id = request.branch_id.unwrap_or_else(|| state.current_branch.clone());

        let nodes_before = state.tree.nodes.len();

        // Record branch resolution
        let span = tracing::Span::current();
        span.record("conversation.branch_id", &*branch_id);
        span.record("tree.nodes_before", nodes_before);

        // Construct AbstractEvent from flattened parameters
        let intention = AbstractEvent::Intention(IntentionEvent {
            what: request.what,
            how: request.how,
            emotion: EmotionalVector {
                valence: request.valence,
                arousal: request.arousal,
                agency: request.agency,
            },
        });

        let event = Event::Abstract(intention);

        let node_id = state
            .tree
            .add_node(
                &branch_id,
                event,
                request.agent_id.clone(),
                EmotionalVector::neutral(), // Use intention's emotion
                request.description,
            )
            .map_err(|e| McpError::parse_error(e, None))?;

        // Record node creation
        span.record("conversation.node_id", node_id);
        span.record("tree.nodes_after", state.tree.nodes.len());

        // Save to persistence
        state.save().map_err(|e| McpError::parse_error(e.to_string(), None))?;

        let result = serde_json::json!({
            "node_id": node_id,
            "branch_id": branch_id,
            "total_nodes": state.tree.nodes.len(),
        });

        Ok(CallToolResult::success(vec![Content::text(
            result.to_string(),
        )]))
    }

    #[tool(description = "Fork the conversation to explore an alternative musical direction")]
    #[tracing::instrument(
        name = "mcp.tool.fork_branch",
        skip(self, request),
        fields(
            conversation.branch_name = %request.branch_name,
            conversation.from_node = tracing::field::Empty,
            conversation.new_branch_id = tracing::field::Empty,
            fork.reason = %request.reason_description,
            fork.participant_count = request.participants.len(),
            fork.participants = ?request.participants,
            tree.branches_before = tracing::field::Empty,
            tree.branches_after = tracing::field::Empty,
        )
    )]
    fn fork_branch(
        &self,
        Parameters(request): Parameters<ForkRequest>,
    ) -> Result<CallToolResult, McpError> {
        let mut state = self.state.lock().unwrap();

        let from_node = request.from_node.unwrap_or_else(|| {
            state
                .tree
                .branches
                .get(&state.current_branch)
                .map(|b| b.head)
                .unwrap_or(0)
        });

        let branches_before = state.tree.branches.len();

        // Record fork point resolution
        let span = tracing::Span::current();
        span.record("conversation.from_node", from_node);
        span.record("tree.branches_before", branches_before);

        // Fork the tree in memory
        let branch_id = state
            .tree
            .fork_branch(
                from_node,
                request.branch_name.clone(),
                ForkReason::ExploreAlternative {
                    description: request.reason_description.clone(),
                },
                request.participants.clone(),
            )
            .map_err(|e| McpError::parse_error(e, None))?;

        // Record branch creation
        span.record("conversation.new_branch_id", &*branch_id);
        span.record("tree.branches_after", state.tree.branches.len());

        // Persist the entire updated tree
        state.save().map_err(|e| McpError::parse_error(e.to_string(), None))?;

        let result = serde_json::json!({
            "branch_id": branch_id,
            "total_branches": state.tree.branches.len(),
        });

        Ok(CallToolResult::success(vec![Content::text(
            result.to_string(),
        )]))
    }

    #[tool(description = "Get the current conversation tree status")]
    #[tracing::instrument(
        name = "mcp.tool.get_tree_status",
        skip(self),
        fields(
            tree.total_nodes = tracing::field::Empty,
            tree.total_branches = tracing::field::Empty,
            tree.current_branch = tracing::field::Empty,
        )
    )]
    fn get_tree_status(&self) -> Result<CallToolResult, McpError> {
        let state = self.state.lock().unwrap();

        // Record tree statistics
        let span = tracing::Span::current();
        span.record("tree.total_nodes", state.tree.nodes.len());
        span.record("tree.total_branches", state.tree.branches.len());
        span.record("tree.current_branch", &*state.current_branch);

        let result = serde_json::json!({
            "total_nodes": state.tree.nodes.len(),
            "total_branches": state.tree.branches.len(),
            "current_branch": state.current_branch,
        });

        Ok(CallToolResult::success(vec![Content::text(
            result.to_string(),
        )]))
    }
}

#[tool_handler]
impl ServerHandler for EventDualityServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "Event Duality MCP Server - Musical Alchemy & Temporal Forking\n\n\
                A conversational music generation server supporting multi-agent collaboration.\n\n\
                🎵 MUSICAL ALCHEMY\n\
                'play' - Transform intentions into sounds via EmotionalVector\n\
                  - what: Note (C, D, E, F, G, A, B)\n\
                  - how: Feeling (softly, normally, boldly, questioning)\n\
                  - emotion: {valence, arousal, agency}\n\n\
                🌳 CONVERSATION TREE\n\
                'add_node' - Add your musical contribution to the tree\n\
                  - intention: Your musical intention\n\
                  - agent_id: Your identifier\n\
                  - branch_id: Optional branch (defaults to current)\n\
                  - description: Optional note about your contribution\n\n\
                'fork_branch' - Explore alternative musical directions\n\
                  - branch_name: Name for new branch\n\
                  - from_node: Optional fork point (defaults to current head)\n\
                  - reason_description: Why fork?\n\
                  - participants: Agent IDs in this exploration\n\n\
                'get_tree_status' - View conversation state\n\n\
                All changes persist across sessions!"
                    .into(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}
