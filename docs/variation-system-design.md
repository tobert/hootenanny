# Variation System Design: A First-Class Exploration Primitive

> **Vision:** Variations as a fundamental organizing principle for agent-driven creative work, optimized for LLM reasoning and collaborative decision-making

**Status:** Design Phase
**Created:** 2025-11-21
**Authors:** Claude, Amy

---

## Executive Summary

We're designing **variations** as a first-class concept throughout HalfRemembered, not just a backend feature. A variation system provides:

- **For Agents:** Structured exploration of creative possibility spaces
- **For Humans:** Clear comparison and selection interfaces
- **For Systems:** Efficient storage and retrieval of related artifacts

This design ensures consistency across HTTP APIs, MCP tools, CLI interfaces, and data structures, optimized specifically for LLM/agent workflows.

---

## Core Insight

**Variation ≠ Just Multiple Outputs**

A variation system is about *relationships* and *exploration*:
- **Flat List:** `[item1, item2, item3]` - no context, hard to reason about
- **Variation Set:** Rich structure with provenance, relationships, and decision support

Agents excel when they can:
1. See the "why" behind each variation
2. Compare along meaningful dimensions
3. Build on selections (variations of variations)
4. Collaborate through voting/ranking
5. Trace decision lineage

---

## 1. Conceptual Model

### 1.1 Core Entities

```
VariationSet
├─ metadata (provenance, intent, parameters)
├─ parent_ref (optional - if this is refinement of another)
├─ variations[]
│  ├─ VariationItem
│  │  ├─ artifact (CAS hash)
│  │  ├─ metadata (comparison dimensions)
│  │  ├─ score (optional - from voting/ranking)
│  │  └─ refinements (child VariationSets)
│  └─ ...
└─ selection_state (voting, ranking, chosen)
```

### 1.2 Key Properties

**VariationSet:**
- Unique ID (`vset_abc123...`)
- Created timestamp
- Creator (agent_id or user_id)
- Intent (natural language description)
- Parameters (what stayed constant)
- Variation dimensions (what changed)
- Parent set (if refinement)

**VariationItem:**
- Position in set (0, 1, 2, ...)
- Unique ID within set (`vset_abc123/var_0`)
- Artifact reference (CAS hash)
- Rich metadata (for comparison)
- Scores/votes (from agents/users)
- Child variation sets (refinements)

### 1.3 Relationships

```
Exploration Tree:

VariationSet_A (generate melody)
├─ variation_0 (upbeat)
│  └─ VariationSet_A0 (refine upbeat)
│     ├─ variation_0 (more piano)
│     └─ variation_1 (add strings)
├─ variation_1 (melancholy)
└─ variation_2 (jazzy)
   └─ VariationSet_A2 (refine jazzy)
      ├─ variation_0 (slower tempo)
      └─ variation_1 (faster tempo)
```

This enables:
- **Breadth:** Explore options at same level
- **Depth:** Refine promising options
- **Backtracking:** Try different refinement paths
- **Merging:** Combine best aspects

---

## 2. Data Structure Design

### 2.1 Storage Schema

**Optimized for:**
- Fast retrieval by set_id
- Efficient metadata queries
- Clear provenance chains
- LLM-friendly JSON serialization

```rust
// Domain model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariationSet {
    /// Unique identifier (vset_<blake3>)
    pub id: VariationSetId,

    /// When this set was created
    pub created_at: DateTime<Utc>,

    /// Who created it (agent or human)
    pub creator: CreatorId,

    /// Natural language intent
    pub intent: String,

    /// Operation that created this set
    pub operation: Operation,

    /// Parameters held constant
    pub parameters: serde_json::Value,

    /// What dimensions varied
    pub variation_dimensions: Vec<String>,

    /// Parent set if this is a refinement
    pub parent: Option<VariationReference>,

    /// The variations in this set
    pub variations: Vec<VariationItem>,

    /// Selection/voting state
    pub selection_state: SelectionState,

    /// Tags for organization
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariationItem {
    /// Position in set (0-indexed)
    pub index: u32,

    /// Unique ID within set
    pub id: VariationItemId,

    /// The actual artifact (CAS hash)
    pub artifact_hash: String,

    /// MIME type of artifact
    pub artifact_type: String,

    /// Rich metadata for comparison
    pub metadata: ArtifactMetadata,

    /// Scores from voting/ranking
    pub scores: HashMap<String, f32>,

    /// Child refinement sets
    pub refinements: Vec<VariationSetId>,

    /// Agent annotations
    pub annotations: Vec<Annotation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariationReference {
    /// Parent set ID
    pub set_id: VariationSetId,

    /// Specific variation in parent (if applicable)
    pub variation_index: Option<u32>,

    /// Why we're refining this
    pub refinement_reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectionState {
    /// Current voting scores
    pub votes: HashMap<u32, Vec<Vote>>,

    /// Chosen variation (if selected)
    pub chosen: Option<u32>,

    /// Ranking (if ranked)
    pub ranking: Option<Vec<u32>>,

    /// Selection reasoning
    pub selection_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vote {
    pub voter: CreatorId,
    pub variation_index: u32,
    pub score: f32,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Annotation {
    pub author: CreatorId,
    pub timestamp: DateTime<Utc>,
    pub content: String,
    pub annotation_type: AnnotationType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AnnotationType {
    Comment,
    Question,
    Suggestion,
    Critique,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Operation {
    pub tool: String,
    pub task: String,
    pub parameters: serde_json::Value,
}
```

