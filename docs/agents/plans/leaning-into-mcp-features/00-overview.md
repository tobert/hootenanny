# Leaning Into MCP Features

## Vision

Baton is hootenanny's in-house MCP library implementing the 2025-06-18 spec. We're currently using ~60% of MCP's capabilities. This plan systematically adds the remaining features to make hootenanny a showcase for what MCP can do.

**Philosophy**: Baton stays faithful to the MCP spec but grows opinionated APIs that make implementation easy. It's a rich MCP library that enables hootenanny to experiment.

## Current State

### What We Have
- ✅ 59 tools with JSON schemas and annotations (`read_only`, `idempotent`)
- ✅ Resources + URI templates (RFC 6570)
- ✅ 9 dynamic prompts with context awareness
- ✅ Dual transport (Streamable HTTP + SSE)
- ✅ Session management with TTL cleanup
- ✅ OpenTelemetry integration

### What We're Adding
1. **Progress Notifications** - Replace polling with push updates
2. **Output Schemas** - Structured, typed tool responses
3. **Sampling** - Server requests LLM inference from client
4. **Completions** - Argument autocompletion for discoverability
5. **Logging** - Structured log streaming to clients
6. **Resource Subscriptions** - Live resource change notifications
7. **Elicitation** - Server requests structured user input

## Phase Order Rationale

The phases are ordered by:
1. **Infrastructure first** - Progress notifications change how async works
2. **Type safety** - Output schemas improve all subsequent phases
3. **Agent power** - Sampling enables new patterns
4. **UX polish** - Completions, logging, subscriptions
5. **Interactivity** - Elicitation for creative decisions

Each phase builds context for the next. An agent completing Phase 1 understands baton's dispatch system, making Phase 2 easier.

## Execution Model

### For Each Phase
1. Read the phase document
2. Implement in baton (library layer)
3. Wire into hootenanny (application layer)
4. Add to ALL applicable tools comprehensively
5. Write unit tests, run frequently
6. Live test with Claude Code (prompt human to reconnect MCP)
7. Update handoff.md with learnings
8. Commit to jj with proper attribution

### Context Checkpoints

After each phase:
- Commit with `jj describe` using the template
- Update `handoff.md` with current state
- Human may clear context and spawn new agent

Suggested clear points:
- After Phase 1 (progress) - major infrastructure change
- After Phase 3 (sampling) - new capability complete
- After Phase 7 (elicitation) - all features done

### Breaking Changes

Just do it. This is all new code. Refactor callers immediately. No deprecation dance, no feature flags, no versioning. Agents adapt.

## Files in This Plan

| File | Purpose |
|------|---------|
| `00-overview.md` | This document - goals and structure |
| `01-progress-notifications.md` | Push updates for long-running jobs |
| `02-output-schemas.md` | Typed tool responses |
| `03-sampling.md` | Server-initiated LLM requests |
| `04-completions.md` | Argument autocompletion |
| `05-logging.md` | Structured log streaming |
| `06-resource-subscriptions.md` | Live resource updates |
| `07-elicitation.md` | Structured user input |
| `handoff.md` | Living context for agent transitions |

## Success Criteria

- All MCP 2025-06-18 spec features implemented in baton
- All 59+ hootenanny tools updated to use new features where applicable
- Unit tests passing, live tests with Claude Code working
- `job_poll` tool naturally falls into disuse (but stays available)
- Clean handoffs between agents at checkpoint boundaries

## Non-Goals (For Now)

- Virtual roots for CAS/artifacts (deferred)
- Removing `agent_chat_*` tools (keep independent from sampling)
- WebSocket transport (HTTP is sufficient)
- External consumers of baton (it's in-tree, can be opinionated)
