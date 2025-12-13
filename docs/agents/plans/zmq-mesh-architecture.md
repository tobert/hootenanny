# ZMQ Mesh Architecture Plan

**Date**: 2025-12-12
**Session**: Systemd bringup + Luanette ZMQ refactor
**Authors**: Claude, atobey

## Executive Summary

Evolve hooteproto from pure MsgPack envelopes to a hybrid frame-based protocol inspired by MDP (Majordomo Protocol). This enables:
- Routing without deserialization
- Efficient heartbeats
- Native binary payloads (no base64)
- Service discovery and registration

## How HOOT01 Differs from MDP

Our protocol is *inspired by* MDP but simplified for our specific use case:

| Aspect | MDP (RFC 7/18) | HOOT01 |
|--------|----------------|--------|
| **Protocol ID** | `MDPC02` (client), `MDPW02` (worker) | `HOOT01` (unified) |
| **Commands** | Ready, Request, Partial, Final, Heartbeat, Disconnect | Ready, Request, Reply, Heartbeat, Disconnect |
| **Streaming** | Partial* + Final for multi-part responses | Single Reply only (no streaming) |
| **Body encoding** | Opaque binary (app decides) | Explicit ContentType field (2 bytes) |
| **Broker model** | Generic service router with worker pools | Hootenanny as central hub with proxies |
| **Client envelope** | Explicit empty delimiter frame | Handled via `from_multipart_with_identity` |
| **Request ID** | Not in spec (app-level) | Required UUID in frame |
| **Traceparent** | Not in spec | Built-in frame slot |

**Why these differences:**

1. **No Partial/Final** - Our tools return single responses. Streaming can be added later if needed.

2. **Unified protocol ID** - We don't distinguish client/worker roles at the wire level. Hootenanny acts as both.

3. **Request ID in frame** - Enables request/response correlation without deserializing the body.

4. **Traceparent slot** - First-class observability. Every request carries trace context.

5. **Explicit ContentType field** - 2-byte field declares body format (MsgPack, RawBinary, JSON). No magic byte sniffing.

6. **Hub not broker** - MDP is a generic pattern. We have a specific architecture where hootenanny proxies to specialized backends.

**Why not just use MDP?** We considered it, but:
- We'd lose traceparent (critical for observability)
- We'd need to fork/wrap MDP libraries anyway to add our features
- Our frame format is simpler to implement (~200 LOC per language)
- We control all clients, so wire compatibility with off-the-shelf MDP tools isn't needed

## Future: Python HOOT01 Client

Python is where the ML models live (Orpheus, BeatThis, MusicGen, etc.). A Python HOOT01 client
would let inference workers connect directly to the mesh:

```python
# Example: orpheus as a HOOT01 worker
from hooteproto import HootClient, Command

client = HootClient("tcp://localhost:5580", service="orpheus")
client.send_ready(tools=["orpheus_generate", "orpheus_continue"])

for frame in client.listen():
    if frame.command == Command.REQUEST:
        result = run_inference(frame.payload)
        client.reply(frame.request_id, result)
```

**Scope**: ~200 lines over pyzmq. Frame encode/decode + heartbeat handling.

**Benefits**:
- ML workers as first-class mesh participants (not subprocess bridges)
- Unified tracing through Python inference
- Hot-reload inference workers without restarting hootenanny

**Not in current plan** - add after Phase 6 stabilizes the Rust implementation.

## Implementation Roadmap

| Phase | Crate | New Files | Est. LOC | Dependencies |
|-------|-------|-----------|----------|--------------|
| 1. Frame Protocol | hooteproto | `frame.rs` | ~150 | bytes |
| 2. Heartbeating | holler, hootenanny | `heartbeat.rs` | ~250 | Phase 1 |
| 3. Backend Response | hootenanny, luanette | (modify existing) | ~100 | Phase 1 |
| 4. Backend Proxies | hootenanny | `chaosgarden_client.rs`, `luanette_client.rs` | ~250 | Phase 1 |
| 5. Holler Simplification | holler | (modify existing) | ~-100 | Phase 2-4 |
| 6. Tool Refresh | holler | (modify existing) | ~50 | Phase 5 |

**Phase order**: 1 → (2, 3, 4 parallel) → 5 → 6

**Net effect**: Holler gets simpler (removes multi-backend logic), hootenanny gains proxy.

## Progress Tracker

Update this section as work completes. Check off items, note blockers, add commits.

### Phase 1: Frame Protocol ✅
- [x] Create `crates/hooteproto/src/frame.rs`
- [x] `Command` enum (u16, big-endian)
- [x] `ContentType` enum (u16, big-endian)
- [x] `HootFrame` struct (7 frames: proto, cmd, ctype, reqid, service, trace, body)
- [x] `ReadyPayload` for service registration
- [x] `from_frames` / `to_frames` implementations
- [x] `from_frames_with_identity` for ROUTER sockets
- [x] Unit tests pass: `cargo test -p hooteproto` (27 tests)
- [ ] Commit: _________________

