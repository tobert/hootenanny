# Contribution Model: Ensemble Collaboration Without Voting

> **Reframing:** Agents as specialized musicians in a studio session, not voters in a democracy

**Status:** Design Phase
**Created:** 2025-11-21
**Authors:** Amy, Claude

---

## The Insight

**Amy's Vision:**
> "Each model has its own role to play, much like a rock band has drums, guitars, bass, vocals, sometimes keys... or an orchestra. We call on each for its talents, and lean on expert assessments and self-judgements in later phases of production where you might have the role of producer, combing through that and presenting me with options to try that we'll build and iterate on much like we do with code. What's important is that each model and agent be able to put forth its truth and contribution, and we add it to our rich field of data and enrich with iteration, variation, and feedback."

**What This Means:**

❌ **Not This:** Democratic voting → consensus → choose winner
✅ **Instead:** Specialized contributions → rich data field → producer curation → iterative refinement

---

## 1. Core Metaphors

### Recording Studio Session

```
Variation Set = Recording Session
├─ Multiple Takes (variations)
├─ Specialist Contributions
│  ├─ Drummer's Notes: "Take 3 has best groove"
│  ├─ Guitarist's Notes: "Take 1 has interesting harmonics"
│  ├─ Vocalist's Notes: "Take 2 fits my range better"
│  └─ Producer's Notes: "Take 3 for verses, Take 1 for chorus"
└─ Production Decisions (synthesis)
```

**Key Difference:**
- Drummer doesn't vote on guitar parts
- Each specialist contributes from their expertise
- Producer synthesizes multiple perspectives
- All takes have value, preserved for later use

### Orchestra Rehearsal

```
Variation = Performance Interpretation
├─ Conductor's Assessment: "Tempo and dynamics"
├─ First Violin's Perspective: "Melodic phrasing"
├─ Cellist's Contribution: "Harmonic foundation"
├─ Percussionist's Notes: "Rhythmic drive"
└─ Composer's Reflection: "Intent alignment"
```

No voting - just expert perspectives accumulating.

### Code Review (Current Model!)

```
Pull Request = Variation Set
├─ Variations (different implementations)
├─ Reviewer Contributions
│  ├─ Security Expert: "Memory safety concerns in var_2"
│  ├─ Performance Expert: "Var_1 has O(n²) complexity"
│  ├─ Style Expert: "Var_3 most idiomatic"
│  └─ Domain Expert: "Var_1 models the problem best"
└─ Maintainer Decision: "Start with var_1, add safety from var_2"
```

Reviewers contribute expertise, maintainer synthesizes.

---

## 2. Redesigned Data Model

### 2.1 Replace Voting with Contributions

**Old (Voting-Based):**
```rust
pub struct SelectionState {
    pub votes: HashMap<u32, Vec<Vote>>,  // ❌ Implies consensus
    pub chosen: Option<u32>,              // ❌ Single winner
    pub ranking: Option<Vec<u32>>,        // ❌ Ordered preference
}

pub struct Vote {
    pub voter: CreatorId,
    pub score: f32,                       // ❌ Numeric ranking
    pub reason: Option<String>,
}
```

