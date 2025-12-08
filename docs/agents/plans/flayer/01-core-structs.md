# Task 01: Core Structs

**Priority:** Critical
**Depends On:** None

---

## Objective

Define the foundational data structures for flayer. This is the heart of the design—get this right and everything else follows.

## Design Philosophy

1. **Automation is universal** - No per-clip effects. Automation lanes modulate parameters over time.
2. **Sections guide generation** - AI-native arrangement structure with mood/energy hints.
3. **Routing is explicit** - Tracks → Buses → Master. No hidden signal paths.
4. **Clips are simple** - Just source + position + time-stretch. That's it.

---

## `crates/flayer/src/lib.rs`

```rust
pub mod project;
pub mod timeline;
pub mod track;
pub mod routing;
pub mod automation;
pub mod midi;
pub mod render;
pub mod resolve;
pub mod graph;

pub use project::Project;
pub use timeline::{Timeline, Section};
pub use track::{Track, Clip, ClipSource, Latent, LatentMode, LatentParams, MusicalAttributes};
pub use routing::{Bus, Send, MasterBus, OutputTarget};
pub use automation::{AutomationLane, AutomationPoint, Parameter, Curve};
pub use midi::{Sequence, TempoMap};

pub use uuid::Uuid;
```

---

## `crates/flayer/src/project.rs`

```rust
use crate::Timeline;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use anyhow::Result;
use std::path::Path;

/// A complete flayer project, serializable to disk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: Uuid,
    pub name: String,
    pub created_at: String,
    pub modified_at: String,

    /// The main timeline
    pub timeline: Timeline,

    /// Project-level metadata
    pub metadata: serde_json::Value,
}

impl Project {
    pub fn new(name: &str, bpm: f64) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            created_at: now.clone(),
            modified_at: now,
            timeline: Timeline::new(bpm),
            metadata: serde_json::Value::Null,
        }
    }

    pub fn save(&self, path: impl AsRef<Path>) -> Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let json = std::fs::read_to_string(path)?;
        let project = serde_json::from_str(&json)?;
        Ok(project)
    }

    pub fn touch(&mut self) {
        self.modified_at = chrono::Utc::now().to_rfc3339();
    }
}
```

---

## `crates/flayer/src/timeline.rs`

