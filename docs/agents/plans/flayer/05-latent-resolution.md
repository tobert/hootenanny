# Task 04: Latent Resolution

**Priority:** High
**Estimated Sessions:** 2-3
**Depends On:** 01-core-structs, 03-renderer

---

## Objective

Implement the latent resolution system - the key innovation of flayer. This enables:
- **Eager resolution** - resolve all latents before render (for live/predictable timing)
- **Lazy resolution** - resolve during render as needed (for batch processing)
- **Continuation chains** - latents can seed from prior latent outputs

## Concept

A **Latent** is a region on the timeline defined by generation parameters rather than concrete data. During resolution, we:

1. Topologically sort latents (handle dependencies)
2. Call MCP tools to generate content
3. Store results as resolved Clips
4. Render proceeds normally with resolved clips

## Files to Create/Modify

### Create `crates/flayer/src/resolve.rs`

```rust
use crate::{AudioBuffer, Clip, ClipSource, AudioSource, Latent, LatentParams, SeedSource, Timeline, Track};
use anyhow::{anyhow, Context, Result};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

/// Context for resolving latents
pub struct ResolveContext {
    /// MCP tool caller - returns artifact hash
    pub mcp_caller: Box<dyn Fn(&str, serde_json::Value) -> Result<ResolveResult> + Send + Sync>,

    /// For PriorContext seed source - render timeline up to a point
    pub render_context: Option<Box<dyn Fn(&Timeline, f64) -> Result<String> + Send + Sync>>,

    /// Quality filter for generation QA (inspired by SMITIN research)
    /// Returns Accept to use the result, Retry to regenerate, or Reject to fail
    pub quality_filter: Option<Box<dyn Fn(&ResolveResult, &Latent) -> FilterDecision + Send + Sync>>,

    /// Maximum retry attempts when quality filter returns Retry
    pub max_retries: u32,
}

/// Result from MCP tool call
#[derive(Debug, Clone)]
pub struct ResolveResult {
    pub artifact_id: String,
    pub content_hash: String,
    pub duration_beats: f64,
    pub sample_rate: Option<u32>,

    /// Optional metadata for quality filtering
    pub metadata: Option<ResolveMetadata>,
}

/// Metadata about the generation for quality assessment
#[derive(Debug, Clone, Default)]
pub struct ResolveMetadata {
    /// Model-reported confidence/quality score (0.0-1.0)
    pub quality_score: Option<f64>,

    /// Detected tempo (e.g., from beatthis analysis)
    pub detected_tempo: Option<f64>,

    /// Detected key/mode
    pub detected_key: Option<String>,

    /// Note density (notes per beat)
    pub note_density: Option<f64>,

    /// Whether the model flagged this as potentially low quality
    pub flagged: bool,
}

/// Decision from quality filter
#[derive(Debug, Clone)]
pub enum FilterDecision {
    /// Accept this generation
    Accept,

    /// Retry with adjusted parameters
    Retry {
        /// Adjusted parameters for retry
        adjusted_params: LatentParams,
        /// Reason for retry (for logging)
        reason: String,
    },

    /// Reject entirely (fail the resolution)
    Reject {
        /// Reason for rejection
        reason: String,
    },
}

impl ResolveContext {
    pub fn new() -> Self {
        Self {
            mcp_caller: Box::new(|_, _| Err(anyhow!("No MCP caller configured"))),
            render_context: None,
            quality_filter: None,
            max_retries: 3,
        }
    }

    pub fn with_mcp_caller<F>(mut self, caller: F) -> Self
    where
        F: Fn(&str, serde_json::Value) -> Result<ResolveResult> + Send + Sync + 'static,
    {
        self.mcp_caller = Box::new(caller);
        self
    }

    /// Add a quality filter for generation QA
    ///
    /// # Example
    /// ```
    /// let ctx = ResolveContext::new()
    ///     .with_quality_filter(|result, latent| {
    ///         // Reject if duration is way off
    ///         let expected = latent.duration;
    ///         let actual = result.duration_beats;
    ///         if (actual - expected).abs() > expected * 0.5 {
    ///             return FilterDecision::Retry {
    ///                 adjusted_params: latent.params.clone(),
    ///                 reason: format!("Duration mismatch: expected {}, got {}", expected, actual),
    ///             };
    ///         }
    ///         FilterDecision::Accept
    ///     });
    /// ```
    pub fn with_quality_filter<F>(mut self, filter: F) -> Self
    where
        F: Fn(&ResolveResult, &Latent) -> FilterDecision + Send + Sync + 'static,
    {
        self.quality_filter = Some(Box::new(filter));
        self
    }

    pub fn with_max_retries(mut self, max: u32) -> Self {
        self.max_retries = max;
        self
    }
}

