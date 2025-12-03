# Handoff Document

**Plan**: Leaning Into MCP Features
**Status**: Planning complete, ready for implementation
**Last Updated**: 2025-12-03

## Current State

All 7 phase documents are written and ready for implementation:

| Phase | File | Status | Complexity |
|-------|------|--------|------------|
| 1 | 01-progress-notifications.md | Ready | High - infrastructure change |
| 2 | 02-output-schemas.md | Ready | Medium - all tools affected |
| 3 | 03-sampling.md | Ready | High - bidirectional MCP |
| 4 | 04-completions.md | Ready | Low-Medium - UX feature |
| 5 | 05-logging.md | Ready | Low - straightforward |
| 6 | 06-resource-subscriptions.md | Ready | Medium - notification system |
| 7 | 07-elicitation.md | Ready | Medium - user interaction |

## Recommended Execution Order

1. **Start with Phase 1** - Progress notifications touch the dispatch layer. Understanding this first makes everything else easier.

2. **Phase 2 is comprehensive** - You'll touch all 59 tools. Plan for this to take time.

3. **Checkpoint after Phase 3** - Sampling is the last major infrastructure piece. Clear context here.

4. **Phases 4-7 are incremental** - Can be done in order or parallelized.

## Key Files to Understand First

Before starting, read these to understand the current architecture:

```
crates/baton/src/protocol/mod.rs      # Dispatch and Handler trait
crates/baton/src/transport/mod.rs     # McpState and transports
crates/baton/src/session/mod.rs       # Session management
crates/hootenanny/src/api/handler.rs  # Tool definitions
crates/hootenanny/src/api/service.rs  # Tool implementations
```

## Testing Approach

1. **Unit tests**: Run frequently with `cargo test`
2. **Live tests**: Prompt human to rebuild and reconnect MCP
3. **Human involvement**: They can restart hootenanny at will

Human command to rebuild:
```bash
cargo build --release && systemctl --user restart hootenanny
```

Then reconnect in Claude Code.

## Context for Next Agent

### What This Plan Is

A systematic enhancement of the baton MCP library to support all MCP 2025-06-18 features. Baton is hootenanny's in-house MCP implementation.

### Why These Features Matter

- **Progress**: No more polling for job status
- **Output schemas**: Typed responses agents can parse
- **Sampling**: Server uses client's LLM inline
- **Completions**: Discoverability for 59+ tools
- **Logging**: Debug info without result clutter
- **Subscriptions**: Real-time multi-agent awareness
- **Elicitation**: Human-in-the-loop decisions

### What Stays the Same

- `agent_chat_*` tools remain (separate from sampling)
- `job_poll` tool stays available (but rarely used)
- Baton stays spec-compliant but grows opinionated helpers

### Breaking Changes Are OK

Just do it. Rewrite callers. No deprecation dance.

## Session Memory Update

If you complete a phase, update this file with:
- Which phase you completed
- Any surprises or learnings
- Specific files that changed
- Test status

## Commit Template

Use this for jj descriptions:

```
<type>: <what> - <why in 5 words>

Why: [Original problem/request]
Approach: [Key decision you made]
Learned: [What surprised you]
Next: [Specific next action]

Phase: leaning-into-mcp-features/<phase-number>

🤖 Claude <claude@anthropic.com>
```

## Questions for Human

If you get stuck, ask about:
- Whether a particular feature is needed for all tools
- Priority if running low on context
- Whether to skip optional features

## Success Looks Like

- All phases implemented
- Tests passing
- Live demos working
- Handoff updated with learnings

---

**Ready to begin.** Start with Phase 1: Progress Notifications.
