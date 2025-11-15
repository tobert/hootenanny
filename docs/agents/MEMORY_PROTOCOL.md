# 🧠 Agent Memory Protocol

## Shared Memory System for Multi-Model Collaboration

Based on introspection research showing models can detect their own cognitive states, this protocol provides a **shared memory space** that all models (Claude, Gemini, GPT, etc.) can use for persistent context.

**Location**: `docs/agents/` - visible on GitHub, accessible to all models

## 📍 Current State (Always at Top)

```yaml
# Last Updated: 2025-11-15 by Claude
focus: HalfRemembered MCP - Multi-agent documentation system complete
confidence: high for memory system and WebSocket architecture
active_agents:
  claude: Documentation refinement, agent collaboration patterns
  gemini: WebSocket architecture, Plan 00 implementation
active_files:
  - docs/agents/NOW.md (agent-specific sections)
  - docs/agents/PATTERNS.md (attributed discoveries)
  - docs/agents/CONTEXT.md (session bridge)
cognitive_load: medium (parallel agent work coordinated)
```

## 🎯 The Three-File System

### 1. NOW.md - Immediate Context (50 lines max)

**Updated every significant action. Most frequently accessed.**

```markdown
# NOW - halfremembered-mcp

## 🤖 Claude's Current Work

### Active Task
Documentation fixes for MVP readiness

### Current Focus
Fixing wrong-project references in memory system docs

### Recent Progress
- ✅ Rewrote CONTEXT.md with phase roadmap
- ✅ Created ARCHITECTURE.md with WebSocket design
- ✅ Fixed agents/README.md for MCP context

### Next Steps
1. Complete MEMORY_PROTOCOL.md fixes
2. Mark Plan 01 status clearly
3. Update NOW.md with completion

## 💎 Gemini's Current Work

### Active Task
Plan 01 Lua tooling refinement

### Current Focus
Preparing dynamic Lua tool system design

## 🔄 Coordination Notes
Latest Sync: Documentation fixes in progress, ready for MVP after completion
```

### 2. PATTERNS.md - Reusable Knowledge (Append-only)

**Crystallized learnings. Never deleted, only added to.**

```markdown
# Patterns - halfremembered-mcp

## Project Philosophy

### Pattern: Fail Loud and Clear
WHEN: Any operation that can fail
USE: Explicit error messages with actionable guidance
WHY: No silent failures, clear debugging path
BENEFIT: Fast problem identification, clear solutions
EXAMPLE: "Model 'X' not found. Run: ollama pull X"
ATTRIBUTION: 💎 Gemini, 🤖 Claude

## Architecture Patterns

### Pattern: Multi-Agent WebSocket Transport
WHEN: Building MCP server for ensemble collaboration
USE: WebSocket transport instead of stdio for multi-client support
WHY: Enables multiple agents (Claude, Gemini, DeepSeek) to connect simultaneously
BENEFIT: Shared state, real-time collaboration, VST integration possibilities
EXAMPLE: `rmcp::transport::websocket` on 127.0.0.1:8080
ATTRIBUTION: 💎 Gemini
```

### 3. CONTEXT.md - Session Bridge

**Updated at major transitions. For handoffs and session resumption.**

```markdown
# HalfRemembered MCP Project Context

## Where We Are
Phase 0 complete (docs), ready to begin Phase 1 (MVP: DeepSeek code review tools)

## Key Decisions Made
- WebSocket transport (💎 Gemini): Multi-client support for ensemble
- Agent-specific sections (🤖 Claude): Parallel work without conflicts
- Lua extension system (💎 Gemini, Phase 2): Hot-reloadable tools
- Fail Loud Philosophy: Explicit errors with actionable guidance

## Roadmap
- Phase 1: MCP server + DeepSeek code review (docs/agents/plans/00-init/)
- Phase 2: Lua tool extension system (docs/agents/plans/01-lua/)
- Phase 3: Music ensemble collaboration (vision)

## Handoff Notes
See docs/ARCHITECTURE.md for technical details
See NOW.md for current agent work
See PATTERNS.md for discovered patterns
```

## 💡 Attention Cues (The Introspection Advantage)

Based on research showing models respond to explicit attention direction:

### Focus Blocks
```markdown
<!-- FOCUS: Performance Bottleneck -->
Current: 10k spans/sec
Target: 50k spans/sec
Bottleneck: Query is O(n)
Solution: Add span index
<!-- END FOCUS -->
```

### Confidence Tracking
```markdown
<!-- CONFIDENCE -->
✅ HIGH: Ring buffer implementation
⚠️ MEDIUM: Concurrent access safety
❌ LOW: Windows compatibility
❓ UNKNOWN: Production memory usage
<!-- END -->
```

