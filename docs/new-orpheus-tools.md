# Orpheus MCP Tools Redesign

## Executive Summary

This document outlines a complete redesign of the Orpheus MCP tools, splitting the monolithic `orpheus_generate` tool into task-specific, ergonomic tools that map 1:1 with the Orpheus API capabilities. The redesign prioritizes clarity, discoverability, and correct usage patterns while maintaining the existing CAS abstraction.

## Current State Analysis

### Existing Architecture

**Current Tool:** `orpheus_generate`
- Single tool handles all tasks via a `task` parameter
- Parameters: `model`, `task`, `input_hash`, `temperature`, `top_p`, `max_tokens`
- Tasks: `generate`, `continue`, `classify`, `bridge`, `loops`

**Problems:**
1. **Poor Discoverability**: Users must know all possible task values
2. **Unclear Parameter Requirements**: Which parameters are required for which tasks?
3. **Type Safety**: No compile-time validation of task-specific parameter combinations
4. **Inconsistent Naming**: `orpheus_classify` exists separately, breaking the pattern
5. **Missing Functionality**: `bridge` task requires `midi_a` parameter but current design uses `input_hash` (aliasing issue)
6. **Unused API Features**: `num_variations` parameter not exposed

### Orpheus API Capabilities

From `/docs/agents/local-models-api.md`:

**Tasks:**
- `generate` - Generates music from scratch or from seed
- `continue` - Continues a given MIDI sequence
- `classify` - Determines if MIDI is human or AI-generated
- `bridge` - Generates a musical bridge connecting two sections
- `loops` - Generates multi-instrumental loops

**Parameters:**
- `model` - Model variant (base, classifier, bridge, loops, children, mono_melodies)
- `task` - Operation to perform
- `midi_input` - Base64 MIDI (required for continue, bridge, classify)
- `temperature` - Randomness control (0.0-2.0, default 1.0)
- `top_p` - Nucleus sampling (0.0-1.0, default 0.95)
- `max_tokens` - Token limit (default 1024)
- `midi_a` - Base64 MIDI for bridge task start section
- `num_variations` - Number of variations to generate (default 1)

## Proposed Design

### New Tool Structure

Split into 6 task-specific tools that directly map to API capabilities:

1. **`orpheus_generate`** - Generate music from scratch
2. **`orpheus_generate_seeded`** - Generate with a seed MIDI
3. **`orpheus_continue`** - Continue existing MIDI
4. **`orpheus_bridge`** - Create bridge between sections
5. **`orpheus_loops`** - Generate loops
6. **`orpheus_classify`** - Classify MIDI (keep existing)

### Tool Definitions

#### 1. orpheus_generate

**Description:** Generate music from scratch using Orpheus.

**Parameters:**
- `model`: Option<String> - Model variant (default: "base")
  - Options: "base", "children", "mono_melodies"
- `temperature`: Option<f32> - Randomness (0.0-2.0, default: 1.0)
- `top_p`: Option<f32> - Nucleus sampling (0.0-1.0, default: 0.95)
- `max_tokens`: Option<u32> - Max tokens (default: 1024)
- `num_variations`: Option<u32> - Number of variations (default: 1)

**Returns:** `OrpheusGenerateResult`
- `status`: "success"
- `output_hash`: CAS hash of generated MIDI
- `summary`: Description (e.g., "Generated 150 tokens")

**Example:**
```json
{
  "model": "base",
  "temperature": 1.2,
  "max_tokens": 512
}
```

**Implementation Notes:**
- Calls API with `task: "generate"`, no `midi_input`
- Appropriate models: base, children, mono_melodies

#### 2. orpheus_generate_seeded

**Description:** Generate music using a seed MIDI as inspiration.

**Parameters:**
- `seed_hash`: String - CAS hash of seed MIDI (required)
- `model`: Option<String> - Model variant (default: "base")
- `temperature`: Option<f32> - Randomness (0.0-2.0, default: 1.0)
- `top_p`: Option<f32> - Nucleus sampling (0.0-1.0, default: 0.95)
- `max_tokens`: Option<u32> - Max tokens (default: 1024)
- `num_variations`: Option<u32> - Number of variations (default: 1)

