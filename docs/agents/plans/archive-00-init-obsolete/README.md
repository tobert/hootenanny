# Plan 00: Initial Project Setup

This plan covers the initial setup of the halfremembered-mcp project.

## Files

- `plan.md` - Complete implementation plan with 6 prompts
- ✅ Status: Ready to execute

## Scope

### Phase 1: Core MCP Server (6 Prompts)
1. Initialize project structure (single `hrmcp` binary)
2. Build Ollama client (async with timeout)
3. Implement DeepSeek code review tool
4. Build MCP server with `serve` subcommand + graceful shutdown
5. Create test documentation
6. Add usage examples

**Note**: Single binary `hrmcp` with subcommands:
- `hrmcp serve` - Run MCP server (WebSocket)
- `hrmcp list-tools` - Client mode (Plan 02)
- `hrmcp call <tool>` - Client mode (Plan 02)

## Prerequisites

- Rust toolchain installed
- Ollama installed and running
- DeepSeek Coder 33B: `ollama pull deepseek-coder:33b`
- jj installed for version control
- gh CLI for GitHub repo creation

## Before Starting

📝 **Read [test-driven-approach.md](../test-driven-approach.md)** for guidance on writing tests first. Start with failing tests that express intent, then implement.

## Execution

Start with Prompt 1 in `plan.md` and work through sequentially.

## Success Criteria

- [ ] Project builds: `cargo build --release`
- [ ] `hrmcp serve` starts MCP server on WebSocket :8080
- [ ] Graceful shutdown on SIGTERM (finishes in-flight requests)
- [ ] MCP inspector connects successfully
- [ ] DeepSeek tools work in Claude Code
- [ ] Server restart → clients auto-reconnect cleanly
- [ ] All documentation complete
- [ ] Code committed to GitHub with jj
