# Plan: Fix SSE Reconnection Session ID Bug in MCP TypeScript SDK

## Overview

Fork `modelcontextprotocol/typescript-sdk` and fix the bug where `SSEClientTransport` fails to properly handle session ID updates on reconnection, causing HTTP 410 errors.

## Prerequisites

- [x] MCP TypeScript SDK cloned to `~/src/mcp-typescript-sdk`
- [ ] Fork created on GitHub
- [ ] Development environment set up

## Phase 1: Setup & Reproduction

### 1.1 Fork and Configure Repository

```bash
# Create GitHub fork (via gh cli or web UI)
gh repo fork modelcontextprotocol/typescript-sdk --clone=false

# Add fork as remote
cd ~/src/mcp-typescript-sdk
git remote add fork git@github.com:YOUR_USERNAME/typescript-sdk.git
git checkout -b fix/sse-reconnection-session-id
```

### 1.2 Install Dependencies and Verify Tests Pass

```bash
npm install
npm test
```

### 1.3 Understand Current Test Structure

Review existing SSE tests in `src/client/sse.test.ts` to understand:
- Test patterns and mocking strategies
- How the test server is set up
- Existing coverage gaps (no reconnection tests!)

## Phase 2: Write Failing Tests (TDD)

### 2.1 Test Case: Endpoint Updates on Reconnection

**File**: `src/client/sse.test.ts`

Add new `describe` block for reconnection scenarios:

```typescript
describe('reconnection handling', () => {
    it('updates endpoint URL when SSE reconnects with new session ID', async () => {
        // Setup: Server that will close connection and reopen with new session
        let connectionCount = 0;
        let currentSessionId = 'session-1';

        resourceServer = createServer((req, res) => {
            if (req.method === 'GET' && req.url === '/') {
                connectionCount++;
                currentSessionId = `session-${connectionCount}`;

                res.writeHead(200, {
                    'Content-Type': 'text/event-stream',
                    'Cache-Control': 'no-cache',
                    Connection: 'keep-alive'
                });

                // Send endpoint event with session ID
                res.write('event: endpoint\n');
                res.write(`data: /message?sessionId=${currentSessionId}\n\n`);

                // Close after first connection to trigger reconnect
                if (connectionCount === 1) {
                    setTimeout(() => res.end(), 100);
                }
            }
        });

        // Start server and transport
        await startServer();
        transport = new SSEClientTransport(resourceBaseUrl);
        await transport.start();

        // Capture initial endpoint
        const initialEndpoint = transport['_endpoint']?.href;
        expect(initialEndpoint).toContain('session-1');

        // Wait for reconnection
        await new Promise(resolve => setTimeout(resolve, 500));

        // Verify endpoint was updated
        const newEndpoint = transport['_endpoint']?.href;
        expect(newEndpoint).toContain('session-2');
        expect(newEndpoint).not.toEqual(initialEndpoint);
    });

    it('successfully sends messages after reconnection with new session ID', async () => {
        // Similar setup but verify POST works after reconnect
        let connectionCount = 0;
        let receivedSessionIds: string[] = [];

        resourceServer = createServer((req, res) => {
            if (req.method === 'GET' && req.url === '/') {
                connectionCount++;
                const sessionId = `session-${connectionCount}`;

                res.writeHead(200, {
                    'Content-Type': 'text/event-stream',
                    'Cache-Control': 'no-cache',
                    Connection: 'keep-alive'
                });
                res.write('event: endpoint\n');
                res.write(`data: /message?sessionId=${sessionId}\n\n`);

                if (connectionCount === 1) {
                    setTimeout(() => res.end(), 100);
                }
            } else if (req.method === 'POST') {
                // Extract session ID from URL
                const url = new URL(req.url!, resourceBaseUrl);
                const sessionId = url.searchParams.get('sessionId');
                receivedSessionIds.push(sessionId!);

                // Return 410 for old session IDs (simulating server restart)
                if (sessionId === 'session-1') {
                    res.writeHead(410).end('Session expired');
                } else {
                    res.writeHead(200).end();
                }
            }
        });

        await startServer();
        transport = new SSEClientTransport(resourceBaseUrl);
        await transport.start();

        // Wait for reconnection
        await new Promise(resolve => setTimeout(resolve, 500));

        // Send message - should use new session ID
        const message: JSONRPCMessage = {
            jsonrpc: '2.0',
            id: 'test-1',
            method: 'test',
            params: {}
        };

        await transport.send(message);

        // Verify the POST used the new session ID
        expect(receivedSessionIds).toContain('session-2');
        expect(receivedSessionIds).not.toContain('session-1');
    });

    it('does not reject start() promise on recoverable connection errors', async () => {
        // Test that transient errors don't permanently break the transport
        let connectionAttempts = 0;

        resourceServer = createServer((req, res) => {
            connectionAttempts++;

            if (connectionAttempts < 3) {
                // Fail first two attempts
                res.writeHead(503).end();
                return;
            }

            // Third attempt succeeds
            res.writeHead(200, {
                'Content-Type': 'text/event-stream',
                'Cache-Control': 'no-cache',
                Connection: 'keep-alive'
            });
            res.write('event: endpoint\n');
            res.write('data: /message?sessionId=final-session\n\n');
        });

        await startServer();
        transport = new SSEClientTransport(resourceBaseUrl);

        // This should eventually succeed after retries
        await transport.start();

        expect(transport['_endpoint']?.href).toContain('final-session');
    });

    it('calls onerror callback on connection drop but continues operating', async () => {
        const errors: Error[] = [];
        let connectionCount = 0;

        resourceServer = createServer((req, res) => {
            connectionCount++;

            res.writeHead(200, {
                'Content-Type': 'text/event-stream',
                'Cache-Control': 'no-cache',
                Connection: 'keep-alive'
            });
            res.write('event: endpoint\n');
            res.write(`data: /message?sessionId=session-${connectionCount}\n\n`);

            if (connectionCount === 1) {
                setTimeout(() => res.destroy(), 100); // Abrupt close
            }
        });

        await startServer();
        transport = new SSEClientTransport(resourceBaseUrl);
        transport.onerror = (err) => errors.push(err);

        await transport.start();

        // Wait for reconnection cycle
        await new Promise(resolve => setTimeout(resolve, 500));

        // Should have received an error notification
        expect(errors.length).toBeGreaterThan(0);

        // But transport should still be functional
        expect(transport['_endpoint']?.href).toContain('session-2');
    });
});
```

