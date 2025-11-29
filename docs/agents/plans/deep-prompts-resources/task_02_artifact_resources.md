# Task 02: Artifact & CAS Resources

**Goal**: Expose the artifact store so agents can query what's been generated, track variations, and understand refinement chains.

## Current State

```rust
// Only CAS content by hash:
ResourceTemplate::new("cas://{hash}", "cas-content")
```

No way to:
- List all artifacts
- Query by tag
- See variation sets
- Trace refinement chains

## Proposed Resources

### Static Resources

| URI | Description |
|-----|-------------|
| `artifacts://summary` | Aggregate stats (counts by type, phase, tool) |
| `artifacts://recent` | Most recently created artifacts |

### Resource Templates

| URI Template | Description |
|--------------|-------------|
| `artifacts://by-tag/{tag}` | All artifacts with a specific tag |
| `artifacts://by-creator/{creator}` | All artifacts by an agent |
| `artifacts://variation-set/{set_id}` | All artifacts in a variation set |
| `artifacts://lineage/{artifact_id}` | Parent chain for refinements |
| `artifacts://detail/{artifact_id}` | Full artifact with resolved CAS data |

## Implementation

### 1. Add new resources

```rust
fn resources(&self) -> Vec<Resource> {
    vec![
        // ... existing ...
        Resource::new("artifacts://summary", "artifact-summary")
            .with_description("Aggregate stats on all artifacts")
            .with_mime_type("application/json"),
        Resource::new("artifacts://recent", "recent-artifacts")
            .with_description("10 most recently created artifacts")
            .with_mime_type("application/json"),
    ]
}

fn resource_templates(&self) -> Vec<ResourceTemplate> {
    vec![
        // ... existing ...
        ResourceTemplate::new("artifacts://by-tag/{tag}", "artifacts-by-tag")
            .with_description("Filter artifacts by tag (e.g., type:midi, phase:generation)")
            .with_mime_type("application/json"),
        ResourceTemplate::new("artifacts://by-creator/{creator}", "artifacts-by-creator")
            .with_description("All artifacts created by a specific agent")
            .with_mime_type("application/json"),
        ResourceTemplate::new("artifacts://variation-set/{set_id}", "variation-set")
            .with_description("All artifacts in a variation set")
            .with_mime_type("application/json"),
        ResourceTemplate::new("artifacts://lineage/{artifact_id}", "artifact-lineage")
            .with_description("Parent chain showing refinement history")
            .with_mime_type("application/json"),
        ResourceTemplate::new("artifacts://detail/{artifact_id}", "artifact-detail")
            .with_description("Full artifact metadata with CAS info")
            .with_mime_type("application/json"),
    ]
}
```

### 2. Add `read_artifacts_resource()` helper

