# Plan 02: CLI Client `hrmcp` (Phase 1.5)

**Status**: Planned - Execute immediately after Plan 00
**Dependencies**: Requires Plan 00 (MVP) functional - server running and responding
**Timeline**: 2-3 hours after Plan 00 completes
**Priority**: High - Build before Plan 01 (Lua tools) for testing
**Binary Name**: `hrmcp`

## Overview

A beautiful, emoji-rich **stateless** command-line interface for humans to interact directly with the halfremembered-mcp server. Each invocation connects, calls a tool, outputs the result, and exits.

## Before Starting

📝 **Read [test-driven-approach.md](../test-driven-approach.md)** first. Write tests for CLI commands before implementing them.

This enables:
- Quick testing of MCP tools without Claude Code
- Direct human access to DeepSeek code review
- Shell scripting and automation (`cat file.rs | hrmcp call review_code -`)
- CI/CD integration
- Composable with Unix tools
- Beautiful, fluent CLI experience with colors and emoji from day one

**Philosophy**: Stateless and simple. If state transfer is needed, build MCP tools for it.

## Goals

Build a simple, elegant, **stateless** CLI that:
1. Connects to WebSocket MCP server at `ws://localhost:8080`
2. Discovers available tools (`hrmcp list-tools`)
3. Invokes tools with arguments (`hrmcp call <tool> <args>`)
4. Displays results in beautiful, human-friendly format
5. Supports stdin for piping (`cat file.rs | hrmcp call review_code -`)
6. Exits after each command (stateless)

## Scope

### In Scope (MVP)
- **Stateless command-line interface**:
  - Each invocation: connect → execute → output → exit
  - No session state, no history
- WebSocket client connection to MCP server
- Tool discovery (`hrmcp list-tools`)
- Tool invocation with arguments (`hrmcp call <tool> <args>`)
- **Stdin support** for piping (`cat file.rs | hrmcp call review_code -`)
- **Beautiful output from day one**:
  - Colors with `owo-colors` or `colored`
  - Emoji (🎨 ✅ ❌ 🤖 💎 etc.)
  - Fluent CLI experience with `clap` v4
  - Box-drawing characters for separators

### Out of Scope
- REPL/interactive mode (stateless is simpler and more scriptable)
- Command history
- Tab completion
- Configuration file (server URL is hardcoded for now)
- Multiple server connections
- State transfer (build MCP tools for that instead)

## Architecture

```
┌──────────────────────────────────────────┐
│  hrmcp (stateless binary)                │
│                                           │
│  ┌─────────────────────────────────────┐ │
│  │  CLI Parser (clap v4)               │ │
│  │  - list-tools                       │ │
│  │  - call <tool> <args>               │ │
│  │  - stdin support (-)                │ │
│  └────────────┬────────────────────────┘ │
│               │                           │
│  ┌────────────▼────────────────────────┐ │
│  │  MCP Client                         │ │
│  │  - Connect to WebSocket             │ │
│  │  - Invoke tool                      │ │
│  │  - Format output                    │ │
│  │  - Exit                             │ │
│  └────────────┬────────────────────────┘ │
└───────────────┼───────────────────────────┘
                │ WebSocket (ephemeral)
                │ ws://localhost:8080
┌───────────────▼───────────────────────────┐
│  halfremembered_mcp MCP Server            │
│  (from Plan 00)                           │
└───────────────────────────────────────────┘
```

## Usage Examples

### Basic Commands
```bash
$ hrmcp call review_code "$(cat src/main.rs)"
🤖 Review Result
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
[Review output...]
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

$ hrmcp list-tools
📋 Available MCP Tools
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
  🤖 review_code - Review code for bugs and improvements
  📖 explain_code - Get detailed explanation of code
  ✨ suggest_improvements - Context-aware improvement suggestions
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

$ hrmcp call explain_code "match x { Some(v) => v, None => 0 }"
📖 Explanation
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
This is a pattern match that extracts a value from an Option.
If x is Some(v), it returns the inner value v.
If x is None, it returns 0 as a default.
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
```

### Piping from stdin (Unix-friendly)
```bash
# Review a file
$ cat src/main.rs | hrmcp call review_code -

# Explain code from clipboard
$ pbpaste | hrmcp call explain_code -

# Chain with other tools
$ git diff HEAD~1 | hrmcp call review_code - | tee review.txt
```

### Error handling examples
```bash
$ hrmcp call review_code "some code"
❌ Error: Server not running
💡 Start server with: halfremembered_mcp

$ hrmcp call nonexistent "arg"
❌ Error: Tool 'nonexistent' not found
📋 Available tools: hrmcp list-tools
```

## Success Criteria

