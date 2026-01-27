//! Core primitives for chaosgarden
//!
//! Time, Signal, Node, and Lifecycle types that form the foundation
//! of the audio graph and timeline.

use std::ops::{Add, Sub};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

mod region;
pub use region::{
    Behavior, ContentType, CurvePoint, CurveType, LatentState, LatentStatus, PlaybackParams,
    Region, RegionMetadata, ResolvedContent, TriggerKind,
};

// =============================================================================
// TIME TYPES
// =============================================================================

/// Pulses per quarter note - standard MIDI resolution
pub const DEFAULT_PPQ: u16 = 960;

/// A tick is the smallest unit of musical time (1/PPQ of a quarter note)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Tick(pub u64);

impl Tick {
    pub fn zero() -> Self {
        Self(0)
    }
}

impl Add for Tick {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        Tick(self.0 + rhs.0)
    }
}

impl Sub for Tick {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self::Output {
        Tick(self.0.saturating_sub(rhs.0))
    }
}

/// Musical time in beats (quarter notes)
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize, Default)]
pub struct Beat(pub f64);

impl Beat {
    pub fn zero() -> Self {
        Self(0.0)
    }
}

impl Add for Beat {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        Beat(self.0 + rhs.0)
    }
}

impl Sub for Beat {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self::Output {
        Beat((self.0 - rhs.0).max(0.0))
    }
}

/// Physical time in seconds
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize, Default)]
pub struct Second(pub f64);

impl Second {
    pub fn zero() -> Self {
        Self(0.0)
    }
}

impl Add for Second {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        Second(self.0 + rhs.0)
    }
}

impl Sub for Second {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self::Output {
        Second((self.0 - rhs.0).max(0.0))
    }
}

/// Audio sample position
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
pub struct Sample(pub u64);

impl Sample {
    pub fn zero() -> Self {
        Self(0)
    }
}

impl Add for Sample {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        Sample(self.0 + rhs.0)
    }
}

impl Sub for Sample {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self::Output {
        Sample(self.0.saturating_sub(rhs.0))
    }
}

/// Time signature (e.g., 4/4, 3/4, 6/8)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeSignature {
    pub numerator: u8,
    pub denominator: u8,
}

impl Default for TimeSignature {
    fn default() -> Self {
        Self {
            numerator: 4,
            denominator: 4,
        }
    }
}

/// A tempo change at a specific tick
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TempoChange {
    pub tick: Tick,
    pub bpm: f64,
}

/// A time signature change at a specific tick
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeSignatureChange {
    pub tick: Tick,
    pub time_sig: TimeSignature,
}

/// Tempo map for converting between time representations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TempoMap {
    pub ppq: u16,
    pub tempo_changes: Vec<TempoChange>,
    pub time_sig_changes: Vec<TimeSignatureChange>,
}

impl TempoMap {
    /// Create a new tempo map with initial tempo and time signature
    pub fn new(bpm: f64, time_sig: TimeSignature) -> Self {
        Self {
            ppq: DEFAULT_PPQ,
            tempo_changes: vec![TempoChange {
                tick: Tick::zero(),
                bpm,
            }],
            time_sig_changes: vec![TimeSignatureChange {
                tick: Tick::zero(),
                time_sig,
            }],
        }
    }

    /// Set the base tempo (at tick 0)
    /// This replaces the initial tempo, leaving any later tempo changes intact
    pub fn set_base_tempo(&mut self, bpm: f64) {
        if let Some(first) = self.tempo_changes.first_mut() {
            if first.tick == Tick::zero() {
                first.bpm = bpm;
                return;
            }
        }
        self.tempo_changes.insert(0, TempoChange { tick: Tick::zero(), bpm });
    }

    /// Get tempo at a given tick
    pub fn tempo_at(&self, tick: Tick) -> f64 {
        self.tempo_changes
            .iter()
            .rev()
            .find(|tc| tc.tick <= tick)
            .map(|tc| tc.bpm)
            .unwrap_or(120.0)
    }

    /// Get time signature at a given tick
    pub fn time_sig_at(&self, tick: Tick) -> TimeSignature {
        self.time_sig_changes
            .iter()
            .rev()
            .find(|tsc| tsc.tick <= tick)
            .map(|tsc| tsc.time_sig)
            .unwrap_or_default()
    }