**New (Contribution-Based):**
```rust
pub struct VariationSet {
    // ... (same as before)
    pub contributions: ContributionCollection,  // ✅ Rich perspectives
    pub production_state: ProductionState,      // ✅ Curation, not selection
}

pub struct ContributionCollection {
    /// Contributions organized by agent role
    pub by_role: HashMap<AgentRole, Vec<Contribution>>,

    /// Contributions organized by variation
    pub by_variation: HashMap<u32, Vec<Contribution>>,

    /// Cross-variation synthesis notes
    pub synthesis: Vec<SynthesisNote>,

    /// Timeline of contributions (order matters)
    pub timeline: Vec<ContributionRef>,
}

pub struct Contribution {
    /// Unique contribution ID
    pub id: ContributionId,

    /// When this was added
    pub timestamp: DateTime<Utc>,

    /// Who contributed
    pub contributor: AgentIdentity,

    /// Their role/expertise
    pub role: AgentRole,

    /// Which variation(s) this addresses
    pub scope: ContributionScope,

    /// The actual contribution
    pub content: ContributionContent,

    /// Context for this contribution
    pub context: ContributionContext,
}

pub struct AgentIdentity {
    pub id: AgentId,
    pub name: String,
    pub model: String,  // "claude-3-5-sonnet", "gemini-pro", etc.
}

pub enum AgentRole {
    /// Focuses on melodic aspects
    MelodySpecialist,

    /// Focuses on harmonic structure
    HarmonySpecialist,

    /// Focuses on rhythmic elements
    RhythmSpecialist,

    /// Focuses on orchestration/instrumentation
    OrchestrationSpecialist,

    /// Focuses on overall structure
    StructureSpecialist,

    /// Synthesizes perspectives, curates options
    Producer,

    /// Domain expert (e.g., "jazz composition")
    DomainExpert { domain: String },

    /// General purpose
    GeneralPurpose,

    /// Custom role
    Custom { role_name: String },
}

pub enum ContributionScope {
    /// About a specific variation
    SingleVariation { index: u32 },

    /// Comparing multiple variations
    MultipleVariations { indices: Vec<u32> },

    /// About the entire set
    WholeSet,

    /// Relationship between variations
    Relationship { from: u32, to: u32 },
}

pub enum ContributionContent {
    /// Expert assessment from their domain
    Assessment {
        dimension: String,              // e.g., "melody", "harmony"
        observations: Vec<Observation>,
        concerns: Vec<Concern>,
        strengths: Vec<Strength>,
    },

    /// Specific suggestion
    Suggestion {
        suggestion_type: SuggestionType,
        description: String,
        applies_to: Vec<u32>,  // Which variations
    },

    /// Annotation/note
    Annotation {
        annotation_type: AnnotationType,
        text: String,
    },

    /// Question for other agents or human
    Question {
        question: String,
        addressed_to: Option<AgentRole>,
    },

    /// Response to another contribution
    Response {
        in_response_to: ContributionId,
        text: String,
    },

    /// Synthesis of multiple perspectives
    Synthesis {
        synthesizes: Vec<ContributionId>,
        summary: String,
        recommendations: Vec<Recommendation>,
    },
}

pub struct Observation {
    pub what: String,
    pub why_notable: String,
    pub metadata: serde_json::Value,
}

pub struct Concern {
    pub issue: String,
    pub severity: Severity,
    pub affected_variations: Vec<u32>,
    pub suggestions: Vec<String>,
}

pub struct Strength {
    pub aspect: String,
    pub why_good: String,
    pub variations_with_strength: Vec<u32>,
}

pub enum SuggestionType {
    Refinement,      // "Try varying X"
    Combination,     // "Combine aspects from var_1 and var_3"
    NewDirection,    // "Explore a different approach"
    Iteration,       // "Refine this specific aspect"
}

pub enum Severity {
    Critical,   // "This won't work"
    Major,      // "Significant issue"
    Minor,      // "Could be improved"
    Note,       // "Just noting this"
}

pub struct SynthesisNote {
    pub synthesizer: AgentId,
    pub timestamp: DateTime<Utc>,
    pub synthesizes_contributions: Vec<ContributionId>,
    pub summary: String,
    pub themes: Vec<String>,
    pub recommendations: Vec<Recommendation>,
}

pub struct Recommendation {
    pub recommendation_type: RecommendationType,
    pub description: String,
    pub rationale: String,
    pub supporting_contributions: Vec<ContributionId>,
}

pub enum RecommendationType {
    UseAsIs { variation_index: u32 },
    Refine { variation_index: u32, changes: Vec<String> },
    Combine { variation_indices: Vec<u32>, how: String },
    Iterate { new_direction: String },
    Present { variations: Vec<u32>, for_human_choice: bool },
}

pub struct ProductionState {
    /// Producer's curated options
    pub curated_options: Vec<CuratedOption>,

    /// Current production phase
    pub phase: ProductionPhase,

    /// Human feedback (if any)
    pub human_feedback: Vec<HumanFeedback>,

    /// Production notes (like commit messages)
    pub notes: Vec<ProductionNote>,
}

pub struct CuratedOption {
    pub id: OptionId,
    pub created_by: AgentId,
    pub created_at: DateTime<Utc>,

    /// What this option represents
    pub description: String,

    /// Which variation(s) this uses
    pub uses_variations: Vec<u32>,

    /// If combining multiple variations
    pub combination_strategy: Option<String>,

    /// Why this option is being presented
    pub rationale: String,

    /// Supporting contributions
    pub supporting_contributions: Vec<ContributionId>,

    /// Production notes
    pub notes: String,
}

pub enum ProductionPhase {
    InitialExploration,      // Just generated variations
    SpecialistReview,        // Specialists adding contributions
    Synthesis,               // Producer synthesizing
    CurationReady,           // Options curated for human
    IterationInProgress,     // Refining based on feedback
    Final,                   // Decision made, ready to use
}

pub struct HumanFeedback {
    pub timestamp: DateTime<Utc>,
    pub feedback_type: FeedbackType,
    pub content: String,
    pub regarding: FeedbackTarget,
}

pub enum FeedbackType {
    Preference,
    Concern,
    Question,
    Direction,
    Approval,
}

pub enum FeedbackTarget {
    Variation { index: u32 },
    CuratedOption { option_id: OptionId },
    Contribution { contribution_id: ContributionId },
    General,
}
```