/// Common quality filters
pub mod filters {
    use super::*;

    /// Filter that checks duration is within tolerance of expected
    pub fn duration_tolerance(tolerance_ratio: f64) -> impl Fn(&ResolveResult, &Latent) -> FilterDecision {
        move |result, latent| {
            let expected = latent.duration;
            let actual = result.duration_beats;
            let diff_ratio = (actual - expected).abs() / expected;

            if diff_ratio > tolerance_ratio {
                FilterDecision::Retry {
                    adjusted_params: latent.params.clone(),
                    reason: format!(
                        "Duration mismatch: expected {:.2} beats, got {:.2} ({:.0}% off)",
                        expected, actual, diff_ratio * 100.0
                    ),
                }
            } else {
                FilterDecision::Accept
            }
        }
    }

    /// Filter that checks model-reported quality score
    pub fn min_quality_score(min_score: f64) -> impl Fn(&ResolveResult, &Latent) -> FilterDecision {
        move |result, latent| {
            if let Some(ref meta) = result.metadata {
                if let Some(score) = meta.quality_score {
                    if score < min_score {
                        // Retry with higher temperature for diversity
                        let mut adjusted = latent.params.clone();
                        adjusted.temperature = (adjusted.temperature * 1.1).min(2.0);
                        return FilterDecision::Retry {
                            adjusted_params: adjusted,
                            reason: format!("Quality score {:.2} below threshold {:.2}", score, min_score),
                        };
                    }
                }
            }
            FilterDecision::Accept
        }
    }

    /// Filter that rejects flagged generations
    pub fn reject_flagged() -> impl Fn(&ResolveResult, &Latent) -> FilterDecision {
        |result, _latent| {
            if let Some(ref meta) = result.metadata {
                if meta.flagged {
                    return FilterDecision::Retry {
                        adjusted_params: _latent.params.clone(),
                        reason: "Generation was flagged by model".to_string(),
                    };
                }
            }
            FilterDecision::Accept
        }
    }

    /// Combine multiple filters (all must Accept)
    pub fn all_of(
        filters: Vec<Box<dyn Fn(&ResolveResult, &Latent) -> FilterDecision + Send + Sync>>
    ) -> impl Fn(&ResolveResult, &Latent) -> FilterDecision {
        move |result, latent| {
            for filter in &filters {
                match filter(result, latent) {
                    FilterDecision::Accept => continue,
                    other => return other,
                }
            }
            FilterDecision::Accept
        }
    }
}

impl Timeline {
    /// Resolve all latents eagerly (before render)
    /// Use for: live performance, predictable timing
    pub fn resolve_latents(&mut self, ctx: &ResolveContext) -> Result<()> {
        // Get resolution order (handles dependencies)
        let order = self.latent_resolution_order()?;

        for latent_id in order {
            self.resolve_single_latent(latent_id, ctx)?;
        }

        Ok(())
    }

    /// Check if all latents are resolved
    pub fn all_latents_resolved(&self) -> bool {
        self.tracks.iter()
            .all(|t| t.latents.iter().all(|l| l.is_resolved()))
    }

    /// Get unresolved latent count
    pub fn unresolved_count(&self) -> usize {
        self.tracks.iter()
            .flat_map(|t| t.latents.iter())
            .filter(|l| !l.is_resolved())
            .count()
    }