    /// Convert tick to beat
    pub fn tick_to_beat(&self, tick: Tick) -> Beat {
        Beat(tick.0 as f64 / self.ppq as f64)
    }

    /// Convert beat to tick
    pub fn beat_to_tick(&self, beat: Beat) -> Tick {
        Tick((beat.0 * self.ppq as f64).round() as u64)
    }

    /// Convert tick to seconds, accounting for tempo changes
    pub fn tick_to_second(&self, tick: Tick) -> Second {
        let mut seconds = 0.0;
        let mut current_tick = 0u64;
        let mut current_tempo = self.tempo_at(Tick::zero());

        for change in &self.tempo_changes {
            if change.tick.0 >= tick.0 {
                break;
            }
            if change.tick.0 > current_tick {
                let delta_ticks = change.tick.0 - current_tick;
                let delta_beats = delta_ticks as f64 / self.ppq as f64;
                let delta_seconds = delta_beats * 60.0 / current_tempo;
                seconds += delta_seconds;
                current_tick = change.tick.0;
            }
            current_tempo = change.bpm;
        }

        let remaining_ticks = tick.0 - current_tick;
        let remaining_beats = remaining_ticks as f64 / self.ppq as f64;
        let remaining_seconds = remaining_beats * 60.0 / current_tempo;
        seconds += remaining_seconds;

        Second(seconds)
    }

    /// Convert seconds to tick, accounting for tempo changes
    pub fn second_to_tick(&self, second: Second) -> Tick {
        let mut remaining_seconds = second.0;
        let mut current_tick = 0u64;
        let mut current_tempo = self.tempo_at(Tick::zero());

        for (i, change) in self.tempo_changes.iter().enumerate() {
            if i == 0 {
                continue;
            }
            let delta_ticks = change.tick.0 - current_tick;
            let delta_beats = delta_ticks as f64 / self.ppq as f64;
            let delta_seconds = delta_beats * 60.0 / current_tempo;

            if remaining_seconds <= delta_seconds {
                break;
            }

            remaining_seconds -= delta_seconds;
            current_tick = change.tick.0;
            current_tempo = change.bpm;
        }

        let remaining_beats = remaining_seconds * current_tempo / 60.0;
        let remaining_ticks = (remaining_beats * self.ppq as f64).round() as u64;

        Tick(current_tick + remaining_ticks)
    }

    /// Convert tick to sample position
    pub fn tick_to_sample(&self, tick: Tick, sample_rate: u32) -> Sample {
        let seconds = self.tick_to_second(tick);
        Sample((seconds.0 * sample_rate as f64).round() as u64)
    }

    /// Convert sample position to tick
    pub fn sample_to_tick(&self, sample: Sample, sample_rate: u32) -> Tick {
        let seconds = Second(sample.0 as f64 / sample_rate as f64);
        self.second_to_tick(seconds)
    }

    /// Add a tempo change
    pub fn add_tempo_change(&mut self, tick: Tick, bpm: f64) {
        self.tempo_changes.push(TempoChange { tick, bpm });
        self.tempo_changes.sort_by_key(|tc| tc.tick);
    }
}

impl Default for TempoMap {
    fn default() -> Self {
        Self::new(120.0, TimeSignature::default())
    }
}

// =============================================================================
// SIGNAL TYPES
// =============================================================================

/// Type of signal flowing through the graph
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SignalType {
    Audio,
    Midi,
    Control,
    Trigger,
}

/// Audio buffer with interleaved samples
#[derive(Debug, Clone)]
pub struct AudioBuffer {
    pub samples: Vec<f32>,
    pub channels: u8,
}

impl AudioBuffer {
    pub fn new(frames: usize, channels: u8) -> Self {
        Self {
            samples: vec![0.0; frames * channels as usize],
            channels,
        }
    }

    pub fn frames(&self) -> usize {
        if self.channels == 0 {
            0
        } else {
            self.samples.len() / self.channels as usize
        }
    }

    pub fn mix(&mut self, other: &AudioBuffer, gain: f32) {
        if self.samples.len() == other.samples.len() {
            for (s, o) in self.samples.iter_mut().zip(other.samples.iter()) {
                *s += o * gain;
            }
        }
    }

