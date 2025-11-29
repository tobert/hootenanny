# Task 04: Music-Aware Prompts

**Goal**: Create prompts that understand what's been generated and suggest informed next steps.

## Current State

Prompts only inject audio graph context (device list, connections). They don't know:
- What MIDI has been generated
- What variations exist
- What refinement chains are in progress
- The musical context (tempo, key, harmony)

## Proposed Prompts

### 1. `generate-continuation`
Suggest how to continue from existing MIDI.

```rust
Prompt::new("generate-continuation")
    .with_title("Continue MIDI")
    .with_description("Suggest how to extend or continue existing MIDI")
    .argument("hash", "CAS hash of MIDI to continue", true)
    .argument("bars", "Number of bars to add", false)
    .argument("direction", "build, wind-down, transition, develop", false)
```

### 2. `orchestrate-parts`
Suggest complementary parts for an ensemble.

```rust
Prompt::new("orchestrate-parts")
    .with_title("Orchestrate Parts")
    .with_description("Suggest parts to fill out an arrangement")
    .argument("base_hash", "Main MIDI track", true)
    .argument("roles", "Roles to fill (drums, bass, melody, pad)", false)
```

### 3. `explore-variations`
Guide variation exploration.

```rust
Prompt::new("explore-variations")
    .with_title("Explore Variations")
    .with_description("Suggest variation strategies for MIDI")
    .argument("hash", "MIDI to vary", true)
    .argument("intensity", "subtle, moderate, radical", false)
```

### 4. `analyze-generation`
Provide analysis of generated MIDI.

```rust
Prompt::new("analyze-generation")
    .with_title("Analyze Generation")
    .with_description("Analyze characteristics of generated MIDI")
    .argument("hash", "MIDI to analyze", true)
```

### 5. `next-in-session`
Suggest what to do next based on session state.

```rust
Prompt::new("next-in-session")
    .with_title("What's Next?")
    .with_description("Suggest next steps based on session progress")
    // No required arguments - reads full session state
```

## Implementation

### Add prompts to `fn prompts()`

```rust
fn prompts(&self) -> Vec<Prompt> {
    vec![
        // ... existing prompts ...

        Prompt::new("generate-continuation")
            .with_title("Continue MIDI")
            .with_description("Suggest how to extend or continue existing MIDI")
            .argument("hash", "CAS hash of MIDI to continue", true)
            .argument("bars", "Number of bars to add (default: 4)", false)
            .argument("direction", "build, wind-down, transition, develop (default: develop)", false),

        Prompt::new("orchestrate-parts")
            .with_title("Orchestrate Parts")
            .with_description("Suggest complementary parts for an arrangement")
            .argument("base_hash", "Main MIDI track (CAS hash)", true)
            .argument("roles", "Comma-separated roles: drums, bass, melody, pad, fx", false),

        Prompt::new("explore-variations")
            .with_title("Explore Variations")
            .with_description("Suggest variation strategies for existing MIDI")
            .argument("hash", "MIDI to vary (CAS hash)", true)
            .argument("intensity", "subtle, moderate, radical (default: moderate)", false),

        Prompt::new("analyze-generation")
            .with_title("Analyze Generation")
            .with_description("Analyze characteristics and suggest improvements")
            .argument("hash", "MIDI to analyze (CAS hash)", true),

        Prompt::new("next-in-session")
            .with_title("What's Next?")
            .with_description("Suggest next creative steps based on session state"),
    ]
}
```

### Implement in `get_prompt()`

