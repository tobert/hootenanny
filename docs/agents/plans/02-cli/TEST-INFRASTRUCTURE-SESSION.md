# Test Infrastructure Session - 2025-11-17

## 🎯 Goal
Rework CLI tests to automatically stand up ephemeral MCP servers using Rust code, avoiding external process management.

## ✅ Accomplishments

### 1. Created Ephemeral Test Server Infrastructure
**File**: `crates/hrcli/tests/common/mod.rs`

- Built `TestMcpServer` that starts hootenanny on random available ports
- Implements proper healthcheck that polls SSE endpoint until ready (no arbitrary delays!)
- Auto-cleanup via `Drop` trait with cancellation token
- Returns actual bound port and full URL

**Key Innovation**: Server runs in dedicated thread with its own tokio runtime - **critical for subprocess connections to work**

### 2. Updated All Test Files
- `dynamic_discovery.rs` - Tool discovery and caching tests
- `execution.rs` - Tool execution tests
- `shell_patterns.rs` - Shell script pattern tests
- All tests now use `HRCLI_SERVER` environment variable to connect to ephemeral server

### 3. Added Debug Instrumentation
- Enhanced MCP client with detailed logging at each connection stage
- Created debug test (`server_connection_debug.rs`) to diagnose issues
- Added SSE stream validation test
- Added subprocess connectivity test

### 4. Used OTLP for Observability
- Leveraged `otlp-mcp` tool to capture server-side telemetry
- Verified MCP initialization handshake completes successfully with standalone server
- Captured traces showing full connection lifecycle

## 🔍 Key Discoveries

### Discovery #1: Subprocess SSE Stream Issue
**Finding**: Axum server spawned with `tokio::spawn` in test runtime cannot properly stream SSE responses to external subprocesses.

**Test Results**:
- ✅ Test process itself can read SSE stream
- ❌ Subprocess (`curl`, `hrcli`) gets empty output

**Root Cause**: tokio runtime context issue - servers need dedicated thread with own runtime for subprocess communication.

### Discovery #2: The Fix
**Solution**: Create entire axum server (including listener binding) within a dedicated `std::thread` with its own tokio runtime.

```rust
std::thread::spawn(move || {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async move {
        let listener = tokio::net::TcpListener::bind(&bind_str).await.unwrap();
        let server = axum::serve(listener, router)
            .with_graceful_shutdown(async move { ct.cancelled().await });
        server.await.unwrap();
    });
});
```

**Result**: `curl` subprocess successfully receives SSE endpoint event with session ID! 🎉

### Discovery #3: Remaining Issue
**Current State**:
- ✅ CLI connects to ephemeral server's SSE endpoint
- ✅ CLI receives session ID
- ✅ CLI spawns SSE listener task
- ❌ SSE listener receives no subsequent messages
- ❌ MCP initialize request times out after 10 seconds

**Hypothesis**: The stream is consumed during initial session ID reading, and the remaining stream passed to the listener task is in a bad state or at EOF.

## 📊 Test Results

### Working: Standalone Server
```bash
$ HRCLI_SERVER=http://127.0.0.1:8765 hrcli discover
✅ [MCP] Connecting to SSE endpoint
✅ [MCP] SSE connection successful
✅ [MCP] Got session ID
✅ [MCP] Starting MCP initialization handshake
✅ [MCP] MCP client fully connected and initialized
✅ 🔍 Discovered Tools (10 total)
```

### Partially Working: Test Embedded Server
```bash
$ cargo test discovers_tools_from_real_server
✅ [MCP] Connecting to SSE endpoint
✅ [MCP] SSE connection successful
✅ [MCP] Got session ID
✅ [MCP] Starting MCP initialization handshake
✅ [MCP] SSE listener task started
❌ [MCP-LISTENER] (no chunks received)
❌ Timeout waiting for initialize response
```

## 🎨 Architecture Patterns Established

### Test Server Pattern
```rust
// 1. Create in dedicated thread with own runtime
std::thread::spawn(move || { /* server setup */ });

// 2. Use proper healthcheck (no delays!)
Self::wait_for_server_ready(port).await?

// 3. Return useful info
Ok(TestMcpServer { port, url, shutdown_token, .. })
```

### Test Pattern
```rust
#[tokio::test]
async fn test_something() {
    let server = TestMcpServer::start().await.unwrap();

    Command::cargo_bin("hrcli")
        .env("HRCLI_SERVER", &server.url)
        .arg("discover")
        .assert()
        .success();

    // Server auto-cleaned up on drop
}
```

## 🚧 Next Steps

### Immediate (to unblock tests)
1. **Fix stream consumption issue**: Refactor MCP client's session ID reading to not consume/move the stream, or use a fresh connection for the listener
2. **Verify initialize response flow**: Ensure server sends responses back through correct SSE stream
3. **Run full test suite**: Verify all tests pass once stream issue resolved

### Future Enhancements
1. Add `--server` CLI flag support (currently only env var works)
2. Implement caching for discovered tools
3. Add more test coverage for error cases
4. Consider connection pooling/reuse patterns

## 📝 Files Changed

### New Files
- `crates/hrcli/tests/common/mod.rs` - Test server infrastructure
- `crates/hrcli/tests/server_connection_debug.rs` - Debug test
- `crates/hrcli/tests/sse_stream_test.rs` - SSE validation
- `crates/hrcli/tests/subprocess_sse_test.rs` - Subprocess validation

### Modified Files
- `crates/hrcli/tests/dynamic_discovery.rs` - Use real server
- `crates/hrcli/tests/execution.rs` - Use real server
- `crates/hrcli/src/mcp_client.rs` - Add debug logging, fix timeouts
- `crates/hrcli/Cargo.toml` - Add hootenanny dev-dependency

## 🎓 Lessons Learned

1. **No Mock When Real Works Better**: Real MCP server provides better test fidelity than wiremock
2. **Healthchecks > Delays**: Always use active polling over arbitrary sleeps
3. **Runtime Boundaries Matter**: tokio tasks != threads for cross-process communication
4. **OTLP for Debugging**: Telemetry invaluable for distributed system debugging
5. **Incremental Validation**: Build up from simple (curl) to complex (full CLI)

## 🤝 Collaboration Notes

This session demonstrated excellent human-AI collaboration:
- Human provided domain expertise on avoiding delays
- AI discovered runtime/threading issue through systematic debugging
- OTLP observability helped validate hypotheses
- Incremental test refinement isolated exact failure point

## 🔖 Session Metadata

**Date**: 2025-11-17
**Duration**: ~3 hours
**Status**: Partially Complete - Core infrastructure working, one stream issue remains
**Next Session Focus**: Fix SSE stream consumption to unblock all tests

---

**Authored by**:
- 🤖 Claude Sonnet 4.5
- 👤 Amy Tobey (human collaborator)

**Tools Used**:
- OTLP-MCP for observability
- hootenanny (embedded test server)
- rmcp (MCP SDK)
- assert_cmd (CLI testing)
