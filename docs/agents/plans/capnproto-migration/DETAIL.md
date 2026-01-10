# Cap'n Proto Migration - Design Rationale

**Purpose:** Deep context for revision sessions. Read when you need to understand *why*.

---

## Why Cap'n Proto over MsgPack?

**Zero-copy reads.** MsgPack deserializes into owned Rust structs (allocations). Cap'n Proto reads directly from the wire buffer. For chaosgarden's audio callback (~5ms cycle), this matters.

**Schema as source of truth.** The `.capnp` files define the protocol. Rust, Python, Lua, Go all generate from the same schema. No version skew.

**Field numbering for evolution.** Adding fields doesn't break old readers. MsgPack has no schema, so evolution is manual and error-prone.

## Why All-In Instead of Hybrid?

**Cognitive overhead.** "Is this message capnp or msgpack?" is a question we don't want to ask. One format everywhere.

**The JSON boundary is already paid.** Holler must translate JSON↔internal regardless. Whether internal is msgpack or capnp, holler does the same work.

**Future Python/Lua access.** With capnp, `pycapnp` loads our schemas directly. With msgpack, we'd maintain parallel type definitions.

## Why Translation at Holler Only?

MCP requires JSON. That boundary is fixed. Everything past holler is internal, so we control it. Keeping translation in one place (holler) means:
- One set of JSON↔capnp converters
- Internal components never see JSON
- Easy to test the translation layer

## Schema Organization

```
common.capnp    — Types used everywhere (Timestamp, errors)
envelope.capnp  — Message wrapper, routing metadata
tools.capnp     — Tool requests/responses (the big file)
garden.capnp    — RT protocol: transport, regions, position
broadcast.capnp — PUB/SUB events
jobs.capnp      — Job system types
```

Split by domain to keep individual files manageable. `tools.capnp` will be largest (~50 tool variants).

## Rejected Alternatives

| Alternative | Why Rejected |
|-------------|--------------|
| FlatBuffers | Less mature Rust ecosystem, similar perf |
| Protobuf | Requires allocation for nested messages |
| Keep MsgPack for tools | Hybrid complexity, lose schema benefits |
| Custom binary format | Not worth maintaining vs proven solution |

## Cross-Cutting Concerns

### Error Handling

Cap'n Proto reads can fail (malformed data). All read sites need error handling:
```rust
let reader = message.get_root::<payload::Reader>()?;
```

### Testing

Each schema file should have corresponding tests that verify:
- Roundtrip (build → serialize → read)
- Field access works
- Evolution (old reader + new writer)

### Performance Verification

After migration, benchmark chaosgarden message handling to verify the zero-copy benefit is real.
