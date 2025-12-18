//! PipeWire monitor input stream
//!
//! Captures audio from hardware devices through PipeWire and writes to a ring buffer
//! for live monitoring (passthrough to output).
//!
//! Architecture (lock-free version):
//! ```text
//! Hardware Device               Chaosgarden
//!       │                            │
//!       ▼                            ▼
//! PipeWire Graph ─────▶ Input Stream (RT thread)
//!                              │
//!                              ▼
//!                       process callback
//!                       (PipeWire RT)
//!                              │
//!                              ▼
//!                       AudioRingProducer.write()
//!                       (lock-free, wait-free)
//!                              │
//!                              ▼
//!                       Output callback reads via AudioRingConsumer
//! ```

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use tracing::{debug, error, info};

use crate::external_io::AudioRingProducer;

/// Configuration for monitor input stream
#[derive(Debug, Clone)]
pub struct MonitorInputConfig {
    /// PipeWire device/node name to capture from
    /// If None, uses default input device
    pub device_name: Option<String>,
    /// Sample rate (should match output)
    pub sample_rate: u32,
    /// Number of channels (1=mono, 2=stereo)
    pub channels: u32,
}

impl Default for MonitorInputConfig {
    fn default() -> Self {
        Self {
            device_name: None,
            sample_rate: 48000,
            channels: 2,
        }
    }
}

/// Runtime statistics from the PipeWire callback
#[derive(Debug, Default)]
pub struct MonitorStats {
    pub callbacks: AtomicU64,
    pub samples_captured: AtomicU64,
    pub overruns: AtomicU64,
    // Warmup flag - don't count overruns until first successful write
    pub warmed_up: AtomicBool,
}

/// Handle to a running PipeWire monitor input stream
///
/// The stream runs in its own thread with PipeWire's main loop.
/// Audio data is captured from the device and written to a lock-free ring buffer.
pub struct MonitorInputStream {
    running: Arc<AtomicBool>,
    thread_handle: Option<JoinHandle<()>>,
    config: MonitorInputConfig,
    stats: Arc<MonitorStats>,
}

/// Error type for monitor input operations
#[derive(Debug, thiserror::Error)]
pub enum MonitorInputError {
    #[error("Failed to initialize PipeWire: {0}")]
    InitFailed(String),

    #[error("Failed to create stream: {0}")]
    StreamFailed(String),

    #[error("Thread spawn failed: {0}")]
    ThreadFailed(#[from] std::io::Error),
}

impl MonitorInputStream {
    /// Create and start a new monitor input stream with a lock-free producer
    ///
    /// The producer end of the ring buffer is owned by this stream and used
    /// in the PipeWire RT callback. The consumer end should be given to the
    /// output stream for lock-free reading.
    pub fn new(
        config: MonitorInputConfig,
        producer: AudioRingProducer,
    ) -> Result<Self, MonitorInputError> {
        use pipewire as pw;

        // Initialize PipeWire (safe to call multiple times)
        pw::init();

        debug!(
            "Creating monitor input stream: device={:?}, {}Hz, {}ch",
            config.device_name, config.sample_rate, config.channels
        );

        let running = Arc::new(AtomicBool::new(true));
        let stats = Arc::new(MonitorStats::default());

        let running_clone = Arc::clone(&running);
        let stats_clone = Arc::clone(&stats);
        let config_clone = config.clone();

        let thread_handle = thread::Builder::new()
            .name("pipewire-monitor-input".to_string())
            .spawn(move || {
                if let Err(e) = run_monitor_capture_loop(
                    config_clone,
                    producer,
                    running_clone,
                    stats_clone,
                ) {
                    error!("Monitor input thread failed: {}", e);
                }
            })?;

        info!(
            "Started monitor input: device={:?}, {}Hz, {}ch",
            config.device_name, config.sample_rate, config.channels
        );

        Ok(Self {
            running,
            thread_handle: Some(thread_handle),
            config,
            stats,
        })
    }

    /// Get runtime statistics
    pub fn stats(&self) -> &Arc<MonitorStats> {
        &self.stats
    }

    /// Get configuration
    pub fn config(&self) -> &MonitorInputConfig {
        &self.config
    }

    /// Stop the capture stream
    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(handle) = self.thread_handle.take() {
            // Give PipeWire time to notice the stop signal
            std::thread::sleep(std::time::Duration::from_millis(100));
            let _ = handle.join();
        }
    }
}

