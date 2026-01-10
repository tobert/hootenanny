# 03-zmq: ZMQ Client

**File:** `crates/vibeweaver/src/zmq.rs`
**Dependencies:** None
**Unblocks:** 04-scheduler, 06-broadcast

---

## Task

Create ZMQ client following luanette patterns with DEALER for request/reply and SUB for broadcasts.

## Deliverables

- `crates/vibeweaver/src/zmq.rs`
- HOOT01 framing integration
- Unit tests with mock responses

## Types

```rust
use zeromq::{DealerSocket, SubSocket, Socket, SocketRecv, SocketSend};
use hooteproto::frame::{Frame, FrameType};
use tokio::sync::mpsc;
use anyhow::Result;

/// ZMQ client for hootenanny communication
pub struct ZmqClient {
    dealer: DealerSocket,
    identity: String,
}

/// Broadcast receiver (separate from client for ownership)
pub struct BroadcastReceiver {
    sub: SubSocket,
}

/// Parsed broadcast message
#[derive(Debug, Clone)]
pub enum Broadcast {
    JobStateChanged {
        job_id: String,
        state: String,
        artifact_id: Option<String>,
    },
    ArtifactCreated {
        artifact_id: String,
        content_hash: String,
        tags: Vec<String>,
    },
    TransportStateChanged {
        state: String,
        position_beats: f64,
    },
    BeatTick {
        beat: f64,
        tempo_bpm: f64,
    },
    MarkerReached {
        name: String,
        beat: f64,
    },
    Unknown(Vec<u8>),
}

impl ZmqClient {
    /// Connect DEALER socket to hootenanny router
    pub async fn connect(endpoint: &str, identity: &str) -> Result<Self>;

    /// Send tool call request, await response
    pub async fn call_tool(&self, name: &str, args: serde_json::Value) -> Result<serde_json::Value>;

    /// Send request without waiting for response
    pub async fn send_request(&self, name: &str, args: serde_json::Value) -> Result<String>;

    /// Receive response for request ID
    pub async fn recv_response(&self, request_id: &str) -> Result<serde_json::Value>;

    /// Close connection
    pub async fn close(self) -> Result<()>;
}

impl BroadcastReceiver {
    /// Connect SUB socket and subscribe to all topics
    pub async fn connect(endpoint: &str) -> Result<Self>;

    /// Subscribe to specific topic prefix
    pub async fn subscribe(&self, topic: &str) -> Result<()>;

    /// Receive next broadcast (blocking)
    pub async fn recv(&mut self) -> Result<Broadcast>;

    /// Try receive broadcast (non-blocking)
    pub fn try_recv(&mut self) -> Result<Option<Broadcast>>;

    /// Close connection
    pub async fn close(self) -> Result<()>;
}

/// Parse raw broadcast bytes into typed Broadcast
fn parse_broadcast(data: &[u8]) -> Result<Broadcast>;
```

## Implementation Notes

- Use HOOT01 framing from `hooteproto::frame`
- DEALER identity should be `vibeweaver-{session_id}`
- SUB socket subscribes to: `job.`, `artifact.`, `transport.`, `beat.`, `marker.`
- Handle reconnection gracefully

## Reference

See `crates/luanette/src/clients/manager.rs` for DEALER patterns.

## Definition of Done

```bash
cargo fmt --check -p vibeweaver
cargo clippy -p vibeweaver -- -D warnings
cargo test -p vibeweaver zmq::
```

## Acceptance Criteria

- [ ] DEALER connects and sends HOOT01 frames
- [ ] Tool call returns parsed response
- [ ] SUB receives and parses broadcasts
- [ ] Reconnection on disconnect