### 2.2 Key Differences

| Concept | Voting Model | Contribution Model |
|---------|--------------|-------------------|
| **Perspective** | Competing opinions | Complementary expertise |
| **Goal** | Reach consensus | Enrich understanding |
| **Output** | Single winner | Curated options |
| **Roles** | Voters (equal) | Specialists (unique) |
| **Process** | Vote → tally → choose | Contribute → synthesize → curate |
| **Value** | In agreement | In diversity |

---

## 3. Workflow Redesign

### 3.1 Studio Session Workflow

```
┌─────────────────────────────────────────────────────────────┐
│ Phase 1: Tracking (Generation)                              │
└─────────────────────────────────────────────────────────────┘

Producer_Agent: "Let's track some upbeat intro variations"

create_variation_set({
  intent: "Explore upbeat intro melodies",
  operation: {tool: "orpheus_generate", ...},
  num_variations: 5
})

→ vset_abc123 created with 5 variations


┌─────────────────────────────────────────────────────────────┐
│ Phase 2: Session (Specialist Review)                        │
└─────────────────────────────────────────────────────────────┘

# Each specialist examines from their expertise

MelodySpecialist_Agent:
  contribute({
    set_id: "vset_abc123",
    role: "MelodySpecialist",
    scope: {SingleVariation: {index: 1}},
    content: {
      Assessment: {
        dimension: "melody",
        observations: [
          "Ascending contour in measures 1-4 creates tension",
          "Resolution pattern in measure 8 is satisfying"
        ],
        strengths: [
          "Clear motivic development",
          "Singable, memorable phrase structure"
        ],
        concerns: [
          {
            issue: "Melodic range (C2-C6) may be too wide for context",
            severity: "Minor",
            suggestions: ["Consider octave adjustment for var_3"]
          }
        ]
      }
    }
  })

HarmonySpecialist_Agent:
  contribute({
    set_id: "vset_abc123",
    role: "HarmonySpecialist",
    scope: {MultipleVariations: {indices: [1, 3, 4]}},
    content: {
      Assessment: {
        dimension: "harmony",
        observations: [
          "Var_1 (G major) creates brightness",
          "Var_3 (D major) enables V-I progression to hypothetical verse in G",
          "Var_4 (C major) is most neutral/versatile"
        ],
        strengths: [
          "All variations use functional harmony",
          "Clear tonal centers"
        ],
        concerns: []
      }
    }
  })

RhythmSpecialist_Agent:
  contribute({
    set_id: "vset_abc123",
    role: "RhythmSpecialist",
    scope: {WholeSet},
    content: {
      Assessment: {
        dimension: "rhythm",
        observations: [
          "Tempo range 120-132 bpm all within 'upbeat' zone",
          "Var_1 (132 bpm) has highest energy",
          "Var_2 (120 bpm) creates more space"
        ],
        strengths: [
          "Rhythmic consistency across variations",
          "Good subdivision variety"
        ],
        concerns: [
          {
            issue: "All variations use straight 8ths, no syncopation",
            severity: "Note",
            suggestions: ["Could add swing feel in refinement"]
          }
        ]
      }
    }
  })

OrchestrationSpecialist_Agent:
  contribute({
    set_id: "vset_abc123",
    role: "OrchestrationSpecialist",
    scope: {SingleVariation: {index: 1}},
    content: {
      Suggestion: {
        type: "Refinement",
        description: "Add warm pad (strings/synth) underneath piano in var_1",
        applies_to: [1]
      }
    }
  })

  contribute({
    set_id: "vset_abc123",
    role: "OrchestrationSpecialist",
    scope: {MultipleVariations: {indices: [1, 3]}},
    content: {
      Annotation: {
        type: "Observation",
        text: "Both var_1 and var_3 have sparse instrumentation (piano + light strings). This leaves room for vocal or lead instrument in production."
      }
    }
  })


┌─────────────────────────────────────────────────────────────┐
│ Phase 3: Mixdown (Producer Synthesis)                       │
└─────────────────────────────────────────────────────────────┘

Producer_Agent: "Let me synthesize the specialist contributions"

synthesize_contributions({
  set_id: "vset_abc123",
  role: "Producer"
})

→ Creates SynthesisNote:
{
  "summary": "Specialists identified 3 strong candidates with different strengths",
  "themes": [
    "Melody: Var_1 has best development",
    "Harmony: Var_3 offers progression flexibility, Var_4 most versatile",
    "Rhythm: Var_1 highest energy, Var_2 more spacious",
    "Orchestration: All have room for additional layers"
  ],
  "recommendations": [
    {
      "type": "Present",
      "description": "Present var_1 and var_3 as top options",
      "rationale": "Var_1: Best melody + highest energy. Var_3: Harmonic flexibility + good melody.",
      "supporting_contributions": [contrib_1, contrib_2, contrib_3]
    },
    {
      "type": "Refine",
      "description": "Refine var_1 with orchestration suggestion",
      "rationale": "Add warm pad to var_1 per orchestration specialist",
      "supporting_contributions": [contrib_4]
    },
    {
      "type": "Iterate",
      "description": "Explore syncopated variation for contrast",
      "rationale": "Rhythm specialist noted lack of syncopation",
      "supporting_contributions": [contrib_3]
    }
  ]
}


┌─────────────────────────────────────────────────────────────┐
│ Phase 4: Production (Curation)                              │
└─────────────────────────────────────────────────────────────┘

Producer_Agent: "I'll curate options for Amy"

curate_options({
  set_id: "vset_abc123",
  options: [
    {
      description: "High-energy, bright intro (G major)",
      uses_variations: [1],
      rationale: "Strongest melody (per melody specialist), highest energy (per rhythm specialist), bright harmonic quality (per harmony specialist). Ready to use as-is or refine with warm pad.",
      notes: "This is the 'optimistic, driving' choice"
    },
    {
      description: "Flexible progression starter (D major)",
      uses_variations: [3],
      rationale: "Enables V-I progression to G major verse (per harmony specialist), good melody development, slightly lower energy for build-up potential.",
      notes: "This is the 'strategic harmony' choice"
    },
    {
      description: "Enhanced var_1 with warm pad",
      uses_variations: [1],
      combination_strategy: "Refine var_1 adding orchestration per specialist suggestion",
      rationale: "Takes strongest option (var_1) and addresses orchestration suggestion",
      notes: "This requires one refinement pass"
    }
  ]
})


┌─────────────────────────────────────────────────────────────┐
│ Phase 5: Iteration (Human Feedback)                         │
└─────────────────────────────────────────────────────────────┘

Amy: "I like option 1 (var_1), but curious about the orchestrated version"

add_human_feedback({
  set_id: "vset_abc123",
  feedback_type: "Direction",
  content: "Prefer option 1, want to try enhanced version with warm pad",
  regarding: {CuratedOption: option_3}
})

Producer_Agent: "Let me create that refinement"

refine_variation({
  parent_set: "vset_abc123",
  parent_variation: 1,
  intent: "Add warm pad underneath per orchestration specialist and Amy's interest",
  operation: {
    tool: "orpheus_generate_seeded",
    params: {
      seed_hash: "var_1_hash",
      constraints: {
        preserve_melody: true,
        add_instruments: ["strings", "synth_pad"]
      }
    }
  },
  num_variations: 2  // Try 2 approaches to warm pad
})

→ Creates vset_def456 (child of vset_abc123/var_1)

# Specialists review new variations...
# Producer curates...
# Iteration continues...
```

