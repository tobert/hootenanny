# Task 03: Musical Context Resources

**Goal**: Expose MusicalContext so agents understand the "rules of the game" - tempo, key, harmony, and constraints.

## Current State

MusicalContext exists in `domain/context.rs` but is completely internal. Agents have no way to query:
- Current tempo/key/time signature
- Harmonic progression
- Active constraints (scale restrictions)
- Energy/complexity state

## Domain Model

```rust
// From domain/context.rs
pub struct MusicalContext {
    pub tempo: TimeMap<f64>,           // BPM over time
    pub key: TimeMap<Key>,             // Key changes
    pub time_signature: TimeMap<TimeSignature>,
    pub chord_progression: TimeMap<Chord>,
    pub constraints: TimeMap<Vec<Constraint>>,  // Scale/rhythm restrictions
    pub current_state: CurrentState,
}

pub struct CurrentState {
    pub emotional_state: EmotionalVector,
    pub energy_level: f64,
    pub complexity: f64,
}
```

## Proposed Resources

### Static Resources

| URI | Description |
|-----|-------------|
| `musical://context` | Current snapshot of all musical parameters |
| `musical://state` | Current emotional/energy/complexity state |

### Resource Templates

| URI Template | Description |
|--------------|-------------|
| `musical://tempo-map` | All tempo changes over time |
| `musical://key-map` | All key changes over time |
| `musical://chord-progression` | Harmonic progression over time |
| `musical://constraints` | Active scale/rhythm constraints |
| `musical://at/{timestamp}` | Musical context at a specific time |

## Implementation

### 1. Access MusicalContext

Need to add MusicalContext to EventDualityServer or derive it from ConversationState:

```rust
// Option A: Store in EventDualityServer
pub struct EventDualityServer {
    // ... existing ...
    pub musical_context: Arc<RwLock<MusicalContext>>,
}

// Option B: Derive from conversation tree (simpler for now)
impl HootHandler {
    fn derive_musical_context(&self) -> MusicalContext {
        let state = self.server.state.lock().unwrap();
        // Build context from node events
        let mut context = MusicalContext::default();

        for node in state.tree.nodes.values() {
            if let Event::Abstract(AbstractEvent::Constraint(c)) = &node.event {
                // Add constraint at node timestamp
                context.constraints.insert(node.timestamp, vec![c.clone()]);
            }
            // ... extract tempo/key from events if present
        }
        context.current_state.emotional_state = state.tree.nodes
            .get(&state.current_head)
            .map(|n| n.emotion.clone())
            .unwrap_or_default();

        context
    }
}
```

### 2. Add resources

```rust
fn resources(&self) -> Vec<Resource> {
    vec![
        // ... existing ...
        Resource::new("musical://context", "musical-context")
            .with_description("Current musical parameters (tempo, key, harmony)")
            .with_mime_type("application/json"),
        Resource::new("musical://state", "current-state")
            .with_description("Current emotional/energy/complexity state")
            .with_mime_type("application/json"),
    ]
}

fn resource_templates(&self) -> Vec<ResourceTemplate> {
    vec![
        // ... existing ...
        ResourceTemplate::new("musical://tempo-map", "tempo-changes")
            .with_description("Tempo changes over musical time")
            .with_mime_type("application/json"),
        ResourceTemplate::new("musical://key-map", "key-changes")
            .with_description("Key changes over musical time")
            .with_mime_type("application/json"),
        ResourceTemplate::new("musical://chord-progression", "harmony")
            .with_description("Chord progression over time")
            .with_mime_type("application/json"),
        ResourceTemplate::new("musical://constraints", "active-constraints")
            .with_description("Scale and rhythm constraints")
            .with_mime_type("application/json"),
    ]
}
```

### 3. Implement `read_musical_resource()`

