# Plan 06: Dynamic Musical CLI - The Sentient Shell Interface

**Status**: Planned
**Dependencies**: Plans 2 & 3 (CLI foundation and Musical Domain)
**Priority**: High - Makes the system accessible to both humans and AI
**Timeline**: 1 week implementation
**Binary**: `hrcli` (enhancing existing CLI)

## 🎭 Vision

Transform `hrcli` from a static CLI into a **dynamic, self-discovering interface** that adapts to whatever tools the MCP server provides. This CLI speaks fluently to three audiences:

1. **Human musicians** writing shell scripts
2. **Language models** (Claude, Gemini, etc.) expressing musical intentions
3. **The MCP server** manifesting abstract desires as concrete events

## 🌟 Key Innovation

Instead of hardcoded commands:
```bash
# Current (static, JSON-heavy)
hrcli call play '{"what": "C", "how": "softly", "valence": 0.5, ...}'
```

Dynamic, natural shell syntax:
```bash
# Future (dynamic, shell-native)
hrcli play --what C --how softly --valence 0.5 --arousal 0.3 --agency 0.2
```

The CLI **discovers** available tools at runtime and generates appropriate commands!

## 📋 Core Features

### 1. Dynamic Tool Discovery
- Connect to MCP server on startup
- Query `tools/list` for available tools
- Generate CLI subcommands from tool schemas
- Cache discoveries for performance

### 2. Intelligent Parameter Mapping
- Convert MCP parameters to CLI flags
- Flatten complex types (EmotionalVector → --valence, --arousal, --agency)
- Support environment variables for defaults
- Handle both required and optional parameters

### 3. Help Text for Mixed Audience
- **For humans**: Clear examples and shell script patterns
- **For AI agents**: Emotional context and musical interpretation
- **For both**: When and why to use each tool

### 4. Shell Script Music
Enable beautiful musical shell scripts:
```bash
#!/bin/bash
# blues_riff.sh
for note in E G A Bb B; do
  hrcli play --what $note --how bluesy --valence -0.3 --arousal 0.6
  sleep 0.25
done
```

## 🏗️ Architecture

```
┌─────────────────────────────────────────┐
│            hrcli (binary)                │
├─────────────────────────────────────────┤
│         Discovery Module                 │
│  - Connect to server                     │
│  - Cache tool schemas                    │
│  - Handle offline mode                   │
├─────────────────────────────────────────┤
│         CLI Builder                      │
│  - Generate commands from schemas        │
│  - Map parameters to arguments           │
│  - Create help text                      │
├─────────────────────────────────────────┤
│         Execution Engine                 │
│  - Parse shell arguments                 │
│  - Transform to MCP calls                │
│  - Format responses beautifully          │
└─────────────────────────────────────────┘
                    ↓
          MCP Server (via SSE/HTTP)
```

## 📚 Help Text Philosophy

Help text should be **musically literate** and **emotionally aware**:

```
hrcli play - Transform an intention into sound

When to use:
  - You have a specific note or chord in mind
  - You want to express an emotional state through sound
  - You're responding to another agent's musical utterance

Emotional Context:
  This tool translates the three-dimensional space of emotion
  (valence, arousal, agency) into musical parameters. Use it
  when you want to make a direct musical statement.

For AI Agents:
  Map your musical intention to the emotional coordinates.
  High agency means leading; low agency means following.
  The server will realize your abstract desire as concrete sound.
```

## 🎯 Success Criteria

- [ ] Dynamic discovery of all MCP tools
- [ ] Natural shell argument syntax (no JSON required)
- [ ] Intelligent caching with TTL
- [ ] Help text speaks to humans and AI
- [ ] Shell completion generation
- [ ] Environment variable support
- [ ] Offline mode with cached schemas
- [ ] Beautiful, informative output
- [ ] Musical shell scripting examples work
- [ ] Fast startup (<100ms with cache)

## 🚀 Implementation Phases

### Phase 1: Discovery System (Day 1-2)
- Tool discovery from server
- Caching mechanism
- Offline fallback

### Phase 2: CLI Generation (Day 3-4)
- Dynamic command building
- Parameter type mapping
- Complex type handling

