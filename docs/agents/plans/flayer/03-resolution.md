# 03: Resolution

**File:** `src/resolution.rs`
**Focus:** Turning GenerateContent regions into concrete content via MCP
**Dependencies:** `anyhow`, calls into `hootenanny` MCP tools

---

## Task

Create `crates/flayer/src/resolution.rs` with ResolutionEngine that processes GenerateContent regions by calling MCP tools and storing results.

**Why this first?** Generative regions must resolve before rendering. This is what makes "latent content" work — the bridge between abstract intentions and concrete audio/MIDI.

**Deliverables:**
1. `resolution.rs` with ResolutionContext, ResolutionEngine, filters module
2. Tests using mock mcp_caller to verify resolution flow, retry logic, filter composition

**Definition of Done:**
```bash
cargo fmt --check
cargo clippy -- -D warnings
cargo check
cargo test
```

## Out of Scope

- ❌ Actual MCP client — use closure/trait injection
- ❌ Hootenanny integration — comes later
- ❌ Rendering resolved content — task 04

Focus ONLY on the resolution flow and quality filtering.

---

## Concept

Resolution takes regions with `Behavior::GenerateContent` and:
1. Calls the specified MCP tool (e.g., `orpheus_generate`)
2. Optionally filters quality
3. Stores result hash in the region's `resolved` field

This happens **before** rendering, not during.

---

## Types

```rust
#[derive(Debug, Clone)]
pub struct GenerationResult {
    pub artifact_id: String,
    pub content_hash: String,
    pub duration_beats: Beat,
    pub content_type: ContentType,
    pub metadata: GenerationMetadata,
}

#[derive(Debug, Clone, Default)]
pub struct GenerationMetadata {
    pub quality_score: Option<f64>,
    pub detected_tempo: Option<f64>,
    pub detected_key: Option<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum FilterDecision {
    Accept,
    Retry { reason: String, adjusted_params: serde_json::Value },
    Reject { reason: String },
}

pub struct ResolutionContext {
    pub mcp_caller: Box<dyn Fn(&str, serde_json::Value) -> Result<GenerationResult> + Send + Sync>,
    pub quality_filter: Option<Box<dyn Fn(&GenerationResult, &Region) -> FilterDecision + Send + Sync>>,
    pub max_retries: u32,
    pub parallel: bool,
}

pub struct ResolutionEngine {
    ctx: ResolutionContext,
}

#[derive(Debug, Default)]
pub struct ResolutionReport {
    pub resolved: Vec<Uuid>,
    pub failed: Vec<(Uuid, String)>,
    pub artifacts: Vec<String>,
}
```

---

## ResolutionEngine Methods

- `new(ctx: ResolutionContext) -> Self`
- `resolve_all(&self, regions: &mut [Region]) -> Result<ResolutionReport>`
- `resolve_region(&self, region: &mut Region) -> Result<Option<GenerationResult>>`

**Resolution flow:**
1. Skip if not `GenerateContent` behavior
2. Skip if already resolved
3. Call `mcp_caller(tool, params)`
4. Apply `quality_filter` if present
5. On `Accept`: set `resolved` field
6. On `Retry`: adjust params, loop (up to `max_retries`)
7. On `Reject`: return error

---

## MCP Caller Interface

The `mcp_caller` function wraps actual MCP tool dispatch. Signature:

```rust
fn mcp_caller(tool: &str, params: serde_json::Value) -> Result<GenerationResult>
```

Tool names match hootenanny MCP tools:
- `orpheus_generate`
- `orpheus_continue`
- `orpheus_bridge`
- `musicgen_generate`

---

## Quality Filters

Composable filter functions:

```rust
pub mod filters {
    pub fn duration_tolerance(tolerance: f64) -> impl Fn(&GenerationResult, &Region) -> FilterDecision;
    pub fn min_quality(min: f64) -> impl Fn(&GenerationResult, &Region) -> FilterDecision;
    pub fn all<F1, F2>(f1: F1, f2: F2) -> impl Fn(&GenerationResult, &Region) -> FilterDecision;
}
```

---

## Acceptance Criteria

- [ ] `resolve_all` processes all `GenerateContent` regions
- [ ] Resolved regions have `resolved: Some(...)` set
- [ ] Quality filter can trigger retry
- [ ] `max_retries` is respected
- [ ] `ResolutionReport` tracks successes and failures
- [ ] Non-generative regions are skipped (not errors)
