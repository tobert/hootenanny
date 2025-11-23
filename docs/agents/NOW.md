# NOW - Current Work Status

**Session**: 12 - Audio Graph MCP Planning
**Date**: 2025-11-25
**Status**: ✅ Complete Plan - Ready for Implementation

---

## 🎉 **Session 12: Audio Graph MCP Architecture & Planning**

### What We Accomplished
- ✅ **Reviewed architecture document** - Claude Opus's comprehensive design
- ✅ **Created project README** - Vision, quick start, philosophy
- ✅ **9 detailed task files** - Self-contained prompts for future sessions
- ✅ **Implementation plan** - From SQLite to full ensemble integration
- ✅ **Testing strategy** - Virtual devices, fixtures, CI/CD

### The Vision: 限界突破 (Limit Break)

**Core Insight**: Agents need to **see** the musical environment.

Before Audio Graph MCP:
- ❌ "Generate music" → Where does it play? Unknown.
- ❌ "Use the synth" → Which synth? Can't tell.
- ❌ Patch cables invisible to the system

After Audio Graph MCP:
- ✅ "What instruments are online?" → Queries live ALSA/PipeWire
- ✅ "Route to JD-Xi" → Agent confirms device available, sends MIDI
- ✅ "Trace signal to Bitbox" → Sees full path including patch cables

### Architecture Decision: Trustfall + Federation

**Why Trustfall?** Full 限界突破:
- GraphQL-style queries over heterogeneous data sources
- Joins live state (ALSA, PipeWire, USB) with persisted data (SQLite)
- Compile-time schema validation
- Perfect for agent exploration

**Federation Pattern**:
```
Live ALSA Devices → [Trustfall] ← SQLite Identities
     ↓                                    ↓
  Fingerprints  ← [Matcher] →  Identity Hints
                      ↓
                High-confidence match → Auto-bind
                Medium confidence → Ask user
```

### Project Structure

```
crates/audio-graph-mcp/
├── README.md                 ✅ Complete vision doc
├── src/
│   ├── db/                   📋 Task 01 - SQLite persistence
│   ├── sources/              📋 Task 02,06 - ALSA, PipeWire
│   ├── matcher.rs            📋 Task 03 - Identity matching
│   ├── adapter/              📋 Task 04 - Trustfall federation
│   ├── mcp_tools/            📋 Task 05 - Agent interface
│   └── schema.graphql
└── tests/
    ├── fixtures/             📋 Task 08 - Virtual devices
    └── integration_tests.rs
```

### Implementation Plan (9 Tasks)

**Foundation** (Days 1-3):
1. **Task 01**: SQLite schema - Identities, hints, tags, connections
2. **Task 02**: ALSA enumeration - MIDI device discovery
3. **Task 03**: Identity matching - Hint-based scoring algorithm

**Query Engine** (Days 4-6):
4. **Task 04**: Trustfall adapter - GraphQL federation (the magic!)
5. **Task 05**: MCP tools - `graph_query`, `graph_bind`, `graph_find`

**Expansion** (Days 7-9):
6. **Task 06**: PipeWire integration - Audio routing visibility
7. **Task 07**: Manual connections - Patch cable tracking
8. **Task 08**: Testing fixtures - Virtual MIDI devices
9. **Task 09**: Ensemble integration - Hootenanny awareness

### The Identity Problem (Solved)

**Challenge**: Hardware has fluid identity
- USB paths change: `hw:2,0` → `hw:3,0` after reboot
- MIDI names vary: "JD-Xi" vs "Roland JD-Xi MIDI 1"
- Devices get shelved, firmware updated

**Solution**: Multi-hint matching
```rust
Identity: "jdxi"
  └─ Hints:
       ├─ (usb_device_id, "0582:0160", confidence: 1.0)  // Strongest
       ├─ (midi_name, "JD-Xi", confidence: 0.9)
       └─ (alsa_card, "Roland JD-Xi", confidence: 0.8)

Live device fingerprints → Matcher → Score → Auto-bind (≥0.9) or Ask user
```

### Key Design Principles

1. **Live by default**: Query ALSA/PipeWire on-demand, not cached snapshots
2. **Persist only what we can't query**: Identity bindings, tags, patch cables
3. **Trustfall for federation**: Join live + persisted in GraphQL queries
4. **Organic, not transactional**: Graph is always "now"
5. **Agent-friendly**: Tools designed for LLM discovery

### Example Agent Workflows

**Discovery**:
```
Agent: find_instruments
→ Returns: JD-Xi, Flame 4VOX (both online via ALSA)
Agent: "I see two synths. Generating bass line for JD-Xi..."
```

**Eurorack Patching**:
```
User: "Patched Poly 2 CV out to Doepfer A-110"
Agent: graph_connect(poly2, cv_out_1, doepfer_a110, voct_in, patch_cable_cv)
→ Records connection in database
Agent: "Now when I send MIDI to Poly 2, I know it drives the VCO"
```

**Troubleshooting**:
```
User: "No audio from Bitbox"
Agent: graph_query(trace connections to bitbox)
→ Finds no connections
Agent: "No patch cables recorded to Bitbox. Did you connect something?"
```

### Testing Strategy

**Virtual Devices** (no hardware needed):
```bash
sudo modprobe snd-virmidi midi_devs=4
# Creates 4 virtual MIDI ports for testing
```