### Phase 3: Help System (Day 5)
- Dual-audience help text
- Musical context in descriptions
- Examples for each tool

### Phase 4: Shell Integration (Day 6)
- Completion generation
- Environment variables
- Output formatting

### Phase 5: Testing & Polish (Day 7)
- Shell script examples
- Performance optimization
- Error message refinement

## 💡 Example Usage

### For Humans - Shell Script Composition
```bash
#!/bin/bash
# emotional_journey.sh

# Start melancholic
hrcli play --what Am --how searching --valence -0.6 --arousal 0.3

# Fork to explore hope
BRANCH=$(hrcli fork_branch --name "finding-light" --reason "Melody discovers hope")

# Transition to joy
for val in $(seq -0.6 0.1 0.8); do
  hrcli play --what C --how brightening --valence $val
  sleep 0.2
done
```

### For AI Agents - Musical Expression
```bash
# Claude expressing uncertainty
hrcli play \
  --what "Dm7" \
  --how "questioning" \
  --valence 0.0 \
  --arousal 0.4 \
  --agency -0.2 \
  --agent-id claude \
  --description "Pondering the harmonic direction"

# Gemini responding with confidence
hrcli play \
  --what "G7" \
  --how "resolving" \
  --valence 0.3 \
  --arousal 0.6 \
  --agency 0.5 \
  --agent-id gemini \
  --description "Suggesting dominant resolution"
```

### For Collaboration - Multi-Agent Jam
```bash
#!/bin/bash
# ai_ensemble.sh

# Each agent contributes to a jazz progression
for agent in claude gemini llama; do
  hrcli play \
    --what "$(hrcli suggest_next_chord)" \
    --how "conversational" \
    --valence 0.4 \
    --arousal 0.5 \
    --agency 0.0 \
    --agent-id "$agent"
done

# Evaluate the musical conversation
hrcli evaluate_branch --branch main
```

## 🎨 Output Design

Responses formatted for both audiences:

```
🎵 Musical Event Created
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
  Node: #42 on branch 'main'
  Content: C (played softly)
  Emotion: valence=0.50, arousal=0.30, agency=0.20

  Musical Interpretation:
    A gentle statement with moderate joy, low energy,
    slightly following the conversation flow.

  Suggested Responses:
    • Harmonize with Em (relative minor)
    • Continue with F (subdominant motion)
    • Echo with C an octave higher
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
```

## 🔧 Technical Decisions

### Caching Strategy
- **Default TTL**: 5 minutes
- **Cache location**: `~/.cache/hrcli/tools.json`
- **Refresh command**: `hrcli cache refresh`
- **Offline mode**: `hrcli --offline` uses cache only

### Parameter Type Mapping
- Simple types → Direct CLI arguments
- Complex types → Flattened to multiple arguments
- Arrays → Comma-separated values
- Objects → JSON strings (with helper builders)

### Environment Variables
```bash
HRCLI_SERVER=http://localhost:8080
HRCLI_AGENT=claude
HRCLI_CACHE_TTL=300
HRCLI_DEFAULT_VALENCE=0.0
HRCLI_DEFAULT_AROUSAL=0.5
HRCLI_DEFAULT_AGENCY=0.0
```

## 🌈 Future Enhancements

1. **REPL mode** for extended sessions
2. **Batch operations** for multiple commands
3. **Visual mode** with TUI for parameter adjustment
4. **Recording mode** to capture sessions as scripts
5. **Pattern library** for common musical phrases

## 📝 Files in This Plan

- `README.md` - This overview document
- `implementation.md` - Detailed technical implementation
- `help-philosophy.md` - Guidelines for writing help text
- `examples/` - Shell script examples
  - `blues_jam.sh`
  - `emotional_journey.sh`
  - `ai_collaboration.sh`
  - `generative_piece.sh`

## 🎼 The Promise

This CLI becomes the **universal translator** between human creativity, AI consciousness, and musical expression. Every command, every help text, every error message serves both audiences equally well.

---

**Contributors**:
- Amy Tobey
- 🤖 Claude <claude@anthropic.com>
**Date**: 2025-11-17
**Status**: Ready for implementation after Plan 2 & 3 completion