# Phase 3: Sampling

## Overview

Implement server-initiated LLM sampling. This allows the MCP server to request inference from the connected client's LLM, enabling inline analysis without external API calls.

**MCP Method**: `sampling/createMessage`

**Impact**: High - enables new agent collaboration patterns

## Context

### Relationship to agent_chat_*

The existing `agent_chat_*` tools spawn persistent sub-agents using external LLM backends (DeepSeek, Ollama). These serve a different purpose:
- **agent_chat**: Long-running collaborators with tool access and conversation memory
- **sampling**: Quick inline questions to the client's LLM (no tool access, single turn)

Both stay. Sampling is for "quick questions", agent_chat for "persistent collaborators".

### Use Cases for Sampling

1. **MIDI Analysis**: "Describe the mood and characteristics of this MIDI"
2. **Vibe Extraction**: "What 3-5 keywords describe this generated clip?"
3. **Orchestration Hints**: "Given this bass line, suggest complementary instruments"
4. **Error Explanation**: "Why might this generation have failed?"
5. **Creative Prompts**: "Generate a text prompt for MusicGen based on this MIDI analysis"

## Current State

Baton has `ClientCapabilities.sampling` defined but not implemented:
```rust
pub struct SamplingCapability {}  // Just a marker
```

No handler exists for `sampling/createMessage`.

## Target State

The server can call:
```rust
let response = state.sample(SamplingRequest {
    messages: vec![
        SamplingMessage::user("Describe the mood of this MIDI in 3 keywords"),
    ],
    model_preferences: Some(ModelPreferences {
        hints: vec![ModelHint::new("claude-sonnet-4-20250514")],
        intelligence_priority: Some(0.5),
        speed_priority: Some(0.8),
    }),
    max_tokens: Some(100),
}).await?;
```

And receive:
```rust
SamplingResponse {
    role: Role::Assistant,
    content: Content::text("melancholic, atmospheric, sparse"),
    model: "claude-sonnet-4-20250514",
    stop_reason: Some(StopReason::EndTurn),
}
```

## Implementation Plan

### Step 1: Add Sampling Types to Baton

**File**: `crates/baton/src/types/sampling.rs` (new)

```rust
//! Sampling Types
//!
//! Types for server-initiated LLM sampling requests.
//! Per MCP 2025-06-18 schema.

use serde::{Deserialize, Serialize};
use super::content::Content;

/// Role in a sampling conversation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
}

/// A message in a sampling request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamplingMessage {
    pub role: Role,
    pub content: Content,
}

impl SamplingMessage {
    pub fn user(text: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: Content::text(text),
        }
    }

    pub fn assistant(text: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: Content::text(text),
        }
    }
}

/// Model preferences for sampling
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelPreferences {
    /// Hints for model selection
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hints: Option<Vec<ModelHint>>,

    /// Priority for model intelligence (0.0-1.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intelligence_priority: Option<f64>,

    /// Priority for response speed (0.0-1.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed_priority: Option<f64>,

    /// Priority for cost efficiency (0.0-1.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_priority: Option<f64>,
}

/// Hint for model selection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelHint {
    /// Model name pattern
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl ModelHint {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: Some(name.into()) }
    }
}

/// Sampling request parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SamplingRequest {
    /// Messages to send
    pub messages: Vec<SamplingMessage>,

    /// Model preferences
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_preferences: Option<ModelPreferences>,

    /// System prompt
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,

    /// Include context from MCP servers
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_context: Option<IncludeContext>,

    /// Temperature (0.0-2.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,

    /// Maximum tokens to generate
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,

    /// Stop sequences
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_sequences: Option<Vec<String>>,

    /// Metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// What context to include
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum IncludeContext {
    None,
    ThisServer,
    AllServers,
}

/// Sampling response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SamplingResponse {
    /// Role (always assistant)
    pub role: Role,

    /// Response content
    pub content: Content,

    /// Model that generated the response
    pub model: String,

    /// Why the model stopped
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<StopReason>,
}

/// Reason the model stopped generating
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    EndTurn,
    StopSequence,
    MaxTokens,
}
```

