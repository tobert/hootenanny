//! Chaosgarden Demo CLI
//!
//! Demonstrates the chaosgarden modules end-to-end:
//! - Create a timeline with tracks, buses, and sections
//! - Build a graph from the timeline
//! - Register participants with capabilities
//! - Query the system via Trustfall
//! - Run the playback engine

use std::sync::{Arc, RwLock};

use anyhow::Result;
use chaosgarden::{
    Beat, Capability, CapabilityRegistry, CapabilityRequirement, CapabilityUri,
    ChaosgardenAdapter, CompiledGraph, Graph, Participant, ParticipantKind, PlaybackEngine,
    Region, TempoMap, Timeline,
};
use serde_json::json;
use trustfall::{execute_query, FieldValue};

#[tokio::main]
async fn main() -> Result<()> {
    println!("🌿 Chaosgarden Demo");
    println!("==================\n");

    // 1. Create a timeline with tracks and sections
    println!("📋 Creating timeline...");
    let mut timeline = Timeline::new("Demo Song", 120.0);

    // Add sections with hints for generation
    {
        let intro = timeline.add_section("Intro", Beat(0.0), Beat(16.0));
        intro.hints.mood = Some("mysterious".to_string());
        intro.hints.energy = Some(0.3);
    }
    {
        let verse = timeline.add_section("Verse", Beat(16.0), Beat(48.0));
        verse.hints.mood = Some("groovy".to_string());
        verse.hints.energy = Some(0.6);
    }
    {
        let chorus = timeline.add_section("Chorus", Beat(48.0), Beat(80.0));
        chorus.hints.mood = Some("euphoric".to_string());
        chorus.hints.energy = Some(0.9);
    }

    // Add a reverb bus
    let reverb_id = {
        let bus = timeline.add_bus("Reverb");
        bus.volume = 0.7;
        bus.id
    };

    // Add tracks with content
    {
        let drums = timeline.add_track("Drums");
        drums.add_audio(Beat(0.0), Beat(80.0), "drums_loop_hash");
        drums.add_send(reverb_id, 0.2);
    }
    {
        let bass = timeline.add_track("Bass");
        bass.add_midi(Beat(16.0), Beat(64.0), "bass_midi_hash");
    }
    {
        let lead = timeline.add_track("Lead");
        // Add a latent region - content to be generated
        let latent_id = lead.add_latent(
            Beat(48.0),
            Beat(32.0),
            "orpheus_generate",
            json!({"prompt": "euphoric lead melody", "temperature": 0.8}),
        );
        // Name the latent region
        if let Some(region) = lead.regions.iter_mut().find(|r| r.id == latent_id) {
            region.metadata.name = Some("Chorus Lead Solo".to_string());
        }
        lead.add_send(reverb_id, 0.4);
    }

    println!("   ✓ Created timeline: {}", timeline.name);
    println!("   ✓ {} sections", timeline.sections.len());
    println!("   ✓ {} tracks", timeline.tracks.len());
    println!("   ✓ {} buses", timeline.buses.len());
    println!(
        "   ✓ {} total regions",
        timeline.all_regions().count()
    );
    println!();

    // 2. Build a graph from the timeline
    println!("🔗 Building audio graph...");
    let graph = timeline.build_graph();
    println!("   ✓ {} nodes", graph.node_count());
    println!("   ✓ {} edges", graph.edge_count());
    println!();

    // 3. Register participants with capabilities
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
    orpheus.add_capability(Capability::new(
        CapabilityUri::new("model:orpheus"),
        "Orpheus Model",
    ));
    registry.register(orpheus).await;

    // Register human participant
    let mut human = Participant::new(ParticipantKind::Human, "alice")
        .with_tag("producer");
    human.add_capability(Capability::new(
        CapabilityUri::new("hitl:approve"),
        "Approve Content",
    ));
    human.add_capability(Capability::new(
        CapabilityUri::new("hitl:annotate"),
        "Add Annotations",
    ));
    registry.register(human).await;

    // Register a device
    let keyboard = Participant::new(ParticipantKind::Device, "midi-keyboard")
        .with_tag("input");
    registry.register(keyboard).await;

    let participants = registry.snapshot().await;
    println!("   ✓ {} participants registered", participants.len());

    // Find who can generate MIDI
    let generators = registry
        .find_satisfying(&[CapabilityRequirement::new(CapabilityUri::new("gen:midi"))])
        .await;
    let gen_names: Vec<_> = generators.iter().map(|p| p.name.as_str()).collect();
    println!(
        "   ✓ {} participant(s) can generate MIDI: {}",
        generators.len(),
        gen_names.join(", ")
    );

    // Find who can approve content
    let approvers = registry
        .find_satisfying(&[CapabilityRequirement::new(CapabilityUri::new(
            "hitl:approve",
        ))])
        .await;
    let approver_names: Vec<_> = approvers.iter().map(|p| p.name.as_str()).collect();
    println!(
        "   ✓ {} participant(s) can approve: {}",
        approvers.len(),
        approver_names.join(", ")
    );
    println!();

    // 4. Query via Trustfall
    println!("🔍 Querying with Trustfall...");

    // Set up regions for query
    let regions: Vec<Region> = timeline.all_regions().cloned().collect();

    // Build a simple graph for the adapter
    let query_graph = Graph::new();

    let adapter = ChaosgardenAdapter::new(
        Arc::new(RwLock::new(regions)),
        Arc::new(RwLock::new(query_graph)),
        Arc::new(TempoMap::new(120.0, Default::default())),
    )?;

    // Query latent regions
    let query = r#"
        query {
            LatentRegion {
                name @output
                position @output
                latent_status @output
                generation_tool @output
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

    println!("   ✓ Found {} latent region(s):", results.len());
    for result in &results {
        let name: Arc<str> = "name".into();
        let tool: Arc<str> = "generation_tool".into();
        println!(
            "     - {:?} using {:?}",
            result.get(&name),
            result.get(&tool)
        );
    }
    println!();

    // 5. Demonstrate playback engine
    println!("🎵 Demonstrating playback engine...");

    // Create a minimal graph for rendering
    let mut render_graph = Graph::new();
    let mut compiled = CompiledGraph::compile(&mut render_graph, 256)?;
    let tempo_map = Arc::new(TempoMap::new(120.0, Default::default()));
    let mut engine = PlaybackEngine::new(48000, 256, tempo_map);

    // Get position
    let pos = engine.position();
    println!("   ✓ Initial position: sample={}, beat={:.2}", pos.samples.0, pos.beats.0);
    println!("   ✓ Is playing: {}", engine.is_playing());

    // Start playback
    engine.play();
    println!("   ✓ After play(): is_playing={}", engine.is_playing());

    // Process a few buffers
    let empty_regions: Vec<Region> = vec![];
    for _ in 0..4 {
        let _ = engine.process(&mut compiled, &empty_regions);
    }
    let pos = engine.position();
    println!(
        "   ✓ After 4 buffers: sample={}, beat={:.2}",
        pos.samples.0, pos.beats.0
    );

    // Stop playback
    engine.stop();
    println!("   ✓ After stop(): is_playing={}", engine.is_playing());
    println!();

    // 6. Show section hints at a position
    println!("💡 Section hints:");
    for beat in [Beat(8.0), Beat(32.0), Beat(64.0)] {
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

    // 7. Summary
    println!("✅ Demo complete!");
    println!();
    println!("Summary of chaosgarden modules demonstrated:");
    println!("  • primitives   - Beat, Region, TempoMap");
    println!("  • patterns     - Timeline, Track, Bus, Section");
    println!("  • graph        - Audio routing DAG");
    println!("  • playback     - CompiledGraph, PlaybackEngine");
    println!("  • query        - Trustfall adapter");
    println!("  • capabilities - CapabilityRegistry, Participant");

    Ok(())
}
