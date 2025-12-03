# Phase 2: Output Schemas

## Overview

Add structured output schemas to tools. This enables typed responses that agents can parse reliably, and allows MCP clients to validate tool results.

**MCP Feature**: `Tool.outputSchema` + `CallToolResult.structuredContent`

**Impact**: Medium - improves type safety and agent reasoning across all tools

## Current State

Today, tools return unstructured text in `CallToolResult.content`:
```json
{
  "content": [{"type": "text", "text": "{\"job_id\": \"job_abc\", \"status\": \"pending\"}"}],
  "isError": false
}
```

Agents must parse the JSON from the text field. There's no schema telling them what to expect.

## Target State

With output schemas:
```json
{
  "content": [{"type": "text", "text": "Job started: job_abc"}],
  "isError": false,
  "structuredContent": {
    "job_id": "job_abc",
    "status": "pending",
    "artifact_id": null
  }
}
```

The tool definition includes:
```json
{
  "name": "orpheus_generate",
  "outputSchema": {
    "type": "object",
    "properties": {
      "job_id": {"type": "string"},
      "status": {"type": "string", "enum": ["pending", "running", "completed", "failed"]},
      "artifact_id": {"type": ["string", "null"]}
    },
    "required": ["job_id", "status"]
  }
}
```

## Implementation Plan

### Step 1: Define Output Schema Types

We already have `ToolSchema` and `CallToolResult.structured_content`. Just need to use them.

**File**: `crates/hootenanny/src/api/schema.rs`

Add response types alongside request types:

```rust
/// Response from job-spawning tools
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct JobSpawnResponse {
    pub job_id: String,
    pub status: JobStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

/// Response from CAS store
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CasStoreResponse {
    pub hash: String,
    pub size_bytes: u64,
    pub mime_type: String,
}

/// Response from graph_find
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GraphFindResponse {
    pub identities: Vec<IdentitySummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct IdentitySummary {
    pub id: String,
    pub name: String,
    pub tags: Vec<String>,
}

// ... define response types for all tools
```

### Step 2: Add Output Schemas to Tool Definitions

**File**: `crates/hootenanny/src/api/handler.rs`

Update `tools()` to include output schemas:

```rust
fn tools(&self) -> Vec<Tool> {
    vec![
        // Job-spawning tools share the same output schema
        Tool::new("orpheus_generate", "Generate MIDI with the Orpheus model")
            .with_input_schema(schema_for::<OrpheusGenerateRequest>())
            .with_output_schema(schema_for::<JobSpawnResponse>()),

        Tool::new("orpheus_continue", "Generate continuation clip from MIDI")
            .with_input_schema(schema_for::<OrpheusContinueRequest>())
            .with_output_schema(schema_for::<JobSpawnResponse>()),

        // CAS tools
        Tool::new("cas_store", "Store content in the CAS")
            .with_input_schema(schema_for::<CasStoreRequest>())
            .with_output_schema(schema_for::<CasStoreResponse>()),

        Tool::new("cas_inspect", "Inspect content in the CAS by hash")
            .with_input_schema(schema_for::<CasInspectRequest>())
            .with_output_schema(schema_for::<CasInspectResponse>())
            .read_only(),

        // Graph tools
        Tool::new("graph_find", "Find identities in the audio graph")
            .with_input_schema(schema_for::<GraphFindRequest>())
            .with_output_schema(schema_for::<GraphFindResponse>())
            .read_only(),

        // ... all 59 tools
    ]
}
```

### Step 3: Return Structured Content from Tools

**File**: `crates/hootenanny/src/api/service.rs`

Update each tool implementation to return both text (for humans) and structured content (for agents):

```rust
impl EventDualityServer {
    pub async fn orpheus_generate(
        &self,
        request: OrpheusGenerateRequest,
    ) -> Result<CallToolResult, ErrorData> {
        let job_id = self.job_manager.spawn(/* ... */).await;

        let response = JobSpawnResponse {
            job_id: job_id.clone(),
            status: JobStatus::Pending,
            artifact_id: None,
            message: Some("Orpheus generation started".to_string()),
        };

        Ok(CallToolResult::text(format!("Started job: {}", job_id))
            .with_structured(serde_json::to_value(&response).unwrap()))
    }

    pub async fn cas_store(
        &self,
        request: CasStoreRequest,
    ) -> Result<CallToolResult, ErrorData> {
        let (hash, size) = self.cas.store(/* ... */).await?;

        let response = CasStoreResponse {
            hash: hash.clone(),
            size_bytes: size,
            mime_type: request.mime_type.clone(),
        };

        Ok(CallToolResult::text(format!("Stored {} bytes as {}", size, hash))
            .with_structured(serde_json::to_value(&response).unwrap()))
    }

    // ... update all tools
}
```

### Step 4: Categorize Tools by Response Type

Group tools by their output schema to ensure consistency:

