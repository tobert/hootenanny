# HalfRemembered MCP 🎵

**Event Duality: Where intentions become sounds.**

An MCP server for collaborative human-AI music ensemble. This is the Master Control Program for the HalfRemembered project - building music the way we build software.

## Quick Start

```bash
# Run the MCP server (hootenanny crate)
cargo run -p hootenanny

# In another terminal, test with curl
curl -N http://127.0.0.1:8080/sse
```

Then connect via your MCP client and try:
```json
{"what": "C", "how": "softly"}  → {"pitch": 60, "velocity": 40}
```

## Current Status: Plan 00 - Event Duality Hello World ✅

**COMPLETE!** Successfully tested end-to-end with Claude Code MCP client (2025-11-16).

We've proven the core concept: **Intentions become sounds through type-rich transformations.**

```rust
enum Event {
    Abstract(Intention),  // What you want: "play C softly"
    Concrete(Sound),      // What you get: pitch:60, velocity:40
}
```

**Test Results:**
- ✅ C, softly → pitch: 60, velocity: 40 (quiet)
- ✅ E, boldly → pitch: 64, velocity: 90 (strong)
- ✅ G, questioning → pitch: 67, velocity: 50 (tentative)
- ✅ A, normally → pitch: 69, velocity: 64 (moderate)

## Running the Server

### Basic Run

```bash
cargo run -p hootenanny
```

The server starts on **http://127.0.0.1:8080** with SSE transport (multi-client ready).

### Auto-Restart on Code Changes

For development, use `cargo-watch` to automatically rebuild and restart when you modify code:

```bash
# Install cargo-watch (one time)
cargo install cargo-watch

# Run with auto-restart
cargo watch -x 'run -p hootenanny'
```

Now any changes to `.rs` files in either `hootenanny` or `resonode` will trigger an automatic rebuild and restart! 🔄

### Integration with MCP Clients

The server is ready to use with any MCP client. For **Claude Code**:

1. Start the server (with or without auto-restart)
2. In Claude Code, run `/mcp` to reconnect
3. The `play` tool will be available to use!

```bash
# Terminal 1: Run the server with auto-restart
cargo watch -x 'run -p hootenanny'

# Terminal 2: Use Claude Code with /mcp
```

### Connecting with a Client

You can connect to the MCP server using various clients.

#### Claude CLI

To connect to the server using the Claude CLI, run the following command:

```bash
claude mcp add --transport sse hootenanny http://127.0.0.1:8080/sse
```

This will add the `hootenanny` server to your Claude CLI configuration. You can then use the `play` tool and other tools provided by the server.

#### Gemini CLI

To connect to the server using the Gemini CLI, run the following command:

```bash
gemini mcp add hootenanny http://127.0.0.1:8080/sse
```

This will add the `hootenanny` server to your Gemini CLI configuration. You can then use the `play` tool and other tools provided by the server.

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

**Architecture Evolution: Two-Crate Design**

We're splitting the system into focused crates:

- **`resonode`** 🎵 - Musical domain (Event Duality, sounds, realization, scales, rhythms)
- **`hootenanny`** 🎭 - Conversation system (multi-agent collaboration, temporal forking, dialogue trees)

This separation keeps musical logic independent from conversation mechanics.

**Upcoming Plans:**
- **Plan 01**: Expand resonode musical domain model
- **Plan 02**: Build hootenanny conversation trees
- **Plan 03**: Multi-agent ensemble coordination (Gemini is working on this!)
- **Plan 04**: Browser UI for visual interaction

---

**Status**: ✅ Dancing
**Contributors**: Amy Tobey, Claude, Gemini
**Vibe**: 🎵 Let's jam
