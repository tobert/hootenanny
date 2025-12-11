//! Chaosgarden Demo CLI
//!
//! Full showcase of chaosgarden modules:
//! - Timeline with tracks, buses, sections, and rich hints
//! - Audio region playback via MemoryResolver
//! - Latent lifecycle simulation (job → progress → resolve → approve)
//! - Mix-in queue with crossfade scheduling
//! - Advanced Trustfall queries
//! - Capability registry
//! - Render to WAV file
//!
//! Run with `--verbose` or `-v` for detailed ASCII visualizations.

use std::io::Cursor;
use std::sync::{Arc, RwLock};

use anyhow::Result;
use chaosgarden::{
    Beat, Capability, CapabilityRegistry, CapabilityRequirement, CapabilityUri,
    ChaosgardenAdapter, CompiledGraph, ContentType, Graph, IOPubPublisher, LatentConfig,
    LatentEvent, LatentManager, MemoryResolver, MixInStrategy, Participant, ParticipantKind,
    PlaybackEngine, Region, TempoMap, Tick, Timeline,
};
use serde_json::json;
use trustfall::{execute_query, FieldValue};
use uuid::Uuid;

// ============================================================================
// VERBOSE MODE HELPERS
// ============================================================================

fn is_verbose() -> bool {
    std::env::args().any(|a| a == "--verbose" || a == "-v")
}

fn print_header(title: &str) {
    if is_verbose() {
        println!("\n{}", "═".repeat(70));
        println!("  {}", title);
        println!("{}\n", "═".repeat(70));
    } else {
        println!("{}", title);
    }
}

fn print_timeline_ascii(timeline: &Timeline) {
    if !is_verbose() {
        return;
    }

    let max_beat = 24.0;
    let width = 60;
    let beat_to_col = |b: f64| ((b / max_beat) * width as f64) as usize;

    println!("Timeline: {} ({} BPM)", timeline.name, timeline.tempo_map.tempo_at(Tick(0)));
    println!("{}", "─".repeat(70));

    // Beat ruler
    print!("Beat:     ");
    for beat in (0..=24).step_by(4) {
        print!("{:<8}", beat);
    }
    println!();

    print!("          ");
    for _ in 0..=6 {
        print!("│       ");
    }
    println!();

    // Sections
    for section in &timeline.sections {
        let start_col = beat_to_col(section.start.0);
        let end_col = beat_to_col(section.end.0).min(width);
        let len = end_col.saturating_sub(start_col);

        print!("[{:<7}] ", &section.name[..section.name.len().min(7)]);
        print!("{}", " ".repeat(start_col));
        print!("{}", "█".repeat(len));
        println!();
    }

    println!("          │       │       │       │       │       │       │");

    // Tracks with regions
    for track in &timeline.tracks {
        print!("{:<9} ", &track.name[..track.name.len().min(9)]);
        let mut line = vec![' '; width];

        for region in &track.regions {
            let start_col = beat_to_col(region.position.0);
            let end_col = beat_to_col(region.end().0).min(width);

            let char = if region.is_latent() {
                '▒'
            } else if region.is_playable() {
                '█'
            } else {
                '░'
            };

            for i in start_col..end_col {
                if i < line.len() {
                    line[i] = char;
                }
            }
        }

        println!("{}", line.iter().collect::<String>());
    }

    println!("          │       │       │       │       │       │       │");
    println!("Legend: █ Playing  ░ Pending  ▒ Latent");
    println!();
}

fn print_progress_bar(progress: f32, width: usize) -> String {
    let filled = (progress * width as f32) as usize;
    let empty = width - filled;
    format!("{}{}",
        "█".repeat(filled),
        "░".repeat(empty)
    )
}