### Phase 2: Heartbeating (holler + hootenanny) ✅
- [x] Create `crates/holler/src/heartbeat.rs`
- [x] `HeartbeatConfig` with interval/timeout/max_failures
- [x] Health tracking fields in `Backend` (via `HealthTracker`)
- [x] `BackendState` enum (Connecting, Ready, Busy, Dead)
- [x] Exponential backoff configuration (reconnect_initial, reconnect_max)
- [x] `Backend::send_heartbeat()` using HOOT01 frames
- [x] Spawn heartbeat tasks in `serve.rs` for connected backends
- [x] Update `/health` endpoint with backend states
- [ ] Socket close/reopen on Dead state (future: reconnection logic)
- [ ] **Bidirectional**: Hootenanny → Holler heartbeats (future)
- [ ] Commit: _________________

### Phase 3: Backend Response (hootenanny) ✅
- [x] Detect frame protocol in `hooteproto_server.rs` (scans for HOOT01)
- [x] Handle `Command::Heartbeat` → immediate reply
- [x] Handle `Command::Request` → dispatch → `Command::Reply`
- [x] Handle `Command::Ready` (log worker registration)
- [x] Handle `Command::Disconnect` (log worker disconnect)
- [x] Legacy envelope fallback preserved for backward compatibility
- [ ] Update luanette similarly (separate task)
- [ ] Commit: _________________

### Phase 4: Backend Proxies (hootenanny)
- [ ] Create `crates/hootenanny/src/chaosgarden_client.rs`
- [ ] IPC connection to chaosgarden on startup
- [ ] Route `graph_*` and `play` through chaosgarden proxy
- [ ] Create `crates/hootenanny/src/luanette_client.rs`
- [ ] ZMQ connection to luanette on startup
- [ ] Route luanette tools through luanette proxy
- [ ] Propagate traceparent for unified tracing
- [ ] Add chaosgarden + luanette health to `/health`
- [ ] Commit: _________________

### Phase 5: Holler Simplification
- [ ] Remove luanette backend config
- [ ] Remove chaosgarden backend config
- [ ] Simplify `AppState` to single backend
- [ ] Remove multi-backend dispatch logic
- [ ] Update systemd units if needed
- [ ] Verify: single backend in `/health`
- [ ] Commit: _________________

### Phase 6: Tool Refresh
- [ ] Detect Dead → Ready transition
- [ ] Call `refresh_tools()` on recovery
- [ ] Update tool registry
- [ ] Integration test: kill/restart hootenanny
- [ ] Commit: _________________

### Blockers / Notes
_Add issues encountered during implementation here._

### Document Structure Note
Design Decisions section is near the end (after References) for historical reasons.
Consider consolidating with Verification Notes in a future cleanup pass.

## Current State

```
MCP Clients ──HTTP──► Holler (8080) ──ZMQ DEALER──► Backends
                           │
              ┌────────────┼────────────┐
              ▼            ▼            ▼
         Luanette    Hootenanny   Chaosgarden
          :5570       :5580         (IPC)
              │
              └──► Hootenanny (direct ZMQ)
```

## Target State (Architecture)

```
MCP Clients ──HTTP──► Holler (8080) ──ZMQ──► Hootenanny (:5580)
                                                   │
                                                   ├──► Chaosgarden (IPC)
                                                   │
                                                   ├──► Luanette (:5570)
                                                   │
Lua Scripts ──► Luanette ──────────ZMQ────────────►┘
```

**Key changes**:
- Holler connects to ONE backend: hootenanny
- Hootenanny proxies to chaosgarden (IPC) AND luanette (ZMQ)
- Luanette is both a proxy target (for MCP clients) AND a peer client (for Lua scripts)
- All tool calls flow through hootenanny → unified tracing
- Bidirectional heartbeating: Holler ↔ Hootenanny (both can detect peer failure)

**Current wire format** (single MsgPack frame):
```rust
// crates/hooteproto/src/lib.rs
pub struct Envelope {
    pub id: Uuid,
    pub traceparent: Option<String>,
    pub payload: Payload,  // 50+ variants
}
```

## Wire Format

**New wire format** (multipart ZMQ message, fixed-width fields first):
```
Frame 0: Protocol version    "HOOT01" (6 bytes)
Frame 1: Command             2 bytes (big-endian u16)
Frame 2: Content-Type        2 bytes (big-endian u16)
Frame 3: Request ID          16 bytes (UUID)
─── fixed-width above / variable-width below ───
Frame 4: Service name        UTF-8 string (variable)
Frame 5: Traceparent         UTF-8 string (variable, or empty)
Frame 6: Body                bytes (interpretation per Content-Type)
```

**Command values** (2 bytes, room to grow):
```
0x0001 = Ready       (worker → broker)
0x0002 = Request     (broker → worker)
0x0003 = Reply       (worker → broker)
0x0004 = Heartbeat   (bidirectional)
0x0005 = Disconnect  (graceful shutdown)
```

**Content-Type values** (2 bytes):
```
0x0000 = Empty       (heartbeats, etc.)
0x0001 = MsgPack     (structured Payload)
0x0002 = RawBinary   (MIDI, audio, etc.)
0x0003 = JSON        (future, for debugging)
```

**ROUTER socket note**: When using ROUTER sockets, ZMQ prepends identity frame(s).
Our parsing handles this by detecting `HOOT01` to find frame 0:
```
[Identity...] | HOOT01 | Cmd | CType | ReqID | Service | Trace | Body
                ↑ scan for this
```

## Data Structures

### hooteproto/src/frame.rs (new file)

