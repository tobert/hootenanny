//! PipeWire audio output stream
//!
//! Provides real audio output through PipeWire when the `pipewire` feature is enabled.
//! The stream reads from a shared ring buffer that the PlaybackEngine writes to.
//!
//! Architecture:
//! ```text
//! PlaybackEngine (your RT thread)     PipeWire (PW's RT thread)
//!        │                                    │
//!        ▼                                    ▼
//!   ring.write(samples)              process callback
//!        │                                    │
//!        └──────────► RingBuffer ────────────►│
//!                    (lock-free SPSC)         │
//!                                             ▼
//!                                      dequeue_buffer()
//!                                      copy to PW buffer
//!                                      queue_buffer()
//! ```

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use portable_atomic::AtomicF32;
use tracing::{debug, error, info};

use crate::external_io::{AudioRingConsumer, AudioRingProducer, RingBuffer};

/// Monitor input state for RT mixing (lock-free version)
///
/// When provided to the output stream, the output callback will read from the
/// monitor input ring buffer and mix it directly in the RT thread, bypassing
/// the tick() loop entirely.
///
/// NOTE: This struct is NOT Clone because AudioRingConsumer uses SPSC semantics
/// (only one consumer can exist). The consumer is moved into the output stream.
pub struct MonitorMixState {
    /// Lock-free ring buffer consumer (from monitor input)
    pub consumer: AudioRingConsumer,
    /// Enable flag (RT-safe)
    pub enabled: Arc<AtomicBool>,
    /// Gain 0.0-1.0 (RT-safe)
    pub gain: Arc<AtomicF32>,
}

/// Configuration for PipeWire output stream
#[derive(Debug, Clone)]
pub struct PipeWireOutputConfig {
    pub name: String,
    pub sample_rate: u32,
    pub channels: u32,
    /// Maximum frames to write per callback. Actual frames written is
    /// `min(latency_frames, buffer.requested(), buffer.max_frames())`.
    /// Common values: 64 (1.3ms), 128 (2.7ms), 256 (5.3ms), 512 (10.7ms), 1024 (21.3ms) @ 48kHz
    /// PipeWire typically requests 128-256 frames per callback regardless of this setting.
    pub latency_frames: u32,
}

impl Default for PipeWireOutputConfig {
    fn default() -> Self {
        Self {
            name: "chaosgarden".to_string(),
            sample_rate: 48000,
            channels: 2,
            latency_frames: 256, // ~5.3ms at 48kHz - PipeWire typically requests 128-256
        }
    }
}

impl PipeWireOutputConfig {
    /// Calculate latency in milliseconds
    pub fn latency_ms(&self) -> f64 {
        self.latency_frames as f64 / self.sample_rate as f64 * 1000.0
    }
}

/// Handle to a running PipeWire output stream
///
/// The stream runs in its own thread with PipeWire's main loop.
/// Audio data is fed via the ring buffer.
///
/// Use `new_paused()` to create a stream that doesn't start immediately,
/// allowing you to pre-fill the buffer before calling `start()`.
pub struct PipeWireOutputStream {
    ring_buffer: Arc<Mutex<RingBuffer>>,
    running: Arc<AtomicBool>,
    thread_handle: Option<JoinHandle<()>>,
    config: PipeWireOutputConfig,
    started: bool,
    // Stats for monitoring (updated by RT callback)
    stats: Arc<StreamStats>,
    // Track whether monitor was attached (for logging)
    #[allow(dead_code)]
    has_monitor: bool,
}

/// Runtime statistics from the PipeWire callback
#[derive(Debug, Default)]
pub struct StreamStats {
    pub callbacks: std::sync::atomic::AtomicU64,
    pub samples_written: std::sync::atomic::AtomicU64,
    pub underruns: std::sync::atomic::AtomicU64,
    // Debug counters for RT mixer
    pub monitor_reads: std::sync::atomic::AtomicU64,
    pub monitor_samples: std::sync::atomic::AtomicU64,
    // Warmup flag - don't count underruns until first successful read
    pub warmed_up: std::sync::atomic::AtomicBool,
    // RAVE streaming counters
    pub rave_writes: std::sync::atomic::AtomicU64,
    pub rave_samples_written: std::sync::atomic::AtomicU64,
    pub rave_reads: std::sync::atomic::AtomicU64,
    pub rave_samples_read: std::sync::atomic::AtomicU64,
}