```rust
async fn read_artifacts_resource(&self, path: &str) -> Result<ReadResourceResult, ErrorData> {
    use crate::artifact_store::ArtifactStore;

    let store = &self.server.artifact_store;

    match path {
        "summary" => {
            let all = store.all()
                .map_err(|e| ErrorData::internal_error(e.to_string()))?;

            // Count by tag prefix
            let mut by_type: HashMap<String, usize> = HashMap::new();
            let mut by_phase: HashMap<String, usize> = HashMap::new();
            let mut by_tool: HashMap<String, usize> = HashMap::new();

            for artifact in &all {
                for tag in &artifact.tags {
                    if let Some(val) = tag.strip_prefix("type:") {
                        *by_type.entry(val.to_string()).or_insert(0) += 1;
                    } else if let Some(val) = tag.strip_prefix("phase:") {
                        *by_phase.entry(val.to_string()).or_insert(0) += 1;
                    } else if let Some(val) = tag.strip_prefix("tool:") {
                        *by_tool.entry(val.to_string()).or_insert(0) += 1;
                    }
                }
            }

            let result = serde_json::json!({
                "total": all.len(),
                "by_type": by_type,
                "by_phase": by_phase,
                "by_tool": by_tool,
                "variation_sets": count_variation_sets(&all),
            });
            Ok(as_json_resource("artifacts://summary", &result))
        }

        "recent" => {
            let mut all = store.all()
                .map_err(|e| ErrorData::internal_error(e.to_string()))?;
            all.sort_by(|a, b| b.created_at.cmp(&a.created_at));
            let recent: Vec<_> = all.into_iter().take(10).collect();

            let result: Vec<_> = recent.iter()
                .map(|a| artifact_to_json(a))
                .collect();
            Ok(as_json_resource("artifacts://recent", &result))
        }

        _ if path.starts_with("by-tag/") => {
            let tag = &path[7..];
            let all = store.all()
                .map_err(|e| ErrorData::internal_error(e.to_string()))?;
            let filtered: Vec<_> = all.into_iter()
                .filter(|a| a.has_tag(tag))
                .map(|a| artifact_to_json(&a))
                .collect();
            Ok(as_json_resource(&format!("artifacts://by-tag/{}", tag), &filtered))
        }

        _ if path.starts_with("by-creator/") => {
            let creator = &path[11..];
            let all = store.all()
                .map_err(|e| ErrorData::internal_error(e.to_string()))?;
            let filtered: Vec<_> = all.into_iter()
                .filter(|a| a.creator == creator)
                .map(|a| artifact_to_json(&a))
                .collect();
            Ok(as_json_resource(&format!("artifacts://by-creator/{}", creator), &filtered))
        }

        _ if path.starts_with("variation-set/") => {
            let set_id = &path[14..];
            let all = store.all()
                .map_err(|e| ErrorData::internal_error(e.to_string()))?;
            let mut filtered: Vec<_> = all.into_iter()
                .filter(|a| a.variation_set_id.as_deref() == Some(set_id))
                .collect();
            filtered.sort_by_key(|a| a.variation_index);

            let result = serde_json::json!({
                "set_id": set_id,
                "count": filtered.len(),
                "variations": filtered.iter().map(|a| artifact_to_json(a)).collect::<Vec<_>>(),
            });
            Ok(as_json_resource(&format!("artifacts://variation-set/{}", set_id), &result))
        }

        _ if path.starts_with("lineage/") => {
            let artifact_id = &path[8..];
            let all = store.all()
                .map_err(|e| ErrorData::internal_error(e.to_string()))?;

            // Build lineage chain
            let mut chain = Vec::new();
            let mut current_id = Some(artifact_id.to_string());

            while let Some(id) = current_id {
                if let Some(artifact) = all.iter().find(|a| a.id == id) {
                    chain.push(artifact_to_json(artifact));
                    current_id = artifact.parent_id.clone();
                } else {
                    break;
                }
            }
            chain.reverse(); // Root first

            let result = serde_json::json!({
                "artifact_id": artifact_id,
                "depth": chain.len(),
                "chain": chain,
            });
            Ok(as_json_resource(&format!("artifacts://lineage/{}", artifact_id), &result))
        }

        _ if path.starts_with("detail/") => {
            let artifact_id = &path[7..];
            let artifact = store.get(artifact_id)
                .map_err(|e| ErrorData::internal_error(e.to_string()))?
                .ok_or_else(|| ErrorData::invalid_params("Artifact not found"))?;

            // Include CAS info if available
            let cas_info = if let Some(hash) = artifact.data.get("hash").and_then(|h| h.as_str()) {
                // Get CAS metadata (size, mime type)
                Some(serde_json::json!({
                    "hash": hash,
                    "uri": format!("cas://{}", hash),
                }))
            } else {
                None
            };

            let result = serde_json::json!({
                "artifact": artifact_to_json(&artifact),
                "cas": cas_info,
            });
            Ok(as_json_resource(&format!("artifacts://detail/{}", artifact_id), &result))
        }

        _ => Err(ErrorData::invalid_params("Unknown artifacts resource"))
    }
}
```

### 3. Helper functions

```rust
fn artifact_to_json(a: &Artifact) -> serde_json::Value {
    serde_json::json!({
        "id": a.id,
        "creator": a.creator,
        "created_at": a.created_at.to_rfc3339(),
        "tags": a.tags,
        "variation_set_id": a.variation_set_id,
        "variation_index": a.variation_index,
        "parent_id": a.parent_id,
        "data": a.data,
    })
}

fn count_variation_sets(artifacts: &[Artifact]) -> usize {
    artifacts.iter()
        .filter_map(|a| a.variation_set_id.as_ref())
        .collect::<std::collections::HashSet<_>>()
        .len()
}

fn as_json_resource(uri: &str, value: &serde_json::Value) -> ReadResourceResult {
    ReadResourceResult::single(ResourceContents::text_with_mime(
        uri,
        serde_json::to_string_pretty(value).unwrap_or_default(),
        "application/json",
    ))
}
```

### 4. Update URI routing in `read_resource()`

```rust
async fn read_resource(&self, uri: &str) -> Result<ReadResourceResult, ErrorData> {
    let (scheme, path) = uri.split_once("://")
        .ok_or_else(|| ErrorData::invalid_params("Invalid URI"))?;

    match scheme {
        "graph" => self.read_graph_resource(path).await,
        "cas" => self.read_cas_resource(path).await,
        "session" => self.read_session_resource(path).await,
        "artifacts" => self.read_artifacts_resource(path).await,  // NEW
        _ => Err(ErrorData::invalid_params("Unknown URI scheme")),
    }
}
```

## Example Responses

### `artifacts://summary`
```json
{
  "total": 47,
  "by_type": {"midi": 42, "json": 5},
  "by_phase": {"generation": 30, "refinement": 12, "exploration": 5},
  "by_tool": {"orpheus_generate": 25, "orpheus_continue": 15, "orpheus_bridge": 7},
  "variation_sets": 8
}
```

### `artifacts://lineage/artifact_refined3`
```json
{
  "artifact_id": "artifact_refined3",
  "depth": 4,
  "chain": [
    {"id": "artifact_original", "tags": ["phase:generation"], ...},
    {"id": "artifact_refined1", "parent_id": "artifact_original", "tags": ["phase:refinement"], ...},
    {"id": "artifact_refined2", "parent_id": "artifact_refined1", ...},
    {"id": "artifact_refined3", "parent_id": "artifact_refined2", ...}
  ]
}
```

## Success Criteria

- [ ] Summary aggregation works
- [ ] Tag filtering works
- [ ] Variation set grouping works
- [ ] Lineage chain traversal works
- [ ] Tests verify all resource responses
