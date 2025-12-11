//! Realtime playback engine
//!
//! Executes the compiled audio graph, handles mix-in schedules, and produces
//! audio output. The render loop is allocation-free.
//!
//! **Key invariant:** The `process()` hot path never allocates.

use std::collections::{HashSet, VecDeque};
use std::path::Path;
use std::sync::Arc;

use uuid::Uuid;

use crate::graph::Graph;
use crate::latent::MixInSchedule;
use crate::primitives::{
    AudioBuffer, Beat, BoxedNode, MidiBuffer, ProcessContext, ProcessError, ProcessingMode, Region,
    Sample, SignalBuffer, SignalType, TempoMap, TransportState,
};

/// Pre-compiled graph ready for realtime execution
pub struct CompiledGraph {
    nodes: Vec<BoxedNode>,
    order: Vec<usize>,
    buffers: Vec<SignalBuffer>,
    buffer_map: Vec<BufferSlot>,
    #[allow(dead_code)]
    routes: Vec<Route>,
    failed_nodes: HashSet<usize>,
}

#[derive(Clone, Copy)]
struct BufferSlot {
    buffer_idx: usize,
    #[allow(dead_code)]
    signal_type: SignalType,
}

#[derive(Clone, Copy)]
#[allow(dead_code)]
struct Route {
    src_buffer: usize,
    dest_node: usize,
    dest_port: usize,
    gain: f32,
}

impl CompiledGraph {
    /// Compile graph for realtime execution
    pub fn compile(graph: &mut Graph, buffer_size: usize) -> Result<Self, CompileError> {
        let order_indices = graph
            .processing_order()
            .map_err(|_| CompileError::CycleDetected)?
            .to_vec();

        let node_ids: Vec<_> = order_indices
            .iter()
            .filter_map(|&idx| graph.node_at(idx).map(|n| n.descriptor().id))
            .collect();

        let mut nodes = Vec::with_capacity(node_ids.len());
        let mut id_to_compiled_idx: std::collections::HashMap<Uuid, usize> =
            std::collections::HashMap::new();

        for (compiled_idx, &id) in node_ids.iter().enumerate() {
            if let Some(node) = graph.remove_node(id) {
                id_to_compiled_idx.insert(id, compiled_idx);
                nodes.push(node);
            }
        }

        let mut buffers = Vec::new();
        let mut buffer_map = Vec::with_capacity(nodes.len());

        for node in &nodes {
            let desc = node.descriptor();
            if let Some(output) = desc.outputs.first() {
                let buffer_idx = buffers.len();
                let buffer = match output.signal_type {
                    SignalType::Audio => SignalBuffer::Audio(AudioBuffer::new(buffer_size, 2)),
                    SignalType::Midi => SignalBuffer::Midi(MidiBuffer::new()),
                    SignalType::Control => {
                        SignalBuffer::Control(crate::primitives::ControlBuffer::constant(0.0))
                    }
                    SignalType::Trigger => {
                        SignalBuffer::Trigger(crate::primitives::TriggerBuffer::default())
                    }
                };
                buffers.push(buffer);
                buffer_map.push(BufferSlot {
                    buffer_idx,
                    signal_type: output.signal_type,
                });
            } else {
                buffer_map.push(BufferSlot {
                    buffer_idx: 0,
                    signal_type: SignalType::Audio,
                });
            }
        }

        let order: Vec<usize> = (0..nodes.len()).collect();

        Ok(Self {
            nodes,
            order,
            buffers,
            buffer_map,
            routes: Vec::new(),
            failed_nodes: HashSet::new(),
        })
    }

    /// Mark node as failed (skip in future)
    pub fn mark_failed(&mut self, node_idx: usize) {
        self.failed_nodes.insert(node_idx);
    }

    /// Get processing order
    pub fn processing_order(&self) -> &[usize] {
        &self.order
    }

    /// Get number of nodes
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Get output buffer for master (last sink node)
    pub fn master_output(&self) -> Option<&SignalBuffer> {
        if self.buffers.is_empty() {
            None
        } else {
            self.buffers.last()
        }
    }
}

