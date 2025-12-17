//! GardenDaemon - Real state management for chaosgarden
//!
//! Replaces StubHandler with actual state:
//! - Transport state (playing, position, tempo)
//! - Regions on the timeline
//! - Trustfall queries over graph state
//! - Latent lifecycle management

use std::sync::{Arc, RwLock};

#[cfg(test)]
use std::collections::HashMap;

use tracing::{debug, info, warn};
#[cfg(test)]
use trustfall::execute_query;
use uuid::Uuid;

use crate::ipc::{
    Beat as IpcBeat, ContentType as IpcContentType, PendingApproval as IpcPendingApproval,
    RegionSummary, SampleFormat as IpcSampleFormat, ShellReply, ShellRequest,
    StreamDefinition as IpcStreamDefinition, StreamFormat as IpcStreamFormat,
};
use crate::pipewire_output::{PipeWireOutputConfig, PipeWireOutputStream};
#[cfg(test)]
use crate::ipc::QueryReply;
use crate::primitives::{Behavior, ContentType};
use crate::stream_io::{
    SampleFormat, StreamDefinition, StreamFormat, StreamManager, StreamUri,
};
use crate::{
    Beat, ChaosgardenAdapter, Graph, LatentConfig, LatentManager, Region, TempoMap, Tick,
    TickClock,
};

/// Transport state
#[derive(Debug, Clone, Default)]
pub struct TransportState {
    pub playing: bool,
    pub position: Beat,
}

/// Configuration for the daemon
#[derive(Debug, Clone)]
pub struct DaemonConfig {
    pub sample_rate: u32,
    pub buffer_size: usize,
    pub auto_approve_tools: Vec<String>,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            sample_rate: 44100,
            buffer_size: 256,
            auto_approve_tools: vec![],
        }
    }
}

/// The main daemon state
///
/// Note: Several fields are scaffolding for upcoming playback integration.
/// See docs/agents/plans/chaosgarden/13-wire-daemon.md for the full plan.
/// The handle_shell method exercises these fields in tests.
pub struct GardenDaemon {
    // Transport - used by handle_shell for playback control (tested)
    #[allow(dead_code)]
    transport: RwLock<TransportState>,
    #[allow(dead_code)]
    tempo_map: Arc<RwLock<TempoMap>>,

    // Timeline - used by handle_shell for region management (tested)
    #[allow(dead_code)]
    regions: Arc<RwLock<Vec<Region>>>,

    // Graph - scaffolding for audio routing (see 13-wire-daemon.md Phase 4)
    #[allow(dead_code)]
    graph: Arc<RwLock<Graph>>,

    // Latent management - used by handle_shell for latent lifecycle (tested)
    #[allow(dead_code)]
    latent_manager: Arc<RwLock<LatentManager>>,

    // Stream capture manager - actively used by Cap'n Proto server
    stream_manager: Arc<StreamManager>,

    // Stream event publisher
    stream_publisher: Arc<dyn StreamEventPublisher>,

    // Active PipeWire input streams
    active_inputs: Arc<RwLock<std::collections::HashMap<crate::stream_io::StreamUri, crate::pipewire_input::PipeWireInputStream>>>,

    // Query adapter - scaffolding for Trustfall queries (see 13-wire-daemon.md Phase 5)
    #[allow(dead_code)]
    query_adapter: Option<Arc<ChaosgardenAdapter>>,

    // Tick clock for position advancement via wall time
    tick_clock: Arc<RwLock<TickClock>>,

    // Optional PipeWire audio output (attached dynamically)
    audio_output: RwLock<Option<PipeWireOutputStream>>,
}

impl GardenDaemon {
    /// Create a new daemon with default configuration
    pub fn new() -> Self {
        Self::with_config(DaemonConfig::default())
    }

    /// Create a new daemon with custom configuration
    pub fn with_config(config: DaemonConfig) -> Self {
        let tempo_map = Arc::new(RwLock::new(TempoMap::new(120.0, Default::default())));
        let regions = Arc::new(RwLock::new(Vec::new()));
        let graph = Arc::new(RwLock::new(Graph::new()));

        let latent_config = LatentConfig {
            auto_approve_tools: config.auto_approve_tools.iter().cloned().collect(),
            default_mix_in: Default::default(),
            max_concurrent_jobs: 4,
        };

        // Create a no-op IOPub publisher for now
        let publisher = Arc::new(NoOpPublisher);
        let latent_manager = Arc::new(RwLock::new(LatentManager::new(latent_config, publisher)));

        // Create stream manager and publisher
        let stream_manager = Arc::new(StreamManager::new());
        let stream_publisher: Arc<dyn StreamEventPublisher> = Arc::new(NoOpStreamPublisher);

        // Build query adapter
        let query_adapter = ChaosgardenAdapter::new(
            Arc::clone(&regions),
            Arc::clone(&graph),
            Arc::clone(&tempo_map),
        ).ok().map(Arc::new);

        // Create tick clock for position advancement
        let tick_clock = Arc::new(RwLock::new(TickClock::new(Arc::clone(&tempo_map))));

        Self {
            transport: RwLock::new(TransportState::default()),
            tempo_map,
            regions,
            graph,
            latent_manager,
            stream_manager,
            stream_publisher,
            active_inputs: Arc::new(RwLock::new(std::collections::HashMap::new())),
            query_adapter,
            tick_clock,
            audio_output: RwLock::new(None),
        }
    }

    // === Transport control methods ===
    // These are called by handle_shell (tested) and will be wired to Cap'n Proto
    // server once playback integration is complete. See 13-wire-daemon.md.

    fn play(&self) {
        self.tick_clock.write().unwrap().start();
        let mut transport = self.transport.write().unwrap();
        transport.playing = true;
        info!("Playback started at beat {}", transport.position.0);
    }

    fn pause(&self) {
        self.tick_clock.write().unwrap().pause();
        let mut transport = self.transport.write().unwrap();
        transport.playing = false;
        info!("Playback paused at beat {}", transport.position.0);
    }

