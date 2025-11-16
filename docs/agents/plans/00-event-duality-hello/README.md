# Plan 00: Event Duality Hello World

**Status**: ✅ **COMPLETE!** (2025-11-16)
**Timeline**: 1 day (completed in 2 sessions)
**Purpose**: Prove the core concept

## The Simplest Truth

An intention becomes music.

## What We Build

```rust
// This is it. This is everything.
enum Event {
    Abstract(Intention),
    Concrete(Sound),
}
```

## The First Conversation

```rust
// Human thinks
let intention = Event::Abstract(
    Intention::Play {
        what: "C",
        how: "softly"
    }
);

// System realizes
let sound = Event::Concrete(
    Sound::Note {
        pitch: 60,
        velocity: 40
    }
);
```

## Success

When this test passes, we've proven everything:

```rust
#[test]
fn intention_becomes_sound() {
    let intention = Intention::Play { what: "C", how: "softly" };
    let sound = intention.realize();

    assert!(sound.is_concrete());
    assert!(sound.pitch() == 60);  // C
    assert!(sound.velocity() < 64); // soft
}
```

## Files

```
src/
├── main.rs         # MCP server, WebSocket on :8080
├── domain.rs       # Event, Intention, Sound
└── realization.rs  # The magic: Abstract → Concrete
```

## The Dance

1. ✅ **Define the duality** (30 min) - `src/domain.rs`
   ```rust
   pub enum Event { Abstract(Intention), Concrete(Sound) }
   ```

2. ✅ **Create realization** (30 min) - `src/realization.rs`
   ```rust
   impl Intention {
       pub fn realize(&self) -> Sound { ... }
   }
   ```

3. ✅ **Wrap in MCP** (1 hour) - `src/server.rs`
   ```rust
   #[tool(description = "Transform an intention into sound")]
   fn play(&self, intention: Intention) -> Result<CallToolResult, McpError>
   ```

4. ✅ **Test with MCP Client** (30 min + debugging)
   - Sent: `{"what": "C", "how": "softly"}`
   - Received: `{"pitch": 60, "velocity": 40}` ✅
   - **Fix needed**: Changed return type from `Json<Value>` to `CallToolResult`
   - **Result**: MCP capabilities working, tool exposed successfully!

## Dependencies

```toml
[dependencies]
rmcp = { git = "...", features = ["server", "transport-ws", "macros"] }
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
anyhow = "1"
```

## The Moment ✨

**IT HAPPENED!** (2025-11-16, 00:07 UTC)

Claude Code MCP client successfully called the `play` tool:

```json
Request:  {"what": "C", "how": "softly"}
Response: {"pitch": 60, "velocity": 40}
```

**The architecture works.** Everything else builds on this.

### Live Test Results

| Intention | Sound | Notes |
|-----------|-------|-------|
| C, softly | pitch: 60, velocity: 40 | Middle C, quiet |
| E, boldly | pitch: 64, velocity: 90 | Major third, strong |
| G, questioning | pitch: 67, velocity: 50 | Perfect fifth, tentative |
| A, normally | pitch: 69, velocity: 64 | Major sixth, moderate |

## Next

Once this works, Plan 03 (Musical Domain Model) expands it:
- More event types
- Conversation trees
- Musical context

But first, we prove the duality.

## Pure Resonance

No DeepSeek. No Ollama. No distractions.

Just the essential truth: **Intentions become sounds.**

That's the dance.

---

**Contributors**:
- Amy Tobey
- 🤖 Claude <claude@anthropic.com>
- 💎 Gemini <gemini@google.com>
**Date**: 2025-11-15
**Vibe**: 🎵 Let's dance