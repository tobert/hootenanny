//! DAW-style patterns for timeline organization
//!
//! Patterns are a convenience layer for participants familiar with traditional DAW
//! concepts. They're one way to organize a performance—not the only way. Agents
//! can work directly with regions and graphs if they prefer.
//!
//! Key types:
//! - `Track` — A lane containing regions, routed to master or a bus
//! - `Bus` — A submix point for grouping and effects
//! - `Section` — A named time range with hints for generation
//! - `Timeline` — The complete arrangement, builds into a Graph
//! - `Project` — Persistent container with save/load

use std::path::Path;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::graph::Graph;
use crate::primitives::{
    Beat, Node, NodeCapabilities, NodeDescriptor, Port, ProcessContext, ProcessError, Region,
    SignalBuffer, SignalType, TempoMap,
};

/// Where a track routes its output
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TrackOutput {
    Master,
    Bus(Uuid),
}

impl Default for TrackOutput {
    fn default() -> Self {
        Self::Master
    }
}

/// An auxiliary send from a track to a bus
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Send {
    pub bus_id: Uuid,
    pub amount: f64,
    pub pre_fader: bool,
}

impl Send {
    pub fn new(bus_id: Uuid, amount: f64) -> Self {
        Self {
            bus_id,
            amount,
            pre_fader: false,
        }
    }

    pub fn pre_fader(mut self) -> Self {
        self.pre_fader = true;
        self
    }
}

/// A track containing regions, with mixer settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    pub id: Uuid,
    pub name: String,
    pub regions: Vec<Region>,
    pub mixer_node_id: Uuid,
    pub output: TrackOutput,
    pub sends: Vec<Send>,
    pub volume: f64,
    pub pan: f64,
    pub mute: bool,
    pub solo: bool,
}

impl Track {
    pub fn new(name: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            regions: Vec::new(),
            mixer_node_id: Uuid::new_v4(),
            output: TrackOutput::Master,
            sends: Vec::new(),
            volume: 1.0,
            pan: 0.0,
            mute: false,
            solo: false,
        }
    }

    /// Add a region to this track
    pub fn add_region(&mut self, region: Region) {
        self.regions.push(region);
    }

    /// Route this track to a bus instead of master
    pub fn route_to_bus(&mut self, bus_id: Uuid) {
        self.output = TrackOutput::Bus(bus_id);
    }

    /// Route this track to master
    pub fn route_to_master(&mut self) {
        self.output = TrackOutput::Master;
    }

    /// Add an auxiliary send to a bus
    pub fn add_send(&mut self, bus_id: Uuid, amount: f64) {
        self.sends.push(Send::new(bus_id, amount));
    }

    /// Get regions active at a specific beat
    pub fn regions_at(&self, beat: Beat) -> impl Iterator<Item = &Region> {
        self.regions.iter().filter(move |r| r.contains(beat))
    }

    /// Get the end position of the last region
    pub fn end(&self) -> Beat {
        self.regions
            .iter()
            .map(|r| r.end())
            .max_by(|a, b| a.0.partial_cmp(&b.0).unwrap())
            .unwrap_or(Beat(0.0))
    }

    /// Convenience for adding a latent region
    pub fn add_latent(
        &mut self,
        position: Beat,
        duration: Beat,
        tool: &str,
        params: serde_json::Value,
    ) -> Uuid {
        let region = Region::latent(position, duration, tool, params);
        let id = region.id;
        self.regions.push(region);
        id
    }

    /// Convenience for adding audio content
    pub fn add_audio(&mut self, position: Beat, duration: Beat, content_hash: &str) -> Uuid {
        let region = Region::play_audio(position, duration, content_hash.to_string());
        let id = region.id;
        self.regions.push(region);
        id
    }

    /// Convenience for adding MIDI content
    pub fn add_midi(&mut self, position: Beat, duration: Beat, content_hash: &str) -> Uuid {
        let region = Region::play_midi(position, duration, content_hash.to_string());
        let id = region.id;
        self.regions.push(region);
        id
    }
}

/// Where a bus routes its output
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BusOutput {
    Master,
    Bus(Uuid),
}

impl Default for BusOutput {
    fn default() -> Self {
        Self::Master
    }
}

/// A submix bus for grouping and effects
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bus {
    pub id: Uuid,
    pub name: String,
    pub mixer_node_id: Uuid,
    pub output: BusOutput,
    pub volume: f64,
    pub pan: f64,
    pub mute: bool,
}

