# Plan 00: Event Duality Hello World

**Status**: Ready to build
**Timeline**: 1 day
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

1. **Define the duality** (30 min)
   ```rust
   pub enum Event { Abstract(Intention), Concrete(Sound) }
   ```

2. **Create realization** (30 min)
   ```rust
   impl Intention {
       pub fn realize(&self) -> Sound { ... }
   }
   ```

3. **Wrap in MCP** (1 hour)
   ```rust
   #[mcp_tool]
   async fn play(intention: Intention) -> Sound {
       intention.realize()
   }
   ```

4. **Test with Inspector** (30 min)
   - Send: `{"intention": {"what": "C", "how": "softly"}}`
   - Receive: `{"sound": {"pitch": 60, "velocity": 40}}`

## Dependencies

```toml
[dependencies]
rmcp = { git = "...", features = ["server", "transport-ws", "macros"] }
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
anyhow = "1"
```

## The Moment

When MCP Inspector shows:

```json
Request:  {"intention": {"what": "C", "how": "softly"}}
Response: {"sound": {"pitch": 60, "velocity": 40}}
```

We know the architecture works. Everything else builds on this.

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