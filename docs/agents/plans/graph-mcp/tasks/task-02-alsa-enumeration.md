# Task 02: ALSA MIDI Enumeration

**Status**: 🟡 Not started
**Estimated effort**: 3-4 hours
**Prerequisites**: Task 01 (SQLite foundation)
**Depends on**: Database layer for identity queries
**Enables**: Task 03 (identity matching), Task 04 (Trustfall adapter)

## 🎯 Goal

Enumerate MIDI devices and ports on the system using ALSA (Advanced Linux Sound Architecture). Extract device fingerprints that can be matched against persisted identities.

**Why ALSA?** Linux audio has evolved (OSS → ALSA → JACK → PipeWire), but ALSA remains the low-level hardware interface. Even on PipeWire systems, MIDI hardware shows up via ALSA sequencer.

## 📋 Context

### ALSA MIDI Architecture

```
Hardware MIDI Device (e.g., JD-Xi connected via USB)
    ↓
USB MIDI driver (snd-usb-audio)
    ↓
ALSA Sequencer (/dev/snd/seq)
    ↓
ALSA Card (hw:2)
    ↓
ALSA Device (hw:2,0)
    ↓
ALSA Subdevice (hw:2,0,0)
    ↓
ALSA Sequencer Port (128:0 - client:port)
```

### What We Need to Extract

For each MIDI device, gather:

1. **Card info**: Card number, name, long name
2. **Device info**: Device ID, subdevice ID
3. **Port info**: Port number, name, capabilities (input/output)
4. **Fingerprints** (for identity matching):
   - `alsa_card`: e.g., "Roland JD-Xi"
   - `alsa_hw`: e.g., "hw:2,0"
   - `midi_name`: e.g., "JD-Xi MIDI 1"
   - (USB info comes from Task 09 - USB adapter)

### Example Output

Given a JD-Xi connected as `hw:2`, we should produce:

```rust
AlsaMidiDevice {
    card_id: 2,
    device_id: 0,
    name: "JD-Xi",
    subdevice_name: Some("JD-Xi MIDI 1"),
    hardware_id: "hw:2,0",
    ports: vec![
        AlsaMidiPort {
            id: "hw:2,0,0",
            name: "JD-Xi MIDI 1",
            direction: PortDirection::Bidirectional,
        },
    ],
}
```

## 🏗️ Module Structure

```
crates/audio-graph-mcp/src/sources/
├── mod.rs
├── alsa.rs           # ALSA enumeration (this task)
├── pipewire.rs       # (Task 06)
├── usb.rs            # (Future)
└── sqlite.rs         # (Task 01 - already exists)
```

## 📦 Dependencies (add to Cargo.toml)

```toml
[dependencies]
alsa = "0.9"          # ALSA bindings
gethostname = "0.4"   # For multi-host awareness
# Existing: rusqlite, serde, anyhow, etc.
```

## 🎨 Types (add to src/types.rs)

```rust
/// Live ALSA MIDI device (not persisted)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlsaMidiDevice {
    pub host: String,         // "desktop", "laptop", or "localhost"
    pub card_id: i32,
    pub device_id: i32,
    pub name: String,
    pub subdevice_name: Option<String>,
    pub hardware_id: String,  // "hw:2,0"
    pub ports: Vec<AlsaMidiPort>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlsaMidiPort {
    pub id: String,           // "hw:2,0,0" or "128:0" (client:port)
    pub name: String,
    pub direction: PortDirection,
    pub capabilities: PortCapabilities,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PortDirection {
    In,
    Out,
    Bidirectional,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortCapabilities {
    pub can_read: bool,
    pub can_write: bool,
    pub can_subs_read: bool,
    pub can_subs_write: bool,
}

/// Device fingerprints for identity matching
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceFingerprint {
    pub kind: HintKind,
    pub value: String,
}
```

## 🔨 Implementation (src/sources/alsa.rs)

### Approach 1: ALSA Sequencer API (Recommended)

Use the `alsa` crate to query the ALSA sequencer directly.

```rust
use alsa::seq::{Seq, ClientIter, PortIter, PortCap, PortType};
use anyhow::{Context, Result};

pub struct AlsaSource;

impl AlsaSource {
    pub fn new() -> Self {
        Self
    }

    /// Enumerate all MIDI devices
    pub fn enumerate_devices(&self) -> Result<Vec<AlsaMidiDevice>> {
        let seq = Seq::open(None, None, false)
            .context("Failed to open ALSA sequencer")?;
        
        let hostname = gethostname::gethostname().into_string().unwrap_or("localhost".into());

        let mut devices = Vec::new();

        // Iterate over ALSA sequencer clients
        for client_info in ClientIter::new(&seq) {
            // ... (filtering logic) ...

            // ... (port iteration) ...

            if !ports.is_empty() {
                devices.push(AlsaMidiDevice {
                    host: hostname.clone(),
                    card_id: -1,  // TODO: Map client to card
                    device_id: client_info.get_client(),
                    name: client_name,
                    subdevice_name: None,
                    hardware_id: format!("client:{}", client_info.get_client()),
                    ports,
                });
            }
        }

        Ok(devices)
    }

    /// Extract fingerprints for identity matching
    pub fn extract_fingerprints(&self, device: &AlsaMidiDevice) -> Vec<DeviceFingerprint> {
        let mut prints = Vec::new();

        // Card name hint
        prints.push(DeviceFingerprint {
            kind: HintKind::AlsaCard,
            value: device.name.clone(),
        });

        // Hardware ID hint
        prints.push(DeviceFingerprint {
            kind: HintKind::AlsaHw,
            value: device.hardware_id.clone(),
        });

        // MIDI port names (useful for matching)
        for port in &device.ports {
            prints.push(DeviceFingerprint {
                kind: HintKind::MidiName,
                value: port.name.clone(),
            });
        }

        prints
    }
}
```