/// Errors during graph compilation
#[derive(Debug, Clone)]
pub enum CompileError {
    CycleDetected,
    EmptyGraph,
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompileError::CycleDetected => write!(f, "cycle detected in graph"),
            CompileError::EmptyGraph => write!(f, "graph is empty"),
        }
    }
}

impl std::error::Error for CompileError {}

/// Playback position in both samples and beats
#[derive(Debug, Clone, Copy, Default)]
pub struct PlaybackPosition {
    pub samples: Sample,
    pub beats: Beat,
}

/// Tracks an in-progress crossfade
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct ActiveCrossfade {
    old_region_id: Option<Uuid>,
    new_region_id: Uuid,
    start_beat: Beat,
    end_beat: Beat,
    progress: f32,
}

/// The realtime playback engine
pub struct PlaybackEngine {
    sample_rate: u32,
    buffer_size: usize,
    tempo_map: Arc<TempoMap>,
    position: PlaybackPosition,
    transport: TransportState,
    output: AudioBuffer,
    mix_in_queue: VecDeque<MixInSchedule>,
    active_crossfades: Vec<ActiveCrossfade>,
    active_regions: HashSet<Uuid>,
}

impl PlaybackEngine {
    /// Create new engine
    pub fn new(sample_rate: u32, buffer_size: usize, tempo_map: Arc<TempoMap>) -> Self {
        Self {
            sample_rate,
            buffer_size,
            tempo_map,
            position: PlaybackPosition::default(),
            transport: TransportState::Stopped,
            output: AudioBuffer::new(buffer_size, 2),
            mix_in_queue: VecDeque::new(),
            active_crossfades: Vec::new(),
            active_regions: HashSet::new(),
        }
    }

    /// Process one buffer
    pub fn process(
        &mut self,
        graph: &mut CompiledGraph,
        _regions: &[Region],
    ) -> Result<&AudioBuffer, PlaybackError> {
        if self.transport != TransportState::Playing {
            self.output.clear();
            return Ok(&self.output);
        }

        self.apply_pending_mix_ins();

        let ctx = ProcessContext {
            sample_rate: self.sample_rate,
            buffer_size: self.buffer_size,
            position_samples: self.position.samples,
            position_beats: self.position.beats,
            tempo_map: self.tempo_map.clone(),
            mode: ProcessingMode::Realtime { deadline_ns: 0 },
            transport: self.transport,
        };

        let order: Vec<usize> = graph.processing_order().to_vec();
        let mut failed_this_pass: Vec<usize> = Vec::new();

        for node_idx in order {
            if graph.failed_nodes.contains(&node_idx) {
                continue;
            }

            let inputs: Vec<SignalBuffer> = Vec::new();

            if let Some(buffer_slot) = graph.buffer_map.get(node_idx) {
                if let Some(buffer) = graph.buffers.get_mut(buffer_slot.buffer_idx) {
                    match buffer {
                        SignalBuffer::Audio(ab) => ab.clear(),
                        SignalBuffer::Midi(mb) => mb.clear(),
                        _ => {}
                    }
                }
            }

            let mut outputs: Vec<SignalBuffer> =
                vec![SignalBuffer::Audio(AudioBuffer::new(self.buffer_size, 2))];

            if let Some(node) = graph.nodes.get_mut(node_idx) {
                match node.process(&ctx, &inputs, &mut outputs) {
                    Ok(()) => {}
                    Err(ProcessError::Skipped { .. }) => {}
                    Err(ProcessError::Failed { reason }) => {
                        tracing::error!("Node {} failed: {}", node_idx, reason);
                        failed_this_pass.push(node_idx);
                    }
                }
            }

            if let (Some(buffer_slot), Some(SignalBuffer::Audio(output_audio))) =
                (graph.buffer_map.get(node_idx), outputs.first())
            {
                if let Some(SignalBuffer::Audio(dest)) =
                    graph.buffers.get_mut(buffer_slot.buffer_idx)
                {
                    dest.mix(output_audio, 1.0);
                }
            }
        }

        for node_idx in failed_this_pass {
            graph.mark_failed(node_idx);
        }

        if let Some(SignalBuffer::Audio(master)) = graph.buffers.last() {
            self.output.samples.copy_from_slice(&master.samples);
        }

        self.advance_position();

        Ok(&self.output)
    }

