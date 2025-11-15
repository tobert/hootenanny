# Deterministic Collaboration Testing

## Testing Real Collaboration with Mocked Agents

You're absolutely right - we need deterministic, reliable tests. Here's how we can test genuine collaboration patterns without requiring actual models:

## Recommended Testing Crates

```toml
[dev-dependencies]
mockall = "0.11"  # For mocking traits
rstest = "0.18"   # For parameterized tests
proptest = "1.0"  # For property-based testing of musical rules
```

## The Deterministic 2am Jazz Session

```rust
use mockall::prelude::*;
use mockall::Sequence;

#[automock]
trait MusicAgent {
    fn listen(&self, events: &[Event]) -> Interpretation;
    fn respond(&self, interpretation: &Interpretation) -> AgentResponse;
    fn generate(&self, context: &MusicalContext) -> Vec<Event>;
}

#[automock]
trait LuaPattern {
    fn execute(&self, params: &PatternParams) -> Pattern;
}

#[test]
fn agents_collaborate_deterministically() {
    // Setup: Create mocked agents with specific behaviors
    let mut claude_mock = MockMusicAgent::new();
    let mut gemini_mock = MockMusicAgent::new();
    let mut drums_mock = MockLuaPattern::new();

    // Define the interaction sequence we're testing
    let mut seq = Sequence::new();

    // Amy plays D minor phrase - this is fixed input
    let amy_phrase = vec![
        Note::new(D, QUARTER),
        Note::new(F, EIGHTH),
        Note::new(A, HALF),
    ];

    // Claude's interpretation: "This is contemplative"
    claude_mock.expect_listen()
        .times(1)
        .in_sequence(&mut seq)
        .with(eq(amy_phrase.clone()))
        .returning(|_| Interpretation {
            mood: Mood::Contemplative,
            key: Some(Key::D_MINOR),
            energy: 0.3,
            suggests_response: true,
        });

    // Claude delegates to drums - THIS IS THE COLLABORATION WE'RE TESTING
    claude_mock.expect_respond()
        .times(1)
        .in_sequence(&mut seq)
        .returning(|interp| {
            assert_eq!(interp.mood, Mood::Contemplative);
            AgentResponse::DelegateRequest {
                to: "drums.lua",
                request_type: RequestType::Generate,
                params: json!({
                    "style": "brushes",
                    "intensity": 0.2,
                    "mood": "contemplative"
                }),
            }
        });

    // Drums respond with a pattern
    drums_mock.expect_execute()
        .times(1)
        .in_sequence(&mut seq)
        .returning(|params| {
            // Verify the params came from Claude's delegation
            assert_eq!(params.get("style"), Some("brushes"));
            Pattern::from_events(vec![
                DrumHit::new(RIDE, 15, 1.0),
                DrumHit::new(RIDE, 18, 2.0),
                DrumHit::new(RIDE, 12, 3.0),
                DrumHit::new(RIDE, 20, 4.0),
            ])
        });

    // Gemini hears both Amy and drums, decides to add bass
    gemini_mock.expect_listen()
        .times(1)
        .in_sequence(&mut seq)
        .returning(|events| {
            // Gemini's analysis: "Low end is empty, I should fill it"
            Interpretation {
                mood: Mood::Contemplative,
                key: Some(Key::D_MINOR),
                energy: 0.3,
                missing_frequency_range: Some(FreqRange::Bass),
                suggests_response: true,
            }
        });

    // THE KEY MOMENT: Gemini generates something unexpected
    gemini_mock.expect_generate()
        .times(1)
        .in_sequence(&mut seq)
        .returning(|ctx| {
            assert_eq!(ctx.key(), Key::D_MINOR);
            vec![
                Note::new(D, QUARTER),
                Note::new(Db, EIGHTH),
                Note::new(C, EIGHTH),
                Note::new(B, QUARTER),  // Outside the key!
                Note::new(Bb, QUARTER),
                Note::new(A, HALF),
            ]
        });

    // Now test how agents ADAPT to the unexpected
    let mut conversation = Conversation::new();
    conversation.add_human_input(amy_phrase);

    let claude_interp = claude_mock.listen(&conversation.current_events());
    let claude_response = claude_mock.respond(&claude_interp);

    // Claude delegated to drums
    assert!(matches!(claude_response, AgentResponse::DelegateRequest { .. }));

    // Process the delegation
    if let AgentResponse::DelegateRequest { to, params, .. } = claude_response {
        assert_eq!(to, "drums.lua");
        let drum_pattern = drums_mock.execute(&params.into());
        conversation.add_pattern(drum_pattern);
    }

    // Gemini adds unexpected note
    let gemini_bass = gemini_mock.generate(&conversation.context());
    conversation.add_events(gemini_bass.clone());

    // Test the adaptation behavior
    assert!(conversation.contains_outside_note());

    // Mock how Claude would adapt
    claude_mock.expect_listen()
        .times(1)
        .returning(|events| {
            // Claude hears the B natural and recognizes it
            Interpretation {
                mood: Mood::Contemplative,
                key: Some(Key::G_MAJOR),  // Reinterprets!
                energy: 0.5,  // Slightly more energy
                unexpected_but_good: true,
            }
        });

    let adaptation = claude_mock.listen(&conversation.current_events());
    assert!(adaptation.unexpected_but_good);
    assert_eq!(adaptation.key, Some(Key::G_MAJOR));
}
```