    fn stop(&self) {
        self.tick_clock.write().unwrap().stop();
        let mut transport = self.transport.write().unwrap();
        transport.playing = false;
        transport.position = Beat(0.0);
        info!("Playback stopped");
    }

    fn seek(&self, beat: Beat) {
        self.tick_clock.write().unwrap().seek(beat);
        let mut transport = self.transport.write().unwrap();
        transport.position = beat;
        info!("Seeked to beat {}", beat.0);
    }

    fn set_tempo(&self, bpm: f64) {
        self.tempo_map.write().unwrap().set_base_tempo(bpm);
        info!("Set tempo to {} BPM", bpm);
    }

    fn get_transport_state(&self) -> (bool, Beat, f64) {
        let transport = self.transport.read().unwrap();
        let tempo = self.tempo_map.read().unwrap().tempo_at(Tick(0));
        (transport.playing, transport.position, tempo)
    }

    /// Called by the tick loop to advance position based on wall time
    ///
    /// This is the main driver for playback position when running.
    pub fn tick(&self) {
        // Get updated position from tick clock
        let position = self.tick_clock.write().unwrap().tick();

        // Update transport state
        let mut transport = self.transport.write().unwrap();
        if transport.playing {
            transport.position = position;
        }
    }

    // === Audio output attachment methods ===

    /// Attach a PipeWire audio output device
    ///
    /// If audio is already attached, it will be detached first.
    fn attach_audio(
        &self,
        device_name: Option<String>,
        sample_rate: Option<u32>,
        latency_frames: Option<u32>,
    ) -> Result<(), String> {
        // Detach any existing audio first
        self.detach_audio();

        let config = PipeWireOutputConfig {
            name: device_name.unwrap_or_else(|| "chaosgarden".to_string()),
            sample_rate: sample_rate.unwrap_or(48000),
            channels: 2,
            latency_frames: latency_frames.unwrap_or(256),
        };

        info!(
            "Attaching audio output: {} @ {}Hz, {} frames latency",
            config.name, config.sample_rate, config.latency_frames
        );

        let stream = PipeWireOutputStream::new(config)
            .map_err(|e| format!("Failed to create PipeWire output: {}", e))?;

        *self.audio_output.write().unwrap() = Some(stream);
        info!("Audio output attached");
        Ok(())
    }

    /// Detach the current audio output (if any)
    fn detach_audio(&self) {
        let mut output = self.audio_output.write().unwrap();
        if output.is_some() {
            *output = None;
            info!("Audio output detached");
        }
    }

    /// Get audio output status
    fn get_audio_status(&self) -> ShellReply {
        let output = self.audio_output.read().unwrap();
        match output.as_ref() {
            Some(stream) => {
                let config = stream.config();
                let stats = stream.stats();
                use std::sync::atomic::Ordering;
                ShellReply::AudioStatus {
                    attached: true,
                    device_name: Some(config.name.clone()),
                    sample_rate: Some(config.sample_rate),
                    latency_frames: Some(config.latency_frames),
                    callbacks: stats.callbacks.load(Ordering::Relaxed),
                    samples_written: stats.samples_written.load(Ordering::Relaxed),
                    underruns: stats.underruns.load(Ordering::Relaxed),
                }
            }
            None => ShellReply::AudioStatus {
                attached: false,
                device_name: None,
                sample_rate: None,
                latency_frames: None,
                callbacks: 0,
                samples_written: 0,
                underruns: 0,
            },
        }
    }

    fn create_region(&self, position: Beat, duration: Beat, behavior: &crate::ipc::Behavior) -> Uuid {
        let internal_behavior = convert_ipc_behavior_to_internal(behavior);
        let region = Region {
            id: Uuid::new_v4(),
            position,
            duration,
            behavior: internal_behavior,
            metadata: crate::primitives::RegionMetadata::default(),
            lifecycle: crate::primitives::Lifecycle::default(),
        };
        let region_id = region.id;
        self.regions.write().unwrap().push(region);
        info!("Created region {} at beat {}", region_id, position.0);
        region_id
    }

    fn delete_region(&self, region_id: Uuid) -> bool {
        let mut regions = self.regions.write().unwrap();
        let len_before = regions.len();
        regions.retain(|r| r.id != region_id);
        let deleted = regions.len() < len_before;
        if deleted {
            info!("Deleted region {}", region_id);
        } else {
            warn!("Region {} not found for deletion", region_id);
        }
        deleted
    }

    fn move_region(&self, region_id: Uuid, new_position: Beat) -> bool {
        let mut regions = self.regions.write().unwrap();
        if let Some(region) = regions.iter_mut().find(|r| r.id == region_id) {
            region.position = new_position;
            info!("Moved region {} to beat {}", region_id, new_position.0);
            true
        } else {
            warn!("Region {} not found for move", region_id);
            false
        }
    }

    fn get_regions(&self, range: Option<(Beat, Beat)>) -> Vec<RegionSummary> {
        let regions = self.regions.read().unwrap();
        regions
            .iter()
            .filter(|r| {
                if let Some((start, end)) = range {
                    r.position >= start && r.position < end
                } else {
                    true
                }
            })
            .map(|r| {
                // Extract artifact_id from behavior
                let artifact_id = match &r.behavior {
                    Behavior::PlayContent { content_hash, .. } => Some(content_hash.clone()),
                    Behavior::Latent { state, .. } => {
                        state.resolved.as_ref().map(|rc| rc.artifact_id.clone())
                    }
                    _ => None,
                };
                RegionSummary {
                    region_id: r.id,
                    position: IpcBeat(r.position.0),
                    duration: IpcBeat(r.duration.0),
                    is_latent: r.is_latent(),
                    artifact_id,
                }
            })
            .collect()
    }

