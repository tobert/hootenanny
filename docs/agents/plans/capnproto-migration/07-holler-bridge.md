# 07: Holler JSON Bridge

**Files:** `crates/holler/src/translation.rs` (new), `crates/holler/src/handler.rs`, `crates/holler/src/backend.rs`
**Focus:** JSON ↔ Cap'n Proto translation at MCP boundary
**Dependencies:** 05-hootenanny, 06-chaosgarden
**Unblocks:** 08-cleanup

---

## Task

Implement translation layer in holler that converts MCP JSON requests to Cap'n Proto for internal transport, and capnp responses back to JSON.

**Deliverables:**
1. `translation.rs` module with JSON↔capnp converters
2. MCP tool handlers use translation layer
3. All holler/MCP tests pass
4. End-to-end tool calls work

**Definition of Done:**
```bash
cargo test -p holler
# Plus manual test: call a tool via MCP, verify response
```

## Out of Scope

- ❌ Changing MCP protocol — JSON is required by spec
- ❌ Internal hootenanny/chaosgarden code — already migrated

---

## Architecture

```
MCP Client (Claude)
       │
       │ JSON-RPC
       ▼
    Holler
       │
       ├─► parse JSON into serde types
       ├─► translate to capnp Builder
       ├─► send via ZMQ (capnp frame)
       │
       │◄─ receive capnp frame
       ├─► read capnp Reader
       ├─► translate to serde types
       └─► serialize as JSON response
```

---

## translation.rs

```rust
//! JSON ↔ Cap'n Proto translation for MCP boundary

use crate::schema::*;  // MCP tool parameter types (serde)
use hooteproto::capnp_gen::*;
use anyhow::Result;

/// Convert MCP OrpheusGenerateRequest to capnp message
pub fn orpheus_generate_to_capnp(
    req: &OrpheusGenerateRequest,
) -> capnp::message::Builder<capnp::message::HeapAllocator> {
    let mut message = capnp::message::Builder::new_default();
    {
        let mut tool_req = message.init_root::<tools_capnp::tool_request::Builder>();
        let mut orpheus = tool_req.init_orpheus_generate();

        if let Some(t) = req.temperature {
            orpheus.set_temperature(t);
        }
        if let Some(p) = req.top_p {
            orpheus.set_top_p(p);
        }
        // ... etc
    }
    message
}

/// Convert capnp success response to JSON-serializable type
pub fn success_from_capnp(
    reader: &envelope_capnp::success::Reader,
) -> Result<serde_json::Value> {
    let result_str = reader.get_result()?;
    let value: serde_json::Value = serde_json::from_str(result_str)?;
    Ok(value)
}

// One function per tool request type...
// One function per response type...
```

---

## Macro Option

If translation is too repetitive, consider a macro:

```rust
translate_request!(OrpheusGenerateRequest => orpheus_generate {
    temperature => set_temperature,
    top_p => set_top_p,
    max_tokens => set_max_tokens,
    // ...
});
```

But explicit functions are fine for ~50 tools. Clearer and easier to debug.

---

## Tool Handler Update

**Before:**
```rust
async fn handle_orpheus_generate(&self, params: Value) -> Result<Value> {
    let req: OrpheusGenerateRequest = serde_json::from_value(params)?;
    let payload = Payload::OrpheusGenerate { ... };
    let response = self.zmq_client.request(payload).await?;
    // ...
}
```

**After:**
```rust
async fn handle_orpheus_generate(&self, params: Value) -> Result<Value> {
    let req: OrpheusGenerateRequest = serde_json::from_value(params)?;
    let message = translation::orpheus_generate_to_capnp(&req);
    let response_frame = self.zmq_client.request_capnp(message).await?;
    let reader = response_frame.read_capnp()?;
    let result = translation::success_from_capnp(&reader)?;
    Ok(result)
}
```

---

## Testing Strategy

1. Unit tests for each translation function
2. Integration test: JSON in → capnp → JSON out roundtrip
3. End-to-end: actual MCP call through holler

```rust
#[test]
fn orpheus_generate_roundtrip() {
    let json_req = json!({
        "temperature": 0.9,
        "max_tokens": 1024
    });
    let req: OrpheusGenerateRequest = serde_json::from_value(json_req.clone())?;

    // To capnp
    let message = translation::orpheus_generate_to_capnp(&req);

    // Serialize and deserialize (simulating wire)
    let bytes = capnp::serialize::write_message_to_words(&message);
    let reader = capnp::serialize::read_message_from_flat_slice(...)?;

    // Back to JSON-compatible
    let back = translation::orpheus_generate_from_capnp(&reader)?;

    assert_eq!(back.temperature, req.temperature);
}
```

---

## Acceptance Criteria

- [ ] `translation.rs` module exists
- [ ] All tool request types have to_capnp functions
- [ ] Response types have from_capnp functions
- [ ] MCP tool calls work end-to-end
- [ ] `cargo test -p holler` passes
