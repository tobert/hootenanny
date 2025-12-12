# Holler + Baton Integration Plans

This directory tracks the migration of MCP handling to baton and the decoupling of hootenanny from MCP types.

## Documents

| Document | Status | Description |
|----------|--------|-------------|
| [README.md](./README.md) | - | This file |
| [hootenanny-mcp-removal.md](./hootenanny-mcp-removal.md) | Complete | Remove MCP server from hootenanny, route through holler |
| [baton-decoupling.md](./baton-decoupling.md) | Ready | Remove baton types from hootenanny tool methods |

## Architecture Goal

```
+------------------+     +------------------+     +------------------+
|  MCP Clients     |     |     holler       |     |   hootenanny     |
|  (Claude, etc)   | --> |  baton MCP       | --> |   ZMQ backend    |
|                  |     |  hooteproto ZMQ  |     |   hooteproto     |
+------------------+     +------------------+     +------------------+
                                 |
                         +-------+-------+
                         |               |
                    +---------+     +-----------+
                    | luanette|     |chaosgarden|
                    |   ZMQ   |     |    ZMQ    |
                    +---------+     +-----------+
```

**Key principles:**
- **baton**: Pure MCP library, no Hootenanny knowledge
- **hooteproto**: Shared types for ZMQ messaging between Hootenanny services
- **holler**: The single MCP entry point, bridges to ZMQ backends
- **hootenanny**: Pure ZMQ backend, no MCP dependency

## Progress

### Phase 1: Holler -> Baton (Complete)
Holler now uses baton for MCP protocol handling instead of custom implementation.

### Phase 2: Hootenanny MCP Removal (Complete)
Hootenanny's MCP server removed. All tools route through ZMQ via holler.
Remaining issue: tool methods still return `baton::CallToolResult`.

### Phase 3: Baton Decoupling (Next)
Replace baton types in hootenanny with hooteproto types:
- Add `ToolOutput`, `ToolResult`, `ToolError` to hooteproto
- Add `Broadcast::Progress` for job progress
- Add schemars helper to baton for schema generation
- Update all hootenanny tool methods
- Add integration tests to holler

See [baton-decoupling.md](./baton-decoupling.md) for detailed plan.
