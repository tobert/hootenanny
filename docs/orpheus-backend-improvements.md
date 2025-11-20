# Orpheus Backend Improvements Roadmap

> Design document for enhancing the Orpheus litserve backend to enable more powerful agent-driven music composition

**Status:** Planning
**Created:** 2025-11-21
**Goal:** Make Orpheus tools more powerful for agent collaboration and creative workflows

## Context

After implementing and testing the new task-specific Orpheus MCP tools, we identified several opportunities to enhance the backend service. This document outlines improvements prioritized by impact and feasibility.

## Testing Observations

Current behavior (as of 2025-11-21):
- ✅ `orpheus_generate` - Working well (generated 511 tokens)
- ⚠️ `orpheus_generate_seeded` - Returns 0 tokens
- ⚠️ `orpheus_continue` - Returns 0 tokens
- ❌ `orpheus_bridge` - HTTP 410 (model unavailable)
- ⚠️ `orpheus_loops` - No output returned

## 🎯 High Impact Improvements

### 1. True Multi-Variation Support

**Priority:** HIGH
**Effort:** Medium
**Impact:** Enables parallel exploration of creative directions

**Current State:**
- `num_variations` parameter exists in API
- Unclear if it actually generates multiple variations
- Only single hash returned

**Proposed Change:**
Return array of variations in single API call.

**Request:**
```json
{
  "task": "generate",
  "temperature": 1.0,
  "num_variations": 3
}
```

**Response:**
```json
{
  "status": "success",
  "variations": [
    {
      "output_hash": "abc123def456...",
      "tokens": 512,
      "variation_id": 0
    },
    {
      "output_hash": "def456ghi789...",
      "tokens": 498,
      "variation_id": 1
    },
    {
      "output_hash": "ghi789jkl012...",
      "tokens": 523,
      "variation_id": 2
    }
  ],
  "summary": "Generated 3 variations (avg 511 tokens)"
}
```

**Benefits:**
- Agents can explore multiple creative directions without separate API calls
- Better for ensemble workflows where agents vote on variations
- More efficient use of GPU time

**Implementation Notes:**
- Run model inference with different random seeds
- Generate all variations before returning (or consider streaming)
- Store all variations in CAS before returning hashes

---

### 2. Streaming Token Generation

**Priority:** HIGH
**Effort:** High
**Impact:** Better UX for long generations, enables progress feedback

**Current State:**
- Client waits for entire generation to complete
- No visibility into progress
- No way to cancel long-running generations

**Proposed Change:**
Add streaming endpoint with Server-Sent Events (SSE).

**New Endpoint:**
```
POST /generate/stream
```

**Response Stream:**
```json
{"type": "start", "task": "generate", "estimated_tokens": 512}
{"type": "progress", "tokens_so_far": 128, "percent": 25}
{"type": "progress", "tokens_so_far": 256, "percent": 50}
{"type": "progress", "tokens_so_far": 384, "percent": 75}
{"type": "complete", "output_hash": "abc123...", "total_tokens": 512}
```

**Benefits:**
- Real-time progress feedback to users
- Early stopping for bad generations
- Better observability for debugging
- Can show partial MIDI as it generates

**Implementation Notes:**
- Use SSE or WebSocket for streaming
- Buffer tokens in chunks (e.g., every 128 tokens)
- Support cancellation via separate endpoint
- Consider streaming partial MIDI for preview

---

### 3. MIDI Metadata in Response

**Priority:** HIGH
**Effort:** Medium
**Impact:** Enables intelligent decision-making without MIDI parsing

**Current State:**
- Responses only return CAS hash
- Agents must download and parse MIDI to understand content
- No quick way to compare generations

**Proposed Change:**
Include structural analysis in response.

**Response:**
```json
{
  "status": "success",
  "output_hash": "abc123def456...",
  "tokens": 512,
  "metadata": {
    "duration_seconds": 45.2,
    "estimated_key": "C major",
    "tempo_bpm": 120,
    "time_signature": "4/4",
    "instruments": ["piano", "strings", "bass"],
    "note_count": 847,
    "note_density": "medium",
    "pitch_range": {
      "lowest": "C2",
      "highest": "C6"
    },
    "energy_profile": {
      "start": 0.4,
      "peak": 0.8,
      "end": 0.3
    },
    "structure": {
      "sections": 3,
      "has_repetition": true
    }
  }
}
```

**Benefits:**
- Agents can make decisions without downloading MIDI
- Quick filtering/sorting of generations
- Better for ensemble voting ("I want high energy")
- Useful for bridge generation (match tempo/key)