```rust
use crate::{Track, Bus, MasterBus, AutomationLane};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The main composition container
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Timeline {
    pub id: Uuid,
    pub bpm: f64,
    pub time_sig: (u8, u8),
    pub ppq: u16,

    /// Arrangement structure - AI-native sections with generation hints
    pub sections: Vec<Section>,

    /// Audio/MIDI tracks
    pub tracks: Vec<Track>,

    /// Submix buses
    pub buses: Vec<Bus>,

    /// Master output
    pub master: MasterBus,
}

/// A named section of the arrangement with AI generation hints
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Section {
    pub id: Uuid,
    pub name: String,
    pub start_beat: f64,
    pub end_beat: f64,

    // AI generation hints - latents in this section inherit these

    /// Emotional quality: "calm", "tense", "euphoric", "melancholy"
    pub mood: Option<String>,

    /// Intensity level: 0.0 (ambient) to 1.0 (peak energy)
    pub energy: Option<f64>,

    /// Musical density: 0.0 (sparse) to 1.0 (dense)
    pub density: Option<f64>,

    /// Reference another section to contrast with
    pub contrast_with: Option<Uuid>,

    /// Freeform style hints: "driving", "spacious", "rhythmic"
    pub style_hints: Vec<String>,
}

impl Timeline {
    pub fn new(bpm: f64) -> Self {
        Self {
            id: Uuid::new_v4(),
            bpm,
            time_sig: (4, 4),
            ppq: 960,
            sections: Vec::new(),
            tracks: Vec::new(),
            buses: Vec::new(),
            master: MasterBus::new(),
        }
    }

    pub fn with_time_sig(mut self, num: u8, denom: u8) -> Self {
        self.time_sig = (num, denom);
        self
    }

    /// Add a section to the arrangement
    pub fn add_section(&mut self, name: &str, start: f64, end: f64) -> &mut Section {
        let section = Section {
            id: Uuid::new_v4(),
            name: name.to_string(),
            start_beat: start,
            end_beat: end,
            mood: None,
            energy: None,
            density: None,
            contrast_with: None,
            style_hints: Vec::new(),
        };
        self.sections.push(section);
        self.sections.last_mut().unwrap()
    }

    /// Find section containing a given beat
    pub fn section_at(&self, beat: f64) -> Option<&Section> {
        self.sections.iter().find(|s| beat >= s.start_beat && beat < s.end_beat)
    }

    /// Add a track routed to master
    pub fn add_track(&mut self, name: &str) -> &mut Track {
        let track = Track::new(name);
        self.tracks.push(track);
        self.tracks.last_mut().unwrap()
    }

    /// Add a bus for submixing
    pub fn add_bus(&mut self, name: &str) -> Uuid {
        let bus = Bus::new(name);
        let id = bus.id;
        self.buses.push(bus);
        id
    }

    /// Get total duration in beats
    pub fn duration_beats(&self) -> f64 {
        let track_end = self.tracks.iter()
            .map(|t| t.end_beat())
            .fold(0.0, f64::max);

        let section_end = self.sections.iter()
            .map(|s| s.end_beat)
            .fold(0.0, f64::max);

        f64::max(track_end, section_end)
    }

    /// Convert beats to seconds
    pub fn beats_to_seconds(&self, beats: f64) -> f64 {
        beats * 60.0 / self.bpm
    }

    /// Convert beats to samples
    pub fn beats_to_samples(&self, beats: f64, sample_rate: u32) -> usize {
        (self.beats_to_seconds(beats) * sample_rate as f64) as usize
    }
}

impl Section {
    pub fn with_mood(mut self, mood: &str) -> Self {
        self.mood = Some(mood.to_string());
        self
    }

    pub fn with_energy(mut self, energy: f64) -> Self {
        self.energy = Some(energy.clamp(0.0, 1.0));
        self
    }

    pub fn with_density(mut self, density: f64) -> Self {
        self.density = Some(density.clamp(0.0, 1.0));
        self
    }

    pub fn contrasting(mut self, other: Uuid) -> Self {
        self.contrast_with = Some(other);
        self
    }

    pub fn with_style(mut self, hint: &str) -> Self {
        self.style_hints.push(hint.to_string());
        self
    }

    pub fn duration(&self) -> f64 {
        self.end_beat - self.start_beat
    }
}
```

---

## `crates/flayer/src/track.rs`