**Returns:** `OrpheusGenerateResult`

**Example:**
```json
{
  "seed_hash": "5ca7815abc...",
  "model": "base",
  "temperature": 0.8
}
```

**Implementation Notes:**
- Calls API with `task: "generate"`, `midi_input: <seed>`
- Seed provides context/inspiration but isn't strictly continued

#### 3. orpheus_continue

**Description:** Continue an existing MIDI sequence.

**Parameters:**
- `input_hash`: String - CAS hash of MIDI to continue (required)
- `model`: Option<String> - Model variant (default: "base")
- `temperature`: Option<f32> - Randomness (0.0-2.0, default: 1.0)
- `top_p`: Option<f32> - Nucleus sampling (0.0-1.0, default: 0.95)
- `max_tokens`: Option<u32> - Max tokens (default: 1024)
- `num_variations`: Option<u32> - Number of variations (default: 1)

**Returns:** `OrpheusGenerateResult`

**Example:**
```json
{
  "input_hash": "5ca7815abc...",
  "max_tokens": 256
}
```

**Implementation Notes:**
- Calls API with `task: "continue"`, `midi_input: <hash>`
- Directly continues the input sequence

#### 4. orpheus_bridge

**Description:** Generate a musical bridge connecting two sections. Currently treats the input as context to continue from.

**Parameters:**
- `section_a_hash`: String - CAS hash of first section MIDI (required)
- `section_b_hash`: Option<String> - CAS hash of second section (future use)
- `model`: Option<String> - Model variant (default: "bridge")
- `temperature`: Option<f32> - Randomness (0.0-2.0, default: 1.0)
- `top_p`: Option<f32> - Nucleus sampling (0.0-1.0, default: 0.95)
- `max_tokens`: Option<u32> - Max tokens (default: 1024)

**Returns:** `OrpheusGenerateResult`

**Example:**
```json
{
  "section_a_hash": "5ca7815abc...",
  "model": "bridge",
  "temperature": 1.0
}
```

**Implementation Notes:**
- Calls API with `task: "bridge"`, `midi_a: <section_a>`
- Per API docs: "Currently treats midi_input (or midi_a) as the context to continue from"
- `section_b_hash` is optional for future API evolution
- Recommend "bridge" model variant

#### 5. orpheus_loops

**Description:** Generate multi-instrumental loops.

**Parameters:**
- `seed_hash`: Option<String> - CAS hash of seed MIDI (optional)
- `model`: Option<String> - Model variant (default: "loops")
- `temperature`: Option<f32> - Randomness (0.0-2.0, default: 1.0)
- `top_p`: Option<f32> - Nucleus sampling (0.0-1.0, default: 0.95)
- `max_tokens`: Option<u32> - Max tokens (default: 1024)
- `num_variations`: Option<u32> - Number of variations (default: 1)

**Returns:** `OrpheusGenerateResult`

**Example:**
```json
{
  "model": "loops",
  "num_variations": 3
}
```

**Implementation Notes:**
- Calls API with `task: "loops"`
- Seed is optional
- Recommend "loops" model variant

#### 6. orpheus_classify

**Description:** Classify MIDI as human or AI-composed. (Keep existing implementation)

**Parameters:**
- `input_hash`: String - CAS hash of MIDI to classify (required)
- `model`: Option<String> - Model variant (default: "classifier")

**Returns:** `OrpheusClassifyResult`
- `is_human`: bool
- `confidence`: f32 (0.0-1.0)
- `probabilities`: HashMap<String, f32>

**Example:**
```json
{
  "input_hash": "5ca7815abc..."
}
```

**Implementation Notes:**
- Keep existing implementation
- Calls API with `task: "classify"`

## Rationale

### Why Task-Specific Tools?

