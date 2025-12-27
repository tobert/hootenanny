# rzmq Issue: REQ socket times out when idle

**Repo:** https://github.com/CheatModeOn/rzmq
**Branch for fix:** (TBD - stack on `fix-ipc-reconnect-uri`)

## Problem

REQ sockets in Operational phase timeout after `rcvtimeo` (default 300s) even when they haven't sent a request yet.

## Root Cause

`SessionConnectionActorX` ARM 6 (`core/src/sessionx/actor.rs:245-267`) unconditionally reads from the network when in Operational phase. For REQ sockets, this is semantically incorrect - REQ should only read AFTER sending a request.

The default timeout is set in `core/src/sessionx/protocol_handler/data_io.rs:24`:
```rust
let operation_timeout = handler.config.rcvtimeo.unwrap_or(Duration::from_secs(300));
```

## Impact

- Idle REQ connections die after 5 minutes
- Triggers exponential backoff (1s -> 2s -> ... -> 8192s)
- Eventually makes services unreachable for hours

## Reproduction

1. Create a REQ socket
2. Connect to a REP server
3. Don't send any messages
4. Wait 300 seconds
5. Observe: Session stops with "Operation timed out"

## Proposed Fix

Gate ARM 6 read based on `local_socket_type`:
- REQ sockets: only read when ISocket signals "expecting reply"
- Other sockets (DEALER, SUB, etc.): read proactively as before

The socket type information is already available from the ZMTP handshake (added in commit b3138ef).

Implementation sketch:
```rust
// In actor.rs ARM 6
if local_socket_type == SocketType::Req && !isocket.expecting_reply() {
    // Don't read - REQ must send before recv
    continue;
}
// Existing read logic...
```

## Workaround (in hootenanny)

Send periodic keepalive heartbeats to prevent idle timeout:

```rust
// GardenClient spawns a keepalive task
fn spawn_keepalive_task(heartbeat: Arc<RwLock<Socket>>, interval: Duration) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        loop {
            ticker.tick().await;
            let _ = send_heartbeat(&heartbeat).await;
        }
    });
}
```

See: `crates/hootenanny/src/zmq/garden_client.rs`

## Related

- Commit b3138ef added `peer_socket_type` to handshake state
- `ZmtpHandshakeStateX` already stores socket type
- libzmq behavior: REQ socket blocks on recv until a message is sent
