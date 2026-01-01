# 14: Wire Garden MCP Tools

**Focus:** Expose all daemon functionality via MCP tools
**Status:** Planning

---

## The Gap

The daemon supports 18 shell request types, but only 8 are exposed via MCP:

### Currently Exposed (9)
| MCP Tool | Daemon Request | Status |
|----------|---------------|--------|
| `garden_status` | GetSnapshot | ✅ Working |
| `garden_play` | Play | ✅ Working |
| `garden_pause` | Pause | ✅ Working |
| `garden_stop` | Stop | ✅ Working |
| `garden_seek` | Seek | ✅ Working |
| `garden_set_tempo` | SetTempo | ✅ Working |
| `garden_query` | (Trustfall query) | ✅ Working |
| `garden_emergency_pause` | (Control) | ✅ Working |
| `schedule` | CreateRegion | ✅ Working |

#### `schedule` Tool Parameters

The `schedule` tool creates regions on the timeline. All timing uses **beats** (not seconds):

| Parameter | Type | Description |
|-----------|------|-------------|
| `encoding` | object | Content reference: `{type: "midi", artifact_id: "..."}` |
| `at` | float | Start position in beats (maps to Region.position) |
| `duration` | float | Length in beats (optional, auto-detected from content) |
| `gain` | float | Volume multiplier (optional) |
| `rate` | float | Playback rate (optional) |

Example:
```json
{
  "encoding": {"type": "midi", "artifact_id": "artifact_abc123"},
  "at": 0,
  "duration": 4
}
```

Returns: `{region_id, position, duration, artifact_id}`

### Missing MCP Tools (10)

#### Region Operations
| MCP Tool | Daemon Request | Priority |
|----------|---------------|----------|
| `garden_create_region` | CreateRegion | **High** |
| `garden_delete_region` | DeleteRegion | **High** |
| `garden_move_region` | MoveRegion | Medium |
| `garden_get_regions` | GetRegions | **High** |

#### Latent Lifecycle (agent → daemon)
| MCP Tool | Daemon Request | Priority |
|----------|---------------|----------|
| `garden_latent_started` | UpdateLatentStarted | **High** |
| `garden_latent_progress` | UpdateLatentProgress | Medium |
| `garden_latent_resolved` | UpdateLatentResolved | **High** |
| `garden_latent_failed` | UpdateLatentFailed | **High** |

#### Approval Operations (HITL)
| MCP Tool | Daemon Request | Priority |
|----------|---------------|----------|
| `garden_get_pending` | GetPendingApprovals | **High** |
| `garden_approve` | ApproveLatent | **High** |
| `garden_reject` | RejectLatent | **High** |

#### Graph Operations (future)
| MCP Tool | Daemon Request | Priority |
|----------|---------------|----------|
| `garden_add_node` | AddNode | Low (not implemented in daemon) |

---

## Implementation Plan

### Phase 1: Region Tools (High Priority)

Add to `crates/hootenanny/src/api/tools/garden.rs`:

```rust
/// Request to create a region on the timeline
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GardenCreateRegionRequest {
    /// Position in beats
    pub position: f64,
    /// Duration in beats
    pub duration: f64,
    /// Behavior type: "play_content" or "latent"
    pub behavior_type: String,
    /// For play_content: artifact_id. For latent: job_id
    pub content_id: String,
}

/// Request to delete a region
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GardenDeleteRegionRequest {
    /// Region UUID
    pub region_id: String,
}

/// Request to move a region
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GardenMoveRegionRequest {
    /// Region UUID
    pub region_id: String,
    /// New position in beats
    pub new_position: f64,
}

/// Request to get regions in a range
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GardenGetRegionsRequest {
    /// Optional start beat (inclusive)
    pub start: Option<f64>,
    /// Optional end beat (exclusive)
    pub end: Option<f64>,
}
```

### Phase 2: Latent Lifecycle Tools

```rust
/// Notify daemon that a latent job has started
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GardenLatentStartedRequest {
    pub region_id: String,
    pub job_id: String,
}

/// Update progress on a latent job
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GardenLatentProgressRequest {
    pub region_id: String,
    /// Progress 0.0 to 1.0
    pub progress: f32,
}

/// Notify daemon that a latent job has resolved
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GardenLatentResolvedRequest {
    pub region_id: String,
    pub artifact_id: String,
    pub content_hash: String,
    /// "audio" or "midi"
    pub content_type: String,
}

/// Notify daemon that a latent job has failed
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GardenLatentFailedRequest {
    pub region_id: String,
    pub error: String,
}
```

