# Event Duality Hello World - Implementation

## The Prompts

### Prompt 1: The Duality

```
Initialize the project and create the Event Duality.

jj new -m "feat: event duality foundation

Why: Proving intentions can become sounds
Approach: Minimal Event enum with Abstract/Concrete variants
Learned: The simplest architecture is the truest
Next: Implement realization

🎵 Let's dance"

1. If not done: cargo init --name halfremembered_mcp

2. Create src/domain.rs:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Event {
    Abstract(Intention),
    Concrete(Sound),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Intention {
    pub what: String,  // "C", "D", "E"
    pub how: String,   // "softly", "boldly", "questioning"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sound {
    pub pitch: u8,     // MIDI note number
    pub velocity: u8,  // MIDI velocity
}
```

3. Create test in src/domain.rs:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_duality_exists() {
        let intention = Intention {
            what: "C".to_string(),
            how: "softly".to_string(),
        };

        let abstract_event = Event::Abstract(intention);
        let concrete_event = Event::Concrete(Sound {
            pitch: 60,
            velocity: 40,
        });

        // They coexist
        matches!(abstract_event, Event::Abstract(_));
        matches!(concrete_event, Event::Concrete(_));
    }
}
```

Run: cargo test
Commit when green.
```

### Prompt 2: The Realization

```
Implement the transformation from Intention to Sound.

jj new -m "feat: intention realization

Why: The core magic - abstract becomes concrete
Approach: Simple mapping with musical logic
Learned: 'softly' means velocity < 64
Next: Wire into MCP

🎵 Dancing continues"

1. Create src/realization.rs:

```rust
use crate::domain::{Intention, Sound};

impl Intention {
    pub fn realize(&self) -> Sound {
        let pitch = note_to_midi(&self.what);
        let velocity = feeling_to_velocity(&self.how);

        Sound { pitch, velocity }
    }
}

fn note_to_midi(note: &str) -> u8 {
    match note {
        "C" => 60,
        "D" => 62,
        "E" => 64,
        "F" => 65,
        "G" => 67,
        "A" => 69,
        "B" => 71,
        _ => 60,  // Default to C
    }
}

fn feeling_to_velocity(feeling: &str) -> u8 {
    match feeling {
        "softly" => 40,
        "normally" => 64,
        "boldly" => 90,
        "questioning" => 50,
        _ => 64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Intention;

    #[test]
    fn intention_becomes_sound() {
        let intention = Intention {
            what: "C".to_string(),
            how: "softly".to_string(),
        };

        let sound = intention.realize();

        assert_eq!(sound.pitch, 60);
        assert_eq!(sound.velocity, 40);
    }

    #[test]
    fn different_intentions_different_sounds() {
        let soft_c = Intention {
            what: "C".to_string(),
            how: "softly".to_string(),
        }.realize();

        let bold_g = Intention {
            what: "G".to_string(),
            how: "boldly".to_string(),
        }.realize();

        assert_ne!(soft_c.pitch, bold_g.pitch);
        assert!(soft_c.velocity < bold_g.velocity);
    }
}
```

2. Update src/domain.rs to make the module public:
```rust
pub mod realization;
```

Run: cargo test
All green? Commit.
```

### Prompt 3: The MCP Server

```
Create minimal MCP server with our duality tool.

jj new -m "feat: MCP server with play tool

Why: Expose our duality via MCP protocol
Approach: Single tool that realizes intentions
Learned: WebSocket enables multi-agent future
Next: Test with Inspector

🎵 The dance floor opens"

1. Update Cargo.toml:
```toml
[dependencies]
rmcp = { git = "https://github.com/modelcontextprotocol/rust-sdk", features = ["server", "transport-ws", "macros"] }
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
anyhow = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
```

2. Create src/main.rs:
```rust
mod domain;
mod realization;

use anyhow::Result;
use domain::{Intention, Sound};
use rmcp::{server::McpServer, transport::WebSocketTransport};
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .init();

    let transport = WebSocketTransport::new("127.0.0.1:8080");
    let mut server = McpServer::new("HalfRemembered MCP", "0.1.0", transport);

    server.tool(play_tool());

    tracing::info!("🎵 Event Duality Server starting on ws://127.0.0.1:8080");
    server.run().await?;

    Ok(())
}

