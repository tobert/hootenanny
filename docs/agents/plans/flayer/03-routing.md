# Task 03: Routing

**Priority:** High
**Sessions:** 2
**Depends On:** 01-core-structs, 02-automation

---

## Objective

Implement the signal routing graph: buses, sends, and master. This makes audio flow explicit and queryable.

## Signal Flow

```
┌──────────────────────────────────────────────────────────────────┐
│                           Timeline                                │
│                                                                   │
│  ┌─────────┐     ┌─────────┐                                     │
│  │ Track   │────▶│         │                                     │
│  │ "Kick"  │     │   Bus   │                                     │
│  └─────────┘     │ "Drums" │────┐                                │
│  ┌─────────┐     │         │    │                                │
│  │ Track   │────▶│         │    │                                │
│  │ "Snare" │     └─────────┘    │                                │
│  └─────────┘                    │     ┌──────────┐               │
│                                 ├────▶│          │               │
│  ┌─────────┐                    │     │  Master  │──▶ PipeWire   │
│  │ Track   │────────────────────┘     │          │               │
│  │ "Bass"  │                          └──────────┘               │
│  └─────────┘                                ▲                    │
│       │                                     │                    │
│       │    ┌─────────┐                      │                    │
│       └───▶│   Bus   │──────────────────────┘                    │
│   (send)   │ "Reverb"│                                           │
│            └─────────┘                                           │
│                                                                   │
└──────────────────────────────────────────────────────────────────┘
```

## Design

### Everything Routes Somewhere

- **Tracks** route to a Bus or Master
- **Buses** route to other Buses or Master
- **Master** is the final destination

No orphan nodes. The graph is always connected.

### Sends are Parallel

A track can have sends to aux buses (reverb, delay) while its main output goes elsewhere:

```rust
track.output = OutputTarget::Bus(drum_bus_id);
track.sends = vec![
    Send { bus: reverb_bus_id, amount: 0.3, pre_fader: false },
];
```

### Master Connects to PipeWire

The master bus has an optional PipeWire node name. This is the bridge between flayer's internal graph and the external audio world.

---

## Files to Create

### `crates/flayer/src/routing.rs`

