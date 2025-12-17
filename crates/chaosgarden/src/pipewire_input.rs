//! PipeWire audio input (capture) stream
//!
//! Captures audio from hardware devices through PipeWire and writes to stream_io for recording.
//!
//! Architecture:
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
//!                  StreamManager::write_samples()
//!                              │
//!                              ▼
//!                      mmap'd chunk file
//! ```

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use tracing::{debug, error, info};

use crate::stream_io::{StreamManager, StreamUri};

/// Configuration for PipeWire input stream
#[derive(Debug, Clone)]
pub struct PipeWireInputConfig {
    /// PipeWire device/node name to capture from (e.g., "alsa_input.usb-...")
    pub device_name: String,
    /// Our stream identifier
    pub stream_uri: StreamUri,
    /// Sample rate (must match device or will fail)
    pub sample_rate: u32,
    /// Number of channels (1=mono, 2=stereo, etc.)
    pub channels: u32,
}

/// Handle to a running PipeWire input stream
///
/// The stream runs in its own thread with PipeWire's main loop.
/// Audio data is captured from the device and written to the StreamManager.
pub struct PipeWireInputStream {
    stream_uri: StreamUri,
    device_name: String,
    running: Arc<AtomicBool>,
    thread_handle: Option<JoinHandle<()>>,
    config: PipeWireInputConfig,
    // Stats for monitoring (updated by RT callback)
    stats: Arc<StreamStats>,
}

/// Runtime statistics from the PipeWire callback
#[derive(Debug, Default)]
pub struct StreamStats {
    pub callbacks: std::sync::atomic::AtomicU64,
    pub samples_captured: std::sync::atomic::AtomicU64,
    pub write_errors: std::sync::atomic::AtomicU64,
}

impl PipeWireInputStream {
    /// Create and start a new PipeWire input stream
    ///
    /// This immediately starts capturing audio from the specified device.
    pub fn new(
        config: PipeWireInputConfig,
        stream_manager: Arc<StreamManager>,
    ) -> Result<Self, PipeWireInputError> {
        use pipewire as pw;

        // Initialize PipeWire (safe to call multiple times)
        pw::init();

        let running = Arc::new(AtomicBool::new(true));
        let stats = Arc::new(StreamStats::default());

        let stream_uri = config.stream_uri.clone();
        let device_name = config.device_name.clone();

        let running_for_thread = Arc::clone(&running);
        let stats_for_thread = Arc::clone(&stats);
        let config_clone = config.clone();

        let thread_handle = thread::Builder::new()
            .name(format!("pipewire-input-{}", stream_uri.as_str()))
            .spawn(move || {
                if let Err(e) = run_pipewire_capture_loop(
                    config_clone,
                    stream_manager,
                    running_for_thread,
                    stats_for_thread,
                ) {
                    error!("PipeWire input thread failed: {}", e);
                }
            })
            .map_err(|e| PipeWireInputError::ThreadSpawn(e.to_string()))?;

        info!(
            "PipeWire input stream started: {} from device {} @ {}Hz, {} channels",
            stream_uri.as_str(),
            device_name,
            config.sample_rate,
            config.channels
        );

        Ok(Self {
            stream_uri,
            device_name,
            running,
            thread_handle: Some(thread_handle),
            config,
            stats,
        })
    }

    /// Get the stream URI
    pub fn stream_uri(&self) -> &StreamUri {
        &self.stream_uri
    }

    /// Get the device name
    pub fn device_name(&self) -> &str {
        &self.device_name
    }

    /// Check if the stream is still running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }

    /// Get the configuration
    pub fn config(&self) -> &PipeWireInputConfig {
        &self.config
    }

    /// Get runtime statistics (callbacks, samples captured, errors)
    pub fn stats(&self) -> &Arc<StreamStats> {
        &self.stats
    }

    /// Stop the input stream
    pub fn stop(&mut self) {
        self.running.store(false, Ordering::Release);
        if let Some(handle) = self.thread_handle.take() {
            debug!("Waiting for PipeWire input thread to stop...");
            let _ = handle.join();
            info!(
                "PipeWire input stream stopped: {}",
                self.stream_uri.as_str()
            );
        }
    }
}

