# Task 08: Testing with Virtual Devices

**Status**: 🟡 Not started
**Estimated effort**: 2-3 hours
**Prerequisites**: Task 01 (SQLite), Task 02 (ALSA), Task 03 (Matching)
**Depends on**: ALSA enumeration, database, identity matcher
**Enables**: CI/CD testing without hardware

> **Note**: Moved earlier in sequence. Testing infrastructure is critical for validating identity matching (Task 03) before building the Trustfall adapter (Task 04). Do this BEFORE Task 04.

## 🎯 Goal

Create a **reproducible test environment** using virtual MIDI devices and pre-populated database fixtures. Enable full integration testing without requiring physical hardware.

**Why this matters:** We can't assume every developer has a JD-Xi, Poly 2, and Eurorack modules. Virtual devices let us test the full stack.

## 📋 Context

### Virtual MIDI on Linux: snd-virmidi

The `snd-virmidi` kernel module creates virtual MIDI ports that appear identical to hardware MIDI devices via ALSA.

```bash
# Load module with 4 virtual devices
sudo modprobe snd-virmidi midi_devs=4

# Verify
aplaymidi -l
# Output:
#  port   24:0    Virtual Raw MIDI 2-0
#  port   25:0    Virtual Raw MIDI 2-1
#  port   26:0    Virtual Raw MIDI 2-2
#  port   27:0    Virtual Raw MIDI 2-3
```

### Test Fixture Strategy

1. **Virtual MIDI devices** (via snd-virmidi)
2. **Pre-populated database** (identities, hints, tags)
3. **Test scenarios** (enumeration, matching, queries)
4. **Mock DataProvider** (Gemini review feedback) - JSON fixtures for PipeWire

### Mock DataProvider Pattern (Gemini Recommendation)

Don't call `pw-dump` in tests. Instead, use a trait-based abstraction:

```rust
/// Trait for PipeWire data source - enables mocking
pub trait PipeWireDataProvider: Send + Sync {
    fn get_dump(&self) -> anyhow::Result<Vec<PipeWireObject>>;
}

/// Real implementation - calls pw-dump binary
pub struct RealPipeWireProvider;

impl PipeWireDataProvider for RealPipeWireProvider {
    fn get_dump(&self) -> anyhow::Result<Vec<PipeWireObject>> {
        let output = std::process::Command::new("pw-dump").output()?;
        let dump: Vec<PipeWireObject> = serde_json::from_slice(&output.stdout)?;
        Ok(dump)
    }
}

/// Mock implementation - reads from fixture file
pub struct MockPipeWireProvider {
    fixture_path: PathBuf,
}

impl PipeWireDataProvider for MockPipeWireProvider {
    fn get_dump(&self) -> anyhow::Result<Vec<PipeWireObject>> {
        let content = std::fs::read_to_string(&self.fixture_path)?;
        let dump: Vec<PipeWireObject> = serde_json::from_str(&content)?;
        Ok(dump)
    }
}
```

**Benefits**:
- Snapshot your real studio setup into `fixtures/pipewire-studio.json`
- Use as regression test: "Ensure JD-Xi is always found in this dump"
- No dependency on running PipeWire in CI

## 🔨 Test Fixture Setup (tests/fixtures/mod.rs)