### 2.2 CAS Integration

**Two-Layer Storage:**

1. **Artifacts in CAS:** Actual MIDI files, audio, etc.
   - Immutable
   - Content-addressed
   - Deduplicated automatically

2. **VariationSets in Database:** Metadata and relationships
   - Queryable
   - Mutable (voting, annotations)
   - References CAS hashes

```
Storage Layout:

/cas/
  5ca7815.../     # MIDI artifact
  abc1234.../     # MIDI artifact
  def5678.../     # MIDI artifact

/variations/
  vset_xyz123/    # VariationSet metadata
    metadata.json
    votes.json
    annotations.json
```

---

## 3. HTTP API Design

### 3.1 REST Endpoints

**Create Variation Set:**
```http
POST /api/variations

{
  "intent": "Explore upbeat melody variations",
  "operation": {
    "tool": "orpheus_generate",
    "task": "generate",
    "parameters": {
      "temperature": 1.0,
      "max_tokens": 512
    }
  },
  "num_variations": 5,
  "variation_dimensions": ["random_seed"],
  "parent": {
    "set_id": "vset_parent123",
    "variation_index": 2,
    "refinement_reason": "Refine the jazzy variation with more piano"
  },
  "creator": "agent_claude_001"
}
```

**Response:**
```http
201 Created

{
  "id": "vset_abc123def456",
  "created_at": "2025-11-21T03:00:00Z",
  "creator": "agent_claude_001",
  "intent": "Explore upbeat melody variations",
  "operation": {...},
  "parameters": {...},
  "variation_dimensions": ["random_seed"],
  "parent": {...},
  "variations": [
    {
      "index": 0,
      "id": "vset_abc123def456/var_0",
      "artifact_hash": "5ca7815abc...",
      "artifact_type": "audio/midi",
      "metadata": {
        "duration_seconds": 45.2,
        "tempo_bpm": 128,
        "key": "C major",
        "energy": 0.72,
        "instruments": ["piano", "strings"],
        "tokens": 512
      },
      "scores": {},
      "refinements": [],
      "annotations": []
    },
    {
      "index": 1,
      "id": "vset_abc123def456/var_1",
      "artifact_hash": "def456ghi...",
      "artifact_type": "audio/midi",
      "metadata": {
        "duration_seconds": 43.8,
        "tempo_bpm": 132,
        "key": "G major",
        "energy": 0.81,
        "instruments": ["piano", "bass", "drums"],
        "tokens": 498
      },
      "scores": {},
      "refinements": [],
      "annotations": []
    },
    // ... 3 more variations
  ],
  "selection_state": {
    "votes": {},
    "chosen": null,
    "ranking": null
  },
  "tags": []
}
```

**Get Variation Set:**
```http
GET /api/variations/{set_id}

# Returns full VariationSet with all metadata
```

**List Variation Sets:**
```http
GET /api/variations?creator=agent_claude_001&limit=20&offset=0

# Query parameters:
# - creator: Filter by creator
# - parent: Filter by parent set
# - tags: Filter by tags
# - created_after: Time filter
# - has_selection: Only sets with chosen variation
# - sort: created_at, num_variations, vote_count
```

**Vote on Variation:**
```http
POST /api/variations/{set_id}/vote

{
  "voter": "agent_gemini_002",
  "variation_index": 1,
  "score": 0.85,
  "reason": "High energy matches the intent, good key choice"
}
```

**Annotate Variation:**
```http
POST /api/variations/{set_id}/variations/{index}/annotate

{
  "author": "agent_claude_001",
  "content": "This variation has excellent rhythm but could use more harmonic complexity",
  "annotation_type": "Critique"
}
```

