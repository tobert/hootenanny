use crate::artifact_store::FileStore;
use crate::gpu_monitor::GpuMonitor;
use crate::job_system::JobStore;
use crate::mcp_tools::local_models::LocalModels;
use crate::sessions::SessionManager;
use crate::streams::{SlicingEngine, StreamManager};
use crate::zmq::{BroadcastPublisher, GardenManager};
use audio_graph_mcp::{AudioGraphAdapter, Database as AudioGraphDb};
use std::sync::{Arc, RwLock};

#[derive(Clone)]
pub struct EventDualityServer {
    pub local_models: Arc<LocalModels>,
    pub artifact_store: Arc<RwLock<FileStore>>,
    pub job_store: JobStore,
    pub audio_graph_db: Arc<AudioGraphDb>,
    pub graph_adapter: Arc<AudioGraphAdapter>,
    pub gpu_monitor: Arc<GpuMonitor>,
    pub garden_manager: Option<Arc<GardenManager>>,
    /// Optional broadcast publisher for SSE events via holler
    pub broadcaster: Option<BroadcastPublisher>,
    /// Stream manager for capture sessions
    pub stream_manager: Option<Arc<StreamManager>>,
    /// Session manager for multi-stream capture coordination
    pub session_manager: Option<Arc<SessionManager>>,
    /// Slicing engine for time-range extraction
    pub slicing_engine: Option<Arc<SlicingEngine>>,
}

impl std::fmt::Debug for EventDualityServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventDualityServer")
            .field("job_store", &"...")
            .finish()
    }
}

impl EventDualityServer {
    pub fn new(
        local_models: Arc<LocalModels>,
        artifact_store: Arc<RwLock<FileStore>>,
        job_store: Arc<JobStore>,
        audio_graph_db: Arc<AudioGraphDb>,
        graph_adapter: Arc<AudioGraphAdapter>,
        gpu_monitor: Arc<GpuMonitor>,
    ) -> Self {
        Self {
            local_models,
            artifact_store,
            job_store: (*job_store).clone(),
            audio_graph_db,
            graph_adapter,
            gpu_monitor,
            garden_manager: None,
            broadcaster: None,
            stream_manager: None,
            session_manager: None,
            slicing_engine: None,
        }
    }

    /// Create with garden manager for chaosgarden connection
    pub fn with_garden(mut self, garden_manager: Option<Arc<GardenManager>>) -> Self {
        self.garden_manager = garden_manager;
        self
    }

    /// Create with broadcast publisher for SSE events
    pub fn with_broadcaster(mut self, broadcaster: Option<BroadcastPublisher>) -> Self {
        self.broadcaster = broadcaster;
        self
    }

    /// Create with stream manager for capture sessions
    pub fn with_stream_manager(mut self, stream_manager: Option<Arc<StreamManager>>) -> Self {
        self.stream_manager = stream_manager;
        self
    }

    /// Create with session manager for multi-stream coordination
    pub fn with_session_manager(mut self, session_manager: Option<Arc<SessionManager>>) -> Self {
        self.session_manager = session_manager;
        self
    }

    /// Create with slicing engine for time-range extraction
    pub fn with_slicing_engine(mut self, slicing_engine: Option<Arc<SlicingEngine>>) -> Self {
        self.slicing_engine = slicing_engine;
        self
    }

    /// Broadcast an artifact creation event (fire-and-forget)
    pub fn broadcast_artifact_created(
        &self,
        artifact_id: &str,
        content_hash: &str,
        tags: Vec<String>,
        creator: Option<String>,
    ) {
        if let Some(ref broadcaster) = self.broadcaster {
            let broadcaster = broadcaster.clone();
            let artifact_id = artifact_id.to_string();
            let content_hash = content_hash.to_string();
            tokio::spawn(async move {
                if let Err(e) = broadcaster.artifact_created(&artifact_id, &content_hash, tags, creator).await {
                    tracing::warn!("Failed to broadcast artifact_created: {}", e);
                }
            });
        }
    }
}
