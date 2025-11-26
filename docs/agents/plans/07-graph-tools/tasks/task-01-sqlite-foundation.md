# Task 01: SQLite Foundation and Schema

**Status**: ✅ Complete (4 tests passing)
**Estimated effort**: 2-3 hours
**Prerequisites**: None (starting point)
**Depends on**: Nothing
**Enables**: All other tasks

## 🎯 Goal

Build the persistent data layer for audio-graph-mcp: SQLite database with schema for identities, hints, tags, notes, manual connections, and changelog.

**Why this first?** The identity system is the foundation. We need to persist device bindings before we can enumerate live devices and match them.

## 📋 Context

Audio devices have **fluid identity**:
- USB paths change: `/dev/snd/midiC2D0` becomes `midiC3D0` after reboot
- MIDI names vary: "JD-Xi" vs "Roland JD-Xi MIDI 1" vs "JD-Xi MIDI Port 1"
- Serial numbers aren't always exposed
- Devices get unplugged, firmware updated, or shelved

We solve this with **identity hints**: multiple fingerprints that map to a stable logical identity.

```
Identity: "jdxi"
  ├─ name: "Roland JD-Xi"
  ├─ data: {"manufacturer": "Roland", "model": "JD-Xi", "kind": "synth"}
  └─ hints:
       ├─ (usb_device_id, "0582:0160", confidence: 1.0)
       ├─ (midi_name, "JD-Xi", confidence: 0.9)
       └─ (alsa_card, "Roland JD-Xi", confidence: 0.8)
```

When a live device is discovered, we extract its fingerprints and match against stored hints.

## 🗃️ Database Schema

Implement this schema exactly as specified:

```sql
-- ============================================
-- IDENTITY BINDINGS
-- "This hardware fingerprint = this logical node"
-- ============================================

CREATE TABLE identities (
    id TEXT PRIMARY KEY,              -- UUID or slug like "jdxi"
    name TEXT NOT NULL,               -- Human-readable: "Roland JD-Xi"
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    data JSON NOT NULL DEFAULT '{}'  -- {"manufacturer": "Roland", "model": "JD-Xi", ...}
);

-- Multiple hints can map to one identity
CREATE TABLE identity_hints (
    identity_id TEXT NOT NULL REFERENCES identities(id) ON DELETE CASCADE,
    hint_kind TEXT NOT NULL,  -- 'usb_device_id', 'midi_name', 'alsa_card', ...
    hint_value TEXT NOT NULL,
    confidence REAL NOT NULL DEFAULT 1.0,  -- 0.0 to 1.0
    PRIMARY KEY (hint_kind, hint_value)
);

CREATE INDEX idx_hints_identity ON identity_hints(identity_id);

-- ============================================
-- ANNOTATIONS
-- ============================================

CREATE TABLE tags (
    identity_id TEXT NOT NULL REFERENCES identities(id) ON DELETE CASCADE,
    namespace TEXT NOT NULL,  -- 'manufacturer', 'capability', 'role', 'user', ...
    value TEXT NOT NULL,      -- 'roland', 'midi-in', 'sound-source', ...
    PRIMARY KEY (identity_id, namespace, value)
);

CREATE INDEX idx_tags_ns_val ON tags(namespace, value);

CREATE TABLE notes (
    id TEXT PRIMARY KEY,
    target_kind TEXT NOT NULL,  -- 'identity', 'port', 'connection'
    target_id TEXT NOT NULL,    -- For live entities, use canonical ID
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    source TEXT NOT NULL,       -- 'user', 'agent', 'discovery'
    message TEXT NOT NULL
);

CREATE INDEX idx_notes_target ON notes(target_kind, target_id);

-- ============================================
-- MANUAL CONNECTIONS
-- Physical patches we can't auto-detect
-- ============================================

CREATE TABLE manual_connections (
    id TEXT PRIMARY KEY,
    from_identity TEXT NOT NULL,
    from_port TEXT NOT NULL,      -- Port name pattern
    to_identity TEXT NOT NULL,
    to_port TEXT NOT NULL,
    transport_kind TEXT,          -- 'patch_cable_cv', 'din_midi', 'trs_midi_a', etc.
    signal_direction TEXT,        -- 'forward', 'reverse', 'unknown'
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    created_by TEXT NOT NULL,
    UNIQUE (from_identity, from_port, to_identity, to_port)
);

-- ============================================
-- CHANGELOG (append-only telemetry)
-- ============================================

CREATE TABLE changelog (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp TEXT NOT NULL DEFAULT (datetime('now')),
    source TEXT NOT NULL,         -- 'discovery:alsa', 'agent', 'user'
    operation TEXT NOT NULL,      -- 'identity_create', 'tag_add', 'connection_add'
    target_kind TEXT NOT NULL,
    target_id TEXT NOT NULL,
    details JSON NOT NULL
);

CREATE INDEX idx_changelog_target ON changelog(target_kind, target_id);
CREATE INDEX idx_changelog_time ON changelog(timestamp DESC);
```