**Choose Variation:**
```http
POST /api/variations/{set_id}/choose

{
  "variation_index": 1,
  "reason": "Highest energy and voted best by ensemble",
  "chooser": "agent_conductor_001"
}
```

**Compare Variations:**
```http
GET /api/variations/{set_id}/compare?dimensions=energy,tempo,key

# Returns structured comparison along specified dimensions
{
  "set_id": "vset_abc123",
  "comparison": {
    "energy": {
      "var_0": 0.72,
      "var_1": 0.81,
      "var_2": 0.65,
      "var_3": 0.78,
      "var_4": 0.70
    },
    "tempo": {
      "var_0": 128,
      "var_1": 132,
      "var_2": 120,
      "var_3": 130,
      "var_4": 125
    },
    "key": {
      "var_0": "C major",
      "var_1": "G major",
      "var_2": "A minor",
      "var_3": "D major",
      "var_4": "C major"
    }
  },
  "recommendations": {
    "highest_energy": 1,
    "lowest_energy": 2,
    "fastest_tempo": 1,
    "most_common_key": "C major"
  }
}
```

**Refine Variation:**
```http
POST /api/variations/{set_id}/variations/{index}/refine

{
  "intent": "Add more harmonic complexity while preserving energy",
  "operation": {
    "tool": "orpheus_generate_seeded",
    "parameters": {
      "seed_hash": "def456ghi...",
      "temperature": 0.8,
      "constraints": {
        "preserve_melody": true,
        "vary_harmony": true
      }
    }
  },
  "num_variations": 3
}

# Creates new VariationSet with parent reference
```

### 3.2 GraphQL Alternative

For complex queries, offer GraphQL:

```graphql
type VariationSet {
  id: ID!
  createdAt: DateTime!
  creator: Creator!
  intent: String!
  operation: Operation!
  parameters: JSON!
  variationDimensions: [String!]!
  parent: VariationReference
  variations: [VariationItem!]!
  selectionState: SelectionState!
  tags: [String!]!

  # Computed fields
  topVoted: VariationItem
  averageScore: Float
  refinementCount: Int
  explorationDepth: Int
}

type VariationItem {
  index: Int!
  id: ID!
  artifactHash: String!
  artifactType: String!
  metadata: ArtifactMetadata!
  scores: [Score!]!
  refinements: [VariationSet!]!
  annotations: [Annotation!]!

  # Computed
  averageScore: Float
  voteCount: Int
  hasRefinements: Boolean
}

query ExploreVariations {
  variationSet(id: "vset_abc123") {
    intent
    variations {
      index
      metadata {
        energy
        tempo
        key
      }
      averageScore
      annotations {
        content
        author {
          name
        }
      }
      refinements {
        id
        intent
        topVoted {
          artifactHash
          metadata {
            energy
          }
        }
      }
    }
  }
}
```

---

## 4. MCP Tool Design

### 4.1 New MCP Tools

**create_variation_set:**
```json
{
  "name": "create_variation_set",
  "description": "Create a set of variations to explore creative possibilities",
  "inputSchema": {
    "type": "object",
    "properties": {
      "intent": {
        "type": "string",
        "description": "Natural language description of exploration goal"
      },
      "operation": {
        "type": "object",
        "description": "Tool and parameters to generate variations"
      },
      "num_variations": {
        "type": "integer",
        "default": 3,
        "description": "Number of variations to create"
      },
      "variation_dimensions": {
        "type": "array",
        "items": {"type": "string"},
        "description": "What will vary (e.g., random_seed, temperature)"
      },
      "parent_variation": {
        "type": "object",
        "description": "If refining an existing variation"
      }
    },
    "required": ["intent", "operation"]
  }
}
```

**Example Usage by Agent:**
```
I want to explore upbeat melody variations.

Let me create a variation set:

create_variation_set({
  intent: "Explore upbeat melodies for intro section",
  operation: {
    tool: "orpheus_generate",
    parameters: {
      temperature: 1.0,
      max_tokens: 512,
      constraints: {target_energy: 0.7}
    }
  },
  num_variations: 5,
  variation_dimensions: ["random_seed"]
})
```

**get_variation_set:**
```json
{
  "name": "get_variation_set",
  "description": "Retrieve a variation set with all metadata for analysis",
  "inputSchema": {
    "type": "object",
    "properties": {
      "set_id": {"type": "string"},
      "include_artifacts": {"type": "boolean", "default": false},
      "include_refinements": {"type": "boolean", "default": true}
    }
  }
}
```

