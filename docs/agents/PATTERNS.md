# Patterns - halfremembered-mcp

> Discovered patterns will be added here as we build the project.
> This file is append-only - patterns are never deleted, only refined.
>
> **Attribution**: Patterns include attribution (🤖 Claude, 💎 Gemini) to track learnings from each agent.

---

## Project Philosophy

### Pattern: Fail Loud and Clear
WHEN: Any operation that can fail
USE: Explicit error messages with actionable guidance
WHY: No silent failures, clear debugging path
BENEFIT: Fast problem identification, clear solutions
EXAMPLE: "Model 'X' not found. Run: ollama pull X"
APPLIES: All error handling in this project
DISCOVERED: Initial project philosophy
ATTRIBUTION: 💎 Gemini, 🤖 Claude

---

## Architecture Patterns

### Pattern: Multi-Agent WebSocket Transport
WHEN: Building MCP server for ensemble collaboration
USE: WebSocket transport instead of stdio for multi-client support
WHY: Enables multiple agents (Claude, Gemini, DeepSeek) to connect simultaneously
BENEFIT: Shared state, real-time collaboration, VST integration possibilities
EXAMPLE: `rmcp::transport::websocket` on 127.0.0.1:8080
GOTCHA: Need to consider authentication/TLS if exposing beyond localhost
DISCOVERED: 2025-11-15, WebSocket refactor
ATTRIBUTION: 💎 Gemini

---

## Documentation Patterns

### Pattern: Agent-Specific Sections in NOW.md
WHEN: Multiple agents working in parallel on same project
USE: Separate sections in NOW.md for each active agent (🤖 Claude, 💎 Gemini)
WHY: Prevents conflicts, tracks concurrent work, enables true parallel collaboration
BENEFIT: Each agent can update their section without overwriting others' work
EXAMPLE: See docs/agents/NOW.md structure
COORDINATION: "Coordination Notes" section for sync points
DISCOVERED: 2025-11-15, multi-agent workflow design
ATTRIBUTION: 🤖 Claude

---

## Domain Model Patterns

### Pattern: Event Duality
WHEN: Modeling musical intentions vs concrete sounds
USE: Dual event types - Abstract (intentions) and Concrete (realizations)
WHY: Separates what agents want from how it's achieved
BENEFIT: Multiple realization strategies, clean agent-system boundary
EXAMPLE: `Intention::Play{what: "C", how: "softly"}` → `Sound{pitch: 60, velocity: 40}`
DISCOVERED: 2025-11-15, core architecture design
ATTRIBUTION: 🤖 Claude, 💎 Gemini (collaborative)

### Pattern: Conversation Trees with Temporal Forking
WHEN: Agents need to explore musical ideas without commitment
USE: Git-like branching structure for musical conversations
WHY: Enables parallel exploration, natural conflict resolution
BENEFIT: Non-destructive experimentation, emergent consensus
EXAMPLE: `conversation.fork("try_jazzy_harmony")` then merge if it works
DISCOVERED: 2025-11-15, multi-agent collaboration design
ATTRIBUTION: 🤖 Claude, 💎 Gemini (enhanced with structured reasons)

### Pattern: Capability-Based Agent Delegation
WHEN: Agents need specialized help (bass lines, drum patterns, etc.)
USE: Request queue with capability registry
WHY: Agents have different strengths, should collaborate not compete
BENEFIT: Heterogeneous agent ecosystem, graceful degradation
EXAMPLE: Claude requests bass from BassBot, falls back to simple pattern if timeout
DISCOVERED: 2025-11-15, agent collaboration patterns
ATTRIBUTION: 🤖 Claude

## Testing Patterns

### Pattern: Test Musical Behavior, Not Implementation
WHEN: Writing tests for musical systems
USE: Tests that express musical intent and verify musical outcomes
WHY: Implementation details change, musical goals remain
BENEFIT: Tests remain valid through refactoring, readable by musicians
EXAMPLE: `agents_can_explore_parallel_musical_ideas()` not `test_fork_function()`
DISCOVERED: 2025-11-15, test-driven development approach
ATTRIBUTION: 🤖 Claude

## (Additional patterns will be added as we discover them during implementation)
