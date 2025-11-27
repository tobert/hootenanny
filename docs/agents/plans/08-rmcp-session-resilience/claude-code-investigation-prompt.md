# Investigation Prompt: Claude Code MCP SSE Reconnection Bug

## Background

When an MCP server (using rmcp SSE transport) restarts, Claude Code successfully reconnects to the SSE endpoint and receives a new session ID, but continues using the **old session ID** for POST requests, causing HTTP 410 errors.

## Evidence from Logs

Location: `~/.cache/claude-cli-nodejs/-home-atobey-src-halfremembered-mcp/mcp-logs-hrmcp/`

```
"debug": "SSE connection dropped after 456s uptime"
"debug": "Connection error: SSE error: TypeError: terminated: other side closed"
[reconnection happens]
"debug": "Error POSTing to endpoint (HTTP 410): "
"debug": "Tool 'upload_file' failed after 0s: Error POSTing to endpoint (HTTP 410): "
```

## Expected Behavior

1. SSE stream closes (server restart)
2. Client reconnects to `/sse`
3. Receives new session ID in first SSE event: `event: endpoint, data: /message?sessionId=<NEW-ID>`
4. Client **updates** session ID for subsequent POST requests
5. Tools work normally

## Actual Behavior

1. ✅ SSE stream closes
2. ✅ Client reconnects to `/sse`
3. ✅ Receives new session ID
4. ❌ Client continues using **old** session ID for POST requests
5. ❌ Gets HTTP 410 GONE, tools fail

## Investigation Tasks

### 1. Find SSE Session Management Code

Search for:
- Where SSE endpoint event is parsed (`event: endpoint`)
- Where session ID is extracted from SSE data
- Where session ID is stored for POST requests
- Session ID update logic on reconnection

Likely locations:
- MCP client implementation
- SSE transport layer
- Session/connection management

### 2. Identify the Bug

Hypothesis: Session ID is captured once on initial connection and never updated on reconnect.

Look for:
```typescript
// Somewhere like this:
this.sessionId = parseSessionIdFromEndpointEvent(event);  // Sets once
// But on reconnect, this doesn't re-run or update the stored sessionId
```

### 3. Verify Fix Feasibility

Check if:
- Session ID is mutable/updatable
- Reconnection logic has access to update it
- There are any architectural reasons it can't be updated

### 4. Propose Fix

Expected fix pattern:
```typescript
// On SSE reconnect, when 'endpoint' event arrives:
onEndpointEvent(event) {
  const newSessionId = parseSessionId(event.data);
  this.updateSessionId(newSessionId);  // ← This might be missing
  logger.info('Updated session ID after reconnect', { old: this.sessionId, new: newSessionId });
}
```

## Context

- **rmcp behavior**: Server ALWAYS generates new session ID on each `/sse` connection
- **Server-side workaround**: Not easily possible without forking rmcp
- **Impact**: Multi-agent ensemble workflows break on server restart
- **Workaround**: Manual reconnect via `/mcp` command

## Success Criteria

1. Identify exact code location where session ID update should happen
2. Confirm whether this is a bug or intentional design
3. Assess fix complexity (one-line fix vs architectural change)
4. If fixable: create minimal reproduction case
5. If not fixable: document why and what alternatives exist

## Output

Please provide:
1. File paths and line numbers of relevant code
2. Current implementation (code snippets)
3. Root cause analysis
4. Proposed fix (if applicable)
5. Any caveats or concerns about the fix

---

**Goal**: Determine if this is fixable in Claude Code, or if we need to work around it server-side by forking rmcp.