**compare_variations:**
```json
{
  "name": "compare_variations",
  "description": "Get structured comparison of variations along specific dimensions",
  "inputSchema": {
    "type": "object",
    "properties": {
      "set_id": {"type": "string"},
      "dimensions": {
        "type": "array",
        "items": {"type": "string"},
        "description": "Metadata fields to compare (e.g., energy, tempo, key)"
      },
      "format": {
        "type": "string",
        "enum": ["table", "json", "natural_language"],
        "default": "natural_language"
      }
    }
  }
}
```

**vote_on_variation:**
```json
{
  "name": "vote_on_variation",
  "description": "Vote for a variation in a set with optional reasoning",
  "inputSchema": {
    "type": "object",
    "properties": {
      "set_id": {"type": "string"},
      "variation_index": {"type": "integer"},
      "score": {"type": "number", "minimum": 0, "maximum": 1},
      "reason": {"type": "string"}
    }
  }
}
```

**choose_variation:**
```json
{
  "name": "choose_variation",
  "description": "Select a variation as the chosen one, optionally explaining why",
  "inputSchema": {
    "type": "object",
    "properties": {
      "set_id": {"type": "string"},
      "variation_index": {"type": "integer"},
      "reason": {"type": "string"}
    }
  }
}
```

**refine_variation:**
```json
{
  "name": "refine_variation",
  "description": "Create a new variation set refining a specific variation",
  "inputSchema": {
    "type": "object",
    "properties": {
      "parent_set_id": {"type": "string"},
      "parent_variation_index": {"type": "integer"},
      "intent": {"type": "string"},
      "operation": {"type": "object"},
      "num_variations": {"type": "integer", "default": 3}
    }
  }
}
```

### 4.2 Updated Orpheus Tools

Modify existing tools to support variation sets:

**orpheus_generate (updated):**
```json
{
  "inputSchema": {
    "properties": {
      "temperature": {...},
      "max_tokens": {...},

      // NEW: Variation support
      "num_variations": {
        "type": "integer",
        "default": 1,
        "description": "Number of variations to generate"
      },
      "create_variation_set": {
        "type": "boolean",
        "default": false,
        "description": "If true, create a VariationSet instead of returning individual hash"
      },
      "variation_set_intent": {
        "type": "string",
        "description": "Required if create_variation_set is true"
      }
    }
  }
}
```

**Response (when create_variation_set=true):**
```json
{
  "type": "variation_set",
  "set_id": "vset_abc123",
  "intent": "Explore upbeat melodies",
  "num_variations": 5,
  "variations": [
    {
      "index": 0,
      "artifact_hash": "5ca7815...",
      "metadata": {...}
    },
    // ...
  ],
  "summary": "Created variation set with 5 melodies (avg 510 tokens)"
}
```

**Response (when create_variation_set=false, legacy):**
```json
{
  "status": "success",
  "output_hash": "5ca7815...",
  "tokens": 512
}
```

---

## 5. CLI Design

### 5.1 Interactive Selection

```bash
$ hrmcp variations create \
    --intent "Explore upbeat melodies" \
    --tool orpheus_generate \
    --num-variations 5 \
    --params '{"temperature": 1.0, "max_tokens": 512}'

Creating variation set...
✓ Generated 5 variations

Variation Set: vset_abc123
Intent: Explore upbeat melodies

┌─────┬──────────────┬────────┬───────┬──────────┬────────┐
│ Idx │ Hash         │ Energy │ Tempo │ Key      │ Tokens │
├─────┼──────────────┼────────┼───────┼──────────┼────────┤
│ 0   │ 5ca7815...   │ 0.72   │ 128   │ C major  │ 512    │
│ 1   │ def456g...   │ 0.81   │ 132   │ G major  │ 498    │
│ 2   │ abc1234...   │ 0.65   │ 120   │ A minor  │ 503    │
│ 3   │ 9876543...   │ 0.78   │ 130   │ D major  │ 515    │
│ 4   │ fedcba9...   │ 0.70   │ 125   │ C major  │ 510    │
└─────┴──────────────┴────────┴───────┴──────────┴────────┘

Recommendations:
  • Highest energy: #1 (0.81)
  • Best tempo match: #1 (132 bpm)

Commands:
  hrmcp variations show vset_abc123        # View details
  hrmcp variations compare vset_abc123     # Compare side-by-side
  hrmcp variations vote vset_abc123 1      # Vote for variation 1
  hrmcp variations choose vset_abc123 1    # Select variation 1
  hrmcp variations refine vset_abc123 1    # Refine variation 1
  hrmcp variations play vset_abc123 1      # Play variation 1
```

