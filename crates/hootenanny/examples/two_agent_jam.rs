//! An example of a two-agent jam session.

use hootenanny::server::EventDualityServer;
use hootenanny::domain::EmotionalVector;
use hootenanny::domain::AbstractEvent;
use hootenanny::domain::IntentionEvent;
use hootenanny::domain::Event;
use std::sync::{Arc, Mutex};
use hootenanny::server::ConversationState;

fn main() {
    let state_dir = std::env::temp_dir().join("two_agent_jam");
    std::fs::create_dir_all(&state_dir).expect("Failed to create temp dir");

    let state = ConversationState::new(state_dir).expect("Failed to create conversation state");
    let server = EventDualityServer::new_with_state(Arc::new(Mutex::new(state)));

    println!("Two-agent jam session example");

    // 1. Two agents starting a conversation
    let agent1 = "agent_1".to_string();
    let agent2 = "agent_2".to_string();

    let root_event = Event::Abstract(AbstractEvent::Intention(IntentionEvent {
        what: "C".to_string(),
        how: "softly".to_string(),
        emotion: EmotionalVector::neutral(),
    }));

    let mut tree = hootenanny::conversation::ConversationTree::new(
        root_event,
        agent1.clone(),
        EmotionalVector::neutral(),
    );

    // 2. One plays a simple melody
    let melody_event = Event::Abstract(AbstractEvent::Intention(IntentionEvent {
        what: "E".to_string(),
        how: "normally".to_string(),
        emotion: EmotionalVector::neutral(),
    }));
    let _ = tree.add_node(
        &"main".to_string(),
        melody_event,
        agent1.clone(),
        EmotionalVector::neutral(),
        None,
    );

    // 3. Other forks to explore harmony and bass
    let harmony_branch = tree.fork_branch(
        0,
        "harmony".to_string(),
        hootenanny::conversation::ForkReason::ExploreAlternative {
            description: "Try a harmony".to_string(),
        },
        vec![agent2.clone()],
    ).unwrap();

    let harmony_event = Event::Abstract(AbstractEvent::Intention(IntentionEvent {
        what: "G".to_string(),
        how: "softly".to_string(),
        emotion: EmotionalVector::neutral(),
    }));
    let _ = tree.add_node(
        &harmony_branch,
        harmony_event,
        agent2.clone(),
        EmotionalVector::neutral(),
        None,
    );

    println!("Tree: {:?}", tree);
}