    /// Topologically sort latents based on dependencies
    /// Returns latents in order: dependencies first, then dependents
    fn latent_resolution_order(&self) -> Result<Vec<Uuid>> {
        let mut all_latents: HashMap<Uuid, &Latent> = HashMap::new();
        // dependents[A] = list of nodes that depend on A (A must be resolved first)
        let mut dependents: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
        // in_degree[A] = number of nodes A depends on (must resolve before A)
        let mut in_degree: HashMap<Uuid, usize> = HashMap::new();

        // Collect all latents
        for track in &self.tracks {
            for latent in &track.latents {
                all_latents.insert(latent.id, latent);
                dependents.insert(latent.id, Vec::new());
                in_degree.insert(latent.id, 0);
            }
        }

        // Build dependency graph
        // If A seeds from B, then B must resolve before A
        // So: in_degree[A] += 1, and dependents[B].push(A)
        for latent in all_latents.values() {
            if let Some(SeedSource::Latent(dep_id)) = &latent.seed_from {
                if all_latents.contains_key(dep_id) {
                    // latent depends on dep_id
                    *in_degree.get_mut(&latent.id).unwrap() += 1;
                    dependents.get_mut(dep_id).unwrap().push(latent.id);
                }
            }
        }

        // Kahn's algorithm: start with nodes that have no dependencies
        let mut queue: Vec<Uuid> = in_degree.iter()
            .filter(|(_, &d)| d == 0)
            .map(|(&id, _)| id)
            .collect();

        // Sort by timeline position for deterministic order among independent nodes
        queue.sort_by(|a, b| {
            let a_at = all_latents[a].at;
            let b_at = all_latents[b].at;
            a_at.partial_cmp(&b_at).unwrap()
        });

        let mut result = Vec::new();

        while let Some(id) = queue.pop() {
            result.push(id);

            // For each node that depends on this one, decrement its in-degree
            for &dependent_id in &dependents[&id] {
                let degree = in_degree.get_mut(&dependent_id).unwrap();
                *degree -= 1;
                if *degree == 0 {
                    // Insert in sorted position by timeline order
                    let at = all_latents[&dependent_id].at;
                    let pos = queue.iter().position(|&q| all_latents[&q].at > at).unwrap_or(queue.len());
                    queue.insert(pos, dependent_id);
                }
            }
        }

        if result.len() != all_latents.len() {
            return Err(anyhow!("Circular dependency in latent chain"));
        }

        Ok(result)
    }

    fn resolve_single_latent(&mut self, latent_id: Uuid, ctx: &ResolveContext) -> Result<()> {
        // Find the latent and its track
        let (track_idx, latent_idx) = self.find_latent_indices(latent_id)?;
        let latent = &self.tracks[track_idx].latents[latent_idx];

        // Skip if already resolved
        if latent.is_resolved() {
            return Ok(());
        }

        // Get seed hash if continuation
        let seed_hash = self.get_seed_hash(latent, ctx)?;

        // Build initial params for MCP call
        let mut current_params = latent.params.clone();
        let mut attempts = 0;

        loop {
            attempts += 1;

            let params = self.build_mcp_params(&current_params, &seed_hash, latent);

            // Call MCP tool
            let result = (ctx.mcp_caller)(&latent.model, params)
                .context(format!("Failed to resolve latent {} with model {}", latent_id, latent.model))?;

            // Apply quality filter if configured
            if let Some(ref filter) = ctx.quality_filter {
                match filter(&result, latent) {
                    FilterDecision::Accept => {
                        // Quality check passed, create clip
                        let clip = self.create_clip_from_result(&result, latent);
                        self.tracks[track_idx].latents[latent_idx].resolved = Some(clip);
                        return Ok(());
                    }
                    FilterDecision::Retry { adjusted_params, reason } => {
                        if attempts >= ctx.max_retries {
                            // Max retries exceeded, accept anyway with warning
                            tracing::warn!(
                                "Latent {} exceeded max retries ({}), accepting last result. Last reason: {}",
                                latent_id, ctx.max_retries, reason
                            );
                            let clip = self.create_clip_from_result(&result, latent);
                            self.tracks[track_idx].latents[latent_idx].resolved = Some(clip);
                            return Ok(());
                        }
                        tracing::info!(
                            "Latent {} retry {}/{}: {}",
                            latent_id, attempts, ctx.max_retries, reason
                        );
                        current_params = adjusted_params;
                        continue;
                    }
                    FilterDecision::Reject { reason } => {
                        return Err(anyhow!("Latent {} rejected by quality filter: {}", latent_id, reason));
                    }
                }
            } else {
                // No filter, accept immediately
                let clip = self.create_clip_from_result(&result, latent);
                self.tracks[track_idx].latents[latent_idx].resolved = Some(clip);
                return Ok(());
            }
        }
    }