```rust
async fn get_prompt(
    &self,
    name: &str,
    arguments: HashMap<String, String>,
) -> Result<GetPromptResult, ErrorData> {
    // Gather context
    let identities = graph_find(&self.server.audio_graph_db, None, None, None)
        .unwrap_or_default();
    let artifacts = self.server.artifact_store.all().unwrap_or_default();
    let midi_artifacts: Vec<_> = artifacts.iter()
        .filter(|a| a.has_tag("type:midi"))
        .collect();

    match name {
        // ... existing prompts ...

        "generate-continuation" => {
            let hash = arguments.get("hash")
                .ok_or_else(|| ErrorData::invalid_params("hash is required"))?;
            let bars = arguments.get("bars").map(|s| s.as_str()).unwrap_or("4");
            let direction = arguments.get("direction").map(|s| s.as_str()).unwrap_or("develop");

            // Find artifact for this hash
            let artifact = midi_artifacts.iter()
                .find(|a| a.data.get("hash").and_then(|h| h.as_str()) == Some(hash));

            let artifact_info = if let Some(a) = artifact {
                format!(
                    "Created by: {}\nTags: {}\nPhase: {}\nPart of variation set: {}",
                    a.creator,
                    a.tags.join(", "),
                    a.tags.iter().find(|t| t.starts_with("phase:")).unwrap_or(&"unknown".to_string()),
                    a.variation_set_id.as_deref().unwrap_or("none")
                )
            } else {
                "No metadata available".to_string()
            };

            let prompt_text = format!(
                "Continue this MIDI for {} more bars with a '{}' direction.\n\n\
                Source MIDI: cas://{}\n\
                {}\n\n\
                Available instruments:\n{}\n\n\
                Direction guide:\n\
                - build: Increase energy, add layers\n\
                - wind-down: Decrease energy, thin out\n\
                - transition: Prepare for a new section\n\
                - develop: Evolve the existing material\n\n\
                Use orpheus_continue with input_hash=\"{}\" to generate the continuation.",
                bars,
                direction,
                hash,
                artifact_info,
                format_devices(&identities),
                hash
            );

            Ok(GetPromptResult::new(vec![PromptMessage::user_text(prompt_text)])
                .with_description(format!("{} continuation ({} bars)", direction, bars)))
        }

        "orchestrate-parts" => {
            let base_hash = arguments.get("base_hash")
                .ok_or_else(|| ErrorData::invalid_params("base_hash is required"))?;
            let roles = arguments.get("roles").map(|s| s.as_str()).unwrap_or("bass, pad");

            // Find existing parts by role tag
            let existing_parts: HashMap<String, Vec<&Artifact>> = midi_artifacts.iter()
                .fold(HashMap::new(), |mut map, a| {
                    for tag in &a.tags {
                        if let Some(role) = tag.strip_prefix("role:") {
                            map.entry(role.to_string()).or_default().push(*a);
                        }
                    }
                    map
                });

            let existing_summary = if existing_parts.is_empty() {
                "No parts tagged with roles yet.".to_string()
            } else {
                existing_parts.iter()
                    .map(|(role, parts)| format!("- {}: {} variations", role, parts.len()))
                    .collect::<Vec<_>>()
                    .join("\n")
            };

            let prompt_text = format!(
                "Orchestrate complementary parts for the base track.\n\n\
                Base track: cas://{}\n\n\
                Requested roles: {}\n\n\
                Existing parts:\n{}\n\n\
                Available instruments:\n{}\n\n\
                For each role, consider:\n\
                - How it complements the base track\n\
                - Register and frequency range\n\
                - Rhythmic relationship\n\
                - Harmonic function\n\n\
                Use orpheus_generate or orpheus_generate_seeded with appropriate tags like \"role:bass\".",
                base_hash,
                roles,
                existing_summary,
                format_devices(&identities)
            );

            Ok(GetPromptResult::new(vec![PromptMessage::user_text(prompt_text)])
                .with_description(format!("Orchestrate: {}", roles)))
        }

        "explore-variations" => {
            let hash = arguments.get("hash")
                .ok_or_else(|| ErrorData::invalid_params("hash is required"))?;
            let intensity = arguments.get("intensity").map(|s| s.as_str()).unwrap_or("moderate");

            // Check if already in a variation set
            let artifact = midi_artifacts.iter()
                .find(|a| a.data.get("hash").and_then(|h| h.as_str()) == Some(hash));

            let variation_info = if let Some(a) = artifact {
                if let Some(set_id) = &a.variation_set_id {
                    let set_count = midi_artifacts.iter()
                        .filter(|v| v.variation_set_id.as_ref() == Some(set_id))
                        .count();
                    format!("Part of variation set '{}' with {} existing variations.", set_id, set_count)
                } else {
                    "Not in a variation set yet.".to_string()
                }
            } else {
                "No artifact metadata.".to_string()
            };

            let intensity_guide = match intensity {
                "subtle" => "Small changes: slight timing shifts, velocity variations, octave doubling",
                "moderate" => "Medium changes: melodic embellishment, rhythm augmentation, harmonic recoloring",
                "radical" => "Major changes: completely new interpretation while keeping core identity",
                _ => "Moderate variation",
            };

            let prompt_text = format!(
                "Create {} variations of this MIDI.\n\n\
                Source: cas://{}\n\
                {}\n\n\
                Intensity: {} - {}\n\n\
                Variation strategies:\n\
                - Melodic: Ornament, simplify, invert, sequence\n\
                - Rhythmic: Augment, diminish, syncopate, straighten\n\
                - Harmonic: Reharmonize, add extensions, substitute\n\
                - Textural: Layer, thin, transpose octave\n\n\
                Use orpheus_generate_seeded with seed_hash=\"{}\" and num_variations=3.",
                intensity,
                hash,
                variation_info,
                intensity,
                intensity_guide,
                hash
            );

            Ok(GetPromptResult::new(vec![PromptMessage::user_text(prompt_text)])
                .with_description(format!("{} variations", intensity)))
        }

        "analyze-generation" => {
            let hash = arguments.get("hash")
                .ok_or_else(|| ErrorData::invalid_params("hash is required"))?;

            let artifact = midi_artifacts.iter()
                .find(|a| a.data.get("hash").and_then(|h| h.as_str()) == Some(hash));

            let artifact_details = if let Some(a) = artifact {
                format!(
                    "Creator: {}\n\
                    Created: {}\n\
                    Tags: {}\n\
                    Model: {}\n\
                    Temperature: {}\n\
                    Tokens: {}",
                    a.creator,
                    a.created_at.to_rfc3339(),
                    a.tags.join(", "),
                    a.data.get("model").and_then(|v| v.as_str()).unwrap_or("unknown"),
                    a.data.get("temperature").and_then(|v| v.as_f64()).unwrap_or(0.0),
                    a.data.get("tokens").and_then(|v| v.as_u64()).unwrap_or(0)
                )
            } else {
                "No artifact metadata available.".to_string()
            };

            let prompt_text = format!(
                "Analyze this generated MIDI.\n\n\
                Hash: cas://{}\n\n\
                Artifact info:\n{}\n\n\
                Consider:\n\
                - Melodic contour and range\n\
                - Rhythmic density and patterns\n\
                - Harmonic implications\n\
                - Energy arc\n\
                - Potential improvements\n\n\
                If the MIDI feels machine-like, suggest how to humanize it.\n\
                If it's too chaotic, suggest how to add structure.",
                hash,
                artifact_details
            );

            Ok(GetPromptResult::new(vec![PromptMessage::user_text(prompt_text)])
                .with_description("MIDI analysis"))
        }

        "next-in-session" => {
            // Aggregate session state
            let total_midi = midi_artifacts.len();
            let by_phase: HashMap<String, usize> = midi_artifacts.iter()
                .fold(HashMap::new(), |mut map, a| {
                    for tag in &a.tags {
                        if let Some(phase) = tag.strip_prefix("phase:") {
                            *map.entry(phase.to_string()).or_insert(0) += 1;
                        }
                    }
                    map
                });

            let recent: Vec<_> = midi_artifacts.iter()
                .take(3)
                .map(|a| format!("- {} ({:?})", a.id, a.tags))
                .collect();

            let prompt_text = format!(
                "What should we do next in this session?\n\n\
                Session state:\n\
                - Total MIDI generated: {}\n\
                - By phase: {:?}\n\
                - Available instruments: {}\n\n\
                Recent activity:\n{}\n\n\
                Suggest next steps based on:\n\
                - What's been created so far\n\
                - What roles/parts might be missing\n\
                - Opportunities for variation or refinement\n\
                - Whether to explore new directions or develop existing material",
                total_midi,
                by_phase,
                identities.len(),
                recent.join("\n")
            );

            Ok(GetPromptResult::new(vec![PromptMessage::user_text(prompt_text)])
                .with_description("Session guidance"))
        }

        _ => { /* ... existing handling ... */ }
    }
}
```

