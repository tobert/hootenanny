# Plan 01: Dynamic Lua Tools & Persistence

This plan details the implementation of a dynamic, file-based Lua tool system with hot-reloading, per-tool sandboxing, and persistent state. This design allows agents to create and iterate on new MCP tools live by modifying files.

## Files

- `plan.md` - Complete implementation plan with 7 prompts.
- ✅ Status: Ready to execute after `00-init` is complete.

## Scope

1.  **Project Structure:** Define a `mcp_lua/tools/` directory where each subdirectory is a dynamically loaded MCP tool.
2.  **Dependencies:** Add `mlua`, `sled`, `notify`, `serde`, and `walkdir`.
3.  **State Management:** Implement a `sled`-backed `StateManager` for persistent data, accessible from Lua.
4.  **Dynamic Tool Loading:** Create a `ToolLoader` that scans the filesystem, parses tool manifests (`mcp_tool.json`), and generates MCP tool definitions.
5.  **Per-Tool Sandboxing:** Implement a `Sandbox` service that creates a Lua environment tailored to the permissions declared in the tool's manifest.
6.  **Hot-Reloading:** Use a file watcher to automatically reload tools when their files change, enabling zero-downtime iteration.
7.  **Documentation:** Provide clear instructions on how to create new Lua-based tools.

## Success Criteria

- [ ] The server discovers and registers MCP tools from the `mcp_lua/tools/` directory on startup.
- [ ] Adding a new tool directory automatically registers a new MCP tool.
- [ ] Modifying a tool's `main.lua` or `mcp_tool.json` updates the tool's behavior/definition without a server restart.
- [ ] Lua tools can read and write persistent data via a `state` global.
- [ ] A Lua tool without declared permissions cannot access the network or filesystem.
- [ ] A Lua tool with specific `network` permissions in its manifest can make HTTP requests to allowed hosts.
