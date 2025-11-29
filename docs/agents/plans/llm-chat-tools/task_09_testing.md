# Task 09: End-to-end testing and documentation

## Goal

Verify the full flow works and document usage.

## Test Scenarios

### Scenario 1: Basic conversation without tools

```rust
#[tokio::test]
async fn test_basic_conversation() {
    // Setup
    let db = ConversationDb::in_memory().unwrap();
    let config = BridgeConfig {
        mcp_url: "http://localhost:8080".to_string(),
        backends: vec![BackendConfig {
            id: "test".to_string(),
            display_name: "Test".to_string(),
            base_url: "http://localhost:2020/v1".to_string(),
            api_key: None,
            default_model: "test-model".to_string(),
            summary_model: None,
            supports_tools: false,
            max_tokens: Some(100),
            default_temperature: None,
        }],
    };
    let manager = Arc::new(AgentManager::new(config, db).unwrap());

    // Create session
    let create_resp = manager.create_session(AgentChatNewRequest {
        backend: "test".to_string(),
        system_prompt: Some("You are helpful.".to_string()),
        enable_tools: false,
        max_tool_iterations: None,
    }).await.unwrap();

    assert!(!create_resp.session_id.is_empty());

    // Send message (would need mock provider)
    // Poll for response
    // Verify response received
}
```

### Scenario 2: Conversation with tool calls

```rust
#[tokio::test]
async fn test_tool_calling() {
    // This test requires:
    // 1. Running hootenanny server
    // 2. Running LLM backend
    // 3. Full integration

    // Create session with tools enabled
    // Send message that triggers tool use
    // Poll until finished
    // Verify tool was called
    // Verify response incorporates tool result
}
```

### Scenario 3: DeepSeek generates music

```rust
#[tokio::test]
#[ignore] // Requires running services
async fn test_deepseek_music_generation() {
    // 1. Create session with deepseek backend
    // 2. Send: "Generate a short melody using orpheus_generate"
    // 3. Poll until finished
    // 4. Verify orpheus_generate was called
    // 5. Verify MIDI artifact created in CAS
}
```

### Scenario 4: Trace propagation

```rust
#[tokio::test]
async fn test_trace_propagation() {
    // Use otlp-mcp to capture traces
    // 1. Create snapshot "before"
    // 2. Run agent chat with tool call
    // 3. Create snapshot "after"
    // 4. Query for traces between snapshots
    // 5. Verify trace_id is consistent across:
    //    - agent_chat_send span
    //    - LLM call span
    //    - MCP tool call span
    //    - hootenanny tool execution span
}
```

## Integration Test Setup

Create `crates/llm-mcp-bridge/tests/integration.rs`:

```rust
use llm_mcp_bridge::*;
use std::sync::Arc;
use tokio::time::{timeout, Duration};

mod common;

/// Requires running hootenanny on port 8080
/// Requires running DeepSeek on port 2020
#[tokio::test]
#[ignore]
async fn test_full_agent_flow() {
    // Load config
    let config: BridgeConfig = toml::from_str(include_str!("fixtures/test_config.toml"))
        .expect("Failed to load test config");

    // Create database
    let db = ConversationDb::in_memory().unwrap();

    // Create manager
    let manager = Arc::new(AgentManager::new(config, db).unwrap());

    // Create session
    let session = manager.create_session(AgentChatNewRequest {
        backend: "deepseek".to_string(),
        system_prompt: Some("You are a helpful assistant.".to_string()),
        enable_tools: true,
        max_tool_iterations: Some(3),
    }).await.unwrap();

    // Send message
    manager.send_message(AgentChatSendRequest {
        session_id: session.session_id.clone(),
        message: "Hello, how are you?".to_string(),
        temperature: None,
    }).await.unwrap();

    // Poll until finished
    let result = timeout(Duration::from_secs(30), async {
        loop {
            let poll = manager.poll(AgentChatPollRequest {
                session_id: session.session_id.clone(),
                since_index: 0,
                timeout_ms: 1000,
            }).await.unwrap();

            if poll.status == SessionStatus::Finished {
                return poll;
            }

            if poll.status == SessionStatus::Failed {
                panic!("Session failed");
            }

            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }).await.expect("Timeout waiting for response");

    // Verify we got a response
    assert!(!result.chunks.is_empty());
}
```

## Test Fixtures

Create `crates/llm-mcp-bridge/tests/fixtures/test_config.toml`:

```toml
mcp_url = "http://127.0.0.1:8080"

[[backends]]
id = "deepseek"
display_name = "DeepSeek Coder"
base_url = "http://127.0.0.1:2020/v1"
default_model = "deepseek-coder-v2-lite"
supports_tools = true
```

## Documentation

### README.md for llm-mcp-bridge

```markdown
# llm-mcp-bridge

Bridge between LLM providers and MCP tools. Enables LLM agents to use MCP tools
with full conversation persistence and distributed tracing.

## Features

- **Multiple LLM backends**: Configure DeepSeek, Ollama, or any OpenAI-compatible API
- **Tool calling**: LLMs can call MCP tools (orpheus_generate, cas_store, etc.)
- **Conversation persistence**: SQLite-backed conversation history
- **Distributed tracing**: Full trace context propagation via traceparent headers
- **Async sessions**: Non-blocking agent loops with polling

## MCP Tools

| Tool | Description |
|------|-------------|
| `agent_chat_new` | Create a new chat session |
| `agent_chat_send` | Send a message (async) |
| `agent_chat_poll` | Poll for output chunks |
| `agent_chat_cancel` | Cancel a session |
| `agent_chat_status` | Get session status |
| `agent_chat_history` | Get message history |
| `agent_chat_summary` | AI-generated summary |
| `agent_chat_list` | List sessions |
| `agent_chat_backends` | List configured backends |

## Configuration

```toml
mcp_url = "http://127.0.0.1:8080"

[[backends]]
id = "deepseek"
display_name = "DeepSeek Coder"
base_url = "http://127.0.0.1:2020/v1"
default_model = "deepseek-coder-v2-lite"
summary_model = "deepseek-coder-v2-lite"
supports_tools = true

[[backends]]
id = "ollama"
display_name = "Ollama Local"
base_url = "http://127.0.0.1:11434/v1"
default_model = "llama3.2"
supports_tools = true
```

## Example Usage

```json
// Create session
{"tool": "agent_chat_new", "arguments": {
  "backend": "deepseek",
  "system_prompt": "You are a music composition assistant.",
  "enable_tools": true
}}

// Send message
{"tool": "agent_chat_send", "arguments": {
  "session_id": "...",
  "message": "Generate a melancholic piano melody"
}}

// Poll for response
{"tool": "agent_chat_poll", "arguments": {
  "session_id": "...",
  "since_index": 0,
  "timeout_ms": 5000
}}
```
```

## Acceptance Criteria

- [ ] Basic conversation test passes with mock
- [ ] Integration test works with real services
- [ ] Trace propagation verified end-to-end
- [ ] README documents all tools
- [ ] Configuration example provided
- [ ] Test fixtures created
