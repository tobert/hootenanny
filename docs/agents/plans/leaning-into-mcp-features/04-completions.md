# Phase 4: Completions

## Overview

Implement argument autocompletion for tool and prompt parameters. This improves discoverability and reduces errors when agents (or humans via CLI) are filling in tool arguments.

**MCP Method**: `completion/complete`

**Impact**: Medium - UX improvement across all tools with enumerable parameters

## Current State

Baton has `CompletionsCapability` defined but not wired:
```rust
pub struct CompletionsCapability {}  // Marker only
```

No `completion/complete` handler exists.

## Target State

When a client requests completions:
```json
{
  "method": "completion/complete",
  "params": {
    "ref": {
      "type": "ref/argument",
      "name": "orpheus_generate",
      "argumentName": "model"
    },
    "argument": {
      "name": "model",
      "value": "ba"  // Partial input
    }
  }
}
```

Server responds:
```json
{
  "completion": {
    "values": ["base", "children", "mono_melodies", "bridge"],
    "total": 4,
    "hasMore": false
  }
}
```

## Use Cases

### High-Value Completions

| Tool | Argument | Completable Values |
|------|----------|-------------------|
| `orpheus_*` | `model` | base, children, mono_melodies, bridge |
| `convert_midi_to_wav` | `soundfont_hash` | Recently used soundfonts from CAS |
| `orpheus_continue` | `input_hash` | Recent MIDI artifacts |
| `graph_bind` | `hints[].kind` | usb_device_id, midi_name, alsa_card, pipewire_name |
| `graph_tag` | `namespace` | Existing tag namespaces |
| `graph_tag` | `value` | Existing values for that namespace |
| `graph_find` | `tag_namespace` | All known namespaces |
| `abc_to_midi` | `channel` | 0-15 with descriptions (9 = drums) |
| `add_annotation` | `artifact_id` | Recent artifact IDs |
| `clap_analyze` | `tasks` | embeddings, genre, mood, zero_shot, similarity |
| `anticipatory_*` | `model_size` | small, medium, large |

### Prompt Argument Completions

| Prompt | Argument | Completable Values |
|--------|----------|-------------------|
| `ensemble-jam` | `style` | ambient, techno, jazz, experimental, ... |
| `patch-synth` | `style` | pad, lead, bass, fx |
| `patch-synth` | `character` | warm, bright, dark, aggressive |
| `patch-synth` | `synth_id` | Known synth identities |
| `explore-variations` | `intensity` | subtle, moderate, radical |

## Implementation Plan

### Step 1: Add Completion Types to Baton

**File**: `crates/baton/src/types/completion.rs` (new)

```rust
//! Completion Types
//!
//! Types for argument autocompletion.
//! Per MCP 2025-06-18 schema.

use serde::{Deserialize, Serialize};

/// Reference to what we're completing
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum CompletionRef {
    /// Completing a prompt argument
    #[serde(rename = "ref/prompt")]
    Prompt {
        /// Prompt name
        name: String,
    },

    /// Completing a resource URI
    #[serde(rename = "ref/resource")]
    Resource {
        /// Resource URI (may be partial)
        uri: String,
    },

    /// Completing a tool argument
    #[serde(rename = "ref/argument")]
    Argument {
        /// Tool name
        name: String,
        /// Argument being completed
        argument_name: String,
    },
}

/// The argument being completed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionArgument {
    /// Argument name
    pub name: String,
    /// Current partial value
    pub value: String,
}

/// Completion request parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompleteParams {
    /// What we're completing
    #[serde(rename = "ref")]
    pub reference: CompletionRef,
    /// The argument (for argument completions)
    pub argument: CompletionArgument,
}

/// Completion result
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionResult {
    /// Suggested completions
    pub values: Vec<String>,
    /// Total number of matches (may be > values.len())
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<usize>,
    /// Whether there are more completions available
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_more: Option<bool>,
}

impl CompletionResult {
    pub fn new(values: Vec<String>) -> Self {
        Self {
            total: Some(values.len()),
            has_more: Some(false),
            values,
        }
    }

    pub fn empty() -> Self {
        Self {
            values: vec![],
            total: Some(0),
            has_more: Some(false),
        }
    }
}
```

### Step 2: Add Completion Handler to Trait

**File**: `crates/baton/src/protocol/mod.rs`

```rust
#[async_trait]
pub trait Handler: Send + Sync + 'static {
    // ... existing methods ...

    /// Provide argument completions
    async fn complete(
        &self,
        params: CompleteParams,
    ) -> Result<CompletionResult, ErrorData> {
        // Default: no completions
        Ok(CompletionResult::empty())
    }
}
```

### Step 3: Wire Completion into Dispatch

**File**: `crates/baton/src/protocol/mod.rs`

