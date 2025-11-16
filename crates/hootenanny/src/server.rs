use crate::conversation::{ConversationTree, ForkReason};
use crate::domain::{EmotionalVector, Event, Intention};
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
        let mut store = ConversationStore::new(&state_dir)?;

        // Try to load existing tree, or create new one
        let tree = match store.load_tree()? {
            Some(tree) => {
                tracing::info!("✅ Loaded existing conversation tree with {} nodes", tree.nodes.len());
                tree
            }
            None => {
                tracing::info!("🌱 Creating new conversation tree");
                let root_event = Event::Abstract(Intention {
                    what: "root".to_string(),
                    how: "gently".to_string(),
                    emotion: EmotionalVector::neutral(),
                });
                ConversationTree::new(
                    root_event,
                    "system".to_string(),
                    EmotionalVector::neutral(),
                )
            }
        };

        let current_branch = tree.main_branch.clone();

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
/// Flattened for easier MCP usage - construct Intention internally.
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

    #[tool(description = "Transform an intention into sound - the Alchemical transmutation of emotion to music")]
    fn play(
        &self,
        Parameters(request): Parameters<AddNodeRequest>,
    ) -> Result<CallToolResult, McpError> {
        // Reuse the flattened AddNodeRequest structure for simplicity
        let intention = Intention {
            what: request.what,
            how: request.how,
            emotion: EmotionalVector {
                valence: request.valence,
                arousal: request.arousal,
                agency: request.agency,
            },
        };

        let sound = intention.realize();

        let result = serde_json::json!({
            "pitch": sound.pitch,
            "velocity": sound.velocity,
            "duration_ms": sound.duration_ms,
            "emotion": {
                "valence": sound.emotion.valence,
                "arousal": sound.emotion.arousal,
                "agency": sound.emotion.agency,
            }
        });

        Ok(CallToolResult::success(vec![Content::text(
            result.to_string(),
        )]))
    }

    #[tool(description = "Add a musical intention to the conversation tree")]
    fn add_node(
        &self,
        Parameters(request): Parameters<AddNodeRequest>,
    ) -> Result<CallToolResult, McpError> {
        let mut state = self.state.lock().unwrap();

        let branch_id = request.branch_id.unwrap_or_else(|| state.current_branch.clone());

        // Construct Intention from flattened parameters
        let intention = Intention {
            what: request.what,
            how: request.how,
            emotion: EmotionalVector {
                valence: request.valence,
                arousal: request.arousal,
                agency: request.agency,
            },
        };

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
    fn get_tree_status(&self) -> Result<CallToolResult, McpError> {
        let state = self.state.lock().unwrap();

        let result = serde_json::json!({
            "total_nodes": state.tree.nodes.len(),
            "total_branches": state.tree.branches.len(),
            "current_branch": state.current_branch,
            "main_branch": state.tree.main_branch,
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
