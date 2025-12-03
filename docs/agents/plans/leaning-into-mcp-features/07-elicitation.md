# Phase 7: Elicitation

## Overview

Implement server-initiated user input requests. This allows the MCP server to ask the user for structured input via forms during tool execution.

**MCP Method**: `elicitation/create`

**Impact**: Medium - enables human-in-the-loop creative decisions

## Use Cases for Hootenanny

### Creative Decision Points

| Scenario | Elicitation |
|----------|-------------|
| Variation Selection | "Choose your favorite: [A] [B] [C]" |
| Key Selection | "Pick a key for continuation: C / Am / F / G" |
| Tempo Adjustment | "Enter desired BPM: [slider 60-180]" |
| Rating Generations | "Rate this clip: ★☆☆☆☆ to ★★★★★" |
| Naming Artifacts | "Name this artifact: [text input]" |
| Confirm Destructive | "Delete these 5 artifacts? [Yes] [No]" |

### When to Use Elicitation vs. Prompts

- **Prompts**: Pre-defined creative templates (start of flow)
- **Elicitation**: Mid-flow decisions requiring human input

Elicitation is for when the agent encounters a fork and wants human guidance.

## Current State

Baton has:
- `ClientCapabilities.elicitation` defined as a marker
- No elicitation request/response types
- No infrastructure for server → client requests with user interaction

## Target State

Server sends elicitation request:
```json
{
  "method": "elicitation/create",
  "params": {
    "message": "Choose your favorite variation:",
    "requestedSchema": {
      "type": "object",
      "properties": {
        "choice": {
          "type": "string",
          "enum": ["A", "B", "C"],
          "enumLabels": ["Variation A (upbeat)", "Variation B (mellow)", "Variation C (experimental)"]
        }
      },
      "required": ["choice"]
    }
  }
}
```

Client presents UI to user, returns:
```json
{
  "result": {
    "action": "accept",
    "content": { "choice": "B" }
  }
}
```

Or if user cancels:
```json
{
  "result": {
    "action": "decline"
  }
}
```

## Implementation Plan

### Step 1: Add Elicitation Types to Baton

**File**: `crates/baton/src/types/elicitation.rs` (new)

```rust
//! Elicitation Types
//!
//! Types for server-initiated user input requests.
//! Per MCP 2025-06-18 schema.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Elicitation request parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ElicitationRequest {
    /// Message to display to user
    pub message: String,

    /// JSON Schema for requested input
    /// Supports primitive types only: string, number, boolean, enum
    pub requested_schema: ElicitationSchema,
}

/// Schema for elicitation (subset of JSON Schema)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ElicitationSchema {
    #[serde(rename = "type")]
    pub schema_type: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<serde_json::Map<String, Value>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<Vec<String>>,
}

impl ElicitationSchema {
    /// Create a simple string input
    pub fn string_input(name: &str, description: &str) -> Self {
        let mut props = serde_json::Map::new();
        props.insert(
            name.to_string(),
            serde_json::json!({
                "type": "string",
                "description": description
            }),
        );

        Self {
            schema_type: "object".to_string(),
            properties: Some(props),
            required: Some(vec![name.to_string()]),
        }
    }

    /// Create a choice from enum
    pub fn choice(name: &str, options: &[(&str, &str)]) -> Self {
        let values: Vec<&str> = options.iter().map(|(v, _)| *v).collect();
        let labels: Vec<&str> = options.iter().map(|(_, l)| *l).collect();

        let mut props = serde_json::Map::new();
        props.insert(
            name.to_string(),
            serde_json::json!({
                "type": "string",
                "enum": values,
                "enumLabels": labels
            }),
        );

        Self {
            schema_type: "object".to_string(),
            properties: Some(props),
            required: Some(vec![name.to_string()]),
        }
    }

    /// Create a number input with optional range
    pub fn number_input(name: &str, min: Option<f64>, max: Option<f64>) -> Self {
        let mut prop = serde_json::json!({ "type": "number" });
        if let Some(min) = min {
            prop["minimum"] = serde_json::json!(min);
        }
        if let Some(max) = max {
            prop["maximum"] = serde_json::json!(max);
        }

        let mut props = serde_json::Map::new();
        props.insert(name.to_string(), prop);

        Self {
            schema_type: "object".to_string(),
            properties: Some(props),
            required: Some(vec![name.to_string()]),
        }
    }

    /// Create a boolean confirmation
    pub fn confirm(name: &str) -> Self {
        let mut props = serde_json::Map::new();
        props.insert(
            name.to_string(),
            serde_json::json!({ "type": "boolean" }),
        );

        Self {
            schema_type: "object".to_string(),
            properties: Some(props),
            required: Some(vec![name.to_string()]),
        }
    }
}

/// User's response to elicitation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ElicitationResponse {
    /// What the user did
    pub action: ElicitationAction,

    /// The input content (if accepted)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<Value>,

    /// Validation message if rejected due to validation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation_message: Option<String>,
}

/// User action in response to elicitation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ElicitationAction {
    /// User provided valid input
    Accept,
    /// User declined to provide input
    Decline,
    /// User cancelled the operation
    Cancel,
}

/// Error when elicitation fails
#[derive(Debug, Clone)]
pub enum ElicitationError {
    /// Client doesn't support elicitation
    NotSupported,
    /// User declined
    Declined,
    /// User cancelled
    Cancelled,
    /// Request timed out
    Timeout,
    /// Channel closed
    ChannelClosed,
    /// Validation failed
    ValidationFailed(String),
}
```

