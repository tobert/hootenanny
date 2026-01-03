use crate::artifact_store::FileStore;
use crate::event_buffer::EventBufferHandle;
use crate::gpu_monitor::GpuMonitor;
use crate::job_system::JobStore;
use crate::mcp_tools::local_models::LocalModels;
use crate::sessions::SessionManager;
use crate::streams::{SlicingEngine, StreamManager};
use crate::zmq::{BroadcastPublisher, GardenManager, VibeweaverClient};
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
    /// Optional vibeweaver client for Python kernel proxy
    pub vibeweaver: Option<Arc<VibeweaverClient>>,
    /// Optional broadcast publisher for SSE events via holler
    pub broadcaster: Option<BroadcastPublisher>,
    /// Stream manager for capture sessions
    pub stream_manager: Option<Arc<StreamManager>>,
    /// Session manager for multi-stream capture coordination
    pub session_manager: Option<Arc<SessionManager>>,
    /// Slicing engine for time-range extraction
    pub slicing_engine: Option<Arc<SlicingEngine>>,
    /// Event buffer for cursor-based polling
    pub event_buffer: Option<EventBufferHandle>,
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
            vibeweaver: None,
            broadcaster: None,
            stream_manager: None,
            session_manager: None,
            slicing_engine: None,
            event_buffer: None,
        }
    }

    /// Create with garden manager for chaosgarden connection
    pub fn with_garden(mut self, garden_manager: Option<Arc<GardenManager>>) -> Self {
        self.garden_manager = garden_manager;
        self
    }

    /// Create with vibeweaver client for Python kernel proxy
    pub fn with_vibeweaver(mut self, vibeweaver: Option<Arc<VibeweaverClient>>) -> Self {
        self.vibeweaver = vibeweaver;
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

    /// Create with event buffer for cursor-based polling
    pub fn with_event_buffer(mut self, event_buffer: Option<EventBufferHandle>) -> Self {
        self.event_buffer = event_buffer;
        self
    }

    /// Start listening for stream events from chaosgarden
    ///
    /// This spawns a background task that:
    /// - Subscribes to IOPub events from garden_manager
    /// - Handles StreamChunkFull by rotating chunks
    /// - Logs StreamHeadPosition for monitoring
    ///
    /// Must be called after garden_manager.start_event_listener().
    pub async fn start_stream_event_handler(&self) -> anyhow::Result<()> {
        use hooteproto::garden::IOPubEvent;
        use tokio_stream::StreamExt;
        use tracing::{debug, error, info, warn};

        let garden_manager = self
            .garden_manager
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("garden_manager not available"))?
            .clone();

        let stream_manager = self
            .stream_manager
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("stream_manager not available"))?
            .clone();

        // Take the event stream (can only be done once)
        let mut events = garden_manager
            .take_events()
            .await
            .ok_or_else(|| anyhow::anyhow!("event stream already taken"))?;

        // Clone garden_manager for sending commands
        let garden_for_rotation = garden_manager.clone();

        tokio::spawn(async move {
            while let Some(event) = events.next().await {
                match event {
                    IOPubEvent::StreamHeadPosition {
                        stream_uri,
                        sample_position,
                        byte_position,
                        wall_clock: _,
                    } => {
                        debug!(
                            stream.uri = %stream_uri,
                            stream.samples = sample_position,
                            stream.bytes = byte_position,
                            "Stream head position update"
                        );
                    }

                    IOPubEvent::StreamChunkFull {
                        stream_uri,
                        path,
                        bytes_written,
                        samples_written,
                        wall_clock: _,
                    } => {
                        info!(
                            stream.uri = %stream_uri,
                            chunk.path = %path,
                            chunk.bytes = bytes_written,
                            chunk.samples = samples_written,
                            "Stream chunk full, rotating"
                        );

                        // Handle chunk rotation (Task 6)
                        if let Err(e) = Self::handle_chunk_rotation(
                            &stream_manager,
                            &garden_for_rotation,
                            &stream_uri,
                            &path,
                            bytes_written,
                            samples_written,
                        )
                        .await
                        {
                            error!("Failed to rotate chunk for {}: {}", stream_uri, e);
                        }
                    }

                    IOPubEvent::StreamError {
                        stream_uri,
                        error,
                        recoverable,
                    } => {
                        if recoverable {
                            warn!(stream.uri = %stream_uri, "Stream error (recoverable): {}", error);
                        } else {
                            error!(stream.uri = %stream_uri, "Stream error (fatal): {}", error);
                        }
                    }

                    _ => {
                        // Ignore other event types
                    }
                }
            }

            info!("Stream event handler stopped");
        });

        Ok(())
    }

    /// Handle chunk rotation when a chunk becomes full
    ///
    /// Steps:
    /// 1. Seal the full chunk to CAS (staging → content)
    /// 2. Create a new staging chunk
    /// 3. Send StreamSwitchChunk command to chaosgarden
    async fn handle_chunk_rotation(
        stream_manager: &Arc<StreamManager>,
        garden_manager: &Arc<GardenManager>,
        stream_uri_str: &str,
        old_chunk_path: &str,
        bytes_written: u64,
        samples_written: u64,
    ) -> anyhow::Result<()> {
        use crate::streams::StreamUri;
        use hooteproto::garden::ShellRequest;
        use tracing::info;

        let uri = StreamUri::from(stream_uri_str);

        // Step 1: Seal the full chunk and create a new staging chunk
        let new_chunk_path = stream_manager
            .handle_chunk_full(&uri, bytes_written, Some(samples_written))
            .map_err(|e| anyhow::anyhow!("failed to rotate chunk: {}", e))?;

        info!(
            stream.uri = %stream_uri_str,
            old_chunk = %old_chunk_path,
            new_chunk = %new_chunk_path.display(),
            bytes = bytes_written,
            samples = samples_written,
            "Rotated chunk: sealed old, created new"
        );

        // Step 2: Send StreamSwitchChunk command to chaosgarden
        let shell_request = ShellRequest::StreamSwitchChunk {
            uri: stream_uri_str.to_string(),
            new_chunk_path: new_chunk_path.to_string_lossy().to_string(),
        };

        let reply = garden_manager
            .request(shell_request)
            .await
            .map_err(|e| anyhow::anyhow!("failed to send StreamSwitchChunk: {}", e))?;

        // Check for error response
        if let hooteproto::garden::ShellReply::Error { error, .. } = reply {
            anyhow::bail!("chaosgarden rejected StreamSwitchChunk: {}", error);
        }

        info!(
            stream.uri = %stream_uri_str,
            new_chunk = %new_chunk_path.display(),
            "Successfully sent StreamSwitchChunk to chaosgarden"
        );

        Ok(())
    }
}
