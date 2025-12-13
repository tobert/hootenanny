# Global Configuration System for Hootenanny

A minimal infrastructure config system. Dynamic runtime state belongs elsewhere.

## Design Philosophy

**Two kinds of settings:**

| Type | Purpose | Examples | Mutability |
|------|---------|----------|------------|
| **Infrastructure** | Where things physically ARE | CAS path, bind ports, socket dirs | Fixed (restart required) |
| **Bootstrap Defaults** | Seed initial runtime state | Model endpoints, timeouts | Startup only, runtime owns it after |

**Everything else is runtime state** - should be queryable and mutable, not static config.

## Hardcoded Values Audit

| Category | Count | Disposition |
|----------|-------|-------------|
| **Filesystem Paths** | 5 | → Infrastructure config |
| **Bind Addresses** | 4 | → Infrastructure config |
| **Telemetry** | 2 | → Infrastructure config |
| **Model Service Ports** | 9 | → Bootstrap defaults (future: runtime registry) |
| **Timeouts/Limits** | 16 | → Bootstrap defaults (future: runtime policies) |
| **Buffer Sizes** | 5 | → Compiled defaults (unsafe to change at runtime) |

## Design Goals

1. **Minimal** - Only true infrastructure, ~12 lines
2. **Zero-config works** - Sensible compiled defaults
3. **Layered** - defaults → config file → env vars → CLI args
4. **Discoverable** - `hootenanny config show` dumps effective config

## Config File Location

```
~/.config/hootenanny/config.toml      # User config (XDG_CONFIG_HOME)
/etc/hootenanny/config.toml           # System config (optional)
./hootenanny.toml                     # Local override (dev/testing)
```

Load order (later wins):
1. Compiled defaults
2. `/etc/hootenanny/config.toml` (if exists)
3. `~/.config/hootenanny/config.toml` (if exists)
4. `./hootenanny.toml` (if exists)
5. Environment variables (`HOOTENANNY_*`)
6. CLI arguments

## Infrastructure Config Schema

```toml
# ~/.config/hootenanny/config.toml
# Only things that physically cannot change at runtime

[paths]
state_dir = "~/.local/share/hootenanny"
cas_dir = "~/.hootenanny/cas"
socket_dir = "/tmp"

[bind]
# What THIS process binds to (restart required to change)
http_port = 8082
zmq_router = "tcp://0.0.0.0:5580"
zmq_pub = "tcp://0.0.0.0:5581"

[telemetry]
otlp_endpoint = "127.0.0.1:4317"
log_level = "info"
```

That's it. ~12 lines of true infrastructure.

## Bootstrap Defaults Schema

```toml
# These seed runtime state at startup
# Runtime becomes source of truth after boot

[bootstrap.models]
orpheus = "http://127.0.0.1:2000"
orpheus_classifier = "http://127.0.0.1:2001"
orpheus_bridge = "http://127.0.0.1:2002"
orpheus_loops = "http://127.0.0.1:2003"
musicgen = "http://127.0.0.1:2006"
clap = "http://127.0.0.1:2007"
yue = "http://127.0.0.1:2008"
beatthis = "http://127.0.0.1:2012"
gpu_observer = "http://127.0.0.1:2099"

[bootstrap.connections]
chaosgarden = "local"
luanette = "tcp://localhost:5570"

[bootstrap.media]
# Directories to search for SoundFonts (.sf2, .sf3)
soundfont_dirs = [
    "~/midi/SF2",
    "/usr/share/sounds/sf2",
]
# Directories to search for samples (.wav, .flac, etc) - future use
sample_dirs = [
    "~/samples",
]

[bootstrap.defaults]
lua_timeout = "30s"
session_expiration = "5m"
max_concurrent_jobs = 4
```

## Environment Variable Mapping

```
[paths]
state_dir       → HOOTENANNY_STATE_DIR
cas_dir         → HOOTENANNY_CAS_DIR

[bind]
http_port       → HOOTENANNY_HTTP_PORT
zmq_router      → HOOTENANNY_ZMQ_ROUTER

[telemetry]
otlp_endpoint   → HOOTENANNY_OTLP_ENDPOINT (or OTEL_EXPORTER_OTLP_ENDPOINT)

[bootstrap.models]
orpheus         → HOOTENANNY_MODEL_ORPHEUS
```

## Implementation

### Config Crate (`crates/hoot-config`)

**Critical: Minimal dependencies to avoid circular imports.**

```toml
# Cargo.toml - KEEP THIS LEAN
[dependencies]
serde = { version = "1", features = ["derive"] }
toml = "0.8"
directories = "5"
thiserror = "2"

# NO: anyhow, tokio, tracing, or any hootenanny crates
```

```
crates/hoot-config/
├── Cargo.toml
└── src/
    ├── lib.rs          # load(), HootConfig
    ├── infra.rs        # PathsConfig, BindConfig, TelemetryConfig
    ├── bootstrap.rs    # ModelsConfig, ConnectionsConfig, MediaConfig, DefaultsConfig
    └── loader.rs       # File discovery, env overlay, path expansion
```

**Key types:**
```rust
/// Infrastructure - cannot change at runtime
pub struct InfraConfig {
    pub paths: PathsConfig,
    pub bind: BindConfig,
    pub telemetry: TelemetryConfig,
}

/// Bootstrap - seeds runtime, then runtime owns it
pub struct BootstrapConfig {
    pub models: HashMap<String, String>,
    pub connections: ConnectionsConfig,
    pub media: MediaConfig,
    pub defaults: DefaultsConfig,
}

pub struct HootConfig {
    pub infra: InfraConfig,
    pub bootstrap: BootstrapConfig,
}

impl HootConfig {
    pub fn load() -> Result<Self, ConfigError>;
    pub fn to_toml(&self) -> String;
}
```