```rust
use crate::{AutomationLane, OutputTarget};
use crate::routing::Send;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// An audio or MIDI track
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    pub id: Uuid,
    pub name: String,

    // Content
    pub clips: Vec<Clip>,
    pub latents: Vec<Latent>,

    // Routing
    pub output: OutputTarget,
    pub sends: Vec<Send>,

    // Automation (replaces static volume/pan/mute)
    pub automation: Vec<AutomationLane>,

    // Crossfade handling for overlapping clips
    pub crossfade_ms: f64,
}

/// A concrete audio or MIDI region on the timeline
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Clip {
    pub id: Uuid,
    pub source: ClipSource,

    // Position on timeline
    pub at: f64,           // Start beat
    pub duration: f64,     // Duration in beats

    // Source window (which part of the source to play)
    pub source_offset: f64,
    pub source_duration: f64,

    // Time manipulation
    pub playback_rate: f64,
    pub reverse: bool,

    // Static gain (automation multiplies this)
    pub gain: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClipSource {
    Audio(AudioSource),
    Midi(MidiSource),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioSource {
    pub hash: String,        // CAS content hash
    pub sample_rate: u32,
    pub channels: u8,
    pub duration_samples: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MidiSource {
    pub hash: String,        // CAS content hash
    pub ppq: u16,
    pub duration_ticks: u64,
}

/// A generative region - the core innovation of flayer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Latent {
    pub id: Uuid,

    // Position on timeline
    pub at: f64,
    pub duration: f64,

    // Generation config
    pub model: String,
    pub mode: LatentMode,
    pub params: LatentParams,

    // Resolution state
    #[serde(skip)]
    pub resolved: Option<Clip>,
}

/// How to generate content for this latent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LatentMode {
    /// Generate from scratch or continue from seed
    Generate {
        seed_from: Option<SeedSource>,
    },

    /// Fill a gap using before and after context
    Infill {
        before_context_beats: f64,
        after_context_beats: f64,
    },

    /// Regenerate with constraints
    Variation {
        source_hash: String,
        preserve_rhythm: bool,
        preserve_harmony: bool,
    },
}

impl Default for LatentMode {
    fn default() -> Self {
        Self::Generate { seed_from: None }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SeedSource {
    /// Use output of another Latent
    Latent(Uuid),
    /// Use a specific CAS hash
    Hash(String),
    /// Use rendered audio before this latent
    PriorContext { beats: f64 },
}

/// Parameters for generation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatentParams {
    pub temperature: f64,
    pub top_p: f64,
    pub max_tokens: Option<u32>,
    pub seed: Option<u64>,
    pub prompt: Option<String>,

    /// Musical attributes for controllable generation
    pub attributes: Option<MusicalAttributes>,
}

impl Default for LatentParams {
    fn default() -> Self {
        Self {
            temperature: 1.0,
            top_p: 0.95,
            max_tokens: None,
            seed: None,
            prompt: None,
            attributes: None,
        }
    }
}

/// Controllable musical attributes
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MusicalAttributes {
    pub instrument: Option<String>,
    pub density: Option<f64>,
    pub polyphony: Option<u8>,
    pub articulation: Option<f64>,
    pub style: Option<String>,
    pub key: Option<String>,
}

// === Implementations ===

impl Track {
    pub fn new(name: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            clips: Vec::new(),
            latents: Vec::new(),
            output: OutputTarget::Master,
            sends: Vec::new(),
            automation: Vec::new(),
            crossfade_ms: 10.0,
        }
    }

    pub fn route_to(mut self, bus_id: Uuid) -> Self {
        self.output = OutputTarget::Bus(bus_id);
        self
    }

    pub fn add_clip(&mut self, clip: Clip) {
        self.clips.push(clip);
        self.clips.sort_by(|a, b| a.at.partial_cmp(&b.at).unwrap());
    }

    pub fn add_latent(&mut self, latent: Latent) {
        self.latents.push(latent);
        self.latents.sort_by(|a, b| a.at.partial_cmp(&b.at).unwrap());
    }

    pub fn add_send(&mut self, bus_id: Uuid, amount: f64) {
        self.sends.push(Send {
            bus_id,
            amount: amount.clamp(0.0, 1.0),
            pre_fader: false,
        });
    }

    pub fn end_beat(&self) -> f64 {
        let clip_end = self.clips.iter()
            .map(|c| c.at + c.duration)
            .fold(0.0, f64::max);

        let latent_end = self.latents.iter()
            .map(|l| l.at + l.duration)
            .fold(0.0, f64::max);

        f64::max(clip_end, latent_end)
    }
}

impl Clip {
    pub fn audio(hash: &str, at: f64, duration: f64) -> Self {
        Self {
            id: Uuid::new_v4(),
            source: ClipSource::Audio(AudioSource {
                hash: hash.to_string(),
                sample_rate: 44100,
                channels: 2,
                duration_samples: 0,
            }),
            at,
            duration,
            source_offset: 0.0,
            source_duration: duration,
            playback_rate: 1.0,
            reverse: false,
            gain: 1.0,
        }
    }

    pub fn midi(hash: &str, at: f64, duration: f64) -> Self {
        Self {
            id: Uuid::new_v4(),
            source: ClipSource::Midi(MidiSource {
                hash: hash.to_string(),
                ppq: 960,
                duration_ticks: 0,
            }),
            at,
            duration,
            source_offset: 0.0,
            source_duration: duration,
            playback_rate: 1.0,
            reverse: false,
            gain: 1.0,
        }
    }

    pub fn end_beat(&self) -> f64 {
        self.at + self.duration
    }
}

impl Latent {
    pub fn new(model: &str, at: f64, duration: f64) -> Self {
        Self {
            id: Uuid::new_v4(),
            at,
            duration,
            model: model.to_string(),
            mode: LatentMode::default(),
            params: LatentParams::default(),
            resolved: None,
        }
    }

    pub fn generate(model: &str, at: f64, duration: f64) -> Self {
        Self::new(model, at, duration)
    }

    pub fn infill(model: &str, at: f64, duration: f64, before: f64, after: f64) -> Self {
        let mut l = Self::new(model, at, duration);
        l.mode = LatentMode::Infill {
            before_context_beats: before,
            after_context_beats: after,
        };
        l
    }

    pub fn variation(model: &str, at: f64, duration: f64, source: &str) -> Self {
        let mut l = Self::new(model, at, duration);
        l.mode = LatentMode::Variation {
            source_hash: source.to_string(),
            preserve_rhythm: false,
            preserve_harmony: false,
        };
        l
    }

    pub fn with_params(mut self, params: LatentParams) -> Self {
        self.params = params;
        self
    }

    pub fn with_seed(mut self, source: SeedSource) -> Self {
        self.mode = LatentMode::Generate { seed_from: Some(source) };
        self
    }

    pub fn is_resolved(&self) -> bool {
        self.resolved.is_some()
    }
}
```

