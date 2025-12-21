# Tech Debt: JSON Boundary at holler

**Status:** Planning
**Authors:** Claude
**Created:** 2025-12-20
**Updated:** 2025-12-20

## Executive Summary

JSON should live at the MCP boundary (holler), not in hooteproto. Currently holler sends
`Payload::ToolCall { name, args: JSON }` and hootenanny parses the JSON. This is backwards.

**Goal:** holler parses JSON and sends typed Payloads. hooteproto has no `serde_json::Value`.

## Current Architecture (Wrong)

```
Claude (MCP)
    │ JSON: { "name": "abc_parse", "arguments": {"notation": "X:1..."} }
    ▼
┌─────────────────────────────────────────────────────────────┐
│ holler                                                       │
│   Payload::ToolCall { name: "abc_parse", args: JSON }       │
│   // Passes JSON through without parsing                     │
└────────────────────────┬────────────────────────────────────┘
                         │ ZMQ: ToolCall with embedded JSON
                         ▼
┌─────────────────────────────────────────────────────────────┐
│ hootenanny (dispatch.rs)                                     │
│   match name {                                               │
│     "abc_parse" => serde_json::from_value(args)?            │
│   }                                                          │
│   // Parses JSON here (wrong layer)                         │
└─────────────────────────────────────────────────────────────┘
```

## Target Architecture

```
Claude (MCP)
    │ JSON: { "name": "abc_parse", "arguments": {"notation": "X:1..."} }
    ▼
┌─────────────────────────────────────────────────────────────┐
│ holler (JSON boundary)                                       │
│   match name {                                               │
│     "abc_parse" => {                                        │
│       let args: AbcParseArgs = serde_json::from_value()?;  │
│       Payload::AbcParse { notation: args.notation }         │
│     }                                                        │
│   }                                                          │
│   // JSON parsed here, typed Payload sent                    │
└────────────────────────┬────────────────────────────────────┘
                         │ ZMQ: Typed Payload (no JSON)
                         ▼
┌─────────────────────────────────────────────────────────────┐
│ hootenanny                                                   │
│   match payload {                                            │
│     Payload::AbcParse { notation } => abc::parse(&notation) │
│   }                                                          │
│   // Receives typed data, no JSON parsing                    │
└─────────────────────────────────────────────────────────────┘
```

## What Changes

### holler (gains JSON parsing)

New `dispatch.rs` that converts tool name + JSON args → typed Payload:

```rust
pub fn json_to_payload(name: &str, args: serde_json::Value) -> Result<Payload> {
    match name {
        "abc_parse" => {
            let p: params::AbcParse = serde_json::from_value(args)?;
            Ok(Payload::AbcParse { notation: p.notation })
        }
        "orpheus_generate" => {
            let p: params::OrpheusGenerate = serde_json::from_value(args)?;
            Ok(Payload::OrpheusGenerate {
                prompt: p.prompt,
                temperature: p.temperature,
                // ...typed fields
            })
        }
        // ... all tools
    }
}
```

### hooteproto (loses JSON)

Remove `serde_json::Value` from all Payload variants:

| Before | After |
|--------|-------|
| `ToolCall { name: String, args: Value }` | **Remove entirely** |
| `Success { result: Value }` | `Success { result: Vec<u8> }` (serialized by holler) |
| `Error { details: Option<Value> }` | `Error { message: String, code: Option<i32> }` |
| `LuaEval { params: Value }` | `LuaEval { params: Vec<(String, LuaValue)> }` |

**Exception:** Keep JSON for Trustfall variables (`GardenQuery::variables`) since Trustfall
needs dynamic typing for query parameters.

### hootenanny (simplifies)

dispatch.rs becomes a simple match on typed Payloads:

```rust
match payload {
    Payload::AbcParse { notation } => {
        let result = abc::parse(&notation)?;
        Payload::AbcParseResult { ast: result }
    }
    Payload::OrpheusGenerate { prompt, temperature, .. } => {
        let job_id = orchestrator.generate(prompt, temperature).await?;
        Payload::JobStarted { job_id }
    }
    // No JSON parsing anywhere
}
```