### Phase 3: Approval Tools

```rust
/// Approve a resolved latent region for playback
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GardenApproveRequest {
    pub region_id: String,
    /// Who made the decision (agent/user UUID)
    pub decided_by: String,
}

/// Reject a resolved latent region
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GardenRejectRequest {
    pub region_id: String,
    pub decided_by: String,
    pub reason: Option<String>,
}
```

---

## GardenManager Extensions

Add convenience methods to `crates/hootenanny/src/zmq/manager.rs`:

```rust
// Region operations
pub async fn create_region(&self, position: f64, duration: f64, behavior: Behavior) -> Result<ShellReply>
pub async fn delete_region(&self, region_id: Uuid) -> Result<ShellReply>
pub async fn move_region(&self, region_id: Uuid, new_position: f64) -> Result<ShellReply>
pub async fn get_regions(&self, range: Option<(f64, f64)>) -> Result<ShellReply>

// Latent lifecycle
pub async fn latent_started(&self, region_id: Uuid, job_id: &str) -> Result<ShellReply>
pub async fn latent_progress(&self, region_id: Uuid, progress: f32) -> Result<ShellReply>
pub async fn latent_resolved(&self, region_id: Uuid, artifact_id: &str, content_hash: &str, content_type: ContentType) -> Result<ShellReply>
pub async fn latent_failed(&self, region_id: Uuid, error: &str) -> Result<ShellReply>

// Approvals
pub async fn get_pending_approvals(&self) -> Result<ShellReply>
pub async fn approve(&self, region_id: Uuid, decided_by: Uuid) -> Result<ShellReply>
pub async fn reject(&self, region_id: Uuid, decided_by: Uuid, reason: Option<&str>) -> Result<ShellReply>
```

---

## Dispatch Wiring

Add to `crates/hootenanny/src/api/dispatch.rs`:

1. Tool definitions in `list_tools()`
2. Match arms in `call_tool()`

---

## Luanette Bindings (Parallel with MCP)

Create `crates/luanette/src/stdlib/garden.rs` to provide the `garden.*` Lua namespace.

### Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                         Luanette                                │
│  ┌─────────────────┐                                           │
│  │  Lua Script     │                                           │
│  │  garden.play()  │                                           │
│  └────────┬────────┘                                           │
│           │                                                     │
│  ┌────────▼────────┐    ┌──────────────────┐                   │
│  │ stdlib/garden.rs│───►│ GardenClient     │                   │
│  │ (Lua bindings)  │    │ (ZMQ IPC)        │                   │
│  └─────────────────┘    └────────┬─────────┘                   │
└─────────────────────────────────│───────────────────────────────┘
                                  │ IPC
                    ┌─────────────▼─────────────┐
                    │      chaosgarden          │
                    │      (daemon)             │
                    └───────────────────────────┘
```

### Lua API Design

```lua
-- garden.* namespace mirrors MCP tools exactly

-- Transport (existing in MCP)
garden.status()                          -- → {playing, position, tempo}
garden.play()                            -- → ok
garden.pause()                           -- → ok
garden.stop()                            -- → ok
garden.seek(beat)                        -- → ok
garden.set_tempo(bpm)                    -- → ok
garden.query(graphql, variables)         -- → results table

-- Region Operations (new)
garden.create_region(opts)               -- → region_id
-- opts = {position=0.0, duration=4.0, behavior="play_content", content_id="hash_xxx"}
-- opts = {position=0.0, duration=4.0, behavior="latent", content_id="job_xxx"}

garden.delete_region(region_id)          -- → ok
garden.move_region(region_id, position)  -- → ok
garden.get_regions(start_beat, end_beat) -- → [{region_id, position, duration, is_latent, ...}]

-- Latent Lifecycle (new)
garden.latent_started(region_id, job_id) -- → ok
garden.latent_progress(region_id, 0.5)   -- → ok
garden.latent_resolved(region_id, opts)  -- → ok
-- opts = {artifact_id="xxx", content_hash="yyy", content_type="audio"|"midi"}