### 2.2 Test Case: Session ID Getter (New Feature)

```typescript
describe('sessionId property', () => {
    it('exposes sessionId extracted from endpoint URL', async () => {
        resourceServer = createServer((req, res) => {
            res.writeHead(200, {
                'Content-Type': 'text/event-stream',
                'Cache-Control': 'no-cache',
                Connection: 'keep-alive'
            });
            res.write('event: endpoint\n');
            res.write('data: /message?sessionId=test-session-123\n\n');
        });

        await startServer();
        transport = new SSEClientTransport(resourceBaseUrl);
        await transport.start();

        // New property should expose session ID
        expect(transport.sessionId).toBe('test-session-123');
    });

    it('updates sessionId after reconnection', async () => {
        let connectionCount = 0;

        resourceServer = createServer((req, res) => {
            connectionCount++;
            res.writeHead(200, {
                'Content-Type': 'text/event-stream',
                'Cache-Control': 'no-cache',
                Connection: 'keep-alive'
            });
            res.write('event: endpoint\n');
            res.write(`data: /message?sessionId=session-${connectionCount}\n\n`);

            if (connectionCount === 1) {
                setTimeout(() => res.end(), 100);
            }
        });

        await startServer();
        transport = new SSEClientTransport(resourceBaseUrl);
        await transport.start();

        expect(transport.sessionId).toBe('session-1');

        await new Promise(resolve => setTimeout(resolve, 500));

        expect(transport.sessionId).toBe('session-2');
    });
});
```

### 2.3 Run Tests - Expect Failures

```bash
npm test -- --grep "reconnection handling"
npm test -- --grep "sessionId property"
```

Expected failures:
- `sessionId` property doesn't exist
- Reconnection doesn't update endpoint (or errors out)
- `onerror` rejects the Promise breaking the transport

## Phase 3: Implement Fix

### 3.1 Add `sessionId` Getter

**File**: `src/client/sse.ts`

```typescript
/**
 * Extract session ID from endpoint URL query parameters.
 */
get sessionId(): string | undefined {
    if (!this._endpoint) return undefined;
    return this._endpoint.searchParams.get('sessionId') ?? undefined;
}
```

### 3.2 Refactor Event Handlers for Reconnection Support

**File**: `src/client/sse.ts`

Key changes:
1. Separate initial connection Promise from ongoing event handling
2. Don't reject Promise on recoverable errors
3. Always update `_endpoint` on `endpoint` events
4. Add reconnection state tracking

