# 10-hootenanny-zmq: ZMQ Infrastructure for Hootenanny

**Prerequisite**: 09a-cas-crate ✅
**Status**: ✅ Complete

## Goal

Add ZMQ infrastructure to hootenanny so it can:
1. Connect to chaosgarden as a client (send playback commands)
2. Accept connections from workers (job dispatch) - future task
3. Eventually replace MCP as the internal protocol (hrmcp becomes thin proxy) - future task

## Implementation Summary

### Added Dependencies
- `zeromq = "0.4"`
- `rmp-serde = "1"`
- `chaosgarden` (path dependency for re-exporting types)

### New Module: `zmq/`

**`zmq/mod.rs`**: Re-exports from chaosgarden + GardenManager

**`zmq/manager.rs`**: `GardenManager` - wraps `GardenClient` with:
- Connection state tracking
- Convenience methods for common operations
- Event channel forwarding

### CLI Integration

New flag: `--chaosgarden=<endpoint>`
- `local` - uses IPC endpoints
- `tcp://host:port` - uses TCP endpoints

### MCP Tools (8 new tools)

| Tool | Description |
|------|-------------|
| `garden_status` | Get connection and transport state |
| `garden_play` | Start playback |
| `garden_pause` | Pause playback |
| `garden_stop` | Stop and reset |
| `garden_seek` | Seek to beat position |
| `garden_set_tempo` | Set tempo in BPM |
| `garden_query` | Execute Trustfall query on chaosgarden's graph |
| `garden_emergency_pause` | Emergency pause (priority channel) |

## Usage

```bash
# Start hootenanny without chaosgarden (tools will error)
./target/debug/hootenanny --port 8080

# Start with local IPC connection
./target/debug/hootenanny --port 8080 --chaosgarden=local

# Start with TCP connection
./target/debug/hootenanny --port 8080 --chaosgarden=tcp://192.168.1.100:5555
```

## Test Results

```
cargo test -p hootenanny --lib: 35 passed
cargo build -p hootenanny: success (warnings only)
```

## Acceptance Criteria

- [x] `GardenManager` wraps chaosgarden's `GardenClient`
- [x] CLI argument for chaosgarden endpoint
- [x] MCP tools for playback control (play, pause, stop, seek)
- [x] MCP tools for transport state and tempo
- [x] Query forwarding via `garden_query`
- [x] Emergency pause via priority channel
- [x] Graceful handling when chaosgarden not connected
- [ ] Integration test with running chaosgarden (manual - requires daemon)

## Architecture

```
┌────────────────────────────────────────────────────────────────────┐
│                         HOOTENANNY                                  │
│                                                                     │
│  ┌──────────────────┐    ┌──────────────────┐                      │
│  │ EventDualityServer│    │ GardenManager    │                      │
│  │                   │───▶│                  │                      │
│  │ - garden_play()   │    │ - connect()      │                      │
│  │ - garden_stop()   │    │ - request()      │                      │
│  │ - garden_query()  │    │ - control()      │                      │
│  └──────────────────┘    └────────┬─────────┘                      │
│                                    │                                │
│                            ┌───────▼───────┐                       │
│                            │ GardenClient  │                       │
│                            │ (chaosgarden) │                       │
│                            └───────┬───────┘                       │
└────────────────────────────────────┼───────────────────────────────┘
                                     │ ZMQ
                                     ▼
                              ┌─────────────┐
                              │ CHAOSGARDEN │
                              │ (RT Audio)  │
                              └─────────────┘
```

## Future Work

- **11-hootenanny-workers**: Add ROUTER/PUSH sockets for worker registration
- **IOPub event streaming**: Forward events to MCP clients as notifications
- **Auto-reconnect**: Reconnect on disconnect with exponential backoff

## Signoff

- 🤖 Claude: 2025-12-11