```rust
async fn dispatch_inner<H: Handler>(/* ... */) -> Result<Value, ErrorData> {
    match message.method.as_str() {
        // ... existing handlers ...

        "completion/complete" => handle_complete(state, message).await,

        // ...
    }
}

async fn handle_complete<H: Handler>(
    state: &Arc<McpState<H>>,
    request: &JsonRpcMessage,
) -> Result<Value, ErrorData> {
    let params: CompleteParams = request
        .params
        .as_ref()
        .map(|p| serde_json::from_value(p.clone()))
        .transpose()
        .map_err(|e| ErrorData::invalid_params(format!("Invalid complete params: {}", e)))?
        .ok_or_else(|| ErrorData::invalid_params("Missing complete params"))?;

    let result = state.handler.complete(params).await?;

    serde_json::to_value(&serde_json::json!({ "completion": result }))
        .map_err(|e| ErrorData::internal_error(e.to_string()))
}
```

### Step 4: Enable Completions Capability

**File**: `crates/baton/src/types/protocol.rs`

```rust
impl ServerCapabilities {
    /// Enable completions.
    pub fn enable_completions(mut self) -> Self {
        self.completions = Some(CompletionsCapability::default());
        self
    }
}
```

**File**: `crates/hootenanny/src/api/handler.rs`

```rust
fn capabilities(&self) -> ServerCapabilities {
    ServerCapabilities::default()
        .enable_tools()
        .enable_resources()
        .enable_prompts()
        .enable_completions()  // Add this
}
```

### Step 5: Implement Completions in HootHandler

**File**: `crates/hootenanny/src/api/handler.rs`

```rust
async fn complete(&self, params: CompleteParams) -> Result<CompletionResult, ErrorData> {
    match params.reference {
        CompletionRef::Argument { name, argument_name } => {
            self.complete_tool_argument(&name, &argument_name, &params.argument.value).await
        }
        CompletionRef::Prompt { name } => {
            self.complete_prompt_argument(&name, &params.argument.name, &params.argument.value).await
        }
        CompletionRef::Resource { uri } => {
            self.complete_resource_uri(&uri, &params.argument.value).await
        }
    }
}

async fn complete_tool_argument(
    &self,
    tool: &str,
    arg: &str,
    partial: &str,
) -> Result<CompletionResult, ErrorData> {
    let values = match (tool, arg) {
        // Orpheus model variants
        (t, "model") if t.starts_with("orpheus_") => {
            vec!["base", "children", "mono_melodies", "bridge"]
        }

        // Anticipatory model sizes
        (t, "model_size") if t.starts_with("anticipatory_") => {
            vec!["small", "medium", "large"]
        }

        // MIDI channel with descriptions
        ("abc_to_midi", "channel") => {
            (0..16).map(|i| {
                if i == 9 { format!("{} (drums)", i) }
                else { i.to_string() }
            }).collect()
        }

        // Graph hint kinds
        ("graph_bind", "hints") => {
            vec!["usb_device_id", "midi_name", "alsa_card", "pipewire_name"]
        }

        // Tag namespaces from existing data
        ("graph_tag", "namespace") | ("graph_find", "tag_namespace") => {
            self.get_existing_tag_namespaces().await
        }

        // CLAP analysis tasks
        ("clap_analyze", "tasks") => {
            vec!["embeddings", "genre", "mood", "zero_shot", "similarity"]
        }

        // Hash arguments - suggest recent artifacts
        (_, arg) if arg.ends_with("_hash") || arg == "hash" => {
            self.get_recent_hashes(10).await
        }

        // Artifact IDs
        (_, "artifact_id") | (_, "parent_id") => {
            self.get_recent_artifact_ids(10).await
        }

        _ => return Ok(CompletionResult::empty()),
    };

    // Filter by partial input
    let filtered: Vec<String> = values
        .into_iter()
        .map(String::from)
        .filter(|v| v.to_lowercase().starts_with(&partial.to_lowercase()))
        .collect();

    Ok(CompletionResult::new(filtered))
}

async fn complete_prompt_argument(
    &self,
    prompt: &str,
    arg: &str,
    partial: &str,
) -> Result<CompletionResult, ErrorData> {
    let values = match (prompt, arg) {
        ("ensemble-jam", "style") | ("sequence-idea", "style") => {
            vec!["ambient", "techno", "jazz", "experimental", "cinematic", "electronic", "classical"]
        }
        ("patch-synth", "style") => {
            vec!["pad", "lead", "bass", "fx", "keys", "strings"]
        }
        ("patch-synth", "character") => {
            vec!["warm", "bright", "dark", "aggressive", "ethereal", "gritty"]
        }
        ("patch-synth", "synth_id") => {
            self.get_synth_identities().await
        }
        ("explore-variations", "intensity") => {
            vec!["subtle", "moderate", "radical"]
        }
        ("generate-continuation", "direction") => {
            vec!["build", "wind-down", "transition", "develop"]
        }
        _ => return Ok(CompletionResult::empty()),
    };

    let filtered: Vec<String> = values
        .into_iter()
        .map(String::from)
        .filter(|v| v.to_lowercase().starts_with(&partial.to_lowercase()))
        .collect();

    Ok(CompletionResult::new(filtered))
}

async fn complete_resource_uri(&self, uri: &str, partial: &str) -> Result<CompletionResult, ErrorData> {
    // Suggest resource URI completions
    let all_uris = vec![
        "graph://identities",
        "graph://connections",
        "artifacts://summary",
        "artifacts://recent",
    ];

    let filtered: Vec<String> = all_uris
        .into_iter()
        .map(String::from)
        .filter(|u| u.starts_with(partial))
        .collect();

    Ok(CompletionResult::new(filtered))
}
```