    pub fn clear(&mut self) {
        self.samples.fill(0.0);
    }
}

/// MIDI event at a specific frame
#[derive(Debug, Clone)]
pub struct MidiEvent {
    pub frame: usize,
    pub message: MidiMessage,
}

/// MIDI message types
#[derive(Debug, Clone)]
pub enum MidiMessage {
    NoteOn {
        channel: u8,
        pitch: u8,
        velocity: u8,
    },
    NoteOff {
        channel: u8,
        pitch: u8,
    },
    ControlChange {
        channel: u8,
        controller: u8,
        value: u8,
    },
    ProgramChange {
        channel: u8,
        program: u8,
    },
    PitchBend {
        channel: u8,
        value: i16,
    },
    /// MIDI Start (0xFA)
    Start,
    /// MIDI Stop (0xFC)
    Stop,
    /// MIDI Continue (0xFB)
    Continue,
    /// MIDI Timing Clock (0xF8)
    TimingClock,
}

/// Convert from garden MidiMessageSpec to internal MidiMessage
///
/// Note: Raw variant is not convertible (must be handled separately)
impl TryFrom<&hooteproto::garden::MidiMessageSpec> for MidiMessage {
    type Error = &'static str;

    fn try_from(spec: &hooteproto::garden::MidiMessageSpec) -> Result<Self, Self::Error> {
        match spec {
            hooteproto::garden::MidiMessageSpec::NoteOn { channel, pitch, velocity } => {
                Ok(MidiMessage::NoteOn { channel: *channel, pitch: *pitch, velocity: *velocity })
            }
            hooteproto::garden::MidiMessageSpec::NoteOff { channel, pitch } => {
                Ok(MidiMessage::NoteOff { channel: *channel, pitch: *pitch })
            }
            hooteproto::garden::MidiMessageSpec::ControlChange { channel, controller, value } => {
                Ok(MidiMessage::ControlChange { channel: *channel, controller: *controller, value: *value })
            }
            hooteproto::garden::MidiMessageSpec::ProgramChange { channel, program } => {
                Ok(MidiMessage::ProgramChange { channel: *channel, program: *program })
            }
            hooteproto::garden::MidiMessageSpec::PitchBend { channel, value } => {
                Ok(MidiMessage::PitchBend { channel: *channel, value: *value })
            }
            hooteproto::garden::MidiMessageSpec::Raw { .. } => {
                Err("Raw MIDI messages cannot be converted to MidiMessage")
            }
            hooteproto::garden::MidiMessageSpec::Start => Ok(MidiMessage::Start),
            hooteproto::garden::MidiMessageSpec::Stop => Ok(MidiMessage::Stop),
            hooteproto::garden::MidiMessageSpec::Continue => Ok(MidiMessage::Continue),
            hooteproto::garden::MidiMessageSpec::TimingClock => Ok(MidiMessage::TimingClock),
        }
    }
}

/// Buffer of MIDI events
#[derive(Debug, Clone, Default)]
pub struct MidiBuffer {
    pub events: Vec<MidiEvent>,
}

impl MidiBuffer {
    pub fn new() -> Self {
        Self { events: Vec::new() }
    }

    pub fn merge(&mut self, other: &MidiBuffer) {
        self.events.extend(other.events.iter().cloned());
        self.events.sort_by_key(|e| e.frame);
    }

    pub fn clear(&mut self) {
        self.events.clear();
    }
}

/// Control signal with start and end values for interpolation
#[derive(Debug, Clone)]
pub struct ControlBuffer {
    pub start: f64,
    pub end: f64,
}

impl ControlBuffer {
    pub fn constant(value: f64) -> Self {
        Self {
            start: value,
            end: value,
        }
    }

    pub fn at(&self, t: f64) -> f64 {
        self.start + (self.end - self.start) * t.clamp(0.0, 1.0)
    }
}

/// Discrete trigger event
#[derive(Debug, Clone)]
pub struct Trigger {
    pub frame: usize,
    pub kind: TriggerKind,
    pub data: Option<serde_json::Value>,
}

/// Buffer of trigger events
#[derive(Debug, Clone, Default)]
pub struct TriggerBuffer {
    pub triggers: Vec<Trigger>,
}