```rust
use bytes::Bytes;
use uuid::Uuid;

/// Protocol version - bump on breaking changes
pub const PROTOCOL_VERSION: &[u8] = b"HOOT01";

/// Command types (2 bytes, big-endian)
/// Reference: inspired by MDP https://rfc.zeromq.org/spec/7/
#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// Worker announces availability (worker → broker)
    Ready = 0x0001,
    /// Request from client or broker (broker → worker)
    Request = 0x0002,
    /// Reply from worker (worker → broker)
    Reply = 0x0003,
    /// Bidirectional liveness check
    Heartbeat = 0x0004,
    /// Graceful shutdown
    Disconnect = 0x0005,
}

/// Content type for body interpretation (2 bytes, big-endian)
#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentType {
    /// No body (heartbeats, simple acks)
    Empty = 0x0000,
    /// MsgPack-encoded Payload
    MsgPack = 0x0001,
    /// Raw binary (MIDI, audio, etc.)
    RawBinary = 0x0002,
    /// JSON (for debugging, future)
    Json = 0x0003,
}

/// A parsed multipart ZMQ message
#[derive(Debug, Clone)]
pub struct HootFrame {
    pub command: Command,
    pub content_type: ContentType,
    pub request_id: Uuid,
    pub service: String,
    pub traceparent: Option<String>,
    pub body: Bytes,  // interpret according to content_type
}

/// Payload for Ready command - worker announces capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadyPayload {
    pub protocol: String,           // "HOOT01"
    pub tools: Vec<String>,         // ["orpheus_generate", "cas_store", ...]
    pub accepts_binary: bool,       // Can handle ContentType::RawBinary
}

/// Errors during frame parsing
#[derive(Debug, thiserror::Error)]
pub enum FrameError {
    #[error("Invalid protocol version: expected HOOT01")]
    InvalidProtocol,
    #[error("Missing frame {0}")]
    MissingFrame(&'static str),
    #[error("Invalid command: {0:#06x}")]
    InvalidCommand(u16),
    #[error("Invalid content type: {0:#06x}")]
    InvalidContentType(u16),
    #[error("Invalid UTF-8 in {0}")]
    InvalidUtf8(&'static str),
    #[error("MsgPack decode error: {0}")]
    MsgPackError(#[from] rmp_serde::decode::Error),
    #[error("Content type mismatch: expected {expected:?}, got {actual:?}")]
    ContentTypeMismatch { expected: ContentType, actual: ContentType },
}
```

### Key Function Signatures

```rust
// hooteproto/src/frame.rs

impl HootFrame {
    /// Parse a multipart ZMQ message into a HootFrame
    /// Scans for HOOT01 to find protocol frame (handles ROUTER identity prefix)
    pub fn from_multipart(msg: &zeromq::ZmqMessage) -> Result<Self, FrameError>;

    /// Parse multipart, returning identity frames separately (for ROUTER reply)
    /// Returns (identity_frames, parsed_frame)
    pub fn from_multipart_with_identity(msg: &zeromq::ZmqMessage)
        -> Result<(Vec<Bytes>, Self), FrameError>;

    /// Serialize to multipart ZMQ message (7 frames)
    pub fn to_multipart(&self) -> zeromq::ZmqMessage;

    /// Serialize with identity prefix (for ROUTER socket replies)
    pub fn to_multipart_with_identity(&self, identity: &[Bytes]) -> zeromq::ZmqMessage;

    /// Create a heartbeat frame (ContentType::Empty, no body)
    pub fn heartbeat(service: &str) -> Self;

    /// Create a ready frame (worker registration, ContentType::MsgPack)
    pub fn ready(service: &str, capabilities: &ReadyPayload) -> Result<Self, rmp_serde::encode::Error>;

    /// Create a request frame (ContentType::MsgPack)
    pub fn request(service: &str, payload: &Payload) -> Result<Self, rmp_serde::encode::Error>;

    /// Create a request frame with raw binary body (ContentType::RawBinary)
    pub fn request_binary(service: &str, request_id: Uuid, data: Bytes) -> Self;

    /// Create a reply frame (ContentType::MsgPack)
    pub fn reply(request_id: Uuid, payload: &Payload) -> Result<Self, rmp_serde::encode::Error>;

    /// Create a reply frame with raw binary body (ContentType::RawBinary)
    pub fn reply_binary(request_id: Uuid, data: Bytes) -> Self;

    /// Extract typed Payload from MsgPack body (checks content_type)
    pub fn payload<T: DeserializeOwned>(&self) -> Result<T, FrameError>;

    /// Get raw body bytes (for RawBinary content type)
    pub fn raw_body(&self) -> Result<&Bytes, FrameError>;
}

impl Command {
    pub fn from_u16(v: u16) -> Result<Self, FrameError>;
    pub fn to_u16(self) -> u16;
}

impl ContentType {
    pub fn from_u16(v: u16) -> Result<Self, FrameError>;
    pub fn to_u16(self) -> u16;
}
```

### Backend State (holler/src/backend.rs updates)