**Job-Spawning Tools** → `JobSpawnResponse`:
- `orpheus_generate`, `orpheus_generate_seeded`, `orpheus_continue`, `orpheus_bridge`, `orpheus_loops`
- `convert_midi_to_wav`
- `musicgen_generate`
- `anticipatory_generate`, `anticipatory_continue`
- `yue_generate`

**CAS Tools**:
- `cas_store` → `CasStoreResponse`
- `cas_inspect` → `CasInspectResponse`
- `cas_upload_file` → `CasUploadResponse`

**Graph Tools**:
- `graph_bind` → `GraphBindResponse`
- `graph_tag` → `GraphTagResponse`
- `graph_connect` → `GraphConnectResponse`
- `graph_find` → `GraphFindResponse`
- `graph_context` → `GraphContextResponse`
- `graph_query` → `GraphQueryResponse`

**Job Management**:
- `job_status` → `JobStatusResponse`
- `job_list` → `JobListResponse`
- `job_cancel` → `JobCancelResponse`
- `job_poll` → `JobPollResponse`
- `job_sleep` → `JobSleepResponse`

**Analysis Tools** → `AnalysisResponse`:
- `orpheus_classify`
- `clap_analyze`
- `beatthis_analyze`
- `anticipatory_embed`

**ABC Tools**:
- `abc_parse` → `AbcParseResponse`
- `abc_to_midi` → `AbcToMidiResponse`
- `abc_validate` → `AbcValidateResponse`
- `abc_transpose` → `AbcTransposeResponse`

**SoundFont Tools**:
- `soundfont_inspect` → `SoundfontInspectResponse`
- `soundfont_preset_inspect` → `SoundfontPresetResponse`

**Annotation Tools**:
- `add_annotation` → `AddAnnotationResponse`

### Step 5: Create Response Type File

**File**: `crates/hootenanny/src/api/responses.rs` (new)

Centralize all response types:

```rust
//! Response types for MCP tool output schemas
//!
//! These types define the structured content returned by tools.
//! Each implements JsonSchema for output schema generation.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// === Job Management ===

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct JobSpawnResponse {
    pub job_id: String,
    pub status: JobStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

// ... all response types
```

### Step 6: Unit Tests

**File**: `crates/hootenanny/src/api/responses_tests.rs`

```rust
#[test]
fn test_job_spawn_response_schema() {
    let schema = schema_for::<JobSpawnResponse>();
    let json = serde_json::to_value(&schema).unwrap();
    assert!(json["properties"]["job_id"].is_object());
    assert!(json["properties"]["status"]["enum"].is_array());
}

#[test]
fn test_structured_content_roundtrip() {
    let response = JobSpawnResponse {
        job_id: "job_abc".to_string(),
        status: JobStatus::Pending,
        artifact_id: None,
        message: None,
    };
    let json = serde_json::to_value(&response).unwrap();
    let parsed: JobSpawnResponse = serde_json::from_value(json).unwrap();
    assert_eq!(parsed.job_id, "job_abc");
}
```

### Step 7: Live Testing

1. Rebuild and reconnect MCP
2. Call `tools/list` - verify output schemas appear
3. Call various tools - verify `structuredContent` in responses
4. Test that agents can parse structured content

## Files Changed

| File | Change |
|------|--------|
| `crates/hootenanny/src/api/responses.rs` | New - all response types |
| `crates/hootenanny/src/api/schema.rs` | Import responses, maybe move requests here |
| `crates/hootenanny/src/api/handler.rs` | Add output schemas to all tools |
| `crates/hootenanny/src/api/service.rs` | Return structured content from all tools |
| `crates/hootenanny/src/api/mod.rs` | Export responses module |

## Verification Checklist

- [ ] All response types defined with JsonSchema
- [ ] All 59 tools have output schemas
- [ ] All tool implementations return structured content
- [ ] `tools/list` response includes outputSchema for each tool
- [ ] Structured content validates against output schema
- [ ] Unit tests pass
- [ ] Live test shows structured content in tool results

## Response Type Reference

For the implementing agent, here's a quick reference of what each category needs:

| Category | Response Type | Key Fields |
|----------|--------------|------------|
| Job spawn | `JobSpawnResponse` | job_id, status, artifact_id?, content_hash? |
| Job complete | `JobCompleteResponse` | artifact_id, content_hash, duration_ms |
| CAS store | `CasStoreResponse` | hash, size_bytes, mime_type |
| CAS inspect | `CasInspectResponse` | hash, size_bytes, mime_type, exists |
| Graph bind | `GraphBindResponse` | id, name, created_at |
| Graph find | `GraphFindResponse` | identities: [{id, name, tags}] |
| Analysis | `AnalysisResponse` | result, confidence?, metadata |
| ABC parse | `AbcParseResponse` | ast, errors, warnings |

## Notes for Next Agent

After this phase:
- Every tool has a defined output schema
- Agents can rely on structured content instead of parsing text
- The response type system is established for future tools
- You understand the full tool surface area

This sets up Phase 3 (sampling) where the server will be able to request LLM help and receive structured responses back.
