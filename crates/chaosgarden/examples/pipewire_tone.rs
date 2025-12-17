//! Simple tone generator to test PipeWire output
//!
//! Run with: cargo run -p chaosgarden --example pipewire_tone

use std::f32::consts::PI;
use std::sync::atomic::Ordering;
use std::time::Duration;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    use chaosgarden::pipewire_output::{PipeWireOutputConfig, PipeWireOutputStream};

    println!("Starting PipeWire tone test...");
    println!("You should hear a 440Hz sine wave for 5 seconds.");

    let config = PipeWireOutputConfig {
        name: "chaosgarden-test".to_string(),
        sample_rate: 48000,
        channels: 2,
        latency_frames: 512, // ~10.7ms - try 256 for ~5ms, 128 for ~2.7ms
    };

    println!(
        "Configured latency: {} frames ({:.1}ms)",
        config.latency_frames,
        config.latency_ms()
    );

    // Create stream in paused state so we can pre-fill the buffer
    let mut stream = PipeWireOutputStream::new_paused(config)?;
    let ring = stream.ring_buffer();

    // Generate audio in a loop
    let sample_rate = 48000.0f32;
    let frequency = 440.0f32;
    let volume = 0.3f32;
    let mut phase = 0.0f32;
    let phase_inc = 2.0 * PI * frequency / sample_rate;

    // Pre-fill the buffer with ~500ms of audio BEFORE starting PipeWire
    // This ensures we have data ready when PipeWire first calls our callback
    println!("Pre-filling buffer...");
    let prefill_samples = (sample_rate * 0.5) as usize; // 0.5 seconds
    {
        let mut samples = Vec::with_capacity(prefill_samples * 2);
        for _ in 0..prefill_samples {
            let sample = (phase.sin() * volume) as f32;
            samples.push(sample); // Left
            samples.push(sample); // Right
            phase += phase_inc;
            if phase >= 2.0 * PI {
                phase -= 2.0 * PI;
            }
        }
        if let Ok(mut ring_guard) = ring.lock() {
            let written = ring_guard.write(&samples);
            println!("Pre-filled {} samples ({} available)", written, ring_guard.available());
        }
    }

    // NOW start the PipeWire thread - buffer is already filled
    println!("Starting PipeWire stream...");
    stream.start()?;

    let start = std::time::Instant::now();
    let duration = Duration::from_secs(5);

    println!("Generating audio...");

    while start.elapsed() < duration && stream.is_running() {
        // Check how much space is available and fill it
        let space_available = if let Ok(ring_guard) = ring.lock() {
            ring_guard.space()
        } else {
            0
        };

        if space_available < 1024 {
            // Buffer is nearly full, sleep a bit
            std::thread::sleep(Duration::from_millis(10));
            continue;
        }

        // Generate enough samples to fill available space (up to 48000 at a time)
        let frames_to_generate = (space_available / 2).min(48000);
        let mut samples = Vec::with_capacity(frames_to_generate * 2);

        for _ in 0..frames_to_generate {
            let sample = (phase.sin() * volume) as f32;
            samples.push(sample); // Left
            samples.push(sample); // Right
            phase += phase_inc;
            if phase >= 2.0 * PI {
                phase -= 2.0 * PI;
            }
        }

        // Write to ring buffer
        if let Ok(mut ring_guard) = ring.lock() {
            ring_guard.write(&samples);
        }
    }

    let stats = stream.stats();
    let callbacks = stats.callbacks.load(Ordering::Relaxed);
    let samples = stats.samples_written.load(Ordering::Relaxed);
    let underruns = stats.underruns.load(Ordering::Relaxed);
    let elapsed = start.elapsed().as_secs_f64();

    println!("Done! Stopping stream...");
    println!();
    println!("Stats:");
    println!("  Callbacks:       {}", callbacks);
    println!("  Samples written: {}", samples);
    println!("  Underruns:       {}", underruns);
    println!("  Duration:        {:.2}s", elapsed);
    println!("  Callbacks/sec:   {:.1}", callbacks as f64 / elapsed);
    println!("  Samples/sec:     {:.0}", samples as f64 / elapsed);

    Ok(())
}