```rust
/// Enhanced backend with health tracking
pub struct Backend {
    pub config: BackendConfig,
    socket: RwLock<DealerSocket>,

    // New fields for heartbeating
    last_heartbeat_sent: RwLock<Instant>,
    last_heartbeat_recv: RwLock<Option<Instant>>,
    consecutive_failures: AtomicU32,
    state: AtomicU8,  // BackendState enum
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendState {
    Connecting = 0,
    Ready = 1,
    Busy = 2,      // Processing request
    Dead = 3,      // Failed heartbeats
}

impl Backend {
    /// Check if backend is alive based on heartbeat state
    /// Reference: Paranoid Pirate pattern - dead after N failures
    pub fn is_alive(&self) -> bool;

    /// Send heartbeat, track timing
    pub async fn send_heartbeat(&self) -> Result<()>;

    /// Record successful heartbeat response
    pub fn record_heartbeat_response(&self);

    /// Record failed heartbeat (timeout or error)
    pub fn record_heartbeat_failure(&self);

    /// Re-fetch tool list after reconnection
    pub async fn refresh_tools(&self) -> Result<Vec<ToolInfo>>;
}
```

### Heartbeat Task (holler/src/heartbeat.rs - new file)

```rust
/// Configuration for heartbeat behavior
/// Reference: MDP spec recommends 2500ms interval, we use 5000ms
pub struct HeartbeatConfig {
    pub interval: Duration,      // 5 seconds
    pub timeout: Duration,       // 2 seconds
    pub max_failures: u32,       // 3 failures = dead
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(5),
            timeout: Duration::from_secs(2),
            max_failures: 3,
        }
    }
}

/// Spawn heartbeat task for a backend
/// Returns handle to cancel on shutdown
pub fn spawn_heartbeat_task(
    backend: Arc<Backend>,
    config: HeartbeatConfig,
    on_state_change: impl Fn(BackendState) + Send + 'static,
) -> tokio::task::JoinHandle<()>;
```

## Implementation Phases

### Phase 1: Frame Protocol (hooteproto)

**Files to create/modify**:
- `crates/hooteproto/src/frame.rs` - New frame types
- `crates/hooteproto/src/lib.rs` - Export frame module
- `crates/hooteproto/Cargo.toml` - Add `bytes` dep if needed

**Tests**:
```rust
#[cfg(test)]
mod tests {
    #[test]
    fn heartbeat_roundtrip() {
        let frame = HootFrame::heartbeat("hootenanny");
        let msg = frame.to_multipart();
        let parsed = HootFrame::from_multipart(&msg).unwrap();
        assert_eq!(parsed.command, Command::Heartbeat);
        assert_eq!(parsed.service, "hootenanny");
    }

    #[test]
    fn request_with_payload_roundtrip() {
        let payload = Payload::Ping;
        let frame = HootFrame::request("hootenanny", &payload).unwrap();
        let msg = frame.to_multipart();
        let parsed = HootFrame::from_multipart(&msg).unwrap();
        let recovered: Payload = parsed.payload().unwrap();
        assert_eq!(recovered, Payload::Ping);
    }

    #[test]
    fn raw_binary_body() {
        let midi_bytes = include_bytes!("../../test_data/simple.mid");
        let frame = HootFrame::reply_binary(Uuid::new_v4(), Bytes::from_static(midi_bytes));
        let msg = frame.to_multipart();
        assert_eq!(msg.len(), 7); // All 7 frames present

        let parsed = HootFrame::from_multipart(&msg).unwrap();
        assert_eq!(parsed.content_type, ContentType::RawBinary);
        assert_eq!(parsed.raw_body().unwrap().len(), midi_bytes.len());
    }

    #[test]
    fn content_type_explicit() {
        // MsgPack frame
        let frame = HootFrame::request("hootenanny", &Payload::Ping).unwrap();
        assert_eq!(frame.content_type, ContentType::MsgPack);

        // Empty frame (heartbeat)
        let hb = HootFrame::heartbeat("hootenanny");
        assert_eq!(hb.content_type, ContentType::Empty);
        assert!(hb.body.is_empty());

        // Binary frame
        let bin = HootFrame::reply_binary(Uuid::new_v4(), Bytes::from_static(b"MIDI"));
        assert_eq!(bin.content_type, ContentType::RawBinary);
    }

    #[test]
    fn ready_with_capabilities() {
        let caps = ReadyPayload {
            protocol: "HOOT01".into(),
            tools: vec!["orpheus_generate".into(), "cas_store".into()],
            accepts_binary: true,
        };
        let frame = HootFrame::ready("hootenanny", &caps).unwrap();
        assert_eq!(frame.content_type, ContentType::MsgPack);

        let msg = frame.to_multipart();
        let parsed = HootFrame::from_multipart(&msg).unwrap();

        let recovered: ReadyPayload = parsed.payload().unwrap();
        assert_eq!(recovered.tools.len(), 2);
    }

    #[test]
    fn command_and_content_type_encoding() {
        assert_eq!(Command::Ready.to_u16(), 0x0001);
        assert_eq!(Command::from_u16(0x0004).unwrap(), Command::Heartbeat);
        assert!(Command::from_u16(0xFFFF).is_err());

        assert_eq!(ContentType::MsgPack.to_u16(), 0x0001);
        assert_eq!(ContentType::from_u16(0x0002).unwrap(), ContentType::RawBinary);
        assert!(ContentType::from_u16(0xFFFF).is_err());
    }
}
```

### Phase 2: Heartbeating (holler + hootenanny)

**Files to create/modify**:
- `crates/holler/src/heartbeat.rs` - New heartbeat task
- `crates/holler/src/backend.rs` - Add health tracking
- `crates/holler/src/serve.rs` - Spawn heartbeat tasks
- `crates/holler/src/health.rs` - Update health endpoint
- `crates/hootenanny/src/zmq/hooteproto_server.rs` - Bidirectional heartbeat (track holler liveness)