- [ ] CLI connects to MCP server WebSocket (ephemeral connection)
- [ ] `hrmcp list-tools` shows available tools with emoji
- [ ] `hrmcp call review_code <code>` returns review from DeepSeek
- [ ] `hrmcp call <tool> -` reads from stdin for piping
- [ ] Beautiful output (colors, emoji, box-drawing)
- [ ] Graceful error handling with actionable messages
- [ ] `hrmcp --help` explains usage
- [ ] Each invocation is stateless (connect → execute → exit)

## Implementation Approach

**Language**: Rust (same as server)
**Dependencies**:
- `clap` v4 - Fluent CLI argument parsing with derive macros
- `tokio-tungstenite` - Async WebSocket client
- `serde_json` - JSON serialization
- `anyhow` - Error handling
- `owo-colors` or `colored` - Beautiful terminal colors
- `unicode-width` - Proper emoji/unicode handling

**File Structure**:
```
src/
├── main.rs           # Single binary entry point with clap subcommands
├── commands/
│   ├── serve.rs      # Server mode (from Plan 00)
│   ├── list_tools.rs # Client: list tools
│   └── call.rs       # Client: call tool
├── mcp_client.rs     # MCP WebSocket client (for client commands)
├── mcp_server.rs     # MCP server logic (from Plan 00)
└── ...existing files
```

**Binary**: Single `hrmcp` binary with subcommands
- `hrmcp serve [--listen <addr>]` - Server mode (default: 127.0.0.1:8080)
- `hrmcp list-tools` - Client mode
- `hrmcp call <tool> <args>` - Client mode

## Why Phase 1.5?

This adds client commands to the same `hrmcp` binary from Plan 00:
- **After Plan 00**: Server mode (`hrmcp serve`) is built
- **Before Plan 01**: Client mode useful for testing Lua tools
- **Same binary**: Simpler deployment, single tool to install

Human-friendly CLI makes the system immediately useful beyond Claude Code integration.

**Development iteration**:
```bash
# Restart server on code changes (manual or with cargo-watch)
$ cargo build && pkill hrmcp && hrmcp serve &

# Clients auto-reconnect on server restart
$ hrmcp call review_code "fn main() {}"
```

## Design Decisions

### WebSocket Client Library
**Decision**: Use `tokio-tungstenite`
**Why**: Async WebSocket client, works with Tokio runtime
**Alternative**: `tungstenite` (blocking) - simpler but blocks

### Output Format
**Decision**: Beautiful, emoji-rich, colorful from day one
**Why**: CLI is for humans - make it delightful to use
**Examples**:
- Success: ✅ (green)
- Error: ❌ (red)
- Tool icons: 🤖 (DeepSeek), 📖 (explain), ✨ (improve)
- Separators: Unicode box-drawing characters (━━━)
- Code blocks: Syntax highlighting if possible

### Stateless-Only Design
**Decision**: Pure stateless command-line tool (no REPL)
**Why**:
- **Simpler**: No state management, session handling, or REPL loop
- **More scriptable**: Easy to use in shell scripts, CI/CD
- **Unix philosophy**: One command, one job, composable
- **Server holds state**: MCP server is persistent, CLI doesn't need to be
**Benefits**:
- Pipe-friendly: `cat file.rs | hrmcp call review_code -`
- Composable: `hrmcp call review_code "code" | tee output.txt`
- State transfer: Build MCP tools for that if needed later

### Error Handling
**Decision**: Fail loud with actionable errors
**Why**: Consistent with project philosophy
**Example**: "Server not running. Start with: halfremembered_mcp"

## Future Enhancements (Post-MVP)

- **Phase 2**: Config file for server URL (`~/.config/halfremembered-mcp/cli.toml`)
- **Phase 2**: JSON output mode for scripting (`--json` flag)
- **Phase 3**: TUI mode with `ratatui` (if interactive exploration is needed)
- **Phase 3**: Multi-server support (`--server ws://...`)
- **Phase 3**: Shell completion generation (`hrmcp completions bash`)

## Amy's Decisions ✅

1. **Priority**: ✅ Build immediately after Plan 00, before Lua tools
2. **Output format**: ✅ Colors and emoji from the start - beautiful CLI
3. **Binary name**: ✅ `hrmcp` (short and memorable)
4. **Architecture**: ✅ Stateless-only (no REPL, simpler and more scriptable)
5. **State transfer**: ✅ Build MCP tools for that if needed later

---

**Contributors**:
- Amy Tobey
- 🤖 Claude <claude@anthropic.com>
- 💎 Gemini <gemini@google.com>
**Date**: 2025-11-15
**Status**: Ready to implement after Plan 00
