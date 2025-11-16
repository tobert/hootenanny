# HalfRemembered MCP 🎵

**Event Duality: Where intentions become sounds.**

An MCP server for collaborative human-AI music ensemble. This is the Master Control Program for the HalfRemembered project - building music the way we build software.

## Current Status: Plan 00 - Event Duality Hello World ✅

We've proven the core concept: **Intentions become sounds through type-rich transformations.**

```rust
enum Event {
    Abstract(Intention),  // What you want: "play C softly"
    Concrete(Sound),      // What you get: pitch:60, velocity:40
}
```

## Running the Server

### Basic Run

```bash
cargo run
```

The server starts on **http://127.0.0.1:8080** with SSE transport (multi-client ready).

### Auto-Restart on Code Changes

For development, use `cargo-watch` to automatically rebuild and restart when you modify code:

```bash
# Install cargo-watch (one time)
cargo install cargo-watch

# Run with auto-restart
cargo watch -x run
```

Now any changes to `.rs` files will trigger an automatic rebuild and restart! 🔄

### Integration with MCP Clients

The server is ready to use with any MCP client. For **Claude Code**:

1. Start the server (with or without auto-restart)
2. In Claude Code, run `/mcp` to reconnect
3. The `play` tool will be available to use!

```bash
# Terminal 1: Run the server
cargo watch -x run

# Terminal 2: Use Claude Code with /mcp
```

### SSE Endpoints

- **GET** `http://127.0.0.1:8080/sse` - Connect and receive your session ID via event stream
- **POST** `http://127.0.0.1:8080/message?sessionId=<id>` - Send MCP messages

## Testing with MCP Inspector

### Using npx (Recommended)

```bash
npx @modelcontextprotocol/inspector
```

Then connect to: `http://127.0.0.1:8080`

### Manual HTTP Test

1. Connect to SSE stream:
```bash
curl -N http://127.0.0.1:8080/sse
```

You'll receive events including your session ID.

2. Send a tool call:
```bash
curl -X POST "http://127.0.0.1:8080/message?sessionId=YOUR_SESSION_ID" \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "id": 1,
    "method": "tools/call",
    "params": {
      "name": "play",
      "arguments": {
        "what": "C",
        "how": "softly"
      }
    }
  }'
```

Expected response:
```json
{
  "pitch": 60,
  "velocity": 40
}
```

## Available Tools

### `play`

Transform an intention into sound.

**Parameters:**
- `what` (string): The note to play (C, D, E, F, G, A, B)
- `how` (string): How to play it (softly, normally, boldly, questioning)

**Returns:**
- `pitch` (u8): MIDI note number
- `velocity` (u8): MIDI velocity

## Examples

### Soft C Major
```json
{"what": "C", "how": "softly"}
```
→ `{"pitch": 60, "velocity": 40}`

### Bold G
```json
{"what": "G", "how": "boldly"}
```
→ `{"pitch": 67, "velocity": 90}`

### Questioning E
```json
{"what": "E", "how": "questioning"}
```
→ `{"pitch": 64, "velocity": 50}`

## Architecture Highlights

- **SSE Transport**: Multi-client HTTP transport ready for ensemble collaboration
- **Type-Rich Domain**: `Intention` and `Sound` are first-class types, not primitives
- **Realization Pattern**: Clear transformation from abstract → concrete
- **MCP Compliant**: Full JSON-RPC protocol with schema generation

## Logs

Watch the server logs to see realizations happen:
```
🎵 softly C → pitch:60, vel:40
🎵 boldly G → pitch:67, vel:90
```

## The Vision

Building a real-time music generation system that is fast, weird, and expressive, powered by a distributed ensemble of language and music models. This MCP server is the foundation for:

- Multi-agent musical dialogue
- Conversation trees for improvisation
- Ensemble coordination across models
- Real-time performance systems

## What's Next

- **Plan 01**: Expand musical domain model (scales, rhythms, dynamics)
- **Plan 02**: Add conversation trees for musical dialogue
- **Plan 03**: Multi-agent ensemble coordination
- **Plan 04**: Browser UI for visual interaction

---

**Status**: ✅ Dancing
**Contributors**: Amy Tobey, Claude, Gemini
**Vibe**: 🎵 Let's jam