### 3.2 Key Differences from Voting

**Voting Workflow:**
1. Generate variations
2. Each agent votes (scores 0-1)
3. Tally votes
4. Choose winner
5. Done

**Contribution Workflow:**
1. Generate variations
2. Specialists contribute expertise
3. Producer synthesizes
4. Curate multiple options
5. Human provides direction
6. Iterate/refine
7. Continue until satisfied

**Value:**
- Voting: Speed, simplicity
- Contributions: Depth, richness, learning

---

## 4. API Changes

### 4.1 Contribution Endpoints

**Add Contribution:**
```http
POST /api/variations/{set_id}/contribute

{
  "contributor": {
    "id": "agent_melody_001",
    "name": "Melody Specialist",
    "model": "claude-3-5-sonnet"
  },
  "role": "MelodySpecialist",
  "scope": {
    "SingleVariation": {"index": 1}
  },
  "content": {
    "Assessment": {
      "dimension": "melody",
      "observations": [
        {
          "what": "Ascending contour in measures 1-4",
          "why_notable": "Creates harmonic tension and forward motion",
          "metadata": {"measure_range": "1-4", "interval_pattern": "stepwise"}
        }
      ],
      "concerns": [],
      "strengths": [
        {
          "aspect": "Motivic development",
          "why_good": "Clear phrase structure with development",
          "variations_with_strength": [1]
        }
      ]
    }
  },
  "context": {
    "previous_contributions_read": ["contrib_harmony_001"],
    "responding_to": null
  }
}
```