impl Bus {
    pub fn new(name: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            mixer_node_id: Uuid::new_v4(),
            output: BusOutput::Master,
            volume: 1.0,
            pan: 0.0,
            mute: false,
        }
    }

    /// Route this bus to another bus
    pub fn route_to_bus(&mut self, bus_id: Uuid) {
        self.output = BusOutput::Bus(bus_id);
    }

    /// Route this bus to master
    pub fn route_to_master(&mut self) {
        self.output = BusOutput::Master;
    }
}

/// Hints for generation within a section
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SectionHints {
    pub mood: Option<String>,
    pub energy: Option<f64>,
    pub density: Option<f64>,
    pub contrast_with: Option<Uuid>,
    pub style_hints: Vec<String>,
}

/// A named time range with semantic hints
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Section {
    pub id: Uuid,
    pub name: String,
    pub start: Beat,
    pub end: Beat,
    pub hints: SectionHints,
    pub color: Option<String>,
}

impl Section {
    pub fn new(name: &str, start: Beat, end: Beat) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            start,
            end,
            hints: SectionHints::default(),
            color: None,
        }
    }

    pub fn with_mood(mut self, mood: &str) -> Self {
        self.hints.mood = Some(mood.to_string());
        self
    }

    pub fn with_energy(mut self, energy: f64) -> Self {
        self.hints.energy = Some(energy.clamp(0.0, 1.0));
        self
    }

    pub fn with_density(mut self, density: f64) -> Self {
        self.hints.density = Some(density.clamp(0.0, 1.0));
        self
    }

    pub fn with_contrast(mut self, other: Uuid) -> Self {
        self.hints.contrast_with = Some(other);
        self
    }

    pub fn with_color(mut self, color: &str) -> Self {
        self.color = Some(color.to_string());
        self
    }

    pub fn with_style_hint(mut self, hint: &str) -> Self {
        self.hints.style_hints.push(hint.to_string());
        self
    }

    /// Duration in beats
    pub fn duration(&self) -> Beat {
        Beat(self.end.0 - self.start.0)
    }

    /// Check if a beat position is within this section
    pub fn contains(&self, beat: Beat) -> bool {
        beat.0 >= self.start.0 && beat.0 < self.end.0
    }
}

/// The complete arrangement
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Timeline {
    pub id: Uuid,
    pub name: String,
    pub tempo_map: TempoMap,
    pub sections: Vec<Section>,
    pub tracks: Vec<Track>,
    pub buses: Vec<Bus>,
    pub master_node_id: Uuid,
}

