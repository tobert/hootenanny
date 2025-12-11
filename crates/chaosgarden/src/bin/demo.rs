//! Chaosgarden Demo CLI
//!
//! Demonstrates the chaosgarden modules end-to-end:
//! - Create a timeline with tracks, buses, and sections
//! - Build a graph from the timeline
//! - Register participants with capabilities
//! - Query the system via Trustfall
//! - Run the playback engine with actual audio
//! - Render output to a WAV file

use std::io::Cursor;
use std::sync::{Arc, RwLock};

use anyhow::Result;
use chaosgarden::{
    Beat, Capability, CapabilityRegistry, CapabilityRequirement, CapabilityUri,
    ChaosgardenAdapter, CompiledGraph, Graph, MemoryResolver, Participant, ParticipantKind,
    PlaybackEngine, Region, TempoMap, Timeline,
};
use serde_json::json;
use trustfall::{execute_query, FieldValue};

/// Generate a sine wave WAV file in memory
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
            writer.write_sample(sample).unwrap(); // L
            writer.write_sample(sample).unwrap(); // R
        }
        writer.finalize().unwrap();
    }

    cursor.into_inner()
}

/// Generate a chord (multiple sine waves mixed)
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
            writer.write_sample(sample).unwrap(); // L
            writer.write_sample(sample).unwrap(); // R
        }
        writer.finalize().unwrap();
    }

    cursor.into_inner()
}

/// Generate a simple kick drum sound (decaying sine)
fn generate_kick_wav(duration_secs: f32, sample_rate: u32) -> Vec<u8> {
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
            // Pitch drops from 150Hz to 50Hz over 0.1s
            let freq = 150.0 - (100.0 * (t * 10.0).min(1.0));
            // Amplitude decays exponentially
            let amp = (-t * 8.0).exp() * 0.8;
            let sample = (2.0 * std::f32::consts::PI * freq * t).sin() * amp;
            writer.write_sample(sample).unwrap();
            writer.write_sample(sample).unwrap();
        }
        writer.finalize().unwrap();
    }

    cursor.into_inner()
}