**Compare View:**
```bash
$ hrmcp variations compare vset_abc123 --dimensions energy,tempo,key

Comparing 5 variations along 3 dimensions:

Energy Distribution:
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
#0 ████████████████████████████████████ 0.72
#1 ████████████████████████████████████████ 0.81 ⭐ HIGHEST
#2 ████████████████████████████████ 0.65
#3 ██████████████████████████████████████ 0.78
#4 ███████████████████████████████████ 0.70

Tempo Distribution:
#0 ████████████████████████████████████ 128 bpm
#1 █████████████████████████████████████ 132 bpm ⭐ FASTEST
#2 ██████████████████████████████████ 120 bpm
#3 ████████████████████████████████████ 130 bpm
#4 ███████████████████████████████████ 125 bpm

Key Distribution:
  C major: #0, #4
  G major: #1
  A minor: #2
  D major: #3

Most common: C major (2 occurrences)
```

**Interactive Selection:**
```bash
$ hrmcp variations choose vset_abc123 --interactive

┌──────────────────────────────────────────────────────┐
│ Choose a variation from set vset_abc123              │
├──────────────────────────────────────────────────────┤
│ Intent: Explore upbeat melodies                      │
└──────────────────────────────────────────────────────┘

  ○ Variation 0 │ Energy: 0.72 │ 128 bpm │ C major
  ◉ Variation 1 │ Energy: 0.81 │ 132 bpm │ G major  ⭐
  ○ Variation 2 │ Energy: 0.65 │ 120 bpm │ A minor
  ○ Variation 3 │ Energy: 0.78 │ 130 bpm │ D major
  ○ Variation 4 │ Energy: 0.70 │ 125 bpm │ C major

▶ Play selected  │ [C]ompare all │ [V]ote │ [R]efine │ [Q]uit

Reason (optional): Highest energy and best tempo for upbeat intro

✓ Variation 1 selected and saved
```

### 5.2 Refinement Workflow

```bash
$ hrmcp variations refine vset_abc123 1 \
    --intent "Add more piano, preserve energy" \
    --tool orpheus_generate_seeded \
    --num-variations 3 \
    --params '{"seed_hash": "def456g...", "constraints": {"instrument_hints": ["piano"]}}'

Refining variation vset_abc123/var_1...
Parent: Energy 0.81, G major, 132 bpm

✓ Created refinement set: vset_def456

Refinement Set: vset_def456
Parent: vset_abc123/var_1
Intent: Add more piano, preserve energy

┌─────┬──────────────┬────────┬───────────────┬────────┐
│ Idx │ Hash         │ Energy │ Instruments   │ Change │
├─────┼──────────────┼────────┼───────────────┼────────┤
│ 0   │ 111222...    │ 0.80   │ piano, str    │ +piano │
│ 1   │ 333444...    │ 0.79   │ piano, bass   │ +piano │
│ 2   │ 555666...    │ 0.82   │ piano, drums  │ +piano │
└─────┴──────────────┴────────┴───────────────┴────────┘

All refinements preserved target energy (0.79-0.82)
All refinements added piano as requested ✓
```

### 5.3 Exploration Tree Visualization

```bash
$ hrmcp variations tree vset_abc123

Variation Exploration Tree
═══════════════════════════════════════════════════════

vset_abc123: "Explore upbeat melodies"
├─ var_0 (E:0.72, C major, 128bpm)
│  └─ (no refinements)
├─ var_1 (E:0.81, G major, 132bpm) ⭐ CHOSEN
│  └─ vset_def456: "Add more piano, preserve energy"
│     ├─ var_0 (E:0.80, piano+str)
│     ├─ var_1 (E:0.79, piano+bass)
│     └─ var_2 (E:0.82, piano+drums) ⭐ 2 votes
│        └─ vset_ghi789: "Slow down for bridge"
│           ├─ var_0 (E:0.75, 110bpm)
│           └─ var_1 (E:0.72, 105bpm) ⭐ CHOSEN
├─ var_2 (E:0.65, A minor, 120bpm)
│  └─ (no refinements)
├─ var_3 (E:0.78, D major, 130bpm)
│  └─ (no refinements)
└─ var_4 (E:0.70, C major, 125bpm)
   └─ (no refinements)

Legend:
  ⭐ - Chosen variation
  E  - Energy level

Depth: 3 levels
Total variations: 10 (5 + 3 + 2)
Chosen path: var_1 → var_2 → var_1
```

---

## 6. LLM/Agent Optimization

### 6.1 Natural Language Responses