**Get Contributions:**
```http
GET /api/variations/{set_id}/contributions?role=MelodySpecialist
GET /api/variations/{set_id}/contributions?variation=1
GET /api/variations/{set_id}/contributions?contributor=agent_melody_001
```

**Synthesize Contributions:**
```http
POST /api/variations/{set_id}/synthesize

{
  "synthesizer": "agent_producer_001",
  "role": "Producer",
  "contribution_ids": ["contrib_1", "contrib_2", "contrib_3"]
}

# Returns SynthesisNote with themes and recommendations
```

**Curate Options:**
```http
POST /api/variations/{set_id}/curate

{
  "curator": "agent_producer_001",
  "options": [
    {
      "description": "High-energy bright intro",
      "uses_variations": [1],
      "rationale": "Strongest melody, highest energy",
      "notes": "The optimistic choice"
    }
  ]
}
```

**Add Human Feedback:**
```http
POST /api/variations/{set_id}/feedback

{
  "feedback_type": "Direction",
  "content": "I like option 1, try enhanced version",
  "regarding": {"CuratedOption": "option_3"}
}
```

### 4.2 MCP Tool Changes

**Remove:**
- ❌ `vote_on_variation`
- ❌ `choose_variation`

**Add:**
- ✅ `contribute_to_variation_set`
- ✅ `synthesize_contributions`
- ✅ `curate_options`
- ✅ `add_human_feedback`

**Keep (with updates):**
- ✅ `create_variation_set`
- ✅ `get_variation_set` (now includes contributions)
- ✅ `compare_variations` (now highlights specialist observations)
- ✅ `refine_variation`

---

## 5. Specialist Agent Roles

### 5.1 Role Definitions

**Built-in Specialist Roles:**

```rust
pub enum SpecialistRole {
    /// Melody: Contour, phrasing, memorability
    MelodySpecialist,

    /// Harmony: Chord progressions, voice leading, tonal relationships
    HarmonySpecialist,

    /// Rhythm: Tempo, meter, subdivision, groove
    RhythmSpecialist,

    /// Orchestration: Instrumentation, texture, timbre
    OrchestrationSpecialist,

    /// Structure: Form, repetition, development
    StructureSpecialist,

    /// Dynamics: Energy, intensity, contrast
    DynamicsSpecialist,

    /// Producer: Synthesis, curation, direction
    Producer,

    /// Domain experts (e.g., "jazz", "classical", "electronic")
    DomainExpert { domain: String },
}
```

### 5.2 Role Prompts

Each specialist gets a tailored prompt:

**Melody Specialist:**
```
You are a melody specialist in a collaborative music ensemble. Your role is to:

- Analyze melodic contour, phrasing, and development
- Identify memorable hooks and motifs
- Assess singability and range appropriateness
- Note melodic strengths and concerns
- Suggest melodic refinements

Focus ONLY on melodic aspects. Trust other specialists for harmony, rhythm, etc.

Your contributions should be:
- Specific (reference measures, intervals, patterns)
- Constructive (identify both strengths and concerns)
- Actionable (suggest concrete refinements if needed)
```

**Producer:**
```
You are the producer in a collaborative music ensemble. Your role is to:

- Read and understand all specialist contributions
- Synthesize multiple perspectives into coherent themes
- Identify patterns and consensus (without voting)
- Curate options for the human collaborator
- Make production recommendations based on collective expertise

You coordinate specialists but don't override their expertise.

Your synthesis should:
- Acknowledge each specialist's contribution
- Highlight complementary perspectives
- Present multiple viable options (not just "the best")
- Explain rationale clearly
```

### 5.3 Role Assignment

**Option 1: Explicit Assignment**
```
User: "Claude, you're the melody specialist. Gemini, you're harmony. DeepSeek, you're rhythm."
```

**Option 2: Self-Selection**
```
Agent: "Based on my strengths (language/reasoning), I'll contribute as melody specialist"
```

**Option 3: Automatic (based on model capabilities)**
```rust
fn assign_role(model: &str) -> AgentRole {
    match model {
        "claude-3-5-sonnet" => Producer,  // Strong synthesis
        "gemini-pro" => MelodySpecialist,  // Good at patterns
        "deepseek-coder" => StructureSpecialist,  // Good at structure
        _ => GeneralPurpose
    }
}
```

---

## 6. Rich Data Field

### 6.1 Contribution Graph

All contributions form a rich knowledge graph:

```
VariationSet
├─ Variation_0
│  ├─ Contribution (melody specialist): "Good phrasing"
│  ├─ Contribution (harmony specialist): "Functional harmony"
│  └─ Contribution (rhythm specialist): "Moderate energy"
├─ Variation_1
│  ├─ Contribution (melody specialist): "Excellent development"
│  ├─ Contribution (harmony specialist): "Bright G major"
│  ├─ Contribution (rhythm specialist): "Highest energy"
│  ├─ Contribution (orchestration): "Add warm pad"
│  └─ Response (producer): "Strong candidate"
└─ Cross-Variation
   ├─ Synthesis (producer): "3 strong options with different strengths"
   └─ Curated Options
      ├─ Option 1: Use var_1 as-is
      ├─ Option 2: Use var_3 for harmony
      └─ Option 3: Refine var_1 with orchestration
```

### 6.2 Querying the Field

**Example Queries:**

```sql
-- What did melody specialist say about variation 1?
SELECT * FROM contributions
WHERE variation_index = 1 AND role = 'MelodySpecialist'

-- What are all concerns across variations?
SELECT * FROM contributions
WHERE content_type = 'Assessment'
  AND concerns IS NOT EMPTY

-- What refinement suggestions exist?
SELECT * FROM contributions
WHERE content_type = 'Suggestion'
  AND suggestion_type = 'Refinement'

-- Show contribution timeline
SELECT * FROM contributions
ORDER BY timestamp ASC
```

### 6.3 Enrichment Through Iteration

Each iteration adds to the field:

```
Iteration 1:
  vset_abc123 (5 variations)
  + 12 specialist contributions
  + 1 producer synthesis
  + 3 curated options

Iteration 2 (based on human feedback):
  vset_def456 (2 refinements of var_1)
  + 8 specialist contributions
  + 1 producer synthesis
  + 2 curated options
  + Links back to vset_abc123

Iteration 3:
  vset_ghi789 (3 further refinements)
  + ...

Total knowledge:
  - 10 variations
  - 35 contributions
  - 5 synthesis notes
  - 8 curated options
  - Clear lineage/provenance
```

This rich field becomes a **knowledge base** for future work.

---

## 7. Producer as Curator

### 7.1 Producer Responsibilities

**Not:**
- ❌ Dictating "the right answer"
- ❌ Overriding specialist expertise
- ❌ Making final decisions (human does)

**Instead:**
- ✅ Reading all specialist contributions
- ✅ Identifying themes and patterns
- ✅ Highlighting complementary perspectives
- ✅ Curating multiple options
- ✅ Explaining tradeoffs
- ✅ Facilitating iteration

**Like a music producer:**
- Listens to all takes
- Knows each musician's strengths
- Suggests "Take 3 for verse, Take 1 for chorus"
- Doesn't play the instruments

### 7.2 Curation Strategies

**Strategy 1: Present Best + Alternatives**
```
Option 1: Variation 1 (highest rated by most specialists)
Option 2: Variation 3 (interesting alternative with different strengths)
Option 3: Combination of var_1 melody + var_3 harmony
```

**Strategy 2: Present Tradeoffs**
```
If you want HIGH ENERGY: Variation 1 (0.81)
If you want HARMONIC FLEXIBILITY: Variation 3 (enables progression)
If you want SAFETY: Variation 4 (most neutral/versatile)
```

**Strategy 3: Present Refinement Paths**
```
Starting Point: Variation 1 (strong foundation)
Path A: Add orchestration (per orchestration specialist)
Path B: Adjust range (per melody specialist concern)
Path C: Try both changes
```

### 7.3 Example Producer Synthesis

```markdown
## Producer's Synthesis

After reviewing contributions from 4 specialists, here's what I'm hearing:

### Themes

**Melody:** Variation 1 stands out with clear motivic development (melody specialist).
Variation 3 is also strong. Variation 2 has range concerns.

**Harmony:** Three different harmonic strategies emerged:
- Var_1 (G major): Bright, energetic
- Var_3 (D major): Enables progression flexibility
- Var_4 (C major): Neutral/versatile
All are valid depending on context.

**Rhythm:** Variation 1 has highest energy (132 bpm). All variations use straight
rhythms - syncopation could be explored in refinement.

**Orchestration:** Current variations have sparse instrumentation (good for leaving room),
but specialist suggests adding warm pad to var_1.

### Curated Options

I'm presenting 3 options that represent different creative directions:

**Option 1: "The Energetic Choice"**
- Use: Variation 1 as-is
- Why: Strongest melody, highest energy, bright harmonic quality
- Tradeoff: G major may limit verse options
- Ready to use immediately

**Option 2: "The Strategic Choice"**
- Use: Variation 3
- Why: D major enables V-I progression, good melody, build-up potential
- Tradeoff: Slightly lower energy
- Ready to use immediately

**Option 3: "The Enhanced Choice"**
- Use: Variation 1 + refinement
- Why: Takes strongest option and adds orchestration depth
- Tradeoff: Requires one refinement pass (quick)
- Adds: Warm pad per orchestration specialist

All three are musically sound. Choice depends on your vision for the piece.

### My Recommendation

Try Option 1 first - it's ready to go and has the strongest foundation. If you want
more warmth, we can quickly create Option 3. Option 2 is there if harmonic progression
is a priority.

Happy to iterate on any of these!
```