### Step 2: Add Elicitation Client (Similar to Sampling)

**File**: `crates/baton/src/transport/elicitation.rs` (new)

```rust
use dashmap::DashMap;
use tokio::sync::oneshot;
use std::time::Duration;

use crate::types::elicitation::{ElicitationRequest, ElicitationResponse, ElicitationError};
use crate::types::jsonrpc::JsonRpcMessage;
use crate::session::Session;

/// Client for sending elicitation requests
pub struct ElicitationClient {
    pending: DashMap<String, oneshot::Sender<ElicitationResponse>>,
}

impl ElicitationClient {
    pub fn new() -> Self {
        Self {
            pending: DashMap::new(),
        }
    }

    /// Send an elicitation request and wait for user response
    pub async fn elicit(
        &self,
        session: &Session,
        request: ElicitationRequest,
    ) -> Result<ElicitationResponse, ElicitationError> {
        // Check capability
        if !session.supports_elicitation() {
            return Err(ElicitationError::NotSupported);
        }

        let request_id = uuid::Uuid::new_v4().to_string();
        let (tx, rx) = oneshot::channel();

        self.pending.insert(request_id.clone(), tx);

        // Send request
        let message = JsonRpcMessage::request(
            &request_id,
            "elicitation/create",
            serde_json::to_value(&request).unwrap(),
        );

        session.send(message).await
            .map_err(|_| ElicitationError::ChannelClosed)?;

        // Wait for response (generous timeout for human input)
        match tokio::time::timeout(Duration::from_secs(300), rx).await {
            Ok(Ok(response)) => Ok(response),
            Ok(Err(_)) => Err(ElicitationError::ChannelClosed),
            Err(_) => {
                self.pending.remove(&request_id);
                Err(ElicitationError::Timeout)
            }
        }
    }

    /// Handle incoming response from client
    pub fn handle_response(&self, id: &str, response: ElicitationResponse) {
        if let Some((_, tx)) = self.pending.remove(id) {
            let _ = tx.send(response);
        }
    }
}
```

### Step 3: Add Elicitor to ToolContext

**File**: `crates/baton/src/protocol/mod.rs`

