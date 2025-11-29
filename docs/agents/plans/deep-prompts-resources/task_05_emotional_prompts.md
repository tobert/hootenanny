# Task 05: Emotional Intelligence Prompts

**Goal**: Create prompts that reason about emotional arcs, mood transitions, and the intent-to-sound mapping.

## The Emotional Layer

Hootenanny's unique strength is the `EmotionalVector`:

```rust
pub struct EmotionalVector {
    pub valence: f64,   // -1.0 (sad) to +1.0 (happy)
    pub arousal: f64,   // 0.0 (calm) to 1.0 (excited)
    pub agency: f64,    // -1.0 (reactive) to +1.0 (proactive)
}
```

This maps to sound via the realization system:
- **Arousal** → velocity (louder = more energy), duration (shorter = more energy)
- **Valence** → mode/color (minor = negative, major = positive)
- **Agency** → rhythmic placement (ahead of beat = proactive, behind = reactive)

## Current Gap

Agents can't:
- Query the emotional state of a branch
- Understand how emotions mapped to sound
- Plan emotional transitions
- Reason about mood coherence

## Proposed Prompts

### 1. `emotional-arc`
Analyze the emotional trajectory of a branch.

```rust
Prompt::new("emotional-arc")
    .with_title("Emotional Arc Analysis")
    .with_description("Analyze how emotions evolved over a branch")
    .argument("branch_id", "Branch to analyze (default: current)", false)
```

### 2. `mood-transition`
Plan a transition between emotional states.

```rust
Prompt::new("mood-transition")
    .with_title("Plan Mood Transition")
    .with_description("Plan how to transition between emotional states")
    .argument("from_mood", "Starting mood (e.g., tense, peaceful, joyful)", true)
    .argument("to_mood", "Target mood", true)
    .argument("bars", "Bars for transition", false)
```

### 3. `express-emotion`
Suggest how to express a specific emotion in sound.

```rust
Prompt::new("express-emotion")
    .with_title("Express Emotion")
    .with_description("Suggest sonic expression of an emotional state")
    .argument("valence", "Valence (-1.0 to 1.0)", true)
    .argument("arousal", "Arousal (0.0 to 1.0)", true)
    .argument("agency", "Agency (-1.0 to 1.0)", false)
```

### 4. `emotional-coherence`
Evaluate if emotions across a branch are coherent.

```rust
Prompt::new("emotional-coherence")
    .with_title("Check Emotional Coherence")
    .with_description("Evaluate if the emotional journey makes sense")
    .argument("branch_id", "Branch to evaluate", false)
```

### 5. `articulation-from-emotion`
Suggest note articulation based on emotional state.

```rust
Prompt::new("articulation-from-emotion")
    .with_title("Articulation Guide")
    .with_description("Suggest how to articulate notes for an emotional state")
    .argument("emotion", "Mood descriptor (e.g., anxious, serene, triumphant)", true)
```

## Implementation

### Helper: Emotion Analysis

```rust
impl HootHandler {
    fn analyze_branch_emotions(&self, branch_id: BranchId) -> Vec<EmotionalPoint> {
        let state = self.server.state.lock().unwrap();
        state.tree.nodes.values()
            .filter(|n| n.branch_id == branch_id)
            .map(|n| EmotionalPoint {
                node_id: n.id,
                timestamp: n.timestamp,
                emotion: n.emotion.clone(),
            })
            .collect()
    }

    fn emotion_from_descriptor(descriptor: &str) -> EmotionalVector {
        match descriptor.to_lowercase().as_str() {
            "peaceful" | "serene" | "calm" => EmotionalVector { valence: 0.3, arousal: 0.2, agency: -0.2 },
            "joyful" | "happy" | "elated" => EmotionalVector { valence: 0.8, arousal: 0.7, agency: 0.5 },
            "sad" | "melancholic" | "sorrowful" => EmotionalVector { valence: -0.6, arousal: 0.2, agency: -0.4 },
            "anxious" | "tense" | "nervous" => EmotionalVector { valence: -0.3, arousal: 0.8, agency: -0.3 },
            "angry" | "aggressive" | "fierce" => EmotionalVector { valence: -0.5, arousal: 0.9, agency: 0.7 },
            "triumphant" | "victorious" => EmotionalVector { valence: 0.9, arousal: 0.8, agency: 0.9 },
            "mysterious" | "eerie" => EmotionalVector { valence: -0.2, arousal: 0.4, agency: -0.5 },
            "nostalgic" | "bittersweet" => EmotionalVector { valence: -0.1, arousal: 0.3, agency: -0.3 },
            _ => EmotionalVector { valence: 0.0, arousal: 0.5, agency: 0.0 },
        }
    }

    fn describe_realization(e: &EmotionalVector) -> String {
        let velocity = ((e.arousal * 0.5 + 0.5) * 127.0) as u8;
        let duration_factor = 1.0 - (e.arousal * 0.5);
        let timing_offset = e.agency * 20.0; // ms ahead/behind

        format!(
            "Velocity: ~{} ({})\n\
            Duration: {}x normal ({})\n\
            Timing: {} beat ({}ms {})\n\
            Color: {} tendency",
            velocity,
            if e.arousal > 0.6 { "punchy" } else if e.arousal < 0.3 { "soft" } else { "moderate" },
            format!("{:.1}", duration_factor),
            if e.arousal > 0.6 { "staccato" } else if e.arousal < 0.3 { "legato" } else { "normal" },
            if e.agency > 0.2 { "ahead of" } else if e.agency < -0.2 { "behind" } else { "on" },
            timing_offset.abs() as i32,
            if e.agency > 0.0 { "early" } else { "late" },
            if e.valence > 0.3 { "major/bright" } else if e.valence < -0.3 { "minor/dark" } else { "neutral" }
        )
    }
}
```