**Heartbeat loop pseudocode**:
```rust
// Reference: ZeroMQ Guide Ch4 - Paranoid Pirate pattern
loop {
    tokio::select! {
        _ = interval.tick() => {
            // Send heartbeat
            let frame = HootFrame::heartbeat(&backend.config.name);
            match timeout(config.timeout, backend.send_frame(frame)).await {
                Ok(Ok(_)) => {
                    // Wait for response
                    match timeout(config.timeout, backend.recv_frame()).await {
                        Ok(Ok(response)) if response.command == Command::Heartbeat => {
                            backend.record_heartbeat_response();
                        }
                        _ => backend.record_heartbeat_failure(),
                    }
                }
                _ => backend.record_heartbeat_failure(),
            }

            // Check if dead
            if backend.consecutive_failures.load(Ordering::Relaxed) >= config.max_failures {
                backend.set_state(BackendState::Dead);
                on_state_change(BackendState::Dead);
            }
        }
        _ = shutdown.recv() => break,
    }
}
```

**Integration test**:
```rust
#[tokio::test]
async fn backend_dies_after_missed_heartbeats() {
    // Start hootenanny
    let hoot = start_hootenanny().await;

    // Connect holler with fast heartbeat (1s interval, 500ms timeout)
    let backend = Backend::connect(...).await.unwrap();
    let config = HeartbeatConfig {
        interval: Duration::from_secs(1),
        timeout: Duration::from_millis(500),
        max_failures: 2,
    };

    let (tx, mut rx) = tokio::sync::mpsc::channel(1);
    let _task = spawn_heartbeat_task(backend.clone(), config, move |state| {
        let _ = tx.try_send(state);
    });

    // Kill hootenanny
    hoot.kill().await;

    // Should receive Dead state within ~3 seconds
    let state = timeout(Duration::from_secs(5), rx.recv()).await.unwrap().unwrap();
    assert_eq!(state, BackendState::Dead);
}
```

### Phase 3: Backend Response to Heartbeats

**Files to modify**:
- `crates/hootenanny/src/zmq/hooteproto_server.rs` - Handle frame protocol
- `crates/luanette/src/zmq_server.rs` - Handle frame protocol

**Hootenanny dispatch update**:
```rust
async fn handle_message(&self, socket: &mut RouterSocket, msg: ZmqMessage) -> Result<()> {
    // Try new frame protocol first (scans for HOOT01, extracts identity)
    if let Ok((identity, frame)) = HootFrame::from_multipart_with_identity(&msg) {
        return self.handle_frame(socket, &identity, frame).await;
    }

    // Fall back to legacy MsgPack envelope (backward compat)
    // Legacy format: [identity, msgpack_envelope]
    let identity = msg.get(0).context("Missing identity")?;
    let envelope: Envelope = rmp_serde::from_slice(msg.get(1)?)?;
    // ... existing dispatch ...
}

async fn handle_frame(&self, socket: &mut RouterSocket, identity: &[Bytes], frame: HootFrame) -> Result<()> {
    match frame.command {
        Command::Heartbeat => {
            // Respond immediately with heartbeat
            let response = HootFrame::heartbeat(&self.service_name);
            let msg = response.to_multipart_with_identity(identity);
            socket.send(msg).await?;
            Ok(())
        }
        Command::Request => {
            // Dispatch to tool handler
            let payload: Payload = frame.payload()?;
            let result = self.dispatch(payload).await;
            let response = HootFrame::reply(frame.request_id, &result)?;
            let msg = response.to_multipart_with_identity(identity);
            socket.send(msg).await?;
            Ok(())
        }
        Command::Ready | Command::Disconnect => {
            // Log but no action needed for server receiving these
            tracing::debug!("Received {:?} from client", frame.command);
            Ok(())
        }
        Command::Reply => {
            // Unexpected at server - log warning
            tracing::warn!("Unexpected Reply command received at server");
            Ok(())
        }
    }
}
```

### Phase 4: Backend Proxies (hootenanny)

**Files to create/modify**:
- `crates/hootenanny/src/chaosgarden_client.rs` - ZMQ client to chaosgarden (IPC)
- `crates/hootenanny/src/luanette_client.rs` - ZMQ client to luanette (TCP)
- `crates/hootenanny/src/api/dispatch.rs` - Route tools to appropriate proxy

**Proxy client pattern** (shared by both):
```rust
/// Client for proxying requests to a backend
pub struct ProxyClient {
    socket: DealerSocket,
    service_name: String,
}

impl ProxyClient {
    /// Connect to backend
    pub async fn connect(endpoint: &str, service_name: &str) -> Result<Self>;

    /// Forward a request and await response
    /// Propagates traceparent for unified tracing
    pub async fn forward(&self, frame: HootFrame) -> Result<HootFrame>;

    /// Check if backend is alive (for health endpoint)
    pub async fn health_check(&self) -> bool;
}

// Concrete types for clarity in dispatch
pub type ChaosgardenClient = ProxyClient;  // IPC
pub type LuanetteClient = ProxyClient;     // TCP :5570
```