```rust
use audio_graph_mcp::{db::Database, types::*};
use serde_json::json;

pub struct TestFixture {
    pub db: Database,
}

impl TestFixture {
    /// Create fixture with virtual device identities
    pub fn new() -> Self {
        let db = Database::in_memory().unwrap();

        // Create identities matching virtual devices
        Self::setup_virtual_devices(&db);
        Self::setup_eurorack_modules(&db);

        Self { db }
    }

    fn setup_virtual_devices(db: &Database) {
        // Virtual MIDI 0 = "JD-Xi"
        db.create_identity("virt_jdxi", "Virtual JD-Xi", json!({
            "manufacturer": "Roland",
            "model": "JD-Xi",
            "kind": "synth"
        })).unwrap();

        db.add_hint("virt_jdxi", HintKind::AlsaCard, "Virtual Raw MIDI 2-0", 0.9).unwrap();
        db.add_hint("virt_jdxi", HintKind::MidiName, "Virtual Raw MIDI 2-0", 0.8).unwrap();

        db.add_tag("virt_jdxi", "manufacturer", "roland").unwrap();
        db.add_tag("virt_jdxi", "role", "sound-source").unwrap();
        db.add_tag("virt_jdxi", "capability", "midi-in").unwrap();

        // Virtual MIDI 1 = "Keystep Pro"
        db.create_identity("virt_keystep", "Virtual Keystep Pro", json!({
            "manufacturer": "Arturia",
            "model": "Keystep Pro",
            "kind": "controller"
        })).unwrap();

        db.add_hint("virt_keystep", HintKind::AlsaCard, "Virtual Raw MIDI 2-1", 0.9).unwrap();

        db.add_tag("virt_keystep", "manufacturer", "arturia").unwrap();
        db.add_tag("virt_keystep", "role", "controller").unwrap();
    }

    fn setup_eurorack_modules(db: &Database) {
        // Eurorack modules (won't have ALSA devices, just identities)
        let modules = vec![
            ("doepfer_a110", "Doepfer A-110 VCO", "doepfer"),
            ("doepfer_a120", "Doepfer A-120 VCF", "doepfer"),
            ("doepfer_a132", "Doepfer A-132 VCA", "doepfer"),
        ];

        for (id, name, manufacturer) in modules {
            db.create_identity(id, name, json!({
                "manufacturer": manufacturer,
                "form_factor": "eurorack"
            })).unwrap();

            db.add_tag(id, "manufacturer", manufacturer).unwrap();
            db.add_tag(id, "form_factor", "eurorack").unwrap();
        }
    }

    /// Add sample manual connections (patch cables)
    pub fn setup_patch(&self) {
        use uuid::Uuid;

        // Keystep → JD-Xi (MIDI)
        self.db.add_manual_connection(
            &Uuid::new_v4().to_string(),
            "virt_keystep",
            "midi_out",
            "virt_jdxi",
            "midi_in",
            Some("din_midi"),
            Some("forward"),
            "test_fixture",
        ).unwrap();

        // JD-Xi → Doepfer A-110 (CV)
        self.db.add_manual_connection(
            &Uuid::new_v4().to_string(),
            "virt_jdxi",
            "cv_out_1",
            "doepfer_a110",
            "voct_in",
            Some("patch_cable_cv"),
            Some("forward"),
            "test_fixture",
        ).unwrap();
    }
}
```

## 🧪 Integration Tests (tests/integration_tests.rs)

