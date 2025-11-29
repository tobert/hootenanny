# Plan: llm-mcp-bridge

## Overview

Create two new crates that enable LLM agents to use MCP tools:

1. **`llmchat`** - SQLite-backed conversation state management
2. **`llm-mcp-bridge`** - LLM ↔ MCP tool bridge with agent loop

Exposes a suite of `agent_chat_*` tools for managing agent sessions. Sessions run async - send a message, poll for results. Agents can use other MCP tools through HTTP loopback to hootenanny, with full distributed tracing.

## Key Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Providers | OpenAI-compatible only (phase 1) | DeepSeek, Ollama, vLLM all use this. Add Claude/Gemini later. |
| Persistence | SQLite with WAL | Survives restarts, follows audio-graph-mcp pattern |
| Tool execution | HTTP loopback | Clean separation, tracing works end-to-end |
| Send behavior | Always async | Return immediately, poll for results. Matches orpheus pattern. |
| Summary model | Per-backend config | Each backend specifies its own fast model for summaries |

## MCP Tools Exposed

| Tool | Description |
|------|-------------|
| **Session Lifecycle** | |
| `agent_chat_new` | Create session (backend, system_prompt, enable_tools) → session_id |
| `agent_chat_send` | Send message → starts async agent loop |
| `agent_chat_poll` | Get new output chunks since index (streaming feel) |
| `agent_chat_cancel` | Abort a running session |
| **Query/Read** | |
| `agent_chat_status` | Session state, message count, pending tool calls |
| `agent_chat_history` | Full message history for session |
| `agent_chat_summary` | Fast model summarizes conversation (uses backend's summary_model) |
| **Discovery** | |
| `agent_chat_list` | List recent/active sessions |
| `agent_chat_backends` | List configured LLM backends |

## Backend Configuration

```toml
[[backends]]
id = "deepseek"
display_name = "DeepSeek Coder"
base_url = "http://127.0.0.1:2020/v1"
default_model = "deepseek-coder-v2-lite"
summary_model = "deepseek-coder-v2-lite"
supports_tools = true

[[backends]]
id = "deepseek-large"
display_name = "DeepSeek Large"
base_url = "http://127.0.0.1:2021/v1"
default_model = "deepseek-coder-33b"
summary_model = "deepseek-coder-v2-lite"  # Can point to smaller sibling
supports_tools = true
```

## Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│  MCP Client (Claude Code, etc.)                                     │
│    calls: agent_chat_new(backend="deepseek", enable_tools=true)     │
└────────────────────────────┬────────────────────────────────────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────────────┐
│  hootenanny (MCP Server)                                            │
│    routes to llm-mcp-bridge handler                                 │
└────────────────────────────┬────────────────────────────────────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────────────┐
│  llm-mcp-bridge                                                     │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  AgentSession (background task)                                     │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │ 1. Send messages to DeepSeek via async-openai               │   │
│  │ 2. Stream response, accumulate tool calls                   │   │
│  │ 3. Execute tools via HTTP to hootenanny                     │   │
│  │ 4. Feed results back, continue until done                   │   │
│  │ 5. Persist conversation to llmchat SQLite                   │   │
│  └─────────────────────────────────────────────────────────────┘   │
│                             │                                       │
│                             │ HTTP POST /mcp                        │
│                             ▼                                       │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │ McpToolClient (calls back to hootenanny)                    │   │
│  │   - traceparent header propagated                           │   │
│  │   - orpheus_generate, cas_store, graph_bind, etc.           │   │
│  └─────────────────────────────────────────────────────────────┘   │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────────────┐
│  llmchat (SQLite)                                                   │
│    conversations, messages, tool_calls, tool_results                │
└─────────────────────────────────────────────────────────────────────┘
```

## Crate Structure

### `crates/llmchat/`
```
src/
  lib.rs           # Public exports
  types.rs         # ConversationId, MessageId, Role, ToolCallId (rich types)
  db.rs            # SQLite database with WAL (audio-graph-mcp pattern)
  conversation.rs  # Conversation CRUD
  message.rs       # Message operations, context window helpers
  tool_call.rs     # Tool call and result tracking
```

### `crates/llm-mcp-bridge/`
```
src/
  lib.rs           # Public exports
  config.rs        # BackendConfig, BridgeConfig
  types.rs         # ChatMessage, ToolCall, request/response types
  provider.rs      # OpenAI-compatible provider via async-openai
  mcp_client.rs    # HTTP client for tool calls (with tracing)
  session.rs       # AgentSession state machine
  manager.rs       # AgentManager (like JobStore)
  handler.rs       # baton::Handler - MCP tool registration
  loop.rs          # Agent loop background task
```

## Tasks

| Task | File | Description |
|------|------|-------------|
| 01 | [task_01_llmchat_schema.md](task_01_llmchat_schema.md) | Create llmchat crate with SQLite schema |
| 02 | [task_02_llmchat_operations.md](task_02_llmchat_operations.md) | Implement conversation and message operations |
| 03 | [task_03_bridge_scaffold.md](task_03_bridge_scaffold.md) | Create llm-mcp-bridge crate scaffold |
| 04 | [task_04_mcp_client.md](task_04_mcp_client.md) | Implement MCP tool client with tracing |
| 05 | [task_05_provider.md](task_05_provider.md) | Implement OpenAI-compatible provider |
| 06 | [task_06_agent_loop.md](task_06_agent_loop.md) | Implement agent session and tool loop |
| 07 | [task_07_handler.md](task_07_handler.md) | Implement MCP handler and integrate |
| 08 | [task_08_resources.md](task_08_resources.md) | Add MCP resources for introspection |
| 09 | [task_09_testing.md](task_09_testing.md) | End-to-end testing and documentation |

## Critical Reference Files

| File | Purpose |
|------|---------|
| `crates/audio-graph-mcp/src/db.rs` | SQLite + WAL pattern for llmchat |
| `crates/baton/src/protocol/mod.rs` | Handler trait to implement |
| `crates/hootenanny/src/api/handler.rs` | Tool registration patterns |
| `crates/hootenanny/src/job_system.rs` | Manager pattern for sessions |
| `crates/hootenanny/src/mcp_tools/local_models.rs` | Traceparent injection |
| `crates/hootenanny/tests/common/mcp_client.rs` | MCP client reference |

## Implementation Notes

1. **Shared datetime parsing**: Extract `parse_datetime` helper to `llmchat/src/types.rs` for reuse across db operations and the bridge crate.

2. **Tool results query**: Add `get_tool_result(&self, tool_call_id: &str) -> Result<Option<ToolResult>>` method to `ConversationDb` in task 02 for use by task 08 resources.

## Future Work (Not in Scope)

- Streaming responses to MCP client
- Claude/Gemini providers (different API formats)
- Conversation branching
- Multi-agent collaboration
- Rate limiting and retries
