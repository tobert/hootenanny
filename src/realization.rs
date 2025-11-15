use crate::domain::{Intention, Sound};

impl Intention {
    pub fn realize(&self) -> Sound {
        let pitch = note_to_midi(&self.what);
        let velocity = feeling_to_velocity(&self.how);

        tracing::info!("🎵 {} {} → pitch:{}, vel:{}",
            self.how, self.what, pitch, velocity);

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
