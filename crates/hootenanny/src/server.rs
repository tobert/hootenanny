use crate::artifact_store::{Artifact, ArtifactStore, FileStore};
use crate::conversation::{ConversationTree, ForkReason};
use crate::domain::{EmotionalVector, Event, AbstractEvent, IntentionEvent, ConcreteEvent};
use crate::persistence::conversation_store::ConversationStore;
use crate::mcp_tools::local_models::{
    LocalModels, OrpheusGenerateParams, Message
};
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, Content, ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router, ErrorData as McpError, ServerHandler,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use base64::{Engine as _, engine::general_purpose};

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

#[derive(Clone)]
pub struct EventDualityServer {
    tool_router: ToolRouter<Self>,
    state: Arc<Mutex<ConversationState>>,
    local_models: Arc<LocalModels>,
    artifact_store: Arc<FileStore>,
}

// Implement Debug manually because LocalModels doesn't implement Debug (client)
impl std::fmt::Debug for EventDualityServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventDualityServer")
            .field("state", &self.state)
            .finish()
    }
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

    // Artifact/variation tracking fields
    #[schemars(description = "Optional variation set ID to group related contributions")]
    pub variation_set_id: Option<String>,

    #[schemars(description = "Optional parent artifact ID for refinements")]
    pub parent_id: Option<String>,

    #[schemars(description = "Optional tags for organizing artifacts (e.g., ['role:melody', 'emotion:joyful'])")]
    #[serde(default)]
    pub tags: Vec<String>,
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

// --- Local Model Requests ---

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OrpheusGenerateRequest {
    #[schemars(description = "Model variant (default: 'base'). Options: 'base', 'children', 'mono_melodies'")]
    pub model: Option<String>,

    #[schemars(description = "Sampling temperature 0.0-2.0 (default: 1.0). Higher = more random")]
    pub temperature: Option<f32>,

    #[schemars(description = "Nucleus sampling 0.0-1.0 (default: 0.95). Lower = more focused")]
    pub top_p: Option<f32>,

    #[schemars(description = "Max tokens to generate (default: 1024). Lower = shorter output")]
    pub max_tokens: Option<u32>,

    #[schemars(description = "Number of variations to generate (default: 1)")]
    pub num_variations: Option<u32>,

    // Artifact/variation tracking fields
    #[schemars(description = "Optional variation set ID to group related generations")]
    pub variation_set_id: Option<String>,

    #[schemars(description = "Optional parent artifact ID for refinements")]
    pub parent_id: Option<String>,

    #[schemars(description = "Optional tags for organizing artifacts (e.g., ['phase:initial', 'experiment:upbeat'])")]
    #[serde(default)]
    pub tags: Vec<String>,

    #[schemars(description = "Creator identifier (agent or user ID)")]
    #[serde(default = "default_creator")]
    pub creator: Option<String>,
}

