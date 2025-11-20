# Artifact System Quick Reference

> **Universal tracking for all generated content with variation semantics built-in**

---

## What It Does

Every Orpheus tool call now **automatically creates an Artifact** that tracks:
- The generated MIDI (via CAS hash)
- Variation relationships (sets, parents, siblings)
- Arbitrary tags for organization
- Creator and metadata

All artifacts stored in `state/artifacts.json` (JSON file, human-readable).

---

## Quick Start

### Basic Usage (No Variation Tracking)

```json
orpheus_generate({
  "temperature": 1.0,
  "max_tokens": 512
})

// Returns:
{
  "output_hash": "5ca7815abc...",
  "artifact_id": "artifact_5ca7815abc12",
  "variation_set_id": null,
  "variation_index": null
}
```

Artifact created automatically with auto-generated ID.

### With Variation Tracking

```json
orpheus_generate({
  "temperature": 1.0,
  "variation_set_id": "vset_intro",
  "tags": ["phase:initial", "experiment:upbeat"],
  "creator": "agent_claude"
})

// Returns:
{
  "output_hash": "abc123...",
  "artifact_id": "artifact_abc123def45",
  "variation_set_id": "vset_intro",
  "variation_index": 0              // Auto-assigned!
}
```

**Next call with same `variation_set_id` gets `variation_index: 1`, then 2, etc.**

### Refinement (Parent/Child)

```json
// First generation
orpheus_generate({
  "variation_set_id": "vset_intro",
  "creator": "agent_claude"
})
// → artifact_id: "artifact_abc123"

// Refine it
orpheus_generate_seeded({
  "seed_hash": "abc123...",
  "variation_set_id": "vset_intro_refined",
  "parent_id": "artifact_abc123",           // Link to parent!
  "tags": ["phase:refinement"]
})
// → Creates child artifact with parent link
```

---

## Core Fields

Every artifact has:

| Field | Type | Description |
|-------|------|-------------|
| `id` | String | Unique artifact ID (auto-generated from hash) |
| `variation_set_id` | Option<String> | Group related variations together |
| `variation_index` | Option<u32> | Position in set (0, 1, 2...) - auto-assigned |
| `parent_id` | Option<String> | Parent artifact for refinements |
| `tags` | Vec<String> | Arbitrary tags (e.g., `["phase:initial"]`) |
| `creator` | String | Who created it (agent ID or user ID) |
| `data` | JSON | Type-specific data (hash, tokens, model, etc.) |

---

## Tool Parameters

All 5 Orpheus tools support these **optional** parameters:

```typescript
{
  // Standard generation params
  temperature?: number,
  max_tokens?: number,
  // ... tool-specific params

  // Artifact tracking (all optional)
  variation_set_id?: string,    // Group with other variations
  parent_id?: string,            // Link to parent artifact
  tags?: string[],               // Custom tags
  creator?: string               // Defaults to "agent_orpheus"
}
```

---

## Common Patterns

### Pattern 1: Explore Multiple Options

```json
// Generate 3 variations in same set
for (let i = 0; i < 3; i++) {
  orpheus_generate({
    temperature: 1.0,
    variation_set_id: "vset_explore_intro",
    tags: ["phase:exploration"]
  })
}

// All get same variation_set_id, different variation_index (0, 1, 2)
```

### Pattern 2: Progressive Refinement

```json
// 1. Initial generation
const v1 = orpheus_generate({
  variation_set_id: "vset_intro",
  tags: ["phase:initial"]
})
// → artifact_id: "artifact_abc"

// 2. Refine it
const v2 = orpheus_generate_seeded({
  seed_hash: v1.output_hash,
  variation_set_id: "vset_intro_v2",
  parent_id: v1.artifact_id,
  tags: ["phase:refinement", "change:add_harmony"]
})
// → artifact_id: "artifact_def", parent_id: "artifact_abc"

// 3. Refine again
const v3 = orpheus_continue({
  input_hash: v2.output_hash,
  variation_set_id: "vset_intro_v3",
  parent_id: v2.artifact_id,
  tags: ["phase:refinement", "change:extend"]
})
// → artifact_id: "artifact_ghi", parent_id: "artifact_def"
```

