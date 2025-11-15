# Test-Driven Development for HalfRemembered MCP

## Philosophy: Write the Music First

Write tests that express **musical intent** before implementation. Each test should read like a musical scenario, not a technical specification.

## Critical Test Scenarios

### 1. The Core Innovation: Forking Conversations

**Write This Test First:**
```rust
#[test]
fn agents_can_explore_parallel_musical_ideas() {
    // Given: A conversation with a simple melody
    let mut conversation = Conversation::new();
    let melody = Event::concrete(vec![C, E, G]);
    let root = conversation.add_event(melody);

    // When: Two agents fork to explore different responses
    let harmony_branch = conversation.fork(root, "explore_harmony");
    let bass_branch = conversation.fork(root, "explore_bass");

    // Then: Both branches exist independently
    assert_ne!(harmony_branch, bass_branch);
    assert_eq!(conversation.active_branches().len(), 3); // main + 2 forks

    // And: Events on one branch don't affect the other
    conversation.add_to_branch(harmony_branch, Event::concrete(vec![Am, F, G]));
    conversation.add_to_branch(bass_branch, Event::concrete(vec![C, G, C]));

    assert!(conversation.branch_events(harmony_branch).contains_chords());
    assert!(conversation.branch_events(bass_branch).contains_bass_line());
}
```

**Why This Test Matters**: It validates our core innovation - parallel musical exploration.

### 2. Event Duality: Intentions Become Music

**Write This Test First:**
```rust
#[test]
fn abstract_events_generate_concrete_music() {
    // Given: An abstract intention
    let intention = Event::abstract_prompt("Create a walking bass line in C major");
    let context = MusicalContext::new()
        .with_key(Key::C_MAJOR)
        .with_chord_progression(vec!["C", "Am", "F", "G"]);

    // When: The intention is realized with context
    let concrete_events = intention.realize(&context);

    // Then: Concrete notes are generated
    assert!(!concrete_events.is_empty());

    // And: All notes respect the context
    for event in &concrete_events {
        if let Event::Concrete(note) = event {
            assert!(context.is_valid_note(note));
        }
    }
}
```

**Why This Test Matters**: It ensures our Event Duality actually works - abstract becomes concrete.

### 3. Musical Context Influences Everything

**Write This Test First:**
```rust
#[test]
fn context_constrains_generation() {
    // Given: Two different contexts
    let happy_context = MusicalContext::new()
        .with_key(Key::C_MAJOR)
        .with_emotion(Emotion::JOYFUL);

    let sad_context = MusicalContext::new()
        .with_key(Key::A_MINOR)
        .with_emotion(Emotion::MELANCHOLIC);

    // When: Same pattern generator runs in both contexts
    let happy_pattern = generate_pattern(&happy_context);
    let sad_pattern = generate_pattern(&sad_context);

    // Then: Results reflect the context
    assert!(happy_pattern.average_pitch() > sad_pattern.average_pitch());
    assert!(happy_pattern.uses_major_intervals());
    assert!(sad_pattern.uses_minor_intervals());
}
```

**Why This Test Matters**: Context must actually influence generation, not just exist.

### 4. Agent Collaboration Through Requests

**Write This Test First:**
```rust
#[test]
fn agents_can_delegate_to_specialists() {
    // Given: A request queue with registered specialists
    let mut queue = RequestQueue::new();
    queue.register_agent("bass_bot", Capabilities::BASS_GENERATION);

    // When: Claude requests bass generation
    let request = Request::new()
        .from("claude")
        .to_capability(Capabilities::BASS_GENERATION)
        .with_context(musical_context);

    let request_id = queue.submit(request);

    // Then: Request routes to the bass specialist
    assert_eq!(queue.assigned_agent(request_id), Some("bass_bot"));

    // And: Response contains bass-appropriate notes
    let response = queue.await_response(request_id);
    assert!(response.notes_in_bass_register());
}
```

**Why This Test Matters**: Agents must actually collaborate, not just coexist.

### 5. Branch Merging Preserves Musical Coherence