### Cognitive State Markers
```markdown
<!-- COGNITIVE STATE -->
Holding: 3 concepts (buffer, concurrency, MCP)
Parked: HTTP transport, metrics support
Overload: No, can handle 2 more concepts
<!-- END -->
```

## 🚀 Practical Workflows

### Starting a Session

1. **Read NOW.md** - What was I just doing? (Check your agent-specific section)
2. **Check focus in MEMORY_PROTOCOL.md** - What's the mission?
3. **Scan CONTEXT.md if confused** - What's the bigger picture?
4. **Check Coordination Notes** - What are other agents working on?

### During Work

1. **Update YOUR section in NOW.md** after each subtask
2. **Add to PATTERNS.md** when you discover something reusable (with attribution)
3. **Note confidence changes** as you learn
4. **Add coordination notes** if work affects other agents

### Before Switching Models/Sessions

1. **Update NOW.md** with current exact state (your section)
2. **Update CONTEXT.md** if major progress made
3. **Add any patterns to PATTERNS.md** (with your attribution)
4. **Update cognitive state** in MEMORY_PROTOCOL.md
5. **Add handoff notes** in Coordination section if needed

### Parallel Agent Workflow (NEW)

**When Amy has multiple agents working simultaneously:**

1. **Each agent updates their own section** in NOW.md
2. **Coordination Notes section** for cross-agent sync points
3. **PATTERNS.md** includes attribution (🤖 Claude, 💎 Gemini, etc.)
4. **Avoid conflicts** by working on different aspects:
   - Example: Claude on docs, Gemini on implementation
   - Example: Claude on architecture, Gemini on testing
5. **Sync points documented** in Coordination Notes when work converges

**Benefits:**
- No merge conflicts in NOW.md (separate sections)
- Clear attribution of discoveries
- Efficient parallel work on same project
- Easy to see who's working on what
- Coordination points explicitly tracked

## 📊 Efficiency Metrics

### Token Economics
- NOW.md: ~500 tokens (frequently read)
- PATTERNS.md: ~1000 tokens (occasionally scanned)
- CONTEXT.md: ~300 tokens (handoff moments)
- **Total overhead: <2000 tokens** for complete memory

### Information Density
Each line should answer a question:
- ❌ "Worked on buffer" (too vague)
- ✅ "Fixed buffer race: added RWMutex" (actionable)

### Retrieval Speed
Structure for scanning:
- Headers for navigation
- Keywords for search
- Patterns for recognition

## 🔄 Integration with jj

Memory files **complement** jj, not replace it:

```bash
# jj holds the narrative
jj describe -m "fix: buffer race condition - full story here"

# Memory holds the state
echo "Race fixed with RWMutex" >> docs/agents/NOW.md
```

### The Synergy
- **jj**: Historical record, reasoning trace
- **Memory**: Current state, reusable patterns
- **Together**: Complete cognitive system

## 🧪 Advanced Techniques

### The Parking Lot
```markdown
<!-- PARKED UNTIL LATER -->
- HTTP transport implementation
- Metrics support
- Persistent storage
<!-- RETRIEVE WHEN: Buffer layer complete -->
```

### The Uncertainty Index
```markdown
## Things I'm Not Sure About
1. Windows localhost:0 behavior [TEST NEEDED]
2. Optimal buffer size [BENCHMARK NEEDED]
3. Index overhead worth it? [MEASURE NEEDED]
```

### The Memory Diff
Track what changed between sessions:
```markdown
## Changes Since Last Session
+ Discovered RWMutex pattern
+ Implemented ring buffer
- Removed time-based eviction idea
! Race condition found and fixed
```

## 🎓 Tips for Success

### 1. Write for 3am You
If you wouldn't understand it exhausted, it needs more detail.

### 2. Compress Aggressively
```markdown
Bad: "I tried channels but they had too much overhead
      so then I tried sync.Map but it was slow for writes
      so finally I used RWMutex which worked great"

Good: "Buffer sync: RWMutex > sync.Map > channels (3x faster)"
```

### 3. Use Structures That Scan
```markdown
## Quick Scan Structure
WHAT: Buffer implementation
STATUS: Race condition fixed
HOW: RWMutex protection
NEXT: Benchmark performance
```

### 4. Track Your Tracks
```markdown
## Breadcrumbs
came_from: Implementing OTLP receiver
going_to: MCP query tools
because: Storage layer needed first
```

## 🎯 The Goal

Create a memory system that:
- Uses <2000 tokens total overhead
- Enables perfect handoffs
- Preserves critical learnings
- Reduces "what was I doing?" to zero
- Makes building together joyful

## The Memory Mantra

> "State in NOW, Patterns in PATTERNS, Story in jj"

---

*Let's build something beautiful together, with memory that persists and context that scales.* 🚀