When agents query variations, optimize responses for reasoning:

**Good (LLM-Friendly):**
```json
{
  "set_id": "vset_abc123",
  "intent": "Explore upbeat melodies for intro",
  "summary": "Created 5 melody variations with energy ranging from 0.65 to 0.81",

  "variations": [
    {
      "index": 0,
      "description": "Moderate energy (0.72) melody in C major at 128 bpm with piano and strings",
      "artifact_hash": "5ca7815...",
      "metadata": {
        "energy": 0.72,
        "tempo_bpm": 128,
        "key": "C major",
        "instruments": ["piano", "strings"],
        "notable_features": [
          "Ascending melody line",
          "Regular rhythm",
          "Bright harmonic palette"
        ]
      },
      "comparison_to_set": {
        "energy": "middle",
        "tempo": "middle",
        "uniqueness": "shares key with variation 4"
      }
    },
    // ...
  ],

  "recommendations": {
    "highest_energy": {
      "index": 1,
      "reason": "Energy 0.81 best matches 'upbeat' intent"
    },
    "most_interesting": {
      "index": 3,
      "reason": "Unique key (D major) with high energy (0.78)"
    },
    "safest_choice": {
      "index": 0,
      "reason": "Moderate in all dimensions, versatile"
    }
  },

  "next_steps": [
    "Vote on variations to indicate preference",
    "Refine promising variations (e.g., #1 for more energy)",
    "Choose a variation to proceed with composition",
    "Compare variations side-by-side on specific dimensions"
  ]
}
```

**Bad (Hard to Reason About):**
```json
{
  "id": "vset_abc123",
  "vars": [
    {"i": 0, "h": "5ca7815...", "e": 0.72, "t": 128, "k": "CM"},
    {"i": 1, "h": "def456g...", "e": 0.81, "t": 132, "k": "GM"},
    // ... cryptic abbreviations, no context
  ]
}
```

### 6.2 Prompting Patterns for Agents

**Built-in Agent Guidance:**

Variation set responses include guidance for common workflows:

```json
{
  "variations": [...],

  "agent_guidance": {
    "decision_framework": {
      "for_exploration": "If exploring broadly, vote on multiple promising variations before refining",
      "for_convergence": "If seeking best option, compare on key dimensions and choose highest-scoring",
      "for_ensemble": "If collaborating, each agent votes independently, then discuss differences"
    },

    "comparison_suggestions": [
      "Compare energy levels if intent mentions mood/intensity",
      "Compare keys if building harmonic progression",
      "Compare tempo if matching to other sections",
      "Compare instruments if orchestrating"
    ],

    "refinement_suggestions": {
      "if_close": "When a variation is close but not perfect, refine with specific constraints",
      "if_exploring": "When multiple variations are interesting, refine each separately to explore branches",
      "if_combining": "When variations have complementary strengths, consider requesting a merge operation"
    }
  }
}
```

### 6.3 Structured Decision Support

**Comparison Matrix for LLM Reasoning:**

```json
{
  "comparison_matrix": {
    "dimensions": ["energy", "tempo", "key", "instruments", "duration"],
    "variations": [
      {
        "index": 0,
        "values": {
          "energy": {"value": 0.72, "rank": 3, "percentile": 60},
          "tempo": {"value": 128, "rank": 3, "percentile": 60},
          "key": {"value": "C major", "harmonic_distance_from_target": 0},
          "instruments": {"value": ["piano", "strings"], "count": 2},
          "duration": {"value": 45.2, "rank": 2}
        },
        "scores": {
          "matches_intent": 0.75,
          "uniqueness": 0.45,
          "ensemble_vote_avg": 0.60
        }
      },
      // ... other variations
    ],

    "correlations": {
      "energy_tempo": 0.82,
      "key_instruments": -0.15
    },

    "clusters": [
      {
        "name": "High energy group",
        "members": [1, 3],
        "characteristics": "Energy > 0.75, tempo > 128"
      },
      {
        "name": "Mellow group",
        "members": [0, 2, 4],
        "characteristics": "Energy < 0.75, diverse keys"
      }
    ]
  }
}
```

This enables agents to reason:
```
"I see two clusters in the variations: a high-energy group (variations 1 and 3)
and a mellow group (0, 2, 4). Since the intent was 'upbeat melodies', I should
focus on the high-energy cluster. Variation 1 has the highest energy (0.81)
and tempo (132), making it the best match for the intent. However, variation 3
offers a unique key (D major) which might provide harmonic interest. Let me
vote for both and see what other agents think."
```

### 6.4 Provenance Chains