**Write This Test First:**
```rust
#[test]
fn merging_branches_maintains_musical_sense() {
    // Given: Two branches with compatible music
    let mut conversation = create_conversation_with_forks();
    let melody_branch = conversation.branch("melody");
    let harmony_branch = conversation.branch("harmony");

    // When: Branches are merged
    let merged = conversation.merge(melody_branch, harmony_branch);

    // Then: No timing conflicts occur
    assert!(merged.no_simultaneous_conflicts());

    // And: Harmonic relationships are preserved
    for (melody_note, harmony_chord) in merged.aligned_events() {
        assert!(harmony_chord.contains_or_harmonizes(melody_note));
    }
}
```

**Why This Test Matters**: Merging must produce musically valid results, not just concatenate data.

## Unit Test Priorities

### Level 1: Data Structure Invariants
```rust
#[test]
fn note_with_midi2_velocity_preserves_precision() {
    let note = Note::new().with_velocity_u16(32768);
    assert_eq!(note.velocity_u16(), 32768); // Not truncated to 7-bit
}

#[test]
fn conversation_tree_maintains_parent_child_relationships() {
    // Ensure tree operations don't break structure
}

#[test]
fn musical_context_interpolates_smoothly() {
    // Context changes between time points are continuous
}
```

### Level 2: Musical Rules
```rust
#[test]
fn scales_correctly_identify_valid_notes() {
    let c_major = Scale::major(C);
    assert!(c_major.contains(E));
    assert!(!c_major.contains(E_FLAT));
}

#[test]
fn chord_progressions_follow_voice_leading() {
    // Smooth transitions between chords
}
```

### Level 3: System Behavior
```rust
#[test]
fn hot_reload_preserves_conversation_state() {
    // Lua script changes don't lose musical context
}

#[test]
fn websocket_handles_reconnection_gracefully() {
    // Agents can recover from network issues
}
```

## Test-First Development Flow

### For Each New Feature:

1. **Write the Failing Test**
   - Express what you want musically
   - Use domain language, not technical terms
   - Test should fail with "method not found" or similar

2. **Implement Minimal Code**
   - Just enough to make the test compile
   - Test should now fail with wrong behavior

3. **Make It Work**
   - Implement the actual logic
   - Test should pass

4. **Make It Musical**
   - Refactor to use domain concepts
   - Add musical validation

## Example: Adding Fork Operation

```rust
// 1. Write test first (this will fail to compile)
#[test]
fn can_fork_conversation() {
    let mut conv = Conversation::new();
    let fork = conv.fork("trying_something");
    assert!(fork.is_some());
}

// 2. Add minimal implementation
impl Conversation {
    fn fork(&mut self, reason: &str) -> Option<BranchId> {
        None // Compiles but test fails
    }
}

// 3. Make it work
impl Conversation {
    fn fork(&mut self, reason: &str) -> Option<BranchId> {
        let branch_id = BranchId::new();
        self.branches.insert(branch_id, Branch::new(reason));
        Some(branch_id)
    }
}

// 4. Make it musical (add context preservation, etc.)
```

## Critical Integration Tests

These test the **musical experience**, not just the code:

```rust
#[test]
fn two_agents_can_jam_together() {
    // Setup two agents
    // One plays melody
    // Other responds with harmony
    // Both adapt to each other
    // Result sounds musical
}

#[test]
fn conversation_produces_playable_music() {
    // Take any conversation
    // Export to MIDI
    // Result is valid, playable file
}

#[test]
fn system_recovers_from_agent_failure() {
    // One agent crashes
    // Others continue
    // Music doesn't stop
}
```

## Test Philosophy

- **Test Musical Behavior**, not implementation details
- **Tests Should Read Like Music Theory**, not computer science
- **Failing Tests Drive Design**, not the other way around
- **Every Bug Becomes a Test**, preventing regression

---

**Contributors**:
- Amy Tobey
- 🤖 Claude <claude@anthropic.com>
- 💎 Gemini <gemini@google.com>
**Date**: 2025-11-15