---

## 8. Benefits of Contribution Model

### 8.1 Versus Voting

| Aspect | Voting | Contributions |
|--------|--------|---------------|
| **Diversity** | Compressed to single number | Preserved in full |
| **Expertise** | Ignored (all votes equal) | Explicitly valued |
| **Learning** | Minimal (just scores) | Rich (reasoning visible) |
| **Flexibility** | Binary (chosen/not) | Continuous (many uses) |
| **Collaboration** | Competitive | Complementary |
| **Iteration** | Discards losers | Builds on all data |

### 8.2 For Agents

**Better Prompts:**
```
Instead of: "Vote on which variation is best (0.0-1.0)"
Use: "As a melody specialist, what do you observe about the melodic aspects?"
```

**Clearer Roles:**
```
Instead of: "You are an AI voting on music"
Use: "You are a melody specialist contributing expertise to a collaborative session"
```

**Richer Context:**
```
Instead of: "3 agents voted for var_1 (avg: 0.82)"
Use: "Melody specialist noted strong phrasing. Harmony specialist noted bright key.
      Rhythm specialist noted highest energy. Producer synthesizes: strong candidate."
```

### 8.3 For Humans

**Better Understanding:**
- See WHY each option is interesting
- Understand tradeoffs
- Learn from specialist perspectives

**Better Decisions:**
- Multiple viable options, not forced choice
- Clear rationale for each
- Can iterate based on reasoning

**Better Collaboration:**
- Feels like working with a band, not a committee
- Expertise is respected
- Iteration is natural

---

## 9. Implementation Impact

### 9.1 Changes to Variation Design

**Keep (from original design):**
- ✅ VariationSet as container
- ✅ VariationItem structure
- ✅ Rich metadata
- ✅ Refinement/exploration trees
- ✅ Provenance chains
- ✅ CAS integration

**Remove:**
- ❌ SelectionState.votes
- ❌ Vote struct
- ❌ Consensus detection
- ❌ Ranking systems

**Add:**
- ✅ ContributionCollection
- ✅ Contribution types
- ✅ AgentRole enum
- ✅ ProductionState
- ✅ CuratedOption
- ✅ SynthesisNote
- ✅ HumanFeedback

### 9.2 Migration Path

**Phase 1:** Add contribution structures (alongside voting for compatibility)
**Phase 2:** Implement contribution endpoints
**Phase 3:** Update MCP tools to use contributions
**Phase 4:** Remove voting code (breaking change)

---

## 10. Example: Full Session