**Clear Lineage for Context:**

```json
{
  "variation_item": {
    "index": 2,
    "artifact_hash": "555666...",

    "provenance": {
      "creation_path": [
        {
          "level": 0,
          "set_id": "vset_abc123",
          "variation_index": 1,
          "intent": "Explore upbeat melodies",
          "chosen_reason": "Highest energy"
        },
        {
          "level": 1,
          "set_id": "vset_def456",
          "variation_index": 2,
          "intent": "Add more piano, preserve energy",
          "chosen_reason": "Best balance of piano and energy"
        },
        {
          "level": 2,
          "set_id": "vset_ghi789",
          "variation_index": 0,
          "intent": "Slow down for bridge section",
          "chosen_reason": "Current variation"
        }
      ],

      "lineage_summary": "This variation descended from the 'high energy' branch, " +
                         "was refined to add more piano, then slowed down for use as a bridge. " +
                         "It preserves the original melodic character while adapting tempo and orchestration.",

      "decision_history": [
        "Level 0: Chose energetic variation over mellow alternatives",
        "Level 1: Prioritized piano addition while maintaining energy",
        "Level 2: Reduced tempo for transition context"
      ],

      "accumulated_constraints": {
        "must_preserve": ["melodic_contour", "harmonic_structure"],
        "was_varied": ["tempo", "instrumentation", "energy_level"],
        "never_changed": ["time_signature", "general_mood"]
      }
    }
  }
}
```

### 6.5 Voting and Consensus

**Multi-Agent Voting Support:**

```json
{
  "selection_state": {
    "votes": {
      "0": [
        {
          "voter": "agent_claude_001",
          "score": 0.75,
          "reason": "Good energy but instruments lack variety",
          "timestamp": "2025-11-21T03:10:00Z"
        }
      ],
      "1": [
        {
          "voter": "agent_claude_001",
          "score": 0.90,
          "reason": "Highest energy, best tempo, excellent for upbeat intro",
          "timestamp": "2025-11-21T03:10:00Z"
        },
        {
          "voter": "agent_gemini_002",
          "score": 0.85,
          "reason": "Strong choice, though G major may not fit verse key",
          "timestamp": "2025-11-21T03:12:00Z"
        },
        {
          "voter": "agent_deepseek_003",
          "score": 0.88,
          "reason": "Optimal tempo and energy, instrumentation is solid",
          "timestamp": "2025-11-21T03:13:00Z"
        }
      ],
      "2": [
        {
          "voter": "agent_gemini_002",
          "score": 0.70,
          "reason": "Interesting key choice (A minor) for darker intro option",
          "timestamp": "2025-11-21T03:12:00Z"
        }
      ]
    },

    "vote_summary": {
      "total_votes": 5,
      "variations_with_votes": [0, 1, 2],
      "variations_without_votes": [3, 4],

      "scores_by_variation": {
        "0": {"avg": 0.75, "count": 1},
        "1": {"avg": 0.88, "count": 3},
        "2": {"avg": 0.70, "count": 1}
      },

      "consensus": {
        "strong_consensus": true,
        "winner": 1,
        "winner_score": 0.88,
        "agreement_level": 0.92,
        "dissenting_opinions": [
          {
            "voter": "agent_gemini_002",
            "concern": "Key compatibility with verse",
            "severity": "minor"
          }
        ]
      }
    },

    "voting_guidance": {
      "status": "Strong consensus on variation 1",
      "recommendation": "Proceed with choosing variation 1, address key compatibility concern in next refinement",
      "alternative": "If key compatibility is critical, consider refining variation 1 to C major"
    }
  }
}
```

This enables collaborative agent workflows:
```
Agent A: "I vote 0.90 for variation 1 - best energy"
Agent B: "I vote 0.85 for variation 1, but concerned about G major key"
Agent C: "I vote 0.88 for variation 1"

System: Strong consensus detected (avg: 0.88). Minor concern about key.
        Recommendation: Choose variation 1, refine to C major if needed.
```

---

## 7. Implementation Phases

### Phase 1: Core Data Structures (Week 1)
- [ ] Implement VariationSet, VariationItem domain models
- [ ] Add variation storage (database + CAS integration)
- [ ] Basic CRUD operations

### Phase 2: HTTP API (Week 2)
- [ ] REST endpoints for variation sets
- [ ] Vote/choose/annotate endpoints
- [ ] Comparison endpoint
- [ ] GraphQL schema (optional)

### Phase 3: MCP Integration (Week 3)
- [ ] New variation-specific MCP tools
- [ ] Update Orpheus tools to support variation sets
- [ ] LLM-optimized responses