## Migration Plan

### Phase 1: Add typed Payload variants

For each tool, ensure hooteproto has a typed variant:

1. Review existing `Payload` variants - many already exist but are unused
2. Add missing variants with proper typed fields
3. Add corresponding result variants where needed

### Phase 2: Add dispatch to holler

1. Create `holler/src/dispatch.rs` with `json_to_payload()`
2. Define arg structs in `holler/src/params.rs` (MCP-shaped, with serde)
3. Update handler to call `json_to_payload()` before ZMQ send
4. Keep `Payload::ToolCall` temporarily for fallback

### Phase 3: Update hootenanny dispatch

1. Add match arms for typed Payloads alongside ToolCall
2. Gradually migrate tools from ToolCall to typed
3. Remove JSON parsing from hootenanny

### Phase 4: Remove ToolCall and JSON

1. Remove `Payload::ToolCall` variant
2. Remove `serde_json` dependency from hooteproto (except for Trustfall)
3. Update Cap'n Proto schemas
4. Remove holler fallback path

## hooteproto JSON Fields (Current → Target)

| Field | Current | Target |
|-------|---------|--------|
| `ToolCall::args` | `Value` | **Remove variant** |
| `Success::result` | `Value` | `Vec<u8>` (opaque to hooteproto) |
| `Error::details` | `Option<Value>` | `String` message |
| `LuaEval::params` | `Value` | `Vec<(String, LuaValue)>` |
| `JobExecute::params` | `Value` | Typed per job type |
| `GardenQuery::variables` | `Value` | **Keep** (Trustfall needs it) |
| `TimelineEvent::metadata` | `Value` | Domain-specific struct |
| `ArtifactCreate::metadata` | `Value` | `ArtifactMetadata` struct |
| `ToolInfo::input_schema` | `Value` | **Keep** (JSON Schema by definition) |

## Benefits

1. **Type safety across ZMQ** - Compiler catches mismatches
2. **hooteproto is protocol-focused** - No MCP/JSON concerns
3. **hootenanny simplifies** - Just handles typed requests
4. **Better errors** - JSON parse errors at holler with tool context
5. **Smaller payloads** - Cap'n Proto more compact than JSON-in-Cap'n Proto

## CAS Access Pattern

holler has `cas` crate as dependency for one purpose: reading file bytes when serving
HTTP artifact content. All artifact metadata operations go through hootenanny via ZMQ.

```rust
// holler serving GET /artifact/{id}/content
let metadata = zmq_request(Payload::ArtifactGet { id }).await?;  // → hootenanny
let bytes = cas.retrieve(&metadata.content_hash)?;                // → local filesystem
Ok(Response::new(Body::from(bytes)))
```

## Success Metrics

- [ ] `Payload::ToolCall` removed from hooteproto
- [ ] No `serde_json::Value` in Payload except `GardenQuery::variables`
- [x] holler has `dispatch.rs` with `json_to_payload()` (38 tools converted)
- [x] hootenanny dispatch matches on typed Payloads via TypedDispatcher
- [x] All existing tests pass (102 hootenanny + 43 hooteproto)

## Progress Log

### 2025-12-21

**Phase 2 Complete:** holler/src/dispatch.rs created with json_to_payload()
- 38 tools have typed dispatch
- Core tools covered: ABC, Orpheus, CAS, artifacts, jobs, graphs
- Falls back to Payload::ToolCall for: weave_*, garden audio/input, schedule, analyze

**Phase 3 Complete:** hooteproto/src/conversion.rs extended
- ~30 typed Payload→ToolRequest conversions
- TypedDispatcher in hootenanny handles typed requests
- JSON fallback remains for unconverted tools

**Remaining:** Phase 4 - remove ToolCall and serde_json from hooteproto