    fn handle_latent_started(&self, region_id: Uuid, job_id: String) -> Result<(), String> {
        let mut regions = self.regions.write().unwrap();
        let mut latent_manager = self.latent_manager.write().unwrap();
        latent_manager.handle_job_started(region_id, job_id.clone(), &mut regions);
        info!("Latent job {} started for region {}", job_id, region_id);
        Ok(())
    }

    fn handle_latent_progress(&self, region_id: Uuid, progress: f32) -> Result<(), String> {
        let mut regions = self.regions.write().unwrap();
        let mut latent_manager = self.latent_manager.write().unwrap();
        latent_manager.handle_progress(region_id, progress, &mut regions);
        debug!("Latent progress for region {}: {:.1}%", region_id, progress * 100.0);
        Ok(())
    }

    fn handle_latent_resolved(
        &self,
        region_id: Uuid,
        artifact_id: String,
        content_hash: String,
        content_type: ContentType,
    ) -> Result<(), String> {
        let mut regions = self.regions.write().unwrap();
        let mut latent_manager = self.latent_manager.write().unwrap();
        latent_manager.handle_resolved(
            region_id,
            artifact_id.clone(),
            content_hash,
            content_type,
            &mut regions,
        );
        info!("Latent region {} resolved with artifact {}", region_id, artifact_id);
        Ok(())
    }

    fn handle_latent_failed(&self, region_id: Uuid, error: String) -> Result<(), String> {
        let mut regions = self.regions.write().unwrap();
        let mut latent_manager = self.latent_manager.write().unwrap();
        latent_manager.handle_failed(region_id, error.clone(), &mut regions);
        warn!("Latent region {} failed: {}", region_id, error);
        Ok(())
    }

    fn handle_approve(&self, region_id: Uuid, decided_by: Uuid) -> Result<(), String> {
        let mut regions = self.regions.write().unwrap();
        let mut latent_manager = self.latent_manager.write().unwrap();
        latent_manager
            .approve(region_id, decided_by, &mut regions)
            .map_err(|e| e.to_string())?;
        info!("Latent region {} approved by {}", region_id, decided_by);
        Ok(())
    }

    fn handle_reject(&self, region_id: Uuid, decided_by: Uuid, reason: Option<String>) -> Result<(), String> {
        let mut regions = self.regions.write().unwrap();
        let mut latent_manager = self.latent_manager.write().unwrap();
        latent_manager
            .reject(region_id, decided_by, reason.clone(), &mut regions)
            .map_err(|e| e.to_string())?;
        info!("Latent region {} rejected by {}: {:?}", region_id, decided_by, reason);
        Ok(())
    }

    fn get_pending_approvals(&self) -> Vec<IpcPendingApproval> {
        let latent_manager = self.latent_manager.read().unwrap();
        latent_manager
            .pending_approvals()
            .into_iter()
            .map(|pa| IpcPendingApproval {
                region_id: pa.region_id,
                artifact_id: pa.artifact_id.clone(),
                content_hash: pa.content_hash.clone(),
                content_type: convert_content_type_to_ipc(pa.content_type),
                // NOTE: Using current time as approximation - PendingApproval doesn't track resolution timestamp yet
                resolved_at: chrono::Utc::now(),
            })
            .collect()
    }

    #[cfg(test)]
    fn execute_query(&self, query: &str, variables: &HashMap<String, serde_json::Value>) -> QueryReply {
        let adapter = match &self.query_adapter {
            Some(a) => Arc::clone(a),
            None => {
                return QueryReply::Error {
                    error: "Query adapter not initialized".to_string(),
                };
            }
        };

        // Convert variables to FieldValue
        let vars: std::collections::BTreeMap<Arc<str>, trustfall::FieldValue> = variables
            .iter()
            .map(|(k, v)| {
                let field_value = json_to_field_value(v);
                (Arc::from(k.as_str()), field_value)
            })
            .collect();

        let schema = adapter.schema_arc();
        match execute_query(&schema, adapter, query, vars) {
            Ok(results) => {
                let rows: Vec<serde_json::Value> = results
                    .take(100)
                    .map(|row| {
                        let obj: serde_json::Map<String, serde_json::Value> = row
                            .into_iter()
                            .map(|(k, v)| (k.to_string(), field_value_to_json(&v)))
                            .collect();
                        serde_json::Value::Object(obj)
                    })
                    .collect();
                QueryReply::Results { rows }
            }
            Err(e) => QueryReply::Error {
                error: e.to_string(),
            },
        }
    }

    /// Handle stream start command
    pub fn handle_stream_start(
        &self,
        uri: String,
        definition: IpcStreamDefinition,
        chunk_path: String,
    ) -> Result<(), String> {
        // Convert IPC types to internal types
        let stream_uri = StreamUri::from(uri.as_str());
        let internal_def = convert_ipc_stream_definition(&definition, stream_uri.clone());

        // Start the stream (creates metadata and staging chunk)
        self.stream_manager
            .start_stream(internal_def.clone(), &chunk_path)
            .map_err(|e| e.to_string())?;

        info!("Started stream: {} -> {}", uri, chunk_path);

        // Start PipeWire capture
        use crate::pipewire_input::{PipeWireInputConfig, PipeWireInputStream};

        // Extract audio format info
        let (sample_rate, channels) = match &internal_def.format {
            crate::stream_io::StreamFormat::Audio { sample_rate, channels, .. } => {
                (*sample_rate, *channels as u32)
            }
            crate::stream_io::StreamFormat::Midi => {
                return Err("MIDI capture not yet supported".to_string());
            }
        };

        // Create PipeWire input config
        // NOTE: device_identity is the PipeWire node name (e.g., "alsa_input.usb-...")
        let pw_config = PipeWireInputConfig {
            device_name: definition.device_identity.clone(),
            stream_uri: stream_uri.clone(),
            sample_rate,
            channels,
        };

        // Create and start PipeWire input stream
        let input_stream = PipeWireInputStream::new(pw_config, self.stream_manager.clone())
            .map_err(|e| format!("Failed to start PipeWire capture: {}", e))?;

        // Track active input
        self.active_inputs
            .write()
            .unwrap()
            .insert(stream_uri.clone(), input_stream);

        info!("PipeWire capture started for stream: {}", uri);
        Ok(())
    }