fn print_latent_event(event: &LatentEvent) {
    if !is_verbose() {
        return;
    }

    match event {
        LatentEvent::JobStarted { region_id, job_id } => {
            println!("   ├─ Job started: {} (region {})", job_id, &region_id.to_string()[..8]);
        }
        LatentEvent::Progress { progress, .. } => {
            let bar = print_progress_bar(*progress, 20);
            println!("   ├─ Progress: {} {:.0}%", bar, progress * 100.0);
        }
        LatentEvent::Resolved { artifact_id, content_hash, .. } => {
            println!("   ├─ Resolved → artifact: {}, hash: {}", artifact_id, &content_hash[..8.min(content_hash.len())]);
        }
        LatentEvent::Approved { region_id } => {
            println!("   ├─ ✓ Approved (region {})", &region_id.to_string()[..8]);
        }
        LatentEvent::Rejected { reason, .. } => {
            println!("   ├─ ✗ Rejected: {:?}", reason);
        }
        LatentEvent::Failed { error, .. } => {
            println!("   ├─ ⚠ Failed: {}", error);
        }
        LatentEvent::MixedIn { at_beat, strategy, .. } => {
            let strat = match strategy {
                MixInStrategy::HardCut => "hard cut".to_string(),
                MixInStrategy::Crossfade { beats } => format!("crossfade {:.1} beats", beats),
            };
            println!("   └─ Mixed in @ beat {:.1} ({})", at_beat.0, strat);
        }
    }
}

// ============================================================================
// EVENT PUBLISHER
// ============================================================================

struct EventCollector {
    events: RwLock<Vec<LatentEvent>>,
}

impl EventCollector {
    fn new() -> Self {
        Self {
            events: RwLock::new(Vec::new()),
        }
    }

    fn events(&self) -> Vec<LatentEvent> {
        self.events.read().unwrap().clone()
    }
}

impl IOPubPublisher for EventCollector {
    fn publish(&self, event: LatentEvent) {
        print_latent_event(&event);
        self.events.write().unwrap().push(event);
    }
}

// ============================================================================
// AUDIO GENERATION
// ============================================================================

fn generate_sine_wav(frequency: f32, duration_secs: f32, sample_rate: u32, amplitude: f32) -> Vec<u8> {
    let num_frames = (sample_rate as f32 * duration_secs) as usize;

    let spec = hound::WavSpec {
        channels: 2,
        sample_rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };

    let mut cursor = Cursor::new(Vec::new());
    {
        let mut writer = hound::WavWriter::new(&mut cursor, spec).unwrap();
        for i in 0..num_frames {
            let t = i as f32 / sample_rate as f32;
            let sample = (2.0 * std::f32::consts::PI * frequency * t).sin() * amplitude;
            writer.write_sample(sample).unwrap();
            writer.write_sample(sample).unwrap();
        }
        writer.finalize().unwrap();
    }

    cursor.into_inner()
}

fn generate_chord_wav(frequencies: &[f32], duration_secs: f32, sample_rate: u32, amplitude: f32) -> Vec<u8> {
    let num_frames = (sample_rate as f32 * duration_secs) as usize;
    let per_freq_amp = amplitude / frequencies.len() as f32;

    let spec = hound::WavSpec {
        channels: 2,
        sample_rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };

    let mut cursor = Cursor::new(Vec::new());
    {
        let mut writer = hound::WavWriter::new(&mut cursor, spec).unwrap();
        for i in 0..num_frames {
            let t = i as f32 / sample_rate as f32;
            let sample: f32 = frequencies
                .iter()
                .map(|&f| (2.0 * std::f32::consts::PI * f * t).sin() * per_freq_amp)
                .sum();
            writer.write_sample(sample).unwrap();
            writer.write_sample(sample).unwrap();
        }
        writer.finalize().unwrap();
    }

    cursor.into_inner()
}

// ============================================================================
// MAIN
// ============================================================================