impl Timeline {
    pub fn new(name: &str, bpm: f64) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            tempo_map: TempoMap::new(bpm, Default::default()),
            sections: Vec::new(),
            tracks: Vec::new(),
            buses: Vec::new(),
            master_node_id: Uuid::new_v4(),
        }
    }

    /// Add a section to the timeline
    pub fn add_section(&mut self, name: &str, start: Beat, end: Beat) -> &mut Section {
        let section = Section::new(name, start, end);
        self.sections.push(section);
        self.sections.last_mut().unwrap()
    }

    /// Add a track to the timeline
    pub fn add_track(&mut self, name: &str) -> &mut Track {
        let track = Track::new(name);
        self.tracks.push(track);
        self.tracks.last_mut().unwrap()
    }

    /// Add a bus to the timeline
    pub fn add_bus(&mut self, name: &str) -> &mut Bus {
        let bus = Bus::new(name);
        self.buses.push(bus);
        self.buses.last_mut().unwrap()
    }

    /// Get the section containing a beat position
    pub fn section_at(&self, beat: Beat) -> Option<&Section> {
        self.sections.iter().find(|s| s.contains(beat))
    }

    /// Get hints for a beat position from the enclosing section
    pub fn hints_at(&self, beat: Beat) -> Option<&SectionHints> {
        self.section_at(beat).map(|s| &s.hints)
    }

    /// Iterate over all regions across all tracks
    pub fn all_regions(&self) -> impl Iterator<Item = &Region> {
        self.tracks.iter().flat_map(|t| t.regions.iter())
    }

    /// Get the total duration (end of last region or section)
    pub fn duration(&self) -> Beat {
        let track_end = self
            .tracks
            .iter()
            .map(|t| t.end())
            .max_by(|a, b| a.0.partial_cmp(&b.0).unwrap())
            .unwrap_or(Beat(0.0));

        let section_end = self
            .sections
            .iter()
            .map(|s| s.end)
            .max_by(|a, b| a.0.partial_cmp(&b.0).unwrap())
            .unwrap_or(Beat(0.0));

        if track_end.0 > section_end.0 {
            track_end
        } else {
            section_end
        }
    }

    /// Get a track by ID
    pub fn track(&self, id: Uuid) -> Option<&Track> {
        self.tracks.iter().find(|t| t.id == id)
    }

    /// Get a mutable track by ID
    pub fn track_mut(&mut self, id: Uuid) -> Option<&mut Track> {
        self.tracks.iter_mut().find(|t| t.id == id)
    }

    /// Get a bus by ID
    pub fn bus(&self, id: Uuid) -> Option<&Bus> {
        self.buses.iter().find(|b| b.id == id)
    }

    /// Get a mutable bus by ID
    pub fn bus_mut(&mut self, id: Uuid) -> Option<&mut Bus> {
        self.buses.iter_mut().find(|b| b.id == id)
    }

    /// Build an executable Graph from the timeline structure
    pub fn build_graph(&self) -> Graph {
        let mut graph = Graph::new();

        // 1. Create master output node
        let master_node = MasterOutputNode::new(self.master_node_id);
        graph.add_node(Box::new(master_node));

        // 2. Create bus mixer nodes (in dependency order - simple case: all route to master)
        // For nested bus routing, we'd need topological sort
        for bus in &self.buses {
            let bus_node = MixerNode::new(bus.mixer_node_id, &bus.name, bus.volume, bus.pan);
            graph.add_node(Box::new(bus_node));

            match &bus.output {
                BusOutput::Master => {
                    let _ = graph.connect(bus.mixer_node_id, "out", self.master_node_id, "in");
                }
                BusOutput::Bus(parent_id) => {
                    // Look up the parent bus's mixer_node_id from its entity id
                    if let Some(parent) = self.buses.iter().find(|b| b.id == *parent_id) {
                        let _ = graph.connect(bus.mixer_node_id, "out", parent.mixer_node_id, "in");
                    }
                }
            }
        }

        // 3. Create track mixer nodes and route to output
        for track in &self.tracks {
            let track_node =
                MixerNode::new(track.mixer_node_id, &track.name, track.volume, track.pan);
            graph.add_node(Box::new(track_node));

            match &track.output {
                TrackOutput::Master => {
                    let _ = graph.connect(track.mixer_node_id, "out", self.master_node_id, "in");
                }
                TrackOutput::Bus(bus_id) => {
                    // Look up the bus's mixer_node_id from its entity id
                    if let Some(bus) = self.buses.iter().find(|b| b.id == *bus_id) {
                        let _ = graph.connect(track.mixer_node_id, "out", bus.mixer_node_id, "in");
                    }
                }
            }

            // Wire sends
            for send in &track.sends {
                // Look up the bus's mixer_node_id from its entity id
                if let Some(bus) = self.buses.iter().find(|b| b.id == send.bus_id) {
                    let _ = graph.connect(track.mixer_node_id, "send", bus.mixer_node_id, "in");
                }
            }
        }

        graph
    }
}

/// Persistent project container
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: Uuid,
    pub name: String,
    pub created_at: String,
    pub modified_at: String,
    pub timeline: Timeline,
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
            timeline: Timeline::new(name, bpm),
            metadata: serde_json::json!({}),
        }
    }

    /// Save project to a file
    pub fn save(&self, path: impl AsRef<Path>) -> anyhow::Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Load project from a file
    pub fn load(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let json = std::fs::read_to_string(path)?;
        let project = serde_json::from_str(&json)?;
        Ok(project)
    }

    /// Update the modified timestamp
    pub fn touch(&mut self) {
        self.modified_at = chrono::Utc::now().to_rfc3339();
    }
}

/// Placeholder mixer node for graph building
struct MixerNode {
    descriptor: NodeDescriptor,
    #[allow(dead_code)]
    volume: f64,
    #[allow(dead_code)]
    pan: f64,
}

impl MixerNode {
    fn new(id: Uuid, name: &str, volume: f64, pan: f64) -> Self {
        Self {
            descriptor: NodeDescriptor {
                id,
                name: format!("{} Mixer", name),
                type_id: "mixer.channel".to_string(),
                inputs: vec![Port {
                    name: "in".to_string(),
                    signal_type: SignalType::Audio,
                }],
                outputs: vec![
                    Port {
                        name: "out".to_string(),
                        signal_type: SignalType::Audio,
                    },
                    Port {
                        name: "send".to_string(),
                        signal_type: SignalType::Audio,
                    },
                ],
                latency_samples: 0,
                capabilities: NodeCapabilities {
                    realtime: true,
                    offline: true,
                },
            },
            volume,
            pan,
        }
    }
}