impl Drop for MonitorInputStream {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Run the PipeWire capture main loop
fn run_monitor_capture_loop(
    config: MonitorInputConfig,
    producer: AudioRingProducer,
    running: Arc<AtomicBool>,
    stats: Arc<MonitorStats>,
) -> Result<(), MonitorInputError> {
    use pipewire as pw;
    use pw::spa::pod::Pod;
    use pw::{properties::properties, spa};
    use spa::param::audio::{AudioFormat, AudioInfoRaw};

    let mainloop = pw::main_loop::MainLoopRc::new(None)
        .map_err(|e| MonitorInputError::InitFailed(format!("MainLoop: {:?}", e)))?;
    let context = pw::context::ContextRc::new(&mainloop, None)
        .map_err(|e| MonitorInputError::InitFailed(format!("Context: {:?}", e)))?;
    let core = context
        .connect_rc(None)
        .map_err(|e| MonitorInputError::InitFailed(format!("Core connect: {:?}", e)))?;

    let name = config
        .device_name
        .clone()
        .unwrap_or_else(|| "chaosgarden-monitor".to_string());

    // Build audio format
    let mut audio_info = AudioInfoRaw::new();
    audio_info.set_format(AudioFormat::F32LE);
    audio_info.set_rate(config.sample_rate);
    audio_info.set_channels(config.channels);

    let values: Vec<u8> = spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &spa::pod::Value::Object(spa::pod::Object {
            type_: spa::sys::SPA_TYPE_OBJECT_Format,
            id: spa::sys::SPA_PARAM_EnumFormat,
            properties: audio_info.into(),
        }),
    )
    .map_err(|e| MonitorInputError::StreamFailed(format!("Pod serialize: {:?}", e)))?
    .0
    .into_inner();

    let mut params = [Pod::from_bytes(&values).unwrap()];

    // Create stream properties for capture
    let props = properties! {
        *pw::keys::MEDIA_TYPE => "Audio",
        *pw::keys::MEDIA_CATEGORY => "Capture",
        *pw::keys::MEDIA_ROLE => "Music",
        *pw::keys::AUDIO_CHANNELS => config.channels.to_string(),
    };

    // Create stream for capture
    let stream = pw::stream::StreamBox::new(&core, &name, props)
        .map_err(|e| MonitorInputError::StreamFailed(format!("Stream create: {:?}", e)))?;

    let channels = config.channels as usize;
    let sample_size = std::mem::size_of::<f32>();
    let stride = sample_size * channels;

    // Register process callback - runs in PipeWire's RT thread
    // Uses lock-free AudioRingProducer for wait-free writes
    let _listener = stream
        .add_local_listener_with_user_data((producer, stats))
        .process(move |stream, (producer, stats)| {
            stats.callbacks.fetch_add(1, Ordering::Relaxed);

            // Get buffer from PipeWire
            let Some(mut buffer) = stream.dequeue_buffer() else {
                return;
            };

            let datas = buffer.datas_mut();
            let Some(data) = datas.first_mut() else {
                return;
            };

            // Get chunk size (chunk() returns a reference directly)
            let size = data.chunk().size() as usize;

            let Some(slice) = data.data() else {
                return;
            };

            let n_frames = size / stride;
            if n_frames == 0 {
                return;
            }

            let n_samples = n_frames * channels;

            // Read samples from PipeWire buffer
            // TODO: Pre-allocate this buffer to avoid allocation in RT callback
            let mut temp_buffer = vec![0.0f32; n_samples];
            for i in 0..n_frames {
                for c in 0..channels {
                    let byte_offset = i * stride + c * sample_size;
                    if byte_offset + sample_size <= slice.len() {
                        let bytes = &slice[byte_offset..byte_offset + sample_size];
                        temp_buffer[i * channels + c] = f32::from_le_bytes([
                            bytes[0], bytes[1], bytes[2], bytes[3],
                        ]);
                    }
                }
            }

            // Write to ring buffer (lock-free, never blocks)
            let written = producer.write(&temp_buffer);
            stats
                .samples_captured
                .fetch_add(written as u64, Ordering::Relaxed);

            // Only count overruns after warmup (don't count initial buffer fill)
            // Warmup happens after first COMPLETE write (output is draining the buffer)
            let was_warmed = stats.warmed_up.load(Ordering::Relaxed);
            if written < n_samples {
                if was_warmed {
                    stats.overruns.fetch_add(1, Ordering::Relaxed);
                }
            } else {
                // Full write succeeded - output is keeping up, we're warmed up
                stats.warmed_up.store(true, Ordering::Relaxed);
            }
        })
        .register()
        .map_err(|e| MonitorInputError::StreamFailed(format!("Listener register: {:?}", e)))?;

    // Connect as input (capture)
    stream
        .connect(
            spa::utils::Direction::Input,
            None,
            pw::stream::StreamFlags::AUTOCONNECT
                | pw::stream::StreamFlags::MAP_BUFFERS
                | pw::stream::StreamFlags::RT_PROCESS,
            &mut params,
        )
        .map_err(|e| MonitorInputError::StreamFailed(format!("Stream connect: {:?}", e)))?;

    info!("Monitor input stream connected to PipeWire");

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
        .map_err(|e| MonitorInputError::StreamFailed(format!("Failed to set timer: {}", e)))?;

    // Run the main loop - this blocks until quit
    mainloop.run();

    info!("Monitor input stream stopped");
    Ok(())
}