**Dispatch routing**:
```rust
// In hootenanny dispatch
match &payload {
    // Chaosgarden tools (audio routing)
    Payload::GraphBind { .. } |
    Payload::GraphConnect { .. } |
    Payload::GraphQuery { .. } |
    Payload::Play { .. } => {
        self.chaosgarden.forward(frame).await
    }

    // Luanette tools (Lua scripting)
    Payload::LuaEval { .. } |
    Payload::LuaCall { .. } => {
        self.luanette.forward(frame).await
    }

    // Local tools (handled by hootenanny directly)
    _ => self.dispatch_local(payload).await
}
```

**Tracing integration**:
```rust
// Create child span for proxy call
let span = tracing::info_span!(
    "proxy_forward",
    backend = %backend_name,
    tool = %tool_name,
    otel.kind = "client"
);
let _guard = span.enter();
// traceparent propagated in frame
```

### Phase 5: Holler Simplification

**Files to modify**:
- `crates/holler/src/backend.rs` - Remove multi-backend Vec, single Backend
- `crates/holler/src/serve.rs` - Simplify dispatch, remove backend routing
- `crates/holler/src/config.rs` - Remove luanette/chaosgarden backend configs
- `crates/holler/src/health.rs` - Simplify to single backend health

**Before** (current):
```rust
pub struct AppState {
    backends: Vec<Arc<Backend>>,  // hootenanny, luanette, chaosgarden
    tool_registry: ToolRegistry,  // merged from all backends
}
```

**After**:
```rust
pub struct AppState {
    hootenanny: Arc<Backend>,     // single backend
    // tool_registry populated from hootenanny's Ready payload
}
```

**Code removal**:
- Backend selection logic in dispatch
- Per-backend tool filtering
- Multi-backend heartbeat management
- Luanette and chaosgarden connection code

### Phase 6: Tool Refresh on Reconnection

**Logic in holler**:
```rust
// In heartbeat task, when backend recovers from Dead:
if old_state == BackendState::Dead && new_state == BackendState::Ready {
    // Backend came back! Refresh tools.
    match backend.refresh_tools().await {
        Ok(tools) => {
            info!("Refreshed {} tools from {}", tools.len(), backend.config.name);
            // Update shared tool registry
            tool_registry.update_backend(&backend.config.name, tools);
        }
        Err(e) => {
            warn!("Failed to refresh tools from {}: {}", backend.config.name, e);
        }
    }
}
```

## Migration Strategy

### Backward Compatibility

1. **Detect protocol by first frame**:
   - If starts with `HOOT01` → new frame protocol
   - Otherwise → legacy MsgPack envelope

2. **Gradual rollout**:
   - Phase 1: Add frame parsing, keep envelope sending
   - Phase 2: holler sends frames, backends accept both
   - Phase 3: All services use frames
   - Phase 4: Remove legacy envelope code

### Version Negotiation (future)

```rust
// On connection, exchange capabilities
Command::Ready payload could include:
{
    "protocol_versions": ["HOOT01"],
    "capabilities": ["heartbeat", "raw_binary"],
    "tools": ["orpheus_generate", "cas_store", ...]
}
```

## Testing Strategy

### Unit Tests (per module)
- `frame.rs`: Roundtrip all command types, error cases
- `backend.rs`: State transitions, health calculations
- `heartbeat.rs`: Timing behavior with mock backend

### Integration Tests
- `tests/heartbeat_integration.rs`: Real services, kill/restart
- `tests/frame_protocol.rs`: End-to-end frame exchange
- `tests/backward_compat.rs`: Mix of old/new clients
- `tests/chaosgarden_proxy.rs`: graph_bind through hootenanny, verify trace propagation
- `tests/luanette_proxy.rs`: luanette tool through hootenanny, verify trace propagation

### Manual Testing Checklist
- [ ] Start holler + hootenanny + chaosgarden + luanette
- [ ] `curl localhost:8080/health` shows hootenanny:Ready, chaosgarden:Ready, luanette:Ready
- [ ] Kill hootenanny, verify holler health updates within 15s
- [ ] Restart hootenanny, verify tools re-discovered
- [ ] Kill chaosgarden, verify hootenanny reports it unhealthy
- [ ] Kill luanette, verify hootenanny reports it unhealthy
- [ ] Call `graph_bind` via MCP, verify trace shows: holler → hootenanny → chaosgarden
- [ ] Call luanette tool via MCP, verify trace shows: holler → hootenanny → luanette
- [ ] Send MIDI through pipeline, verify no base64 bloat
- [ ] Run under load, verify heartbeats don't starve

## Configuration

### Environment Variables
```bash
# Heartbeat tuning (optional, sensible defaults)
HOOT_HEARTBEAT_INTERVAL_MS=5000
HOOT_HEARTBEAT_TIMEOUT_MS=2000
HOOT_HEARTBEAT_MAX_FAILURES=3
```

### Systemd Updates
No changes needed - frame protocol is wire-level only.

## Verification Notes (from RFC/zguide research)

Research performed 2025-12-13 from:
- `~/src/rfc/content/docs/rfcs/7/README.md` (MDP 0.1)
- `~/src/rfc/content/docs/rfcs/18/README.md` (MDP 0.2)
- `~/src/rfc/content/docs/rfcs/6/README.md` (PPP - Paranoid Pirate)
- `~/src/zguide/site/content/docs/chapter4.md`

### Critical Findings

**1. Socket close/reopen is REQUIRED on disconnect (not just reconnect)**