impl PipeWireOutputStream {
    /// Create and start a new PipeWire output stream immediately
    ///
    /// For better control, use `new_paused()` + `start()` to pre-fill the buffer first.
    pub fn new(config: PipeWireOutputConfig) -> Result<Self, PipeWireOutputError> {
        let mut stream = Self::new_paused(config)?;
        stream.start()?;
        Ok(stream)
    }

    /// Create and start a new PipeWire output stream with streaming tap only
    ///
    /// The streaming tap receives the final mixed output for WebSocket streaming.
    pub fn new_with_streaming_tap(
        config: PipeWireOutputConfig,
        streaming_tap: Option<AudioRingProducer>,
        timeline_consumer: Option<AudioRingConsumer>,
    ) -> Result<Self, PipeWireOutputError> {
        Self::new_internal(config, None, streaming_tap, timeline_consumer, None, None)
    }

    /// Create and start a new PipeWire output stream with monitor mixing
    ///
    /// The monitor input is mixed directly in the RT callback, bypassing tick().
    /// This eliminates underruns/overruns caused by timing mismatches.
    /// The streaming tap receives the final mixed output for WebSocket streaming.
    ///
    /// Note: This constructor starts immediately. Monitor state is moved directly
    /// to the RT thread (never stored in the struct) to avoid Send/Sync issues.
    pub fn new_with_monitor(
        config: PipeWireOutputConfig,
        monitor: MonitorMixState,
        streaming_tap: Option<AudioRingProducer>,
        timeline_consumer: Option<AudioRingConsumer>,
        rave_input: Option<Arc<Mutex<Option<AudioRingProducer>>>>,
        rave_output: Option<Arc<Mutex<Option<AudioRingConsumer>>>>,
    ) -> Result<Self, PipeWireOutputError> {
        Self::new_internal(config, Some(monitor), streaming_tap, timeline_consumer, rave_input, rave_output)
    }

    /// Internal constructor that handles all variants
    fn new_internal(
        config: PipeWireOutputConfig,
        monitor: Option<MonitorMixState>,
        streaming_tap: Option<AudioRingProducer>,
        timeline_consumer: Option<AudioRingConsumer>,
        rave_input: Option<Arc<Mutex<Option<AudioRingProducer>>>>,
        rave_output: Option<Arc<Mutex<Option<AudioRingConsumer>>>>,
    ) -> Result<Self, PipeWireOutputError> {
        use pipewire as pw;

        // Initialize PipeWire
        pw::init();

        // Create ring buffer sized for ~1 second of audio
        let ring_capacity = config.sample_rate as usize * config.channels as usize * 2;
        let ring_buffer = Arc::new(Mutex::new(RingBuffer::new(ring_capacity)));

        let running = Arc::new(AtomicBool::new(true));
        let stats = Arc::new(StreamStats::default());
        let has_monitor = monitor.is_some();

        debug!(
            "Creating output stream: {} @ {}Hz, {} channels (monitor: {}, streaming_tap: {})",
            config.name, config.sample_rate, config.channels, has_monitor, streaming_tap.is_some()
        );

        // Spawn thread immediately with monitor state (moved, not stored)
        let ring_for_thread = Arc::clone(&ring_buffer);
        let running_for_thread = Arc::clone(&running);
        let stats_for_thread = Arc::clone(&stats);
        let config_clone = config.clone();

        let thread_handle = thread::Builder::new()
            .name("pipewire-output".to_string())
            .spawn(move || {
                if let Err(e) = run_pipewire_loop(
                    config_clone,
                    ring_for_thread,
                    running_for_thread,
                    stats_for_thread,
                    monitor,
                    streaming_tap,
                    timeline_consumer,
                    rave_input,
                    rave_output,
                ) {
                    error!("PipeWire output thread failed: {}", e);
                }
            })
            .map_err(|e| PipeWireOutputError::ThreadSpawn(e.to_string()))?;

        info!(
            "PipeWire output stream started: {} @ {}Hz, {} channels",
            config.name, config.sample_rate, config.channels
        );

        Ok(Self {
            ring_buffer,
            running,
            thread_handle: Some(thread_handle),
            config,
            started: true,
            stats,
            has_monitor,
        })
    }