```rust
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::automation::AutomationSet;

/// Where audio from a track or bus goes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OutputTarget {
    /// Route to a bus.
    Bus(Uuid),

    /// Route directly to master.
    Master,
}

impl Default for OutputTarget {
    fn default() -> Self {
        Self::Master
    }
}

/// A parallel send to an aux bus.
///
/// Sends tap audio from a track and route it to another bus
/// (typically for effects like reverb or delay).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Send {
    /// Which bus to send to.
    pub target: Uuid,

    /// Send level (0.0 - 1.0).
    pub amount: f64,

    /// If true, send is taken before track fader (pre-fader).
    /// If false, send follows track volume (post-fader).
    pub pre_fader: bool,
}

impl Send {
    pub fn new(target: Uuid, amount: f64) -> Self {
        Self {
            target,
            amount: amount.clamp(0.0, 1.0),
            pre_fader: false,
        }
    }

    pub fn pre_fader(mut self) -> Self {
        self.pre_fader = true;
        self
    }

    pub fn post_fader(mut self) -> Self {
        self.pre_fader = false;
        self
    }
}

/// A submix bus that multiple tracks can route to.
///
/// Buses allow grouping (drum bus, vocal bus) and
/// shared processing (group compression, EQ).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bus {
    pub id: Uuid,
    pub name: String,

    /// Where this bus routes to.
    pub output: OutputTarget,

    /// Sends from this bus to other buses.
    pub sends: Vec<Send>,

    /// Automation for this bus (volume, pan, etc.)
    pub automation: AutomationSet,
}

impl Bus {
    pub fn new(name: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            output: OutputTarget::Master,
            sends: Vec::new(),
            automation: AutomationSet::standard(),
        }
    }

    pub fn with_output(mut self, output: OutputTarget) -> Self {
        self.output = output;
        self
    }

    pub fn add_send(&mut self, target: Uuid, amount: f64) -> &mut Self {
        self.sends.push(Send::new(target, amount));
        self
    }
}

/// The master bus - final destination for all audio.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Master {
    pub id: Uuid,

    /// Automation for master (volume, etc.)
    pub automation: AutomationSet,

    /// Optional PipeWire node name for external routing.
    /// e.g., "Flayer Master" will appear in pavucontrol.
    pub pipewire_name: Option<String>,
}

impl Master {
    pub fn new() -> Self {
        Self {
            id: Uuid::new_v4(),
            automation: AutomationSet::standard(),
            pipewire_name: None,
        }
    }

    pub fn with_pipewire_name(mut self, name: &str) -> Self {
        self.pipewire_name = Some(name.to_string());
        self
    }
}

impl Default for Master {
    fn default() -> Self {
        Self::new()
    }
}

/// Routing configuration for a track.
///
/// This is stored separately from Track to keep track.rs focused
/// on content (clips, latents). Routing is a cross-cutting concern.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackRouting {
    pub track_id: Uuid,
    pub output: OutputTarget,
    pub sends: Vec<Send>,
    pub automation: AutomationSet,
}

impl TrackRouting {
    pub fn new(track_id: Uuid) -> Self {
        Self {
            track_id,
            output: OutputTarget::Master,
            sends: Vec::new(),
            automation: AutomationSet::standard(),
        }
    }

    pub fn with_output(mut self, output: OutputTarget) -> Self {
        self.output = output;
        self
    }

    pub fn add_send(&mut self, target: Uuid, amount: f64) -> &mut Self {
        self.sends.push(Send::new(target, amount));
        self
    }
}

/// Complete routing graph for a timeline.
///
/// This holds all buses, the master, and track routing.
/// It's separate from Timeline to keep concerns clean.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingGraph {
    pub buses: Vec<Bus>,
    pub master: Master,
    pub track_routing: Vec<TrackRouting>,
}

impl RoutingGraph {
    pub fn new() -> Self {
        Self {
            buses: Vec::new(),
            master: Master::new(),
            track_routing: Vec::new(),
        }
    }

    pub fn add_bus(&mut self, name: &str) -> &mut Bus {
        let bus = Bus::new(name);
        self.buses.push(bus);
        self.buses.last_mut().unwrap()
    }

    pub fn bus(&self, id: Uuid) -> Option<&Bus> {
        self.buses.iter().find(|b| b.id == id)
    }

    pub fn bus_mut(&mut self, id: Uuid) -> Option<&mut Bus> {
        self.buses.iter_mut().find(|b| b.id == id)
    }

    pub fn bus_by_name(&self, name: &str) -> Option<&Bus> {
        self.buses.iter().find(|b| b.name == name)
    }

    pub fn routing_for(&self, track_id: Uuid) -> Option<&TrackRouting> {
        self.track_routing.iter().find(|r| r.track_id == track_id)
    }

    pub fn routing_for_mut(&mut self, track_id: Uuid) -> Option<&mut TrackRouting> {
        self.track_routing.iter_mut().find(|r| r.track_id == track_id)
    }

    /// Ensure routing exists for a track, creating default if needed.
    pub fn ensure_routing(&mut self, track_id: Uuid) -> &mut TrackRouting {
        if !self.track_routing.iter().any(|r| r.track_id == track_id) {
            self.track_routing.push(TrackRouting::new(track_id));
        }
        self.routing_for_mut(track_id).unwrap()
    }

    /// Validate the routing graph.
    ///
    /// Returns errors if:
    /// - A bus routes to itself
    /// - Circular routing exists
    /// - A send targets a non-existent bus
    pub fn validate(&self) -> Result<(), RoutingError> {
        // Check for self-routing buses
        for bus in &self.buses {
            if let OutputTarget::Bus(target) = &bus.output {
                if *target == bus.id {
                    return Err(RoutingError::SelfRouting(bus.name.clone()));
                }
            }
        }

        // Check for non-existent send targets
        let bus_ids: std::collections::HashSet<_> = self.buses.iter().map(|b| b.id).collect();

        for bus in &self.buses {
            for send in &bus.sends {
                if !bus_ids.contains(&send.target) {
                    return Err(RoutingError::MissingSendTarget {
                        from: bus.name.clone(),
                        target: send.target,
                    });
                }
            }
        }

        for routing in &self.track_routing {
            for send in &routing.sends {
                if !bus_ids.contains(&send.target) {
                    return Err(RoutingError::MissingSendTarget {
                        from: format!("track:{}", routing.track_id),
                        target: send.target,
                    });
                }
            }
        }

        // TODO: Check for circular routing (bus A → bus B → bus A)

        Ok(())
    }

    /// Get topological order for rendering.
    ///
    /// Returns buses in order such that dependencies come first.
    /// Tracks → Leaf buses → Parent buses → Master
    pub fn render_order(&self) -> Vec<RenderNode> {
        let mut order = Vec::new();

        // First, all tracks
        for routing in &self.track_routing {
            order.push(RenderNode::Track(routing.track_id));
        }

        // Then buses in dependency order
        // Simple approach: buses routing to master come after buses routing to other buses
        let mut remaining: Vec<_> = self.buses.iter().collect();
        let mut added = std::collections::HashSet::new();

        while !remaining.is_empty() {
            let before_len = remaining.len();

            remaining.retain(|bus| {
                let can_add = match &bus.output {
                    OutputTarget::Master => true,
                    OutputTarget::Bus(target) => added.contains(target),
                };

                if can_add {
                    order.push(RenderNode::Bus(bus.id));
                    added.insert(bus.id);
                    false // Remove from remaining
                } else {
                    true // Keep in remaining
                }
            });

            // If nothing was added, we have a cycle
            if remaining.len() == before_len && !remaining.is_empty() {
                // Add remaining anyway to avoid infinite loop
                // validate() should catch this
                for bus in remaining {
                    order.push(RenderNode::Bus(bus.id));
                }
                break;
            }
        }

        // Finally, master
        order.push(RenderNode::Master);

        order
    }
}

impl Default for RoutingGraph {
    fn default() -> Self {
        Self::new()
    }
}

/// A node in the render order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderNode {
    Track(Uuid),
    Bus(Uuid),
    Master,
}

/// Routing validation errors.
#[derive(Debug, Clone)]
pub enum RoutingError {
    SelfRouting(String),
    CircularRouting(Vec<String>),
    MissingSendTarget { from: String, target: Uuid },
}

impl std::fmt::Display for RoutingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SelfRouting(name) => write!(f, "Bus '{}' routes to itself", name),
            Self::CircularRouting(path) => write!(f, "Circular routing: {}", path.join(" → ")),
            Self::MissingSendTarget { from, target } => {
                write!(f, "Send from '{}' targets non-existent bus {}", from, target)
            }
        }
    }
}

impl std::error::Error for RoutingError {}
```