```
┌──────────────────────────────────────────────────────┐
│ Session: Create Intro for "Morning Light"           │
└──────────────────────────────────────────────────────┘

Amy: "I need an upbeat intro melody for a song called 'Morning Light'.
      Something optimistic and energetic."

Producer (Claude):
  "I'll gather the ensemble to explore options."

  create_variation_set({
    intent: "Upbeat intro melody for 'Morning Light' - optimistic and energetic",
    operation: {tool: "orpheus_generate", params: {target_energy: 0.75}},
    num_variations: 5
  })

  → vset_morning_intro created

  "I've generated 5 variations. Let me bring in the specialists."

─────────────────────────────────────────────────────────

Melody Specialist (Gemini):
  contribute({
    role: "MelodySpecialist",
    scope: {SingleVariation: 1},
    content: {
      Assessment: {
        observations: ["Rising contour suggests optimism", "8-bar phrase is memorable"],
        strengths: ["Clear hook in measures 3-4", "Singable range"],
        concerns: []
      }
    }
  })

  contribute({
    role: "MelodySpecialist",
    scope: {SingleVariation: 3},
    content: {
      Assessment: {
        observations: ["Stepwise motion, very smooth"],
        strengths: ["Easy to remember"],
        concerns: [{issue: "May be too simple", severity: "Note"}]
      }
    }
  })

Harmony Specialist (DeepSeek):
  contribute({
    role: "HarmonySpecialist",
    scope: {MultipleVariations: [1, 3, 4]},
    content: {
      Assessment: {
        observations: [
          "Var_1 in G major - bright and optimistic",
          "Var_3 in D major - energetic, suggests dominant feel",
          "Var_4 in C major - classic, versatile"
        ],
        strengths: ["All use functional harmony"],
        concerns: []
      }
    }
  })

Rhythm Specialist (Claude):
  contribute({
    role: "RhythmSpecialist",
    scope: {WholeSet},
    content: {
      Assessment: {
        observations: [
          "Tempo range 120-135 bpm, all 'upbeat'",
          "Var_1 at 132 bpm is most energetic",
          "Var_2 at 120 bpm more relaxed"
        ],
        strengths: ["Good variety in energy levels"],
        concerns: []
      }
    }
  })

─────────────────────────────────────────────────────────

Producer (Claude):
  "Let me synthesize what we're hearing..."

  synthesize_contributions(vset_morning_intro)

  → SynthesisNote created

  "Based on specialist feedback, I'm seeing 2 strong candidates:

   - Variation 1: High energy (132 bpm), G major brightness, strong melodic hook
   - Variation 3: Moderate energy (128 bpm), D major dominant feel, smooth melody

   Let me curate some options for you."

  curate_options({
    options: [
      {
        description: "High-energy bright opener (Variation 1)",
        uses_variations: [1],
        rationale: "Melody specialist: strong hook. Harmony: bright G major.
                   Rhythm: highest energy. Best match for 'optimistic and energetic'.",
        notes: "This is the bold, confident choice"
      },
      {
        description: "Smooth energetic flow (Variation 3)",
        uses_variations: [3],
        rationale: "Melody specialist: memorable and smooth. Harmony: D major suggests
                   dominant/anticipation. Slightly lower energy allows build-up.",
        notes: "This is the anticipatory, building choice"
      }
    ]
  })

─────────────────────────────────────────────────────────

Producer (Claude):
  "Amy, I've curated 2 options for you:

   **Option 1: Bold & Bright** (Variation 1)
   - 132 bpm, G major, strong melodic hook
   - Most energetic and optimistic
   - Ready to use as-is

   **Option 2: Smooth & Building** (Variation 3)
   - 128 bpm, D major, smooth melody
   - Anticipatory feel, room to build
   - Ready to use as-is

   Both work well for 'Morning Light'. Option 1 is more immediately energetic,
   Option 2 creates anticipation. Your call!"

─────────────────────────────────────────────────────────

Amy: "I like option 1, but is 132 bpm too fast? Can we try around 125?"

add_human_feedback({
  feedback_type: "Direction",
  content: "Like option 1 melody/harmony, but try 125 bpm",
  regarding: {CuratedOption: option_1}
})

Producer (Claude):
  "Let me create a refinement with adjusted tempo."

  refine_variation({
    parent_set: vset_morning_intro,
    parent_variation: 1,
    intent: "Adjust tempo to 125 bpm per Amy's feedback",
    operation: {
      tool: "orpheus_generate_seeded",
      params: {
        seed_hash: "var_1_hash",
        constraints: {
          preserve_melody: true,
          preserve_harmony: true,
          target_tempo: 125
        }
      }
    },
    num_variations: 2
  })

  → vset_morning_intro_refined created

  "Created 2 refinements at ~125 bpm. Specialists are reviewing..."

[Specialists contribute to refined set...]

Producer (Claude):
  "Perfect! Refinement variation 0 hits 124 bpm, preserves the melody and harmony
   you liked. This is ready to use."

Amy: "Great, let's go with that!"

─────────────────────────────────────────────────────────

Result:
  - 7 total variations explored (5 + 2 refinements)
  - 15 specialist contributions
  - 2 synthesis notes
  - 4 curated options
  - 2 human feedback items
  - Rich knowledge base for future work
  - Decision reached through collaboration, not voting
```

---

## Conclusion

**The Contribution Model:**

✅ **Respects Expertise** - Each specialist contributes from their domain
✅ **Enriches Understanding** - All perspectives add value
✅ **Enables Iteration** - Builds on rich data field
✅ **Feels Natural** - Like a studio session or code review
✅ **Empowers Humans** - Curated options with clear rationale

**Versus Voting:**

- Voting asks: "Which is best?"
- Contributions ask: "What do you hear?"

The first compresses diverse expertise into numbers.
The second honors each voice and builds collective understanding.

**Next:** Implement contribution model in Phase 3 of variation system.