    /// Create a new PipeWire output stream without starting it
    ///
    /// This allows you to pre-fill the ring buffer before calling `start()`.
    pub fn new_paused(config: PipeWireOutputConfig) -> Result<Self, PipeWireOutputError> {
        use pipewire as pw;

        // Initialize PipeWire (safe to call multiple times)
        pw::init();

        // Create ring buffer sized for ~1 second of audio
        // PipeWire may request large buffers (e.g., 24576 samples = ~256ms at 48kHz stereo)
        // so we need enough headroom
        let ring_capacity = config.sample_rate as usize * config.channels as usize * 2;
        let ring_buffer = Arc::new(Mutex::new(RingBuffer::new(ring_capacity)));

        debug!(
            "Created ring buffer with capacity {} samples (~{:.1}s at {}Hz {}ch)",
            ring_capacity,
            ring_capacity as f64 / (config.sample_rate as f64 * config.channels as f64),
            config.sample_rate,
            config.channels
        );

        let running = Arc::new(AtomicBool::new(true));
        let stats = Arc::new(StreamStats::default());

        info!(
            "PipeWire output stream created (paused): {} @ {}Hz, {} channels",
            config.name, config.sample_rate, config.channels
        );

        Ok(Self {
            ring_buffer,
            running,
            thread_handle: None,
            config,
            started: false,
            stats,
            has_monitor: false,
        })
    }

    /// Start the PipeWire thread (call after pre-filling the buffer)
    ///
    /// Note: To start with monitor mixing, use `new_with_monitor` instead.
    /// This method starts without monitor support.
    pub fn start(&mut self) -> Result<(), PipeWireOutputError> {
        if self.started {
            return Ok(()); // Already started
        }

        let ring_for_thread = Arc::clone(&self.ring_buffer);
        let running_for_thread = Arc::clone(&self.running);
        let stats_for_thread = Arc::clone(&self.stats);
        let config_clone = self.config.clone();

        let thread_handle = thread::Builder::new()
            .name("pipewire-output".to_string())
            .spawn(move || {
                if let Err(e) = run_pipewire_loop(
                    config_clone,
                    ring_for_thread,
                    running_for_thread,
                    stats_for_thread,
                    None, // No monitor mixing - use new_with_monitor for that
                    None, // No streaming tap - use new_with_streaming_tap for that
                    None, // No timeline consumer - use new_with_streaming_tap for that
                    None, // No RAVE input
                    None, // No RAVE output
                ) {
                    error!("PipeWire output thread failed: {}", e);
                }
            })
            .map_err(|e| PipeWireOutputError::ThreadSpawn(e.to_string()))?;

        self.thread_handle = Some(thread_handle);
        self.started = true;

        info!(
            "PipeWire output stream started: {} @ {}Hz, {} channels",
            self.config.name,
            self.config.sample_rate,
            self.config.channels,
        );

        Ok(())
    }

    /// Get access to the ring buffer for writing audio
    pub fn ring_buffer(&self) -> Arc<Mutex<RingBuffer>> {
        Arc::clone(&self.ring_buffer)
    }

    /// Check if the stream is still running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }

    /// Get the configuration
    pub fn config(&self) -> &PipeWireOutputConfig {
        &self.config
    }

    /// Get runtime statistics (callbacks, samples written, underruns)
    pub fn stats(&self) -> &Arc<StreamStats> {
        eprintln!("stats() returning Arc ptr: {:p}, callbacks: {}",
            Arc::as_ptr(&self.stats),
            self.stats.callbacks.load(std::sync::atomic::Ordering::Relaxed));
        &self.stats
    }