/// Union type for all signal buffers
#[derive(Debug, Clone)]
pub enum SignalBuffer {
    Audio(AudioBuffer),
    Midi(MidiBuffer),
    Control(ControlBuffer),
    Trigger(TriggerBuffer),
}

// =============================================================================
// NODE TYPES
// =============================================================================

/// Port definition for a node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Port {
    pub name: String,
    pub signal_type: SignalType,
}

/// Node capabilities
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct NodeCapabilities {
    pub realtime: bool,
    pub offline: bool,
}

impl NodeCapabilities {
    pub fn to_capability_uris(&self) -> Vec<&'static str> {
        let mut uris = vec![];
        if self.realtime {
            uris.push("audio:realtime");
        }
        if self.offline {
            uris.push("audio:offline");
        }
        uris
    }
}

/// Descriptor for a node in the graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeDescriptor {
    pub id: Uuid,
    pub name: String,
    pub type_id: String,
    pub inputs: Vec<Port>,
    pub outputs: Vec<Port>,
    pub latency_samples: u64,
    pub capabilities: NodeCapabilities,
}

/// Processing mode for the graph
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessingMode {
    Offline,
    Realtime { deadline_ns: u64 },
}

/// Transport state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TransportState {
    #[default]
    Stopped,
    Playing,
    Recording,
}

/// Context passed to nodes during processing
#[derive(Clone)]
pub struct ProcessContext {
    pub sample_rate: u32,
    pub buffer_size: usize,
    pub position_samples: Sample,
    pub position_beats: Beat,
    pub tempo_map: Arc<TempoMap>,
    pub mode: ProcessingMode,
    pub transport: TransportState,
}

/// Error during node processing
#[derive(Debug, Clone)]
pub enum ProcessError {
    Skipped { reason: &'static str },
    Failed { reason: String },
}

/// Trait for audio processing nodes
pub trait Node: Send + Sync {
    fn descriptor(&self) -> &NodeDescriptor;

    fn process(
        &mut self,
        ctx: &ProcessContext,
        inputs: &[SignalBuffer],
        outputs: &mut [SignalBuffer],
    ) -> Result<(), ProcessError>;

    fn reset(&mut self) {}

    fn shutdown(&mut self) {}
}

/// Type alias for boxed node
pub type BoxedNode = Box<dyn Node>;

// =============================================================================
// LIFECYCLE (from 08-capabilities, needed by Region)
// =============================================================================

/// Generation counter for grooming
pub type Generation = u64;

/// Lifecycle state for any entity that can be groomed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lifecycle {
    pub created_at: DateTime<Utc>,
    pub created_generation: Generation,
    pub last_touched_at: DateTime<Utc>,
    pub last_touched_generation: Generation,
    pub tombstoned_at: Option<DateTime<Utc>>,
    pub tombstoned_generation: Option<Generation>,
    pub permanent: bool,
}

impl Lifecycle {
    pub fn new(generation: Generation) -> Self {
        let now = Utc::now();
        Self {
            created_at: now,
            created_generation: generation,
            last_touched_at: now,
            last_touched_generation: generation,
            tombstoned_at: None,
            tombstoned_generation: None,
            permanent: false,
        }
    }

    pub fn touch(&mut self, generation: Generation) {
        self.last_touched_at = Utc::now();
        self.last_touched_generation = generation;
        if self.tombstoned_at.is_some() {
            self.tombstoned_at = None;
            self.tombstoned_generation = None;
        }
    }

    pub fn tombstone(&mut self, generation: Generation) {
        if !self.permanent {
            self.tombstoned_at = Some(Utc::now());
            self.tombstoned_generation = Some(generation);
        }
    }

    pub fn set_permanent(&mut self, permanent: bool) {
        self.permanent = permanent;
        if permanent {
            self.tombstoned_at = None;
            self.tombstoned_generation = None;
        }
    }

    pub fn is_tombstoned(&self) -> bool {
        self.tombstoned_at.is_some()
    }

    pub fn is_alive(&self) -> bool {
        !self.is_tombstoned()
    }
}

