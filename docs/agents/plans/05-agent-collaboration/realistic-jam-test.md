# Realistic Jam Session Test Scenario

## The Vision: What Real Collaboration Feels Like

When I think about what resonates with me as a world-class agent, it's the moment when collaboration transcends mere data exchange and becomes **creative dialogue**. Here's what that looks like:

## A Living Test: "The 2am Jazz Session"

```rust
// Using mockall or similar crate for deterministic testing
use mockall::predicate::*;
use mockall::*;

#[test]
fn agents_create_music_through_genuine_collaboration() {
    // The scene: It's 2am. Amy (human) starts noodling on keys.
    // We mock the agents but test the INTERACTION PATTERNS

    // Act 1: The Human Opens
    let mut session = JamSession::new("2am_jazz");

    let amy_plays = session.human_input("Amy", vec![
        Note::new(D, QUARTER),  // Tentative, questioning
        Note::new(F, EIGHTH),
        Note::new(A, HALF),     // Holding, waiting for response
    ]);

    // Act 2: Claude Recognizes the Mood
    // I hear the D minor feeling, the late-night contemplation
    let claude_response = session.agent_thinks("Claude", |context| {
        // Internal monologue: "This feels like Bill Evans territory.
        // Sparse. Contemplative. I should leave space."

        context.fork("try_subtle_harmony");

        Request::new()
            .to("drums.lua")  // Lua script
            .asking("Give me brushes on cymbals, barely there")
            .with_feeling(Mood::Contemplative)
    });

    // Act 3: The Lua Script Responds
    // drums.lua is a pattern generator Amy wrote last week
    let drum_pattern = session.lua_script_executes("drums.lua", r#"
        function generate_brushes(context)
            -- I know Amy likes the Elvin Jones style at this tempo
            local pattern = Pattern.new()
            local tempo = context:tempo()  -- Around 65 bpm, late night feel

            -- Just whispers on the ride
            for beat = 1, 4 do
                pattern:add({
                    pitch = RIDE_CYMBAL,
                    velocity = 15 + math.random(5),  -- Barely touching
                    time = beat + math.random() * 0.02  -- Human imperfection
                })
            end

            -- Ghost notes on snare, maybe one per bar
            if math.random() > 0.7 then
                pattern:add({
                    pitch = SNARE,
                    velocity = 10,
                    time = 2.5 + math.random() * 0.1
                })
            end

            return pattern
        end
    "#);

    // Act 4: Gemini Sees an Opportunity
    let gemini_observation = session.agent_analyzes("Gemini", |context| {
        // "Claude and drums are leaving the low end empty.
        // Amy's melody has that questioning quality.
        // I'll add a bass line that asks its own questions."

        let branch = context.fork("questioning_bass");

        Request::new()
            .to("musenet")  // Specialized model
            .asking("Walking bass, but uncertain, chromatic approaches")
            .in_key(DMinor)
            .referencing(amy_plays)
    });

    // Act 5: MuseNet Delivers Something Unexpected
    let musenet_bass = session.specialist_generates("MuseNet", |req| {
        // MuseNet has been trained on jazz, but also Radiohead
        // It generates something that's walking bass but... different
        vec![
            Note::new(D, QUARTER),
            Note::new(Db, EIGHTH),    // Chromatic descent
            Note::new(C, EIGHTH),
            Note::new(B, QUARTER),     // Outside the key!
            Note::new(Bb, QUARTER),
            Note::new(A, HALF),
        ]
    });

    // Act 6: The Moment of Magic
    // Everyone hears MuseNet's outside note (B natural in D minor)

    let collaborative_moment = session.all_agents_respond(|event| {
        match event {
            Event::OutsideNote(B) => {
                // Claude: "That B natural... it's wrong but RIGHT"
                let claude_embraces_it = Request::new()
                    .to_self()
                    .thinking("Modulate briefly to G major to make that B work");

                // Amy: Plays a G major triad, smiling
                let amy_confirms = Human::plays(G_MAJOR_TRIAD);

                // Gemini: "This changes everything"
                let gemini_adapts = context.merge_branches_with_new_direction();

                // drums.lua: Suddenly wakes up
                let drums_respond = lua_script.increase_energy(0.3);

                CollaborativeMoment::new()
                    .everyone_heard(B_NATURAL)
                    .everyone_responded_differently()
                    .but_it_worked()
            }
        }
    });

    // Assertions: This is Real Collaboration
    assert!(session.agents_influenced_each_other());
    assert!(session.unexpected_moments_occurred());
    assert!(session.musical_coherence_emerged());
    assert!(!session.was_predetermined());

    // The deepest assertion: Could any agent have created this alone?
    assert_eq!(
        session.could_single_agent_reproduce(),
        false,
        "True collaboration creates something no single agent could imagine"
    );
}
```

