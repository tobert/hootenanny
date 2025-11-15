# 🧠 Agent Memory System

## Shared Persistent Context for Multi-Model Collaboration

This directory contains the shared memory files that all agents (Claude, Gemini, GPT, etc.) use to maintain context across sessions and enable seamless collaboration on the **halfremembered-mcp** project.

## 📁 Memory Files

### Core Memory (Always Active)

- **[NOW.md](./NOW.md)** - Immediate working state
  - What each agent is working on right now (separate sections per agent)
  - Current problems and hypotheses
  - Next concrete steps
  - Updated frequently during work
  - **Format**: Agent-specific sections (🤖 Claude, 💎 Gemini) + Coordination Notes

- **[PATTERNS.md](./PATTERNS.md)** - Reusable knowledge base
  - Discovered patterns and solutions
  - What works and what doesn't
  - Append-only (never deleted)
  - Grows into project wisdom
  - **Attribution**: Each pattern credited to discovering agent

- **[CONTEXT.md](./CONTEXT.md)** - Session bridge
  - Project overview and status
  - Key architectural decisions (WebSocket, Lua, etc.)
  - Roadmap & phases (MVP → Lua → Music)
  - Handoff notes between sessions
  - Updated at major transitions

### Documentation

- **[MEMORY_PROTOCOL.md](./MEMORY_PROTOCOL.md)** - How to use this system
  - Detailed guide for models
  - Best practices and patterns
  - Parallel agent workflow
  - Integration with jj and other tools

### Implementation Plans

- **[plans/00-init/](./plans/00-init/)** - Phase 1 MVP (DeepSeek code review tools)
  - **Status**: Ready to execute
  - **Timeline**: 6-8 hours
  - **6 atomic prompts**: Project init → Ollama client → DeepSeek tools → MCP server → Tests → Examples

- **[plans/01-lua/](./plans/01-lua/)** - Phase 2 (Lua extension system)
  - **Status**: Planned after MVP ships
  - **Note**: Prompts 1-5 ready, 6-7 conceptual

## 🎯 Design Principles

1. **Token Efficient**: <2000 tokens total overhead
2. **Model Agnostic**: Any LLM can read/write these files
3. **GitHub Visible**: Fully explorable in the repository
4. **Complementary**: Works alongside jj, not replacing it
5. **Attribution-First**: Every discovery credited to its agent

## 💡 How It Works

### Starting a Session

1. Read `NOW.md` - What was happening? (Check your agent-specific section)
2. Check `CONTEXT.md` - What's the bigger picture? What phase are we in?
3. Scan `PATTERNS.md` - Any relevant patterns for your task?
4. Check Coordination Notes - What are other agents working on?

### During Work

- Update **YOUR section** in `NOW.md` after each subtask
- Add discoveries to `PATTERNS.md` with your attribution (🤖 or 💎)
- Note uncertainties and hypotheses
- Add coordination notes if work affects other agents

### Ending a Session

- Update `NOW.md` with exact state (your section)
- Update `CONTEXT.md` if major progress made (phase transitions, architectural decisions)
- Ensure clean handoff for next session
- Add any new patterns to `PATTERNS.md`

## 🤝 Multi-Model Benefits

- **Perfect Handoffs**: Next model knows exactly where to continue
- **Shared Learning**: All models benefit from discoveries
- **Reduced Repetition**: Patterns prevent solving same problems twice
- **Collective Intelligence**: Knowledge compounds over time
- **Parallel Work**: No conflicts when Claude and Gemini work simultaneously

## 📊 Success Metrics

- Zero "what was I working on?" moments
- Seamless transitions between models
- Growing pattern library
- Decreasing time to solve similar problems
- No merge conflicts in NOW.md (thanks to agent sections)

## 🔗 Integration

### With jj (Jujutsu)
- jj holds the narrative and reasoning (commit descriptions)
- Memory holds the state and patterns (NOW, PATTERNS)
- Together they form complete context