impl Default for Lifecycle {
    fn default() -> Self {
        Self::new(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Time conversion tests

    #[test]
    fn test_tick_beat_conversion() {
        let map = TempoMap::default();

        let tick = Tick(960);
        let beat = map.tick_to_beat(tick);
        assert_eq!(beat.0, 1.0);

        let back = map.beat_to_tick(beat);
        assert_eq!(back.0, tick.0);
    }

    #[test]
    fn test_tick_second_conversion_constant_tempo() {
        let map = TempoMap::new(120.0, TimeSignature::default());

        let tick = Tick(960);
        let second = map.tick_to_second(tick);
        assert!((second.0 - 0.5).abs() < 0.0001);

        let back = map.second_to_tick(second);
        assert_eq!(back.0, tick.0);
    }

    #[test]
    fn test_tick_second_conversion_with_tempo_change() {
        let mut map = TempoMap::new(120.0, TimeSignature::default());
        map.add_tempo_change(Tick(960), 60.0);

        let second_at_1beat = map.tick_to_second(Tick(960));
        assert!((second_at_1beat.0 - 0.5).abs() < 0.0001);

        let second_at_2beats = map.tick_to_second(Tick(1920));
        assert!((second_at_2beats.0 - 1.5).abs() < 0.0001);
    }

    #[test]
    fn test_tick_sample_conversion() {
        let map = TempoMap::new(120.0, TimeSignature::default());
        let sample_rate = 48000;

        let tick = Tick(960);
        let sample = map.tick_to_sample(tick, sample_rate);
        assert_eq!(sample.0, 24000);

        let back = map.sample_to_tick(sample, sample_rate);
        assert_eq!(back.0, tick.0);
    }

    #[test]
    fn test_tempo_at() {
        let mut map = TempoMap::new(120.0, TimeSignature::default());
        map.add_tempo_change(Tick(960), 140.0);
        map.add_tempo_change(Tick(1920), 100.0);

        assert_eq!(map.tempo_at(Tick(0)), 120.0);
        assert_eq!(map.tempo_at(Tick(500)), 120.0);
        assert_eq!(map.tempo_at(Tick(960)), 140.0);
        assert_eq!(map.tempo_at(Tick(1500)), 140.0);
        assert_eq!(map.tempo_at(Tick(1920)), 100.0);
        assert_eq!(map.tempo_at(Tick(3000)), 100.0);
    }

    // Signal buffer tests

    #[test]
    fn test_audio_buffer_mix() {
        let mut buf1 = AudioBuffer::new(4, 2);
        buf1.samples = vec![1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0];

        let mut buf2 = AudioBuffer::new(4, 2);
        buf2.samples = vec![0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5];

        buf1.mix(&buf2, 1.0);
        assert_eq!(buf1.samples, vec![1.5; 8]);
    }

    #[test]
    fn test_control_buffer_interpolation() {
        let buf = ControlBuffer {
            start: 0.0,
            end: 1.0,
        };
        assert_eq!(buf.at(0.0), 0.0);
        assert_eq!(buf.at(0.5), 0.5);
        assert_eq!(buf.at(1.0), 1.0);
        assert_eq!(buf.at(2.0), 1.0);
    }

    #[test]
    fn test_midi_buffer_merge() {
        let mut buf1 = MidiBuffer::new();
        buf1.events.push(MidiEvent {
            frame: 10,
            message: MidiMessage::NoteOn {
                channel: 0,
                pitch: 60,
                velocity: 100,
            },
        });

        let mut buf2 = MidiBuffer::new();
        buf2.events.push(MidiEvent {
            frame: 5,
            message: MidiMessage::NoteOn {
                channel: 0,
                pitch: 64,
                velocity: 100,
            },
        });

        buf1.merge(&buf2);
        assert_eq!(buf1.events.len(), 2);
        assert_eq!(buf1.events[0].frame, 5);
        assert_eq!(buf1.events[1].frame, 10);
    }

    #[test]
    fn test_node_trait_object_safe() {
        struct TestNode {
            descriptor: NodeDescriptor,
        }

        impl Node for TestNode {
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

        let node = TestNode {
            descriptor: NodeDescriptor {
                id: Uuid::new_v4(),
                name: "test".to_string(),
                type_id: "test.node".to_string(),
                inputs: vec![],
                outputs: vec![],
                latency_samples: 0,
                capabilities: NodeCapabilities::default(),
            },
        };

        let _boxed: BoxedNode = Box::new(node);
    }
}
