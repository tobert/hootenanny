# rzmq Issue: Cap reconnect_ivl_max to prevent runaway backoff

**Repo:** https://github.com/CheatModeOn/rzmq
**Branch for fix:** (TBD - stack on `fix-ipc-reconnect-uri`)

## Problem

When connections fail repeatedly, the exponential backoff can grow unbounded:
1s -> 2s -> 4s -> 8s -> ... -> 8192s (2.3 hours)

This makes recovery from transient failures very slow.

## Current Behavior

- `RECONNECT_IVL` sets initial interval (default 1s)
- `RECONNECT_IVL_MAX` is supported but defaults to 0
- When `RECONNECT_IVL_MAX` is 0, backoff grows unbounded
- Backoff doubles on each failure with no upper limit

## Impact

After a server restart or network hiccup:
- Client waits increasingly long between reconnection attempts
- After ~13 failures: 8192 seconds (2.3 hours) between attempts
- Services appear "stuck" even after the server is back

## Reproduction

1. Create a DEALER socket connecting to non-existent endpoint
2. Observe reconnection attempts in tracing logs
3. Each attempt doubles the interval: 1s, 2s, 4s, 8s, ...
4. After ~10 minutes, interval exceeds 1 hour

## Proposed Fix

Option A: Honor `RECONNECT_IVL_MAX` and add reasonable default
```rust
// When RECONNECT_IVL_MAX is 0, cap at 60 seconds by default
let max_interval = if config.reconnect_ivl_max == 0 {
    Duration::from_secs(60)
} else {
    Duration::from_millis(config.reconnect_ivl_max as u64)
};
let next_interval = std::cmp::min(current_interval * 2, max_interval);
```

Option B: Interpret 0 as "10x RECONNECT_IVL"
```rust
let max_interval = if config.reconnect_ivl_max == 0 {
    Duration::from_millis(config.reconnect_ivl as u64 * 10)
} else {
    Duration::from_millis(config.reconnect_ivl_max as u64)
};
```

## Workaround (in hootenanny)

Set `RECONNECT_IVL_MAX` explicitly when creating sockets:

```rust
// Cap reconnect backoff at 60 seconds
socket.set_option_raw(RECONNECT_IVL_MAX, &60000i32.to_ne_bytes()).await?;
```

See: `crates/hootenanny/src/zmq/garden_client.rs`

## libzmq Reference

From libzmq docs:
> ZMQ_RECONNECT_IVL_MAX: Set maximum reconnection interval
> The default value of zero means no upper limit.

However, in practice most applications want some reasonable upper bound
to ensure timely recovery from transient failures.