    fn build_mcp_params(
        &self,
        params: &LatentParams,
        seed_hash: &Option<String>,
        latent: &Latent,
    ) -> serde_json::Value {
        let mut json_params = serde_json::json!({
            "temperature": params.temperature,
            "top_p": params.top_p,
        });

        if let Some(prompt) = &params.prompt {
            json_params["prompt"] = serde_json::json!(prompt);
        }

        if let Some(max_tokens) = params.max_tokens {
            json_params["max_tokens"] = serde_json::json!(max_tokens);
        }

        if let Some(seed) = params.seed {
            json_params["seed"] = serde_json::json!(seed);
        }

        if let Some(hash) = seed_hash {
            json_params["seed_hash"] = serde_json::json!(hash);
        }

        // Include musical attributes if present
        if let Some(ref attrs) = params.attributes {
            if let Some(ref instrument) = attrs.instrument {
                json_params["instrument"] = serde_json::json!(instrument);
            }
            if let Some(density) = attrs.density {
                json_params["density"] = serde_json::json!(density);
            }
            if let Some(polyphony) = attrs.polyphony {
                json_params["polyphony"] = serde_json::json!(polyphony);
            }
            if let Some(ref style) = attrs.style {
                json_params["style"] = serde_json::json!(style);
            }
        }

        // Include mode-specific params
        match &latent.mode {
            LatentMode::Infill { before_context_beats, after_context_beats } => {
                json_params["mode"] = serde_json::json!("infill");
                json_params["before_context_beats"] = serde_json::json!(before_context_beats);
                json_params["after_context_beats"] = serde_json::json!(after_context_beats);
            }
            LatentMode::Variation { source_hash, preserve_rhythm, preserve_harmony } => {
                json_params["mode"] = serde_json::json!("variation");
                json_params["source_hash"] = serde_json::json!(source_hash);
                json_params["preserve_rhythm"] = serde_json::json!(preserve_rhythm);
                json_params["preserve_harmony"] = serde_json::json!(preserve_harmony);
            }
            LatentMode::Generate => {
                json_params["mode"] = serde_json::json!("generate");
            }
        }

        // Merge extra params
        if let serde_json::Value::Object(extra) = &params.extra {
            for (k, v) in extra {
                json_params[k] = v.clone();
            }
        }

        json_params
    }

    fn create_clip_from_result(&self, result: &ResolveResult, latent: &Latent) -> Clip {
        Clip {
            id: Uuid::new_v4(),
            source: ClipSource::Audio(AudioSource {
                hash: result.content_hash.clone(),
                sample_rate: result.sample_rate.unwrap_or(44100),
                channels: 2,
                duration_samples: 0, // Will be determined on load
            }),
            at: latent.at,
            duration: result.duration_beats,
            source_offset: 0.0,
            source_duration: result.duration_beats,
            playback_rate: 1.0,
            reverse: false,
            gain: 1.0,
            fade_in: 0.0,
            fade_out: 0.0,
            effects: Vec::new(),
        }
    }

    fn get_seed_hash(&self, latent: &Latent, ctx: &ResolveContext) -> Result<Option<String>> {
        match &latent.seed_from {
            None => Ok(None),

            Some(SeedSource::Clip(hash)) => Ok(Some(hash.clone())),

            Some(SeedSource::Latent(dep_id)) => {
                let (track_idx, latent_idx) = self.find_latent_indices(*dep_id)?;
                let dep_latent = &self.tracks[track_idx].latents[latent_idx];

                let resolved = dep_latent.resolved.as_ref()
                    .ok_or_else(|| anyhow!("Dependency latent {} not resolved", dep_id))?;

                match &resolved.source {
                    ClipSource::Audio(src) => Ok(Some(src.hash.clone())),
                    ClipSource::Midi(src) => Ok(Some(src.hash.clone())),
                }
            }

            Some(SeedSource::PriorContext { beats }) => {
                // Render the N beats before this latent, return hash
                if let Some(render_fn) = &ctx.render_context {
                    let hash = render_fn(self, latent.at - beats)?;
                    Ok(Some(hash))
                } else {
                    Err(anyhow!("PriorContext seed requires render_context in ResolveContext"))
                }
            }
        }
    }

    fn find_latent_indices(&self, latent_id: Uuid) -> Result<(usize, usize)> {
        for (track_idx, track) in self.tracks.iter().enumerate() {
            for (latent_idx, latent) in track.latents.iter().enumerate() {
                if latent.id == latent_id {
                    return Ok((track_idx, latent_idx));
                }
            }
        }
        Err(anyhow!("Latent {} not found", latent_id))
    }
}

/// Lazy resolution during render
impl Timeline {
    /// Resolve a single latent on-demand (for lazy mode)
    pub fn resolve_latent_lazy(&mut self, latent_id: Uuid, ctx: &ResolveContext) -> Result<&Clip> {
        // Check if already resolved
        let (track_idx, latent_idx) = self.find_latent_indices(latent_id)?;
        if self.tracks[track_idx].latents[latent_idx].is_resolved() {
            return Ok(self.tracks[track_idx].latents[latent_idx].resolved.as_ref().unwrap());
        }

        // Resolve dependencies first (recursive)
        let latent = &self.tracks[track_idx].latents[latent_idx];
        if let Some(SeedSource::Latent(dep_id)) = &latent.seed_from {
            let dep_id = *dep_id;
            self.resolve_latent_lazy(dep_id, ctx)?;
        }

        // Now resolve this one
        self.resolve_single_latent(latent_id, ctx)?;

        let (track_idx, latent_idx) = self.find_latent_indices(latent_id)?;
        Ok(self.tracks[track_idx].latents[latent_idx].resolved.as_ref().unwrap())
    }
}
```

### Update `crates/flayer/src/lib.rs`

```rust
pub mod resolve;

