# Task 05: MCP Tool Interface

**Status**: ✅ Complete (5 tests passing)
**Estimated effort**: 3-4 hours
**Prerequisites**: Task 04 (Trustfall adapter)
**Depends on**: Query engine, database, matcher
**Enables**: Agent interaction with audio graph

## 🎯 Goal

Expose the audio graph through MCP tools that agents can call. Provide both powerful (`graph_query` for arbitrary GraphQL) and convenient (`graph_find`, `graph_bind`) interfaces.

## 📋 Tools to Implement

### Core Query Tool

```rust
#[tool]
async fn graph_query(
    /// GraphQL query string
    query: String,
    /// Optional query variables as JSON
    variables: Option<serde_json::Value>,
) -> Result<Vec<serde_json::Value>, String>;
```

**Example usage by agent:**
```
Use graph_query with query:
{
    AlsaMidiDevice {
        name @output
        identity { name @output }
    }
}
```

### Convenience Tools

```rust
#[tool]
async fn graph_find(
    /// Fuzzy name match
    name: Option<String>,
    /// Tags to filter by (e.g., ["manufacturer:roland", "role:sound-source"])
    tags: Option<Vec<String>>,
) -> Result<Vec<DeviceSummary>, String>;

#[tool]
async fn graph_bind(
    /// Live device reference: "alsa:hw:2,0"
    device: String,
    /// Identity ID to bind to, or "new" to create
    identity: String,
    /// Name if creating new identity
    name: Option<String>,
) -> Result<Identity, String>;

#[tool]
async fn graph_unbind(
    /// Identity ID to remove
    identity: String,
) -> Result<(), String>;

#[tool]
async fn graph_unbound() -> Result<Vec<UnboundDevice>, String>;

#[tool]
async fn graph_tag(
    /// Identity ID
    identity: String,
    /// Tags to add (e.g., ["manufacturer:roland", "role:sound-source"])
    add: Option<Vec<String>>,
    /// Tags to remove
    remove: Option<Vec<String>>,
) -> Result<Identity, String>;

#[tool]
async fn graph_note(
    /// Target reference: "identity:jdxi" or "port:alsa:hw:2,0:0"
    target: String,
    /// Note message
    message: String,
) -> Result<Note, String>;

/// Suggest tags from controlled vocabulary (Gemini review feedback)
/// Helps agents use consistent terminology instead of inventing new tags
#[tool]
async fn graph_suggest_tags(
    /// Tag namespace to get suggestions for (e.g., "role", "manufacturer", "capability")
    namespace: String,
) -> Result<Vec<String>, String>;
```

## 🏗️ Module Structure

```
src/
├── mcp_tools/
│   ├── mod.rs
│   ├── query.rs        # graph_query
│   ├── find.rs         # graph_find convenience wrapper
│   ├── identity.rs     # graph_bind, graph_unbind, graph_unbound
│   └── annotate.rs     # graph_tag, graph_note
```

## 🔨 Implementation (src/mcp_tools/query.rs)

```rust
use trustfall::execute_query;
use serde_json::Value;
use std::sync::Arc;
use std::collections::HashMap;

use crate::adapter::AudioGraphAdapter;

pub async fn graph_query(
    adapter: Arc<AudioGraphAdapter>,
    query: String,
    variables: Option<Value>,
) -> Result<Vec<Value>, String> {
    let vars = if let Some(v) = variables {
        serde_json::from_value(v).map_err(|e| format!("Invalid variables: {}", e))?
    } else {
        HashMap::new()
    };

    let results = execute_query(adapter.schema(), adapter.clone(), &query, vars)
        .map_err(|e| format!("Query execution failed: {}", e))?
        .map(|r| serde_json::to_value(r).unwrap())
        .collect();

    Ok(results)
}
```

## 🔨 Implementation (src/mcp_tools/identity.rs)

