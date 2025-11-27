mod fixtures;

use audio_graph_mcp::{
    AudioGraphAdapter, IdentityId,
    sources::AlsaSource,
    matcher::IdentityMatcher,
};
use fixtures::TestFixture;
use std::sync::Arc;
use trustfall::{execute_query, FieldValue};

type Variables = std::collections::BTreeMap<Arc<str>, FieldValue>;

#[test]
fn test_fixture_creates_identities() {
    let fixture = TestFixture::new();

    let identities = fixture.db.list_identities().unwrap();
    assert!(identities.len() >= 7, "Expected at least 7 identities, got {}", identities.len());

    let jdxi = fixture.db.get_identity("jdxi").unwrap();
    assert!(jdxi.is_some(), "JD-Xi identity should exist");
    assert_eq!(jdxi.unwrap().name, "Roland JD-Xi");
}

#[test]
fn test_fixture_creates_tags() {
    let fixture = TestFixture::new();

    let tags = fixture.db.get_tags("jdxi").unwrap();
    assert!(!tags.is_empty(), "JD-Xi should have tags");

    let manufacturers: Vec<_> = tags.iter()
        .filter(|t| t.namespace == "manufacturer")
        .collect();
    assert_eq!(manufacturers.len(), 1);
    assert_eq!(manufacturers[0].value, "roland");
}

#[test]
fn test_fixture_creates_hints() {
    let fixture = TestFixture::new();

    let hints = fixture.db.get_hints("jdxi").unwrap();
    assert!(hints.len() >= 2, "JD-Xi should have multiple hints");

    let usb_hints: Vec<_> = hints.iter()
        .filter(|h| h.kind.as_str() == "usb_device_id")
        .collect();
    assert_eq!(usb_hints.len(), 1);
    assert_eq!(usb_hints[0].value, "0582:0160");
}

#[test]
fn test_fixture_manual_connections() {
    let fixture = TestFixture::new();
    fixture.setup_manual_patch();

    let connections = fixture.db.list_connections(None).unwrap();
    assert_eq!(connections.len(), 2, "Should have 2 manual connections");

    let keystep_id = IdentityId("keystep".to_string());
    let jdxi_id = IdentityId("jdxi".to_string());
    let midi_conn = connections.iter()
        .find(|c| c.from_identity == keystep_id && c.to_identity == jdxi_id);
    assert!(midi_conn.is_some(), "Should have keystep->jdxi connection");
}

#[test]
fn test_fixture_pipewire_snapshot() {
    let fixture = TestFixture::new();

    assert_eq!(fixture.pipewire_snapshot.nodes.len(), 4);
    assert_eq!(fixture.pipewire_snapshot.ports.len(), 6);

    let midi_nodes: Vec<_> = fixture.pipewire_snapshot.nodes.iter()
        .filter(|n| n.media_class.as_deref() == Some("Midi/Bridge"))
        .collect();
    assert_eq!(midi_nodes.len(), 2, "Should have 2 MIDI bridge nodes");
}

#[test]
fn test_adapter_with_fixture() {
    let fixture = TestFixture::new();
    let adapter = Arc::new(
        AudioGraphAdapter::new(fixture.db.clone(), fixture.pipewire_snapshot.clone())
            .expect("Failed to create adapter")
    );

    let query = r#"
        query {
            Identity {
                name @output
            }
        }
    "#;

    let results: Vec<_> = execute_query(
        adapter.schema(),
        adapter.clone(),
        query,
        Variables::new(),
    ).unwrap().collect();

    assert!(results.len() >= 7, "Should return at least 7 identities");
}

#[test]
fn test_query_by_manufacturer_tag() {
    let fixture = TestFixture::new();
    let adapter = Arc::new(
        AudioGraphAdapter::new(fixture.db.clone(), fixture.pipewire_snapshot.clone())
            .expect("Failed to create adapter")
    );

    let query = r#"
        query {
            Identity {
                name @output
                tags {
                    namespace @filter(op: "=", value: ["$ns"])
                    value @filter(op: "=", value: ["$val"])
                }
            }
        }
    "#;

    let mut vars = Variables::new();
    vars.insert("ns".into(), FieldValue::String("manufacturer".into()));
    vars.insert("val".into(), FieldValue::String("roland".into()));

    let results: Vec<_> = execute_query(
        adapter.schema(),
        adapter.clone(),
        query,
        vars,
    ).unwrap().collect();

    assert_eq!(results.len(), 1, "Should find exactly 1 Roland device");
    let name_key: Arc<str> = "name".into();
    assert_eq!(results[0].get(&name_key), Some(&FieldValue::String("Roland JD-Xi".into())));
}

#[test]
fn test_query_pipewire_midi_bridges() {
    let fixture = TestFixture::new();
    let adapter = Arc::new(
        AudioGraphAdapter::new(fixture.db.clone(), fixture.pipewire_snapshot.clone())
            .expect("Failed to create adapter")
    );

    let query = r#"
        query {
            PipeWireNode(media_class: "Midi/Bridge") {
                name @output
                description @output
            }
        }
    "#;

    let results: Vec<_> = execute_query(
        adapter.schema(),
        adapter.clone(),
        query,
        Variables::new(),
    ).unwrap().collect();

    assert_eq!(results.len(), 2, "Should find 2 MIDI bridge nodes");
}

#[test]
fn test_query_pipewire_with_ports() {
    let fixture = TestFixture::new();
    let adapter = Arc::new(
        AudioGraphAdapter::new(fixture.db.clone(), fixture.pipewire_snapshot.clone())
            .expect("Failed to create adapter")
    );

    let query = r#"
        query {
            PipeWireNode(media_class: "Midi/Bridge") {
                name @output
                ports {
                    name @output(name: "port_name")
                    direction @output
                }
            }
        }
    "#;

    let results: Vec<_> = execute_query(
        adapter.schema(),
        adapter.clone(),
        query,
        Variables::new(),
    ).unwrap().collect();

    assert_eq!(results.len(), 4, "Each MIDI node has 2 ports = 4 total results");
}

#[test]
fn test_identity_matching_simulation() {
    let fixture = TestFixture::new();
    let matcher = IdentityMatcher::new(&fixture.db);

    let fingerprints = vec![
        audio_graph_mcp::sources::DeviceFingerprint {
            kind: audio_graph_mcp::HintKind::UsbDeviceId,
            value: "0582:0160".to_string(),
        },
    ];

    let best_match = matcher.best_match(&fingerprints).unwrap();
    assert!(best_match.is_some(), "Should match JD-Xi by USB ID");

    let matched = best_match.unwrap();
    assert_eq!(matched.identity.id.0, "jdxi");
}

#[test]
fn test_alsa_source_available() {
    let alsa = AlsaSource::new();
    println!("ALSA available: {}", alsa.is_available());

    if alsa.is_available() {
        match alsa.enumerate_devices() {
            Ok(devices) => {
                println!("Found {} ALSA MIDI devices", devices.len());
                for device in &devices {
                    println!("  - {} (client {})", device.client_name, device.client_id);
                }
            }
            Err(e) => {
                println!("ALSA enumeration failed: {}", e);
            }
        }
    }
}

#[test]
fn test_eurorack_identities() {
    let fixture = TestFixture::new();

    let eurorack: Vec<_> = fixture.db.list_identities().unwrap()
        .into_iter()
        .filter(|i| {
            fixture.db.get_tags(&i.id.0).unwrap()
                .iter()
                .any(|t| t.namespace == "form_factor" && t.value == "eurorack")
        })
        .collect();

    assert_eq!(eurorack.len(), 4, "Should have 4 eurorack modules");
}
