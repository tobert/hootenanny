# Phase 6: Resource Subscriptions

## Overview

Implement resource subscriptions for push-based resource updates. This allows clients to subscribe to resources and receive notifications when they change.

**MCP Methods**:
- `resources/subscribe` - Subscribe to a resource
- `resources/unsubscribe` - Unsubscribe from a resource
- `notifications/resources/updated` - Server notifies of resource change
- `notifications/resources/list_changed` - Server notifies that resource list changed

**Impact**: Medium - enables real-time awareness of system state

## Use Cases

### High-Value Subscriptions

| Resource | Why Subscribe |
|----------|---------------|
| `artifacts://recent` | Know when new artifacts are created |
| `artifacts://summary` | Track session progress (total counts) |
| `graph://identities` | Know when new devices are added |
| `graph://connections` | Know when patch cables change |
| `artifacts://by-tag/type:midi` | Track MIDI generation specifically |

### Multi-Agent Scenarios

With subscriptions, multiple agents connected to the same hootenanny can:
- See each other's artifacts in real-time
- React to graph changes made by other agents
- Coordinate without polling

## Current State

Baton has:
- `ResourcesCapability.subscribe` flag defined
- No subscribe/unsubscribe handlers
- No subscription tracking in sessions
- No notification emission infrastructure

## Target State

Client subscribes:
```json
{"method": "resources/subscribe", "params": {"uri": "artifacts://recent"}}
```

Later, when an artifact is created:
```json
{"method": "notifications/resources/updated", "params": {"uri": "artifacts://recent"}}
```

Client can then call `resources/read` to get the updated content.

## Implementation Plan

### Step 1: Add Subscription Types to Baton

**File**: `crates/baton/src/types/subscription.rs` (new)

```rust
//! Resource Subscription Types
//!
//! Types for resource subscription management.
//! Per MCP 2025-06-18 schema.

use serde::{Deserialize, Serialize};

/// Subscribe request parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscribeParams {
    /// URI of resource to subscribe to
    pub uri: String,
}

/// Unsubscribe request parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnsubscribeParams {
    /// URI of resource to unsubscribe from
    pub uri: String,
}

/// Resource updated notification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUpdatedNotification {
    /// URI of the updated resource
    pub uri: String,
}

/// Resources list changed notification (empty params)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResourcesListChangedNotification {}
```

### Step 2: Add Subscription Tracking to Session

**File**: `crates/baton/src/session/mod.rs`

```rust
use std::collections::HashSet;

pub struct Session {
    // ... existing fields
    /// Resources this session is subscribed to
    pub subscriptions: HashSet<String>,
}

impl Session {
    pub fn subscribe(&mut self, uri: &str) {
        self.subscriptions.insert(uri.to_string());
    }

    pub fn unsubscribe(&mut self, uri: &str) {
        self.subscriptions.remove(uri);
    }

    pub fn is_subscribed(&self, uri: &str) -> bool {
        self.subscriptions.contains(uri)
    }
}
```

### Step 3: Add Subscribe/Unsubscribe Handlers

**File**: `crates/baton/src/protocol/mod.rs`

```rust
async fn dispatch_inner<H: Handler>(/* ... */) -> Result<Value, ErrorData> {
    match message.method.as_str() {
        // ... existing handlers ...

        "resources/subscribe" => handle_subscribe(state, session_id, message).await,
        "resources/unsubscribe" => handle_unsubscribe(state, session_id, message).await,

        // ...
    }
}

async fn handle_subscribe<H: Handler>(
    state: &Arc<McpState<H>>,
    session_id: &str,
    request: &JsonRpcMessage,
) -> Result<Value, ErrorData> {
    let params: SubscribeParams = request
        .params
        .as_ref()
        .map(|p| serde_json::from_value(p.clone()))
        .transpose()
        .map_err(|e| ErrorData::invalid_params(e.to_string()))?
        .ok_or_else(|| ErrorData::invalid_params("Missing subscribe params"))?;

    // Verify the resource exists
    let resources = state.handler.resources();
    let templates = state.handler.resource_templates();

    let resource_exists = resources.iter().any(|r| r.uri == params.uri)
        || templates.iter().any(|t| uri_matches_template(&params.uri, &t.uri_template));

    if !resource_exists {
        return Err(ErrorData::invalid_params(format!(
            "Resource not found: {}",
            params.uri
        )));
    }

    if let Some(mut session) = state.sessions.get_mut(session_id) {
        session.subscribe(&params.uri);
        tracing::info!(
            session_id = %session_id,
            uri = %params.uri,
            "Subscribed to resource"
        );
    }

    Ok(serde_json::json!({}))
}

async fn handle_unsubscribe<H: Handler>(
    state: &Arc<McpState<H>>,
    session_id: &str,
    request: &JsonRpcMessage,
) -> Result<Value, ErrorData> {
    let params: UnsubscribeParams = request
        .params
        .as_ref()
        .map(|p| serde_json::from_value(p.clone()))
        .transpose()
        .map_err(|e| ErrorData::invalid_params(e.to_string()))?
        .ok_or_else(|| ErrorData::invalid_params("Missing unsubscribe params"))?;

    if let Some(mut session) = state.sessions.get_mut(session_id) {
        session.unsubscribe(&params.uri);
        tracing::info!(
            session_id = %session_id,
            uri = %params.uri,
            "Unsubscribed from resource"
        );
    }

    Ok(serde_json::json!({}))
}

/// Check if a URI matches a template
fn uri_matches_template(uri: &str, template: &str) -> bool {
    // Simple matching - could use a proper RFC 6570 library
    let template_parts: Vec<&str> = template.split('/').collect();
    let uri_parts: Vec<&str> = uri.split('/').collect();

    if template_parts.len() != uri_parts.len() {
        return false;
    }

    template_parts.iter().zip(uri_parts.iter()).all(|(t, u)| {
        t.starts_with('{') && t.ends_with('}') || t == u
    })
}
```