    /// Handle stream chunk switch command
    pub fn handle_stream_switch_chunk(&self, uri: String, new_chunk_path: String) -> Result<(), String> {
        let stream_uri = StreamUri::from(uri.as_str());

        self.stream_manager
            .switch_chunk(&stream_uri, &new_chunk_path)
            .map_err(|e| e.to_string())?;

        info!("Switched chunk for stream: {} -> {}", uri, new_chunk_path);
        Ok(())
    }

    /// Handle stream stop command
    pub fn handle_stream_stop(&self, uri: String) -> Result<(), String> {
        let stream_uri = StreamUri::from(uri.as_str());

        // Stop PipeWire capture first (if active)
        if let Some(mut input_stream) = self.active_inputs.write().unwrap().remove(&stream_uri) {
            input_stream.stop();
            info!("Stopped PipeWire capture for stream: {}", uri);
        }

        // Stop the stream (seals final chunk, creates manifest)
        self.stream_manager
            .stop_stream(&stream_uri)
            .map_err(|e| e.to_string())?;

        info!("Stopped stream: {}", uri);
        Ok(())
    }

    /// Poll active streams and send broadcast updates
    ///
    /// This should be called periodically (e.g., every 100ms) by a background task.
    /// It checks all active streams for:
    /// - Head position updates (always broadcast)
    /// - Chunk full events (broadcast when detected)
    pub fn poll_stream_events(&self) {
        let active = self.stream_manager.active_streams();

        for uri in active {
            // Get head position
            if let Ok(pos) = self.stream_manager.head_position(&uri) {
                self.stream_publisher.publish_head_position(
                    uri.as_str().to_string(),
                    pos.sample_position,
                    pos.byte_position,
                );
            }

            // Check for full chunks
            if let Ok(is_full) = self.stream_manager.is_chunk_full(&uri) {
                if is_full {
                    if let Ok(Some(info)) = self.stream_manager.chunk_info(&uri) {
                        debug!("Detected full chunk for stream: {}", uri.as_str());
                        self.stream_publisher.publish_chunk_full(
                            uri.as_str().to_string(),
                            info.path.to_string_lossy().to_string(),
                            info.bytes_written,
                            info.samples_written,
                        );
                    }
                }
            }
        }
    }
}

impl Default for GardenDaemon {
    fn default() -> Self {
        Self::new()
    }
}

impl GardenDaemon {
    /// Handle shell requests
    ///
    /// This method dispatches ShellRequest variants to the appropriate internal handlers.
    /// Used by the ZMQ server to process incoming requests.
    pub fn handle_shell(&self, req: ShellRequest) -> ShellReply {
        match req {
            ShellRequest::Play => {
                self.play();
                ShellReply::Ok {
                    result: serde_json::Value::Null,
                }
            }
            ShellRequest::Pause => {
                self.pause();
                ShellReply::Ok {
                    result: serde_json::Value::Null,
                }
            }
            ShellRequest::Stop => {
                self.pause(); // Stop is same as pause in current implementation
                ShellReply::Ok {
                    result: serde_json::Value::Null,
                }
            }
            ShellRequest::Seek { beat } => {
                self.seek(Beat(beat.0));
                ShellReply::Ok {
                    result: serde_json::Value::Null,
                }
            }
            ShellRequest::SetTempo { bpm } => {
                self.set_tempo(bpm);
                ShellReply::Ok {
                    result: serde_json::Value::Null,
                }
            }
            ShellRequest::GetTransportState => {
                let transport = self.transport.read().unwrap();
                let tempo_map = self.tempo_map.read().unwrap();
                let current_tempo = tempo_map.tempo_at(Tick(0)); // Default to tempo at start
                ShellReply::TransportState {
                    playing: transport.playing,
                    position: IpcBeat(transport.position.0),
                    tempo: current_tempo,
                }
            }
            ShellRequest::CreateRegion { position, duration, behavior } => {
                let region_id = self.create_region(Beat(position.0), Beat(duration.0), &behavior);
                ShellReply::RegionCreated { region_id }
            }
            ShellRequest::DeleteRegion { region_id } => {
                if self.delete_region(region_id) {
                    ShellReply::Ok {
                        result: serde_json::Value::Null,
                    }
                } else {
                    ShellReply::Error {
                        error: format!("Region {} not found", region_id),
                        traceback: None,
                    }
                }
            }
            ShellRequest::MoveRegion { region_id, new_position } => {
                if self.move_region(region_id, Beat(new_position.0)) {
                    ShellReply::Ok {
                        result: serde_json::Value::Null,
                    }
                } else {
                    ShellReply::Error {
                        error: format!("Region {} not found", region_id),
                        traceback: None,
                    }
                }
            }
            ShellRequest::GetRegions { range } => {
                let regions = self.get_regions(range.map(|(start, end)| (Beat(start.0), Beat(end.0))));
                ShellReply::Regions { regions }
            }
            ShellRequest::GetPendingApprovals => {
                let approvals = self.get_pending_approvals();
                ShellReply::PendingApprovals { approvals }
            }

            // Latent lifecycle management
            ShellRequest::UpdateLatentStarted { region_id, job_id } => {
                match self.handle_latent_started(region_id, job_id) {
                    Ok(()) => ShellReply::Ok { result: serde_json::Value::Null },
                    Err(e) => ShellReply::Error { error: e, traceback: None },
                }
            }
            ShellRequest::UpdateLatentProgress { region_id, progress } => {
                match self.handle_latent_progress(region_id, progress) {
                    Ok(()) => ShellReply::Ok { result: serde_json::Value::Null },
                    Err(e) => ShellReply::Error { error: e, traceback: None },
                }
            }
            ShellRequest::UpdateLatentResolved { region_id, artifact_id, content_hash, content_type } => {
                let internal_content_type = convert_ipc_content_type_to_internal(&content_type);
                match self.handle_latent_resolved(region_id, artifact_id, content_hash, internal_content_type) {
                    Ok(()) => ShellReply::Ok { result: serde_json::Value::Null },
                    Err(e) => ShellReply::Error { error: e, traceback: None },
                }
            }
            ShellRequest::UpdateLatentFailed { region_id, error } => {
                match self.handle_latent_failed(region_id, error) {
                    Ok(()) => ShellReply::Ok { result: serde_json::Value::Null },
                    Err(e) => ShellReply::Error { error: e, traceback: None },
                }
            }
            ShellRequest::ApproveLatent { region_id, decided_by } => {
                match self.handle_approve(region_id, decided_by) {
                    Ok(()) => ShellReply::Ok { result: serde_json::Value::Null },
                    Err(e) => ShellReply::Error { error: e, traceback: None },
                }
            }
            ShellRequest::RejectLatent { region_id, decided_by, reason } => {
                match self.handle_reject(region_id, decided_by, reason) {
                    Ok(()) => ShellReply::Ok { result: serde_json::Value::Null },
                    Err(e) => ShellReply::Error { error: e, traceback: None },
                }
            }

            // Stream commands are handled by the stream manager
            ShellRequest::StreamStart { uri, definition, chunk_path } => {
                match self.handle_stream_start(uri, definition, chunk_path) {
                    Ok(()) => ShellReply::Ok { result: serde_json::Value::Null },
                    Err(e) => ShellReply::Error { error: e, traceback: None },
                }
            }
            ShellRequest::StreamSwitchChunk { uri, new_chunk_path } => {
                match self.handle_stream_switch_chunk(uri, new_chunk_path) {
                    Ok(()) => ShellReply::Ok { result: serde_json::Value::Null },
                    Err(e) => ShellReply::Error { error: e, traceback: None },
                }
            }
            ShellRequest::StreamStop { uri } => {
                match self.handle_stream_stop(uri) {
                    Ok(()) => ShellReply::Ok { result: serde_json::Value::Null },
                    Err(e) => ShellReply::Error { error: e, traceback: None },
                }
            }

            // Audio output attachment
            ShellRequest::AttachAudio { device_name, sample_rate, latency_frames } => {
                match self.attach_audio(device_name, sample_rate, latency_frames) {
                    Ok(()) => ShellReply::Ok {
                        result: serde_json::json!({"status": "attached"}),
                    },
                    Err(e) => ShellReply::Error { error: e, traceback: None },
                }
            }
            ShellRequest::DetachAudio => {
                self.detach_audio();
                ShellReply::Ok {
                    result: serde_json::json!({"status": "detached"}),
                }
            }
            ShellRequest::GetAudioStatus => self.get_audio_status(),

            // Unhandled requests
            _ => ShellReply::Error {
                error: format!("Unhandled shell request: {:?}", req),
                traceback: None,
            },
        }
    }
}