### Integration

Each crate imports only what it needs:

```rust
// cas/src/lib.rs
use hoot_config::InfraConfig;
let config = HootConfig::load()?;
let cas = FileStore::at_path(&config.infra.paths.cas_dir)?;

// hootenanny/src/main.rs
use hoot_config::HootConfig;
let config = HootConfig::load()?;
// CLI args override config.infra.bind.*
```

### CLI Subcommand

```bash
hootenanny config show      # Dump effective config
hootenanny config init      # Generate default config.toml
```

### Tool: `config_get` (Read-Only)

**Lives in**: `crates/hootenanny/src/api/tools/config.rs`

**Architecture**: Native hootenanny tool on ZMQ, exposed via MCP through holler (like all other tools).

Why not in hoot-config crate: Tools need serde_json, the job system, etc. Config crate stays lean.

```rust
/// Read-only access to current configuration
///
/// Returns infrastructure config, bootstrap defaults, and effective values.
/// Does NOT allow mutation - config changes require restart or runtime tools.
#[derive(Debug, Deserialize)]
pub struct ConfigGetParams {
    /// Optional: specific section to return ("paths", "bind", "telemetry", "models", etc.)
    /// If omitted, returns entire config
    pub section: Option<String>,

    /// Optional: specific key within section
    /// e.g. section="paths", key="cas_dir" returns just that value
    pub key: Option<String>,
}

// Dispatched via EventDualityServer like other tools
pub async fn config_get(params: ConfigGetParams, config: &HootConfig) -> Result<Value>
```

**Example responses:**

```json
// config_get {}
{
  "infra": {
    "paths": {
      "state_dir": "/home/user/.local/share/hootenanny",
      "cas_dir": "/home/user/.hootenanny/cas",
      "socket_dir": "/tmp"
    },
    "bind": {
      "http_port": 8082,
      "zmq_router": "tcp://0.0.0.0:5580",
      "zmq_pub": "tcp://0.0.0.0:5581"
    },
    "telemetry": {
      "otlp_endpoint": "127.0.0.1:4317",
      "log_level": "info"
    }
  },
  "bootstrap": {
    "models": { ... },
    "connections": { ... },
    "media": {
      "soundfont_dirs": ["~/midi/SF2", "/usr/share/sounds/sf2"],
      "sample_dirs": ["~/samples"]
    },
    "defaults": { ... }
  },
  "sources": {
    "config_file": "/home/user/.config/hootenanny/config.toml",
    "env_overrides": ["HOOTENANNY_CAS_DIR"]
  }
}

// config_get { section: "paths" }
{
  "state_dir": "/home/user/.local/share/hootenanny",
  "cas_dir": "/home/user/.hootenanny/cas",
  "socket_dir": "/tmp"
}

// config_get { section: "paths", key: "cas_dir" }
{
  "value": "/home/user/.hootenanny/cas",
  "source": "config_file"  // or "default", "env", "cli"
}
```

**Use cases:**
- Agent discovers where CAS is stored
- Agent finds soundfont directories to search
- Debugging: "where did this value come from?"
- Documentation: dump config for bug reports

## Future Work (TODOs)

These are OUT OF SCOPE for the config crate, but noted for future:

- [ ] **Runtime model registry** - Models register/unregister dynamically, queryable via Trustfall
- [ ] **Runtime policies** - Timeouts/limits mutable at runtime via tools
- [ ] **Bootstrap Lua script** - `init.lua` for programmable startup
- [ ] **Presets as artifacts** - Save/load runtime state snapshots
- [ ] **Trustfall schema for config** - Query `{ Policy { name value } }`

## Files to Create

| File | Purpose |
|------|---------|
| `crates/hoot-config/Cargo.toml` | Minimal deps: serde, toml, directories, thiserror |
| `crates/hoot-config/src/lib.rs` | `HootConfig::load()`, re-exports |
| `crates/hoot-config/src/infra.rs` | `PathsConfig`, `BindConfig`, `TelemetryConfig` |
| `crates/hoot-config/src/bootstrap.rs` | `ModelsConfig`, `ConnectionsConfig`, `MediaConfig`, `DefaultsConfig` |
| `crates/hoot-config/src/loader.rs` | File discovery, env overlay, path expansion |
| `crates/hootenanny/src/api/tools/config.rs` | `config_get` tool (ZMQ native, MCP via holler) |

## Files to Modify

| File | Change |
|------|--------|
| `Cargo.toml` (workspace) | Add `hoot-config` to members |
| `crates/cas/src/config.rs` | Use `hoot_config::InfraConfig` |
| `crates/hootenanny/Cargo.toml` | Add `hoot-config` dependency |
| `crates/hootenanny/src/main.rs` | Load config, use for CLI defaults |
| `crates/hootenanny/src/api/service.rs` | Wire up `config_get` tool dispatch |
| `crates/holler/Cargo.toml` | Add `hoot-config` dependency |
| `crates/holler/src/main.rs` | Load config for bind address |
| `crates/luanette/Cargo.toml` | Add `hoot-config` dependency |
| `crates/luanette/src/main.rs` | Load config for bind address |
| `crates/chaosgarden/Cargo.toml` | Add `hoot-config` dependency |
| `crates/chaosgarden/src/ipc.rs` | Use `config.infra.paths.socket_dir` |
| `crates/hootenanny/src/mcp_tools/local_models.rs` | Use `config.bootstrap.models` |

---

*Plan created: 2024-12-13*
*Revised: 2024-12-13 - Minimal infrastructure + bootstrap model*
*Status: 📋 Ready for implementation*
