# Task 08: ZMQ Integration

**Status:** In Progress
**Crates:** `chaosgarden`, `hootenanny`, `hooteproto`

Wire up the ZMQ message flow between hootenanny (control plane) and chaosgarden (RT plane) for stream capture.

---

## Architecture

```
┌─────────────────┐                    ┌──────────────────┐
│   Hootenanny    │                    │   Chaosgarden    │
│  (Control)      │                    │   (RT)           │
├─────────────────┤                    ├──────────────────┤
│                 │                    │                  │
│  MCP Tool       │                    │  ZMQ Handler     │
│    ↓            │  StreamStart       │    ↓             │
│  StreamManager  │───────────────────>│  stream_io.rs    │
│    ↓            │  (Command)         │    │             │
│  ZMQ Client     │                    │    └──> mmap     │
│                 │                    │                  │
│  Broadcast Sub  │  ChunkFull         │  Broadcast Pub   │
│    ↓            │<───────────────────│    ↑             │
│  Handle Event   │  (Event)           │  (periodic)      │
│    ↓            │                    │                  │
│  Seal to CAS    │  SwitchChunk       │                  │
│    ↓            │───────────────────>│  Close old       │
│  New Chunk      │  (Command)         │  Open new        │
│                 │                    │                  │
└─────────────────┘                    └──────────────────┘
```

---

## Message Flow

### 1. Stream Start
```
User → MCP tool → hootenanny StreamManager
  ↓
Create staging chunk in CAS
  ↓
Send StreamStart{uri, definition, chunk_path} → chaosgarden
  ↓
Chaosgarden receives → calls stream_io::start_stream()
  ↓
Opens file, mmaps, ready to write
  ↓
Broadcasts StreamHeadPosition (periodic updates)
```

### 2. Chunk Rotation
```
Chaosgarden detects chunk full
  ↓
Broadcasts StreamChunkFull{uri, path, bytes, samples}
  ↓
Hootenanny receives broadcast
  ↓
Seals chunk to CAS (staging → content)
  ↓
Creates new staging chunk
  ↓
Sends StreamSwitchChunk{uri, new_chunk_path}
  ↓
Chaosgarden closes old mmap, opens new
```

### 3. Stream Stop
```
User → MCP tool → hootenanny StreamManager
  ↓
Send StreamStop{uri} → chaosgarden
  ↓
Chaosgarden calls stream_io::stop_stream()
  ↓
Flushes mmap, closes file
  ↓
Hootenanny seals final chunk to CAS
  ↓
Creates StreamManifest artifact
```

---

## Implementation Tasks

### Phase 1: Chaosgarden ZMQ Reception (Tasks 1-3)

#### Task 1: Add ZMQ Message Handlers to Chaosgarden ⚡
**File:** `crates/chaosgarden/src/daemon.rs`

Add handlers for the stream commands:

```rust
// Add to GardenDaemon impl
async fn handle_stream_start(&self, start: StreamStart) -> Result<()> {
    // 1. Parse stream definition
    // 2. Call stream_manager.start_stream(definition, chunk_path)
    // 3. Return success/error
}

async fn handle_stream_switch_chunk(&self, switch: StreamSwitchChunk) -> Result<()> {
    // 1. Call stream_manager.switch_chunk(uri, new_path)
    // 2. Return success/error
}

async fn handle_stream_stop(&self, stop: StreamStop) -> Result<()> {
    // 1. Call stream_manager.stop_stream(uri)
    // 2. Return success/error
}
```

**Integration Point:**
- Extend the existing `Handler` trait or create new dispatch logic
- May need to convert from Cap'n Proto to Rust types (use `hooteproto::conversion`)

#### Task 2: Wire stream_io.rs to Daemon 🔌
**File:** `crates/chaosgarden/src/daemon.rs`

Add `StreamManager` (from `stream_io.rs`) to the daemon state:

```rust
pub struct GardenDaemon {
    // ... existing fields
    stream_manager: Arc<StreamManager>,
}
```

Initialize in constructor and wire to handlers from Task 1.

#### Task 3: Implement Broadcast Sending 📡
**File:** `crates/chaosgarden/src/daemon.rs` or new `stream_broadcast.rs`

Create a background task that:
1. Polls active streams via `stream_manager.head_position(uri)`
2. Sends `StreamHeadPosition` broadcasts periodically (e.g., every 100ms)
3. Sends `StreamChunkFull` when `stream_manager.is_chunk_full(uri)` returns true

**Broadcast Method:**
```rust
async fn broadcast_stream_event(&self, event: Broadcast) -> Result<()> {
    // Use existing iopub socket or create new stream-specific PUB socket
    // Serialize event to Cap'n Proto
    // Send on ZMQ PUB socket
}
```

---

### Phase 2: Hootenanny ZMQ Transmission (Tasks 4-6)

#### Task 4: Implement MCP Tools → ZMQ Commands 🎮
**File:** `crates/hootenanny/src/api/tools/streams.rs`

Update the scaffolded tools to send ZMQ messages:

```rust
pub async fn stream_create(&self, request: StreamCreateRequest) -> ToolResult {
    // 1. Create staging chunk via CAS
    let chunk_path = self.stream_manager.create_staging_chunk(&uri)?;

    // 2. Build StreamStart message
    let cmd = StreamStart {
        uri: request.uri,
        definition: /* convert from request */,
        chunk_path,
    };

    // 3. Send via ZMQ to chaosgarden
    self.garden_manager.send_command(Payload::StreamStart(cmd)).await?;

    // 4. Return success
}
```

**Dependencies:**
- Access to `GardenManager` for sending commands
- May need to add `send_stream_command()` helper to `GardenManager`

#### Task 5: Add Broadcast Subscriber 📻
**File:** `crates/hootenanny/src/streams/manager.rs` or new `broadcast_handler.rs`

Create a background task that:
1. Subscribes to chaosgarden's iopub socket
2. Filters for `Broadcast::StreamChunkFull` and `Broadcast::StreamHeadPosition`
3. Calls appropriate handlers

```rust
pub async fn handle_stream_broadcasts(
    garden_manager: Arc<GardenManager>,
    stream_manager: Arc<StreamManager>,
) {
    let mut events = garden_manager.subscribe_events().await;

    while let Some(broadcast) = events.next().await {
        match broadcast {
            Broadcast::StreamChunkFull(event) => {
                handle_chunk_full(stream_manager.clone(), event).await;
            }
            Broadcast::StreamHeadPosition(pos) => {
                // Optional: update UI/monitoring
            }
            _ => {}
        }
    }
}
```

#### Task 6: Implement Chunk Rotation Logic 🔄
**File:** `crates/hootenanny/src/streams/manager.rs`

Add handler for `StreamChunkFull` events:

```rust
async fn handle_chunk_full(
    stream_manager: Arc<StreamManager>,
    event: StreamChunkFull,
) -> Result<()> {
    let uri = StreamUri::from(event.stream_uri);

    // 1. Seal the full chunk to CAS
    let chunk_hash = stream_manager.seal_chunk(&uri, &event.path).await?;

    // 2. Update manifest
    stream_manager.add_chunk_to_manifest(&uri, chunk_hash, event.bytes_written)?;

    // 3. Create new staging chunk
    let new_chunk_path = stream_manager.create_staging_chunk(&uri)?;

    // 4. Send SwitchChunk command to chaosgarden
    let cmd = StreamSwitchChunk {
        uri: uri.to_string(),
        new_chunk_path,
    };
    garden_manager.send_command(Payload::StreamSwitchChunk(cmd)).await?;

    Ok(())
}
```

---

### Phase 3: Testing (Task 7)

#### Task 7: End-to-End Message Flow Test 🧪
**File:** `crates/hootenanny/tests/stream_capture_integration.rs`

Write integration test:
1. Start chaosgarden daemon (or mock)
2. Call `stream_create` MCP tool
3. Write mock audio data
4. Verify ChunkFull broadcast received
5. Verify chunk sealed to CAS
6. Verify SwitchChunk sent
7. Call `stream_stop`
8. Verify manifest created

---

## Success Criteria

- [ ] StreamStart command reaches chaosgarden and starts stream
- [ ] Chaosgarden sends StreamHeadPosition broadcasts
- [ ] ChunkFull triggers in chaosgarden when chunk fills
- [ ] Hootenanny receives ChunkFull and seals to CAS
- [ ] SwitchChunk command sent and new chunk opened
- [ ] StreamStop command stops stream and seals final chunk
- [ ] End-to-end test passes
- [ ] No deadlocks or message loss

---

## Dependencies

**Chaosgarden needs:**
- `StreamManager` from `stream_io.rs` (✅ already implemented)
- ZMQ PUB socket for broadcasts (may exist in IPC layer)
- Access to Cap'n Proto message parsing

**Hootenanny needs:**
- `GardenManager` for sending commands (✅ already exists)
- Broadcast subscriber (needs implementation)
- StreamManager CAS integration (partially exists)

---

## Open Questions

1. **Socket Architecture:**
   - Use existing IPC sockets or create new stream-specific sockets?
   - **Decision:** Use existing iopub for broadcasts, shell for commands

2. **Broadcast Frequency:**
   - How often to send StreamHeadPosition?
   - **Proposal:** Every 100ms or configurable

3. **Error Handling:**
   - What happens if chaosgarden is down when StreamStart sent?
   - **Proposal:** Tool returns error, client can retry

4. **Threading:**
   - Run stream_io in separate thread for RT safety?
   - **Proposal:** Yes, spawn dedicated thread for I/O

---

## Timeline Estimate

- Phase 1 (Chaosgarden): 2-3 hours
- Phase 2 (Hootenanny): 2-3 hours
- Phase 3 (Testing): 1-2 hours
- **Total:** ~5-8 hours of focused work

---

## Next Steps

Start with **Task 1** - add the ZMQ message handlers to chaosgarden daemon.