/// Convert IPC Behavior to internal Behavior
fn convert_ipc_behavior_to_internal(ipc: &crate::ipc::Behavior) -> Behavior {
    match ipc {
        crate::ipc::Behavior::PlayContent { artifact_id } => Behavior::PlayContent {
            content_hash: artifact_id.clone(),
            content_type: crate::primitives::ContentType::Audio, // Default, could be enhanced
            params: crate::primitives::PlaybackParams::default(),
        },
        crate::ipc::Behavior::Latent { job_id } => Behavior::Latent {
            tool: "orpheus".to_string(), // Default tool
            params: serde_json::json!({"job_id": job_id}),
            state: crate::primitives::LatentState::default(),
        },
        crate::ipc::Behavior::ApplyProcessing { parameter, curve } => Behavior::ApplyProcessing {
            target_node: Uuid::nil(), // Will be connected later
            parameter: parameter.clone(),
            curve: curve.iter().map(|p| crate::primitives::CurvePoint {
                position: p.beat.0,
                value: p.value,
                curve: crate::primitives::CurveType::Linear,
            }).collect(),
        },
        crate::ipc::Behavior::EmitTrigger { event_type } => Behavior::EmitTrigger {
            kind: crate::primitives::TriggerKind::Custom(event_type.clone()),
            data: None,
        },
    }
}

/// Convert IPC ContentType to internal ContentType
fn convert_ipc_content_type_to_internal(ipc: &IpcContentType) -> ContentType {
    match ipc {
        IpcContentType::Audio => ContentType::Audio,
        IpcContentType::Midi => ContentType::Midi,
        IpcContentType::Control => ContentType::Audio, // Default to Audio for Control
    }
}

/// Convert internal ContentType to IPC ContentType
fn convert_content_type_to_ipc(internal: ContentType) -> IpcContentType {
    match internal {
        ContentType::Audio => IpcContentType::Audio,
        ContentType::Midi => IpcContentType::Midi,
    }
}

/// Convert IPC StreamDefinition to internal StreamDefinition
fn convert_ipc_stream_definition(ipc: &IpcStreamDefinition, uri: StreamUri) -> StreamDefinition {
    StreamDefinition {
        uri,
        device_identity: ipc.device_identity.clone(),
        format: convert_ipc_stream_format(&ipc.format),
        chunk_size_bytes: ipc.chunk_size_bytes,
    }
}

/// Convert IPC StreamFormat to internal StreamFormat
fn convert_ipc_stream_format(ipc: &IpcStreamFormat) -> StreamFormat {
    match ipc {
        IpcStreamFormat::Audio {
            sample_rate,
            channels,
            sample_format,
        } => StreamFormat::Audio {
            sample_rate: *sample_rate,
            channels: *channels as u8,
            sample_format: convert_ipc_sample_format(sample_format),
        },
        IpcStreamFormat::Midi => StreamFormat::Midi,
    }
}