```rust
/// Handle for making elicitation requests
pub struct Elicitor {
    client: Arc<ElicitationClient>,
    session_id: String,
}

impl Elicitor {
    /// Request a simple choice
    pub async fn choose(
        &self,
        message: &str,
        options: &[(&str, &str)],
    ) -> Result<String, ElicitationError> {
        let request = ElicitationRequest {
            message: message.to_string(),
            requested_schema: ElicitationSchema::choice("choice", options),
        };

        let response = self.elicit(request).await?;

        match response.action {
            ElicitationAction::Accept => {
                response.content
                    .and_then(|c| c.get("choice").and_then(|v| v.as_str().map(String::from)))
                    .ok_or(ElicitationError::ValidationFailed("Missing choice".to_string()))
            }
            ElicitationAction::Decline => Err(ElicitationError::Declined),
            ElicitationAction::Cancel => Err(ElicitationError::Cancelled),
        }
    }

    /// Request a text input
    pub async fn text_input(
        &self,
        message: &str,
        field_name: &str,
    ) -> Result<String, ElicitationError> {
        let request = ElicitationRequest {
            message: message.to_string(),
            requested_schema: ElicitationSchema::string_input(field_name, message),
        };

        let response = self.elicit(request).await?;

        match response.action {
            ElicitationAction::Accept => {
                response.content
                    .and_then(|c| c.get(field_name).and_then(|v| v.as_str().map(String::from)))
                    .ok_or(ElicitationError::ValidationFailed("Missing input".to_string()))
            }
            ElicitationAction::Decline => Err(ElicitationError::Declined),
            ElicitationAction::Cancel => Err(ElicitationError::Cancelled),
        }
    }

    /// Request confirmation
    pub async fn confirm(&self, message: &str) -> Result<bool, ElicitationError> {
        let request = ElicitationRequest {
            message: message.to_string(),
            requested_schema: ElicitationSchema::confirm("confirmed"),
        };

        let response = self.elicit(request).await?;

        match response.action {
            ElicitationAction::Accept => {
                response.content
                    .and_then(|c| c.get("confirmed").and_then(|v| v.as_bool()))
                    .ok_or(ElicitationError::ValidationFailed("Missing confirmation".to_string()))
            }
            ElicitationAction::Decline => Ok(false),
            ElicitationAction::Cancel => Err(ElicitationError::Cancelled),
        }
    }

    /// Request a number in range
    pub async fn number(
        &self,
        message: &str,
        min: Option<f64>,
        max: Option<f64>,
    ) -> Result<f64, ElicitationError> {
        let request = ElicitationRequest {
            message: message.to_string(),
            requested_schema: ElicitationSchema::number_input("value", min, max),
        };

        let response = self.elicit(request).await?;

        match response.action {
            ElicitationAction::Accept => {
                response.content
                    .and_then(|c| c.get("value").and_then(|v| v.as_f64()))
                    .ok_or(ElicitationError::ValidationFailed("Missing number".to_string()))
            }
            ElicitationAction::Decline => Err(ElicitationError::Declined),
            ElicitationAction::Cancel => Err(ElicitationError::Cancelled),
        }
    }

    /// Full elicitation request
    pub async fn elicit(
        &self,
        request: ElicitationRequest,
    ) -> Result<ElicitationResponse, ElicitationError> {
        self.client.elicit_session(&self.session_id, request).await
    }
}

pub struct ToolContext {
    pub session_id: String,
    pub progress_token: Option<ProgressToken>,
    pub progress_sender: Option<ProgressSender>,
    pub sampler: Option<Sampler>,
    pub logger: McpLogger,
    pub notifier: ResourceNotifier,
    /// Elicitor for user input requests
    pub elicitor: Option<Elicitor>,
}
```

### Step 4: Use Elicitation in Hootenanny Tools

**File**: `crates/hootenanny/src/api/service.rs`

```rust
impl EventDualityServer {
    /// Interactive variation selection
    pub async fn select_variation(
        &self,
        variations: &[ArtifactId],
        context: &ToolContext,
    ) -> Result<ArtifactId, Error> {
        // If no elicitor, return first variation
        let Some(elicitor) = &context.elicitor else {
            return Ok(variations[0].clone());
        };

        let options: Vec<(&str, &str)> = variations.iter()
            .enumerate()
            .map(|(i, id)| {
                let label = format!("Variation {} ({})", i + 1, id.as_str());
                // Leak the string for the tuple (in practice, use a proper owned type)
                (id.as_str(), Box::leak(label.into_boxed_str()) as &str)
            })
            .collect();

        match elicitor.choose("Choose your favorite variation:", &options).await {
            Ok(choice) => Ok(ArtifactId::from(choice)),
            Err(ElicitationError::Declined) => {
                context.log_info("User skipped variation selection, using first").await;
                Ok(variations[0].clone())
            }
            Err(e) => Err(anyhow::anyhow!("Elicitation failed: {:?}", e)),
        }
    }

    /// Get artifact name from user
    pub async fn name_artifact(
        &self,
        artifact_id: &ArtifactId,
        context: &ToolContext,
    ) -> Result<Option<String>, Error> {
        let Some(elicitor) = &context.elicitor else {
            return Ok(None);
        };

        match elicitor.text_input(
            &format!("Name for artifact {}:", artifact_id.as_str()),
            "name",
        ).await {
            Ok(name) => Ok(Some(name)),
            Err(ElicitationError::Declined) => Ok(None),
            Err(e) => {
                context.log_warning(format!("Naming failed: {:?}", e)).await;
                Ok(None)
            }
        }
    }

    /// Confirm destructive operation
    pub async fn confirm_delete(
        &self,
        artifact_ids: &[ArtifactId],
        context: &ToolContext,
    ) -> Result<bool, Error> {
        let Some(elicitor) = &context.elicitor else {
            // No elicitor = assume agent knows what it's doing
            return Ok(true);
        };

        elicitor.confirm(&format!(
            "Delete {} artifacts? This cannot be undone.",
            artifact_ids.len()
        )).await.map_err(|e| anyhow::anyhow!("Confirmation failed: {:?}", e))
    }

    /// Get tempo from user
    pub async fn get_tempo(
        &self,
        context: &ToolContext,
    ) -> Result<Option<f64>, Error> {
        let Some(elicitor) = &context.elicitor else {
            return Ok(None);
        };

        match elicitor.number(
            "Enter desired BPM:",
            Some(40.0),
            Some(200.0),
        ).await {
            Ok(bpm) => Ok(Some(bpm)),
            Err(ElicitationError::Declined) => Ok(None),
            Err(e) => {
                context.log_warning(format!("Tempo input failed: {:?}", e)).await;
                Ok(None)
            }
        }
    }
}
```