**Implementation Notes:**
- Use music21, mido, or custom parser
- Cache analysis results
- Make metadata fields configurable (don't compute everything)
- Consider exposing as separate `/analyze` endpoint

---

## 🔧 Medium Impact Improvements

### 4. Proper Bridge Implementation

**Priority:** MEDIUM
**Effort:** High (requires ML work)
**Impact:** Unlocks agent-driven composition workflows

**Current State:**
- Bridge endpoint returns HTTP 410
- `section_b_hash` parameter ignored
- No actual transition logic

**Proposed Change:**
Implement true bridge generation that connects two sections.

**Algorithm:**
```python
def generate_bridge(section_a_midi, section_b_midi=None, params):
    # 1. Analyze section_a ending
    ending_analysis = analyze_ending(section_a_midi)
    # - final notes/chord
    # - key
    # - tempo
    # - energy level

    # 2. If section_b provided, analyze its beginning
    if section_b_midi:
        beginning_analysis = analyze_beginning(section_b_midi)
        # - opening notes/chord
        # - key (may differ from section_a)
        # - tempo (may differ)
        # - target energy

    # 3. Generate transition
    bridge = model.generate_bridge(
        from_state=ending_analysis,
        to_state=beginning_analysis if section_b_midi else None,
        length=params.max_tokens
    )

    # 4. Validate transition quality
    # - smooth key changes
    # - tempo transitions
    # - no jarring jumps

    return bridge
```

**Benefits:**
- Agents can compose multi-section pieces
- Enable medleys, mashups, transitions
- Key feature for collaborative composition

**Implementation Notes:**
- May require fine-tuning bridge model
- Consider using transformer with conditioning on both endpoints
- Provide hints: "modulate to relative minor", "fade out", etc.

---

### 5. Generation Constraints/Controls

**Priority:** MEDIUM
**Effort:** Medium (depends on model capabilities)
**Impact:** Fine-grained creative control

**Current State:**
- Only temperature and top_p for control
- No way to specify what should vary
- Limited style control

**Proposed Change:**
Add semantic constraint controls.

**Request:**
```json
{
  "task": "generate",
  "seed_hash": "abc123...",
  "temperature": 0.9,
  "constraints": {
    "preserve_melody": true,
    "vary_harmony": true,
    "vary_rhythm": false,
    "target_energy": 0.7,
    "instrument_hints": ["piano", "strings"],
    "style_hints": ["classical", "flowing"],
    "avoid_jumps": true,
    "density": "sparse"
  }
}
```

**Benefits:**
- More intentional variation generation
- Better for iterative refinement
- Agents can request specific changes
- Preserve what works, vary what doesn't

**Implementation Notes:**
- Start with model-supported constraints
- Use classifier guidance for some constraints
- May require control tokens in model
- Document which constraints are supported per model

---

### 6. Batch Processing Endpoint

**Priority:** MEDIUM
**Effort:** Low
**Impact:** Efficient parallel exploration

**Current State:**
- One request = one generation
- Agents must make multiple serial requests
- No request batching

**Proposed Change:**
Add batch endpoint for parallel operations.

**Request:**
```json
POST /batch

{
  "operations": [
    {
      "id": "gen1",
      "task": "continue",
      "input_hash": "abc...",
      "params": {"max_tokens": 256}
    },
    {
      "id": "gen2",
      "task": "generate_seeded",
      "seed_hash": "def...",
      "params": {"temperature": 0.8}
    },
    {
      "id": "gen3",
      "task": "continue",
      "input_hash": "ghi...",
      "params": {"max_tokens": 256}
    }
  ]
}
```

**Response:**
```json
{
  "results": [
    {"id": "gen1", "output_hash": "...", "tokens": 256},
    {"id": "gen2", "output_hash": "...", "tokens": 512},
    {"id": "gen3", "output_hash": "...", "tokens": 243}
  ],
  "summary": "Completed 3 operations in 4.2s"
}
```

**Benefits:**
- More efficient GPU utilization
- Lower latency for exploration workflows
- Single HTTP round-trip for multiple operations

**Implementation Notes:**
- Use model batching if supported
- Process in parallel where possible
- Return results in same order as requests
- Consider max batch size limits

---

## 💡 Nice to Have

### 7. MIDI Structure Analysis Endpoint

**Priority:** LOW
**Effort:** Low
**Impact:** Helps agents understand MIDI content

**Proposal:**
```
POST /analyze

{
  "midi_input_hash": "abc123...",
  "analyses": ["structure", "motifs", "harmony", "rhythm"]
}
```

**Response:**
```json
{
  "structure": {
    "form": "ABA",
    "sections": [
      {"label": "A", "measures": "1-8"},
      {"label": "B", "measures": "9-16"},
      {"label": "A", "measures": "17-24"}
    ]
  },
  "motifs": [
    {
      "notes": ["C", "E", "G"],
      "occurrences": 5,
      "measures": [1, 3, 17, 19, 23]
    }
  ],
  "harmony": {
    "key": "C major",
    "chord_progression": ["I", "V", "vi", "IV"],
    "modulations": []
  },
  "rhythm": {
    "time_signature": "4/4",
    "tempo": 120,
    "syncopation_level": "low"
  }
}
```

---

### 8. Model Introspection Endpoint

**Priority:** LOW
**Effort:** Low
**Impact:** Better error messages, dynamic capability discovery

**Proposal:**
```
GET /models

{
  "models": {
    "base": {
      "status": "available",
      "capabilities": ["generate", "continue"],
      "max_tokens": 1024,
      "supported_constraints": ["temperature", "top_p"]
    },
    "bridge": {
      "status": "available",
      "capabilities": ["bridge"],
      "max_tokens": 512,
      "requires": ["section_a"],
      "optional": ["section_b"]
    },
    "loops": {
      "status": "available",
      "capabilities": ["loops"],
      "max_tokens": 1024
    },
    "classifier": {
      "status": "available",
      "capabilities": ["classify"]
    }
  },
  "version": "1.0.0"
}
```

**Benefits:**
- Agents discover capabilities dynamically
- Better error messages
- Version compatibility checking

---

### 9. Better Error Messages

**Priority:** LOW
**Effort:** Low
**Impact:** Improved debugging and recovery

**Current State:**
- Generic HTTP status codes
- Minimal error context

**Proposed Change:**
Structured errors with recovery hints.

**Error Response:**
```json
{
  "error": "generation_failed",
  "message": "Model produced invalid MIDI sequence",
  "details": {
    "tokens_generated": 45,
    "failure_point": "note_sequence_validation",
    "invalid_reason": "pitch_out_of_range"
  },
  "suggestions": [
    "Try lower temperature (< 1.0)",
    "Reduce max_tokens to 512 or less",
    "Use a different seed"
  ],
  "request_id": "req_abc123",
  "timestamp": "2025-11-21T02:30:00Z"
}
```

**Error Types:**
- `invalid_request` - Bad parameters
- `model_unavailable` - Model not loaded/broken
- `generation_failed` - Model produced bad output
- `timeout` - Generation took too long
- `rate_limited` - Too many requests

---

## 🏆 Recommended Implementation Order

If implementing all changes, suggested order:

1. **MIDI Metadata in Response** (HIGH, Medium effort)
   - Immediate value, enables intelligent decisions
   - Foundation for other features

2. **True Multi-Variation Support** (HIGH, Medium effort)
   - Big UX win
   - Relatively straightforward

3. **Better Error Messages** (LOW, Low effort)
   - Quick win, improves debugging

4. **Model Introspection** (LOW, Low effort)
   - Quick win, enables dynamic capabilities

5. **Generation Constraints/Controls** (MEDIUM, Medium effort)
   - Depends on model capabilities
   - High creative value

6. **Batch Processing** (MEDIUM, Low effort)
   - Good performance optimization
   - Simple to implement

7. **Proper Bridge Implementation** (MEDIUM, High effort)
   - Requires ML work
   - High creative value when done

8. **Streaming Token Generation** (HIGH, High effort)
   - Complex but valuable
   - Consider after core features stable

9. **MIDI Structure Analysis** (LOW, Low effort)
   - Nice complement to metadata

---

## Success Metrics

How to measure if improvements are successful:

### Quantitative
- Reduction in API calls per composition session
- Increase in successful generations (fewer errors)
- Reduction in average time-to-good-result
- GPU utilization improvement (with batching)

### Qualitative
- Agent collaboration workflows become possible
- Composition quality improves
- Fewer "I can't do that" scenarios
- Better error recovery

---

## Open Questions

1. **Multi-variation**: Should variations be deterministic (same seed + params = same variations)?
2. **Streaming**: What's the right chunk size for progress updates?
3. **Metadata**: Which analyses are cheap enough to always compute?
4. **Bridge**: How to handle key/tempo mismatches between sections?
5. **Constraints**: Which constraints are feasible without retraining?
6. **Batch**: What's a reasonable max batch size?

---

## Related Documents

- `docs/new-orpheus-tools.md` - MCP tools redesign (completed)
- `docs/agents/local-models-api.md` - Current API documentation
- `docs/CAS_HTTP_API.md` - Content addressable storage API

---

## Appendix: Example Workflows Enabled

### Workflow 1: Parallel Exploration
```
Agent: Generate 5 variations of this melody
  → Single API call with num_variations=5
  → Receive 5 hashes + metadata back
  → Compare metadata (energy, key, density)
  → Download only the best 2 for detailed analysis
```

### Workflow 2: Progressive Refinement
```
Agent: Generate base melody
  → Review metadata, seems too sparse
Agent: Generate again with constraints
  → {preserve_melody: true, density: "medium"}
  → Gets denser variation
Agent: Perfect! Now continue it
  → Use continue endpoint
```

### Workflow 3: Composition Assembly
```
Agent: Generate intro (low energy)
  → Check metadata: energy=0.3, key="C major"
Agent: Generate bridge to chorus
  → Bridge from intro_hash to chorus_hash
  → Smooth transition generated
Agent: Generate outro
  → Uses constraints: {target_energy: 0.1, fade_out: true}
```

### Workflow 4: Ensemble Voting
```
Agent A: Generate 3 variations
  → All agents receive metadata
Agent B: I vote for variation 2 (highest energy)
Agent C: I vote for variation 1 (better harmony)
  → Majority vote, download winner only
```