pub use resolve::{ResolveContext, ResolveResult};
```

### Update `crates/flayer/src/render.rs`

Add lazy resolution support:

```rust
use crate::resolve::ResolveContext;
use crate::ResolutionMode;

pub struct RenderContext {
    // ... existing fields ...

    /// Resolution mode for latents
    pub resolution_mode: ResolutionMode,

    /// For lazy resolution
    pub resolve_context: Option<ResolveContext>,
}

impl Timeline {
    pub fn render(&mut self, ctx: &mut RenderContext) -> Result<AudioBuffer> {
        // 1. Lazy Resolution Pass
        if ctx.resolution_mode == ResolutionMode::Lazy {
            if let Some(resolve_ctx) = &ctx.resolve_context {
                // Collect all unresolved latent IDs first to avoid borrowing conflicts
                // We cannot iterate self.tracks (immutable) while calling resolve (mutable)
                let unresolved_ids: Vec<Uuid> = self.tracks.iter()
                    .flat_map(|t| t.latents.iter())
                    .filter(|l| !l.is_resolved())
                    .map(|l| l.id)
                    .collect();

                // Resolve them one by one
                // resolve_latent_lazy handles dependencies recursively
                for id in unresolved_ids {
                    // Check again if resolved (dependency might have resolved it)
                    let already_resolved = self.tracks.iter()
                        .flat_map(|t| t.latents.iter())
                        .find(|l| l.id == id)
                        .map(|l| l.is_resolved())
                        .unwrap_or(false);

                    if !already_resolved {
                        self.resolve_latent_lazy(id, resolve_ctx)?;
                    }
                }
            }
        }

        // 2. Render Pass
        let duration_beats = self.total_duration_beats();
        let duration_seconds = duration_beats * 60.0 / self.bpm;
        let num_samples = (duration_seconds * ctx.sample_rate as f64).ceil() as usize;

        let mut output = AudioBuffer::new(num_samples, ctx.sample_rate);

        for track in &self.tracks {
            if track.muted {
                continue;
            }

            // Eager mode check
            if ctx.resolution_mode == ResolutionMode::Eager {
                for latent in &track.latents {
                    if !latent.is_resolved() {
                        return Err(anyhow!("Latent {} not resolved in eager mode", latent.id));
                    }
                }
            }

            let track_buffer = self.render_track(track, ctx)?;
            output.mix_at(&track_buffer, 0, track.volume, track.pan);
        }

        Ok(output)
    }
}
```

## Lua Integration Example

```lua
local flayer = require("flayer")

local tl = flayer.Timeline.new(120)
local drums = tl:add_track("Drums")

-- Add latent with continuation
drums:add_latent({
    at = 0,
    duration = 8,
    model = "orpheus_generate",
    params = { temperature = 1.0, prompt = "funky drums" }
})

drums:add_latent({
    at = 8,
    duration = 8,
    model = "orpheus_continue",
    seed_from = "prior",  -- Continues from previous latent
    params = { temperature = 0.9 }
})

-- Eager resolution (for live performance)
tl:resolve_latents()
tl:render("output.wav")

-- OR lazy resolution (for batch processing)
tl:render("output.wav", { resolution_mode = "lazy" })
```

## Acceptance Criteria

- [ ] `resolve_latents()` resolves all latents in dependency order
- [ ] `SeedSource::Latent` chains work correctly
- [ ] `SeedSource::Clip` uses specified hash as seed
- [ ] `SeedSource::PriorContext` renders prior content as seed
- [ ] Circular dependencies detected and error reported
- [ ] Eager mode fails if unresolved latents exist
- [ ] Lazy mode resolves on-demand during render

## Tests to Write

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_simple_resolution() {
        // Create timeline with one latent, mock MCP call, verify resolved
    }

    #[test]
    fn test_continuation_chain() {
        // Create A -> B -> C chain, verify resolution order
    }

    #[test]
    fn test_circular_dependency() {
        // Create A -> B -> A, verify error
    }

    #[test]
    fn test_eager_mode_fails_unresolved() {
        // Create latent, don't resolve, render in eager mode, verify error
    }
}
```