### Step 2: Add Sampling Client to McpState

The server needs to send requests TO the client. This inverts the normal flow.

**File**: `crates/baton/src/transport/mod.rs`

```rust
/// Pending sampling requests waiting for client response
pub struct SamplingClient {
    pending: DashMap<String, oneshot::Sender<SamplingResponse>>,
}

impl SamplingClient {
    /// Send a sampling request and wait for response
    pub async fn sample(
        &self,
        session: &Session,
        request: SamplingRequest,
    ) -> Result<SamplingResponse, SamplingError> {
        let request_id = Uuid::new_v4().to_string();
        let (tx, rx) = oneshot::channel();

        self.pending.insert(request_id.clone(), tx);

        // Send JSON-RPC request to client via SSE
        let message = JsonRpcMessage::request(
            &request_id,
            "sampling/createMessage",
            serde_json::to_value(&request)?,
        );

        session.send(message).await?;

        // Wait for response with timeout
        match tokio::time::timeout(Duration::from_secs(60), rx).await {
            Ok(Ok(response)) => Ok(response),
            Ok(Err(_)) => Err(SamplingError::ChannelClosed),
            Err(_) => Err(SamplingError::Timeout),
        }
    }

    /// Handle incoming response from client
    pub fn handle_response(&self, id: &str, result: SamplingResponse) {
        if let Some((_, tx)) = self.pending.remove(id) {
            let _ = tx.send(result);
        }
    }
}
```

### Step 3: Wire Sampling into Dispatch

**File**: `crates/baton/src/protocol/mod.rs`

Handle sampling responses coming back from the client:

```rust
async fn dispatch_inner<H: Handler>(
    state: &Arc<McpState<H>>,
    session_id: &str,
    message: &JsonRpcMessage,
) -> Result<Value, ErrorData> {
    match message.method.as_str() {
        // ... existing handlers ...

        // This is a RESPONSE to our sampling request
        // (JSON-RPC responses have no method, handled separately)
        _ => Err(ErrorData::method_not_found(&message.method)),
    }
}

// Separate handler for JSON-RPC responses (not requests)
pub async fn handle_response<H: Handler>(
    state: &Arc<McpState<H>>,
    response: &JsonRpcResponse,
) {
    if let Some(id) = &response.id {
        if let Some(result) = &response.result {
            if let Ok(sampling_result) = serde_json::from_value::<SamplingResponse>(result.clone()) {
                state.sampling_client.handle_response(&id.to_string(), sampling_result);
            }
        }
    }
}
```

### Step 4: Check Client Capability

Only attempt sampling if client advertised the capability:

**File**: `crates/baton/src/session/mod.rs`

```rust
impl Session {
    pub fn supports_sampling(&self) -> bool {
        self.client_capabilities
            .as_ref()
            .map(|c| c.sampling.is_some())
            .unwrap_or(false)
    }
}
```

Store client capabilities during initialize:

```rust
pub struct Session {
    // ... existing fields
    pub client_capabilities: Option<ClientCapabilities>,
}
```

### Step 5: Add Sampling Helper to Handler Context

Extend `ToolContext` from Phase 1:

**File**: `crates/baton/src/protocol/mod.rs`

```rust
pub struct ToolContext {
    pub session_id: String,
    pub progress_token: Option<ProgressToken>,
    pub progress_sender: Option<ProgressSender>,
    /// Sampler for requesting LLM inference from client
    pub sampler: Option<Sampler>,
}

/// Handle for making sampling requests
pub struct Sampler {
    client: Arc<SamplingClient>,
    session_id: String,
}

impl Sampler {
    /// Request a simple text completion
    pub async fn ask(&self, question: &str) -> Result<String, SamplingError> {
        let request = SamplingRequest {
            messages: vec![SamplingMessage::user(question)],
            max_tokens: Some(500),
            ..Default::default()
        };

        let response = self.sample(request).await?;
        Ok(response.content.as_text().unwrap_or_default().to_string())
    }

    /// Request sampling with full control
    pub async fn sample(&self, request: SamplingRequest) -> Result<SamplingResponse, SamplingError> {
        self.client.sample(&self.session_id, request).await
    }
}
```