### Implement prompts

```rust
async fn get_prompt(
    &self,
    name: &str,
    arguments: HashMap<String, String>,
) -> Result<GetPromptResult, ErrorData> {
    match name {
        // ... existing prompts ...

        "emotional-arc" => {
            let branch_id: BranchId = arguments.get("branch_id")
                .and_then(|s| s.parse().ok())
                .unwrap_or(0); // Default to main branch

            let points = self.analyze_branch_emotions(branch_id);

            if points.is_empty() {
                return Ok(GetPromptResult::new(vec![PromptMessage::user_text(
                    "No emotional data found for this branch."
                )]));
            }

            // Calculate trajectory
            let emotions: Vec<_> = points.iter()
                .map(|p| format!(
                    "Node {}: valence={:.2}, arousal={:.2}, agency={:.2} ({})",
                    p.node_id,
                    p.emotion.valence,
                    p.emotion.arousal,
                    p.emotion.agency,
                    describe_mood(&p.emotion)
                ))
                .collect();

            // Trend analysis
            let first = &points.first().unwrap().emotion;
            let last = &points.last().unwrap().emotion;
            let valence_trend = last.valence - first.valence;
            let arousal_trend = last.arousal - first.arousal;

            let trend_description = match (valence_trend > 0.2, arousal_trend > 0.2) {
                (true, true) => "Building toward joy/excitement",
                (true, false) => "Settling into contentment",
                (false, true) => "Building tension/anxiety",
                (false, false) if valence_trend < -0.2 => "Descending into melancholy",
                _ => "Relatively stable emotional landscape",
            };

            let prompt_text = format!(
                "Emotional arc analysis for branch {}:\n\n\
                Journey:\n{}\n\n\
                Overall trend: {}\n\
                Valence change: {:.2} ({} → {})\n\
                Arousal change: {:.2} ({} → {})\n\n\
                Consider:\n\
                - Is this arc intentional or accidental?\n\
                - Does it serve the musical narrative?\n\
                - Where might the emotional journey go next?",
                branch_id,
                emotions.join("\n"),
                trend_description,
                valence_trend,
                describe_mood(first),
                describe_mood(last),
                arousal_trend,
                if first.arousal > 0.5 { "energetic" } else { "calm" },
                if last.arousal > 0.5 { "energetic" } else { "calm" }
            );

            Ok(GetPromptResult::new(vec![PromptMessage::user_text(prompt_text)])
                .with_description(format!("Emotional arc: {}", trend_description)))
        }

        "mood-transition" => {
            let from_mood = arguments.get("from_mood")
                .ok_or_else(|| ErrorData::invalid_params("from_mood is required"))?;
            let to_mood = arguments.get("to_mood")
                .ok_or_else(|| ErrorData::invalid_params("to_mood is required"))?;
            let bars = arguments.get("bars").map(|s| s.as_str()).unwrap_or("8");

            let from_emotion = Self::emotion_from_descriptor(from_mood);
            let to_emotion = Self::emotion_from_descriptor(to_mood);

            let valence_shift = to_emotion.valence - from_emotion.valence;
            let arousal_shift = to_emotion.arousal - from_emotion.arousal;

            let transition_strategy = match (valence_shift.abs() > 0.5, arousal_shift.abs() > 0.3) {
                (true, true) => "Major emotional shift - use a bridge section",
                (true, false) => "Mood change at similar energy - gradual harmonic shift",
                (false, true) => "Energy change, same mood - rhythmic transformation",
                (false, false) => "Subtle transition - textural evolution",
            };

            let prompt_text = format!(
                "Plan a transition from '{}' to '{}' over {} bars.\n\n\
                From: valence={:.1}, arousal={:.1}, agency={:.1}\n\
                To: valence={:.1}, arousal={:.1}, agency={:.1}\n\n\
                Shift required:\n\
                - Valence: {:.2} ({})\n\
                - Arousal: {:.2} ({})\n\n\
                Suggested strategy: {}\n\n\
                Techniques:\n\
                - Valence shift: Change mode/key, alter chord extensions\n\
                - Arousal increase: Add layers, increase velocity, shorten notes\n\
                - Arousal decrease: Thin texture, reduce velocity, sustain notes\n\n\
                When generating MIDI, set the emotion parameter to intermediate values\n\
                across the transition bars.",
                from_mood, to_mood, bars,
                from_emotion.valence, from_emotion.arousal, from_emotion.agency,
                to_emotion.valence, to_emotion.arousal, to_emotion.agency,
                valence_shift,
                if valence_shift > 0.0 { "brightening" } else { "darkening" },
                arousal_shift,
                if arousal_shift > 0.0 { "intensifying" } else { "calming" },
                transition_strategy
            );

            Ok(GetPromptResult::new(vec![PromptMessage::user_text(prompt_text)])
                .with_description(format!("{} → {}", from_mood, to_mood)))
        }

        "express-emotion" => {
            let valence: f64 = arguments.get("valence")
                .and_then(|s| s.parse().ok())
                .ok_or_else(|| ErrorData::invalid_params("valence is required"))?;
            let arousal: f64 = arguments.get("arousal")
                .and_then(|s| s.parse().ok())
                .ok_or_else(|| ErrorData::invalid_params("arousal is required"))?;
            let agency: f64 = arguments.get("agency")
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.0);

            let emotion = EmotionalVector { valence, arousal, agency };
            let realization = Self::describe_realization(&emotion);

            let prompt_text = format!(
                "Express this emotional state in sound:\n\n\
                Emotion: {} (valence={:.1}, arousal={:.1}, agency={:.1})\n\n\
                How it translates to MIDI:\n{}\n\n\
                Suggested musical elements:\n\
                - Key: {}\n\
                - Tempo feel: {}\n\
                - Texture: {}\n\
                - Rhythm: {}\n\n\
                When using add_node, set:\n\
                emotion: {{ \"valence\": {:.1}, \"arousal\": {:.1}, \"agency\": {:.1} }}",
                describe_mood(&emotion),
                valence, arousal, agency,
                realization,
                if valence > 0.2 { "Major or Lydian" } else if valence < -0.2 { "Minor or Phrygian" } else { "Dorian or Mixolydian" },
                if arousal > 0.6 { "driving, energetic" } else if arousal < 0.3 { "spacious, breathing" } else { "moderate groove" },
                if arousal > 0.6 { "dense, layered" } else { "sparse, open" },
                if agency > 0.3 { "syncopated, pushing ahead" } else if agency < -0.3 { "laid back, behind the beat" } else { "straight, on the grid" },
                valence, arousal, agency
            );

            Ok(GetPromptResult::new(vec![PromptMessage::user_text(prompt_text)])
                .with_description(describe_mood(&emotion).to_string()))
        }

        "emotional-coherence" => {
            let branch_id: BranchId = arguments.get("branch_id")
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);

            let points = self.analyze_branch_emotions(branch_id);

            if points.len() < 2 {
                return Ok(GetPromptResult::new(vec![PromptMessage::user_text(
                    "Not enough nodes to evaluate coherence."
                )]));
            }

            // Check for jarring transitions
            let mut issues = Vec::new();
            for window in points.windows(2) {
                let prev = &window[0].emotion;
                let curr = &window[1].emotion;
                let jump = (
                    (curr.valence - prev.valence).abs(),
                    (curr.arousal - prev.arousal).abs(),
                );
                if jump.0 > 0.6 || jump.1 > 0.5 {
                    issues.push(format!(
                        "Sharp transition at node {}: {} → {} (Δvalence={:.2}, Δarousal={:.2})",
                        window[1].node_id,
                        describe_mood(prev),
                        describe_mood(curr),
                        jump.0, jump.1
                    ));
                }
            }

            let coherence_score = if issues.is_empty() {
                "High - emotions flow naturally"
            } else if issues.len() <= 2 {
                "Medium - some abrupt transitions"
            } else {
                "Low - many jarring emotional shifts"
            };

            let prompt_text = format!(
                "Emotional coherence analysis for branch {}:\n\n\
                Overall: {}\n\n\
                {}\n\n\
                Considerations:\n\
                - Abrupt shifts can be intentional (contrast, surprise)\n\
                - Or unintentional (inconsistent agent intentions)\n\
                - Consider adding transition nodes to smooth jarring jumps",
                branch_id,
                coherence_score,
                if issues.is_empty() {
                    "No jarring transitions detected.".to_string()
                } else {
                    format!("Issues found:\n{}", issues.join("\n"))
                }
            );

            Ok(GetPromptResult::new(vec![PromptMessage::user_text(prompt_text)])
                .with_description(coherence_score.to_string()))
        }

        "articulation-from-emotion" => {
            let emotion_desc = arguments.get("emotion")
                .ok_or_else(|| ErrorData::invalid_params("emotion is required"))?;

            let emotion = Self::emotion_from_descriptor(emotion_desc);
            let realization = Self::describe_realization(&emotion);

            let prompt_text = format!(
                "Articulation guide for '{}' mood:\n\n\
                Emotional vector: valence={:.1}, arousal={:.1}, agency={:.1}\n\n\
                Note-level articulation:\n{}\n\n\
                Performance suggestions:\n\
                - Attack: {}\n\
                - Sustain: {}\n\
                - Release: {}\n\
                - Dynamics: {}\n\n\
                Apply these when adding nodes with emotion parameters.",
                emotion_desc,
                emotion.valence, emotion.arousal, emotion.agency,
                realization,
                if emotion.arousal > 0.6 { "sharp, immediate" } else { "soft, gradual" },
                if emotion.arousal > 0.5 { "short, punchy" } else { "long, singing" },
                if emotion.agency > 0.0 { "clean cut" } else { "natural decay" },
                if emotion.arousal > 0.7 { "ff with accents" } else if emotion.arousal < 0.3 { "pp, whispered" } else { "mf, expressive" }
            );

            Ok(GetPromptResult::new(vec![PromptMessage::user_text(prompt_text)])
                .with_description(format!("Articulation for {}", emotion_desc)))
        }

        _ => { /* ... */ }
    }
}
```