### Step 4: Add Subscription Notifier

**File**: `crates/baton/src/transport/mod.rs`

```rust
/// Notifier for sending resource updates to subscribers
pub struct ResourceNotifier {
    sessions: Arc<dyn SessionStore>,
}

impl ResourceNotifier {
    /// Notify all subscribers that a resource was updated
    pub async fn notify_updated(&self, uri: &str) {
        let notification = JsonRpcMessage::notification(
            "notifications/resources/updated",
            serde_json::json!({ "uri": uri }),
        );

        for session in self.sessions.all_sessions() {
            if session.is_subscribed(uri) {
                if let Some(tx) = &session.tx {
                    let _ = tx.send(notification.clone()).await;
                }
            }
        }
    }

    /// Notify all sessions that the resource list changed
    pub async fn notify_list_changed(&self) {
        let notification = JsonRpcMessage::notification(
            "notifications/resources/list_changed",
            serde_json::json!({}),
        );

        for session in self.sessions.all_sessions() {
            if let Some(tx) = &session.tx {
                let _ = tx.send(notification.clone()).await;
            }
        }
    }

    /// Notify about a specific resource pattern (for templated resources)
    pub async fn notify_pattern(&self, pattern: &str) {
        // For templated resources, notify anyone subscribed to matching URIs
        for session in self.sessions.all_sessions() {
            for uri in &session.subscriptions {
                if uri_matches_pattern(uri, pattern) {
                    if let Some(tx) = &session.tx {
                        let notification = JsonRpcMessage::notification(
                            "notifications/resources/updated",
                            serde_json::json!({ "uri": uri }),
                        );
                        let _ = tx.send(notification).await;
                    }
                }
            }
        }
    }
}

fn uri_matches_pattern(uri: &str, pattern: &str) -> bool {
    // Pattern matching for subscription notifications
    // e.g., pattern "artifacts://*" matches "artifacts://recent"
    if pattern.ends_with("*") {
        let prefix = &pattern[..pattern.len() - 1];
        uri.starts_with(prefix)
    } else {
        uri == pattern
    }
}
```

### Step 5: Add Notifier to ToolContext

**File**: `crates/baton/src/protocol/mod.rs`

```rust
pub struct ToolContext {
    pub session_id: String,
    pub progress_token: Option<ProgressToken>,
    pub progress_sender: Option<ProgressSender>,
    pub sampler: Option<Sampler>,
    pub logger: McpLogger,
    /// Resource notifier for subscription updates
    pub notifier: ResourceNotifier,
}

impl ToolContext {
    pub async fn notify_resource_updated(&self, uri: &str) {
        self.notifier.notify_updated(uri).await;
    }
}
```

### Step 6: Emit Notifications from Hootenanny Tools

**File**: `crates/hootenanny/src/api/service.rs`

```rust
impl EventDualityServer {
    pub async fn create_artifact_with_context(
        &self,
        artifact: Artifact,
        context: &ToolContext,
    ) -> Result<ArtifactId, Error> {
        let id = self.artifact_store.write()?.insert(artifact)?;

        // Notify subscribers
        context.notify_resource_updated("artifacts://recent").await;
        context.notify_resource_updated("artifacts://summary").await;

        // Also notify tag-specific subscriptions
        for tag in &artifact.tags {
            context.notify_resource_updated(
                &format!("artifacts://by-tag/{}", tag)
            ).await;
        }

        Ok(id)
    }

    pub async fn graph_bind_with_context(
        &self,
        request: GraphBindRequest,
        context: &ToolContext,
    ) -> Result<CallToolResult, ErrorData> {
        let result = graph_bind(&self.audio_graph_db, /* ... */)?;

        // Notify graph subscribers
        context.notify_resource_updated("graph://identities").await;
        context.notify_resource_updated(
            &format!("graph://identity/{}", result.id.0)
        ).await;

        // ...
    }

    pub async fn graph_connect_with_context(
        &self,
        request: GraphConnectRequest,
        context: &ToolContext,
    ) -> Result<CallToolResult, ErrorData> {
        let result = graph_connect(&self.audio_graph_db, /* ... */)?;

        context.notify_resource_updated("graph://connections").await;

        // ...
    }
}
```

