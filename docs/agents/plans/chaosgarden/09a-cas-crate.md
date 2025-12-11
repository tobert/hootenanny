# 09a-cas-crate: Shared Content Addressable Storage

**Prerequisite for**: 10-hootenanny-zmq
**Status**: Planning

## Goal

Extract CAS into a shared crate so hootenanny, chaosgarden, and workers can all access the same content store. Enables NFS-based multi-machine setups.

## Current State

| Location | What | Notes |
|----------|------|-------|
| `hootenanny/src/cas.rs` | Full CAS impl | BLAKE3, metadata, read/write |
| `chaosgarden/src/nodes/audio_file.rs` | `FileCasClient` | Read-only, simpler path layout |

**Mismatch**: hootenanny uses `objects/{prefix}/{hash}`, chaosgarden uses `{prefix}/{hash}`.

## Design

### Crate Structure

```
crates/cas/
├── src/
│   ├── lib.rs           # Re-exports, CasConfig
│   ├── store.rs         # ContentStore trait + FileStore impl
│   ├── hash.rs          # ContentHash newtype, BLAKE3
│   ├── metadata.rs      # CasMetadata, CasReference
│   └── config.rs        # CasConfig with env/file loading
├── Cargo.toml
└── README.md
```

### Core Types

```rust
// crates/cas/src/hash.rs
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ContentHash(String);  // 32 hex chars (128 bits of BLAKE3)

impl ContentHash {
    pub fn from_data(data: &[u8]) -> Self;
    pub fn prefix(&self) -> &str;      // First 2 chars
    pub fn remainder(&self) -> &str;   // Rest of hash
    pub fn as_str(&self) -> &str;
}

// crates/cas/src/store.rs
pub trait ContentStore: Send + Sync {
    fn store(&self, data: &[u8], mime_type: &str) -> Result<ContentHash>;
    fn retrieve(&self, hash: &ContentHash) -> Result<Option<Vec<u8>>>;
    fn exists(&self, hash: &ContentHash) -> bool;
    fn path(&self, hash: &ContentHash) -> Option<PathBuf>;
    fn inspect(&self, hash: &ContentHash) -> Result<Option<CasReference>>;
}

pub struct FileStore {
    config: CasConfig,
    objects_dir: PathBuf,
    metadata_dir: PathBuf,
}
```

### Configuration

```rust
// crates/cas/src/config.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CasConfig {
    /// Base path for CAS storage
    /// Default: ~/.halfremembered/cas or $HALFREMEMBERED_CAS_PATH
    pub base_path: PathBuf,

    /// Whether to write metadata JSON alongside objects
    /// Default: true
    pub store_metadata: bool,

    /// Read-only mode (for chaosgarden)
    /// Default: false
    pub read_only: bool,
}

impl CasConfig {
    /// Load from environment and defaults
    pub fn from_env() -> Result<Self>;

    /// Load from config file, falling back to env
    pub fn from_file(path: &Path) -> Result<Self>;

    /// Use specific base path
    pub fn with_base_path(path: impl Into<PathBuf>) -> Self;
}
```

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `HALFREMEMBERED_CAS_PATH` | `~/.halfremembered/cas` | Base path for CAS |
| `HALFREMEMBERED_CAS_READONLY` | `false` | Read-only mode |

### Path Layout

```
$HALFREMEMBERED_CAS_PATH/
├── objects/
│   ├── ab/
│   │   └── cdef1234...  # Content file
│   └── 12/
│       └── 3456789...
└── metadata/
    ├── ab/
    │   └── cdef1234....json  # {mime_type, size}
    └── 12/
        └── 3456789....json
```

## Migration Plan

### Phase 1: Create crate, move code
1. Create `crates/cas/`
2. Move `hootenanny/src/cas.rs` → `cas/src/store.rs`
3. Add `ContentHash` newtype
4. Add `CasConfig` with env loading
5. Tests pass

### Phase 2: Update hootenanny
1. `hootenanny` depends on `cas`
2. Replace local `Cas` with `cas::FileStore`
3. Use `CasConfig::from_env()`
4. Tests pass

### Phase 3: Update chaosgarden
1. `chaosgarden` depends on `cas`
2. Replace `FileCasClient` with wrapper around `cas::FileStore`
3. Keep `ContentResolver` trait for flexibility (MemoryResolver stays)
4. Tests pass

### Phase 4: Config file support (optional)
1. Add TOML/JSON config file loading
2. `~/.halfremembered/config.toml` or per-project `.halfremembered/config.toml`

## NFS Considerations

- **Write from one machine**: hootenanny/workers write new content
- **Read from any machine**: chaosgarden just needs read access
- **No locking needed**: Content-addressed = write-once = no conflicts
- **Metadata optional for reads**: chaosgarden can work without `.json` files

## Dependencies

```toml
[dependencies]
blake3 = "1"
hex = "0.4"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
anyhow = "1"
directories = "5"  # For default paths (~/.halfremembered)
```

## Acceptance Criteria

- [ ] `ContentHash::from_data()` produces same hash as current hootenanny
- [ ] `FileStore` passes all existing hootenanny CAS tests
- [ ] `CasConfig::from_env()` respects `HALFREMEMBERED_CAS_PATH`
- [ ] hootenanny compiles with `cas` dependency
- [ ] chaosgarden compiles with `cas` dependency
- [ ] Demo works with shared CAS path
- [ ] NFS scenario: write on machine A, read on machine B (manual test)

## Open Questions

- [ ] Config file format: TOML vs JSON vs both?
- [ ] Should metadata be optional or required?
- [ ] Cache layer for remote/slow storage? (future)