From RFC 7 line 188:
> "When the worker receives DISCONNECT it must send no further commands to the broker;
> it MUST close its socket, and reconnect to the broker on a new socket."

From zguide chapter 4:
> "You might wonder why the worker API is manually closing its socket and opening a new one,
> when ZeroMQ will automatically reconnect a socket if the peer disappears and comes back.
> [...] Although ZeroMQ will automatically reconnect workers if the broker dies and comes back up,
> this isn't sufficient to re-register the workers with the broker."

**Action**: Our heartbeat failure handler MUST close and recreate the socket, not rely on
ZMQ's automatic reconnection. This is how we trigger re-registration via `Command::Ready`.

**2. Heartbeat interval must match on both sides**

From RFC 7 Known Weaknesses:
> "The heartbeat rate must be set to similar values in broker and worker, or false
> disconnections will occur."

**Action**: Our 5s interval is fine, but we should document it clearly. Future: negotiate
via Ready payload.

**3. Any command acts as heartbeat**

From RFC 7 line 200:
> "Any received command except DISCONNECT acts as a heartbeat. Peers SHOULD NOT send
> HEARTBEAT commands while also sending other commands."

**Action**: In our heartbeat logic, reset liveness counter on ANY received message, not
just Command::Heartbeat. This is already implied in our pseudocode but worth emphasizing.

**4. DEALER socket is correct for workers**

From zguide:
> "For the Paranoid Pirate worker, we'll switch to a DEALER socket. This has the advantage
> of letting us send and receive messages at any time."

**Action**: We're already using DEALER for clients (holler→hootenanny). Correct.

**5. Reconnection with exponential backoff**

From zguide heartbeat code (lines 336-365):
```
#define INTERVAL_INIT       1000    //  Initial reconnect
#define INTERVAL_MAX       32000    //  After exponential backoff
```

**Action**: Add exponential backoff to our reconnection logic. Sleep before reconnecting,
double interval each time up to 32s max.

### Socket Options