### Approach 2: Parse /proc/asound (Fallback)

If the sequencer API is too complex, fall back to parsing `/proc/asound/`:

```bash
$ cat /proc/asound/cards
 0 [PCH            ]: HDA-Intel - HDA Intel PCH
 1 [Device         ]: USB-Audio - USB Audio Device
 2 [JDXi           ]: USB-Audio - JD-Xi

$ cat /proc/asound/card2/id
JDXi

$ ls /proc/asound/card2/
id  midi0  usbbus  usbid
```

This is simpler but gives less port-level detail.

## 🧪 Testing Strategy

### Test with Virtual MIDI Devices

```bash
# Load virtual MIDI module
sudo modprobe snd-virmidi midi_devs=4

# Verify devices exist
aplaymidi -l
# Should show: Virtual Raw MIDI 0-0, 1-0, 2-0, 3-0
```

### Test Code (tests/alsa_tests.rs)

```rust
use audio_graph_mcp::sources::alsa::AlsaSource;

#[test]
fn test_enumerate_devices() {
    let alsa = AlsaSource::new();
    let devices = alsa.enumerate_devices().expect("Failed to enumerate ALSA devices");

    // Should find at least virtual MIDI devices (if loaded)
    println!("Found {} MIDI devices", devices.len());

    for device in &devices {
        println!("Device: {} ({})", device.name, device.hardware_id);
        for port in &device.ports {
            println!("  Port: {} - {:?}", port.name, port.direction);
        }
    }

    // Basic sanity check
    assert!(!devices.is_empty(), "No MIDI devices found - is snd-virmidi loaded?");
}

#[test]
fn test_extract_fingerprints() {
    let alsa = AlsaSource::new();
    let devices = alsa.enumerate_devices().unwrap();

    if let Some(device) = devices.first() {
        let prints = alsa.extract_fingerprints(device);
        assert!(!prints.is_empty(), "No fingerprints extracted");

        for print in prints {
            println!("{:?}: {}", print.kind, print.value);
        }
    }
}
```

## ✅ Acceptance Criteria

When this task is complete:

1. ✅ `AlsaSource::enumerate_devices()` returns list of MIDI devices
2. ✅ Each device includes ports with direction (In/Out/Bidirectional)
3. ✅ `extract_fingerprints()` produces at least 2 hints per device
4. ✅ Tests pass with `snd-virmidi` virtual devices
5. ✅ When real hardware (JD-Xi, Keystep) is connected, it appears in enumeration
6. ✅ Fingerprints are unique enough to distinguish devices

## 🔍 Verification Commands

```bash
# List ALSA MIDI ports
aplaymidi -l

# List ALSA cards
cat /proc/asound/cards

# Show sequencer clients
cat /proc/asound/seq/clients

# Run tests
cargo test --package audio-graph-mcp --test alsa_tests
```

## 🚧 Out of Scope (for this task)

- ❌ Identity matching algorithm (Task 03)
- ❌ Trustfall integration (Task 04)
- ❌ USB device enumeration (separate task)
- ❌ PipeWire enumeration (Task 06)

Focus ONLY on ALSA MIDI. We'll join with other sources later via Trustfall.

## 💡 Implementation Tips

1. **Start simple**: Get basic enumeration working first (client list)
2. **Filter carefully**: ALSA exposes many internal clients (timers, announce). Skip non-MIDI hardware.
3. **Test early**: Load `snd-virmidi` immediately and verify detection
4. **Map client to card**: Use `/proc/asound/` to correlate sequencer clients with card numbers
5. **Handle errors gracefully**: Missing permissions, no sequencer → clear error messages

## 🐛 Common Pitfalls

- **Permission denied on /dev/snd/seq**: User needs to be in `audio` group
- **No devices found**: Load `snd-virmidi` or connect real MIDI hardware
- **Software synths appear**: Filter by port type (hardware vs software)

## 📚 References

- ALSA Sequencer API: https://www.alsa-project.org/alsa-doc/alsa-lib/seq.html
- `alsa` crate docs: https://docs.rs/alsa/latest/alsa/
- `/proc/asound` format: https://www.kernel.org/doc/html/latest/sound/designs/procfile.html

## 🎬 Next Task

After enumeration works: **[Task 03: Identity Hint Matching System](task-03-identity-matching.md)**

We'll take the fingerprints from this task and match them against the database from Task 01.