    /// Stop the stream
    pub fn stop(&mut self) {
        self.running.store(false, Ordering::Release);
        if let Some(handle) = self.thread_handle.take() {
            debug!("Waiting for PipeWire thread to stop...");
            let _ = handle.join();
            info!("PipeWire output stream stopped");
        }
    }
}

impl Drop for PipeWireOutputStream {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Run the PipeWire main loop (called from thread)
fn run_pipewire_loop(
    config: PipeWireOutputConfig,
    _ring_buffer: Arc<Mutex<RingBuffer>>, // Deprecated: kept for backwards compatibility
    running: Arc<AtomicBool>,
    stats: Arc<StreamStats>,
    monitor: Option<MonitorMixState>,
    streaming_tap: Option<AudioRingProducer>,
    timeline_consumer: Option<AudioRingConsumer>,
    rave_input: Option<Arc<Mutex<Option<AudioRingProducer>>>>,
    rave_output: Option<Arc<Mutex<Option<AudioRingConsumer>>>>,
) -> Result<(), PipeWireOutputError> {
    use pipewire as pw;
    use pw::spa::pod::Pod;
    use pw::{properties::properties, spa};

    let mainloop = pw::main_loop::MainLoopRc::new(None)
        .map_err(|e| PipeWireOutputError::Init(format!("Failed to create main loop: {}", e)))?;

    let context = pw::context::ContextRc::new(&mainloop, None)
        .map_err(|e| PipeWireOutputError::Init(format!("Failed to create context: {}", e)))?;

    let core = context
        .connect_rc(None)
        .map_err(|e| PipeWireOutputError::Init(format!("Failed to connect to PipeWire: {}", e)))?;

    // Build latency string: "frames/rate" format
    let latency_str = if config.latency_frames > 0 {
        format!("{}/{}", config.latency_frames, config.sample_rate)
    } else {
        String::new()
    };

    // Create stream with properties
    let mut props = properties! {
        *pw::keys::MEDIA_TYPE => "Audio",
        *pw::keys::MEDIA_ROLE => "Music",
        *pw::keys::MEDIA_CATEGORY => "Playback",
        *pw::keys::AUDIO_CHANNELS => config.channels.to_string(),
    };

    // Add latency hint if specified
    if !latency_str.is_empty() {
        props.insert("node.latency", latency_str.as_str());
        debug!("Requesting latency: {} ({:.1}ms)", latency_str, config.latency_ms());
    }

    let stream = pw::stream::StreamBox::new(&core, &config.name, props)
        .map_err(|e| PipeWireOutputError::Init(format!("Failed to create stream: {}", e)))?;

    let channels = config.channels as usize;
    let sample_size = std::mem::size_of::<f32>();
    let stride = sample_size * channels;
    let target_frames = config.latency_frames as usize;

    // Pre-allocate RT buffers (8192 frames * 2 channels = 16384 samples max)
    // This avoids allocation in the RT callback path
    let max_buffer_samples = 8192 * channels;
    let output_buffer = vec![0.0f32; max_buffer_samples];
    let temp_buffer = vec![0.0f32; max_buffer_samples];

    // Register process callback - runs in PipeWire's RT thread
    // This is the RT mixer: reads from monitor input (if enabled) and timeline (lock-free!)
    // Final mixed output is also written to streaming_tap for WebSocket streaming
    // RAVE rings are optional Arc<Mutex<Option<...>>> - RT thread try_locks them
    let _listener = stream
        .add_local_listener_with_user_data((
            stats.clone(),
            monitor,
            streaming_tap,
            timeline_consumer,
            output_buffer,
            temp_buffer,
            rave_input,
            rave_output,
        ))
        .process(move |stream, (stats, monitor, streaming_tap, timeline_consumer, ref mut output_buffer, ref mut temp_buffer, rave_input, rave_output)| {
            let count = stats.callbacks.fetch_add(1, Ordering::Relaxed);
            // Debug: log first callback
            if count == 0 {
                eprintln!("🎵 First PipeWire process callback!");
            }

            let Some(mut buffer) = stream.dequeue_buffer() else {
                return;
            };

            let requested = buffer.requested() as usize;
            let datas = buffer.datas_mut();
            let Some(data) = datas.first_mut() else {
                return;
            };
            let Some(slice) = data.data() else {
                return;
            };

            let max_frames = slice.len() / stride;
            let n_frames = if requested > 0 {
                target_frames.min(requested).min(max_frames)
            } else {
                target_frames.min(max_frames)
            };

            let samples_needed = n_frames * channels;

            // Use pre-allocated buffers, clear only the portion we need
            let output_slice = &mut output_buffer[..samples_needed];
            let temp_slice = &mut temp_buffer[..samples_needed];
            output_slice.fill(0.0);
            temp_slice.fill(0.0);
            let mut has_audio = false;

            // === RT Mixer: Mix monitor input if enabled (lock-free!) ===
            if let Some(ref mut mon) = monitor {
                if mon.enabled.load(Ordering::Relaxed) {
                    // Lock-free read from SPSC ring buffer - never blocks!
                    let read = mon.consumer.read(temp_slice);
                    if read > 0 {
                        stats.monitor_reads.fetch_add(1, Ordering::Relaxed);
                        stats.monitor_samples.fetch_add(read as u64, Ordering::Relaxed);
                        // Mark warmed up after first successful read
                        stats.warmed_up.store(true, Ordering::Relaxed);

                        // Check if RAVE streaming is active (non-blocking)
                        let rave_active = rave_input.as_ref()
                            .and_then(|r| r.try_lock().ok())
                            .map(|g| g.is_some())
                            .unwrap_or(false);

                        if rave_active {
                            // RAVE is active - send to RAVE, don't mix raw monitor
                            if let Some(ref rave_in) = rave_input {
                                if let Ok(mut guard) = rave_in.try_lock() {
                                    if let Some(ref mut producer) = *guard {
                                        producer.write(&temp_slice[..read]);
                                    }
                                }
                            }
                            // Don't set has_audio - RAVE output will provide it
                        } else {
                            // No RAVE - mix raw monitor directly to output
                            let gain = mon.gain.load(Ordering::Relaxed);
                            for i in 0..read {
                                output_slice[i] += temp_slice[i] * gain;
                            }
                            has_audio = true;
                        }
                    }
                }
            }

            // === RT Mixer: Mix timeline audio (lock-free!) ===
            let timeline_read = if let Some(ref mut consumer) = timeline_consumer {
                consumer.read(temp_slice)
            } else {
                0
            };

            if timeline_read > 0 {
                for i in 0..timeline_read {
                    output_slice[i] += temp_slice[i];
                }
                has_audio = true;
            }

            // === RAVE Output: Mix RAVE-processed audio ===
            // Try to get the RAVE output consumer (non-blocking)
            if let Some(ref rave_out) = rave_output {
                if let Ok(mut guard) = rave_out.try_lock() {
                    if let Some(ref mut consumer) = *guard {
                        // Read RAVE-processed audio
                        let rave_read = consumer.read(temp_slice);
                        if rave_read > 0 {
                            // Mix RAVE output with existing audio
                            for i in 0..rave_read {
                                output_slice[i] += temp_slice[i];
                            }
                            has_audio = true;
                        }
                    }
                }
            }

            // Count underrun only if no audio from any source AND we've warmed up
            // (don't count initial empty buffer during startup)
            if !has_audio && stats.warmed_up.load(Ordering::Relaxed) {
                stats.underruns.fetch_add(1, Ordering::Relaxed);
            }

            // Count total samples written to output (from all sources)
            stats
                .samples_written
                .fetch_add(samples_needed as u64, Ordering::Relaxed);

            // Write final mixed output to streaming tap for WebSocket consumers
            // This is lock-free (SPSC) so it won't block the RT thread
            if let Some(ref mut tap) = streaming_tap {
                tap.write(output_slice);
            }

            // Fill output buffer
            for i in 0..n_frames {
                for c in 0..channels {
                    let sample_idx = i * channels + c;
                    let sample = if sample_idx < samples_needed {
                        output_slice[sample_idx]
                    } else {
                        0.0
                    };
                    let start = i * stride + c * sample_size;
                    slice[start..start + sample_size].copy_from_slice(&sample.to_le_bytes());
                }
            }

            let chunk = data.chunk_mut();
            *chunk.offset_mut() = 0;
            *chunk.stride_mut() = stride as i32;
            *chunk.size_mut() = (stride * n_frames) as u32;
        })
        .register()
        .map_err(|e| PipeWireOutputError::Init(format!("Failed to register listener: {}", e)))?;

    // Build audio format parameters
    let mut audio_info = spa::param::audio::AudioInfoRaw::new();
    audio_info.set_format(spa::param::audio::AudioFormat::F32LE);
    audio_info.set_rate(config.sample_rate);
    audio_info.set_channels(config.channels);

    // Set channel positions
    let mut position = [0u32; spa::param::audio::MAX_CHANNELS];
    if config.channels >= 1 {
        position[0] = pipewire::spa::sys::SPA_AUDIO_CHANNEL_FL;
    }
    if config.channels >= 2 {
        position[1] = pipewire::spa::sys::SPA_AUDIO_CHANNEL_FR;
    }
    audio_info.set_position(position);

    // Serialize format to Pod
    let values: Vec<u8> = pw::spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &pw::spa::pod::Value::Object(pw::spa::pod::Object {
            type_: pipewire::spa::sys::SPA_TYPE_OBJECT_Format,
            id: pipewire::spa::sys::SPA_PARAM_EnumFormat,
            properties: audio_info.into(),
        }),
    )
    .map_err(|e| PipeWireOutputError::Init(format!("Failed to serialize format: {}", e)))?
    .0
    .into_inner();