/// Convert IPC SampleFormat to internal SampleFormat
fn convert_ipc_sample_format(ipc: &IpcSampleFormat) -> SampleFormat {
    match ipc {
        IpcSampleFormat::F32Le => SampleFormat::F32,
        IpcSampleFormat::S16Le => SampleFormat::I16,
        IpcSampleFormat::S24Le => SampleFormat::I24,
        // S32 not supported in internal format, default to I24
        IpcSampleFormat::S32Le => SampleFormat::I24,
    }
}

/// Trait for publishing stream events to IOPub channel
pub trait StreamEventPublisher: Send + Sync {
    fn publish_head_position(
        &self,
        stream_uri: String,
        sample_position: u64,
        byte_position: u64,
    );

    fn publish_chunk_full(
        &self,
        stream_uri: String,
        path: String,
        bytes_written: u64,
        samples_written: u64,
    );

    fn publish_stream_error(&self, stream_uri: String, error: String, recoverable: bool);
}

/// No-op IOPub publisher for daemon initialization
struct NoOpPublisher;

impl crate::IOPubPublisher for NoOpPublisher {
    fn publish(&self, _event: crate::LatentEvent) {
        // No-op for now - will wire to actual IOPub socket later
    }
}

/// No-op stream event publisher for daemon initialization
struct NoOpStreamPublisher;

impl StreamEventPublisher for NoOpStreamPublisher {
    fn publish_head_position(
        &self,
        _stream_uri: String,
        _sample_position: u64,
        _byte_position: u64,
    ) {
        // No-op for now - will wire to actual IOPub socket later
    }

    fn publish_chunk_full(
        &self,
        _stream_uri: String,
        _path: String,
        _bytes_written: u64,
        _samples_written: u64,
    ) {
        // No-op for now - will wire to actual IOPub socket later
    }

    fn publish_stream_error(&self, _stream_uri: String, _error: String, _recoverable: bool) {
        // No-op for now - will wire to actual IOPub socket later
    }
}

/// Convert JSON value to Trustfall FieldValue
#[cfg(test)]
fn json_to_field_value(v: &serde_json::Value) -> trustfall::FieldValue {
    match v {
        serde_json::Value::Null => trustfall::FieldValue::Null,
        serde_json::Value::Bool(b) => trustfall::FieldValue::Boolean(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                trustfall::FieldValue::Int64(i)
            } else if let Some(u) = n.as_u64() {
                trustfall::FieldValue::Uint64(u)
            } else if let Some(f) = n.as_f64() {
                trustfall::FieldValue::Float64(f)
            } else {
                trustfall::FieldValue::Null
            }
        }
        serde_json::Value::String(s) => trustfall::FieldValue::String(s.clone().into()),
        serde_json::Value::Array(arr) => {
            let items: Vec<_> = arr.iter().map(json_to_field_value).collect();
            trustfall::FieldValue::List(items.into())
        }
        serde_json::Value::Object(_) => {
            // Trustfall doesn't support objects directly
            trustfall::FieldValue::Null
        }
    }
}