### Update `crates/flayer/src/lib.rs`

```rust
pub mod timeline;
pub mod track;
pub mod automation;
pub mod routing;

pub use timeline::{Timeline, Section, SectionHints};
pub use track::{Track, Clip, ClipSource, AudioSource, MidiSource};
pub use track::{Latent, LatentParams, LatentMode, SeedSource, MusicalAttributes};
pub use automation::{AutomationSet, AutomationLane, AutomationPoint, Parameter, Curve, Lfo};
pub use routing::{RoutingGraph, Bus, Master, TrackRouting, Send, OutputTarget, RenderNode};

pub use uuid::Uuid;
```

---

## Usage Examples

### Basic Drum Bus

```rust
let mut tl = Timeline::new("Song", 120.0);
let mut routing = RoutingGraph::new();

// Create drum bus
let drum_bus = routing.add_bus("Drums");
let drum_bus_id = drum_bus.id;

// Create tracks
let kick = tl.add_track("Kick");
let snare = tl.add_track("Snare");

// Route tracks to drum bus
routing.ensure_routing(kick.id).output = OutputTarget::Bus(drum_bus_id);
routing.ensure_routing(snare.id).output = OutputTarget::Bus(drum_bus_id);

// Drum bus routes to master (default)
```

### Reverb Send

```rust
// Create reverb bus
let reverb_bus = routing.add_bus("Reverb");
let reverb_id = reverb_bus.id;

// Vocal track with reverb send
let vocals = tl.add_track("Vocals");
let vocal_routing = routing.ensure_routing(vocals.id);
vocal_routing.output = OutputTarget::Master;
vocal_routing.add_send(reverb_id, 0.3);  // 30% to reverb
```

### Parallel Compression

```rust
// Drum bus with parallel compression
let parallel_bus = routing.add_bus("DrumParallel");
let parallel_id = parallel_bus.id;

let drum_bus = routing.bus_mut(drum_bus_id).unwrap();
drum_bus.add_send(parallel_id, 0.5);  // 50% to parallel compression bus

// Both drum_bus and parallel_bus route to master
// Result: drums + compressed drums mixed together
```