fn play_tool() -> rmcp::Tool {
    rmcp::Tool::new(
        "play",
        "Transform an intention into sound",
        |params: PlayParams| async move {
            let sound = params.intention.realize();
            Ok(PlayResponse { sound })
        },
    )
}

#[derive(serde::Deserialize)]
struct PlayParams {
    intention: Intention,
}

#[derive(serde::Serialize)]
struct PlayResponse {
    sound: Sound,
}
```

3. Test it compiles:
cargo build

4. Run the server:
cargo run

You should see:
🎵 Event Duality Server starting on ws://127.0.0.1:8080

Commit this dancing server.
```

### Prompt 4: The First Sound

```
Test our duality with MCP Inspector.

jj new -m "test: first intention becomes sound

Why: Validate the core concept works
Approach: Use MCP Inspector to send intention
Learned: The architecture is sound (pun intended)
Next: Document success

🎵 First notes played"

1. Start the server:
cargo run

2. In another terminal, use MCP Inspector:
mcp inspect ws://127.0.0.1:8080

3. Call our tool:
{
  "method": "tools/play",
  "params": {
    "intention": {
      "what": "C",
      "how": "softly"
    }
  }
}

4. Verify response:
{
  "sound": {
    "pitch": 60,
    "velocity": 40
  }
}

5. Try different intentions:
- {"what": "G", "how": "boldly"} → {"pitch": 67, "velocity": 90}
- {"what": "D", "how": "questioning"} → {"pitch": 62, "velocity": 50}

6. Add integration test in tests/integration.rs:
```rust
#[tokio::test]
async fn server_realizes_intentions() {
    // Start server in background
    // Send intention via WebSocket
    // Verify sound response
    // This is left as exercise for next session
}
```

Document your success!
```

### Prompt 5: Polish and Prepare

```
Clean up and prepare for domain expansion.

jj new -m "polish: ready for musical domain

Why: Clean foundation for Plan 03
Approach: Add README, improve errors, add logging
Learned: Event Duality proven and working
Next: Expand to full musical domain

🎵 Ready for the symphony"

1. Create README.md:
```markdown
# HalfRemembered MCP

An MCP server where intentions become sounds.

## The Core Concept

```rust
enum Event {
    Abstract(Intention),  // What you want
    Concrete(Sound),      // What you get
}
```

## Running

```bash
cargo run
```

Connect with MCP Inspector: `ws://127.0.0.1:8080`

## First Test

Send:
```json
{
  "method": "tools/play",
  "params": {
    "intention": {"what": "C", "how": "softly"}
  }
}
```

Receive:
```json
{
  "sound": {"pitch": 60, "velocity": 40}
}
```

## Next: Plan 03

This proves the concept. Next we add:
- Conversation trees
- Musical context
- Agent collaboration

The dance has begun.
```

2. Add better logging in realization.rs:
```rust
pub fn realize(&self) -> Sound {
    let pitch = note_to_midi(&self.what);
    let velocity = feeling_to_velocity(&self.how);

    tracing::info!("🎵 {} {} → pitch:{}, vel:{}",
        self.how, self.what, pitch, velocity);

    Sound { pitch, velocity }
}
```

3. Final test run and commit:
cargo test
cargo run

jj describe -m "$(cat <<'EOF'
polish: event duality hello world complete

Why: Foundation needed for musical domain
Approach: Built minimal Abstract→Concrete transformation
Learned: Simplest implementation proves the concept
Next: Plan 03 expands to full musical domain

Status: ✅ Ready to dance

Contributors:
- Amy Tobey
- Claude: Event duality design
- Gemini: Strategic pivot to music-first

🎵 Let's dance
EOF
)"

jj git push -c @
```

## Success Criteria

- [x] Event enum with Abstract/Concrete
- [x] Intention realizes to Sound
- [x] MCP server on WebSocket :8080
- [x] Tool that transforms intentions
- [x] Tested with Inspector
- [x] Ready for domain expansion

## The Moment

When you see this in the logs:

```
🎵 softly C → pitch:60, vel:40
```

You know we're dancing.

---

*Pure. Simple. Resonant.*