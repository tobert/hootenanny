# 00: ZeroMQ IPC Layer

**File:** `src/ipc.rs` (and `src/ipc/` module)
**Focus:** Protocol between hootenanny (control plane) and chaosgarden (RT audio)
**Dependencies:** `zeromq`, `serde`, `rmp-serde` (MessagePack)

---

## Task

Create `crates/chaosgarden/src/ipc.rs` with the ZeroMQ socket infrastructure for bidirectional communication between hootenanny and chaosgarden.

**Why this matters:** Chaosgarden needs RT priority for audio. Hootenanny handles orchestration, jobs, CAS. Separating them via IPC gives us:
- RT priority for the audio process
- Crash isolation (hootenanny restart doesn't kill playback mid-performance)
- Multiple clients can subscribe to IOPub (visualization tools, other agents)
- Clean Jupyter-inspired architecture that's proven at scale
- Foundation for distributed multi-machine setup

**Deliverables:**
1. `ipc.rs` with socket setup, message types, and protocol handlers
2. `GardenClient` for hootenanny side (connects to chaosgarden)
3. `GardenServer` for chaosgarden side (accepts connections)
4. Message envelope with routing, correlation IDs
5. Tests using inproc:// transport

**Definition of Done:**
```bash
cargo fmt --check
cargo clippy -- -D warnings
cargo check
cargo test
```

## Out of Scope

- PipeWire integration (task 05)
- Actual command implementations (later tasks)
- Authentication/encryption (CURVE can be added later)
- Multi-daemon support (one daemon per host for now)

Focus ONLY on the transport layer and message protocol.

---

## Architecture: Jupyter-Inspired ZeroMQ

Jupyter's kernel protocol has run for a decade at massive scale. We adapt it:

```
┌─────────────────────────────────────────────────────────────────────┐
│                           HOOTENANNY                                 │
│  (control plane — CAS, jobs, luanette, worker registry)             │
│                                                                      │
│  ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌──────────┐ ┌───────────┐    │
│  │Control  │ │ Shell   │ │IOPub    │ │Heartbeat │ │  Query    │    │
│  │DEALER   │ │DEALER   │ │ SUB     │ │  REQ     │ │  REQ      │    │
│  └────┬────┘ └────┬────┘ └────┬────┘ └────┬─────┘ └─────┬─────┘    │
└───────┼──────────┼──────────┼──────────┼───────────────┼───────────┘
        │          │          │          │               │
        │   ipc:// (local) or tcp:// (multi-machine)     │
        │          │          │          │               │
┌───────┼──────────┼──────────┼──────────┼───────────────┼───────────┐
│  ┌────▼────┐ ┌───▼────┐ ┌───▼────┐ ┌───▼──────┐ ┌─────▼─────┐     │
│  │Control  │ │ Shell  │ │IOPub   │ │Heartbeat │ │  Query    │     │
│  │ROUTER   │ │ROUTER  │ │ PUB    │ │  REP     │ │  REP      │     │
│  └─────────┘ └────────┘ └────────┘ └──────────┘ └───────────┘     │
│                                                                    │
│                      CHAOSGARDEN DAEMON                            │
│                      (RT priority, owns PipeWire)                  │
└────────────────────────────────────────────────────────────────────┘
```

### Socket Roles

| Socket | Pattern | Direction | Purpose |
|--------|---------|-----------|---------|
| **Control** | DEALER/ROUTER | Hootenanny→Garden | Urgent: stop, pause, shutdown, interrupt |
| **Shell** | DEALER/ROUTER | Hootenanny↔Garden | Commands: create region, resolve latent, set tempo |
| **IOPub** | SUB/PUB | Garden→Hootenanny+ | Events: LatentResolved, PlaybackPosition, Progress |
| **Heartbeat** | REQ/REP | Hootenanny↔Garden | Liveness: ping/pong every N seconds |
| **Query** | REQ/REP | Hootenanny→Garden | Trustfall queries (separate to not block shell) |

**Note:** IOPub is PUB/SUB so multiple subscribers can listen (hootenanny, visualization tools, other agents).

### Why 5 Sockets?

**Control vs Shell:** Control is the "priority lane." If the shell is busy processing a long command, control messages (stop!, shutdown!) still get through immediately.

**IOPub (PUB/SUB):** Events broadcast to all subscribers. Future visualization tools can subscribe without touching MCP. PUB/SUB is fire-and-forget — the daemon never blocks waiting for subscribers.

**Query separate from Shell:** Trustfall queries can be expensive. Running them on Shell would block commands. Separate socket means queries don't starve control.

**Heartbeat:** Simple liveness detection. If 3 pings fail, MCP knows daemon died.

---

## Message Protocol

### Envelope Format

Every message has a header envelope (like Jupyter):

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageHeader {
    /// Unique message ID (for correlation)
    pub msg_id: Uuid,
    /// Session ID (identifies the MCP connection)
    pub session: Uuid,
    /// Message type (e.g., "execute_request", "status")
    pub msg_type: String,
    /// Protocol version
    pub version: String,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message<T> {
    pub header: MessageHeader,
    /// Reference to message this is replying to (if any)
    pub parent_header: Option<MessageHeader>,
    /// Arbitrary metadata
    pub metadata: HashMap<String, serde_json::Value>,
    /// The actual content
    pub content: T,
}
```

### Message Types

#### Shell Channel (Request/Reply)

```rust
// Requests (Hootenanny → Chaosgarden)
pub enum ShellRequest {
    // Region operations
    CreateRegion { position: Beat, duration: Beat, behavior: Behavior },
    DeleteRegion { region_id: Uuid },
    MoveRegion { region_id: Uuid, new_position: Beat },

    // Latent state updates (hootenanny tells chaosgarden about job progress)
    UpdateLatentStarted { region_id: Uuid, job_id: String },
    UpdateLatentProgress { region_id: Uuid, progress: f32 },
    UpdateLatentResolved { region_id: Uuid, artifact_id: String, content_hash: String, content_type: ContentType },
    UpdateLatentFailed { region_id: Uuid, error: String },

    // Approval operations (human/agent decisions flow through hootenanny)
    ApproveLatent { region_id: Uuid, decided_by: Uuid },
    RejectLatent { region_id: Uuid, decided_by: Uuid, reason: Option<String> },

    // Playback control
    Play,
    Pause,
    Stop,
    Seek { beat: Beat },
    SetTempo { bpm: f64 },

    // Graph operations
    AddNode { node: NodeDescriptor },
    RemoveNode { node_id: Uuid },
    Connect { source: PortRef, dest: PortRef },
    Disconnect { source: PortRef, dest: PortRef },

    // Participant operations
    RegisterParticipant { participant: Participant },
    UpdateParticipant { participant_id: Uuid, updates: ParticipantUpdate },

    // State queries (simple ones; complex go to Query socket)
    GetTransportState,
    GetRegions { range: Option<(Beat, Beat)> },
    GetPendingApprovals,
}

// Replies (Chaosgarden → Hootenanny)
pub enum ShellReply {
    Ok { result: serde_json::Value },
    Error { error: String, traceback: Option<String> },
    RegionCreated { region_id: Uuid },
    NodeAdded { node_id: Uuid },
    TransportState { playing: bool, position: Beat, tempo: f64 },
    Regions { regions: Vec<RegionSummary> },
    PendingApprovals { approvals: Vec<PendingApproval> },
}
```

#### Control Channel (Priority)

```rust
pub enum ControlRequest {
    Shutdown,
    Interrupt,  // Cancel current operation
    Pause,      // Emergency pause (bypasses shell queue)
    DebugDump,  // Dump internal state for debugging
}

pub enum ControlReply {
    Ok,
    ShuttingDown,
    Interrupted { was_running: String },
}
```

#### IOPub Channel (Events)

```rust
pub enum IOPubMessage {
    // Status
    Status { execution_state: ExecutionState },

    // Latent lifecycle
    LatentSubmitted { region_id: Uuid, job_id: String },
    LatentProgress { region_id: Uuid, progress: f32 },
    LatentResolved { region_id: Uuid, artifact_id: String, content_hash: String },
    LatentFailed { region_id: Uuid, error: String },
    LatentApproved { region_id: Uuid },
    LatentRejected { region_id: Uuid, reason: Option<String> },

    // Playback
    PlaybackStarted,
    PlaybackStopped,
    PlaybackPosition { beat: Beat, second: f64 },
    MixedIn { region_id: Uuid, at_beat: Beat },

    // Graph changes
    NodeAdded { node_id: Uuid, name: String },
    NodeRemoved { node_id: Uuid },
    ConnectionMade { source: PortRef, dest: PortRef },
    ConnectionBroken { source: PortRef, dest: PortRef },

    // Participant changes
    ParticipantOnline { participant_id: Uuid, name: String },
    ParticipantOffline { participant_id: Uuid },
    CapabilityChanged { participant_id: Uuid, capability: String, available: bool },

    // Errors and warnings
    Error { error: String, context: Option<String> },
    Warning { message: String },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ExecutionState {
    Idle,
    Busy,
    Starting,
    ShuttingDown,
}
```

#### Query Channel

```rust
pub struct QueryRequest {
    pub query: String,          // Trustfall query
    pub variables: HashMap<String, serde_json::Value>,
}

pub enum QueryReply {
    Results { rows: Vec<serde_json::Value> },
    Error { error: String },
}
```

#### Heartbeat Channel

Simple ping/pong — send any bytes, get same bytes back.

---

## Types

```rust
use zeromq::{Socket, DealerSocket, RouterSocket, PubSocket, SubSocket, ReqSocket, RepSocket};

/// Configuration for connecting to a chaosgarden daemon
#[derive(Debug, Clone)]
pub struct GardenEndpoints {
    pub control: String,    // e.g., "ipc:///tmp/garden-control"
    pub shell: String,      // e.g., "ipc:///tmp/garden-shell"
    pub iopub: String,      // e.g., "ipc:///tmp/garden-iopub"
    pub heartbeat: String,  // e.g., "ipc:///tmp/garden-hb"
    pub query: String,      // e.g., "ipc:///tmp/garden-query"
}

impl GardenEndpoints {
    /// Default IPC endpoints for local daemon
    pub fn local() -> Self {
        Self {
            control: "ipc:///tmp/chaosgarden-control".into(),
            shell: "ipc:///tmp/chaosgarden-shell".into(),
            iopub: "ipc:///tmp/chaosgarden-iopub".into(),
            heartbeat: "ipc:///tmp/chaosgarden-hb".into(),
            query: "ipc:///tmp/chaosgarden-query".into(),
        }
    }

    /// TCP endpoints for remote daemon
    pub fn tcp(host: &str, base_port: u16) -> Self {
        Self {
            control: format!("tcp://{}:{}", host, base_port),
            shell: format!("tcp://{}:{}", host, base_port + 1),
            iopub: format!("tcp://{}:{}", host, base_port + 2),
            heartbeat: format!("tcp://{}:{}", host, base_port + 3),
            query: format!("tcp://{}:{}", host, base_port + 4),
        }
    }
}

/// Client side (hootenanny uses this to talk to chaosgarden)
pub struct GardenClient {
    session: Uuid,
    control: DealerSocket,
    shell: DealerSocket,
    iopub: SubSocket,
    heartbeat: ReqSocket,
    query: ReqSocket,
}

/// Server side (chaosgarden daemon uses this)
pub struct GardenServer {
    control: RouterSocket,
    shell: RouterSocket,
    iopub: PubSocket,
    heartbeat: RepSocket,
    query: RepSocket,
}
```

---

## GardenClient Methods (used by hootenanny)

```rust
impl GardenClient {
    /// Connect to a running chaosgarden daemon
    pub async fn connect(endpoints: &GardenEndpoints) -> Result<Self>;

    /// Send a shell request and wait for reply
    pub async fn request(&self, req: ShellRequest) -> Result<ShellReply>;

    /// Send a control request (priority)
    pub async fn control(&self, req: ControlRequest) -> Result<ControlReply>;

    /// Execute a Trustfall query
    pub async fn query(&self, query: &str, vars: HashMap<String, serde_json::Value>)
        -> Result<Vec<serde_json::Value>>;

    /// Subscribe to IOPub events (returns a stream)
    pub fn events(&self) -> impl Stream<Item = IOPubMessage>;

    /// Check if daemon is alive
    pub async fn ping(&self, timeout: Duration) -> Result<bool>;

    /// Convenience: wait for specific event
    pub async fn wait_for<F>(&self, predicate: F, timeout: Duration) -> Result<IOPubMessage>
    where F: Fn(&IOPubMessage) -> bool;
}
```

---

## GardenServer Methods (chaosgarden daemon)

```rust
impl GardenServer {
    /// Bind to endpoints and start listening
    pub async fn bind(endpoints: &GardenEndpoints) -> Result<Self>;

    /// Main event loop - call handlers for incoming messages
    pub async fn run<H: Handler>(&self, handler: H) -> Result<()>;

    /// Publish an event to all subscribers
    pub fn publish(&self, event: IOPubMessage) -> Result<()>;

    /// Update execution state (broadcasts on IOPub)
    pub fn set_state(&self, state: ExecutionState);
}

/// Handler trait for processing requests (implemented by chaosgarden)
pub trait Handler: Send + Sync {
    fn handle_shell(&self, req: ShellRequest) -> ShellReply;
    fn handle_control(&self, req: ControlRequest) -> ControlReply;
    fn handle_query(&self, req: QueryRequest) -> QueryReply;
}
```

---

## Serialization

MessagePack for production (fast, compact), JSON for debugging:

```rust
pub trait WireFormat {
    fn serialize<T: Serialize>(msg: &Message<T>) -> Result<Vec<u8>>;
    fn deserialize<T: DeserializeOwned>(data: &[u8]) -> Result<Message<T>>;
}

pub struct MsgPackFormat;
pub struct JsonFormat;  // For debugging

impl WireFormat for MsgPackFormat {
    fn serialize<T: Serialize>(msg: &Message<T>) -> Result<Vec<u8>> {
        rmp_serde::to_vec(msg).map_err(Into::into)
    }

    fn deserialize<T: DeserializeOwned>(data: &[u8]) -> Result<Message<T>> {
        rmp_serde::from_slice(data).map_err(Into::into)
    }
}
```

---

## Connection Lifecycle

### Chaosgarden Daemon Startup

```
1. GardenServer::bind() creates and binds all 5 sockets
2. Daemon initializes playback engine, PipeWire connection
3. GardenServer::run() enters main event loop
4. Publishes Status { execution_state: Idle } on IOPub
5. Waits for commands from hootenanny
```

### Hootenanny Connects

```
1. GardenClient::connect() connects to all 5 endpoints
2. Hootenanny subscribes to IOPub (empty prefix = all messages)
3. Hootenanny sends ping on heartbeat to verify connection
4. On success, hootenanny can send shell/control commands
```

### Graceful Shutdown

```
1. Hootenanny sends ControlRequest::Shutdown
2. Chaosgarden publishes Status { execution_state: ShuttingDown }
3. Chaosgarden stops playback, closes PipeWire
4. Chaosgarden replies ControlReply::ShuttingDown
5. Chaosgarden closes all sockets and exits
```

---

## Error Handling

**Socket errors:** Reconnect with exponential backoff. ZMQ sockets are resilient.

**Message errors:** Log and reply with ShellReply::Error. Don't crash.

**Heartbeat timeout:** After 3 missed pings, hootenanny assumes chaosgarden dead. Can attempt restart or alert.

---

## Future Extensions

**CURVE encryption:** ZMQ has built-in encryption. Add when we need remote access.

**Multiple daemons:** One per host, different endpoints. Client connects to multiple.

**WebSocket bridge:** Expose IOPub via WebSocket for browser-based visualization.

**Recording/replay:** Log all messages for debugging and session replay.

---

## Acceptance Criteria

- [ ] `GardenServer::bind()` creates all 5 sockets
- [ ] `GardenClient::connect()` connects to running daemon
- [ ] Shell request/reply round-trip works
- [ ] Control messages bypass shell queue
- [ ] IOPub events broadcast to all subscribers
- [ ] Heartbeat ping/pong works with timeout
- [ ] Query socket handles Trustfall queries
- [ ] MessagePack serialization works
- [ ] Tests pass with inproc:// transport