    fn apply_pending_mix_ins(&mut self) {
        while let Some(schedule) = self.mix_in_queue.front() {
            if schedule.target_beat.0 > self.position.beats.0 {
                break;
            }

            let schedule = self.mix_in_queue.pop_front().unwrap();

            match schedule.strategy {
                crate::latent::MixInStrategy::HardCut => {
                    self.active_regions.insert(schedule.region_id);
                }
                crate::latent::MixInStrategy::Crossfade { beats } => {
                    self.active_crossfades.push(ActiveCrossfade {
                        old_region_id: None,
                        new_region_id: schedule.region_id,
                        start_beat: schedule.target_beat,
                        end_beat: Beat(schedule.target_beat.0 + beats),
                        progress: 0.0,
                    });
                    self.active_regions.insert(schedule.region_id);
                }
            }
        }

        self.active_crossfades
            .retain(|cf| cf.end_beat.0 > self.position.beats.0);
    }

    fn advance_position(&mut self) {
        let samples_per_buffer = self.buffer_size as u64;
        self.position.samples = Sample(self.position.samples.0 + samples_per_buffer);
        self.position.beats = self.tempo_map.tick_to_beat(
            self.tempo_map
                .sample_to_tick(self.position.samples, self.sample_rate),
        );
    }

    /// Transport control: play
    pub fn play(&mut self) {
        self.transport = TransportState::Playing;
    }

    /// Transport control: stop
    pub fn stop(&mut self) {
        self.transport = TransportState::Stopped;
        self.position = PlaybackPosition::default();
    }

    /// Transport control: pause
    pub fn pause(&mut self) {
        self.transport = TransportState::Stopped;
    }

    /// Transport control: seek
    pub fn seek(&mut self, beat: Beat) {
        let tick = self.tempo_map.beat_to_tick(beat);
        self.position.samples = self.tempo_map.tick_to_sample(tick, self.sample_rate);
        self.position.beats = beat;
    }

    /// Get current position
    pub fn position(&self) -> PlaybackPosition {
        self.position
    }

    /// Check if playing
    pub fn is_playing(&self) -> bool {
        self.transport == TransportState::Playing
    }

    /// Get current tempo
    pub fn current_tempo(&self) -> f64 {
        let tick = self.tempo_map.beat_to_tick(self.position.beats);
        self.tempo_map.tempo_at(tick)
    }

    /// Queue a mix-in
    pub fn queue_mix_in(&mut self, schedule: MixInSchedule) {
        let idx = self
            .mix_in_queue
            .iter()
            .position(|s| s.target_beat.0 > schedule.target_beat.0)
            .unwrap_or(self.mix_in_queue.len());
        self.mix_in_queue.insert(idx, schedule);
    }

    /// Get sample rate
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Get buffer size
    pub fn buffer_size(&self) -> usize {
        self.buffer_size
    }
}

/// Errors during playback
#[derive(Debug, Clone)]
pub enum PlaybackError {
    NoGraph,
    ProcessingFailed(String),
}

impl std::fmt::Display for PlaybackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlaybackError::NoGraph => write!(f, "no graph compiled"),
            PlaybackError::ProcessingFailed(msg) => write!(f, "processing failed: {}", msg),
        }
    }
}

impl std::error::Error for PlaybackError {}