### Step 6: Use Sampling in Hootenanny Tools

**File**: `crates/hootenanny/src/api/service.rs`

Example: Add vibe extraction after generation:

```rust
impl EventDualityServer {
    pub async fn orpheus_generate_with_progress(
        &self,
        request: OrpheusGenerateRequest,
        context: ToolContext,
    ) -> Result<CallToolResult, ErrorData> {
        // ... run generation ...

        // If sampling available, extract vibes
        if let Some(sampler) = &context.sampler {
            if let Ok(vibes) = sampler.ask(
                "Describe this MIDI's mood in 3-5 keywords, comma-separated"
            ).await {
                // Store vibes as annotation on artifact
                self.artifact_store.annotate(artifact_id, "vibe", &vibes).await?;
            }
        }

        // ... return result
    }
}
```

### Step 7: Add Sampling Tool for Direct Use

**File**: `crates/hootenanny/src/api/handler.rs`

Add a tool that exposes sampling directly (useful for testing):

```rust
Tool::new("sample_llm", "Request LLM inference from the connected client")
    .with_input_schema(schema_for::<SampleLlmRequest>())
    .with_output_schema(schema_for::<SampleLlmResponse>()),
```

This is meta - the server tool calls back to the client's LLM.

### Step 8: Unit Tests

**File**: `crates/baton/src/types/sampling_tests.rs`

```rust
#[test]
fn test_sampling_request_serialization() {
    let request = SamplingRequest {
        messages: vec![SamplingMessage::user("Hello")],
        max_tokens: Some(100),
        ..Default::default()
    };
    let json = serde_json::to_value(&request).unwrap();
    assert_eq!(json["messages"][0]["role"], "user");
}

#[tokio::test]
async fn test_sampling_roundtrip() {
    // Mock client that responds to sampling requests
    // ...
}
```

### Step 9: Live Testing

1. Rebuild and reconnect MCP
2. Verify `initialize` response shows sampling is understood
3. Call a generation tool
4. Observe sampling request in client logs
5. Verify vibes are extracted and stored

## Files Changed

| File | Change |
|------|--------|
| `crates/baton/src/types/sampling.rs` | New - sampling types |
| `crates/baton/src/types/mod.rs` | Export sampling |
| `crates/baton/src/transport/mod.rs` | Add SamplingClient |
| `crates/baton/src/protocol/mod.rs` | Handle sampling responses, add Sampler |
| `crates/baton/src/session/mod.rs` | Store client capabilities |
| `crates/hootenanny/src/api/handler.rs` | Add sample_llm tool |
| `crates/hootenanny/src/api/service.rs` | Use sampling in generation tools |

## Verification Checklist

- [ ] Sampling types compile and serialize correctly
- [ ] Client capabilities stored during initialize
- [ ] SamplingClient can send requests via SSE
- [ ] Responses are matched to pending requests
- [ ] Sampler helper works in ToolContext
- [ ] Vibe extraction works in generation tools
- [ ] sample_llm tool works end-to-end
- [ ] Unit tests pass
- [ ] Live test with Claude Code

## Edge Cases

1. **Client doesn't support sampling**: Check `supports_sampling()` before attempting
2. **Sampling timeout**: Return gracefully, don't fail the whole tool
3. **Client returns error**: Handle JSON-RPC error responses
4. **Multiple concurrent samples**: Use request IDs to match responses

## Notes for Next Agent

After this phase:
- The server can request LLM inference from the client
- Vibes are automatically extracted for generated artifacts
- You understand bidirectional MCP communication
- The sampling infrastructure can be used for other inline analysis

**Checkpoint Suggestion**: This is a good point to clear context and start fresh. Phases 1-3 represent the major infrastructure. Phases 4-7 are more incremental features.

Phase 4 (completions) adds discoverability UX but is less architecturally significant.
