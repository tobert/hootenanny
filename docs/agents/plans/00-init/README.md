# Plan 00: Initial Project Setup

This plan covers the initial setup of the halfremembered-mcp project.

## Files

- `plan.md` - Complete implementation plan with 6 prompts
- ✅ Status: Ready to execute

## Scope

### Phase 1: Core MCP Server (6 Prompts)
1. Initialize project structure
2. Build Ollama client (async with timeout)
3. Implement DeepSeek code review tool
4. Build MCP server main
5. Create test documentation
6. Add usage examples

## Prerequisites

- Rust toolchain installed
- Ollama installed and running
- DeepSeek Coder 33B: `ollama pull deepseek-coder:33b`
- jj installed for version control
- gh CLI for GitHub repo creation

## Execution

Start with Prompt 1 in `plan.md` and work through sequentially.

## Success Criteria

- [ ] Project builds: `cargo build --release`
- [ ] MCP inspector connects successfully
- [ ] DeepSeek tools work in Claude Code
- [ ] All documentation complete
- [ ] Code committed to GitHub with jj