#[tokio::main]
async fn main() -> Result<()> {
    let verbose = is_verbose();

    println!("🌿 Chaosgarden Demo {}", if verbose { "(verbose)" } else { "" });
    println!("==================\n");

    let sample_rate = 48000;

    // ========================================================================
    // PART 1: Generate test audio
    // ========================================================================
    print_header("🎹 Generating test audio...");

    let mut resolver = MemoryResolver::new();

    let bass_wav = generate_sine_wav(82.0, 4.0, sample_rate, 0.4);
    resolver.insert("bass_drone", bass_wav);

    let lead_wav = generate_sine_wav(440.0, 4.0, sample_rate, 0.3);
    resolver.insert("lead_melody", lead_wav);

    let pad_wav = generate_chord_wav(&[261.63, 329.63, 392.00], 4.0, sample_rate, 0.25);
    resolver.insert("pad_chord", pad_wav);

    // Generated content (will be "produced" by latent simulation)
    let synth_wav = generate_chord_wav(&[523.25, 659.25, 783.99], 4.0, sample_rate, 0.3);
    resolver.insert("generated_synth", synth_wav);

    if verbose {
        println!("   ✓ bass_drone: 82Hz sine, 4.0s");
        println!("   ✓ lead_melody: 440Hz sine, 4.0s");
        println!("   ✓ pad_chord: C major chord, 4.0s");
        println!("   ✓ generated_synth: C5 major chord (for latent), 4.0s");
    } else {
        println!("   ✓ 4 audio clips generated");
    }

    let resolver = Arc::new(resolver);

    // ========================================================================
    // PART 2: Create timeline with rich hints
    // ========================================================================
    print_header("📋 Creating timeline...");

    let mut timeline = Timeline::new("Demo Song", 120.0);

    // Sections with rich hints
    {
        let intro = timeline.add_section("Intro", Beat(0.0), Beat(8.0));
        intro.hints.mood = Some("mysterious".to_string());
        intro.hints.energy = Some(0.3);
        intro.hints.density = Some(0.4);
        intro.hints.style_hints.push("ambient".to_string());
        intro.hints.style_hints.push("sparse".to_string());
    }
    {
        let verse = timeline.add_section("Verse", Beat(8.0), Beat(16.0));
        verse.hints.mood = Some("groovy".to_string());
        verse.hints.energy = Some(0.6);
        verse.hints.density = Some(0.7);
        verse.hints.style_hints.push("rhythmic".to_string());
    }
    {
        let chorus = timeline.add_section("Chorus", Beat(16.0), Beat(24.0));
        chorus.hints.mood = Some("euphoric".to_string());
        chorus.hints.energy = Some(0.9);
        chorus.hints.density = Some(0.85);
        chorus.hints.style_hints.push("energetic".to_string());
        chorus.hints.style_hints.push("layered".to_string());
    }

    // Reverb bus
    let reverb_id = {
        let bus = timeline.add_bus("Reverb");
        bus.volume = 0.7;
        bus.id
    };

    // Tracks
    let latent_region_id: Uuid;
    {
        let bass = timeline.add_track("Bass");
        bass.add_audio(Beat(0.0), Beat(16.0), "bass_drone");
        bass.volume = 0.8;
    }
    {
        let pad = timeline.add_track("Pad");
        pad.add_audio(Beat(0.0), Beat(16.0), "pad_chord");
        pad.add_send(reverb_id, 0.3);
        pad.volume = 0.6;
    }
    {
        let lead = timeline.add_track("Lead");
        lead.add_audio(Beat(4.0), Beat(12.0), "lead_melody");
        lead.add_send(reverb_id, 0.4);
        lead.volume = 0.7;
    }
    {
        let synth = timeline.add_track("Synth");
        latent_region_id = synth.add_latent(
            Beat(8.0),
            Beat(8.0),
            "test_generator",
            json!({"prompt": "euphoric synth pad", "temperature": 0.8}),
        );
        if let Some(region) = synth.regions.iter_mut().find(|r| r.id == latent_region_id) {
            region.metadata.name = Some("Generated Synth".to_string());
            region.metadata.tags.push("ai-generated".to_string());
        }
        synth.add_send(reverb_id, 0.5);
        synth.volume = 0.65;
    }

    let region_count = timeline.all_regions().count();

    if verbose {
        print_timeline_ascii(&timeline);
    }
    println!("   ✓ Timeline: {} ({} BPM)", timeline.name, timeline.tempo_map.tempo_at(Tick(0)));
    println!("   ✓ {} sections, {} tracks, {} buses, {} regions",
        timeline.sections.len(),
        timeline.tracks.len(),
        timeline.buses.len(),
        region_count
    );

    // ========================================================================
    // PART 3: Build audio graph
    // ========================================================================
    print_header("🔗 Building audio graph...");

    let graph = timeline.build_graph();

    if verbose {
        println!("Audio Graph ({} nodes, {} edges):", graph.node_count(), graph.edge_count());
        println!("┌──────────┐   ┌──────────┐   ┌──────────┐");
        println!("│  Bass    │──▶│          │   │  Reverb  │");
        println!("└──────────┘   │          │   │   Bus    │");
        println!("┌──────────┐   │  Master  │◀──┤          │");
        println!("│  Pad     │──▶│          │   └──────────┘");
        println!("└──────────┘   │          │        ▲");
        println!("┌──────────┐   └──────────┘        │ sends");
        println!("│  Lead    │───────────────────────┤");
        println!("└──────────┘                       │");
        println!("┌──────────┐                       │");
        println!("│  Synth   │───────────────────────┘");
        println!("└──────────┘");
    }
    println!("   ✓ {} nodes, {} edges", graph.node_count(), graph.edge_count());

    // ========================================================================
    // PART 4: Latent lifecycle simulation
    // ========================================================================
    print_header("🔄 Simulating latent lifecycle...");

    let event_collector = Arc::new(EventCollector::new());

    let mut config = LatentConfig::default();
    config.auto_approve_tools.insert("test_generator".to_string());
    config.default_mix_in = MixInStrategy::Crossfade { beats: 2.0 };

    let mut manager = LatentManager::new(config, event_collector.clone());
    let mut regions: Vec<Region> = timeline.all_regions().cloned().collect();

    if verbose {
        println!("🔄 Latent Region: \"Generated Synth\"");
    }

    // Simulate job lifecycle
    manager.handle_job_started(latent_region_id, "job_001".to_string(), &mut regions);

    for progress in [0.25, 0.50, 0.75, 1.0] {
        manager.handle_progress(latent_region_id, progress, &mut regions);
    }

    manager.handle_resolved(
        latent_region_id,
        "artifact_xyz".to_string(),
        "generated_synth".to_string(), // This matches our resolver!
        ContentType::Audio,
        &mut regions,
    );

    // Schedule mix-in
    let schedule = manager.schedule_mix_in(
        latent_region_id,
        Beat(8.0),
        Some(MixInStrategy::Crossfade { beats: 2.0 }),
    )?;

    let events = event_collector.events();
    if !verbose {
        println!("   ✓ job_001: started → 25% → 50% → 75% → 100% → resolved → auto-approved");
    }
    println!("   ✓ {} events emitted", events.len());
    println!("   ✓ Mix-in scheduled @ beat {:.1} ({:?})", schedule.target_beat.0, schedule.strategy);

    // ========================================================================
    // PART 5: Capability registry
    // ========================================================================
    print_header("👥 Registering participants...");

    let registry = CapabilityRegistry::new();

    let mut orpheus = Participant::new(ParticipantKind::Model, "orpheus");
    orpheus.add_capability(Capability::new(CapabilityUri::new("gen:midi"), "Generate MIDI"));
    orpheus.add_capability(Capability::new(CapabilityUri::new("gen:audio"), "Generate Audio"));
    registry.register(orpheus).await;

    let mut human = Participant::new(ParticipantKind::Human, "alice").with_tag("producer");
    human.add_capability(Capability::new(CapabilityUri::new("hitl:approve"), "Approve Content"));
    human.add_capability(Capability::new(CapabilityUri::new("hitl:annotate"), "Add Annotations"));
    registry.register(human).await;

    let keyboard = Participant::new(ParticipantKind::Device, "midi-keyboard").with_tag("input");
    registry.register(keyboard).await;

    let participants = registry.snapshot().await;
    let generators = registry
        .find_satisfying(&[CapabilityRequirement::new(CapabilityUri::new("gen:"))])
        .await;

    println!("   ✓ {} participants: {:?}",
        participants.len(),
        participants.iter().map(|p| p.name.as_str()).collect::<Vec<_>>()
    );
    println!("   ✓ {} can generate content", generators.len());

    // ========================================================================
    // PART 6: Advanced Trustfall queries
    // ========================================================================
    print_header("🔍 Querying with Trustfall...");

    let query_graph = Graph::new();
    let adapter = ChaosgardenAdapter::new(
        Arc::new(RwLock::new(regions.clone())),
        Arc::new(RwLock::new(query_graph)),
        Arc::new(TempoMap::new(120.0, Default::default())),
    )?;
    let adapter = Arc::new(adapter);

    type Variables = std::collections::BTreeMap<Arc<str>, FieldValue>;

    // Query 1: All regions with behavior type
    let query1 = r#"
        query {
            Region {
                id @output
                position @output
                duration @output
                behavior_type @output
                is_playable @output
            }
        }
    "#;

    let results1: Vec<_> = execute_query(
        adapter.schema(),
        adapter.clone(),
        query1,
        Variables::new(),
    )?.collect();

    let playable_count = results1.iter().filter(|r| {
        r.get(&Arc::from("is_playable")) == Some(&FieldValue::Boolean(true))
    }).count();

    if verbose {
        println!("Query: All regions");
        for result in &results1 {
            let pos = result.get(&Arc::from("position")).and_then(|v| match v {
                FieldValue::Float64(f) => Some(*f),
                _ => None,
            }).unwrap_or(0.0);
            let behavior = result.get(&Arc::from("behavior_type")).and_then(|v| match v {
                FieldValue::String(s) => Some(s.as_ref()),
                _ => None,
            }).unwrap_or("?");
            let playable = result.get(&Arc::from("is_playable")) == Some(&FieldValue::Boolean(true));
            println!("   - Beat {:.1}: {} {}", pos, behavior, if playable { "✓" } else { "○" });
        }
        println!();
    }
    println!("   ✓ {} regions ({} playable, {} latent)",
        results1.len(), playable_count, results1.len() - playable_count);

    // Query 2: Latent regions
    let query2 = r#"
        query {
            LatentRegion {
                name @output
                position @output
                latent_status @output
                generation_tool @output
            }
        }
    "#;

    let results2: Vec<_> = execute_query(
        adapter.schema(),
        adapter.clone(),
        query2,
        Variables::new(),
    )?.collect();

    if verbose && !results2.is_empty() {
        println!("Query: Latent regions");
        for result in &results2 {
            let name = result.get(&Arc::from("name")).and_then(|v| match v {
                FieldValue::String(s) => Some(s.as_ref()),
                _ => None,
            }).unwrap_or("unnamed");
            let status = result.get(&Arc::from("latent_status")).and_then(|v| match v {
                FieldValue::String(s) => Some(s.as_ref()),
                _ => None,
            }).unwrap_or("?");
            let tool = result.get(&Arc::from("generation_tool")).and_then(|v| match v {
                FieldValue::String(s) => Some(s.as_ref()),
                _ => None,
            }).unwrap_or("?");
            println!("   - \"{}\": {} via {}", name, status, tool);
        }
        println!();
    }
    println!("   ✓ {} latent region(s)", results2.len());

    // Query 3: Time conversion
    let query3 = r#"
        query {
            BeatToSecond(beat: 8.0) {
                value @output
            }
        }
    "#;

    let results3: Vec<_> = execute_query(
        adapter.schema(),
        adapter.clone(),
        query3,
        Variables::new(),
    )?.collect();

    if let Some(result) = results3.first() {
        if let Some(FieldValue::Float64(sec)) = result.get(&Arc::from("value")) {
            println!("   ✓ Beat 8.0 = {:.2}s (tempo conversion)", sec);
        }
    }

    // ========================================================================
    // PART 7: Playback with audio
    // ========================================================================
    print_header("🎵 Running playback engine...");

    let tempo_map = Arc::new(TempoMap::new(120.0, Default::default()));
    let mut engine = PlaybackEngine::with_resolver(
        sample_rate,
        256,
        tempo_map.clone(),
        resolver.clone(),
    );

    let mut render_graph = Graph::new();
    let mut compiled = CompiledGraph::compile(&mut render_graph, 256)?;

    // Get playable regions (now includes the resolved latent!)
    let playable_regions: Vec<Region> = regions
        .iter()
        .filter(|r| r.is_playable())
        .cloned()
        .collect();

    println!("   ✓ {} playable regions", playable_regions.len());

    // Queue the mix-in
    for mix_in in manager.pending_mix_ins() {
        engine.queue_mix_in(mix_in.clone());
    }
    println!("   ✓ {} pending mix-in(s) queued", manager.pending_mix_ins().len());

    // Start playback
    engine.play();

    // ========================================================================
    // PART 8: Render to WAV
    // ========================================================================
    print_header("💾 Rendering to WAV file...");

    let output_path = "/tmp/chaosgarden_demo.wav";
    let duration_beats = Beat(16.0);

    let spec = hound::WavSpec {
        channels: 2,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut writer = hound::WavWriter::create(output_path, spec)?;
    let mut samples_written = 0;
    let mut last_beat = 0.0;

    while engine.position().beats.0 < duration_beats.0 {
        let output = engine.process(&mut compiled, &playable_regions)?;

        for &sample in &output.samples {
            let int_sample = (sample * 32767.0).clamp(-32768.0, 32767.0) as i16;
            writer.write_sample(int_sample)?;
            samples_written += 1;
        }

        // Verbose progress
        if verbose {
            let current_beat = engine.position().beats.0;
            if (current_beat - last_beat) >= 4.0 {
                let bar = print_progress_bar(current_beat as f32 / duration_beats.0 as f32, 30);
                println!("   ▶ {} {:.1}/{:.1} beats", bar, current_beat, duration_beats.0);
                last_beat = current_beat;
            }
        }
    }

    writer.finalize()?;

    let duration_secs = samples_written as f64 / (sample_rate as f64 * 2.0);
    println!("   ✓ Rendered {} samples ({:.2}s) to {}", samples_written, duration_secs, output_path);

    // ========================================================================
    // PART 9: Section hints summary
    // ========================================================================
    if verbose {
        print_header("💡 Section hints for generation...");

        for section in &timeline.sections {
            println!("   {} (beat {:.0}-{:.0}):", section.name, section.start.0, section.end.0);
            println!("      mood: {:?}, energy: {:?}, density: {:?}",
                section.hints.mood,
                section.hints.energy,
                section.hints.density
            );
            if !section.hints.style_hints.is_empty() {
                println!("      styles: {:?}", section.hints.style_hints);
            }
        }
    }

    // ========================================================================
    // SUMMARY
    // ========================================================================
    println!();
    println!("✅ Demo complete!");
    println!();
    if verbose {
        println!("Modules demonstrated:");
        println!("  • primitives   - Beat, Region, TempoMap, ContentType");
        println!("  • patterns     - Timeline, Track, Bus, Section, SectionHints");
        println!("  • graph        - Audio routing DAG");
        println!("  • playback     - PlaybackEngine with region→audio wiring");
        println!("  • nodes        - AudioFileNode, MemoryResolver");
        println!("  • latent       - LatentManager, lifecycle events, mix-in scheduling");
        println!("  • query        - Trustfall adapter with advanced queries");
        println!("  • capabilities - CapabilityRegistry, Participant");
        println!();
    }
    println!("🎧 Listen to the output: {}", output_path);
    println!("   Run with --verbose for detailed output");

    Ok(())
}