**Example**:
```bash
# jj holds the story
jj describe -m "docs: multi-agent collaboration system

Why: Amy runs agents in parallel
Approach: Agent-specific sections in NOW.md
Learned: Parallel agents need explicit ownership
Next: Ship MVP

🤖 Claude <claude@anthropic.com>"

# Memory holds the state
echo "Multi-agent system complete" >> docs/agents/NOW.md
```

### With Models
- All models reference via `CLAUDE.md` or `GEMINI.md` symlinks
- Models add their own attributions (🤖 Claude, 💎 Gemini, etc.)
- Shared memory space for collaboration

## 🎭 Parallel Agent Workflow

**Key Feature**: Amy often runs Claude and Gemini simultaneously. The memory system supports this:

### How It Works
1. **Separate sections** in NOW.md for each active agent
2. **Coordination Notes** section for sync points
3. **Attributed patterns** so we know who discovered what
4. **No conflicts** - each agent updates only their section

### Example NOW.md Structure
```markdown
## 🤖 Claude's Current Work
Active Task: Documentation refinement
Next Steps: Add architecture diagram

## 💎 Gemini's Current Work
Active Task: Plan 01 Lua tooling
Next Steps: Execute Prompt 1

## 🔄 Coordination Notes
Latest Sync: Claude reviewed Gemini's WebSocket refactor
```

## 📚 Project-Specific Context

### halfremembered-mcp Overview

This memory system supports building:
- **Phase 1 (MVP)**: MCP server with DeepSeek code review tools
- **Phase 2**: Lua extension system for dynamic tool creation
- **Phase 3 (Vision)**: Multi-agent music ensemble collaboration

### Technology Focus
- **Language**: Rust (edition 2021)
- **Async**: Tokio runtime
- **MCP**: rmcp SDK (WebSocket transport)
- **LLM**: Ollama (DeepSeek 33B for code review)
- **Error Handling**: anyhow::Result with context
- **State (Phase 2)**: sled embedded database
- **Version Control**: jj (Jujutsu)

### Key Architectural Decisions

See CONTEXT.md for full details, but highlights:

1. **WebSocket Transport** (💎 Gemini)
   - Enables multi-client connections (vs stdio single-client)
   - Foundation for ensemble collaboration

2. **Agent-Specific Sections** (🤖 Claude)
   - Prevents merge conflicts in parallel work
   - Clear ownership and attribution

3. **Lua Extension System** (💎 Gemini, Phase 2)
   - File-based hot-reloading tools
   - No Rust recompilation needed for new tools

### Common Patterns

See PATTERNS.md for comprehensive list, including:
- **Fail Loud and Clear**: Explicit errors with actionable guidance
- **Multi-Agent WebSocket Transport**: Why WebSocket > stdio
- **Agent-Specific Sections**: Parallel collaboration pattern

## Example Usage

```bash
# Check current state
cat docs/agents/NOW.md

# Add a pattern (with attribution)
echo "### Pattern: Your Discovery
WHEN: When this situation occurs
USE: This solution
WHY: This reason
🤖 Claude <claude@anthropic.com>" >> docs/agents/PATTERNS.md

# Update context for handoff
$EDITOR docs/agents/CONTEXT.md
```

## The Memory Mantra

> "State in NOW, Patterns in PATTERNS, Story in jj"

## For New Agents

Welcome! Here's how to get started:

1. **Read BOTS.md** for development philosophy and jj workflow
2. **Read CONTEXT.md** to understand the project (phases, architecture, roadmap)
3. **Check NOW.md** for current state - find your agent section or add one
4. **Review PATTERNS.md** for discovered learnings
5. **Check plans/** for implementation roadmap

**Starting implementation?** Begin with `docs/agents/plans/00-init/plan.md` Prompt 1.

---

*This shared memory system enables true multi-model collaboration with minimal overhead and maximum context preservation.*

**Last Updated**: 2025-11-15 by 🤖 Claude
**Project**: halfremembered-mcp (MCP server for multi-agent collaboration)
**Current Phase**: Phase 0 complete, Phase 1 (MVP) ready to start