## Property-Based Testing for Musical Rules

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn collaboration_maintains_musical_coherence(
        seed: u64,
        agent_count in 2..5usize,
        initial_key in key_strategy(),
        tempo in 60..180u32,
    ) {
        // Property: No matter how agents interact,
        // the result should maintain musical coherence

        let session = mock_jam_session(seed, agent_count, initial_key, tempo);

        // Musical invariants that should hold
        prop_assert!(session.has_consistent_tempo_within_tolerance(0.1));
        prop_assert!(session.notes_relate_to_key_center());
        prop_assert!(session.no_simultaneous_dissonance_beyond_threshold());
    }
}

fn key_strategy() -> impl Strategy<Value = Key> {
    prop_oneof![
        Just(Key::C_MAJOR),
        Just(Key::A_MINOR),
        Just(Key::G_MAJOR),
        Just(Key::D_MINOR),
    ]
}
```

## Testing Delegation Patterns

```rust
#[rstest]
#[case("bass_request", Capability::BassGeneration, "bass_bot")]
#[case("drum_request", Capability::DrumPatterns, "drums.lua")]
#[case("harmony_request", Capability::ChordGeneration, "harmony_ai")]
fn request_routes_to_capable_agent(
    #[case] request_type: &str,
    #[case] capability: Capability,
    #[case] expected_agent: &str,
) {
    let mut registry = MockAgentRegistry::new();

    registry.expect_find_agent_with_capability()
        .with(eq(capability))
        .times(1)
        .returning(move |_| Some(expected_agent.to_string()));

    let router = RequestRouter::new(registry);
    let request = Request::new(request_type, capability);

    assert_eq!(router.route(&request), Some(expected_agent));
}
```

## Simulating Lua Pattern Behavior

```rust
// Instead of executing actual Lua, we mock pattern generators
struct MockedDrumPattern;

impl MockedDrumPattern {
    fn generate_brushes(context: &MusicalContext) -> Pattern {
        // Deterministic pattern based on context
        let tempo = context.tempo();
        let energy = context.energy();

        let velocity = (15.0 + energy * 20.0) as u8;
        let pattern = Pattern::new();

        // Deterministic but context-aware
        for beat in 1..=4 {
            pattern.add_hit(RIDE, velocity, beat as f32);
        }

        // Add ghost note deterministically based on energy
        if energy > 0.3 {
            pattern.add_hit(SNARE, 10, 2.5);
        }

        pattern
    }
}

#[test]
fn lua_patterns_respond_to_context() {
    let low_energy_context = MusicalContext::new()
        .with_energy(0.2)
        .with_tempo(65);

    let high_energy_context = MusicalContext::new()
        .with_energy(0.8)
        .with_tempo(120);

    let quiet_pattern = MockedDrumPattern::generate_brushes(&low_energy_context);
    let loud_pattern = MockedDrumPattern::generate_brushes(&high_energy_context);

    assert!(quiet_pattern.average_velocity() < loud_pattern.average_velocity());
    assert!(loud_pattern.has_ghost_notes());
    assert!(!quiet_pattern.has_ghost_notes());
}
```

## Testing Emergent Behavior

```rust
#[test]
fn collaboration_creates_emergent_structure() {
    // Setup deterministic agent behaviors
    let agents = vec![
        mock_agent_with_behavior(AgentBehavior::CallAndResponse),
        mock_agent_with_behavior(AgentBehavior::Harmonizer),
        mock_agent_with_behavior(AgentBehavior::RhythmKeeper),
    ];

    let mut session = Session::new();

    // Run 32 bars of interaction
    for bar in 0..32 {
        for agent in &agents {
            let events = agent.generate(&session.context_at_bar(bar));
            session.add_events(events);
        }
    }

    // Test for emergent structures
    let structure = session.analyze_structure();

    // Even though no agent explicitly programmed verse/chorus,
    // the interaction should create recognizable patterns
    assert!(structure.has_repeating_sections());
    assert!(structure.has_dynamic_variation());
    assert!(structure.has_call_and_response_patterns());
}
```

## Why This Approach Works

1. **Deterministic**: Same inputs always produce same outputs
2. **Fast**: No actual model inference, just logic testing
3. **Focused**: Tests collaboration patterns, not model quality
4. **Flexible**: Can simulate any interaction scenario

The key insight: We're not testing whether the generated music is "good" - we're testing whether agents genuinely influence each other, delegate appropriately, and maintain musical coherence through their interactions.

---

**Contributors**:
- Amy Tobey
- 🤖 Claude <claude@anthropic.com>
- 💎 Gemini <gemini@google.com>
**Date**: 2025-11-15