/// Convert Trustfall FieldValue to JSON
#[cfg(test)]
fn field_value_to_json(v: &trustfall::FieldValue) -> serde_json::Value {
    match v {
        trustfall::FieldValue::Null => serde_json::Value::Null,
        trustfall::FieldValue::Boolean(b) => serde_json::Value::Bool(*b),
        trustfall::FieldValue::Int64(i) => serde_json::Value::Number((*i).into()),
        trustfall::FieldValue::Uint64(u) => serde_json::Value::Number((*u).into()),
        trustfall::FieldValue::Float64(f) => {
            serde_json::Number::from_f64(*f)
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null)
        }
        trustfall::FieldValue::String(s) => serde_json::Value::String(s.to_string()),
        trustfall::FieldValue::List(items) => {
            let arr: Vec<_> = items.iter().map(field_value_to_json).collect();
            serde_json::Value::Array(arr)
        }
        _ => serde_json::Value::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transport_state() {
        let daemon = GardenDaemon::new();

        // Initially stopped at beat 0
        let (playing, position, tempo) = daemon.get_transport_state();
        assert!(!playing);
        assert_eq!(position.0, 0.0);
        assert_eq!(tempo, 120.0);

        // Play
        daemon.play();
        let (playing, _, _) = daemon.get_transport_state();
        assert!(playing);

        // Pause
        daemon.pause();
        let (playing, _, _) = daemon.get_transport_state();
        assert!(!playing);

        // Seek
        daemon.seek(Beat(16.0));
        let (_, position, _) = daemon.get_transport_state();
        assert_eq!(position.0, 16.0);

        // Stop resets position
        daemon.stop();
        let (playing, position, _) = daemon.get_transport_state();
        assert!(!playing);
        assert_eq!(position.0, 0.0);
    }

    #[test]
    fn test_set_tempo() {
        let daemon = GardenDaemon::new();

        // Initial tempo is 120
        let (_, _, tempo) = daemon.get_transport_state();
        assert_eq!(tempo, 120.0);

        // Set new tempo
        daemon.set_tempo(140.0);
        let (_, _, tempo) = daemon.get_transport_state();
        assert_eq!(tempo, 140.0);

        // Set another tempo
        daemon.set_tempo(90.0);
        let (_, _, tempo) = daemon.get_transport_state();
        assert_eq!(tempo, 90.0);
    }

    #[test]
    fn test_handler_transport() {
        let daemon = GardenDaemon::new();

        // GetTransportState
        let reply = daemon.handle_shell(ShellRequest::GetTransportState);
        match reply {
            ShellReply::TransportState { playing, position, tempo } => {
                assert!(!playing);
                assert_eq!(position.0, 0.0);
                assert_eq!(tempo, 120.0);
            }
            _ => panic!("expected TransportState"),
        }

        // Play
        let reply = daemon.handle_shell(ShellRequest::Play);
        assert!(matches!(reply, ShellReply::Ok { .. }));

        // Verify playing
        let reply = daemon.handle_shell(ShellRequest::GetTransportState);
        match reply {
            ShellReply::TransportState { playing, .. } => assert!(playing),
            _ => panic!("expected TransportState"),
        }
    }

    #[test]
    fn test_create_region() {
        let daemon = GardenDaemon::new();

        // Create a region with PlayContent behavior
        let behavior = crate::ipc::Behavior::PlayContent {
            artifact_id: "hash_abc123".to_string(),
        };
        let reply = daemon.handle_shell(ShellRequest::CreateRegion {
            position: IpcBeat(4.0),
            duration: IpcBeat(8.0),
            behavior,
        });

        // Should get back a region ID
        let region_id = match reply {
            ShellReply::RegionCreated { region_id } => region_id,
            other => panic!("expected RegionCreated, got {:?}", other),
        };

        // Verify region appears in list
        let regions = daemon.get_regions(None);
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].region_id, region_id);
        assert_eq!(regions[0].position.0, 4.0);
        assert_eq!(regions[0].duration.0, 8.0);
        assert!(!regions[0].is_latent);
    }

    #[test]
    fn test_create_latent_region() {
        let daemon = GardenDaemon::new();

        // Create a latent region
        let behavior = crate::ipc::Behavior::Latent {
            job_id: "job_xyz789".to_string(),
        };
        let reply = daemon.handle_shell(ShellRequest::CreateRegion {
            position: IpcBeat(0.0),
            duration: IpcBeat(16.0),
            behavior,
        });

        let region_id = match reply {
            ShellReply::RegionCreated { region_id } => region_id,
            other => panic!("expected RegionCreated, got {:?}", other),
        };

        // Verify latent flag
        let regions = daemon.get_regions(None);
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].region_id, region_id);
        assert!(regions[0].is_latent);
    }

    #[test]
    fn test_delete_region() {
        let daemon = GardenDaemon::new();

        // Create a region
        let behavior = crate::ipc::Behavior::PlayContent {
            artifact_id: "hash_123".to_string(),
        };
        let reply = daemon.handle_shell(ShellRequest::CreateRegion {
            position: IpcBeat(0.0),
            duration: IpcBeat(4.0),
            behavior,
        });
        let region_id = match reply {
            ShellReply::RegionCreated { region_id } => region_id,
            _ => panic!("expected RegionCreated"),
        };

        // Verify it exists
        assert_eq!(daemon.get_regions(None).len(), 1);

        // Delete it
        let reply = daemon.handle_shell(ShellRequest::DeleteRegion { region_id });
        assert!(matches!(reply, ShellReply::Ok { .. }));

        // Verify it's gone
        assert_eq!(daemon.get_regions(None).len(), 0);
    }

    #[test]
    fn test_delete_nonexistent_region() {
        let daemon = GardenDaemon::new();
        let fake_id = Uuid::new_v4();

        let reply = daemon.handle_shell(ShellRequest::DeleteRegion { region_id: fake_id });
        assert!(matches!(reply, ShellReply::Error { .. }));
    }

    #[test]
    fn test_move_region() {
        let daemon = GardenDaemon::new();

        // Create a region at beat 0
        let behavior = crate::ipc::Behavior::PlayContent {
            artifact_id: "hash_456".to_string(),
        };
        let reply = daemon.handle_shell(ShellRequest::CreateRegion {
            position: IpcBeat(0.0),
            duration: IpcBeat(4.0),
            behavior,
        });
        let region_id = match reply {
            ShellReply::RegionCreated { region_id } => region_id,
            _ => panic!("expected RegionCreated"),
        };

        // Move it to beat 8
        let reply = daemon.handle_shell(ShellRequest::MoveRegion {
            region_id,
            new_position: IpcBeat(8.0),
        });
        assert!(matches!(reply, ShellReply::Ok { .. }));

        // Verify new position
        let regions = daemon.get_regions(None);
        assert_eq!(regions[0].position.0, 8.0);
    }

    #[test]
    fn test_move_nonexistent_region() {
        let daemon = GardenDaemon::new();
        let fake_id = Uuid::new_v4();

        let reply = daemon.handle_shell(ShellRequest::MoveRegion {
            region_id: fake_id,
            new_position: IpcBeat(4.0),
        });
        assert!(matches!(reply, ShellReply::Error { .. }));
    }

    #[test]
    fn test_get_regions_with_range() {
        let daemon = GardenDaemon::new();

        // Create regions at beats 0, 8, and 16
        for pos in [0.0, 8.0, 16.0] {
            let behavior = crate::ipc::Behavior::PlayContent {
                artifact_id: format!("hash_{}", pos as i32),
            };
            daemon.handle_shell(ShellRequest::CreateRegion {
                position: IpcBeat(pos),
                duration: IpcBeat(4.0),
                behavior,
            });
        }

        // Get all regions
        assert_eq!(daemon.get_regions(None).len(), 3);

        // Get regions in range [4, 12) - should only get the one at beat 8
        let filtered = daemon.get_regions(Some((Beat(4.0), Beat(12.0))));
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].position.0, 8.0);
    }

    #[test]
    fn test_handler_get_regions() {
        let daemon = GardenDaemon::new();

        // Create a region via handler
        let behavior = crate::ipc::Behavior::PlayContent {
            artifact_id: "hash_handler".to_string(),
        };
        daemon.handle_shell(ShellRequest::CreateRegion {
            position: IpcBeat(2.0),
            duration: IpcBeat(6.0),
            behavior,
        });

        // Query via handler
        let reply = daemon.handle_shell(ShellRequest::GetRegions { range: None });
        match reply {
            ShellReply::Regions { regions } => {
                assert_eq!(regions.len(), 1);
                assert_eq!(regions[0].position.0, 2.0);
            }
            other => panic!("expected Regions, got {:?}", other),
        }
    }

    #[test]
    fn test_latent_lifecycle_started() {
        let daemon = GardenDaemon::new();

        // Create a latent region
        let behavior = crate::ipc::Behavior::Latent {
            job_id: "job_test".to_string(),
        };
        let reply = daemon.handle_shell(ShellRequest::CreateRegion {
            position: IpcBeat(0.0),
            duration: IpcBeat(8.0),
            behavior,
        });
        let region_id = match reply {
            ShellReply::RegionCreated { region_id } => region_id,
            _ => panic!("expected RegionCreated"),
        };

        // Send started notification
        let reply = daemon.handle_shell(ShellRequest::UpdateLatentStarted {
            region_id,
            job_id: "job_abc123".to_string(),
        });
        assert!(matches!(reply, ShellReply::Ok { .. }));
    }

    #[test]
    fn test_latent_lifecycle_progress() {
        let daemon = GardenDaemon::new();

        // Create a latent region
        let behavior = crate::ipc::Behavior::Latent {
            job_id: "job_prog".to_string(),
        };
        let reply = daemon.handle_shell(ShellRequest::CreateRegion {
            position: IpcBeat(0.0),
            duration: IpcBeat(8.0),
            behavior,
        });
        let region_id = match reply {
            ShellReply::RegionCreated { region_id } => region_id,
            _ => panic!("expected RegionCreated"),
        };

        // Start job first
        daemon.handle_shell(ShellRequest::UpdateLatentStarted {
            region_id,
            job_id: "job_prog".to_string(),
        });

        // Send progress updates
        let reply = daemon.handle_shell(ShellRequest::UpdateLatentProgress {
            region_id,
            progress: 0.5,
        });
        assert!(matches!(reply, ShellReply::Ok { .. }));
    }

    #[test]
    fn test_latent_lifecycle_resolved_and_approve() {
        let daemon = GardenDaemon::new();

        // Create a latent region
        let behavior = crate::ipc::Behavior::Latent {
            job_id: "job_resolve".to_string(),
        };
        let reply = daemon.handle_shell(ShellRequest::CreateRegion {
            position: IpcBeat(0.0),
            duration: IpcBeat(8.0),
            behavior,
        });
        let region_id = match reply {
            ShellReply::RegionCreated { region_id } => region_id,
            _ => panic!("expected RegionCreated"),
        };

        // Start job
        daemon.handle_shell(ShellRequest::UpdateLatentStarted {
            region_id,
            job_id: "job_resolve".to_string(),
        });

        // Resolve with artifact
        let reply = daemon.handle_shell(ShellRequest::UpdateLatentResolved {
            region_id,
            artifact_id: "artifact_xyz".to_string(),
            content_hash: "hash_xyz".to_string(),
            content_type: IpcContentType::Audio,
        });
        assert!(matches!(reply, ShellReply::Ok { .. }));

        // Should have pending approval now
        let approvals = daemon.get_pending_approvals();
        assert_eq!(approvals.len(), 1);
        assert_eq!(approvals[0].region_id, region_id);

        // Approve it
        let decider = Uuid::new_v4();
        let reply = daemon.handle_shell(ShellRequest::ApproveLatent {
            region_id,
            decided_by: decider,
        });
        assert!(matches!(reply, ShellReply::Ok { .. }));

        // No more pending approvals
        let approvals = daemon.get_pending_approvals();
        assert!(approvals.is_empty());
    }

    #[test]
    fn test_latent_lifecycle_failed() {
        let daemon = GardenDaemon::new();

        // Create a latent region
        let behavior = crate::ipc::Behavior::Latent {
            job_id: "job_fail".to_string(),
        };
        let reply = daemon.handle_shell(ShellRequest::CreateRegion {
            position: IpcBeat(0.0),
            duration: IpcBeat(8.0),
            behavior,
        });
        let region_id = match reply {
            ShellReply::RegionCreated { region_id } => region_id,
            _ => panic!("expected RegionCreated"),
        };

        // Start job
        daemon.handle_shell(ShellRequest::UpdateLatentStarted {
            region_id,
            job_id: "job_fail".to_string(),
        });

        // Mark as failed
        let reply = daemon.handle_shell(ShellRequest::UpdateLatentFailed {
            region_id,
            error: "Generation failed".to_string(),
        });
        assert!(matches!(reply, ShellReply::Ok { .. }));
    }

    #[test]
    fn test_latent_reject() {
        let daemon = GardenDaemon::new();

        // Create and resolve a latent region
        let behavior = crate::ipc::Behavior::Latent {
            job_id: "job_reject".to_string(),
        };
        let reply = daemon.handle_shell(ShellRequest::CreateRegion {
            position: IpcBeat(0.0),
            duration: IpcBeat(8.0),
            behavior,
        });
        let region_id = match reply {
            ShellReply::RegionCreated { region_id } => region_id,
            _ => panic!("expected RegionCreated"),
        };

        daemon.handle_shell(ShellRequest::UpdateLatentStarted {
            region_id,
            job_id: "job_reject".to_string(),
        });

        daemon.handle_shell(ShellRequest::UpdateLatentResolved {
            region_id,
            artifact_id: "artifact_rej".to_string(),
            content_hash: "hash_rej".to_string(),
            content_type: IpcContentType::Midi,
        });

        // Reject it
        let decider = Uuid::new_v4();
        let reply = daemon.handle_shell(ShellRequest::RejectLatent {
            region_id,
            decided_by: decider,
            reason: Some("Not what I wanted".to_string()),
        });
        assert!(matches!(reply, ShellReply::Ok { .. }));
    }

    #[test]
    fn test_get_pending_approvals_empty() {
        let daemon = GardenDaemon::new();
        let reply = daemon.handle_shell(ShellRequest::GetPendingApprovals);
        match reply {
            ShellReply::PendingApprovals { approvals } => {
                assert!(approvals.is_empty());
            }
            other => panic!("expected PendingApprovals, got {:?}", other),
        }
    }
}