## 🏗️ Module Structure

```
crates/audio-graph-mcp/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── db/
│   │   ├── mod.rs              # pub use items
│   │   ├── schema.rs           # SQL schema constant
│   │   ├── connection.rs       # Database connection pool
│   │   ├── identity.rs         # Identity CRUD
│   │   ├── hints.rs            # Hint CRUD
│   │   ├── tags.rs             # Tag CRUD
│   │   ├── notes.rs            # Note CRUD
│   │   ├── connections.rs      # Manual connection CRUD
│   │   └── changelog.rs        # Changelog append-only ops
│   └── types.rs                # Core types
└── tests/
    └── db_tests.rs
```

## 📦 Dependencies (Cargo.toml)

```toml
[dependencies]
rusqlite = { version = "0.32", features = ["bundled", "serde_json"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
anyhow = "1"
thiserror = "1"
uuid = { version = "1", features = ["v4", "serde"] }
```

## 🎨 Core Types (src/types.rs)

Define these rich types to prevent "primitive obsession":

```rust
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

// Newtypes for strong typing
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct IdentityId(pub String);

impl fmt::Display for IdentityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for IdentityId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManualConnectionId(pub String);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NoteId(pub String);

/// Stable logical identity for a device
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Identity {
    pub id: IdentityId,
    pub name: String,
    pub created_at: String,
    pub data: serde_json::Value,
}

/// A hint that maps to an identity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityHint {
    pub identity_id: IdentityId,
    pub kind: HintKind,
    pub value: String,
    pub confidence: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HintKind {
    UsbDeviceId,     // VID:PID - strongest (but same for identical devices)
    UsbSerial,       // Unique serial (gold standard, but many devices lack it)
    UsbPath,         // USB topology path (e.g., "usb-0000:00:14.0-3.2") - for twin devices
    MidiName,
    AlsaCard,
    AlsaHw,
    PipewireName,
    PipewireAlsaPath, // Links PipeWire node to underlying ALSA device
}

// Implement Display/FromStr for database persistence
impl fmt::Display for HintKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::UsbDeviceId => "usb_device_id",
            Self::UsbSerial => "usb_serial",
            Self::UsbPath => "usb_path",
            Self::MidiName => "midi_name",
            Self::AlsaCard => "alsa_card",
            Self::AlsaHw => "alsa_hw",
            Self::PipewireName => "pipewire_name",
            Self::PipewireAlsaPath => "pipewire_alsa_path",
        };
        write!(f, "{}", s)
    }
}

impl FromStr for HintKind {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "usb_device_id" => Ok(Self::UsbDeviceId),
            "usb_serial" => Ok(Self::UsbSerial),
            "usb_path" => Ok(Self::UsbPath),
            "midi_name" => Ok(Self::MidiName),
            "alsa_card" => Ok(Self::AlsaCard),
            "alsa_hw" => Ok(Self::AlsaHw),
            "pipewire_name" => Ok(Self::PipewireName),
            "pipewire_alsa_path" => Ok(Self::PipewireAlsaPath),
            _ => Err(format!("Unknown hint kind: {}", s)),
        }
    }
}

impl HintKind {
    pub fn as_str(&self) -> &'static str {
        // (Redundant with Display but useful for const contexts if needed)
        match self {
            Self::UsbDeviceId => "usb_device_id",
            Self::UsbSerial => "usb_serial",
            Self::MidiName => "midi_name",
            Self::AlsaCard => "alsa_card",
            Self::AlsaHw => "alsa_hw",
            Self::PipewireName => "pipewire_name",
        }
    }
}

/// Tag on an identity (namespace:value)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub identity_id: IdentityId,
    pub namespace: String,
    pub value: String,
}

/// Note attached to any entity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    pub id: NoteId,
    pub target_kind: String,
    pub target_id: String,
    pub created_at: String,
    pub source: String,
    pub message: String,
}

/// Manual connection (patch cable)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManualConnection {
    pub id: ManualConnectionId,
    pub from_identity: IdentityId,
    pub from_port: String,
    pub to_identity: IdentityId,
    pub to_port: String,
    pub transport_kind: Option<String>,
    pub signal_direction: Option<String>,
    pub created_at: String,
    pub created_by: String,
}

/// Changelog entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangelogEntry {
    pub id: i64,
    pub timestamp: String,
    pub source: String,
    pub operation: String,
    pub target_kind: String,
    pub target_id: String,
    pub details: serde_json::Value,
}
```

## 🔨 Implementation Checklist