**Fixtures**:
- Pre-populated identities ("Virtual JD-Xi", "Virtual Keystep")
- Sample tags, hints, manual connections
- Full integration tests: enumeration → matching → query

**CI/CD**:
- GitHub Actions loads snd-virmidi
- Tests run with virtual devices
- No hardware dependency

### Documentation Deliverables

**Main README** (`crates/audio-graph-mcp/README.md`):
- Vision and quick start
- Architecture diagram
- Example queries
- Hardware context (Poly 2, JD-Xi, Eurorack)

**Task Files** (`docs/agents/plans/graph-mcp/tasks/`):
1. `task-01-sqlite-foundation.md` - Database layer
2. `task-02-alsa-enumeration.md` - MIDI discovery
3. `task-03-identity-matching.md` - Hint scoring
4. `task-04-trustfall-adapter.md` - GraphQL engine
5. `task-05-mcp-tools.md` - Agent interface
6. `task-06-pipewire-integration.md` - Audio routing
7. `task-07-manual-connections.md` - Patch cables
8. `task-08-testing-fixtures.md` - Virtual devices
9. `task-09-ensemble-integration.md` - Hootenanny

Each task:
- ✅ Self-contained context
- ✅ Clear goals and acceptance criteria
- ✅ Code examples (guidance, not full implementation)
- ✅ Testing strategy
- ✅ Out-of-scope items clearly marked

### What's Next (Session 13+)

**Pick any task and start building!** Each task file has everything needed:
- Background context
- Technical approach
- Acceptance criteria
- Test examples
- Integration points

**Suggested order**:
1. Task 01 (foundation) → Required for everything
2. Task 02 (ALSA) → Get live devices showing up
3. Task 03 (matcher) → Bind devices to identities
4. Task 04 (Trustfall) → The big integration
5. Task 05 (MCP tools) → Agent interface
6. Tasks 06-09 → Expand capabilities

**No rush** - Plan is solid, implementation can happen in parallel sessions.

---

## 📚 Previous Sessions Summary

### Session 11 - Custom Dynamic CLI Implementation ✅
- Custom argument parser (no Clap)
- Active discovery (no caching)
- Build succeeds, tests deferred
- 51 integration tests waiting

### Session 10 - Dynamic CLI Plan & Testing ✅
- Comprehensive plan for dynamic CLI
- Example shell scripts
- Pure Rust testing approach

### Sessions 6-9 - MCP Server Working ✅
- Fixed `tools/list` (notifications/initialized)
- All 11 musical tools registered
- Server runs on `http://127.0.0.1:8080`
- OpenTelemetry observability working

### Sessions 3-5 - Musical Domain ✅
- Event Duality system
- Conversation Tree with branching
- EmotionalVector 3D space
- Persistence with sled database

---

## 🚀 **Current State**

### What's Working
- ✅ MCP Server (`hootenanny`) - All tools registered
- ✅ Custom CLI Parser - Runtime schema parsing
- ✅ Musical Domain - Event Duality + Conversation Tree
- ✅ OpenTelemetry observability
- ✅ **Audio Graph MCP Plan** - Complete architecture and tasks

### What's Planned
- 📋 Audio Graph MCP - 9 tasks ready for implementation
- 📋 SQLite persistence for device identities
- 📋 ALSA/PipeWire enumeration
- 📋 Trustfall GraphQL queries
- 📋 Virtual device testing
- 📋 Ensemble integration

### What Needs Testing (hrcli)
- ⏳ Integration tests (need running server)
- ⏳ Parameter transformation (composite types)
- ⏳ Example shell scripts

---

## 🎯 **Next Session Priorities**

**Option A - Audio Graph MCP** (Recommended for new capability):
1. Start with Task 01 (SQLite foundation)
2. Implement schema, basic CRUD
3. Write tests with in-memory database
4. Foundation for all other tasks

**Option B - hrcli Testing**:
1. Start hootenanny server in test harness
2. Run all 51 integration tests
3. Fix any discovery/transformation issues

**Option C - Continue Either Project**:
- Pick any task file from audio-graph-mcp
- Each is self-contained and ready to execute

---

## 💡 **Key Insights**

### Planning > Rushing
- Spent full session on architecture review and task creation
- No code written, but future sessions will be **fast**
- Self-contained tasks = parallel work possible

### Trustfall is the Right Choice
- GraphQL power without running a server
- Federated queries across heterogeneous sources
- Compile-time safety for runtime-discovered devices

### Testing Without Hardware Works
- Virtual MIDI devices (snd-virmidi) = full stack testable
- CI/CD can run all tests
- Hardware testing comes later for validation

### The Identity Problem is Solvable
- Multi-hint matching with confidence scoring
- Auto-bind high confidence, ask user for medium
- Graceful degradation when hints change

### Rich Types Tell Stories
- Not `String`, but `Identity`, `HintKind`, `DeviceFingerprint`
- Compiler validates logic at build time
- Code reads like the domain model

---

**Last Updated**: 2025-11-25 by 🤖 Claude (Sonnet 4.5)
**Status**: Audio Graph MCP fully planned, ready for implementation
**Next Steps**: Pick a task (suggest Task 01) and start building
**Commit**: (pending - planning session, no code commits)