---

## `crates/flayer/src/routing.rs`

```rust
use crate::AutomationLane;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Where a track or bus outputs to
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum OutputTarget {
    Master,
    Bus(Uuid),
}

impl Default for OutputTarget {
    fn default() -> Self {
        Self::Master
    }
}

/// A submix bus
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bus {
    pub id: Uuid,
    pub name: String,

    /// Where this bus outputs to (can chain to other buses)
    pub output: OutputTarget,

    /// Automation lanes for this bus
    pub automation: Vec<AutomationLane>,
}

impl Bus {
    pub fn new(name: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            output: OutputTarget::Master,
            automation: Vec::new(),
        }
    }

    pub fn route_to(mut self, target: OutputTarget) -> Self {
        self.output = target;
        self
    }
}

/// Aux send from a track to a bus
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Send {
    pub bus_id: Uuid,
    pub amount: f64,
    pub pre_fader: bool,
}

/// The master output bus
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MasterBus {
    pub id: Uuid,
    pub automation: Vec<AutomationLane>,

    /// PipeWire node name for external routing
    pub pipewire_node: Option<String>,
}

impl MasterBus {
    pub fn new() -> Self {
        Self {
            id: Uuid::new_v4(),
            automation: Vec::new(),
            pipewire_node: None,
        }
    }

    pub fn with_pipewire(mut self, node_name: &str) -> Self {
        self.pipewire_node = Some(node_name.to_string());
        self
    }
}

impl Default for MasterBus {
    fn default() -> Self {
        Self::new()
    }
}
```

---

## `crates/flayer/src/automation.rs`

