use rmcp::schemars;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Event {
    Abstract(Intention),
    Concrete(Sound),
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Intention {
    #[schemars(description = "The note to play (C, D, E, F, G, A, B)")]
    pub what: String,
    #[schemars(description = "How to play it (softly, normally, boldly, questioning)")]
    pub how: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sound {
    pub pitch: u8,     // MIDI note number
    pub velocity: u8,  // MIDI velocity
}

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
        assert!(matches!(abstract_event, Event::Abstract(_)));
        assert!(matches!(concrete_event, Event::Concrete(_)));
    }
}