### Step 6: Helper Methods for Dynamic Completions

**File**: `crates/hootenanny/src/api/handler.rs`

```rust
impl HootHandler {
    async fn get_existing_tag_namespaces(&self) -> Vec<String> {
        // Query graph for distinct namespaces
        graph_find(&self.server.audio_graph_db, None, None, None)
            .unwrap_or_default()
            .into_iter()
            .flat_map(|i| i.tags)
            .filter_map(|t| t.split(':').next().map(String::from))
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect()
    }

    async fn get_recent_hashes(&self, limit: usize) -> Vec<String> {
        use crate::artifact_store::ArtifactStore;

        self.server.artifact_store.read()
            .ok()
            .and_then(|store| store.all().ok())
            .map(|mut artifacts| {
                artifacts.sort_by(|a, b| b.created_at.cmp(&a.created_at));
                artifacts.into_iter()
                    .take(limit)
                    .map(|a| a.content_hash.as_str().to_string())
                    .collect()
            })
            .unwrap_or_default()
    }

    async fn get_recent_artifact_ids(&self, limit: usize) -> Vec<String> {
        use crate::artifact_store::ArtifactStore;

        self.server.artifact_store.read()
            .ok()
            .and_then(|store| store.all().ok())
            .map(|mut artifacts| {
                artifacts.sort_by(|a, b| b.created_at.cmp(&a.created_at));
                artifacts.into_iter()
                    .take(limit)
                    .map(|a| a.id.as_str().to_string())
                    .collect()
            })
            .unwrap_or_default()
    }

    async fn get_synth_identities(&self) -> Vec<String> {
        graph_find(&self.server.audio_graph_db, None, Some("type"), Some("synth"))
            .unwrap_or_default()
            .into_iter()
            .map(|i| i.id)
            .collect()
    }
}
```

### Step 7: Unit Tests

**File**: `crates/baton/src/types/completion_tests.rs`

```rust
#[test]
fn test_completion_ref_serialization() {
    let ref_arg = CompletionRef::Argument {
        name: "orpheus_generate".to_string(),
        argument_name: "model".to_string(),
    };
    let json = serde_json::to_value(&ref_arg).unwrap();
    assert_eq!(json["type"], "ref/argument");
    assert_eq!(json["name"], "orpheus_generate");
}

#[test]
fn test_completion_result() {
    let result = CompletionResult::new(vec!["base".to_string(), "bridge".to_string()]);
    assert_eq!(result.total, Some(2));
    assert!(!result.has_more.unwrap());
}
```

**File**: `crates/hootenanny/src/api/completion_tests.rs`

```rust
#[tokio::test]
async fn test_orpheus_model_completion() {
    let handler = create_test_handler();
    let params = CompleteParams {
        reference: CompletionRef::Argument {
            name: "orpheus_generate".to_string(),
            argument_name: "model".to_string(),
        },
        argument: CompletionArgument {
            name: "model".to_string(),
            value: "ba".to_string(),
        },
    };

    let result = handler.complete(params).await.unwrap();
    assert!(result.values.contains(&"base".to_string()));
    assert!(!result.values.contains(&"children".to_string())); // Doesn't start with "ba"
}
```

### Step 8: Live Testing

1. Rebuild and reconnect MCP
2. Verify capabilities include completions
3. Send `completion/complete` for various tools
4. Verify partial matches are filtered correctly
5. Test dynamic completions (artifact IDs change as you create them)

## Files Changed

| File | Change |
|------|--------|
| `crates/baton/src/types/completion.rs` | New - completion types |
| `crates/baton/src/types/mod.rs` | Export completion |
| `crates/baton/src/types/protocol.rs` | Add enable_completions() |
| `crates/baton/src/protocol/mod.rs` | Add complete handler, dispatch |
| `crates/hootenanny/src/api/handler.rs` | Implement complete() |

## Verification Checklist

- [ ] Completion types compile
- [ ] CompletionsCapability advertised in initialize
- [ ] completion/complete dispatches correctly
- [ ] Static completions work (model names, styles)
- [ ] Dynamic completions work (artifact IDs, hashes)
- [ ] Partial matching filters correctly
- [ ] Empty completions returned gracefully
- [ ] Unit tests pass
- [ ] Live test shows completions

## Notes for Next Agent

After this phase:
- All tools with enumerable parameters offer completions
- Dynamic completions pull from live data (artifacts, graph)
- The completion infrastructure is established

Phase 5 (logging) adds structured log streaming - a smaller feature. Consider combining with Phase 6 if time permits.