```typescript
private _connected = false;
private _reconnecting = false;

private _startOrAuth(): Promise<void> {
    const fetchImpl = (this?._eventSourceInit?.fetch ?? this._fetch ?? fetch) as typeof fetch;

    return new Promise((resolve, reject) => {
        this._eventSource = new EventSource(this._url.href, {
            // ... existing config
        });
        this._abortController = new AbortController();

        // Track if we've resolved the initial connection
        let initialConnectionResolved = false;

        this._eventSource.onerror = event => {
            if (event.code === 401 && this._authProvider) {
                this._authThenStart().then(resolve, reject);
                return;
            }

            const error = new SseError(event.code, event.message, event);

            // Only reject if we haven't connected yet
            if (!initialConnectionResolved) {
                reject(error);
            }

            // Always notify via callback
            this.onerror?.(error);

            // Mark as reconnecting (EventSource will auto-reconnect)
            if (this._connected) {
                this._reconnecting = true;
            }
        };

        this._eventSource.onopen = () => {
            if (this._reconnecting) {
                this._reconnecting = false;
                // Reconnected - endpoint event will update _endpoint
            }
        };

        this._eventSource.addEventListener('endpoint', (event: Event) => {
            const messageEvent = event as MessageEvent;

            try {
                const newEndpoint = new URL(messageEvent.data, this._url);
                if (newEndpoint.origin !== this._url.origin) {
                    throw new Error(`Endpoint origin does not match: ${newEndpoint.origin}`);
                }

                // Always update endpoint (handles both initial and reconnection)
                this._endpoint = newEndpoint;
                this._connected = true;

            } catch (error) {
                if (!initialConnectionResolved) {
                    reject(error);
                }
                this.onerror?.(error as Error);
                void this.close();
                return;
            }

            // Only resolve once for initial connection
            if (!initialConnectionResolved) {
                initialConnectionResolved = true;
                resolve();
            }
        });

        this._eventSource.onmessage = (event: Event) => {
            // ... existing message handling (unchanged)
        };
    });
}
```

### 3.3 Update Transport Interface Compliance

Ensure `SSEClientTransport` properly implements the `sessionId` property from `Transport` interface.

## Phase 4: Verify Fix

### 4.1 Run All Tests

```bash
npm test
```

All tests should pass, including new reconnection tests.

### 4.2 Manual Integration Test

Create a simple test script that:
1. Starts an SSE server
2. Connects a client
3. Restarts the server (new session ID)
4. Verifies client can still send messages

### 4.3 Test Against rmcp

```bash
# In one terminal: start hrmcp server
cd ~/src/halfremembered-mcp && cargo run

# In another: run test client using patched SDK
node test-reconnection.js
```

## Phase 5: Submit PR

### 5.1 Commit Changes

```bash
git add -A
git commit -m "fix(sse): handle session ID updates on reconnection

Previously, SSEClientTransport would fail after server restarts because:
1. The onerror handler rejected the start() Promise on any error
2. No sessionId property was exposed (unlike StreamableHTTPClientTransport)

This fix:
- Adds sessionId getter to extract session from endpoint URL
- Only rejects start() Promise for initial connection failures
- Properly handles endpoint updates on EventSource auto-reconnect
- Adds comprehensive tests for reconnection scenarios

Fixes #XXX"
```

### 5.2 Push and Create PR

```bash
git push fork fix/sse-reconnection-session-id
gh pr create --title "fix(sse): handle session ID updates on reconnection" \
  --body "..." \
  --repo modelcontextprotocol/typescript-sdk
```

## Success Criteria

- [ ] All existing tests pass
- [ ] New reconnection tests pass
- [ ] `sessionId` property works correctly
- [ ] Manual test with rmcp server works
- [ ] PR submitted and passes CI

## Risks & Mitigations

| Risk | Mitigation |
|------|------------|
| Breaking change for existing users | Keep backward-compatible; `onerror` still called |
| EventSource reconnect timing varies | Tests use generous timeouts; document behavior |
| SSEClientTransport is deprecated | Note in PR that this helps users still on SSE |

## Timeline Estimate

- Phase 1 (Setup): 15 minutes
- Phase 2 (Tests): 45 minutes
- Phase 3 (Fix): 30 minutes
- Phase 4 (Verify): 30 minutes
- Phase 5 (PR): 15 minutes

**Total: ~2-3 hours**