### Helper functions

```rust
fn format_devices(identities: &[Identity]) -> String {
    if identities.is_empty() {
        "No devices registered.".to_string()
    } else {
        identities.iter()
            .map(|i| format!("- {} ({})", i.name, i.id))
            .collect::<Vec<_>>()
            .join("\n")
    }
}
```

## Example Prompt Outputs

### `generate-continuation` with direction=build
```
Continue this MIDI for 4 more bars with a 'build' direction.

Source MIDI: cas://5c735d76fe3537
Created by: agent_orpheus
Tags: type:midi, phase:generation, tool:orpheus_generate
Phase: generation
Part of variation set: vset_123

Available instruments:
- Roland JD-Xi (jdxi)
- Arturia Keystep Pro (keystep)

Direction guide:
- build: Increase energy, add layers

Use orpheus_continue with input_hash="5c735d76fe3537" to generate the continuation.
```

### `next-in-session`
```
What should we do next in this session?

Session state:
- Total MIDI generated: 12
- By phase: {"generation": 8, "refinement": 3, "exploration": 1}
- Available instruments: 2

Recent activity:
- artifact_5c735 (["type:midi", "phase:generation", "role:melody"])
- artifact_abc12 (["type:midi", "phase:refinement"])
- artifact_xyz78 (["type:midi", "phase:generation", "role:bass"])

Suggest next steps based on:
- What's been created so far
- What roles/parts might be missing
- Opportunities for variation or refinement
- Whether to explore new directions or develop existing material
```

## Success Criteria

- [ ] All 5 new prompts implemented
- [ ] Prompts correctly read artifact store
- [ ] Context injection is informative
- [ ] Prompts guide tool usage
- [ ] Tests verify prompt responses