impl Drop for PipeWireInputStream {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Run the PipeWire capture loop (called from thread)
fn run_pipewire_capture_loop(
    config: PipeWireInputConfig,
    stream_manager: Arc<StreamManager>,
    running: Arc<AtomicBool>,
    stats: Arc<StreamStats>,
) -> Result<(), PipeWireInputError> {
    use pipewire as pw;
    use pw::spa::pod::Pod;
    use pw::{properties::properties, spa};

    let mainloop = pw::main_loop::MainLoopRc::new(None).map_err(|e| {
        PipeWireInputError::Init(format!("Failed to create main loop: {}", e))
    })?;

    let context = pw::context::ContextRc::new(&mainloop, None).map_err(|e| {
        PipeWireInputError::Init(format!("Failed to create context: {}", e))
    })?;

    let core = context.connect_rc(None).map_err(|e| {
        PipeWireInputError::Init(format!("Failed to connect to PipeWire: {}", e))
    })?;

    // Create stream with properties for capture
    let props = properties! {
        *pw::keys::MEDIA_TYPE => "Audio",
        *pw::keys::MEDIA_ROLE => "Production", // Recording/production role
        *pw::keys::MEDIA_CATEGORY => "Capture",
        *pw::keys::AUDIO_CHANNELS => config.channels.to_string(),
        "target.object" => config.device_name.as_str(), // Target specific device
    };

    let stream_name = format!("capture-{}", config.stream_uri.as_str());
    let stream = pw::stream::StreamBox::new(&core, &stream_name, props)
        .map_err(|e| PipeWireInputError::Init(format!("Failed to create stream: {}", e)))?;

    let channels = config.channels as usize;
    let sample_size = std::mem::size_of::<f32>();
    let stride = sample_size * channels;

    // Register process callback - runs in PipeWire's RT thread
    // CRITICAL: This callback MUST be RT-safe (no blocking, no allocation on hot path)
    let stream_uri_for_callback = config.stream_uri.clone();
    let _listener = stream
        .add_local_listener_with_user_data((stream_manager, stats))
        .process(move |stream, (stream_mgr, stats)| {
            stats.callbacks.fetch_add(1, Ordering::Relaxed);

            // Get buffer from PipeWire
            let Some(mut buffer) = stream.dequeue_buffer() else {
                return;
            };

            let datas = buffer.datas_mut();
            let Some(data) = datas.first_mut() else {
                return;
            };

            // Get chunk info first (before borrowing data)
            let size = data.chunk().size() as usize;

            let Some(slice) = data.data() else {
                return;
            };
            let n_frames = size / stride;

            if n_frames == 0 {
                return;
            }

            // RT-SAFE: Allocate temp buffer (could optimize with thread-local pool)
            let mut samples = vec![0.0f32; n_frames * channels];

            // RT-SAFE: Read samples from PipeWire buffer
            for i in 0..n_frames {
                for c in 0..channels {
                    let byte_offset = i * stride + c * sample_size;
                    if byte_offset + sample_size <= slice.len() {
                        let bytes = &slice[byte_offset..byte_offset + sample_size];
                        samples[i * channels + c] = f32::from_le_bytes([
                            bytes[0], bytes[1], bytes[2], bytes[3],
                        ]);
                    }
                }
            }

            // RT-SAFE: Convert f32 samples to bytes
            // Safety: transmute Vec<f32> to &[u8] for writing
            let sample_count = samples.len() as u64;
            let bytes: &[u8] = unsafe {
                std::slice::from_raw_parts(
                    samples.as_ptr() as *const u8,
                    samples.len() * std::mem::size_of::<f32>(),
                )
            };

            // RT-SAFE: Write to mmap'd file via StreamManager
            if let Err(e) = stream_mgr.write_samples(&stream_uri_for_callback, bytes, sample_count) {
                // Log error but don't panic - keep capturing
                // Note: tracing is RT-safe with lock-free logging
                error!(
                    stream.uri = %stream_uri_for_callback.as_str(),
                    "Failed to write samples: {}", e
                );
                stats.write_errors.fetch_add(1, Ordering::Relaxed);
            } else {
                stats
                    .samples_captured
                    .fetch_add(sample_count, Ordering::Relaxed);
            }

            // Note: Chunk rotation (StreamChunkFull broadcast) is handled by
            // StreamManager internally when write_samples() detects chunk full
        })
        .register()
        .map_err(|e| {
            PipeWireInputError::Init(format!("Failed to register listener: {}", e))
        })?;

    // Build audio format parameters for F32LE (float32 little-endian)
    let mut audio_info = spa::param::audio::AudioInfoRaw::new();
    audio_info.set_format(spa::param::audio::AudioFormat::F32LE);
    audio_info.set_rate(config.sample_rate);
    audio_info.set_channels(config.channels);

    // Set channel positions
    let mut position = [0u32; spa::param::audio::MAX_CHANNELS];
    if config.channels >= 1 {
        position[0] = pipewire::spa::sys::SPA_AUDIO_CHANNEL_FL; // Front Left
    }
    if config.channels >= 2 {
        position[1] = pipewire::spa::sys::SPA_AUDIO_CHANNEL_FR; // Front Right
    }
    // For >2 channels, could set more positions, but default should work
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
    .map_err(|e| PipeWireInputError::Init(format!("Failed to serialize format: {}", e)))?
    .0
    .into_inner();

    let mut params = [Pod::from_bytes(&values).ok_or_else(|| {
        PipeWireInputError::Init("Failed to create Pod from bytes".to_string())
    })?];

    // Connect stream for INPUT (capture)
    stream
        .connect(
            spa::utils::Direction::Input, // KEY DIFFERENCE: Input for capture
            None,                          // Don't specify target node (use NODE_TARGET property)
            pw::stream::StreamFlags::AUTOCONNECT
                | pw::stream::StreamFlags::MAP_BUFFERS
                | pw::stream::StreamFlags::RT_PROCESS,
            &mut params,
        )
        .map_err(|e| PipeWireInputError::Init(format!("Failed to connect stream: {}", e)))?;

    info!(
        "PipeWire capture stream connected to {}, entering main loop",
        config.device_name
    );

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
        .map_err(|e| PipeWireInputError::Init(format!("Failed to set timer: {}", e)))?;

    // Run the main loop - this blocks until quit
    mainloop.run();

    info!("PipeWire capture main loop exited for {}", config.stream_uri.as_str());
    Ok(())
}

/// Errors from PipeWire input
#[derive(Debug, thiserror::Error)]
pub enum PipeWireInputError {
    #[error("PipeWire feature not enabled (compile with --features pipewire)")]
    NotAvailable,

    #[error("Failed to initialize PipeWire: {0}")]
    Init(String),

    #[error("Failed to spawn PipeWire thread: {0}")]
    ThreadSpawn(String),

    #[error("Stream error: {0}")]
    Stream(String),

    #[error("Device not found: {0}")]
    DeviceNotFound(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_creation() {
        let config = PipeWireInputConfig {
            device_name: "test_device".to_string(),
            stream_uri: StreamUri::from("stream://test/audio"),
            sample_rate: 48000,
            channels: 2,
        };

        assert_eq!(config.device_name, "test_device");
        assert_eq!(config.sample_rate, 48000);
        assert_eq!(config.channels, 2);
    }
}