    let mut params = [Pod::from_bytes(&values)
        .ok_or_else(|| PipeWireOutputError::Init("Failed to create Pod from bytes".to_string()))?];

    // Connect stream for output
    stream
        .connect(
            spa::utils::Direction::Output,
            None,
            pw::stream::StreamFlags::AUTOCONNECT
                | pw::stream::StreamFlags::MAP_BUFFERS
                | pw::stream::StreamFlags::RT_PROCESS,
            &mut params,
        )
        .map_err(|e| PipeWireOutputError::Init(format!("Failed to connect stream: {}", e)))?;

    info!("PipeWire stream connected, entering main loop");

    // Set up a timer to check the running flag periodically
    let mainloop_weak = mainloop.downgrade();
    let timer = mainloop.loop_().add_timer(move |_| {
        if !running.load(Ordering::Acquire) {
            if let Some(ml) = mainloop_weak.upgrade() {
                ml.quit();
            }
        }
    });

    // Check every 100ms
    timer
        .update_timer(
            Some(std::time::Duration::from_millis(100)),
            Some(std::time::Duration::from_millis(100)),
        )
        .into_result()
        .map_err(|e| PipeWireOutputError::Init(format!("Failed to set timer: {}", e)))?;

    // Run the main loop - this blocks until quit
    mainloop.run();

    info!("PipeWire main loop exited");
    Ok(())
}

/// Errors from PipeWire output
#[derive(Debug, thiserror::Error)]
pub enum PipeWireOutputError {
    #[error("PipeWire feature not enabled (compile with --features pipewire)")]
    NotAvailable,

    #[error("Failed to initialize PipeWire: {0}")]
    Init(String),

    #[error("Failed to spawn PipeWire thread: {0}")]
    ThreadSpawn(String),

    #[error("Stream error: {0}")]
    Stream(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = PipeWireOutputConfig::default();
        assert_eq!(config.sample_rate, 48000);
        assert_eq!(config.channels, 2);
        assert_eq!(config.name, "chaosgarden");
    }
}