**LINGER** - Set to 0 for clean shutdown (don't block waiting for pending messages):
```rust
// zeromq crate: check if SocketOptions supports this
let mut opts = SocketOptions::default();
// opts.linger = Some(Duration::ZERO);  // if available
```

**IMMEDIATE** - Only queue to completed connections. Important for avoiding message
loss to not-yet-connected peers. Check if zeromq crate supports this.

**RECONNECT_IVL / RECONNECT_IVL_MAX** - ZMQ's automatic reconnection intervals.
Since we're doing manual reconnection on failure, these matter less.

**Buffer tuning (SNDHWM/RCVHWM)**: DEFER. Defaults (1000 messages) are fine for our
KB/s throughput. Only tune if we see queue overflow warnings.

### What We Got Right

✓ DEALER/ROUTER socket pairing
✓ Multipart message frames with protocol version prefix
✓ Heartbeat interval (5s) with 3-failure threshold
✓ Ready command for service registration
✓ Separate heartbeat from request handling

### What Needs Adjustment

1. **Add socket close/reopen to Backend recovery**:
```rust
// In heartbeat task, when transitioning to Dead:
async fn handle_dead_backend(&self) {
    // Close existing socket
    let old_socket = self.socket.write().await.take();
    drop(old_socket);  // Explicit close

    // Wait with exponential backoff
    tokio::time::sleep(self.reconnect_interval).await;
    self.reconnect_interval = (self.reconnect_interval * 2).min(Duration::from_secs(32));

    // Create new socket and reconnect
    let mut new_socket = DealerSocket::new();
    new_socket.connect(&self.config.endpoint).await?;

    // Send Ready to re-register
    let ready = HootFrame::ready(&self.config.name, &self.capabilities)?;
    new_socket.send(ready.to_multipart()).await?;

    *self.socket.write().await = Some(new_socket);
    self.set_state(BackendState::Connecting);
}
```

2. **Reset liveness on ANY message** (not just heartbeat):
```rust
// In message receive loop
match frame.command {
    Command::Heartbeat | Command::Reply | Command::Request => {
        backend.reset_liveness();  // Any command = alive
    }
    Command::Disconnect => {
        // Handle disconnect, don't reset liveness
    }
}
```

3. **Check zeromq crate for socket options**:
```bash
cargo doc -p zeromq --open
# Look for SocketOptions, set_linger, etc.
```

## References

### ZeroMQ Guide (authoritative)
- [Ch3: Advanced Request-Reply](https://zguide.zeromq.org/docs/chapter3/) - ROUTER/DEALER patterns
- [Ch4: Reliable Request-Reply](https://zguide.zeromq.org/docs/chapter4/) - Paranoid Pirate, heartbeating
- [Ch5: Advanced Pub-Sub](https://zguide.zeromq.org/docs/chapter5/) - If we add PUB/SUB events

### Protocol Specs
- [RFC 7: MDP](https://rfc.zeromq.org/spec/7/) - Majordomo Protocol (our inspiration)
- [RFC 18: MDP/Worker](https://rfc.zeromq.org/spec/18/) - Worker side of MDP
- [RFC 36: ZRE](https://rfc.zeromq.org/spec/36/) - Peer discovery (future reference)

### Rust Crates
- [zeromq](https://docs.rs/zeromq) - Our async ZMQ (pure Rust)
- [tmq](https://docs.rs/tmq) - Alternative if zeromq issues persist
- [rmp-serde](https://docs.rs/rmp-serde) - MsgPack serialization

## Design Decisions

These were open questions; recommendations are now locked in.

### 1. Chaosgarden Routing: **Proxy Through Hootenanny** ✓

Holler connects ONLY to hootenanny. Hootenanny proxies to chaosgarden via IPC.

**Rationale**:
- **Negligible latency**: We're passing control messages (KB/s), not audio samples.
  Extra ZMQ hop is microseconds. Audio flows through PipeWire directly.
- **Simplified holler**: One backend connection, one heartbeat task, one tool source.
- **Protection**: Chaosgarden only accepts IPC from hootenanny. No network exposure.
- **Unified tracing**: All requests flow through hootenanny. Complete spans for
  the entire music domain. Essential for debugging and observability.

**Implementation**: Hootenanny dispatches `graph_*` and `play` tools to chaosgarden
over IPC, same frame protocol.

### 2. Binary Body Encoding: **Explicit ContentType Field** ✓

A 2-byte ContentType field (Frame 2) explicitly declares body interpretation:

```rust
#[repr(u16)]
pub enum ContentType {
    Empty = 0x0000,      // heartbeats, acks
    MsgPack = 0x0001,    // structured Payload
    RawBinary = 0x0002,  // MIDI, audio, etc.
    Json = 0x0003,       // debugging, future
}
```

**Rationale**:
- Explicit is better than magic byte detection
- 2 bytes gives room for future content types (compression, encryption markers)
- No ambiguity about body interpretation
- Cleaner for non-Rust implementations (Python, Go)

**Supersedes**: Earlier design used magic byte detection (0x80-0x9f = MsgPack).
Changed to explicit field for clarity and extensibility.

### 3. Service Registration: **Explicit Ready** ✓

Workers send `Command::Ready` on connect with `ReadyPayload`.

```
Worker connects → sends Ready{tools: [...]} → broker adds to routing table
Worker disconnects → implicit removal (or explicit Disconnect if graceful)
```

**Rationale**: Enables:
- Tool discovery without separate ListTools RPC
- Protocol version negotiation
- Future capability flags (compression, encryption)

**Simplification**: Skip Disconnect for now. Heartbeat timeout handles ungraceful exits.

## Session Log

**2025-12-13 Session** (plan verification):
- Cloned `~/src/zguide` (booksbyus/zguide) and `~/src/rfc` (zeromq/rfc)
- Reviewed RFC 7 (MDP 0.1), RFC 18 (MDP 0.2), RFC 6 (PPP)
- Read zguide Chapter 4 (Paranoid Pirate, Majordomo, heartbeating)
- Added Verification Notes section with critical findings
- Key insight: socket close/reopen required on disconnect (ZMQ auto-reconnect insufficient)
- Architecture decision: holler → hootenanny → chaosgarden (unified tracing)
- Added Progress Tracker with per-phase checklists
- Buffer tuning deferred (defaults fine for KB/s throughput)

**2025-12-12 Session**:
- Fixed circular dependency (holler ↔ luanette)
- Moved `tool_to_payload` to hooteproto
- Luanette now connects directly to hootenanny via ZMQ
- Created systemd user units
- Researched MDP, decided on hybrid frame + MsgPack approach

**Commits**:
- `ce126a6` fix(holler): resilient startup with optional backends
- `c5d47ed` feat: add systemd user units
- `b93736e` refactor: luanette connects directly to hootenanny via ZMQ

**Next Session Should**:

**Phase 1** (start here):
1. Create `crates/hooteproto/src/frame.rs` with types from this doc
2. Implement `Command` (u16) and `ContentType` (u16) enums
3. Implement `from_multipart`, `to_multipart`, `from_multipart_with_identity`
4. Unit tests: roundtrip all command types, content type handling
5. `cargo test -p hooteproto` passes

**Phase 2** (can parallelize with 3 and 4):
1. Create `crates/holler/src/heartbeat.rs`
2. Add health tracking fields to `Backend`
3. Spawn heartbeat tasks in `serve.rs`
4. Update `/health` endpoint to show backend states

**Phase 3** (can parallelize with 2 and 4):
1. Update `hootenanny/src/zmq/hooteproto_server.rs` to detect frame protocol
2. Handle `Command::Heartbeat` with immediate response
3. Handle `Command::Request` with dispatch and `Command::Reply`
4. Update luanette similarly (it's a peer client to hootenanny)

**Phase 4** (can parallelize with 2 and 3):
1. Create `crates/hootenanny/src/chaosgarden_client.rs` (IPC)
2. Create `crates/hootenanny/src/luanette_client.rs` (TCP :5570)
3. Route `graph_*` and `play` through chaosgarden proxy
4. Route luanette tools through luanette proxy
5. Propagate traceparent for unified tracing
6. Add chaosgarden + luanette health to hootenanny's `/health`

**Phase 5** (requires 2, 3, 4 complete):
1. Remove luanette and chaosgarden backend configs from holler
2. Simplify `AppState` to single `hootenanny` backend
3. Remove multi-backend dispatch logic
4. Update systemd units if needed
5. Verify: `curl localhost:8080/health` shows hootenanny + chaosgarden + luanette status

**Phase 6**:
1. On backend recovery (Dead → Ready), call `refresh_tools()`
2. Integration test: start all, kill hootenanny, verify Dead state, restart, verify tools re-discovered