garden.latent_failed(region_id, error)   -- → ok

-- Approval Operations (new)
garden.get_pending()                     -- → [{region_id, artifact_id, content_hash, content_type}]
garden.approve(region_id, decided_by)    -- → ok
garden.reject(region_id, decided_by, reason) -- → ok
```

### Implementation: `crates/luanette/src/stdlib/garden.rs`

```rust
//! Chaosgarden control functions for Lua scripts.
//!
//! Provides `garden.*` namespace for timeline and transport control.

use anyhow::Result;
use chaosgarden::ipc::client::GardenClient;
use chaosgarden::ipc::{Beat, Behavior, ContentType, ShellRequest, ShellReply};
use mlua::{Lua, Table, Value as LuaValue, Function};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Shared garden client for all Lua functions
pub struct GardenContext {
    client: Arc<RwLock<Option<GardenClient>>>,
}

impl GardenContext {
    pub fn new() -> Self {
        Self {
            client: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn connect(&self) -> Result<()> {
        let client = GardenClient::connect_local().await?;
        *self.client.write().await = Some(client);
        Ok(())
    }
}

/// Register the `garden` global table.
pub fn register_garden_globals(lua: &Lua, ctx: Arc<GardenContext>) -> Result<()> {
    let globals = lua.globals();
    let garden_table = lua.create_table()?;

    // ═══════════════════════════════════════════════════════════════
    // Transport Controls
    // ═══════════════════════════════════════════════════════════════

    // garden.status() -> {playing, position, tempo}
    let ctx_clone = ctx.clone();
    let status_fn = lua.create_async_function(move |lua, ()| {
        let ctx = ctx_clone.clone();
        async move {
            let client = ctx.client.read().await;
            let client = client.as_ref().ok_or_else(|| mlua::Error::external("Not connected"))?;

            match client.shell(ShellRequest::GetTransportState).await {
                Ok(ShellReply::TransportState { playing, position, tempo }) => {
                    let table = lua.create_table()?;
                    table.set("playing", playing)?;
                    table.set("position", position.0)?;
                    table.set("tempo", tempo)?;
                    Ok(LuaValue::Table(table))
                }
                Ok(other) => Err(mlua::Error::external(format!("Unexpected reply: {:?}", other))),
                Err(e) => Err(mlua::Error::external(e)),
            }
        }
    })?;
    garden_table.set("status", status_fn)?;

    // garden.play()
    let ctx_clone = ctx.clone();
    let play_fn = lua.create_async_function(move |_, ()| {
        let ctx = ctx_clone.clone();
        async move {
            let client = ctx.client.read().await;
            let client = client.as_ref().ok_or_else(|| mlua::Error::external("Not connected"))?;
            client.shell(ShellRequest::Play).await.map_err(mlua::Error::external)?;
            Ok(LuaValue::Boolean(true))
        }
    })?;
    garden_table.set("play", play_fn)?;

    // garden.pause()
    let ctx_clone = ctx.clone();
    let pause_fn = lua.create_async_function(move |_, ()| {
        let ctx = ctx_clone.clone();
        async move {
            let client = ctx.client.read().await;
            let client = client.as_ref().ok_or_else(|| mlua::Error::external("Not connected"))?;
            client.shell(ShellRequest::Pause).await.map_err(mlua::Error::external)?;
            Ok(LuaValue::Boolean(true))
        }
    })?;
    garden_table.set("pause", pause_fn)?;

    // garden.stop()
    let ctx_clone = ctx.clone();
    let stop_fn = lua.create_async_function(move |_, ()| {
        let ctx = ctx_clone.clone();
        async move {
            let client = ctx.client.read().await;
            let client = client.as_ref().ok_or_else(|| mlua::Error::external("Not connected"))?;
            client.shell(ShellRequest::Stop).await.map_err(mlua::Error::external)?;
            Ok(LuaValue::Boolean(true))
        }
    })?;
    garden_table.set("stop", stop_fn)?;

    // garden.seek(beat)
    let ctx_clone = ctx.clone();
    let seek_fn = lua.create_async_function(move |_, beat: f64| {
        let ctx = ctx_clone.clone();
        async move {
            let client = ctx.client.read().await;
            let client = client.as_ref().ok_or_else(|| mlua::Error::external("Not connected"))?;
            client.shell(ShellRequest::Seek { beat: Beat(beat) }).await.map_err(mlua::Error::external)?;
            Ok(LuaValue::Boolean(true))
        }
    })?;
    garden_table.set("seek", seek_fn)?;

    // garden.set_tempo(bpm)
    let ctx_clone = ctx.clone();
    let set_tempo_fn = lua.create_async_function(move |_, bpm: f64| {
        let ctx = ctx_clone.clone();
        async move {
            let client = ctx.client.read().await;
            let client = client.as_ref().ok_or_else(|| mlua::Error::external("Not connected"))?;
            client.shell(ShellRequest::SetTempo { bpm }).await.map_err(mlua::Error::external)?;
            Ok(LuaValue::Boolean(true))
        }
    })?;
    garden_table.set("set_tempo", set_tempo_fn)?;

    // ═══════════════════════════════════════════════════════════════
    // Region Operations
    // ═══════════════════════════════════════════════════════════════

    // garden.create_region(opts) -> region_id
    let ctx_clone = ctx.clone();
    let create_region_fn = lua.create_async_function(move |_, opts: Table| {
        let ctx = ctx_clone.clone();
        async move {
            let position: f64 = opts.get("position")?;
            let duration: f64 = opts.get("duration")?;
            let behavior_type: String = opts.get("behavior")?;
            let content_id: String = opts.get("content_id")?;

            let behavior = match behavior_type.as_str() {
                "play_content" => Behavior::PlayContent { artifact_id: content_id },
                "latent" => Behavior::Latent { job_id: content_id },
                other => return Err(mlua::Error::external(format!("Unknown behavior: {}", other))),
            };

            let client = ctx.client.read().await;
            let client = client.as_ref().ok_or_else(|| mlua::Error::external("Not connected"))?;

            match client.shell(ShellRequest::CreateRegion {
                position: Beat(position),
                duration: Beat(duration),
                behavior,
            }).await {
                Ok(ShellReply::RegionCreated { region_id }) => {
                    Ok(LuaValue::String(lua.create_string(&region_id.to_string())?))
                }
                Ok(other) => Err(mlua::Error::external(format!("Unexpected reply: {:?}", other))),
                Err(e) => Err(mlua::Error::external(e)),
            }
        }
    })?;
    garden_table.set("create_region", create_region_fn)?;

    // garden.delete_region(region_id)
    let ctx_clone = ctx.clone();
    let delete_region_fn = lua.create_async_function(move |_, region_id: String| {
        let ctx = ctx_clone.clone();
        async move {
            let region_id = Uuid::parse_str(&region_id).map_err(mlua::Error::external)?;
            let client = ctx.client.read().await;
            let client = client.as_ref().ok_or_else(|| mlua::Error::external("Not connected"))?;
            client.shell(ShellRequest::DeleteRegion { region_id }).await.map_err(mlua::Error::external)?;
            Ok(LuaValue::Boolean(true))
        }
    })?;
    garden_table.set("delete_region", delete_region_fn)?;

    // garden.move_region(region_id, new_position)
    let ctx_clone = ctx.clone();
    let move_region_fn = lua.create_async_function(move |_, (region_id, new_position): (String, f64)| {
        let ctx = ctx_clone.clone();
        async move {
            let region_id = Uuid::parse_str(&region_id).map_err(mlua::Error::external)?;
            let client = ctx.client.read().await;
            let client = client.as_ref().ok_or_else(|| mlua::Error::external("Not connected"))?;
            client.shell(ShellRequest::MoveRegion {
                region_id,
                new_position: Beat(new_position)
            }).await.map_err(mlua::Error::external)?;
            Ok(LuaValue::Boolean(true))
        }
    })?;
    garden_table.set("move_region", move_region_fn)?;

    // garden.get_regions(start_beat, end_beat) -> table
    let ctx_clone = ctx.clone();
    let get_regions_fn = lua.create_async_function(move |lua, (start, end): (Option<f64>, Option<f64>)| {
        let ctx = ctx_clone.clone();
        async move {
            let range = match (start, end) {
                (Some(s), Some(e)) => Some((Beat(s), Beat(e))),
                _ => None,
            };

            let client = ctx.client.read().await;
            let client = client.as_ref().ok_or_else(|| mlua::Error::external("Not connected"))?;

            match client.shell(ShellRequest::GetRegions { range }).await {
                Ok(ShellReply::Regions { regions }) => {
                    let result = lua.create_table()?;
                    for (i, r) in regions.iter().enumerate() {
                        let region_table = lua.create_table()?;
                        region_table.set("region_id", r.region_id.to_string())?;
                        region_table.set("position", r.position.0)?;
                        region_table.set("duration", r.duration.0)?;
                        region_table.set("is_latent", r.is_latent)?;
                        if let Some(ref aid) = r.artifact_id {
                            region_table.set("artifact_id", aid.clone())?;
                        }
                        result.set(i + 1, region_table)?;
                    }
                    Ok(LuaValue::Table(result))
                }
                Ok(other) => Err(mlua::Error::external(format!("Unexpected reply: {:?}", other))),
                Err(e) => Err(mlua::Error::external(e)),
            }
        }
    })?;
    garden_table.set("get_regions", get_regions_fn)?;

    // ═══════════════════════════════════════════════════════════════
    // Latent Lifecycle
    // ═══════════════════════════════════════════════════════════════

    // garden.latent_started(region_id, job_id)
    let ctx_clone = ctx.clone();
    let latent_started_fn = lua.create_async_function(move |_, (region_id, job_id): (String, String)| {
        let ctx = ctx_clone.clone();
        async move {
            let region_id = Uuid::parse_str(&region_id).map_err(mlua::Error::external)?;
            let client = ctx.client.read().await;
            let client = client.as_ref().ok_or_else(|| mlua::Error::external("Not connected"))?;
            client.shell(ShellRequest::UpdateLatentStarted { region_id, job_id }).await.map_err(mlua::Error::external)?;
            Ok(LuaValue::Boolean(true))
        }
    })?;
    garden_table.set("latent_started", latent_started_fn)?;

    // garden.latent_progress(region_id, progress)
    let ctx_clone = ctx.clone();
    let latent_progress_fn = lua.create_async_function(move |_, (region_id, progress): (String, f32)| {
        let ctx = ctx_clone.clone();
        async move {
            let region_id = Uuid::parse_str(&region_id).map_err(mlua::Error::external)?;
            let client = ctx.client.read().await;
            let client = client.as_ref().ok_or_else(|| mlua::Error::external("Not connected"))?;
            client.shell(ShellRequest::UpdateLatentProgress { region_id, progress }).await.map_err(mlua::Error::external)?;
            Ok(LuaValue::Boolean(true))
        }
    })?;
    garden_table.set("latent_progress", latent_progress_fn)?;

    // garden.latent_resolved(region_id, opts)
    let ctx_clone = ctx.clone();
    let latent_resolved_fn = lua.create_async_function(move |_, (region_id, opts): (String, Table)| {
        let ctx = ctx_clone.clone();
        async move {
            let region_id = Uuid::parse_str(&region_id).map_err(mlua::Error::external)?;
            let artifact_id: String = opts.get("artifact_id")?;
            let content_hash: String = opts.get("content_hash")?;
            let content_type_str: String = opts.get("content_type")?;

            let content_type = match content_type_str.as_str() {
                "audio" => ContentType::Audio,
                "midi" => ContentType::Midi,
                other => return Err(mlua::Error::external(format!("Unknown content_type: {}", other))),
            };

            let client = ctx.client.read().await;
            let client = client.as_ref().ok_or_else(|| mlua::Error::external("Not connected"))?;
            client.shell(ShellRequest::UpdateLatentResolved {
                region_id,
                artifact_id,
                content_hash,
                content_type
            }).await.map_err(mlua::Error::external)?;
            Ok(LuaValue::Boolean(true))
        }
    })?;
    garden_table.set("latent_resolved", latent_resolved_fn)?;

    // garden.latent_failed(region_id, error)
    let ctx_clone = ctx.clone();
    let latent_failed_fn = lua.create_async_function(move |_, (region_id, error): (String, String)| {
        let ctx = ctx_clone.clone();
        async move {
            let region_id = Uuid::parse_str(&region_id).map_err(mlua::Error::external)?;
            let client = ctx.client.read().await;
            let client = client.as_ref().ok_or_else(|| mlua::Error::external("Not connected"))?;
            client.shell(ShellRequest::UpdateLatentFailed { region_id, error }).await.map_err(mlua::Error::external)?;
            Ok(LuaValue::Boolean(true))
        }
    })?;
    garden_table.set("latent_failed", latent_failed_fn)?;

    // ═══════════════════════════════════════════════════════════════
    // Approval Operations
    // ═══════════════════════════════════════════════════════════════

    // garden.get_pending() -> table
    let ctx_clone = ctx.clone();
    let get_pending_fn = lua.create_async_function(move |lua, ()| {
        let ctx = ctx_clone.clone();
        async move {
            let client = ctx.client.read().await;
            let client = client.as_ref().ok_or_else(|| mlua::Error::external("Not connected"))?;

            match client.shell(ShellRequest::GetPendingApprovals).await {
                Ok(ShellReply::PendingApprovals { approvals }) => {
                    let result = lua.create_table()?;
                    for (i, a) in approvals.iter().enumerate() {
                        let approval_table = lua.create_table()?;
                        approval_table.set("region_id", a.region_id.to_string())?;
                        approval_table.set("artifact_id", a.artifact_id.clone())?;
                        approval_table.set("content_hash", a.content_hash.clone())?;
                        approval_table.set("content_type", format!("{:?}", a.content_type))?;
                        result.set(i + 1, approval_table)?;
                    }
                    Ok(LuaValue::Table(result))
                }
                Ok(other) => Err(mlua::Error::external(format!("Unexpected reply: {:?}", other))),
                Err(e) => Err(mlua::Error::external(e)),
            }
        }
    })?;
    garden_table.set("get_pending", get_pending_fn)?;

    // garden.approve(region_id, decided_by)
    let ctx_clone = ctx.clone();
    let approve_fn = lua.create_async_function(move |_, (region_id, decided_by): (String, String)| {
        let ctx = ctx_clone.clone();
        async move {
            let region_id = Uuid::parse_str(&region_id).map_err(mlua::Error::external)?;
            let decided_by = Uuid::parse_str(&decided_by).map_err(mlua::Error::external)?;
            let client = ctx.client.read().await;
            let client = client.as_ref().ok_or_else(|| mlua::Error::external("Not connected"))?;
            client.shell(ShellRequest::ApproveLatent { region_id, decided_by }).await.map_err(mlua::Error::external)?;
            Ok(LuaValue::Boolean(true))
        }
    })?;
    garden_table.set("approve", approve_fn)?;

    // garden.reject(region_id, decided_by, reason)
    let ctx_clone = ctx.clone();
    let reject_fn = lua.create_async_function(move |_, (region_id, decided_by, reason): (String, String, Option<String>)| {
        let ctx = ctx_clone.clone();
        async move {
            let region_id = Uuid::parse_str(&region_id).map_err(mlua::Error::external)?;
            let decided_by = Uuid::parse_str(&decided_by).map_err(mlua::Error::external)?;
            let client = ctx.client.read().await;
            let client = client.as_ref().ok_or_else(|| mlua::Error::external("Not connected"))?;
            client.shell(ShellRequest::RejectLatent { region_id, decided_by, reason }).await.map_err(mlua::Error::external)?;
            Ok(LuaValue::Boolean(true))
        }
    })?;
    garden_table.set("reject", reject_fn)?;

    globals.set("garden", garden_table)?;
    Ok(())
}
```

### Example Lua Scripts

#### Basic Playback Control
```lua
-- start_playback.lua
garden.set_tempo(120)
garden.seek(0)
garden.play()

-- Wait and check status
local status = garden.status()
print(string.format("Playing: %s, Position: %.2f beats",
    tostring(status.playing), status.position))
```

#### Create and Manage Regions
```lua
-- build_timeline.lua
-- Create a 4-bar loop region
local region_id = garden.create_region({
    position = 0,
    duration = 16,  -- 4 bars at 4/4
    behavior = "play_content",
    content_id = "artifact_abc123"
})
print("Created region: " .. region_id)

-- Move it to bar 5
garden.move_region(region_id, 16)

-- List all regions
local regions = garden.get_regions()
for i, r in ipairs(regions) do
    print(string.format("Region %d: %s at beat %.1f", i, r.region_id, r.position))
end
```

#### Latent Region Workflow
```lua
-- orchestrate_generation.lua
-- Create a latent region for AI generation
local region_id = garden.create_region({
    position = 0,
    duration = 8,
    behavior = "latent",
    content_id = "job_orpheus_001"
})

-- Simulate job progress (in real use, this would be from orpheus callbacks)
garden.latent_started(region_id, "job_orpheus_001")

for progress = 0.1, 1.0, 0.1 do
    garden.latent_progress(region_id, progress)
    -- In real code: wait for actual progress
end

-- Mark resolved with generated artifact
garden.latent_resolved(region_id, {
    artifact_id = "artifact_generated_xyz",
    content_hash = "sha256:abc123...",
    content_type = "audio"
})

-- Check pending approvals
local pending = garden.get_pending()
if #pending > 0 then
    print("Pending approval: " .. pending[1].artifact_id)

    -- Auto-approve (or wait for human decision)
    local my_agent_id = "00000000-0000-0000-0000-000000000001"
    garden.approve(pending[1].region_id, my_agent_id)
end
```

#### Query and Transform
```lua
-- query_regions.lua
-- Use Trustfall query to find all latent regions
local results = garden.query([[
    query {
        Region(is_latent: true) {
            id @output
            position @output
            latent_status @output
        }
    }
]], {})

for _, row in ipairs(results) do
    print(string.format("Latent region %s at %.1f: %s",
        row.id, row.position, row.latent_status))
end
```

### Integration with stdlib

Update `crates/luanette/src/stdlib/mod.rs`:

```rust
pub mod garden;
pub mod midi;
pub mod temp;

use anyhow::Result;
use mlua::Lua;
use std::sync::Arc;

/// Register all stdlib modules in the Lua VM.
pub fn register_all(lua: &Lua, garden_ctx: Option<Arc<garden::GardenContext>>) -> Result<()> {
    midi::register_midi_globals(lua)?;
    temp::register_temp_globals(lua)?;

    if let Some(ctx) = garden_ctx {
        garden::register_garden_globals(lua, ctx)?;
    }

    Ok(())
}
```

---

## Testing Strategy

### Unit Tests
- Each new tool method in garden.rs
- GardenManager convenience methods

### Integration Tests
- Create region → query via Trustfall → verify exists
- Full latent lifecycle: create → started → progress → resolved → approve
- Reject workflow

### MCP Tests
- Tool schema validation
- Round-trip through holler

---

## File Changes

| File | Changes |
|------|---------|
| `hootenanny/src/api/tools/garden.rs` | Add 10 new tool methods + request types |
| `hootenanny/src/api/dispatch.rs` | Wire new tools in list_tools + call_tool |
| `hootenanny/src/zmq/manager.rs` | Add convenience methods |
| `luanette/src/stdlib/garden.rs` | **New file** - Lua bindings for garden.* namespace |
| `luanette/src/stdlib/mod.rs` | Register garden module |
| `luanette/src/lib.rs` | Re-export GardenContext |

---

## Acceptance Criteria

### MCP Tools
- [ ] `garden_create_region` creates region, returns UUID
- [ ] `garden_delete_region` removes region
- [ ] `garden_move_region` updates position
- [ ] `garden_get_regions` returns region list
- [ ] `garden_latent_started` notifies daemon of job start
- [ ] `garden_latent_progress` updates progress
- [ ] `garden_latent_resolved` marks complete with artifact
- [ ] `garden_latent_failed` marks failed
- [ ] `garden_get_pending` returns pending approvals
- [ ] `garden_approve` approves for playback
- [ ] `garden_reject` rejects with reason
- [ ] All tools accessible via Claude Code MCP

### Luanette Bindings
- [ ] `garden.status()` returns transport state
- [ ] `garden.play/pause/stop/seek/set_tempo` work
- [ ] `garden.create_region(opts)` creates region
- [ ] `garden.delete_region(id)` removes region
- [ ] `garden.move_region(id, pos)` updates position
- [ ] `garden.get_regions()` returns region list
- [ ] `garden.latent_started/progress/resolved/failed` lifecycle
- [ ] `garden.get_pending()` returns approvals
- [ ] `garden.approve/reject` work
- [ ] Example scripts run successfully

### Integration
- [ ] MCP and Lua APIs have identical semantics
- [ ] Tests pass for both interfaces
- [ ] Documentation updated

---

## Dependencies

- Depends on: 13-wire-daemon (✅ complete)
- Blocks: Agent-driven composition workflows