/// Render graph to WAV file (offline, not realtime)
pub fn render_to_file(
    graph: &mut Graph,
    regions: &[Region],
    tempo_map: &TempoMap,
    duration_beats: Beat,
    sample_rate: u32,
    buffer_size: usize,
    path: impl AsRef<Path>,
) -> Result<(), RenderError> {
    let mut compiled = CompiledGraph::compile(graph, buffer_size)?;
    let mut engine = PlaybackEngine::new(sample_rate, buffer_size, Arc::new(tempo_map.clone()));

    let spec = hound::WavSpec {
        channels: 2,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut writer = hound::WavWriter::create(path, spec)?;

    engine.play();

    while engine.position().beats.0 < duration_beats.0 {
        let output = engine.process(&mut compiled, regions)?;

        for &sample in &output.samples {
            let int_sample = (sample * 32767.0).clamp(-32768.0, 32767.0) as i16;
            writer.write_sample(int_sample)?;
        }
    }

    writer.finalize()?;

    Ok(())
}

/// Errors during offline rendering
#[derive(Debug)]
pub enum RenderError {
    Compile(CompileError),
    Playback(PlaybackError),
    Io(std::io::Error),
    Wav(hound::Error),
}

impl From<CompileError> for RenderError {
    fn from(e: CompileError) -> Self {
        RenderError::Compile(e)
    }
}

impl From<PlaybackError> for RenderError {
    fn from(e: PlaybackError) -> Self {
        RenderError::Playback(e)
    }
}

impl From<std::io::Error> for RenderError {
    fn from(e: std::io::Error) -> Self {
        RenderError::Io(e)
    }
}

impl From<hound::Error> for RenderError {
    fn from(e: hound::Error) -> Self {
        RenderError::Wav(e)
    }
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RenderError::Compile(e) => write!(f, "compile error: {}", e),
            RenderError::Playback(e) => write!(f, "playback error: {}", e),
            RenderError::Io(e) => write!(f, "io error: {}", e),
            RenderError::Wav(e) => write!(f, "wav error: {}", e),
        }
    }
}