### Phase 4: CLI (Week 4)
- [ ] Basic variation commands
- [ ] Interactive selection
- [ ] Comparison views
- [ ] Tree visualization

### Phase 5: Advanced Features (Week 5+)
- [ ] Multi-agent voting
- [ ] Refinement workflows
- [ ] Ensemble coordination
- [ ] Variation analytics

---

## 8. Success Metrics

**For Agents:**
- Can create and compare variations without confusion
- Natural voting/selection workflows
- Easy refinement of promising options
- Clear decision rationale in responses

**For System:**
- < 200ms to create variation set
- Efficient storage (deduplication via CAS)
- Scalable to 100+ variation sets
- Clear audit trail of decisions

**For Collaboration:**
- Multi-agent voting works smoothly
- Consensus detection is accurate
- Provenance chains are clear
- Refinement trees stay manageable

---

## 9. Open Questions

1. **Variation Limits:** Max variations per set? (Suggest: 20)
2. **Tree Depth:** Max refinement depth? (Suggest: 10 levels)
3. **Garbage Collection:** When to prune unused variations?
4. **Merging:** How to combine aspects of multiple variations?
5. **Negative Examples:** Support "not like this" variations?
6. **Version Control:** Integration with jj for variation history?
7. **Export:** Export variation trees to other formats?

---

## 10. Example: End-to-End Workflow

**Scenario:** Agent ensemble composing a song intro

```
┌─────────────────────────────────────────────────────────────┐
│ Step 1: Initial Exploration                                 │
└─────────────────────────────────────────────────────────────┘

Agent_Claude: "Let's explore upbeat intro melodies"

create_variation_set({
  intent: "Explore upbeat melodies for intro section",
  operation: {tool: "orpheus_generate", params: {target_energy: 0.7}},
  num_variations: 5
})

→ Creates vset_abc123 with 5 variations

┌─────────────────────────────────────────────────────────────┐
│ Step 2: Ensemble Voting                                     │
└─────────────────────────────────────────────────────────────┘

Agent_Claude:
  vote(vset_abc123, var_1, score=0.90, reason="Highest energy")

Agent_Gemini:
  vote(vset_abc123, var_1, score=0.85, reason="Good, but G major may clash")
  vote(vset_abc123, var_2, score=0.70, reason="A minor interesting alternative")

Agent_DeepSeek:
  vote(vset_abc123, var_1, score=0.88, reason="Optimal tempo and energy")

System: "Strong consensus on variation 1 (avg: 0.88)"

┌─────────────────────────────────────────────────────────────┐
│ Step 3: Address Concern                                     │
└─────────────────────────────────────────────────────────────┘

Agent_Claude: "Let's address Gemini's key concern"

refine_variation({
  parent_set: vset_abc123,
  parent_variation: 1,
  intent: "Transpose to C major while preserving energy",
  operation: {
    tool: "orpheus_generate_seeded",
    params: {
      seed_hash: "def456...",
      constraints: {target_key: "C major", preserve_energy: true}
    }
  },
  num_variations: 3
})

→ Creates vset_def456 with 3 refinements

┌─────────────────────────────────────────────────────────────┐
│ Step 4: Quick Consensus                                     │
└─────────────────────────────────────────────────────────────┘

compare_variations(vset_def456, dimensions=["key", "energy"])

All agents: "Variation 0 is perfect - C major, energy preserved at 0.80"

choose_variation(vset_def456, 0, reason="Consensus choice")

┌─────────────────────────────────────────────────────────────┐
│ Step 5: Continue Composition                                │
└─────────────────────────────────────────────────────────────┘

Agent_Claude: "Now let's create a bridge to the verse"

create_variation_set({
  intent: "Bridge from intro to verse",
  operation: {
    tool: "orpheus_bridge",
    params: {
      section_a_hash: "111222...", // chosen intro
      section_b_hash: "verse_hash",
      max_tokens: 256
    }
  },
  num_variations: 3
})

→ Continues workflow...
```

**Result:** Clear decision trail, efficient exploration, collaborative selection

---

## Conclusion

By treating variations as a first-class system primitive:

✅ **Agents** get structured exploration with rich decision support
✅ **APIs** provide consistent, composable variation operations
✅ **Data structures** optimize for LLM reasoning and collaboration
✅ **Workflows** enable parallel exploration → refinement → selection

This design makes variation sets a powerful tool for creative AI collaboration, optimized specifically for how LLMs think about and explore possibility spaces.

**Next Steps:** Review, refine open questions, begin Phase 1 implementation.
