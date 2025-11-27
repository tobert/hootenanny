use audio_graph_mcp::{
    Database, HintKind,
    sources::{PipeWireNode, PipeWirePort, PipeWireSnapshot, pipewire::PortDirection},
};
use serde_json::json;
use std::sync::Arc;

pub struct TestFixture {
    pub db: Arc<Database>,
    pub pipewire_snapshot: PipeWireSnapshot,
}

impl TestFixture {
    pub fn new() -> Self {
        let db = Database::in_memory().expect("Failed to create in-memory database");

        Self::setup_synth_identities(&db);
        Self::setup_controller_identities(&db);
        Self::setup_eurorack_modules(&db);

        let pipewire_snapshot = Self::create_pipewire_snapshot();

        Self {
            db: Arc::new(db),
            pipewire_snapshot,
        }
    }

    fn setup_synth_identities(db: &Database) {
        db.create_identity("jdxi", "Roland JD-Xi", json!({
            "manufacturer": "Roland",
            "model": "JD-Xi",
            "kind": "synth"
        })).expect("Failed to create jdxi identity");

        db.add_hint("jdxi", HintKind::UsbDeviceId, "0582:0160", 1.0).unwrap();
        db.add_hint("jdxi", HintKind::MidiName, "JD-Xi", 0.9).unwrap();
        db.add_hint("jdxi", HintKind::PipewireName, "JD-Xi", 0.85).unwrap();

        db.add_tag("jdxi", "manufacturer", "roland").unwrap();
        db.add_tag("jdxi", "role", "sound-source").unwrap();
        db.add_tag("jdxi", "capability", "midi-in").unwrap();
        db.add_tag("jdxi", "capability", "midi-out").unwrap();

        db.create_identity("poly2", "Dreadbox Poly 2", json!({
            "manufacturer": "Dreadbox",
            "model": "Poly 2",
            "kind": "synth"
        })).expect("Failed to create poly2 identity");

        db.add_hint("poly2", HintKind::UsbDeviceId, "1234:5678", 1.0).unwrap();
        db.add_hint("poly2", HintKind::MidiName, "Poly 2", 0.9).unwrap();

        db.add_tag("poly2", "manufacturer", "dreadbox").unwrap();
        db.add_tag("poly2", "role", "sound-source").unwrap();
        db.add_tag("poly2", "capability", "cv-out").unwrap();
    }

    fn setup_controller_identities(db: &Database) {
        db.create_identity("keystep", "Arturia Keystep Pro", json!({
            "manufacturer": "Arturia",
            "model": "Keystep Pro",
            "kind": "controller"
        })).expect("Failed to create keystep identity");

        db.add_hint("keystep", HintKind::UsbDeviceId, "1c75:0288", 1.0).unwrap();
        db.add_hint("keystep", HintKind::MidiName, "Keystep Pro", 0.9).unwrap();

        db.add_tag("keystep", "manufacturer", "arturia").unwrap();
        db.add_tag("keystep", "role", "controller").unwrap();
        db.add_tag("keystep", "capability", "sequencer").unwrap();
        db.add_tag("keystep", "capability", "arpeggiator").unwrap();
    }

    fn setup_eurorack_modules(db: &Database) {
        let modules = vec![
            ("doepfer_a110", "Doepfer A-110 VCO", "doepfer", "oscillator"),
            ("doepfer_a120", "Doepfer A-120 VCF", "doepfer", "filter"),
            ("doepfer_a132", "Doepfer A-132 VCA", "doepfer", "amplifier"),
            ("maths", "Make Noise Maths", "makenoise", "function"),
        ];

        for (id, name, manufacturer, role) in modules {
            db.create_identity(id, name, json!({
                "manufacturer": manufacturer,
                "form_factor": "eurorack"
            })).expect(&format!("Failed to create {} identity", id));

            db.add_tag(id, "manufacturer", manufacturer).unwrap();
            db.add_tag(id, "form_factor", "eurorack").unwrap();
            db.add_tag(id, "role", role).unwrap();
        }
    }

    fn create_pipewire_snapshot() -> PipeWireSnapshot {
        PipeWireSnapshot {
            nodes: vec![
                PipeWireNode {
                    id: 42,
                    name: "JD-Xi".to_string(),
                    description: Some("Roland JD-Xi Synthesizer".to_string()),
                    media_class: Some("Midi/Bridge".to_string()),
                    device_bus_path: Some("usb-0000:00:14.0-1".to_string()),
                    alsa_card: Some("2".to_string()),
                },
                PipeWireNode {
                    id: 43,
                    name: "Keystep Pro".to_string(),
                    description: Some("Arturia Keystep Pro".to_string()),
                    media_class: Some("Midi/Bridge".to_string()),
                    device_bus_path: Some("usb-0000:00:14.0-2".to_string()),
                    alsa_card: Some("3".to_string()),
                },
                PipeWireNode {
                    id: 100,
                    name: "Built-in Audio".to_string(),
                    description: Some("Built-in Audio Analog Stereo".to_string()),
                    media_class: Some("Audio/Sink".to_string()),
                    device_bus_path: None,
                    alsa_card: Some("0".to_string()),
                },
                PipeWireNode {
                    id: 101,
                    name: "Bitwig Studio".to_string(),
                    description: Some("Bitwig Studio Audio".to_string()),
                    media_class: Some("Stream/Output/Audio".to_string()),
                    device_bus_path: None,
                    alsa_card: None,
                },
            ],
            ports: vec![
                PipeWirePort {
                    id: 200,
                    node_id: 42,
                    name: "capture_0".to_string(),
                    direction: PortDirection::In,
                    media_type: Some("8 bit raw midi".to_string()),
                },
                PipeWirePort {
                    id: 201,
                    node_id: 42,
                    name: "playback_0".to_string(),
                    direction: PortDirection::Out,
                    media_type: Some("8 bit raw midi".to_string()),
                },
                PipeWirePort {
                    id: 202,
                    node_id: 43,
                    name: "capture_0".to_string(),
                    direction: PortDirection::In,
                    media_type: Some("8 bit raw midi".to_string()),
                },
                PipeWirePort {
                    id: 203,
                    node_id: 43,
                    name: "playback_0".to_string(),
                    direction: PortDirection::Out,
                    media_type: Some("8 bit raw midi".to_string()),
                },
                PipeWirePort {
                    id: 300,
                    node_id: 100,
                    name: "playback_FL".to_string(),
                    direction: PortDirection::In,
                    media_type: Some("32 bit float mono audio".to_string()),
                },
                PipeWirePort {
                    id: 301,
                    node_id: 100,
                    name: "playback_FR".to_string(),
                    direction: PortDirection::In,
                    media_type: Some("32 bit float mono audio".to_string()),
                },
            ],
            links: vec![],
        }
    }

    pub fn setup_manual_patch(&self) {
        use uuid::Uuid;

        self.db.add_connection(
            &Uuid::new_v4().to_string(),
            "keystep",
            "midi_out",
            "jdxi",
            "midi_in",
            Some("din_midi"),
            "test_fixture",
        ).expect("Failed to add keystep->jdxi connection");

        self.db.add_connection(
            &Uuid::new_v4().to_string(),
            "poly2",
            "cv_out_1",
            "doepfer_a110",
            "voct_in",
            Some("patch_cable_cv"),
            "test_fixture",
        ).expect("Failed to add poly2->a110 connection");
    }
}

impl Default for TestFixture {
    fn default() -> Self {
        Self::new()
    }
}

pub fn studio_pipewire_snapshot() -> PipeWireSnapshot {
    TestFixture::create_pipewire_snapshot()
}