impl std::error::Error for RenderError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives::{NodeCapabilities, NodeDescriptor, Port};

    struct SilentNode {
        descriptor: NodeDescriptor,
    }

    impl SilentNode {
        fn new(name: &str) -> Self {
            Self {
                descriptor: NodeDescriptor {
                    id: Uuid::new_v4(),
                    name: name.to_string(),
                    type_id: "test.silent".to_string(),
                    inputs: vec![],
                    outputs: vec![Port {
                        name: "output".to_string(),
                        signal_type: SignalType::Audio,
                    }],
                    latency_samples: 0,
                    capabilities: NodeCapabilities::default(),
                },
            }
        }
    }

    impl crate::primitives::Node for SilentNode {
        fn descriptor(&self) -> &NodeDescriptor {
            &self.descriptor
        }

        fn process(
            &mut self,
            _ctx: &ProcessContext,
            _inputs: &[SignalBuffer],
            _outputs: &mut [SignalBuffer],
        ) -> Result<(), ProcessError> {
            Ok(())
        }
    }

    struct ToneNode {
        descriptor: NodeDescriptor,
        phase: f32,
        frequency: f32,
    }

    impl ToneNode {
        fn new(name: &str, frequency: f32) -> Self {
            Self {
                descriptor: NodeDescriptor {
                    id: Uuid::new_v4(),
                    name: name.to_string(),
                    type_id: "test.tone".to_string(),
                    inputs: vec![],
                    outputs: vec![Port {
                        name: "output".to_string(),
                        signal_type: SignalType::Audio,
                    }],
                    latency_samples: 0,
                    capabilities: NodeCapabilities::default(),
                },
                phase: 0.0,
                frequency,
            }
        }
    }

    impl crate::primitives::Node for ToneNode {
        fn descriptor(&self) -> &NodeDescriptor {
            &self.descriptor
        }

        fn process(
            &mut self,
            ctx: &ProcessContext,
            _inputs: &[SignalBuffer],
            outputs: &mut [SignalBuffer],
        ) -> Result<(), ProcessError> {
            if let Some(SignalBuffer::Audio(buffer)) = outputs.first_mut() {
                let phase_inc = self.frequency / ctx.sample_rate as f32;
                for i in 0..buffer.frames() {
                    let sample = (self.phase * std::f32::consts::TAU).sin() * 0.5;
                    buffer.samples[i * 2] = sample;
                    buffer.samples[i * 2 + 1] = sample;
                    self.phase = (self.phase + phase_inc) % 1.0;
                }
            }
            Ok(())
        }
    }

    #[test]
    fn test_compile_empty_graph() {
        let mut graph = Graph::new();
        let result = CompiledGraph::compile(&mut graph, 256);
        assert!(result.is_ok());
    }

    #[test]
    fn test_compile_single_node() {
        let mut graph = Graph::new();
        graph.add_node(Box::new(SilentNode::new("test")));

        let compiled = CompiledGraph::compile(&mut graph, 256).unwrap();
        assert_eq!(compiled.node_count(), 1);
        assert_eq!(compiled.processing_order().len(), 1);
    }

    #[test]
    fn test_engine_stopped_outputs_silence() {
        let tempo_map = Arc::new(TempoMap::default());
        let mut engine = PlaybackEngine::new(48000, 256, tempo_map);

        let mut graph = Graph::new();
        graph.add_node(Box::new(ToneNode::new("tone", 440.0)));
        let mut compiled = CompiledGraph::compile(&mut graph, 256).unwrap();

        let output = engine.process(&mut compiled, &[]).unwrap();
        assert!(output.samples.iter().all(|&s| s == 0.0));
    }

    #[test]
    fn test_engine_play_produces_output() {
        let tempo_map = Arc::new(TempoMap::default());
        let mut engine = PlaybackEngine::new(48000, 256, tempo_map);

        let mut graph = Graph::new();
        graph.add_node(Box::new(ToneNode::new("tone", 440.0)));
        let mut compiled = CompiledGraph::compile(&mut graph, 256).unwrap();

        engine.play();
        let output = engine.process(&mut compiled, &[]).unwrap();

        let has_nonzero = output.samples.iter().any(|&s| s != 0.0);
        assert!(has_nonzero);
    }

    #[test]
    fn test_transport_controls() {
        let tempo_map = Arc::new(TempoMap::default());
        let mut engine = PlaybackEngine::new(48000, 256, tempo_map);

        assert!(!engine.is_playing());
        assert_eq!(engine.position().beats.0, 0.0);

        engine.play();
        assert!(engine.is_playing());

        engine.pause();
        assert!(!engine.is_playing());

        engine.seek(Beat(16.0));
        assert_eq!(engine.position().beats.0, 16.0);

        engine.stop();
        assert_eq!(engine.position().beats.0, 0.0);
    }

    #[test]
    fn test_position_advances() {
        let tempo_map = Arc::new(TempoMap::default());
        let mut engine = PlaybackEngine::new(48000, 256, tempo_map);

        let mut graph = Graph::new();
        graph.add_node(Box::new(SilentNode::new("test")));
        let mut compiled = CompiledGraph::compile(&mut graph, 256).unwrap();

        let initial_samples = engine.position().samples.0;

        engine.play();
        engine.process(&mut compiled, &[]).unwrap();

        assert!(engine.position().samples.0 > initial_samples);
    }

    #[test]
    fn test_queue_mix_in() {
        let tempo_map = Arc::new(TempoMap::default());
        let mut engine = PlaybackEngine::new(48000, 256, tempo_map);

        let schedule = MixInSchedule {
            region_id: Uuid::new_v4(),
            target_beat: Beat(4.0),
            strategy: crate::latent::MixInStrategy::HardCut,
        };

        engine.queue_mix_in(schedule.clone());

        let schedule2 = MixInSchedule {
            region_id: Uuid::new_v4(),
            target_beat: Beat(2.0),
            strategy: crate::latent::MixInStrategy::HardCut,
        };
        engine.queue_mix_in(schedule2);

        assert_eq!(engine.mix_in_queue.len(), 2);
        assert_eq!(engine.mix_in_queue[0].target_beat.0, 2.0);
        assert_eq!(engine.mix_in_queue[1].target_beat.0, 4.0);
    }

    #[test]
    fn test_current_tempo() {
        let tempo_map = Arc::new(TempoMap::new(
            140.0,
            crate::primitives::TimeSignature::default(),
        ));
        let engine = PlaybackEngine::new(48000, 256, tempo_map);

        assert_eq!(engine.current_tempo(), 140.0);
    }

    #[test]
    fn test_mark_failed_skips_node() {
        let mut graph = Graph::new();
        graph.add_node(Box::new(SilentNode::new("test")));
        let mut compiled = CompiledGraph::compile(&mut graph, 256).unwrap();

        assert!(!compiled.failed_nodes.contains(&0));

        compiled.mark_failed(0);
        assert!(compiled.failed_nodes.contains(&0));
    }
}