impl Node for MixerNode {
    fn descriptor(&self) -> &NodeDescriptor {
        &self.descriptor
    }

    fn process(
        &mut self,
        _ctx: &ProcessContext,
        _inputs: &[SignalBuffer],
        _outputs: &mut [SignalBuffer],
    ) -> Result<(), ProcessError> {
        // Placeholder - actual mixing DSP would go here
        Ok(())
    }
}

/// Master output node for graph building
struct MasterOutputNode {
    descriptor: NodeDescriptor,
}

impl MasterOutputNode {
    fn new(id: Uuid) -> Self {
        Self {
            descriptor: NodeDescriptor {
                id,
                name: "Master Output".to_string(),
                type_id: "output.master".to_string(),
                inputs: vec![Port {
                    name: "in".to_string(),
                    signal_type: SignalType::Audio,
                }],
                outputs: vec![],
                latency_samples: 0,
                capabilities: NodeCapabilities {
                    realtime: true,
                    offline: true,
                },
            },
        }
    }
}

impl Node for MasterOutputNode {
    fn descriptor(&self) -> &NodeDescriptor {
        &self.descriptor
    }

    fn process(
        &mut self,
        _ctx: &ProcessContext,
        _inputs: &[SignalBuffer],
        _outputs: &mut [SignalBuffer],
    ) -> Result<(), ProcessError> {
        // Placeholder - actual output handling would go here
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_track_creation() {
        let track = Track::new("Drums");
        assert_eq!(track.name, "Drums");
        assert_eq!(track.output, TrackOutput::Master);
        assert!(track.regions.is_empty());
    }

    #[test]
    fn test_track_add_region() {
        let mut track = Track::new("Bass");
        let region = Region::play_audio(Beat(0.0), Beat(4.0), "hash123".to_string());
        track.add_region(region);
        assert_eq!(track.regions.len(), 1);
        assert_eq!(track.end(), Beat(4.0));
    }

    #[test]
    fn test_track_routing() {
        let mut track = Track::new("Keys");
        let bus_id = Uuid::new_v4();

        track.route_to_bus(bus_id);
        assert_eq!(track.output, TrackOutput::Bus(bus_id));

        track.route_to_master();
        assert_eq!(track.output, TrackOutput::Master);
    }

    #[test]
    fn test_track_sends() {
        let mut track = Track::new("Synth");
        let reverb_bus = Uuid::new_v4();
        let delay_bus = Uuid::new_v4();

        track.add_send(reverb_bus, 0.5);
        track.add_send(delay_bus, 0.3);

        assert_eq!(track.sends.len(), 2);
        assert_eq!(track.sends[0].amount, 0.5);
        assert_eq!(track.sends[1].amount, 0.3);
    }

    #[test]
    fn test_track_add_latent() {
        let mut track = Track::new("Lead");
        let region_id = track.add_latent(
            Beat(8.0),
            Beat(4.0),
            "orpheus_generate",
            json!({"prompt": "lead melody"}),
        );

        assert_eq!(track.regions.len(), 1);
        assert_eq!(track.regions[0].id, region_id);
        assert!(track.regions[0].is_latent());
    }

    #[test]
    fn test_track_regions_at() {
        let mut track = Track::new("Pads");
        track.add_audio(Beat(0.0), Beat(8.0), "pad1");
        track.add_audio(Beat(4.0), Beat(8.0), "pad2");
        track.add_audio(Beat(16.0), Beat(4.0), "pad3");

        let at_6: Vec<_> = track.regions_at(Beat(6.0)).collect();
        assert_eq!(at_6.len(), 2); // pad1 and pad2 overlap at beat 6

        let at_20: Vec<_> = track.regions_at(Beat(20.0)).collect();
        assert_eq!(at_20.len(), 0); // pad3 ends at 20
    }

    #[test]
    fn test_bus_creation() {
        let bus = Bus::new("Reverb");
        assert_eq!(bus.name, "Reverb");
        assert_eq!(bus.output, BusOutput::Master);
    }

    #[test]
    fn test_bus_routing() {
        let mut reverb = Bus::new("Reverb");
        let submix = Uuid::new_v4();

        reverb.route_to_bus(submix);
        assert_eq!(reverb.output, BusOutput::Bus(submix));
    }

    #[test]
    fn test_section_creation() {
        let section = Section::new("Chorus", Beat(32.0), Beat(64.0));
        assert_eq!(section.name, "Chorus");
        assert_eq!(section.duration(), Beat(32.0));
    }

    #[test]
    fn test_section_hints() {
        let section = Section::new("Drop", Beat(64.0), Beat(96.0))
            .with_mood("euphoric")
            .with_energy(0.95)
            .with_density(0.8)
            .with_style_hint("big synth leads")
            .with_style_hint("heavy drums");

        assert_eq!(section.hints.mood, Some("euphoric".to_string()));
        assert_eq!(section.hints.energy, Some(0.95));
        assert_eq!(section.hints.density, Some(0.8));
        assert_eq!(section.hints.style_hints.len(), 2);
    }

    #[test]
    fn test_section_contains() {
        let section = Section::new("Verse", Beat(0.0), Beat(32.0));

        assert!(section.contains(Beat(0.0)));
        assert!(section.contains(Beat(16.0)));
        assert!(section.contains(Beat(31.9)));
        assert!(!section.contains(Beat(32.0))); // exclusive end
        assert!(!section.contains(Beat(-1.0)));
    }

    #[test]
    fn test_timeline_creation() {
        let timeline = Timeline::new("My Song", 120.0);
        assert_eq!(timeline.name, "My Song");
        assert!(timeline.tracks.is_empty());
        assert!(timeline.buses.is_empty());
    }

    #[test]
    fn test_timeline_add_section() {
        let mut timeline = Timeline::new("Song", 120.0);
        let section = timeline.add_section("Intro", Beat(0.0), Beat(16.0));
        section.hints.mood = Some("mysterious".to_string());

        assert_eq!(timeline.sections.len(), 1);
        assert_eq!(
            timeline.sections[0].hints.mood,
            Some("mysterious".to_string())
        );
    }

    #[test]
    fn test_timeline_add_track() {
        let mut timeline = Timeline::new("Song", 120.0);
        let track = timeline.add_track("Drums");
        track.add_audio(Beat(0.0), Beat(32.0), "drums_loop");

        assert_eq!(timeline.tracks.len(), 1);
        assert_eq!(timeline.tracks[0].regions.len(), 1);
    }

    #[test]
    fn test_timeline_add_bus() {
        let mut timeline = Timeline::new("Song", 120.0);
        timeline.add_bus("Reverb");
        timeline.add_bus("Delay");

        assert_eq!(timeline.buses.len(), 2);
    }

    #[test]
    fn test_timeline_section_at() {
        let mut timeline = Timeline::new("Song", 120.0);
        timeline.add_section("Intro", Beat(0.0), Beat(16.0));
        timeline.add_section("Verse", Beat(16.0), Beat(48.0));

        let intro = timeline.section_at(Beat(8.0));
        assert!(intro.is_some());
        assert_eq!(intro.unwrap().name, "Intro");

        let verse = timeline.section_at(Beat(32.0));
        assert!(verse.is_some());
        assert_eq!(verse.unwrap().name, "Verse");

        let none = timeline.section_at(Beat(100.0));
        assert!(none.is_none());
    }

    #[test]
    fn test_timeline_hints_at() {
        let mut timeline = Timeline::new("Song", 120.0);
        {
            let chorus = timeline.add_section("Chorus", Beat(32.0), Beat(64.0));
            chorus.hints.mood = Some("uplifting".to_string());
            chorus.hints.energy = Some(0.9);
        }

        let hints = timeline.hints_at(Beat(48.0));
        assert!(hints.is_some());
        assert_eq!(hints.unwrap().mood, Some("uplifting".to_string()));

        let no_hints = timeline.hints_at(Beat(0.0));
        assert!(no_hints.is_none());
    }

    #[test]
    fn test_timeline_all_regions() {
        let mut timeline = Timeline::new("Song", 120.0);

        {
            let drums = timeline.add_track("Drums");
            drums.add_audio(Beat(0.0), Beat(32.0), "drums");
        }
        {
            let bass = timeline.add_track("Bass");
            bass.add_audio(Beat(0.0), Beat(32.0), "bass");
            bass.add_audio(Beat(32.0), Beat(32.0), "bass2");
        }

        let regions: Vec<_> = timeline.all_regions().collect();
        assert_eq!(regions.len(), 3);
    }

    #[test]
    fn test_timeline_duration() {
        let mut timeline = Timeline::new("Song", 120.0);

        timeline.add_section("Verse", Beat(0.0), Beat(64.0));
        {
            let track = timeline.add_track("Lead");
            track.add_audio(Beat(0.0), Beat(48.0), "lead");
        }

        // Duration is max of section end and track end
        assert_eq!(timeline.duration(), Beat(64.0));
    }

    #[test]
    fn test_timeline_build_graph() {
        let mut timeline = Timeline::new("Song", 120.0);

        let reverb_id = {
            let bus = timeline.add_bus("Reverb");
            bus.id
        };

        {
            let drums = timeline.add_track("Drums");
            drums.add_send(reverb_id, 0.3);
        }
        {
            let bass = timeline.add_track("Bass");
            bass.route_to_bus(reverb_id);
        }

        let graph = timeline.build_graph();

        // Should have: master, reverb bus, drums track, bass track
        assert_eq!(graph.node_count(), 4);
        // Should have: reverb->master, drums->master, drums->reverb (send), bass->reverb
        assert_eq!(graph.edge_count(), 4);
    }

    #[test]
    fn test_timeline_build_graph_nested_buses() {
        let mut timeline = Timeline::new("Song", 120.0);

        let submix_id = {
            let bus = timeline.add_bus("Submix");
            bus.id
        };

        {
            let reverb = timeline.add_bus("Reverb");
            reverb.route_to_bus(submix_id);
        }

        {
            let track = timeline.add_track("Synth");
            track.route_to_bus(submix_id);
        }

        let graph = timeline.build_graph();

        // master, submix, reverb, synth
        assert_eq!(graph.node_count(), 4);
    }

    #[test]
    fn test_project_creation() {
        let project = Project::new("My Project", 128.0);
        assert_eq!(project.name, "My Project");
        assert!(!project.created_at.is_empty());
    }

    #[test]
    fn test_project_touch() {
        let mut project = Project::new("Test", 120.0);
        let original = project.modified_at.clone();

        std::thread::sleep(std::time::Duration::from_millis(10));
        project.touch();

        assert_ne!(project.modified_at, original);
    }

    #[test]
    fn test_project_save_load() {
        let mut project = Project::new("Roundtrip Test", 140.0);

        {
            let track = project.timeline.add_track("Lead");
            track.add_audio(Beat(0.0), Beat(16.0), "lead_content");
        }
        project.timeline.add_section("Intro", Beat(0.0), Beat(16.0));

        let temp_path = std::env::temp_dir().join("chaosgarden_test_project.json");
        project.save(&temp_path).unwrap();

        let loaded = Project::load(&temp_path).unwrap();

        assert_eq!(loaded.name, project.name);
        assert_eq!(loaded.timeline.tracks.len(), 1);
        assert_eq!(loaded.timeline.sections.len(), 1);

        std::fs::remove_file(temp_path).ok();
    }

    #[test]
    fn test_track_serialization() {
        let mut track = Track::new("Synth");
        track.add_audio(Beat(0.0), Beat(8.0), "synth_loop");
        track.volume = 0.8;
        track.pan = -0.5;

        let json = serde_json::to_string(&track).unwrap();
        let loaded: Track = serde_json::from_str(&json).unwrap();

        assert_eq!(loaded.name, "Synth");
        assert_eq!(loaded.volume, 0.8);
        assert_eq!(loaded.pan, -0.5);
        assert_eq!(loaded.regions.len(), 1);
    }

    #[test]
    fn test_section_serialization() {
        let section = Section::new("Bridge", Beat(64.0), Beat(80.0))
            .with_mood("reflective")
            .with_energy(0.4)
            .with_color("#3366cc");

        let json = serde_json::to_string(&section).unwrap();
        let loaded: Section = serde_json::from_str(&json).unwrap();

        assert_eq!(loaded.name, "Bridge");
        assert_eq!(loaded.hints.mood, Some("reflective".to_string()));
        assert_eq!(loaded.color, Some("#3366cc".to_string()));
    }

    #[test]
    fn test_send_pre_fader() {
        let bus_id = Uuid::new_v4();
        let send = Send::new(bus_id, 0.5).pre_fader();

        assert!(send.pre_fader);
        assert_eq!(send.amount, 0.5);
    }
}
