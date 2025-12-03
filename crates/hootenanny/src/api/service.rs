use crate::artifact_store::FileStore;
use crate::job_system::JobStore;
use crate::mcp_tools::local_models::LocalModels;
use audio_graph_mcp::{AudioGraphAdapter, Database as AudioGraphDb};
use std::sync::{Arc, RwLock};

#[derive(Clone)]
pub struct EventDualityServer {
    pub local_models: Arc<LocalModels>,
    pub artifact_store: Arc<RwLock<FileStore>>,
    pub job_store: JobStore,
    pub audio_graph_db: Arc<AudioGraphDb>,
    pub graph_adapter: Arc<AudioGraphAdapter>,
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
    ) -> Self {
        Self {
            local_models,
            artifact_store,
            job_store: (*job_store).clone(),
            audio_graph_db,
            graph_adapter,
        }
    }
}
