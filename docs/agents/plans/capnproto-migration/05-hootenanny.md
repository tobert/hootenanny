# 05: Hootenanny Migration

**Files:** `crates/hootenanny/src/zmq/*.rs`, `crates/hootenanny/src/api/*.rs`
**Focus:** Replace MsgPack with Cap'n Proto in hootenanny
**Dependencies:** 03-frame-protocol, 04-helpers
**Unblocks:** 07-holler-bridge

---

## Task

Migrate hootenanny's ZMQ message handling from MsgPack to Cap'n Proto.

**Deliverables:**
1. ZMQ client uses capnp frames
2. ZMQ publisher uses capnp for broadcasts
3. Tool dispatch reads capnp requests
4. All hootenanny tests pass

**Definition of Done:**
```bash
cargo test -p hootenanny
```

## Out of Scope

- ❌ Holler/MCP translation — that's task 07
- ❌ Chaosgarden — that's task 06

---

## Key Files to Modify

1. `src/zmq/client.rs` — Sending requests, receiving responses
2. `src/zmq/publisher.rs` — Broadcasting events
3. `src/api/dispatch.rs` — Reading incoming tool requests

---

## Migration Pattern

**Before (MsgPack):**
```rust
let payload = Payload::OrpheusGenerate { ... };
let frame = HootFrame::request("hootenanny", &payload)?;
```

**After (Cap'n Proto):**
```rust
let message = builders::build_orpheus_generate(
    Some(0.9), Some(0.95), Some(1024), Some(1)
);
let frame = HootFrame::request_capnp("hootenanny", &message);
```

---

## Reading Requests

**Before:**
```rust
let payload: Payload = frame.payload()?;
match payload {
    Payload::OrpheusGenerate { temperature, ... } => { ... }
}
```

**After:**
```rust
let reader = frame.read_capnp()?;
let req = reader.get_root::<tools_capnp::tool_request::Reader>()?;
match req.which()? {
    tools_capnp::tool_request::Which::OrpheusGenerate(orpheus) => {
        let orpheus = orpheus?;
        let temperature = orpheus.get_temperature();
        // ...
    }
}
```

---

## Broadcast Migration

`BroadcastPublisher` sends events to subscribers. Update to use capnp:

```rust
pub async fn beat_tick(&self, beat: u64, position: f64, tempo: f64) -> Result<()> {
    let mut message = capnp::message::Builder::new_default();
    {
        let mut broadcast = message.init_root::<broadcast_capnp::broadcast::Builder>();
        let mut tick = broadcast.init_beat_tick();
        tick.set_beat(beat);
        tick.set_position_beats(position);
        tick.set_tempo_bpm(tempo);
        // timestamp at edge
        let mut ts = tick.init_timestamp();
        ts.set_nanos(builders::timestamp_now());
    }
    self.publish(&message).await
}
```

---

## Incremental Approach

If full migration is too big:
1. Start with broadcasts (simpler, fewer variants)
2. Then tool responses
3. Then tool requests
4. Run tests after each step

---

## Acceptance Criteria

- [ ] `cargo test -p hootenanny` passes
- [ ] ZMQ messages use ContentType::CapnProto
- [ ] Broadcasts serialize with capnp
- [ ] Tool dispatch reads capnp requests
- [ ] No MsgPack serialization in hootenanny (except legacy compat if needed)
