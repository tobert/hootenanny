use crate::artifact_store::FileStore;
use crate::conversation::ConversationTree;
use crate::domain::{AbstractEvent, EmotionalVector, Event, IntentionEvent};
use crate::job_system::JobStore;
use crate::mcp_tools::local_models::LocalModels;
use crate::persistence::conversation_store::ConversationStore;
use audio_graph_mcp::{AudioGraphAdapter, Database as AudioGraphDb};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Shared state for the conversation tree and persistence.
#[derive(Debug)]
pub struct ConversationState {
    pub tree: ConversationTree,
    pub store: ConversationStore,
    pub current_branch: String,
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
    // tool_router: ToolRouter<Self>, // No longer needed for manual dispatch
    pub state: Arc<Mutex<ConversationState>>,
    pub local_models: Arc<LocalModels>,
    pub artifact_store: Arc<FileStore>,
    pub job_store: JobStore,
    pub audio_graph_adapter: Arc<AudioGraphAdapter>,
    pub audio_graph_db: Arc<AudioGraphDb>,
}

// Implement Debug manually because LocalModels doesn't implement Debug (client)
impl std::fmt::Debug for EventDualityServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventDualityServer")
            .field("state", &self.state)
            .finish()
    }
}

impl EventDualityServer {
    pub fn new_with_state(
        state: Arc<Mutex<ConversationState>>,
        local_models: Arc<LocalModels>,
        artifact_store: Arc<FileStore>,
        job_store: Arc<JobStore>,
        audio_graph_adapter: Arc<AudioGraphAdapter>,
        audio_graph_db: Arc<AudioGraphDb>,
    ) -> Self {
        Self {
            // tool_router: Self::tool_router(), // No longer needed for manual dispatch
            state,
            local_models,
            artifact_store,
            job_store: (*job_store).clone(),
            audio_graph_adapter,
            audio_graph_db,
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

        // Job store
        let job_store = Arc::new(JobStore::new());

        // Audio graph - initialize with in-memory database for default
        let audio_graph_db = Arc::new(AudioGraphDb::in_memory().expect("Failed to create audio graph db"));
        let audio_graph_adapter = Arc::new(
            AudioGraphAdapter::new_without_pipewire(audio_graph_db.clone())
                .expect("Failed to create audio graph adapter")
        );

        Self::new_with_state(
            Arc::new(Mutex::new(state)),
            local_models,
            artifact_store,
            job_store,
            audio_graph_adapter,
            audio_graph_db,
        )
    }
}