#[tokio::main]
async fn main() -> Result<()> {
    println!("🌿 Chaosgarden Demo");
    println!("==================\n");

    // === Part 1: Generate test audio ===
    println!("🎹 Generating test audio...");
    let sample_rate = 48000;

    // Create memory resolver with test audio
    let mut resolver = MemoryResolver::new();

    // Bass drone (low E, ~82Hz) - 4 seconds
    let bass_wav = generate_sine_wav(82.0, 4.0, sample_rate, 0.4);
    resolver.insert("bass_drone", bass_wav);
    println!("   ✓ bass_drone: 82Hz sine, 4.0s");

    // Lead melody (A4 = 440Hz) - 2 seconds
    let lead_wav = generate_sine_wav(440.0, 2.0, sample_rate, 0.3);
    resolver.insert("lead_melody", lead_wav);
    println!("   ✓ lead_melody: 440Hz sine, 2.0s");

    // Pad chord (C major: C4, E4, G4) - 4 seconds
    let pad_wav = generate_chord_wav(&[261.63, 329.63, 392.00], 4.0, sample_rate, 0.25);
    resolver.insert("pad_chord", pad_wav);
    println!("   ✓ pad_chord: C major chord, 4.0s");

    // Kick drum - 0.3 seconds
    let kick_wav = generate_kick_wav(0.3, sample_rate);
    resolver.insert("kick_drum", kick_wav);
    println!("   ✓ kick_drum: synthetic kick, 0.3s");

    let resolver = Arc::new(resolver);
    println!();

    // === Part 2: Create a timeline with tracks and sections ===
    println!("📋 Creating timeline...");
    let mut timeline = Timeline::new("Demo Song", 120.0);

    // Add sections with hints for generation
    {
        let intro = timeline.add_section("Intro", Beat(0.0), Beat(8.0));
        intro.hints.mood = Some("mysterious".to_string());
        intro.hints.energy = Some(0.3);
    }
    {
        let verse = timeline.add_section("Verse", Beat(8.0), Beat(16.0));
        verse.hints.mood = Some("groovy".to_string());
        verse.hints.energy = Some(0.6);
    }

    // Add a reverb bus
    let reverb_id = {
        let bus = timeline.add_bus("Reverb");
        bus.volume = 0.7;
        bus.id
    };

    // Add tracks with actual audio content
    {
        let bass = timeline.add_track("Bass");
        bass.add_audio(Beat(0.0), Beat(8.0), "bass_drone");
        bass.volume = 0.8;
    }
    {
        let pad = timeline.add_track("Pad");
        pad.add_audio(Beat(0.0), Beat(8.0), "pad_chord");
        pad.add_send(reverb_id, 0.3);
        pad.volume = 0.6;
    }
    {
        let lead = timeline.add_track("Lead");
        // Lead comes in at beat 4
        lead.add_audio(Beat(4.0), Beat(4.0), "lead_melody");
        lead.add_send(reverb_id, 0.4);
        lead.volume = 0.7;
    }
    {
        let drums = timeline.add_track("Drums");
        // Add a latent region for future generation
        let latent_id = drums.add_latent(
            Beat(8.0),
            Beat(8.0),
            "orpheus_generate",
            json!({"prompt": "funky drum pattern", "temperature": 0.7}),
        );
        if let Some(region) = drums.regions.iter_mut().find(|r| r.id == latent_id) {
            region.metadata.name = Some("Generated Drums".to_string());
        }
    }

    println!("   ✓ Created timeline: {}", timeline.name);
    println!("   ✓ Tempo: {} BPM", timeline.tempo_map.tempo_at(chaosgarden::Tick(0)));
    println!("   ✓ {} sections", timeline.sections.len());
    println!("   ✓ {} tracks", timeline.tracks.len());
    println!("   ✓ {} buses", timeline.buses.len());
    println!(
        "   ✓ {} total regions",
        timeline.all_regions().count()
    );
    println!();

    // === Part 3: Build a graph from the timeline ===
    println!("🔗 Building audio graph...");
    let graph = timeline.build_graph();
    println!("   ✓ {} nodes", graph.node_count());
    println!("   ✓ {} edges", graph.edge_count());
    println!();

    // === Part 4: Register participants with capabilities ===
    println!("👥 Registering participants...");
    let registry = CapabilityRegistry::new();

    // Register Orpheus model
    let mut orpheus = Participant::new(ParticipantKind::Model, "orpheus");
    orpheus.add_capability(
        Capability::new(CapabilityUri::new("gen:midi"), "Generate MIDI")
            .with_description("Generate MIDI sequences using Orpheus model"),
    );
    orpheus.add_capability(Capability::new(
        CapabilityUri::new("gen:continuation"),
        "Continue MIDI",
    ));
    registry.register(orpheus).await;

    // Register human participant
    let mut human = Participant::new(ParticipantKind::Human, "alice")
        .with_tag("producer");
    human.add_capability(Capability::new(
        CapabilityUri::new("hitl:approve"),
        "Approve Content",
    ));
    registry.register(human).await;

    let participants = registry.snapshot().await;
    println!("   ✓ {} participants registered", participants.len());

    // Find who can generate MIDI
    let generators = registry
        .find_satisfying(&[CapabilityRequirement::new(CapabilityUri::new("gen:midi"))])
        .await;
    println!(
        "   ✓ {} participant(s) can generate MIDI",
        generators.len()
    );
    println!();

    // === Part 5: Query via Trustfall ===
    println!("🔍 Querying with Trustfall...");

    let regions: Vec<Region> = timeline.all_regions().cloned().collect();
    let query_graph = Graph::new();

    let adapter = ChaosgardenAdapter::new(
        Arc::new(RwLock::new(regions.clone())),
        Arc::new(RwLock::new(query_graph)),
        Arc::new(TempoMap::new(120.0, Default::default())),
    )?;

    // Query all regions
    let query = r#"
        query {
            Region {
                id @output
                position @output
                duration @output
                is_playable @output
            }
        }
    "#;

    type Variables = std::collections::BTreeMap<Arc<str>, FieldValue>;
    let adapter = Arc::new(adapter);
    let results: Vec<_> = execute_query(
        adapter.schema(),
        adapter.clone(),
        query,
        Variables::new(),
    )?
    .collect();

    println!("   ✓ Found {} region(s)", results.len());

    // Count playable vs latent
    let playable = results.iter().filter(|r| {
        let key: Arc<str> = "is_playable".into();
        r.get(&key) == Some(&FieldValue::Boolean(true))
    }).count();
    println!("   ✓ {} playable, {} pending generation", playable, results.len() - playable);
    println!();

    // === Part 6: Playback with actual audio ===
    println!("🎵 Running playback engine with audio...");

    let tempo_map = Arc::new(TempoMap::new(120.0, Default::default()));
    let mut engine = PlaybackEngine::with_resolver(
        sample_rate,
        256, // buffer size
        tempo_map.clone(),
        resolver.clone(),
    );

    // Create a minimal compiled graph (empty - we're using regions)
    let mut render_graph = Graph::new();
    let mut compiled = CompiledGraph::compile(&mut render_graph, 256)?;

    // Get playable regions (exclude latent)
    let playable_regions: Vec<Region> = regions
        .iter()
        .filter(|r| r.is_playable())
        .cloned()
        .collect();

    println!("   ✓ {} playable audio regions", playable_regions.len());
    for region in &playable_regions {
        println!("     - Beat {:.1}-{:.1}: {:?}",
            region.position.0,
            region.end().0,
            region.metadata.name.as_deref().unwrap_or("unnamed")
        );
    }

    // Start playback
    engine.play();
    println!("   ✓ Playback started");

    // Process a few buffers and show position
    let initial_pos = engine.position();
    for _ in 0..10 {
        let _ = engine.process(&mut compiled, &playable_regions);
    }
    let pos = engine.position();
    println!(
        "   ✓ Position advanced: sample {} → {}, beat {:.3} → {:.3}",
        initial_pos.samples.0, pos.samples.0,
        initial_pos.beats.0, pos.beats.0
    );
    println!();

    // === Part 7: Render to WAV file ===
    println!("💾 Rendering to WAV file...");

    // Reset engine for rendering
    engine.stop();
    engine.play();

    let output_path = "/tmp/chaosgarden_demo.wav";
    let duration_beats = Beat(8.0); // Render 8 beats (4 seconds at 120 BPM)

    let spec = hound::WavSpec {
        channels: 2,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut writer = hound::WavWriter::create(output_path, spec)?;
    let mut samples_written = 0;

    while engine.position().beats.0 < duration_beats.0 {
        let output = engine.process(&mut compiled, &playable_regions)?;

        for &sample in &output.samples {
            let int_sample = (sample * 32767.0).clamp(-32768.0, 32767.0) as i16;
            writer.write_sample(int_sample)?;
            samples_written += 1;
        }
    }

    writer.finalize()?;

    let duration_secs = samples_written as f64 / (sample_rate as f64 * 2.0); // stereo
    println!("   ✓ Rendered {} samples ({:.2}s) to {}", samples_written, duration_secs, output_path);
    println!();

    // === Part 8: Section hints ===
    println!("💡 Section hints:");
    for beat in [Beat(2.0), Beat(10.0)] {
        if let Some(section) = timeline.section_at(beat) {
            println!(
                "   Beat {:.0}: {} (mood={:?}, energy={:?})",
                beat.0,
                section.name,
                section.hints.mood,
                section.hints.energy
            );
        }
    }
    println!();

    // === Summary ===
    println!("✅ Demo complete!");
    println!();
    println!("Modules demonstrated:");
    println!("  • primitives   - Beat, Region, TempoMap");
    println!("  • patterns     - Timeline, Track, Bus, Section");
    println!("  • graph        - Audio routing DAG");
    println!("  • playback     - PlaybackEngine with region→audio wiring");
    println!("  • nodes        - AudioFileNode, MemoryResolver");
    println!("  • query        - Trustfall adapter");
    println!("  • capabilities - CapabilityRegistry, Participant");
    println!();
    println!("🎧 Listen to the output: {}", output_path);

    Ok(())
}