fn default_creator() -> Option<String> {
    Some("unknown".to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OrpheusGenerateSeededRequest {
    #[schemars(description = "CAS hash of seed MIDI (required)")]
    pub seed_hash: String,

    #[schemars(description = "Model variant (default: 'base'). Options: 'base', 'children', 'mono_melodies'")]
    pub model: Option<String>,

    #[schemars(description = "Sampling temperature 0.0-2.0 (default: 1.0). Higher = more random")]
    pub temperature: Option<f32>,

    #[schemars(description = "Nucleus sampling 0.0-1.0 (default: 0.95). Lower = more focused")]
    pub top_p: Option<f32>,

    #[schemars(description = "Max tokens to generate (default: 1024). Lower = shorter output")]
    pub max_tokens: Option<u32>,

    #[schemars(description = "Number of variations to generate (default: 1)")]
    pub num_variations: Option<u32>,

    // Artifact/variation tracking fields
    #[schemars(description = "Optional variation set ID to group related generations")]
    pub variation_set_id: Option<String>,

    #[schemars(description = "Optional parent artifact ID for refinements")]
    pub parent_id: Option<String>,

    #[schemars(description = "Optional tags for organizing artifacts")]
    #[serde(default)]
    pub tags: Vec<String>,

    #[schemars(description = "Creator identifier (agent or user ID)")]
    #[serde(default = "default_creator")]
    pub creator: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OrpheusContinueRequest {
    #[schemars(description = "CAS hash of MIDI to continue (required)")]
    pub input_hash: String,

    #[schemars(description = "Model variant (default: 'base'). Options: 'base', 'children', 'mono_melodies'")]
    pub model: Option<String>,

    #[schemars(description = "Sampling temperature 0.0-2.0 (default: 1.0). Higher = more random")]
    pub temperature: Option<f32>,

    #[schemars(description = "Nucleus sampling 0.0-1.0 (default: 0.95). Lower = more focused")]
    pub top_p: Option<f32>,

    #[schemars(description = "Max tokens to generate (default: 1024). Lower = shorter output")]
    pub max_tokens: Option<u32>,

    #[schemars(description = "Number of variations to generate (default: 1)")]
    pub num_variations: Option<u32>,

    // Artifact/variation tracking fields
    #[schemars(description = "Optional variation set ID to group related generations")]
    pub variation_set_id: Option<String>,

    #[schemars(description = "Optional parent artifact ID for refinements")]
    pub parent_id: Option<String>,

    #[schemars(description = "Optional tags for organizing artifacts")]
    #[serde(default)]
    pub tags: Vec<String>,

    #[schemars(description = "Creator identifier (agent or user ID)")]
    #[serde(default = "default_creator")]
    pub creator: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OrpheusBridgeRequest {
    #[schemars(description = "CAS hash of first section MIDI (required)")]
    pub section_a_hash: String,

    #[schemars(description = "CAS hash of second section (optional, for future use)")]
    pub section_b_hash: Option<String>,

    #[schemars(description = "Model variant (default: 'bridge'). Recommended: 'bridge'")]
    pub model: Option<String>,

    #[schemars(description = "Sampling temperature 0.0-2.0 (default: 1.0). Higher = more random")]
    pub temperature: Option<f32>,

    #[schemars(description = "Nucleus sampling 0.0-1.0 (default: 0.95). Lower = more focused")]
    pub top_p: Option<f32>,

    #[schemars(description = "Max tokens to generate (default: 1024). Lower = shorter output")]
    pub max_tokens: Option<u32>,

    // Artifact/variation tracking fields
    #[schemars(description = "Optional variation set ID to group related generations")]
    pub variation_set_id: Option<String>,

    #[schemars(description = "Optional parent artifact ID for refinements")]
    pub parent_id: Option<String>,

    #[schemars(description = "Optional tags for organizing artifacts")]
    #[serde(default)]
    pub tags: Vec<String>,

    #[schemars(description = "Creator identifier (agent or user ID)")]
    #[serde(default = "default_creator")]
    pub creator: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OrpheusLoopsRequest {
    #[schemars(description = "CAS hash of seed MIDI (optional)")]
    pub seed_hash: Option<String>,

    #[schemars(description = "Model variant (default: 'loops'). Recommended: 'loops'")]
    pub model: Option<String>,

    #[schemars(description = "Sampling temperature 0.0-2.0 (default: 1.0). Higher = more random")]
    pub temperature: Option<f32>,

    #[schemars(description = "Nucleus sampling 0.0-1.0 (default: 0.95). Lower = more focused")]
    pub top_p: Option<f32>,

    #[schemars(description = "Max tokens to generate (default: 1024). Lower = shorter output")]
    pub max_tokens: Option<u32>,

    #[schemars(description = "Number of variations to generate (default: 1)")]
    pub num_variations: Option<u32>,

    // Artifact/variation tracking fields
    #[schemars(description = "Optional variation set ID to group related generations")]
    pub variation_set_id: Option<String>,

    #[schemars(description = "Optional parent artifact ID for refinements")]
    pub parent_id: Option<String>,

    #[schemars(description = "Optional tags for organizing artifacts")]
    #[serde(default)]
    pub tags: Vec<String>,

    #[schemars(description = "Creator identifier (agent or user ID)")]
    #[serde(default = "default_creator")]
    pub creator: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OrpheusClassifyRequest {
    #[schemars(description = "Model to use (default: 'classifier')")]
    pub model: Option<String>,

    #[schemars(description = "CAS hash of MIDI file to classify")]
    pub input_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DeepSeekQueryRequest {
    #[schemars(description = "Model name (default: 'deepseek-coder-v2-lite')")]
    pub model: Option<String>,

    #[schemars(description = "Chat messages: [{role: 'user', content: '...'}]")]
    pub messages: Vec<Message>,

    #[schemars(description = "Stream response (default: false, not implemented)")]
    pub stream: Option<bool>,

    // Artifact/variation tracking fields
    #[schemars(description = "Optional variation set ID to group related generations")]
    pub variation_set_id: Option<String>,

    #[schemars(description = "Optional parent artifact ID for refinements")]
    pub parent_id: Option<String>,

    #[schemars(description = "Optional tags for organizing artifacts (e.g., ['language:rust', 'task:debug'])")]
    #[serde(default)]
    pub tags: Vec<String>,

    #[schemars(description = "Creator identifier (agent or user ID)")]
    #[serde(default = "default_creator")]
    pub creator: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CasStoreRequest {
    pub content_base64: String,
    pub mime_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CasInspectRequest {
    pub hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UploadFileRequest {
    #[schemars(description = "Absolute path to file to upload")]
    pub file_path: String,

    #[schemars(description = "MIME type of the file (e.g., 'audio/soundfont', 'audio/midi')")]
    pub mime_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MidiToWavRequest {
    #[schemars(description = "CAS hash of MIDI file to render (required)")]
    pub input_hash: String,

    #[schemars(description = "CAS hash of SoundFont file (required)")]
    pub soundfont_hash: String,

    #[schemars(description = "Sample rate (default: 44100)")]
    pub sample_rate: Option<u32>,

    // Artifact tracking fields
    #[schemars(description = "Optional variation set ID to group related conversions")]
    pub variation_set_id: Option<String>,

    #[schemars(description = "Optional parent artifact ID for refinements")]
    pub parent_id: Option<String>,

    #[schemars(description = "Optional tags for organizing artifacts")]
    #[serde(default)]
    pub tags: Vec<String>,

    #[schemars(description = "Creator identifier (agent or user ID)")]
    #[serde(default = "default_creator")]
    pub creator: Option<String>,
}

#[tool_router]
impl EventDualityServer {
    pub fn new_with_state(
        state: Arc<Mutex<ConversationState>>,
        local_models: Arc<LocalModels>,
        artifact_store: Arc<FileStore>,
    ) -> Self {
        Self {
            tool_router: Self::tool_router(),
            state,
            local_models,
            artifact_store,
        }
    }

    pub fn new() -> Self {
        // Default stateless mode for backward compatibility
        let state_dir = std::env::temp_dir().join("hrmcp_default");
        std::fs::create_dir_all(&state_dir).expect("Failed to create temp dir");

        let state = ConversationState::new(state_dir.clone()).expect("Failed to create conversation state");

        // Dummy local models for default constructor (should be replaced in real use)
        let cas = crate::cas::Cas::new(&std::env::temp_dir().join("hrmcp_cas")).expect("Failed to create CAS");
        let local_models = Arc::new(LocalModels::new(cas, 2000, 2001));

        // Artifact store
        let artifact_store = Arc::new(
            FileStore::new(state_dir.join("artifacts.json")).expect("Failed to create artifact store")
        );

        Self::new_with_state(Arc::new(Mutex::new(state)), local_models, artifact_store)
    }

    #[tool(description = "Merge two branches in the conversation tree")]
    #[tracing::instrument(name = "mcp.tool.merge_branches", skip(self, _request))]
    fn merge_branches(
        &self,
        Parameters(_request): Parameters<MergeRequest>,
    ) -> Result<CallToolResult, McpError> {
        Ok(CallToolResult::success(vec![Content::text(
            "not implemented".to_string(),
        )]))
    }

    #[tool(description = "Prune a branch from the conversation tree")]
    #[tracing::instrument(name = "mcp.tool.prune_branch", skip(self, _request))]
    fn prune_branch(
        &self,
        Parameters(_request): Parameters<PruneRequest>,
    ) -> Result<CallToolResult, McpError> {
        Ok(CallToolResult::success(vec![Content::text(
            "not implemented".to_string(),
        )]))
    }

    #[tool(description = "Evaluate a branch")]
    #[tracing::instrument(name = "mcp.tool.evaluate_branch", skip(self, _request))]
    fn evaluate_branch(
        &self,
        Parameters(_request): Parameters<EvaluateRequest>,
    ) -> Result<CallToolResult, McpError> {
        Ok(CallToolResult::success(vec![Content::text(
            "not implemented".to_string(),
        )]))
    }

    #[tool(description = "Get the musical context at a specific time")]
    #[tracing::instrument(name = "mcp.tool.get_context", skip(self, _request))]
    fn get_context(
        &self,
        Parameters(_request): Parameters<GetContextRequest>,
    ) -> Result<CallToolResult, McpError> {
        Ok(CallToolResult::success(vec![Content::text(
            "not implemented".to_string(),
        )]))
    }

    #[tool(description = "Subscribe to a stream of events")]
    #[tracing::instrument(name = "mcp.tool.subscribe_events", skip(self))]
    fn subscribe_events(&self) -> Result<CallToolResult, McpError> {
        Ok(CallToolResult::success(vec![Content::text(
            "not implemented".to_string(),
        )]))
    }

    #[tool(description = "Broadcast a message to all agents in the conversation")]
    #[tracing::instrument(name = "mcp.tool.broadcast_message", skip(self, _request))]
    fn broadcast_message(
        &self,
        Parameters(_request): Parameters<BroadcastMessageRequest>,
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
    async fn play(
        &self,
        Parameters(request): Parameters<AddNodeRequest>,
    ) -> Result<CallToolResult, McpError> {
        // Reuse the flattened AddNodeRequest structure for simplicity
        let intention = AbstractEvent::Intention(IntentionEvent {
            what: request.what.clone(),
            how: request.how.clone(),
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

        let result_value = serde_json::to_value(&sound)
            .map_err(|e| McpError::internal_error(format!("Failed to serialize sound: {}", e), None))?;

        // Store the sound event in CAS
        let sound_json = serde_json::to_string_pretty(&sound)
            .map_err(|e| McpError::internal_error(format!("Failed to serialize sound for CAS: {}", e), None))?;

        // Use general_purpose encoding
        let sound_hash = self.local_models.store_cas_content(
            sound_json.as_bytes(),
            "application/json"
        ).await.map_err(|e| McpError::internal_error(e.to_string(), None))?;

        // Create artifact with musical metadata
        let artifact_id = format!("artifact_{}", &sound_hash[..12]);
        let mut artifact = Artifact::new(
            &artifact_id,
            &request.agent_id,
            serde_json::json!({
                "hash": sound_hash,
                "intention": {
                    "what": request.what,
                    "how": request.how,
                },
                "emotion": {
                    "valence": request.valence,
                    "arousal": request.arousal,
                    "agency": request.agency,
                },
                "description": request.description,
            })
        )
        .with_tags(vec![
            "type:musical_event",
            "phase:realization",
            "tool:play"
        ]);

        // Add variation set info if provided
        if let Some(set_id) = request.variation_set_id {
            let index = self.artifact_store.next_variation_index(&set_id)
                .map_err(|e| McpError::internal_error(e.to_string(), None))?;
            artifact = artifact.with_variation_set(&set_id, index);
        }

        // Add parent if provided
        if let Some(parent_id) = request.parent_id {
            artifact = artifact.with_parent(&parent_id);
        }

        // Add custom tags
        artifact = artifact.with_tags(request.tags);

        // Store artifact
        self.artifact_store.put(artifact.clone())
            .map_err(|e| McpError::internal_error(format!("Failed to store artifact: {}", e), None))?;
        self.artifact_store.flush()
            .map_err(|e| McpError::internal_error(format!("Failed to flush artifact store: {}", e), None))?;

        // Include artifact_id in response
        let response = serde_json::json!({
            "sound": result_value,
            "artifact_id": artifact.id,
            "cas_hash": sound_hash,
        });

        Ok(CallToolResult::success(vec![Content::text(
            response.to_string(),
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
    async fn add_node(
        &self,
        Parameters(request): Parameters<AddNodeRequest>,
    ) -> Result<CallToolResult, McpError> {
        // Scope the mutex to ensure it's dropped before async operations
        let (node_id, branch_id, total_nodes, intention) = {
            let mut state = self.state.lock().unwrap();

            let branch_id = request.branch_id.clone().unwrap_or_else(|| state.current_branch.clone());

            let nodes_before = state.tree.nodes.len();

            // Record branch resolution
            let span = tracing::Span::current();
            span.record("conversation.branch_id", &*branch_id);
            span.record("tree.nodes_before", nodes_before);

            // Construct AbstractEvent from flattened parameters
            let intention = AbstractEvent::Intention(IntentionEvent {
                what: request.what.clone(),
                how: request.how.clone(),
                emotion: EmotionalVector {
                    valence: request.valence,
                    arousal: request.arousal,
                    agency: request.agency,
                },
            });

            let event = Event::Abstract(intention.clone());

            let node_id = state
                .tree
                .add_node(
                    &branch_id,
                    event,
                    request.agent_id.clone(),
                    EmotionalVector::neutral(), // Use intention's emotion
                    request.description.clone(),
                )
                .map_err(|e| McpError::parse_error(e, None))?;

            // Record node creation
            span.record("conversation.node_id", node_id);
            span.record("tree.nodes_after", state.tree.nodes.len());

            // Save to persistence
            state.save().map_err(|e| McpError::parse_error(e.to_string(), None))?;

            let total_nodes = state.tree.nodes.len();

            // MutexGuard dropped here at end of scope
            (node_id, branch_id, total_nodes, intention)
        };

        // Store the intention in CAS
        let intention_json = serde_json::to_string_pretty(&intention)
            .map_err(|e| McpError::internal_error(format!("Failed to serialize intention: {}", e), None))?;

        let intention_hash = self.local_models.store_cas_content(
            intention_json.as_bytes(),
            "application/json"
        ).await.map_err(|e| McpError::internal_error(e.to_string(), None))?;

        // Create artifact with conversation context
        let artifact_id = format!("artifact_{}", &intention_hash[..12]);
        let mut artifact = Artifact::new(
            &artifact_id,
            &request.agent_id,
            serde_json::json!({
                "hash": intention_hash,
                "node_id": node_id,
                "branch_id": branch_id,
                "intention": {
                    "what": request.what,
                    "how": request.how,
                },
                "emotion": {
                    "valence": request.valence,
                    "arousal": request.arousal,
                    "agency": request.agency,
                },
                "description": request.description,
            })
        )
        .with_tags(vec![
            "type:intention",
            "phase:contribution",
            "tool:add_node"
        ]);

        // Add variation set info if provided
        if let Some(set_id) = request.variation_set_id {
            let index = self.artifact_store.next_variation_index(&set_id)
                .map_err(|e| McpError::internal_error(e.to_string(), None))?;
            artifact = artifact.with_variation_set(&set_id, index);
        }

        // Add parent if provided
        if let Some(parent_id) = request.parent_id {
            artifact = artifact.with_parent(&parent_id);
        }

        // Add custom tags
        artifact = artifact.with_tags(request.tags);

        // Store artifact
        self.artifact_store.put(artifact.clone())
            .map_err(|e| McpError::internal_error(format!("Failed to store artifact: {}", e), None))?;
        self.artifact_store.flush()
            .map_err(|e| McpError::internal_error(format!("Failed to flush artifact store: {}", e), None))?;

        let result = serde_json::json!({
            "node_id": node_id,
            "branch_id": branch_id,
            "total_nodes": total_nodes,
            "artifact_id": artifact.id,
            "cas_hash": intention_hash,
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

    // --- CAS Tools ---

    #[tool(description = "Store content in Content Addressable Storage (CAS).

Example: {content_base64: 'SGVsbG8=', mime_type: 'text/plain'}

Returns: BLAKE3 hash string that can be used to retrieve the content.
Use cas_inspect to get the local file path.")]
    #[tracing::instrument(
        name = "mcp.tool.cas_store",
        skip(self, request),
        fields(
            cas.mime_type = %request.mime_type,
            cas.content_size = request.content_base64.len(),
            cas.hash = tracing::field::Empty,
        )
    )]
    async fn cas_store(
        &self,
        Parameters(request): Parameters<CasStoreRequest>,
    ) -> Result<CallToolResult, McpError> {
        let decoded_content = general_purpose::STANDARD.decode(&request.content_base64)
            .map_err(|e| McpError::parse_error(format!("Failed to base64 decode content: {}", e), None))?;

        let hash = self.local_models.store_cas_content(&decoded_content, &request.mime_type)
            .await
            .map_err(|e| McpError::internal_error(format!("Failed to store content in CAS: {}", e), None))?;

        tracing::Span::current().record("cas.hash", &hash);

        Ok(CallToolResult::success(vec![Content::text(hash)]))
    }

    #[tool(description = "Inspect content in CAS and get its metadata.

Example: {hash: '5ca7815...'}

Returns: {hash, mime_type, size, local_path}
The local_path can be used to access the file directly on disk.")]
    #[tracing::instrument(
        name = "mcp.tool.cas_inspect",
        skip(self, request),
        fields(
            cas.hash = %request.hash,
            cas.mime_type = tracing::field::Empty,
            cas.size_bytes = tracing::field::Empty,
        )
    )]
    async fn cas_inspect(
        &self,
        Parameters(request): Parameters<CasInspectRequest>,
    ) -> Result<CallToolResult, McpError> {
        let cas_ref = self.local_models.inspect_cas_content(&request.hash)
            .await
            .map_err(|e| McpError::internal_error(format!("Failed to inspect CAS: {}", e), None))?;

        let span = tracing::Span::current();
        span.record("cas.mime_type", &*cas_ref.mime_type);
        span.record("cas.size_bytes", cas_ref.size_bytes);

        let result = serde_json::json!({
            "hash": cas_ref.hash,
            "mime_type": cas_ref.mime_type,
            "size": cas_ref.size_bytes,
            "local_path": cas_ref.local_path,
        });

        let json = serde_json::to_string(&result)
            .map_err(|e| McpError::internal_error(format!("Failed to serialize CAS reference: {}", e), None))?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(description = "Upload a file from disk to Content Addressable Storage (CAS).

Example: {file_path: '/path/to/soundfont.sf2', mime_type: 'audio/soundfont'}

Returns: BLAKE3 hash string that can be used to retrieve the content.")]
    #[tracing::instrument(
        name = "mcp.tool.upload_file",
        skip(self, request),
        fields(
            file.path = %request.file_path,
            file.mime_type = %request.mime_type,
            file.size = tracing::field::Empty,
            cas.hash = tracing::field::Empty,
        )
    )]
    async fn upload_file(
        &self,
        Parameters(request): Parameters<UploadFileRequest>,
    ) -> Result<CallToolResult, McpError> {
        // Read file from disk
        let file_bytes = tokio::fs::read(&request.file_path)
            .await
            .map_err(|e| McpError::internal_error(format!("Failed to read file: {}", e), None))?;

        let span = tracing::Span::current();
        span.record("file.size", file_bytes.len());

        // Store in CAS
        let hash = self.local_models.store_cas_content(&file_bytes, &request.mime_type)
            .await
            .map_err(|e| McpError::internal_error(format!("Failed to store file in CAS: {}", e), None))?;

        span.record("cas.hash", &*hash);

        let result = serde_json::json!({
            "hash": hash,
            "size_bytes": file_bytes.len(),
            "mime_type": request.mime_type,
        });

        let json = serde_json::to_string(&result)
            .map_err(|e| McpError::internal_error(format!("Failed to serialize response: {}", e), None))?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    // --- Local Model Tools ---

    // Helper function to validate sampling parameters
    fn validate_sampling_params(temperature: Option<f32>, top_p: Option<f32>) -> Result<(), McpError> {
        if let Some(temp) = temperature {
            if temp < 0.0 || temp > 2.0 {
                return Err(McpError::invalid_params(
                    format!("temperature must be 0.0-2.0, got {}", temp),
                    None
                ));
            }
        }
        if let Some(p) = top_p {
            if p < 0.0 || p > 1.0 {
                return Err(McpError::invalid_params(
                    format!("top_p must be 0.0-1.0, got {}", p),
                    None
                ));
            }
        }
        Ok(())
    }

    // Helper function to create and store artifact
    fn create_artifact(
        &self,
        output_hash: &str,
        task: &str,
        model: &str,
        temperature: Option<f32>,
        tokens: Option<u32>,
        variation_set_id: Option<String>,
        parent_id: Option<String>,
        tags: Vec<String>,
        creator: Option<String>,
    ) -> Result<Artifact, McpError> {
        let artifact_id = format!("artifact_{}", &output_hash[..12]);
        let creator = creator.unwrap_or_else(|| "agent_orpheus".to_string());

        // Determine artifact type and tool name based on task
        let (artifact_type, tool_name) = if task == "query" {
            ("type:text", format!("tool:deepseek_{}", task))
        } else if task == "midi_to_wav" {
            ("type:audio", "tool:midi_to_wav".to_string())
        } else {
            ("type:midi", format!("tool:orpheus_{}", task))
        };

        let mut artifact = Artifact::new(
            &artifact_id,
            &creator,
            serde_json::json!({
                "hash": output_hash,
                "tokens": tokens,
                "model": model,
                "temperature": temperature,
                "task": task,
            })
        )
        .with_tags(vec![
            artifact_type,
            "phase:generation",
            &tool_name
        ]);

        // Add variation set info if provided
        if let Some(set_id) = variation_set_id {
            let index = self.artifact_store.next_variation_index(&set_id)
                .map_err(|e| McpError::internal_error(e.to_string(), None))?;
            artifact = artifact.with_variation_set(&set_id, index);
        }

        // Add parent if provided
        if let Some(parent_id) = parent_id {
            artifact = artifact.with_parent(&parent_id);
        }

        // Add custom tags
        artifact = artifact.with_tags(tags);

        // Store artifact
        self.artifact_store.put(artifact.clone())
            .map_err(|e| McpError::internal_error(format!("Failed to store artifact: {}", e), None))?;

        // Flush to disk
        self.artifact_store.flush()
            .map_err(|e| McpError::internal_error(format!("Failed to flush artifact store: {}", e), None))?;

        Ok(artifact)
    }

    #[tool(description = "Generate music from scratch using Orpheus.

Example: {temperature: 1.2, max_tokens: 512, num_variations: 1}

Returns: {status: 'success', output_hash: '<cas-hash>', summary: 'Generated N tokens'}
The output_hash can be used with cas_inspect to get the MIDI file path.")]
    #[tracing::instrument(
        name = "mcp.tool.orpheus_generate",
        skip(self, request),
        fields(
            model.name = ?request.model,
            model.temperature = request.temperature,
            model.num_variations = request.num_variations,
            model.output_hash = tracing::field::Empty,
        )
    )]
    async fn orpheus_generate(
        &self,
        Parameters(request): Parameters<OrpheusGenerateRequest>,
    ) -> Result<CallToolResult, McpError> {
        // Validate parameters
        Self::validate_sampling_params(request.temperature, request.top_p)?;

        let params = OrpheusGenerateParams {
            temperature: request.temperature,
            top_p: request.top_p,
            max_tokens: request.max_tokens,
            num_variations: request.num_variations,
        };

        let model = request.model.clone().unwrap_or_else(|| "base".to_string());

        let result = self.local_models.run_orpheus_generate(
            model.clone(),
            "generate".to_string(),
            None,  // No input for from-scratch generation
            params
        ).await.map_err(|e| McpError::internal_error(e.to_string(), None))?;

        tracing::Span::current().record("model.output_hash", &*result.output_hash);

        // Create artifact automatically
        let tokens = result.summary.split_whitespace()
            .find(|s| s.parse::<u32>().is_ok())
            .and_then(|s| s.parse::<u32>().ok());

        let artifact = self.create_artifact(
            &result.output_hash,
            "generate",
            &model,
            request.temperature,
            tokens,
            request.variation_set_id,
            request.parent_id,
            request.tags,
            request.creator,
        )?;

        // Enhanced response with artifact info
        let response = serde_json::json!({
            "status": result.status,
            "output_hash": result.output_hash,
            "summary": result.summary,
            "artifact_id": artifact.id,
            "variation_set_id": artifact.variation_set_id,
            "variation_index": artifact.variation_index,
        });

        let json = serde_json::to_string(&response)
            .map_err(|e| McpError::internal_error(format!("Failed to serialize result: {}", e), None))?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(description = "Generate music using a seed MIDI as inspiration.

Example: {seed_hash: '5ca7815abc...', temperature: 0.8}

Returns: {status: 'success', output_hash: '<cas-hash>', summary: 'Generated N tokens'}")]
    #[tracing::instrument(
        name = "mcp.tool.orpheus_generate_seeded",
        skip(self, request),
        fields(
            model.name = ?request.model,
            model.seed_hash = %request.seed_hash,
            model.temperature = request.temperature,
            model.output_hash = tracing::field::Empty,
        )
    )]
    async fn orpheus_generate_seeded(
        &self,
        Parameters(request): Parameters<OrpheusGenerateSeededRequest>,
    ) -> Result<CallToolResult, McpError> {
        Self::validate_sampling_params(request.temperature, request.top_p)?;

        let params = OrpheusGenerateParams {
            temperature: request.temperature,
            top_p: request.top_p,
            max_tokens: request.max_tokens,
            num_variations: request.num_variations,
        };

        let model = request.model.clone().unwrap_or_else(|| "base".to_string());

        let result = self.local_models.run_orpheus_generate(
            model.clone(),
            "generate".to_string(),
            Some(request.seed_hash),  // Seed MIDI as input
            params
        ).await.map_err(|e| McpError::internal_error(e.to_string(), None))?;

        tracing::Span::current().record("model.output_hash", &*result.output_hash);

        // Create artifact automatically
        let tokens = result.summary.split_whitespace()
            .find(|s| s.parse::<u32>().is_ok())
            .and_then(|s| s.parse::<u32>().ok());

        let artifact = self.create_artifact(
            &result.output_hash,
            "generate_seeded",
            &model,
            request.temperature,
            tokens,
            request.variation_set_id,
            request.parent_id,
            request.tags,
            request.creator,
        )?;

        // Enhanced response with artifact info
        let response = serde_json::json!({
            "status": result.status,
            "output_hash": result.output_hash,
            "summary": result.summary,
            "artifact_id": artifact.id,
            "variation_set_id": artifact.variation_set_id,
            "variation_index": artifact.variation_index,
        });

        let json = serde_json::to_string(&response)
            .map_err(|e| McpError::internal_error(format!("Failed to serialize: {}", e), None))?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(description = "Continue an existing MIDI sequence.

Example: {input_hash: '5ca7815abc...', max_tokens: 256}

Returns: {status: 'success', output_hash: '<cas-hash>', summary: 'Generated N tokens'}")]
    #[tracing::instrument(
        name = "mcp.tool.orpheus_continue",
        skip(self, request),
        fields(
            model.name = ?request.model,
            model.input_hash = %request.input_hash,
            model.temperature = request.temperature,
            model.output_hash = tracing::field::Empty,
        )
    )]
    async fn orpheus_continue(
        &self,
        Parameters(request): Parameters<OrpheusContinueRequest>,
    ) -> Result<CallToolResult, McpError> {
        Self::validate_sampling_params(request.temperature, request.top_p)?;

        let params = OrpheusGenerateParams {
            temperature: request.temperature,
            top_p: request.top_p,
            max_tokens: request.max_tokens,
            num_variations: request.num_variations,
        };

        let model = request.model.clone().unwrap_or_else(|| "base".to_string());

        let result = self.local_models.run_orpheus_generate(
            model.clone(),
            "continue".to_string(),  // Task is "continue"
            Some(request.input_hash),
            params
        ).await.map_err(|e| McpError::internal_error(e.to_string(), None))?;

        tracing::Span::current().record("model.output_hash", &*result.output_hash);

        // Create artifact automatically
        let tokens = result.summary.split_whitespace()
            .find(|s| s.parse::<u32>().is_ok())
            .and_then(|s| s.parse::<u32>().ok());

        let artifact = self.create_artifact(
            &result.output_hash,
            "continue",
            &model,
            request.temperature,
            tokens,
            request.variation_set_id,
            request.parent_id,
            request.tags,
            request.creator,
        )?;

        // Enhanced response with artifact info
        let response = serde_json::json!({
            "status": result.status,
            "output_hash": result.output_hash,
            "summary": result.summary,
            "artifact_id": artifact.id,
            "variation_set_id": artifact.variation_set_id,
            "variation_index": artifact.variation_index,
        });

        let json = serde_json::to_string(&response)
            .map_err(|e| McpError::internal_error(format!("Failed to serialize: {}", e), None))?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(description = "Generate a musical bridge connecting sections.

Example: {section_a_hash: '5ca7815abc...', model: 'bridge'}

Returns: {status: 'success', output_hash: '<cas-hash>', summary: 'Generated N tokens'}")]
    #[tracing::instrument(
        name = "mcp.tool.orpheus_bridge",
        skip(self, request),
        fields(
            model.name = ?request.model,
            model.section_a_hash = %request.section_a_hash,
            model.section_b_hash = ?request.section_b_hash,
            model.temperature = request.temperature,
            model.output_hash = tracing::field::Empty,
        )
    )]
    async fn orpheus_bridge(
        &self,
        Parameters(request): Parameters<OrpheusBridgeRequest>,
    ) -> Result<CallToolResult, McpError> {
        Self::validate_sampling_params(request.temperature, request.top_p)?;

        let params = OrpheusGenerateParams {
            temperature: request.temperature,
            top_p: request.top_p,
            max_tokens: request.max_tokens,
            num_variations: None,  // Bridge doesn't use variations
        };

        let model = request.model.clone().unwrap_or_else(|| "bridge".to_string());

        // Note: API currently uses section_a as midi_input, section_b for future use
        let result = self.local_models.run_orpheus_generate(
            model.clone(),
            "bridge".to_string(),
            Some(request.section_a_hash),
            params
        ).await.map_err(|e| McpError::internal_error(e.to_string(), None))?;

        tracing::Span::current().record("model.output_hash", &*result.output_hash);

        // Create artifact automatically
        let tokens = result.summary.split_whitespace()
            .find(|s| s.parse::<u32>().is_ok())
            .and_then(|s| s.parse::<u32>().ok());

        let artifact = self.create_artifact(
            &result.output_hash,
            "bridge",
            &model,
            request.temperature,
            tokens,
            request.variation_set_id,
            request.parent_id,
            request.tags,
            request.creator,
        )?;

        // Enhanced response with artifact info
        let response = serde_json::json!({
            "status": result.status,
            "output_hash": result.output_hash,
            "summary": result.summary,
            "artifact_id": artifact.id,
            "variation_set_id": artifact.variation_set_id,
            "variation_index": artifact.variation_index,
        });

        let json = serde_json::to_string(&response)
            .map_err(|e| McpError::internal_error(format!("Failed to serialize: {}", e), None))?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(description = "Generate multi-instrumental loops.

Example: {model: 'loops', num_variations: 3}

Returns: {status: 'success', output_hash: '<cas-hash>', summary: 'Generated N tokens'}")]
    #[tracing::instrument(
        name = "mcp.tool.orpheus_loops",
        skip(self, request),
        fields(
            model.name = ?request.model,
            model.seed_hash = ?request.seed_hash,
            model.temperature = request.temperature,
            model.num_variations = request.num_variations,
            model.output_hash = tracing::field::Empty,
        )
    )]
    async fn orpheus_loops(
        &self,
        Parameters(request): Parameters<OrpheusLoopsRequest>,
    ) -> Result<CallToolResult, McpError> {
        Self::validate_sampling_params(request.temperature, request.top_p)?;

        let params = OrpheusGenerateParams {
            temperature: request.temperature,
            top_p: request.top_p,
            max_tokens: request.max_tokens,
            num_variations: request.num_variations,
        };

        let model = request.model.clone().unwrap_or_else(|| "loops".to_string());

        let result = self.local_models.run_orpheus_generate(
            model.clone(),
            "loops".to_string(),
            request.seed_hash,  // Optional seed
            params
        ).await.map_err(|e| McpError::internal_error(e.to_string(), None))?;

        tracing::Span::current().record("model.output_hash", &*result.output_hash);

        // Create artifact automatically
        let tokens = result.summary.split_whitespace()
            .find(|s| s.parse::<u32>().is_ok())
            .and_then(|s| s.parse::<u32>().ok());

        let artifact = self.create_artifact(
            &result.output_hash,
            "loops",
            &model,
            request.temperature,
            tokens,
            request.variation_set_id,
            request.parent_id,
            request.tags,
            request.creator,
        )?;

        // Enhanced response with artifact info
        let response = serde_json::json!({
            "status": result.status,
            "output_hash": result.output_hash,
            "summary": result.summary,
            "artifact_id": artifact.id,
            "variation_set_id": artifact.variation_set_id,
            "variation_index": artifact.variation_index,
        });

        let json = serde_json::to_string(&response)
            .map_err(|e| McpError::internal_error(format!("Failed to serialize: {}", e), None))?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(description = "Classify MIDI as human or AI-composed using the local Orpheus model.

Example: {input_hash: '<cas-hash-of-midi>'}

Returns: {is_human: true/false, confidence: 0.0-1.0, probabilities: {...}}")]
    #[tracing::instrument(
        name = "mcp.tool.orpheus_classify",
        skip(self, request),
        fields(
            model.name = ?request.model,
            model.input_hash = %request.input_hash,
        )
    )]
    async fn orpheus_classify(
        &self,
        Parameters(request): Parameters<OrpheusClassifyRequest>,
    ) -> Result<CallToolResult, McpError> {
        let result = self.local_models.run_orpheus_classify(
            request.model,
            request.input_hash,
        ).await.map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let json = serde_json::to_string(&result)
            .map_err(|e| McpError::internal_error(format!("Failed to serialize Orpheus classification: {}", e), None))?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(description = "Query the local DeepSeek Coder model for code generation and questions.

Example: {messages: [{role: 'user', content: 'Write a hello world in Rust'}]}

Returns: {text: '...response...', finish_reason: 'stop'}")]
    #[tracing::instrument(
        name = "mcp.tool.deepseek_query",
        skip(self, request),
        fields(
            model.name = ?request.model,
            model.message_count = request.messages.len(),
            model.stream = ?request.stream,
            model.response_length = tracing::field::Empty,
        )
    )]
    async fn deepseek_query(
        &self,
        Parameters(request): Parameters<DeepSeekQueryRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model = request.model.clone().unwrap_or_else(|| "deepseek-coder-v2-lite".to_string());

        let result = self.local_models.run_deepseek_query(
            request.model,
            request.messages,
            request.stream,
        ).await.map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let response_len = result.text.len();
        tracing::Span::current().record("model.response_length", response_len);

        // Warn if response is very large (>100KB)
        if response_len > 100_000 {
            tracing::warn!("DeepSeek response is large: {} bytes", response_len);
        }

        // Store generated text in CAS
        let text_hash = self.local_models.store_cas_content(
            result.text.as_bytes(),
            "text/plain"
        ).await.map_err(|e| McpError::internal_error(e.to_string(), None))?;

        // Create artifact
        let artifact = self.create_artifact(
            &text_hash,
            "query",
            &model,
            None, // No temperature for DeepSeek
            Some(response_len as u32),
            request.variation_set_id,
            request.parent_id,
            request.tags,
            request.creator,
        )?;

        // Include artifact_id in response
        let response = serde_json::json!({
            "text": result.text,
            "finish_reason": result.finish_reason,
            "artifact_id": artifact.id,
            "cas_hash": text_hash,
        });

        let json = serde_json::to_string(&response)
            .map_err(|e| McpError::internal_error(format!("Failed to serialize DeepSeek result: {}", e), None))?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(description = "Render MIDI file to WAV using RustySynth.

Example: {input_hash: '5ca7815abc...', soundfont_hash: 'a1b2c3d...', sample_rate: 44100}

Returns: {status: 'success', output_hash: '<cas-hash>', summary: 'Rendered N seconds'}
The output_hash can be used with cas_inspect to get the WAV file path.")]
    #[tracing::instrument(
        name = "mcp.tool.midi_to_wav",
        skip(self, request),
        fields(
            midi.input_hash = %request.input_hash,
            soundfont.hash = %request.soundfont_hash,
            audio.sample_rate = request.sample_rate.unwrap_or(44100),
            audio.output_hash = tracing::field::Empty,
            audio.duration_seconds = tracing::field::Empty,
        )
    )]
    async fn midi_to_wav(
        &self,
        Parameters(request): Parameters<MidiToWavRequest>,
    ) -> Result<CallToolResult, McpError> {
        // Get MIDI file from CAS
        let midi_bytes = self.local_models.read_cas_content(&request.input_hash)
            .await
            .map_err(|e| McpError::internal_error(format!("Failed to read MIDI from CAS: {}", e), None))?;

        // Get SoundFont from CAS
        let soundfont_bytes = self.local_models.read_cas_content(&request.soundfont_hash)
            .await
            .map_err(|e| McpError::internal_error(format!("Failed to read SoundFont from CAS: {}", e), None))?;

        // Render using RustySynth
        let sample_rate = request.sample_rate.unwrap_or(44100);
        let wav_bytes = crate::mcp_tools::rustysynth::render_midi_to_wav(
            &midi_bytes,
            &soundfont_bytes,
            sample_rate,
        ).map_err(|e| McpError::internal_error(format!("Failed to render MIDI: {}", e), None))?;

        // Calculate duration
        let duration_seconds = crate::mcp_tools::rustysynth::calculate_wav_duration(&wav_bytes, sample_rate);

        // Store WAV in CAS
        let wav_hash = self.local_models.store_cas_content(&wav_bytes, "audio/wav")
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let span = tracing::Span::current();
        span.record("audio.output_hash", &*wav_hash);
        span.record("audio.duration_seconds", duration_seconds);

        // Create artifact
        let artifact = self.create_artifact(
            &wav_hash,
            "midi_to_wav",
            "rustysynth",
            None,  // No temperature for this tool
            Some(wav_bytes.len() as u32),  // Store size instead of tokens
            request.variation_set_id,
            request.parent_id,
            request.tags,
            request.creator,
        )?;

        // Return response
        let response = serde_json::json!({
            "status": "success",
            "output_hash": wav_hash,
            "summary": format!("Rendered {:.2} seconds of audio", duration_seconds),
            "artifact_id": artifact.id,
            "sample_rate": sample_rate,
            "size_bytes": wav_bytes.len(),
            "duration_seconds": duration_seconds,
        });

        let json = serde_json::to_string(&response)
            .map_err(|e| McpError::internal_error(format!("Failed to serialize: {}", e), None))?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }
}

#[tool_handler]
impl ServerHandler for EventDualityServer {
    // The macro generates list_tools and call_tool methods
    // Let's add a custom method to verify the handler is working
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "Event Duality MCP Server - Musical ensemble collaboration".into()
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}