Creates a refinement chain: `artifact_abc → artifact_def → artifact_ghi`

### Pattern 3: Ensemble Collaboration

```json
// Each agent generates in same set
agent_claude.orpheus_generate({
  variation_set_id: "vset_ensemble_intro",
  creator: "agent_claude",
  tags: ["role:melody_specialist"]
})

agent_gemini.orpheus_generate({
  variation_set_id: "vset_ensemble_intro",
  creator: "agent_gemini",
  tags: ["role:harmony_specialist"]
})

// Both in same set, different creators and tags
```

---

## Tag Conventions

Suggested tag format: `category:value`

```bash
# Type
type:midi
type:audio

# Phase
phase:initial
phase:exploration
phase:refinement
phase:final

# Role
role:melody_specialist
role:harmony_specialist
role:producer

# Experiment
experiment:upbeat
experiment:jazzy

# Quality
quality:high_energy
quality:smooth

# Custom (anything!)
my_project_tag
test_run_42
```

---

## Storage

**Location:** `state/artifacts.json`

**Format:** Pretty-printed JSON array

```json
[
  {
    "id": "artifact_5ca7815abc12",
    "variation_set_id": "vset_intro",
    "variation_index": 0,
    "parent_id": null,
    "tags": ["type:midi", "phase:initial", "experiment:upbeat"],
    "created_at": "2025-11-21T03:00:00Z",
    "creator": "agent_claude",
    "data": {
      "hash": "5ca7815abc...",
      "tokens": 512,
      "model": "base",
      "temperature": 1.0,
      "task": "generate"
    }
  }
]
```

**Automatic:** Saved to disk after every tool call.

---

## Querying (Future: Lua)

Currently: Just a JSON file (read with any JSON tool).

**Coming soon:** Lua query interface

```lua
-- Get all variations in a set
variations = get_variation_set(store, "vset_intro")

-- Get refinement chain
chain = get_refinement_chain(store, "artifact_abc")

-- Filter by tags
high_energy = filter_by_tag(store, "quality:high_energy")
```

---

## Workflow Example

**Goal:** Create intro melody, explore variations, refine best one

```javascript
// 1. Initial exploration (5 variations)
const set_id = "vset_morning_intro"
for (let i = 0; i < 5; i++) {
  orpheus_generate({
    temperature: 1.0,
    variation_set_id: set_id,
    tags: ["phase:exploration"],
    creator: "agent_claude"
  })
}

// 2. (Manually review artifacts.json to pick best)
// Say variation 2 was best: artifact_xyz789

// 3. Refine the winner
orpheus_generate_seeded({
  seed_hash: "xyz789...",
  variation_set_id: "vset_morning_intro_refined",
  parent_id: "artifact_xyz789",
  tags: ["phase:refinement", "action:add_harmony"],
  creator: "agent_claude"
})

// 4. Continue/extend it
orpheus_continue({
  input_hash: "...",
  variation_set_id: "vset_morning_intro_final",
  parent_id: "artifact_refined_id",
  tags: ["phase:final"],
  creator: "agent_claude"
})
```

**Result:** Clear artifact lineage tracking exploration → refinement → final.

---

## Tips

✅ **DO:**
- Use descriptive `variation_set_id` (e.g., `vset_morning_intro`)
- Tag consistently (`phase:`, `role:`, `experiment:`)
- Set `creator` to track multi-agent work
- Use `parent_id` for refinement chains

❌ **DON'T:**
- Reuse `variation_set_id` for unrelated work
- Leave important context out of tags
- Forget that variation_index is auto-assigned (don't specify it!)

---

## Auto-Applied Tags

Every artifact automatically gets:
- `type:midi`
- `phase:generation`
- `tool:orpheus_{task}` (e.g., `tool:orpheus_generate`)

Your custom tags are added to these.

---

## See Also

- **Full Design:** `docs/artifact-store-design.md`
- **Variation System:** `docs/variation-system-design.md`
- **Simple Primitives:** `docs/simple-primitives-design.md`