```rust
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A lane of automation data for a single parameter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationLane {
    pub id: Uuid,
    pub parameter: Parameter,
    pub points: Vec<AutomationPoint>,
}

/// What parameter is being automated
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Parameter {
    Volume,
    Pan,
    Mute,

    /// Named effect parameter (e.g., "compressor.ratio", "reverb.wet")
    Effect { name: String },
}

/// A single point on an automation curve
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationPoint {
    pub beat: f64,
    pub value: f64,
    pub curve: Curve,
}

/// How to interpolate from the previous point to this one
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum Curve {
    #[default]
    Linear,
    Exponential,
    Logarithmic,
    SCurve,
    Hold,  // Step/instant change
}

impl AutomationLane {
    pub fn new(parameter: Parameter) -> Self {
        Self {
            id: Uuid::new_v4(),
            parameter,
            points: Vec::new(),
        }
    }

    pub fn add_point(&mut self, beat: f64, value: f64) {
        self.points.push(AutomationPoint {
            beat,
            value,
            curve: Curve::Linear,
        });
        self.points.sort_by(|a, b| a.beat.partial_cmp(&b.beat).unwrap());
    }

    pub fn add_point_with_curve(&mut self, beat: f64, value: f64, curve: Curve) {
        self.points.push(AutomationPoint { beat, value, curve });
        self.points.sort_by(|a, b| a.beat.partial_cmp(&b.beat).unwrap());
    }

    /// Get the value at a given beat, interpolating between points
    pub fn value_at(&self, beat: f64) -> Option<f64> {
        if self.points.is_empty() {
            return None;
        }

        // Before first point
        if beat <= self.points[0].beat {
            return Some(self.points[0].value);
        }

        // After last point
        if beat >= self.points.last().unwrap().beat {
            return Some(self.points.last().unwrap().value);
        }

        // Find surrounding points
        for i in 0..self.points.len() - 1 {
            let p1 = &self.points[i];
            let p2 = &self.points[i + 1];

            if beat >= p1.beat && beat < p2.beat {
                return Some(interpolate(p1, p2, beat));
            }
        }

        None
    }
}

fn interpolate(from: &AutomationPoint, to: &AutomationPoint, beat: f64) -> f64 {
    let t = (beat - from.beat) / (to.beat - from.beat);

    match to.curve {
        Curve::Hold => from.value,
        Curve::Linear => lerp(from.value, to.value, t),
        Curve::Exponential => {
            // Exponential curve (fast start, slow end)
            let t_curved = t * t;
            lerp(from.value, to.value, t_curved)
        }
        Curve::Logarithmic => {
            // Logarithmic curve (slow start, fast end)
            let t_curved = t.sqrt();
            lerp(from.value, to.value, t_curved)
        }
        Curve::SCurve => {
            // Smooth S-curve (slow-fast-slow)
            let t_curved = t * t * (3.0 - 2.0 * t);
            lerp(from.value, to.value, t_curved)
        }
    }
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

// === Convenience constructors ===

impl AutomationLane {
    /// Create a volume fade in
    pub fn fade_in(start_beat: f64, duration: f64) -> Self {
        let mut lane = Self::new(Parameter::Volume);
        lane.add_point(start_beat, 0.0);
        lane.add_point(start_beat + duration, 1.0);
        lane
    }

    /// Create a volume fade out
    pub fn fade_out(end_beat: f64, duration: f64) -> Self {
        let mut lane = Self::new(Parameter::Volume);
        lane.add_point(end_beat - duration, 1.0);
        lane.add_point(end_beat, 0.0);
        lane
    }

    /// Create a constant value
    pub fn constant(parameter: Parameter, value: f64) -> Self {
        let mut lane = Self::new(parameter);
        lane.add_point(0.0, value);
        lane
    }
}
```

---

## Acceptance Criteria

- [ ] `cargo build -p flayer` succeeds
- [ ] All structs derive `Debug, Clone, Serialize, Deserialize`
- [ ] `Project::save()` and `Project::load()` round-trip correctly
- [ ] `Section` creation and lookup by beat works
- [ ] `AutomationLane::value_at()` interpolates correctly for all curve types
- [ ] `Latent::infill()` and `Latent::variation()` constructors work

## Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_project_roundtrip() {
        let project = Project::new("test", 120.0);
        let json = serde_json::to_string(&project).unwrap();
        let loaded: Project = serde_json::from_str(&json).unwrap();
        assert_eq!(project.name, loaded.name);
    }

    #[test]
    fn test_section_at() {
        let mut tl = Timeline::new(120.0);
        tl.add_section("intro", 0.0, 8.0);
        tl.add_section("verse", 8.0, 24.0);

        assert_eq!(tl.section_at(4.0).unwrap().name, "intro");
        assert_eq!(tl.section_at(16.0).unwrap().name, "verse");
        assert!(tl.section_at(30.0).is_none());
    }

    #[test]
    fn test_automation_interpolation() {
        let mut lane = AutomationLane::new(Parameter::Volume);
        lane.add_point(0.0, 0.0);
        lane.add_point(4.0, 1.0);

        assert_eq!(lane.value_at(0.0), Some(0.0));
        assert_eq!(lane.value_at(2.0), Some(0.5));
        assert_eq!(lane.value_at(4.0), Some(1.0));
    }

    #[test]
    fn test_track_routing() {
        let mut tl = Timeline::new(120.0);
        let drum_bus = tl.add_bus("Drums");
        let track = Track::new("Kick").route_to(drum_bus);

        assert_eq!(track.output, OutputTarget::Bus(drum_bus));
    }
}
```