---

## Acceptance Criteria

- [ ] `RoutingGraph::add_bus()` creates buses
- [ ] `TrackRouting` correctly stores output and sends
- [ ] `RoutingGraph::validate()` catches self-routing
- [ ] `RoutingGraph::validate()` catches missing send targets
- [ ] `RoutingGraph::render_order()` returns correct topological order
- [ ] `Send::pre_fader` and `post_fader` work correctly

---

## Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_routing() {
        let mut routing = RoutingGraph::new();

        let drum_bus = routing.add_bus("Drums");
        let drum_bus_id = drum_bus.id;

        let track_id = Uuid::new_v4();
        let track_routing = routing.ensure_routing(track_id);
        track_routing.output = OutputTarget::Bus(drum_bus_id);

        assert!(matches!(
            routing.routing_for(track_id).unwrap().output,
            OutputTarget::Bus(id) if id == drum_bus_id
        ));
    }

    #[test]
    fn test_validate_self_routing() {
        let mut routing = RoutingGraph::new();

        let bus = routing.add_bus("Bad");
        let bus_id = bus.id;

        // Route to self
        routing.bus_mut(bus_id).unwrap().output = OutputTarget::Bus(bus_id);

        assert!(matches!(routing.validate(), Err(RoutingError::SelfRouting(_))));
    }

    #[test]
    fn test_validate_missing_send() {
        let mut routing = RoutingGraph::new();

        let bus = routing.add_bus("Source");
        let fake_id = Uuid::new_v4();  // Non-existent

        routing.bus_mut(bus.id).unwrap().add_send(fake_id, 0.5);

        assert!(matches!(
            routing.validate(),
            Err(RoutingError::MissingSendTarget { .. })
        ));
    }

    #[test]
    fn test_render_order() {
        let mut routing = RoutingGraph::new();

        // Create bus hierarchy: track → bus_a → bus_b → master
        let bus_b = routing.add_bus("B");
        let bus_b_id = bus_b.id;

        let bus_a = routing.add_bus("A");
        let bus_a_id = bus_a.id;
        routing.bus_mut(bus_a_id).unwrap().output = OutputTarget::Bus(bus_b_id);

        let track_id = Uuid::new_v4();
        let track_routing = routing.ensure_routing(track_id);
        track_routing.output = OutputTarget::Bus(bus_a_id);

        let order = routing.render_order();

        // Track should come first
        assert_eq!(order[0], RenderNode::Track(track_id));

        // bus_a before bus_b (since A routes to B)
        let a_pos = order.iter().position(|n| *n == RenderNode::Bus(bus_a_id)).unwrap();
        let b_pos = order.iter().position(|n| *n == RenderNode::Bus(bus_b_id)).unwrap();
        assert!(a_pos < b_pos);

        // Master last
        assert_eq!(order.last(), Some(&RenderNode::Master));
    }

    #[test]
    fn test_sends() {
        let mut routing = RoutingGraph::new();

        let reverb = routing.add_bus("Reverb");
        let reverb_id = reverb.id;

        let track_id = Uuid::new_v4();
        let track_routing = routing.ensure_routing(track_id);
        track_routing.add_send(reverb_id, 0.3);

        assert!(routing.validate().is_ok());
        assert_eq!(track_routing.sends.len(), 1);
        assert_eq!(track_routing.sends[0].amount, 0.3);
    }
}
```

---

## Notes

### Why Separate RoutingGraph from Timeline?

Separation of concerns:
- **Timeline**: Content (what is played)
- **RoutingGraph**: Signal flow (how it's mixed)

This also enables:
- Saving routing separately from content
- Multiple routing configurations for same content
- Cleaner Trustfall queries (query content vs. query routing)

### Why Pre/Post Fader Sends?

- **Post-fader** (default): Send follows track volume. Turn down the track, send goes down too. Natural for most uses.
- **Pre-fader**: Send is constant regardless of track volume. Useful for monitor mixes or when you want reverb even on muted tracks.

### Why Validate Instead of Enforce?

We could prevent invalid routing at the type level, but that makes the API harder:
- Can't create a bus that routes to another bus created later
- Harder to deserialize saved projects

Instead, we allow any routing and validate before render. This is the same approach most DAWs use.
