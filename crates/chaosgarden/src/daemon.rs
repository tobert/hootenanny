//! GardenDaemon - Real state management for chaosgarden
//!
//! Replaces StubHandler with actual state:
//! - Transport state (playing, position, tempo)
//! - Regions on the timeline
//! - Latent lifecycle management
//! - Snapshot export for external query evaluation

use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, RwLock};

use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::ipc::{
    Beat as IpcBeat, ContentType as IpcContentType, PendingApproval as IpcPendingApproval,
    RegionSummary, SampleFormat as IpcSampleFormat, ShellReply, ShellRequest,
    StreamDefinition as IpcStreamDefinition, StreamFormat as IpcStreamFormat,
};
use crate::external_io::{audio_ring_pair, AudioRingConsumer, AudioRingProducer};
use crate::mixer::{MixerChannel, MixerState};
use crate::monitor_input::{MonitorInputConfig, MonitorInputStream};
use crate::nodes::ContentResolver;
use crate::pipewire_output::{MonitorMixState, PipeWireOutputConfig, PipeWireOutputStream};
use crate::rave_streaming::RaveStreamingClient;
use crate::playback::{CompiledGraph, PlaybackEngine};
use crate::primitives::{Behavior, ContentType};
use crate::stream_io::{
    SampleFormat, StreamDefinition, StreamFormat, StreamManager, StreamUri,
};
use crate::{Beat, Graph, LatentConfig, LatentManager, Region, TempoMap, Tick, TickClock};

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

    // Tick clock for position advancement via wall time
    tick_clock: Arc<RwLock<TickClock>>,

    // Optional PipeWire audio output (attached dynamically)
    audio_output: RwLock<Option<PipeWireOutputStream>>,

    // Optional PipeWire monitor input (attached dynamically)
    monitor_input: RwLock<Option<MonitorInputStream>>,
    // Lock-free ring consumer for monitor mixing (created by attach_input, consumed by attach_audio)
    // Uses Mutex instead of RwLock because AudioRingConsumer is Send but not Sync
    monitor_consumer: Mutex<Option<AudioRingConsumer>>,

    // Audio mixer state (control plane for all mixing)
    // The mixer holds channels with gain/pan/mute/solo controls.
    // Ring buffers are transport and stay separate.
    #[allow(dead_code)]
    mixer: MixerState,
    // Reference to the monitor channel in the mixer (for convenience)
    monitor_channel: Arc<MixerChannel>,

    // === Playback Engine (Phase 3) ===
    // Content resolver for loading audio from CAS
    content_resolver: Option<Arc<dyn ContentResolver>>,
    // Playback engine processes regions and produces audio
    playback_engine: RwLock<Option<PlaybackEngine>>,
    // Compiled graph for RT processing (currently empty - placeholder for future graph routing)
    compiled_graph: RwLock<Option<CompiledGraph>>,
    // Timeline audio producer (written by tick(), consumer is in RT callback)
    // Lock-free SPSC ring - producer writes rendered audio, consumer reads in RT
    timeline_producer: Mutex<Option<AudioRingProducer>>,

    // Streaming tap for WebSocket/HTTP audio streaming (lock-free SPSC)
    // Consumer is read by get_audio_snapshot(), producer is moved to RT callback
    streaming_tap_consumer: Mutex<AudioRingConsumer>,
    // Producer is taken when audio output is attached and moved to RT thread
    streaming_tap_producer: Mutex<Option<AudioRingProducer>>,
    streaming_tap_sample_rate: u32,

    // Monotonic version counter for snapshot invalidation
    snapshot_version: std::sync::atomic::AtomicU64,

    // RAVE streaming client for realtime neural audio processing
    rave_streaming: Mutex<RaveStreamingClient>,
    // RAVE audio ring buffers (created when streaming starts, consumed by RT callback)
    // Arc-wrapped so they can be passed to the PipeWire thread
    // Producer: write monitor audio to RAVE input
    rave_input_producer: Arc<Mutex<Option<AudioRingProducer>>>,
    // Consumer: read RAVE-processed audio for mixing
    rave_output_consumer: Arc<Mutex<Option<AudioRingConsumer>>>,

    // MIDI I/O manager (direct ALSA for low latency)
    midi_manager: crate::midi_io::MidiIOManager,
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

        // Create tick clock for position advancement
        let tick_clock = Arc::new(RwLock::new(TickClock::new(Arc::clone(&tempo_map))));

        // Create mixer with monitor channel
        let mut mixer = MixerState::new();
        let monitor_channel = mixer.add_channel(MixerChannel::new("monitor"));
        // Default: enabled but muted until attach_input is called
        monitor_channel.enabled.store(false, Ordering::Relaxed);
        monitor_channel.set_gain(0.8); // Default 80% gain

        // Create streaming tap (lock-free SPSC) - ~500ms of stereo audio at 48kHz
        // Producer moves to RT callback when output is attached, consumer stays for snapshots
        let streaming_tap_sample_rate = 48000u32;
        let streaming_tap_capacity = streaming_tap_sample_rate as usize * 2; // ~500ms stereo
        let (streaming_tap_producer, streaming_tap_consumer) = audio_ring_pair(streaming_tap_capacity);

        Self {
            transport: RwLock::new(TransportState::default()),
            tempo_map,
            regions,
            graph,
            latent_manager,
            stream_manager,
            stream_publisher,
            active_inputs: Arc::new(RwLock::new(std::collections::HashMap::new())),
            tick_clock,
            audio_output: RwLock::new(None),
            monitor_input: RwLock::new(None),
            monitor_consumer: Mutex::new(None),
            mixer,
            monitor_channel,
            // Playback engine fields - initialized lazily when content_resolver is set
            content_resolver: None,
            playback_engine: RwLock::new(None),
            compiled_graph: RwLock::new(None),
            timeline_producer: Mutex::new(None),
            streaming_tap_consumer: Mutex::new(streaming_tap_consumer),
            streaming_tap_producer: Mutex::new(Some(streaming_tap_producer)),
            streaming_tap_sample_rate,
            snapshot_version: std::sync::atomic::AtomicU64::new(0),
            rave_streaming: Mutex::new(RaveStreamingClient::new()),
            rave_input_producer: Arc::new(Mutex::new(None::<AudioRingProducer>)),
            rave_output_consumer: Arc::new(Mutex::new(None::<AudioRingConsumer>)),
            midi_manager: crate::midi_io::MidiIOManager::new(),
        }
    }

    /// Set the content resolver for loading audio from CAS
    ///
    /// This enables timeline playback by allowing the PlaybackEngine to load
    /// audio content from the content-addressable storage.
    pub fn set_content_resolver(&mut self, resolver: Arc<dyn ContentResolver>) {
        // Clone the Arc<RwLock<TempoMap>>, then get an Arc<TempoMap> from it
        let tempo_map_snapshot = Arc::new(self.tempo_map.read().unwrap().clone());
        let engine = PlaybackEngine::with_resolver(
            48000,  // Default sample rate (will match PipeWire)
            256,    // Default buffer size
            tempo_map_snapshot,
            Arc::clone(&resolver),
        );
        self.content_resolver = Some(resolver);
        *self.playback_engine.write().unwrap() = Some(engine);

        // Create empty compiled graph for now (future: build from audio graph)
        let mut empty_graph = Graph::new();
        if let Ok(compiled) = CompiledGraph::compile(&mut empty_graph, 256) {
            *self.compiled_graph.write().unwrap() = Some(compiled);
        }

        info!("Content resolver set, playback engine initialized");
    }

    // === Transport control methods ===
    // These are called by handle_shell (tested) and will be wired to Cap'n Proto
    // server once playback integration is complete. See 13-wire-daemon.md.

    fn play(&self) {
        self.tick_clock.write().unwrap().start();
        let mut transport = self.transport.write().unwrap();
        transport.playing = true;

        // Sync playback engine
        if let Some(ref mut engine) = *self.playback_engine.write().unwrap() {
            engine.play();
            engine.seek(transport.position);
        }

        info!("Playback started at beat {}", transport.position.0);
    }

    fn pause(&self) {
        self.tick_clock.write().unwrap().pause();
        let mut transport = self.transport.write().unwrap();
        transport.playing = false;

        // Sync playback engine
        if let Some(ref mut engine) = *self.playback_engine.write().unwrap() {
            engine.pause();
        }

        info!("Playback paused at beat {}", transport.position.0);
    }

    #[allow(dead_code)]
    fn stop(&self) {
        self.tick_clock.write().unwrap().stop();
        let mut transport = self.transport.write().unwrap();
        transport.playing = false;
        transport.position = Beat(0.0);

        // Sync playback engine
        if let Some(ref mut engine) = *self.playback_engine.write().unwrap() {
            engine.stop();
        }

        info!("Playback stopped");
    }

    fn seek(&self, beat: Beat) {
        self.tick_clock.write().unwrap().seek(beat);
        let mut transport = self.transport.write().unwrap();
        transport.position = beat;

        // Sync playback engine
        if let Some(ref mut engine) = *self.playback_engine.write().unwrap() {
            engine.seek(beat);
        }

        info!("Seeked to beat {}", beat.0);
    }

    fn set_tempo(&self, bpm: f64) {
        self.tempo_map.write().unwrap().set_base_tempo(bpm);
        info!("Set tempo to {} BPM", bpm);
    }

    #[allow(dead_code)]
    fn get_transport_state(&self) -> (bool, Beat, f64) {
        let transport = self.transport.read().unwrap();
        let tempo = self.tempo_map.read().unwrap().tempo_at(Tick(0));
        (transport.playing, transport.position, tempo)
    }

    /// Build a full state snapshot for Trustfall query evaluation in hootenanny.
    ///
    /// This collects all queryable state into a GardenSnapshot struct that can be
    /// serialized to Cap'n Proto and sent over ZMQ. Designed to minimize allocations
    /// by reusing existing data structures where possible.
    pub fn build_snapshot(&self, version: u64) -> hooteproto::GardenSnapshot {
        use hooteproto::garden_snapshot::*;

        // Transport state
        let transport = self.transport.read().unwrap();
        let tempo_map = self.tempo_map.read().unwrap();
        let transport_snapshot = TransportSnapshot {
            playing: transport.playing,
            position: transport.position.0,
            tempo: tempo_map.tempo_at(crate::Tick(0)),
        };

        // Regions
        let regions = self.regions.read().unwrap();
        let region_snapshots: Vec<RegionSnapshot> = regions
            .iter()
            .map(|r| self.region_to_snapshot(r))
            .collect();

        // Graph
        let graph = self.graph.read().unwrap();
        let graph_snapshot = graph.snapshot();
        let nodes: Vec<GraphNode> = graph_snapshot
            .nodes
            .iter()
            .map(|n| GraphNode {
                id: n.id.to_string(),
                name: n.name.clone(),
                type_id: n.type_id.clone(),
                inputs: n.inputs.iter().map(|p| Port {
                    name: p.name.clone(),
                    signal_type: signal_type_to_snapshot(&p.signal_type),
                }).collect(),
                outputs: n.outputs.iter().map(|p| Port {
                    name: p.name.clone(),
                    signal_type: signal_type_to_snapshot(&p.signal_type),
                }).collect(),
                latency_samples: n.latency_samples as u32,
                can_realtime: n.capabilities.realtime,
                can_offline: n.capabilities.offline,
            })
            .collect();

        let edges: Vec<GraphEdge> = graph_snapshot
            .edges
            .iter()
            .map(|e| GraphEdge {
                source_id: e.source_id.to_string(),
                source_port: e.source_port.clone(),
                dest_id: e.dest_id.to_string(),
                dest_port: e.dest_port.clone(),
            })
            .collect();

        // Latent jobs and approvals
        let latent_manager = self.latent_manager.read().unwrap();
        let latent_jobs: Vec<LatentJob> = regions
            .iter()
            .filter_map(|r| {
                if let crate::primitives::Behavior::Latent { tool, state, .. } = &r.behavior {
                    if state.status == crate::primitives::LatentStatus::Running {
                        Some(LatentJob {
                            id: state.job_id.clone().unwrap_or_default(),
                            region_id: r.id.to_string(),
                            tool: tool.clone(),
                            progress: state.progress,
                        })
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect();

        let pending_approvals: Vec<ApprovalInfo> = latent_manager
            .pending_approvals()
            .iter()
            .map(|a| ApprovalInfo {
                region_id: a.region_id.to_string(),
                content_hash: a.content_hash.clone(),
                content_type: content_type_to_snapshot(&a.content_type),
            })
            .collect();

        // Tempo map
        let tempo_map_snapshot = TempoMapSnapshot {
            default_tempo: tempo_map.tempo_at(crate::Tick(0)),
            ticks_per_beat: 480, // Standard MIDI resolution
            changes: vec![], // TODO: Extract tempo changes from TempoMap
        };

        // I/O state - minimal for now, can be expanded
        let outputs = self.build_audio_output_snapshot();
        let inputs = self.build_audio_input_snapshot();

        hooteproto::GardenSnapshot {
            version,
            transport: transport_snapshot,
            regions: region_snapshots,
            nodes,
            edges,
            latent_jobs,
            pending_approvals,
            outputs,
            inputs,
            midi_devices: vec![], // TODO: Add MIDI device tracking
            tempo_map: tempo_map_snapshot,
        }
    }

    /// Convert a Region to RegionSnapshot
    fn region_to_snapshot(&self, region: &crate::Region) -> hooteproto::garden_snapshot::RegionSnapshot {
        use hooteproto::garden_snapshot::*;
        use crate::primitives::Behavior;

        let (behavior_type, content_hash, content_type, latent_status, latent_progress, job_id, generation_tool) =
            match &region.behavior {
                Behavior::PlayContent { content_hash, content_type, .. } => (
                    BehaviorType::PlayContent,
                    Some(content_hash.clone()),
                    Some(content_type_to_snapshot(content_type)),
                    None,
                    0.0,
                    None,
                    None,
                ),
                Behavior::Latent { tool, state, .. } => (
                    BehaviorType::Latent,
                    state.resolved.as_ref().map(|r| r.content_hash.clone()),
                    state.resolved.as_ref().map(|r| content_type_to_snapshot(&r.content_type)),
                    Some(latent_status_to_snapshot(&state.status)),
                    state.progress,
                    state.job_id.clone(),
                    Some(tool.clone()),
                ),
                Behavior::ApplyProcessing { .. } => (
                    BehaviorType::ApplyProcessing,
                    None,
                    None,
                    None,
                    0.0,
                    None,
                    None,
                ),
                Behavior::EmitTrigger { .. } => (
                    BehaviorType::EmitTrigger,
                    None,
                    None,
                    None,
                    0.0,
                    None,
                    None,
                ),
                Behavior::Custom { .. } => (
                    BehaviorType::Custom,
                    None,
                    None,
                    None,
                    0.0,
                    None,
                    None,
                ),
            };

        RegionSnapshot {
            id: region.id.to_string(),
            position: region.position.0,
            duration: region.duration.0,
            behavior_type,
            name: region.metadata.name.clone(),
            tags: region.metadata.tags.clone(),
            content_hash,
            content_type,
            latent_status,
            latent_progress,
            job_id,
            generation_tool,
            is_resolved: region.is_resolved(),
            is_approved: region.is_approved(),
            is_playable: region.is_playable(),
            is_alive: region.lifecycle.is_alive(),
            is_tombstoned: region.lifecycle.is_tombstoned(),
        }
    }

    /// Build audio output snapshot (minimal for now)
    fn build_audio_output_snapshot(&self) -> Vec<hooteproto::garden_snapshot::AudioOutput> {
        let output = self.audio_output.read().unwrap();
        match output.as_ref() {
            Some(stream) => {
                let config = stream.config();
                vec![hooteproto::garden_snapshot::AudioOutput {
                    id: "default".to_string(),
                    name: config.name.clone(),
                    channels: config.channels as u8,
                    pw_node_id: None, // TODO: Get PipeWire node ID
                }]
            }
            None => vec![],
        }
    }

    /// Build audio input snapshot (minimal for now)
    fn build_audio_input_snapshot(&self) -> Vec<hooteproto::garden_snapshot::AudioInput> {
        let input = self.monitor_input.read().unwrap();
        match input.as_ref() {
            Some(stream) => {
                let config = stream.config();
                vec![hooteproto::garden_snapshot::AudioInput {
                    id: "monitor".to_string(),
                    name: config.device_name.clone().unwrap_or_else(|| "default".to_string()),
                    channels: config.channels as u8,
                    port_pattern: None, // Not currently tracked in config
                    pw_node_id: None, // TODO: Get PipeWire node ID
                }]
            }
            None => vec![],
        }
    }

    /// Called by the tick loop to advance position based on wall time
    ///
    /// This is the main driver for playback position when running.
    /// Note: Monitor mixing now happens directly in the PipeWire output callback
    /// (RT thread) for proper timing. See pipewire_output.rs.
    ///
    /// Timeline playback (Phase 3):
    /// - When playing, calls PlaybackEngine.process() with current regions
    /// - Writes output audio to timeline ring buffer
    /// - RT callback reads from timeline ring and mixes with monitor
    pub fn tick(&self) {
        // Get updated position from tick clock
        let position = self.tick_clock.write().unwrap().tick();

        // Update transport state
        let is_playing = {
            let mut transport = self.transport.write().unwrap();
            if transport.playing {
                transport.position = position;
            }
            transport.playing
        };

        // Process playback engine if playing and we have all the pieces
        if is_playing {
            self.process_playback();
        }
    }

    /// Process the playback engine and write output to timeline producer (lock-free!)
    fn process_playback(&self) {
        // Get timeline producer (if audio output is attached)
        let mut producer_guard = match self.timeline_producer.lock() {
            Ok(guard) => guard,
            Err(_) => return,
        };
        let producer = match producer_guard.as_mut() {
            Some(p) => p,
            None => return, // No audio output attached
        };

        // Check if ring has room for a full buffer before processing
        // This prevents the engine from advancing ahead of actual playback
        // 256 frames * 2 channels = 512 samples minimum
        const MIN_RING_SPACE: usize = 512;
        if producer.space() < MIN_RING_SPACE {
            // Ring is full, skip this tick - RT callback will drain it
            return;
        }

        // Get playback engine
        let mut engine_guard = self.playback_engine.write().unwrap();
        let engine = match engine_guard.as_mut() {
            Some(e) => e,
            None => return, // No engine configured
        };

        // Get compiled graph
        let mut graph_guard = self.compiled_graph.write().unwrap();
        let graph = match graph_guard.as_mut() {
            Some(g) => g,
            None => return, // No graph compiled
        };

        // Get regions
        let regions = self.regions.read().unwrap();

        // Engine transport is synced by play/pause/stop/seek methods
        // Just process the buffer

        // Process one buffer
        match engine.process(graph, &regions) {
            Ok(output_buffer) => {
                // Write output to timeline producer (lock-free for RT playback)
                // The RT callback mixes this with monitor input and writes
                // the final mix to both PipeWire output and streaming tap
                // AudioBuffer.samples is interleaved [L, R, L, R, ...]
                producer.write(&output_buffer.samples);
            }
            Err(e) => {
                debug!("Playback process error: {}", e);
            }
        }
        // Note: producer_guard dropped here, releasing the Mutex
    }

    // === Audio output attachment methods ===

    /// Attach a PipeWire audio output device
    ///
    /// If audio is already attached, it will be detached first.
    /// If monitor input is attached, the output will be created with RT mixing enabled.
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

        // Take the streaming tap producer for the RT callback to write the final mix
        // If it was already consumed (re-attach scenario), create a new pair
        let streaming_tap = {
            let mut producer_guard = self.streaming_tap_producer.lock().unwrap();
            match producer_guard.take() {
                Some(producer) => Some(producer),
                None => {
                    // Recreate the pair for re-attach
                    let capacity = self.streaming_tap_sample_rate as usize * 2; // ~500ms stereo
                    let (new_producer, new_consumer) = audio_ring_pair(capacity);
                    // Update the consumer
                    *self.streaming_tap_consumer.lock().unwrap() = new_consumer;
                    Some(new_producer)
                }
            }
        };

        // Create lock-free SPSC pair for timeline audio
        // Sized for ~1 second of stereo audio at configured sample rate
        let ring_capacity = config.sample_rate as usize * 2 * 2; // 2 channels, 2 seconds
        let (timeline_producer, timeline_consumer) = audio_ring_pair(ring_capacity);

        // Check if we have a monitor consumer available - if so, use RT mixing
        let monitor_consumer = self.monitor_consumer.lock().unwrap().take();
        let stream = if let Some(consumer) = monitor_consumer {
            // Create monitor mix state for RT callback with lock-free consumer
            // Use the mixer channel's atomics for RT-safe control
            let monitor_state = MonitorMixState {
                consumer,
                enabled: Arc::clone(&self.monitor_channel.enabled),
                gain: Arc::clone(&self.monitor_channel.gain),
            };

            info!("Creating output stream with RT monitor mixing (lock-free)");
            PipeWireOutputStream::new_with_monitor(
                config,
                monitor_state,
                streaming_tap,
                Some(timeline_consumer),
                Some(Arc::clone(&self.rave_input_producer)),
                Some(Arc::clone(&self.rave_output_consumer)),
            )
                .map_err(|e| format!("Failed to create PipeWire output with monitor: {}", e))?
        } else {
            PipeWireOutputStream::new_with_streaming_tap(config, streaming_tap, Some(timeline_consumer))
                .map_err(|e| format!("Failed to create PipeWire output: {}", e))?
        };

        // Store the timeline producer for tick() to write to (lock-free!)
        *self.timeline_producer.lock().unwrap() = Some(timeline_producer);

        *self.audio_output.write().unwrap() = Some(stream);
        info!("Audio output attached (lock-free timeline available for playback)");
        Ok(())
    }

    /// Detach the current audio output (if any)
    fn detach_audio(&self) {
        // Clear timeline producer first
        *self.timeline_producer.lock().unwrap() = None;

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
                    monitor_reads: stats.monitor_reads.load(Ordering::Relaxed),
                    monitor_samples: stats.monitor_samples.load(Ordering::Relaxed),
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
                monitor_reads: 0,
                monitor_samples: 0,
            },
        }
    }

    // === Monitor input attachment methods ===

    /// Attach a PipeWire monitor input device
    ///
    /// Captures audio from the specified device and routes to monitor output.
    /// If audio output is already attached, it will be recreated with RT mixing.
    fn attach_input(
        &self,
        device_name: Option<String>,
        sample_rate: Option<u32>,
    ) -> Result<(), String> {
        // Detach any existing input first
        self.detach_input();

        let sample_rate = sample_rate.unwrap_or(48000);
        let channels = 2u32; // Stereo

        let config = MonitorInputConfig {
            device_name: device_name.clone(),
            sample_rate,
            channels,
        };

        info!(
            "Attaching monitor input: device={:?} @ {}Hz, {}ch",
            config.device_name, config.sample_rate, config.channels
        );

        // Create lock-free ring buffer pair
        // Size: ~500ms of audio at the given sample rate
        let ring_capacity = (sample_rate as usize) * (channels as usize) / 2;
        let (producer, consumer) = audio_ring_pair(ring_capacity);

        // Create input stream with the producer end
        let stream = MonitorInputStream::new(config, producer)
            .map_err(|e| format!("Failed to create monitor input: {}", e))?;

        // Store the consumer for when audio output is created
        *self.monitor_consumer.lock().unwrap() = Some(consumer);
        *self.monitor_input.write().unwrap() = Some(stream);
        info!("Monitor input attached (lock-free SPSC ring buffer)");

        // If audio output is already attached, recreate it with RT mixing
        let has_output = self.audio_output.read().unwrap().is_some();
        if has_output {
            info!("Recreating audio output with RT monitor mixing");
            // Get current output config before detaching
            let (output_name, output_rate, output_latency) = {
                let output_guard = self.audio_output.read().unwrap();
                if let Some(ref output) = *output_guard {
                    let cfg = output.config();
                    (Some(cfg.name.clone()), Some(cfg.sample_rate), Some(cfg.latency_frames))
                } else {
                    (None, None, None)
                }
            };
            // Recreate output with monitor mixing
            self.attach_audio(output_name, output_rate, output_latency)?;
        }

        Ok(())
    }

    /// Detach the current monitor input (if any)
    fn detach_input(&self) {
        // Clear consumer first (it may not have been used yet)
        *self.monitor_consumer.lock().unwrap() = None;

        let mut input = self.monitor_input.write().unwrap();
        if input.is_some() {
            *input = None;
            info!("Monitor input detached");
        }
    }

    /// Get monitor input status
    fn get_input_status(&self) -> ShellReply {
        let input = self.monitor_input.read().unwrap();
        let monitor_enabled = self.monitor_channel.enabled.load(Ordering::Relaxed);
        let monitor_gain = self.monitor_channel.get_gain();
        match input.as_ref() {
            Some(stream) => {
                let config = stream.config();
                let stats = stream.stats();
                ShellReply::InputStatus {
                    attached: true,
                    device_name: config.device_name.clone(),
                    sample_rate: Some(config.sample_rate),
                    channels: Some(config.channels),
                    monitor_enabled,
                    monitor_gain,
                    callbacks: stats.callbacks.load(Ordering::Relaxed),
                    samples_captured: stats.samples_captured.load(Ordering::Relaxed),
                    overruns: stats.overruns.load(Ordering::Relaxed),
                }
            }
            None => ShellReply::InputStatus {
                attached: false,
                device_name: None,
                sample_rate: None,
                channels: None,
                monitor_enabled,
                monitor_gain,
                callbacks: 0,
                samples_captured: 0,
                overruns: 0,
            },
        }
    }

    /// Set monitor enabled state and/or gain
    fn set_monitor(&self, enabled: Option<bool>, gain: Option<f32>) {
        if let Some(en) = enabled {
            self.monitor_channel.enabled.store(en, Ordering::Relaxed);
            info!("Monitor enabled: {}", en);
        }
        if let Some(g) = gain {
            // MixerChannel.set_gain clamps to 0.0-2.0, but for monitor we want 0.0-1.0
            let clamped = g.clamp(0.0, 1.0);
            self.monitor_channel.gain.store(clamped, Ordering::Relaxed);
            info!("Monitor gain: {}", clamped);
        }
    }

    /// Get an audio snapshot from the streaming tap buffer.
    ///
    /// Returns interleaved stereo f32 samples from the most recent output.
    /// This reads from the streaming tap consumer (lock-free SPSC from RT callback).
    fn get_audio_snapshot(&self, frames: u32) -> ShellReply {
        let samples_needed = frames as usize * 2; // stereo
        let mut samples = vec![0.0f32; samples_needed];

        if let Ok(mut consumer) = self.streaming_tap_consumer.lock() {
            // Read available samples from the lock-free ring buffer
            let read = consumer.read(&mut samples);
            if read < samples_needed {
                samples.truncate(read);
            }
        }

        ShellReply::AudioSnapshot {
            sample_rate: self.streaming_tap_sample_rate,
            channels: 2,
            format: 0, // 0 = f32le
            samples,
        }
    }

    // === MIDI I/O Methods ===

    /// List available MIDI ports
    fn list_midi_ports(&self) -> ShellReply {
        let inputs = match crate::midi_io::list_input_ports() {
            Ok(ports) => ports
                .into_iter()
                .map(|p| crate::ipc::MidiPortSpec {
                    index: p.index,
                    name: p.name,
                })
                .collect(),
            Err(e) => {
                return ShellReply::Error {
                    error: format!("Failed to list MIDI inputs: {}", e),
                    traceback: None,
                };
            }
        };

        let outputs = match crate::midi_io::list_output_ports() {
            Ok(ports) => ports
                .into_iter()
                .map(|p| crate::ipc::MidiPortSpec {
                    index: p.index,
                    name: p.name,
                })
                .collect(),
            Err(e) => {
                return ShellReply::Error {
                    error: format!("Failed to list MIDI outputs: {}", e),
                    traceback: None,
                };
            }
        };

        ShellReply::MidiPorts { inputs, outputs }
    }

    /// Attach a MIDI input by port pattern
    fn attach_midi_input(&self, port_pattern: &str) -> ShellReply {
        // For now, just log received MIDI - later we'll publish to IOPub
        let callback: crate::midi_io::MidiInputCallback = Box::new(move |msg| {
            debug!(
                "MIDI received: {:?} (timestamp: {}µs)",
                msg.message, msg.timestamp_us
            );
        });

        match self.midi_manager.attach_input(port_pattern, callback) {
            Ok(port_name) => {
                info!("Attached MIDI input: {}", port_name);
                ShellReply::MidiInputAttached { port_name }
            }
            Err(e) => ShellReply::Error {
                error: format!("Failed to attach MIDI input: {}", e),
                traceback: None,
            },
        }
    }

    /// Attach a MIDI output by port pattern
    fn attach_midi_output(&self, port_pattern: &str) -> ShellReply {
        match self.midi_manager.attach_output(port_pattern) {
            Ok(port_name) => {
                info!("Attached MIDI output: {}", port_name);
                ShellReply::MidiOutputAttached { port_name }
            }
            Err(e) => ShellReply::Error {
                error: format!("Failed to attach MIDI output: {}", e),
                traceback: None,
            },
        }
    }

    /// Detach a MIDI input
    fn detach_midi_input(&self, port_pattern: &str) -> ShellReply {
        if self.midi_manager.detach_input(port_pattern) {
            info!("Detached MIDI input matching: {}", port_pattern);
            ShellReply::Ok {
                result: serde_json::Value::Null,
            }
        } else {
            ShellReply::Error {
                error: format!("No MIDI input matching '{}' to detach", port_pattern),
                traceback: None,
            }
        }
    }

    /// Detach a MIDI output
    fn detach_midi_output(&self, port_pattern: &str) -> ShellReply {
        if self.midi_manager.detach_output(port_pattern) {
            info!("Detached MIDI output matching: {}", port_pattern);
            ShellReply::Ok {
                result: serde_json::Value::Null,
            }
        } else {
            ShellReply::Error {
                error: format!("No MIDI output matching '{}' to detach", port_pattern),
                traceback: None,
            }
        }
    }

    /// Send a MIDI message to connected outputs
    fn send_midi(&self, port_pattern: Option<&str>, message: &crate::ipc::MidiMessageSpec) -> ShellReply {
        use crate::primitives::MidiMessage;

        // Convert IPC MidiMessageSpec to internal MidiMessage
        let midi_msg = match message {
            crate::ipc::MidiMessageSpec::NoteOn { channel, pitch, velocity } => {
                MidiMessage::NoteOn {
                    channel: *channel,
                    pitch: *pitch,
                    velocity: *velocity,
                }
            }
            crate::ipc::MidiMessageSpec::NoteOff { channel, pitch } => {
                MidiMessage::NoteOff {
                    channel: *channel,
                    pitch: *pitch,
                }
            }
            crate::ipc::MidiMessageSpec::ControlChange { channel, controller, value } => {
                MidiMessage::ControlChange {
                    channel: *channel,
                    controller: *controller,
                    value: *value,
                }
            }
            crate::ipc::MidiMessageSpec::ProgramChange { channel, program } => {
                MidiMessage::ProgramChange {
                    channel: *channel,
                    program: *program,
                }
            }
            crate::ipc::MidiMessageSpec::PitchBend { channel, value } => {
                MidiMessage::PitchBend {
                    channel: *channel,
                    value: *value,
                }
            }
            crate::ipc::MidiMessageSpec::Raw { bytes } => {
                // Send raw MIDI bytes directly
                let result = if let Some(pattern) = port_pattern {
                    self.midi_manager.send_raw_to(pattern, bytes)
                } else {
                    self.midi_manager.send_raw_to_all(bytes)
                };
                return match result {
                    Ok(()) => ShellReply::Ok {
                        result: serde_json::Value::Null,
                    },
                    Err(e) => ShellReply::Error {
                        error: format!("Failed to send raw MIDI: {}", e),
                        traceback: None,
                    },
                };
            }
        };

        let result = if let Some(pattern) = port_pattern {
            self.midi_manager.send_to(pattern, &midi_msg)
        } else {
            self.midi_manager.send_to_all(&midi_msg)
        };

        match result {
            Ok(()) => ShellReply::Ok {
                result: serde_json::Value::Null,
            },
            Err(e) => ShellReply::Error {
                error: format!("Failed to send MIDI: {}", e),
                traceback: None,
            },
        }
    }

    /// Get MIDI I/O status
    fn get_midi_status(&self) -> ShellReply {
        let status = self.midi_manager.status();
        ShellReply::MidiStatus {
            inputs: status
                .inputs
                .into_iter()
                .map(|s| crate::ipc::MidiConnectionSpec {
                    port_name: s.port_name,
                    messages: s.messages,
                })
                .collect(),
            outputs: status
                .outputs
                .into_iter()
                .map(|s| crate::ipc::MidiConnectionSpec {
                    port_name: s.port_name,
                    messages: s.messages,
                })
                .collect(),
        }
    }

    // === RAVE Streaming Methods ===

    /// Start a RAVE streaming session
    ///
    /// This creates a pipeline: monitor input -> RAVE -> output mixer
    fn start_rave_streaming(
        &self,
        model: Option<String>,
        input_identity: String,
        output_identity: String,
        buffer_size: Option<u32>,
    ) -> ShellReply {
        let mut rave = self.rave_streaming.lock().unwrap();

        if rave.is_running() {
            return ShellReply::Error {
                error: "RAVE streaming already running".to_string(),
                traceback: None,
            };
        }

        // Generate stream ID
        let stream_id = format!("rave_{}", uuid::Uuid::new_v4().to_string().split('-').next().unwrap_or("unknown"));
        let model_name = model.clone().unwrap_or_else(|| "vintage".to_string());
        let buffer_frames = buffer_size.unwrap_or(2048) as usize;

        // Start the streaming session
        match rave.start(
            stream_id.clone(),
            model_name.clone(),
            input_identity.clone(),
            output_identity.clone(),
        ) {
            Ok((input_producer, output_consumer)) => {
                // Store the ring buffers for the RT callback to use
                *self.rave_input_producer.lock().unwrap() = Some(input_producer);
                *self.rave_output_consumer.lock().unwrap() = Some(output_consumer);

                let latency_ms = (buffer_frames as u32 * 1000) / self.streaming_tap_sample_rate;

                info!(
                    "RAVE streaming started: stream_id={}, model={}, latency={}ms",
                    stream_id, model_name, latency_ms
                );

                ShellReply::RaveStreamStarted {
                    stream_id,
                    model: model_name,
                    input_identity,
                    output_identity,
                    latency_ms,
                }
            }
            Err(e) => ShellReply::Error {
                error: format!("Failed to start RAVE streaming: {}", e),
                traceback: None,
            },
        }
    }

    /// Stop a RAVE streaming session
    fn stop_rave_streaming(&self, stream_id: String) -> ShellReply {
        let mut rave = self.rave_streaming.lock().unwrap();

        if !rave.is_running() {
            return ShellReply::Error {
                error: "RAVE streaming not running".to_string(),
                traceback: None,
            };
        }

        // Verify stream ID matches
        if let Some(session) = rave.session() {
            if session.stream_id != stream_id {
                return ShellReply::Error {
                    error: format!("Stream ID mismatch: expected {}", session.stream_id),
                    traceback: None,
                };
            }
        }

        match rave.stop() {
            Ok(session) => {
                // Clear the ring buffers (RT callback will stop using them)
                *self.rave_input_producer.lock().unwrap() = None;
                *self.rave_output_consumer.lock().unwrap() = None;

                let duration = session
                    .started_at
                    .elapsed()
                    .map(|d| d.as_secs_f64())
                    .unwrap_or(0.0);

                info!(
                    "RAVE streaming stopped: stream_id={}, duration={:.1}s",
                    stream_id, duration
                );

                ShellReply::RaveStreamStopped {
                    stream_id,
                    duration_seconds: duration,
                }
            }
            Err(e) => ShellReply::Error {
                error: format!("Failed to stop RAVE streaming: {}", e),
                traceback: None,
            },
        }
    }

    /// Get RAVE streaming session status
    fn get_rave_streaming_status(&self, stream_id: String) -> ShellReply {
        let rave = self.rave_streaming.lock().unwrap();

        // Get RT stats from audio output (if attached)
        let (rt_rave_writes, rt_rave_samples_written, rt_rave_reads, rt_rave_samples_read) = {
            let output = self.audio_output.read().unwrap();
            if let Some(ref stream) = *output {
                let stats = stream.stats();
                (
                    stats.rave_writes.load(Ordering::Relaxed),
                    stats.rave_samples_written.load(Ordering::Relaxed),
                    stats.rave_reads.load(Ordering::Relaxed),
                    stats.rave_samples_read.load(Ordering::Relaxed),
                )
            } else {
                (0, 0, 0, 0)
            }
        };

        match rave.session() {
            Some(session) if session.stream_id == stream_id => {
                let stats = rave.stats();
                let frames_processed = stats.samples_processed.load(Ordering::Relaxed);
                let buffer_frames = 2048u32; // Default, should track actual
                let latency_ms = (buffer_frames * 1000) / self.streaming_tap_sample_rate;

                ShellReply::RaveStreamStatus {
                    stream_id,
                    running: session.running,
                    model: session.model_name.clone(),
                    input_identity: session.input_identity.clone(),
                    output_identity: session.output_identity.clone(),
                    frames_processed,
                    latency_ms,
                    rt_rave_writes,
                    rt_rave_samples_written,
                    rt_rave_reads,
                    rt_rave_samples_read,
                }
            }
            Some(session) => ShellReply::Error {
                error: format!("Stream ID mismatch: expected {}", session.stream_id),
                traceback: None,
            },
            None => ShellReply::RaveStreamStatus {
                stream_id,
                running: false,
                model: String::new(),
                input_identity: String::new(),
                output_identity: String::new(),
                frames_processed: 0,
                latency_ms: 0,
                rt_rave_writes,
                rt_rave_samples_written,
                rt_rave_reads,
                rt_rave_samples_read,
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

    fn clear_regions(&self) -> usize {
        let mut regions = self.regions.write().unwrap();
        let count = regions.len();
        regions.clear();
        info!("Cleared {} regions", count);
        count
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
                let region_count = self.regions.read().unwrap().len();
                ShellReply::TransportState {
                    playing: transport.playing,
                    position: IpcBeat(transport.position.0),
                    tempo: current_tempo,
                    region_count,
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
            ShellRequest::ClearRegions => {
                let count = self.clear_regions();
                ShellReply::Ok {
                    result: serde_json::json!({"cleared": count}),
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

            // Monitor input attachment
            ShellRequest::AttachInput { device_name, sample_rate } => {
                match self.attach_input(device_name, sample_rate) {
                    Ok(()) => ShellReply::Ok {
                        result: serde_json::json!({"status": "attached"}),
                    },
                    Err(e) => ShellReply::Error { error: e, traceback: None },
                }
            }
            ShellRequest::DetachInput => {
                self.detach_input();
                ShellReply::Ok {
                    result: serde_json::json!({"status": "detached"}),
                }
            }
            ShellRequest::GetInputStatus => self.get_input_status(),
            ShellRequest::SetMonitor { enabled, gain } => {
                self.set_monitor(enabled, gain);
                ShellReply::MonitorStatus {
                    enabled: self.monitor_channel.enabled.load(Ordering::Relaxed),
                    gain: self.monitor_channel.get_gain(),
                }
            }

            // State snapshot requests for Trustfall query evaluation
            ShellRequest::GetSnapshot => {
                let version = self.snapshot_version.fetch_add(1, Ordering::Relaxed);
                let snapshot = self.build_snapshot(version);
                ShellReply::Snapshot { snapshot }
            }
            ShellRequest::GetGraph => {
                let graph = self.graph.read().unwrap();
                let graph_snapshot = graph.snapshot();
                let nodes: Vec<hooteproto::garden_snapshot::GraphNode> = graph_snapshot
                    .nodes
                    .iter()
                    .map(|n| hooteproto::garden_snapshot::GraphNode {
                        id: n.id.to_string(),
                        name: n.name.clone(),
                        type_id: n.type_id.clone(),
                        inputs: n.inputs.iter().map(|p| hooteproto::garden_snapshot::Port {
                            name: p.name.clone(),
                            signal_type: signal_type_to_snapshot(&p.signal_type),
                        }).collect(),
                        outputs: n.outputs.iter().map(|p| hooteproto::garden_snapshot::Port {
                            name: p.name.clone(),
                            signal_type: signal_type_to_snapshot(&p.signal_type),
                        }).collect(),
                        latency_samples: n.latency_samples as u32,
                        can_realtime: n.capabilities.realtime,
                        can_offline: n.capabilities.offline,
                    })
                    .collect();
                let edges: Vec<hooteproto::garden_snapshot::GraphEdge> = graph_snapshot
                    .edges
                    .iter()
                    .map(|e| hooteproto::garden_snapshot::GraphEdge {
                        source_id: e.source_id.to_string(),
                        source_port: e.source_port.clone(),
                        dest_id: e.dest_id.to_string(),
                        dest_port: e.dest_port.clone(),
                    })
                    .collect();
                ShellReply::GraphSnapshot { nodes, edges }
            }
            ShellRequest::GetIOState => {
                let outputs = self.build_audio_output_snapshot();
                let inputs = self.build_audio_input_snapshot();
                ShellReply::IOState {
                    outputs,
                    inputs,
                    midi_devices: vec![], // TODO: Add MIDI device tracking
                }
            }

            // Audio streaming snapshot
            ShellRequest::GetAudioSnapshot { frames } => self.get_audio_snapshot(frames),

            // RAVE streaming
            ShellRequest::RaveStreamStart { model, input_identity, output_identity, buffer_size } => {
                self.start_rave_streaming(model, input_identity, output_identity, buffer_size)
            }
            ShellRequest::RaveStreamStop { stream_id } => {
                self.stop_rave_streaming(stream_id)
            }
            ShellRequest::RaveStreamStatus { stream_id } => {
                self.get_rave_streaming_status(stream_id)
            }

            // MIDI I/O (direct ALSA)
            ShellRequest::ListMidiPorts => {
                self.list_midi_ports()
            }
            ShellRequest::AttachMidiInput { port_pattern } => {
                self.attach_midi_input(&port_pattern)
            }
            ShellRequest::AttachMidiOutput { port_pattern } => {
                self.attach_midi_output(&port_pattern)
            }
            ShellRequest::DetachMidiInput { port_pattern } => {
                self.detach_midi_input(&port_pattern)
            }
            ShellRequest::DetachMidiOutput { port_pattern } => {
                self.detach_midi_output(&port_pattern)
            }
            ShellRequest::SendMidi { port_pattern, message } => {
                self.send_midi(port_pattern.as_deref(), &message)
            }
            ShellRequest::GetMidiStatus => {
                self.get_midi_status()
            }

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
        crate::ipc::Behavior::PlayContent { content_hash } => Behavior::PlayContent {
            content_hash: content_hash.clone(),
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

/// Convert chaosgarden SignalType to snapshot SignalType
fn signal_type_to_snapshot(signal: &crate::primitives::SignalType) -> hooteproto::garden_snapshot::SignalType {
    use crate::primitives::SignalType;
    match signal {
        SignalType::Audio => hooteproto::garden_snapshot::SignalType::Audio,
        SignalType::Midi => hooteproto::garden_snapshot::SignalType::Midi,
        SignalType::Control => hooteproto::garden_snapshot::SignalType::Control,
        SignalType::Trigger => hooteproto::garden_snapshot::SignalType::Trigger,
    }
}

/// Convert chaosgarden ContentType to snapshot MediaType
fn content_type_to_snapshot(content_type: &crate::primitives::ContentType) -> hooteproto::garden_snapshot::MediaType {
    use crate::primitives::ContentType;
    match content_type {
        ContentType::Audio => hooteproto::garden_snapshot::MediaType::Audio,
        ContentType::Midi => hooteproto::garden_snapshot::MediaType::Midi,
    }
}

/// Convert chaosgarden LatentStatus to snapshot LatentStatus
fn latent_status_to_snapshot(status: &crate::primitives::LatentStatus) -> hooteproto::garden_snapshot::LatentStatus {
    use crate::primitives::LatentStatus;
    match status {
        LatentStatus::Pending => hooteproto::garden_snapshot::LatentStatus::Pending,
        LatentStatus::Running => hooteproto::garden_snapshot::LatentStatus::Running,
        LatentStatus::Resolved => hooteproto::garden_snapshot::LatentStatus::Resolved,
        LatentStatus::Approved => hooteproto::garden_snapshot::LatentStatus::Approved,
        LatentStatus::Rejected => hooteproto::garden_snapshot::LatentStatus::Rejected,
        LatentStatus::Failed => hooteproto::garden_snapshot::LatentStatus::Failed,
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
            ShellReply::TransportState { playing, position, tempo, region_count } => {
                assert!(!playing);
                assert_eq!(position.0, 0.0);
                assert_eq!(tempo, 120.0);
                assert_eq!(region_count, 0);
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
            content_hash: "hash_abc123".to_string(),
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
            content_hash: "hash_123".to_string(),
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
            content_hash: "hash_456".to_string(),
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
                content_hash: format!("hash_{}", pos as i32),
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
            content_hash: "hash_handler".to_string(),
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

    // === Playback Engine Wiring Tests (Phase 3) ===

    #[test]
    fn test_set_content_resolver_initializes_engine() {
        use crate::nodes::MemoryResolver;

        let mut daemon = GardenDaemon::new();

        // Before setting resolver, no engine
        assert!(daemon.playback_engine.read().unwrap().is_none());
        assert!(daemon.compiled_graph.read().unwrap().is_none());

        // Set resolver
        let resolver = Arc::new(MemoryResolver::new());
        daemon.set_content_resolver(resolver);

        // Now engine and graph should exist
        assert!(daemon.playback_engine.read().unwrap().is_some());
        assert!(daemon.compiled_graph.read().unwrap().is_some());
    }

    #[test]
    fn test_play_syncs_playback_engine() {
        use crate::nodes::MemoryResolver;

        let mut daemon = GardenDaemon::new();
        let resolver = Arc::new(MemoryResolver::new());
        daemon.set_content_resolver(resolver);

        // Initially not playing
        {
            let engine = daemon.playback_engine.read().unwrap();
            assert!(!engine.as_ref().unwrap().is_playing());
        }

        // Play
        daemon.play();

        // Engine should be playing
        {
            let engine = daemon.playback_engine.read().unwrap();
            assert!(engine.as_ref().unwrap().is_playing());
        }

        // Pause
        daemon.pause();

        // Engine should be paused
        {
            let engine = daemon.playback_engine.read().unwrap();
            assert!(!engine.as_ref().unwrap().is_playing());
        }
    }

    #[test]
    fn test_seek_syncs_playback_engine() {
        use crate::nodes::MemoryResolver;

        let mut daemon = GardenDaemon::new();
        let resolver = Arc::new(MemoryResolver::new());
        daemon.set_content_resolver(resolver);

        // Seek to beat 8
        daemon.seek(Beat(8.0));

        // Engine position should be at beat 8
        {
            let engine = daemon.playback_engine.read().unwrap();
            let pos = engine.as_ref().unwrap().position();
            assert_eq!(pos.beats.0, 8.0);
        }

        // Seek again
        daemon.seek(Beat(16.0));

        {
            let engine = daemon.playback_engine.read().unwrap();
            let pos = engine.as_ref().unwrap().position();
            assert_eq!(pos.beats.0, 16.0);
        }
    }

    #[test]
    fn test_stop_resets_playback_engine() {
        use crate::nodes::MemoryResolver;

        let mut daemon = GardenDaemon::new();
        let resolver = Arc::new(MemoryResolver::new());
        daemon.set_content_resolver(resolver);

        // Seek and play
        daemon.seek(Beat(10.0));
        daemon.play();

        // Stop
        daemon.stop();

        // Engine should be stopped and at position 0
        {
            let engine = daemon.playback_engine.read().unwrap();
            let engine_ref = engine.as_ref().unwrap();
            assert!(!engine_ref.is_playing());
            assert_eq!(engine_ref.position().beats.0, 0.0);
        }
    }
}