```rust
async fn read_musical_resource(&self, path: &str) -> Result<ReadResourceResult, ErrorData> {
    let context = self.derive_musical_context();

    match path {
        "context" => {
            let result = serde_json::json!({
                "tempo": context.tempo.current().unwrap_or(120.0),
                "key": context.key.current().map(|k| k.to_string()),
                "time_signature": context.time_signature.current()
                    .map(|ts| format!("{}/{}", ts.numerator, ts.denominator)),
                "current_chord": context.chord_progression.current()
                    .map(|c| c.to_string()),
                "constraints": context.constraints.current()
                    .map(|cs| cs.iter().map(|c| constraint_to_json(c)).collect::<Vec<_>>())
                    .unwrap_or_default(),
                "state": {
                    "emotion": emotion_to_json(&context.current_state.emotional_state),
                    "energy": context.current_state.energy_level,
                    "complexity": context.current_state.complexity,
                },
            });
            Ok(as_json_resource("musical://context", &result))
        }

        "state" => {
            let result = serde_json::json!({
                "emotion": emotion_to_json(&context.current_state.emotional_state),
                "energy": context.current_state.energy_level,
                "complexity": context.current_state.complexity,
                "mood": describe_mood(&context.current_state.emotional_state),
            });
            Ok(as_json_resource("musical://state", &result))
        }

        "tempo-map" => {
            let changes: Vec<_> = context.tempo.iter()
                .map(|(time, tempo)| serde_json::json!({
                    "time": time,
                    "tempo": tempo,
                }))
                .collect();
            Ok(as_json_resource("musical://tempo-map", &changes))
        }

        "key-map" => {
            let changes: Vec<_> = context.key.iter()
                .map(|(time, key)| serde_json::json!({
                    "time": time,
                    "key": key.to_string(),
                    "mode": format!("{:?}", key.mode),
                }))
                .collect();
            Ok(as_json_resource("musical://key-map", &changes))
        }

        "chord-progression" => {
            let chords: Vec<_> = context.chord_progression.iter()
                .map(|(time, chord)| serde_json::json!({
                    "time": time,
                    "chord": chord.to_string(),
                    "root": chord.root.to_string(),
                    "quality": format!("{:?}", chord.quality),
                }))
                .collect();
            Ok(as_json_resource("musical://chord-progression", &chords))
        }

        "constraints" => {
            let constraints: Vec<_> = context.constraints.iter()
                .flat_map(|(time, cs)| {
                    cs.iter().map(move |c| serde_json::json!({
                        "time": time,
                        "constraint": constraint_to_json(c),
                    }))
                })
                .collect();
            Ok(as_json_resource("musical://constraints", &constraints))
        }

        _ => Err(ErrorData::invalid_params("Unknown musical resource"))
    }
}

fn constraint_to_json(c: &Constraint) -> serde_json::Value {
    match c {
        Constraint::Scale(scale) => serde_json::json!({
            "type": "scale",
            "root": scale.root.to_string(),
            "mode": format!("{:?}", scale.mode),
            "notes": scale.notes(),
        }),
        Constraint::Rhythm(pattern) => serde_json::json!({
            "type": "rhythm",
            "pattern": pattern,
        }),
        // ... other constraint types
    }
}
```

## Example Responses

### `musical://context`
```json
{
  "tempo": 128.0,
  "key": "D minor",
  "time_signature": "4/4",
  "current_chord": "Dm7",
  "constraints": [
    {"type": "scale", "root": "D", "mode": "Dorian", "notes": ["D", "E", "F", "G", "A", "B", "C"]}
  ],
  "state": {
    "emotion": {"valence": -0.2, "arousal": 0.6, "agency": 0.3, "mood": "tense/anxious"},
    "energy": 0.7,
    "complexity": 0.5
  }
}
```

### `musical://chord-progression`
```json
[
  {"time": 0, "chord": "Dm7", "root": "D", "quality": "Minor7"},
  {"time": 4000, "chord": "G7", "root": "G", "quality": "Dominant7"},
  {"time": 8000, "chord": "Cmaj7", "root": "C", "quality": "Major7"},
  {"time": 12000, "chord": "Am7", "root": "A", "quality": "Minor7"}
]
```

## Dependencies

- Need to either:
  1. Add `MusicalContext` to `EventDualityServer` (cleaner)
  2. Derive from conversation tree events (simpler implementation)

- May need to extend `Event` types to carry tempo/key changes

## Success Criteria

- [ ] Current context snapshot works
- [ ] Time-based queries work
- [ ] Constraints are readable
- [ ] Emotional state is exposed
- [ ] Tests verify resource responses