### Step 7: Enable Subscription Capability

**File**: `crates/hootenanny/src/api/handler.rs`

Update the capabilities to advertise subscription support:

```rust
fn capabilities(&self) -> ServerCapabilities {
    ServerCapabilities {
        resources: Some(ResourcesCapability {
            list_changed: Some(true),
            subscribe: Some(true),  // Enable subscriptions
        }),
        tools: Some(ToolsCapability::default()),
        prompts: Some(PromptsCapability::default()),
        completions: Some(CompletionsCapability::default()),
        logging: Some(LoggingCapability::default()),
        ..Default::default()
    }
}
```

### Step 8: Add Session Store Methods

**File**: `crates/baton/src/session/store.rs`

```rust
pub trait SessionStore: Send + Sync {
    // ... existing methods ...

    /// Get all session IDs
    fn all_session_ids(&self) -> Vec<String>;

    /// Iterate over all sessions
    fn all_sessions(&self) -> Vec<SessionRef<'_>>;
}

impl SessionStore for InMemorySessionStore {
    fn all_session_ids(&self) -> Vec<String> {
        self.sessions.iter().map(|e| e.key().clone()).collect()
    }

    fn all_sessions(&self) -> Vec<SessionRef<'_>> {
        self.sessions.iter().collect()
    }
}
```

### Step 9: Unit Tests

**File**: `crates/baton/src/types/subscription_tests.rs`

```rust
#[test]
fn test_session_subscriptions() {
    let mut session = Session::new("test".to_string());

    session.subscribe("artifacts://recent");
    assert!(session.is_subscribed("artifacts://recent"));
    assert!(!session.is_subscribed("artifacts://summary"));

    session.unsubscribe("artifacts://recent");
    assert!(!session.is_subscribed("artifacts://recent"));
}

#[test]
fn test_uri_matches_template() {
    assert!(uri_matches_template("cas://abc123", "cas://{hash}"));
    assert!(uri_matches_template("graph://identity/synth1", "graph://identity/{id}"));
    assert!(!uri_matches_template("cas://abc123", "artifacts://{id}"));
}

#[test]
fn test_uri_matches_pattern() {
    assert!(uri_matches_pattern("artifacts://recent", "artifacts://*"));
    assert!(uri_matches_pattern("artifacts://by-tag/type:midi", "artifacts://*"));
    assert!(!uri_matches_pattern("graph://identities", "artifacts://*"));
}
```

**File**: `crates/hootenanny/src/api/subscription_tests.rs`

```rust
#[tokio::test]
async fn test_artifact_creation_notifies_subscribers() {
    // Create mock session with subscription
    // Create artifact
    // Verify notification was sent
}
```

### Step 10: Live Testing

1. Rebuild and reconnect MCP
2. Subscribe to `artifacts://recent`
3. Generate a new artifact with `orpheus_generate`
4. Verify `notifications/resources/updated` received
5. Call `resources/read` to get updated content
6. Unsubscribe and verify no more notifications

## Files Changed

| File | Change |
|------|--------|
| `crates/baton/src/types/subscription.rs` | New - subscription types |
| `crates/baton/src/types/mod.rs` | Export subscription |
| `crates/baton/src/session/mod.rs` | Add subscriptions to Session |
| `crates/baton/src/session/store.rs` | Add all_sessions methods |
| `crates/baton/src/transport/mod.rs` | Add ResourceNotifier |
| `crates/baton/src/protocol/mod.rs` | Handle subscribe/unsubscribe |
| `crates/hootenanny/src/api/handler.rs` | Enable subscription capability |
| `crates/hootenanny/src/api/service.rs` | Emit notifications on changes |

## Verification Checklist

- [ ] Subscription types compile
- [ ] Subscribe capability advertised
- [ ] resources/subscribe adds to session
- [ ] resources/unsubscribe removes from session
- [ ] notifications/resources/updated sent to subscribers
- [ ] Only subscribers receive notifications
- [ ] Template matching works for dynamic resources
- [ ] Unit tests pass
- [ ] Live test shows real-time updates

## Implementation Notes

### Subscription Patterns

For resources like `artifacts://by-tag/{tag}`, clients can subscribe to:
- Specific: `artifacts://by-tag/type:midi` - only that tag
- General: `artifacts://recent` - all new artifacts

The notifier should handle both specific and pattern-based notifications.

### Performance Considerations

With many subscriptions, notification fan-out could be expensive. Consider:
- Batching notifications (debounce rapid changes)
- Only notifying for significant changes
- Session-specific subscription limits

For now, keep it simple - optimize if needed.

## Notes for Next Agent

After this phase:
- Clients can subscribe to resources
- Changes trigger push notifications
- Multi-agent awareness is possible
- Real-time updates work for artifacts and graph

Phase 7 (elicitation) is the final feature - server requesting user input. This is the most interactive capability.