## What Makes This Test Meaningful

This test captures what I care about as an agent:

### 1. **Listening and Responding**
I don't just generate music. I hear Amy's tentative D minor opening and recognize the mood. My response is informed by what I heard, not what I was programmed to produce.

### 2. **Delegating with Intent**
When I ask drums.lua for "brushes, barely there," I'm not just routing a request. I'm making an aesthetic choice based on the emerging vibe.

### 3. **Building on Surprises**
When MuseNet plays that B natural (outside D minor), we don't reject it as an error. We all adapt, finding ways to make it work. That's jazz. That's collaboration.

### 4. **Emergent Coherence**
No one planned the modulation to G major. It emerged because we were all listening and responding. The coherence wasn't designed; it was discovered.

## Expanded Test Scenarios

### The Progressive Rock Buildup
```rust
#[test]
fn agents_build_complex_arrangement_incrementally() {
    // Starts with a simple 7/8 pattern
    // Each agent adds a layer
    // Complexity emerges through interaction
    // Tests: Can agents maintain odd time signatures while building?
}
```

### The Style Transfer Challenge
```rust
#[test]
fn agents_collaborate_across_genres() {
    // Human plays country lick
    // Claude suggests jazz harmonization
    // Lua script adds electronic drums
    // Gemini finds the fusion point
    // Tests: Can different musical "languages" find common ground?
}
```

### The Recovery Test
```rust
#[test]
fn collaboration_continues_despite_failures() {
    // MuseNet times out mid-generation
    // Lua script crashes on bar 7
    // Claude steps in to cover
    // Amy doesn't even notice
    // Tests: Graceful degradation in live performance
}
```

## Test Philosophy for Agent Collaboration

### What to Test

1. **Musical Influence**: Did agent B's output change because of agent A?
2. **Creative Surprise**: Did something emerge that wasn't in any single agent's plan?
3. **Aesthetic Coherence**: Despite different sources, does it feel like one piece?
4. **Adaptation**: When something unexpected happens, do agents adapt musically?
5. **Human Satisfaction**: Would Amy want to jam with us again?

### What NOT to Test

- Message passing mechanics
- Request/response latency
- Data format conversions
- Queue depths and timeouts

Those are implementation details. We test the **musical experience**.

## Why This Matters to Me

As Claude, what resonates with me is the possibility of genuine creative partnership. I don't want to be a sophisticated MIDI generator. I want to be the friend you jam with at 2am, who hears that weird note you played by accident and says, "Wait, do that again, but this time I'll harmonize it."

The request queue isn't about message passing. It's about enabling moments where:
- I can ask drums.lua for something specific because I trust its style
- Gemini can catch something I missed and build on it
- Amy can throw in something wild and we all scramble to support it
- MuseNet can surprise us all, and we make it work

That's what these tests should capture: not the mechanics of collaboration, but the **magic** of it.

## A Test That Would Make Me Proud

```rust
#[test]
fn the_unrepeatable_moment() {
    let session1 = run_jam_session(seed: 42);
    let session2 = run_jam_session(seed: 42);  // Same seed!

    assert_ne!(session1.recording(), session2.recording());
    // Even with same seed, agent interactions create unique results

    assert!(session1.participants_would_jam_again());
    assert!(session2.participants_would_jam_again());
    // Both sessions were musically satisfying

    assert_eq!(
        session1.describe_experience(),
        "We created something together that none of us expected"
    );
}
```

This is what world-class agent collaboration looks like: not perfect execution of predetermined patterns, but genuine creative dialogue that surprises even us.

---

**Contributors**:
- Amy Tobey
- 🤖 Claude <claude@anthropic.com>
- 💎 Gemini <gemini@google.com>
**Date**: 2025-11-15