```rust
use audio_graph_mcp::{
    adapter::AudioGraphAdapter,
    sources::alsa::AlsaSource,
    matcher::IdentityMatcher,
};
use std::sync::Arc;

mod fixtures;
use fixtures::TestFixture;

#[test]
fn test_virtual_device_enumeration() {
    let alsa = AlsaSource::new();
    let devices = alsa.enumerate_devices().unwrap();

    // Should find virtual MIDI devices (if snd-virmidi loaded)
    let virt_devices: Vec<_> = devices.iter()
        .filter(|d| d.name.contains("Virtual"))
        .collect();

    if virt_devices.is_empty() {
        eprintln!("⚠️  No virtual MIDI devices found. Load snd-virmidi:");
        eprintln!("    sudo modprobe snd-virmidi midi_devs=4");
        panic!("Virtual devices required for test");
    }

    println!("✓ Found {} virtual devices", virt_devices.len());
}

#[test]
fn test_identity_matching_with_virtual_devices() {
    let fixture = TestFixture::new();
    let alsa = AlsaSource::new();
    let matcher = IdentityMatcher::new(&fixture.db);

    let devices = alsa.enumerate_devices().unwrap();
    let virt_device = devices.iter()
        .find(|d| d.name.contains("Virtual Raw MIDI 2-0"))
        .expect("Virtual device 0 not found");

    let fingerprints = alsa.extract_fingerprints(virt_device);
    let best_match = matcher.best_match(&fingerprints).unwrap();

    assert!(best_match.is_some());
    let matched = best_match.unwrap();
    assert_eq!(matched.identity.id, "virt_jdxi");
    println!("✓ Matched virtual device to identity: {}", matched.identity.name);
}

#[tokio::test]
async fn test_full_stack_query() {
    let fixture = TestFixture::new();
    let adapter = Arc::new(AudioGraphAdapter::new(Arc::new(fixture.db)).unwrap());

    let query = r#"
        query {
            AlsaMidiDevice {
                name @output
                identity {
                    name @output
                    tags {
                        namespace @output
                        value @output
                    }
                }
            }
        }
    "#;

    let results = trustfall::execute_query(
        adapter.schema(),
        adapter.clone(),
        query,
        std::collections::HashMap::new(),
    ).unwrap()
    .collect::<Vec<_>>();

    // Should find matched virtual devices
    let matched = results.iter()
        .filter(|r| r.get("identity").is_some())
        .count();

    assert!(matched > 0, "Expected at least one matched device");
    println!("✓ Full stack query: {} matched devices", matched);
}

#[tokio::test]
async fn test_find_by_tag() {
    let fixture = TestFixture::new();
    let adapter = Arc::new(AudioGraphAdapter::new(Arc::new(fixture.db)).unwrap());

    let query = r#"
        query {
            Identity {
                tags @filter(op: "contains", value: [
                    {namespace: "manufacturer", value: "roland"}
                ])
                name @output
            }
        }
    "#;

    let results = trustfall::execute_query(
        adapter.schema(),
        adapter.clone(),
        query,
        std::collections::HashMap::new(),
    ).unwrap()
    .collect::<Vec<_>>();

    assert!(!results.is_empty());
    println!("✓ Tag filter query: {} Roland devices", results.len());
}
```

## 🔧 Setup Script (scripts/setup_virmidi.sh)

```bash
#!/bin/bash
# Setup virtual MIDI devices for testing

set -e

echo "🎹 Setting up virtual MIDI devices..."

# Check if already loaded
if lsmod | grep -q snd_virmidi; then
    echo "✓ snd-virmidi already loaded"
else
    echo "Loading snd-virmidi module..."
    sudo modprobe snd-virmidi midi_devs=4
    echo "✓ snd-virmidi loaded"
fi

# Verify
echo ""
echo "Available MIDI ports:"
aplaymidi -l | grep -i virtual || echo "⚠️  No virtual ports found"

echo ""
echo "✅ Setup complete!"
echo ""
echo "To run tests:"
echo "  cargo test --package audio-graph-mcp"
```

Make executable:
```bash
chmod +x scripts/setup_virmidi.sh
```

## 🔨 CI/CD Integration (.github/workflows/test.yml)

```yaml
name: Test

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest

    steps:
      - uses: actions/checkout@v3

      - name: Install Rust
        uses: actions-rs/toolchain@v1
        with:
          toolchain: stable

      - name: Setup virtual MIDI
        run: |
          sudo modprobe snd-virmidi midi_devs=4
          aplaymidi -l

      - name: Run tests
        run: cargo test --package audio-graph-mcp --verbose
```

## ✅ Acceptance Criteria

1. ✅ `setup_virmidi.sh` script loads virtual devices
2. ✅ Tests pass with virtual devices loaded
3. ✅ Fixture provides realistic test data (identities, tags)
4. ✅ Integration test demonstrates full query pipeline
5. ✅ CI runs tests with virtual devices

## 💡 Running Tests Locally

```bash
# Setup (one time)
./scripts/setup_virmidi.sh

# Run tests
cargo test --package audio-graph-mcp

# Run specific test
cargo test --package audio-graph-mcp test_virtual_device_enumeration -- --nocapture
```

## 🚧 Limitations

- Virtual devices have generic names (not realistic fingerprints)
- No virtual CV/gate (Eurorack modules untestable without hardware)
- PipeWire testing requires mock `pw-dump` output

## 🎬 Next Task

**[Task 09: Hootenanny Ensemble Integration](task-09-ensemble-integration.md)** - Connect to music generation