1. **Discoverability**: Tools appear in tool listings with clear names
2. **Documentation**: Each tool has focused, relevant documentation
3. **Type Safety**: Parameter requirements enforced at tool boundary
4. **Validation**: Task-specific validation logic is clearer
5. **Ergonomics**: Users don't need to remember task string values
6. **Evolution**: Easy to add task-specific parameters in the future

### Design Decisions

**Model Parameter Defaults:**
- Each tool recommends appropriate models in description
- Base model is safe default for most generation tasks
- Task-specific models (bridge, loops, classifier) are suggested

**Parameter Naming:**
- `input_hash` - For continuation/classification (follows existing pattern)
- `seed_hash` - For seeded generation (clarifies intent)
- `section_a_hash`, `section_b_hash` - For bridge (domain clarity)

**num_variations Support:**
- Added to generation tools
- Exposes API functionality previously hidden
- Enables exploration workflows

**Error Messages:**
- Task-specific validation with helpful error messages
- Example: "orpheus_continue requires input_hash" vs generic "invalid params"

## Implementation Plan

### Phase 1: Add New Request Types (server.rs)

Create new request structs for each tool with comprehensive schema descriptions.

### Phase 2: Update LocalModels (local_models.rs)

Add support for `num_variations` parameter to `OrpheusGenerateParams`.

### Phase 3: Add Tool Methods (server.rs)

Implement each tool with appropriate validation and tracing instrumentation.

### Phase 4: Shared Validation Helpers

Add shared validation logic for temperature and top_p parameters.

### Phase 5: Remove Old Tool

Delete the old monolithic `orpheus_generate` tool method and struct.

## Migration Notes

### Breaking Changes

This is a complete rewrite with NO backwards compatibility:

**Old:**
```json
{
  "model": "base",
  "task": "continue",
  "input_hash": "abc123",
  "temperature": 1.0
}
```

**New:**
```json
// Call orpheus_continue instead
{
  "input_hash": "abc123",
  "temperature": 1.0
}
```

### Migration Path

Users must update their code to use the new tool names:

| Old Call | New Tool | Notes |
|----------|----------|-------|
| `orpheus_generate(task="generate")` | `orpheus_generate()` | Scratch generation |
| `orpheus_generate(task="generate", input_hash=...)` | `orpheus_generate_seeded()` | With seed |
| `orpheus_generate(task="continue", input_hash=...)` | `orpheus_continue()` | Direct continuation |
| `orpheus_generate(task="bridge", input_hash=...)` | `orpheus_bridge()` | Bridge generation |
| `orpheus_generate(task="loops")` | `orpheus_loops()` | Loop generation |
| `orpheus_classify(...)` | `orpheus_classify()` | No change |

## Testing Approach

### Integration Tests

Test complete workflows for each tool:

1. **Generate Workflow:** Generate → store to CAS → inspect → verify
2. **Continue Workflow:** Store MIDI → continue → verify continuation
3. **Bridge Workflow:** Store sections → bridge → verify output
4. **Classification Workflow:** Store MIDI → classify → verify result
5. **Variations:** Generate with num_variations → verify multiple outputs

### Parameter Validation Tests

- Temperature out of range (negative, >2.0)
- top_p out of range (negative, >1.0)
- Missing required parameters (input_hash, section_a_hash)
- Invalid CAS hashes

## Summary

This redesign transforms the Orpheus MCP tools from a monolithic interface into a suite of focused, ergonomic tools that map 1:1 with API capabilities. The new design prioritizes:

- **Clarity**: Tool names describe exactly what they do
- **Type Safety**: Parameter requirements enforced at boundaries
- **Discoverability**: Tools appear in listings with rich documentation
- **Correctness**: Task-specific validation prevents errors
- **Evolution**: Easy to extend with new capabilities

**Key Insight:** The bridge task currently uses `midi_a` parameter in the API but our abstraction treats it as `midi_input`. The new `section_a_hash` naming makes this explicit and leaves room for `section_b_hash` when the API evolves.