```rust
pub async fn graph_bind(
    db: Arc<Database>,
    alsa: Arc<AlsaSource>,
    device_ref: String,
    identity: String,
    name: Option<String>,
) -> Result<Identity, String> {
    // Parse device reference: "alsa:hw:2,0"
    let (source, device_id) = device_ref.split_once(':')
        .ok_or("Invalid device reference (expected 'alsa:hw:X,Y')")?;

    if source != "alsa" {
        return Err(format!("Unsupported source: {}", source));
    }

    // Find the live device
    let devices = alsa.enumerate_devices()
        .map_err(|e| format!("Failed to enumerate devices: {}", e))?;

    let device = devices.iter()
        .find(|d| d.hardware_id == device_id)
        .ok_or_else(|| format!("Device not found: {}", device_id))?;

    // Create or get identity
    let identity = if identity == "new" {
        let name = name.ok_or("Name required when creating new identity")?;
        let id = format!("{}", uuid::Uuid::new_v4());
        db.create_identity(&id, &name, serde_json::json!({}))
            .map_err(|e| format!("Failed to create identity: {}", e))?
    } else {
        db.get_identity(&identity)
            .map_err(|e| format!("Failed to get identity: {}", e))?
            .ok_or_else(|| format!("Identity not found: {}", identity))?
    };

    // Extract fingerprints and add as hints
    let fingerprints = alsa.extract_fingerprints(device);
    for print in fingerprints {
        db.add_hint(&identity.id, print.kind, &print.value, 1.0)
            .map_err(|e| format!("Failed to add hint: {}", e))?;
    }

    // Log to changelog
    db.log_event(
        "mcp_tool",
        "identity_bind",
        "identity",
        &identity.id,
        serde_json::json!({
            "device": device_ref,
            "fingerprints": fingerprints.len(),
        }),
    ).ok();

    Ok(identity)
}

pub async fn graph_unbound(
    db: Arc<Database>,
    alsa: Arc<AlsaSource>,
) -> Result<Vec<UnboundDevice>, String> {
    let devices = alsa.enumerate_devices()
        .map_err(|e| format!("Failed to enumerate: {}", e))?;

    let matcher = IdentityMatcher::new(&db);
    let mut unbound = Vec::new();

    for device in devices {
        let fingerprints = alsa.extract_fingerprints(&device);
        let best_match = matcher.best_match(&fingerprints)
            .map_err(|e| format!("Matching failed: {}", e))?;

        // Include if no match or low confidence
        if best_match.is_none() || best_match.as_ref().unwrap().confidence != MatchConfidence::High {
            unbound.push(UnboundDevice {
                source: "alsa".into(),
                raw_name: device.name.clone(),
                device_ref: format!("alsa:{}", device.hardware_id),
                fingerprints,
                best_match: best_match.as_ref().map(|m| m.identity.clone()),
                best_match_score: best_match.map(|m| m.score),
            });
        }
    }

    Ok(unbound)
}
```

## 🧪 Testing

```rust
#[tokio::test]
async fn test_graph_query_basic() {
    let db = setup_test_db();
    let adapter = Arc::new(AudioGraphAdapter::new(db).unwrap());

    let query = r#"
        { AlsaMidiDevice { name @output } }
    "#.to_string();

    let results = graph_query(adapter, query, None).await.unwrap();
    assert!(!results.is_empty());
}

#[tokio::test]
async fn test_graph_bind_new_identity() {
    let db = setup_test_db();
    let alsa = Arc::new(AlsaSource::new());

    // Bind first virmidi device
    let identity = graph_bind(
        db.clone(),
        alsa.clone(),
        "alsa:hw:4,0".into(),
        "new".into(),
        Some("Test Synth".into()),
    ).await.unwrap();

    assert_eq!(identity.name, "Test Synth");

    // Verify hints were added
    let hints = db.get_hints(&identity.id).unwrap();
    assert!(!hints.is_empty());
}

#[tokio::test]
async fn test_graph_unbound() {
    let db = setup_test_db();
    let alsa = Arc::new(AlsaSource::new());

    let unbound = graph_unbound(db, alsa).await.unwrap();

    // With virmidi but no identities, all should be unbound
    assert!(!unbound.is_empty());
}
```

## ✅ Acceptance Criteria

1. ✅ `graph_query` executes arbitrary Trustfall queries
2. ✅ `graph_find` provides convenient device search
3. ✅ `graph_bind` creates identity and adds hints
4. ✅ `graph_unbound` returns devices without matches
5. ✅ `graph_tag` adds/removes tags
6. ✅ All tools return JSON-serializable results
7. ✅ Errors are user-friendly strings

## 💡 Integration with Hootenanny

Add to `crates/hootenanny/src/mcp_tools/mod.rs`:

```rust
pub mod audio_graph {
    pub use audio_graph_mcp::mcp_tools::*;
}
```

Register tools in MCP server initialization.

## 🎬 Next Task

**[Task 06: PipeWire Integration](task-06-pipewire-integration.md)** - Add audio routing visibility
