# 06: Chaosgarden Migration

**Files:** `crates/chaosgarden/src/daemon.rs`, `crates/chaosgarden/src/zmq_handler.rs`
**Focus:** Zero-copy message handling in RT context
**Dependencies:** 03-frame-protocol, 04-helpers
**Unblocks:** 07-holler-bridge

---

## Task

Migrate chaosgarden's ZMQ message handling to Cap'n Proto. This is the **hot path** — zero-copy reads matter here.

**Deliverables:**
1. Daemon reads commands via capnp (zero-copy)
2. Position/beat updates sent via capnp
3. No allocations in message read path
4. All chaosgarden tests pass

**Definition of Done:**
```bash
cargo test -p chaosgarden
```

## Out of Scope

- ❌ Audio rendering code — just message handling
- ❌ Hootenanny side — that's task 05

---

## Zero-Copy Pattern

The key benefit: read directly from ZMQ buffer without allocation.

```rust
// Receive ZMQ frame
let frame = HootFrame::from_frames(&frames)?;

// Zero-copy read — reader borrows from frame.body
let reader = frame.read_capnp()?;
let cmd = reader.get_root::<garden_capnp::shell_request::Reader>()?;

// Process without allocation
match cmd.which()? {
    garden_capnp::shell_request::Which::Play(_) => {
        self.transport.play();
    }
    garden_capnp::shell_request::Which::Seek(beat) => {
        let beat = beat?;
        self.transport.seek(beat.get_value());
    }
    // ...
}
// reader dropped here, frame.body can be reused
```

---

## Key Files

1. `src/daemon.rs` — Main daemon loop, receives shell requests
2. `src/zmq_handler.rs` — If exists, ZMQ message dispatch

---

## Sending Position Updates

```rust
fn send_position_update(&self, beat: f64, sample_frame: u64) -> Result<()> {
    let mut message = capnp::message::Builder::new_default();
    {
        let mut update = message.init_root::<garden_capnp::position_update::Builder>();
        let mut ts = update.init_timestamp();
        ts.set_nanos(timestamp_now()); // Capture at edge!
        update.init_beat().set_value(beat);
        update.set_sample_frame(sample_frame);
    }
    let frame = HootFrame::reply_capnp(self.request_id, &message);
    self.send_frame(frame)
}
```

---

## Timestamp at Edge

Critical: capture timestamps at the moment of event, not after processing:

```rust
// GOOD: timestamp captured immediately
let now = timestamp_now();
// ... some processing ...
ts.set_nanos(now);

// BAD: timestamp captured after processing delay
// ... some processing ...
ts.set_nanos(timestamp_now());
```

---

## Performance Note

After migration, the message read path should have:
- 0 heap allocations for reading commands
- 1 allocation for sending responses (the Builder)

Verify with `#[global_allocator]` counting or tracing if concerned.

---

## Acceptance Criteria

- [ ] `cargo test -p chaosgarden` passes
- [ ] Shell requests read via capnp
- [ ] Position updates sent via capnp
- [ ] Timestamps captured at event edge
- [ ] No allocations in read path (verified by inspection)