### Database Connection (src/db/connection.rs)
- [ ] Create `Database` struct wrapping `std::sync::Mutex<rusqlite::Connection>`
- [ ] Implement schema initialization (run SQL on first open)
- [ ] Support `:memory:` for tests, file path for production
- [ ] Enable foreign keys: `PRAGMA foreign_keys = ON`

### Identity CRUD (src/db/identity.rs)
- [ ] `create_identity(id, name, data) -> Result<Identity>`
- [ ] `get_identity(id) -> Result<Option<Identity>>`
- [ ] `list_identities() -> Result<Vec<Identity>>`
- [ ] `delete_identity(id) -> Result<()>` (cascade deletes hints/tags)

### Hint CRUD (src/db/hints.rs)
- [ ] `add_hint(identity_id, kind, value, confidence) -> Result<()>`
- [ ] `get_hints(identity_id) -> Result<Vec<IdentityHint>>`
- [ ] `find_identity_by_hint(kind, value) -> Result<Option<Identity>>`
- [ ] `delete_hint(kind, value) -> Result<()>`

### Tag CRUD (src/db/tags.rs)
- [ ] `add_tag(identity_id, namespace, value) -> Result<()>`
- [ ] `remove_tag(identity_id, namespace, value) -> Result<()>`
- [ ] `get_tags(identity_id) -> Result<Vec<Tag>>`
- [ ] `find_identities_by_tag(namespace, value) -> Result<Vec<Identity>>`

### Note CRUD (src/db/notes.rs)
- [ ] `add_note(target_kind, target_id, source, message) -> Result<Note>`
- [ ] `get_notes(target_kind, target_id) -> Result<Vec<Note>>`

### Manual Connections (src/db/connections.rs)
- [ ] `add_connection(from, from_port, to, to_port, transport, direction, created_by) -> Result<ManualConnection>`
- [ ] `remove_connection(id) -> Result<()>`
- [ ] `get_connections(identity_id) -> Result<Vec<ManualConnection>>`

### Changelog (src/db/changelog.rs)
- [ ] `log_event(source, operation, target_kind, target_id, details) -> Result<()>`
- [ ] `get_changelog(since, target) -> Result<Vec<ChangelogEntry>>`

### Tests (tests/db_tests.rs)
- [ ] Test identity create/read/delete
- [ ] Test hint matching: add hint, find identity by hint
- [ ] Test tag queries: find identities by tag
- [ ] Test foreign key cascades: delete identity → hints/tags deleted
- [ ] Test changelog append-only behavior

## ✅ Acceptance Criteria

When this task is complete:

1. ✅ `cargo test` passes with 100% test coverage for db module
2. ✅ Can create identity with multiple hints
3. ✅ Can query: "Find identity with hint (usb_device_id, '0582:0160')" → returns identity
4. ✅ Can tag identity and query by tag: "Find all identities tagged manufacturer:roland"
5. ✅ Deleting identity cascades to hints and tags
6. ✅ Changelog records all mutations

## 🧪 Test Example

```rust
#[test]
fn test_identity_hint_matching() {
    let db = Database::in_memory().unwrap();

    // Create identity
    db.create_identity("jdxi", "Roland JD-Xi", json!({"model": "JD-Xi"})).unwrap();

    // Add hints
    db.add_hint("jdxi", HintKind::UsbDeviceId, "0582:0160", 1.0).unwrap();
    db.add_hint("jdxi", HintKind::MidiName, "JD-Xi", 0.9).unwrap();

    // Query by hint
    let found = db.find_identity_by_hint(HintKind::UsbDeviceId, "0582:0160").unwrap();
    assert_eq!(found.unwrap().id, "jdxi");

    // Tag it
    db.add_tag("jdxi", "manufacturer", "roland").unwrap();
    db.add_tag("jdxi", "role", "sound-source").unwrap();

    // Query by tag
    let roland_devices = db.find_identities_by_tag("manufacturer", "roland").unwrap();
    assert_eq!(roland_devices.len(), 1);
}
```

## 📚 References

- SQLite JSON functions: https://www.sqlite.org/json1.html
- rusqlite docs: https://docs.rs/rusqlite/latest/rusqlite/

## 🚧 Out of Scope (for this task)

- ❌ Live device enumeration (Task 02)
- ❌ Identity matching algorithm (Task 03)
- ❌ Trustfall integration (Task 04)
- ❌ MCP tools (Task 05)

Focus ONLY on the database layer. Get the persistence right, then build on top.

## 💡 Implementation Tips

1. **Start with types.rs**: Define the domain model first
2. **Schema in one place**: Put all SQL in `db/schema.rs` as a constant
3. **Test-driven**: Write tests first, implement to pass
4. **Use transactions**: Wrap multi-step operations in `db.transaction()`
5. **Error context**: Use `anyhow::Context` to add context to errors

## 🎬 Next Task

After this is complete and tested: **[Task 02: ALSA MIDI Enumeration](task-02-alsa-enumeration.md)**