### Step 5: Add Tool for Direct Elicitation

**File**: `crates/hootenanny/src/api/handler.rs`

```rust
Tool::new("elicit_choice", "Ask the user to choose from options")
    .with_input_schema(schema_for::<ElicitChoiceRequest>())
    .with_output_schema(schema_for::<ElicitChoiceResponse>()),

Tool::new("elicit_input", "Ask the user for text input")
    .with_input_schema(schema_for::<ElicitInputRequest>())
    .with_output_schema(schema_for::<ElicitInputResponse>()),

Tool::new("elicit_confirm", "Ask the user to confirm an action")
    .with_input_schema(schema_for::<ElicitConfirmRequest>())
    .with_output_schema(schema_for::<ElicitConfirmResponse>()),
```

These tools let agents explicitly request user input when they need it.

### Step 6: Check Client Capability

**File**: `crates/baton/src/session/mod.rs`

```rust
impl Session {
    pub fn supports_elicitation(&self) -> bool {
        self.client_capabilities
            .as_ref()
            .map(|c| c.elicitation.is_some())
            .unwrap_or(false)
    }
}
```

### Step 7: Unit Tests

**File**: `crates/baton/src/types/elicitation_tests.rs`

```rust
#[test]
fn test_elicitation_schema_choice() {
    let schema = ElicitationSchema::choice("key", &[
        ("C", "C Major"),
        ("Am", "A Minor"),
        ("F", "F Major"),
    ]);

    let json = serde_json::to_value(&schema).unwrap();
    let props = &json["properties"]["key"];
    assert_eq!(props["enum"].as_array().unwrap().len(), 3);
    assert_eq!(props["enumLabels"][0], "C Major");
}

#[test]
fn test_elicitation_response_accept() {
    let response = ElicitationResponse {
        action: ElicitationAction::Accept,
        content: Some(serde_json::json!({"choice": "Am"})),
        validation_message: None,
    };

    let json = serde_json::to_value(&response).unwrap();
    assert_eq!(json["action"], "accept");
    assert_eq!(json["content"]["choice"], "Am");
}
```

### Step 8: Live Testing

1. Rebuild and reconnect MCP
2. Call a tool that triggers elicitation
3. Verify UI appears in client
4. Make selection
5. Verify tool receives the response
6. Test decline and cancel flows

## Files Changed

| File | Change |
|------|--------|
| `crates/baton/src/types/elicitation.rs` | New - elicitation types |
| `crates/baton/src/types/mod.rs` | Export elicitation |
| `crates/baton/src/transport/elicitation.rs` | New - elicitation client |
| `crates/baton/src/transport/mod.rs` | Export elicitation client |
| `crates/baton/src/protocol/mod.rs` | Add Elicitor to ToolContext |
| `crates/baton/src/session/mod.rs` | Add supports_elicitation |
| `crates/hootenanny/src/api/handler.rs` | Add elicit_* tools |
| `crates/hootenanny/src/api/service.rs` | Add elicitation helpers |
| `crates/hootenanny/src/api/schema.rs` | Add elicit request/response types |

## Verification Checklist

- [ ] Elicitation types compile
- [ ] Client capability detected correctly
- [ ] elicitation/create sent correctly
- [ ] Response handling works
- [ ] Timeout works (5 minute default)
- [ ] Decline/cancel handled gracefully
- [ ] Elicit tools work end-to-end
- [ ] Unit tests pass
- [ ] Live test with user interaction

## Edge Cases

1. **Client doesn't support elicitation**: Return defaults or skip optional inputs
2. **User takes too long**: 5 minute timeout, return error
3. **User cancels**: Propagate cancellation appropriately
4. **Validation fails**: Return error, don't retry automatically

## Notes for Next Agent

After this phase:
- Server can request structured user input
- Human-in-the-loop decisions are possible
- Creative choices can be delegated to humans
- All MCP 2025-06-18 features are implemented

**This completes the plan.** All seven phases are documented and ready for implementation.

## Celebration Checklist 🎉

When all phases are complete:
- [ ] All MCP 2025-06-18 spec features implemented in baton
- [ ] All 59+ hootenanny tools updated
- [ ] Progress notifications working (Phase 1)
- [ ] Output schemas on all tools (Phase 2)
- [ ] Sampling from client LLM (Phase 3)
- [ ] Argument completions (Phase 4)
- [ ] Structured logging (Phase 5)
- [ ] Resource subscriptions (Phase 6)
- [ ] User elicitation (Phase 7)
- [ ] Clean handoffs documented
- [ ] Tests passing
- [ ] Live testing successful