## Example Outputs

### `mood-transition` from peaceful to triumphant
```
Plan a transition from 'peaceful' to 'triumphant' over 8 bars.

From: valence=0.3, arousal=0.2, agency=-0.2
To: valence=0.9, arousal=0.8, agency=0.9

Shift required:
- Valence: 0.60 (brightening)
- Arousal: 0.60 (intensifying)

Suggested strategy: Major emotional shift - use a bridge section

Techniques:
- Valence shift: Change mode/key, alter chord extensions
- Arousal increase: Add layers, increase velocity, shorten notes
- Arousal decrease: Thin texture, reduce velocity, sustain notes

When generating MIDI, set the emotion parameter to intermediate values
across the transition bars.
```

### `express-emotion` for anxious
```
Express this emotional state in sound:

Emotion: tense/anxious (valence=-0.3, arousal=0.8, agency=-0.3)

How it translates to MIDI:
Velocity: ~89 (punchy)
Duration: 0.6x normal (staccato)
Timing: behind beat (-6ms late)
Color: minor/dark tendency

Suggested musical elements:
- Key: Minor or Phrygian
- Tempo feel: driving, energetic
- Texture: dense, layered
- Rhythm: laid back, behind the beat

When using add_node, set:
emotion: { "valence": -0.3, "arousal": 0.8, "agency": -0.3 }
```

## Success Criteria

- [ ] Emotion-to-descriptor mapping is rich
- [ ] Realization description is accurate
- [ ] Arc analysis detects trends
- [ ] Coherence check finds issues
- [ ] Prompts guide emotional expression
- [ ] Tests verify all prompt responses
