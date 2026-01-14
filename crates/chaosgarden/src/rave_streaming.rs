//! RAVE Streaming Client
//!
//! Provides realtime audio streaming to the Python RAVE service for neural audio
//! transformation. Uses ZMQ PAIR sockets for bidirectional audio streaming.
//!
//! Architecture:
//! ```text
//! Monitor Input (RT)     RAVE Client Thread        Python RAVE Service
//!       │                       │                          │
//!       ▼                       │                          │
//! AudioRingProducer ──────► AudioRingConsumer              │
//!                               │                          │
//!                               ▼                          │
//!                         collect buffer                   │
//!                               │                          │
//!                               ▼                          │
//!                         ZMQ PAIR send ─────────────────► recv
//!                               │                          │
//!                               │                          ▼
//!                               │                    RAVE forward()
//!                               │                          │
//!                         ZMQ PAIR recv ◄─────────────────send
//!                               │
//!                               ▼
//!                         write to output ring
//!                               │
//!                               ▼
//!                         AudioRingProducer ──────► AudioRingConsumer
//!                                                          │
//!                                                          ▼
//!                                                   Output Mixer (RT)
//! ```

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Instant;

use tracing::{debug, error, info, warn};

use crate::external_io::{audio_ring_pair, AudioRingConsumer, AudioRingProducer};

/// Default endpoint for RAVE streaming service
pub const DEFAULT_RAVE_STREAMING_ENDPOINT: &str = "tcp://127.0.0.1:5592";

/// Configuration for RAVE streaming
#[derive(Debug, Clone)]
pub struct RaveStreamingConfig {
    /// ZMQ endpoint for RAVE streaming service
    pub endpoint: String,
    /// Buffer size in frames (default 2048 = ~43ms at 48kHz)
    pub buffer_frames: usize,
    /// Sample rate (should match audio I/O)
    pub sample_rate: u32,
    /// Number of channels (typically 2 for stereo)
    pub channels: u32,
}

impl Default for RaveStreamingConfig {
    fn default() -> Self {
        Self {
            endpoint: DEFAULT_RAVE_STREAMING_ENDPOINT.to_string(),
            buffer_frames: 2048,
            sample_rate: 48000,
            channels: 2,
        }
    }
}

/// Statistics for RAVE streaming
#[derive(Debug, Default)]
pub struct RaveStreamingStats {
    /// Number of audio chunks sent to RAVE
    pub chunks_sent: AtomicU64,
    /// Number of audio chunks received from RAVE
    pub chunks_received: AtomicU64,
    /// Total samples processed
    pub samples_processed: AtomicU64,
    /// Number of input underruns (nothing to read from input)
    pub input_underruns: AtomicU64,
    /// Number of output overruns (output buffer full)
    pub output_overruns: AtomicU64,
    /// Number of ZMQ errors
    pub zmq_errors: AtomicU64,
}

/// Error types for RAVE streaming
#[derive(Debug, thiserror::Error)]
pub enum RaveStreamingError {
    #[error("Failed to connect to RAVE service at {endpoint}: {message}")]
    ConnectionFailed { endpoint: String, message: String },

    #[error("ZMQ error: {0}")]
    ZmqError(#[from] zmq::Error),

    #[error("Thread spawn failed: {0}")]
    ThreadFailed(#[from] std::io::Error),

    #[error("Streaming not running")]
    NotRunning,

    #[error("Streaming already running")]
    AlreadyRunning,
}

/// RAVE streaming session information
#[derive(Debug, Clone)]
pub struct RaveStreamingSession {
    /// Unique session ID
    pub stream_id: String,
    /// Model name being used
    pub model_name: String,
    /// Input identity (graph node reference)
    pub input_identity: String,
    /// Output identity (graph node reference)
    pub output_identity: String,
    /// When the session started
    pub started_at: std::time::SystemTime,
    /// Whether the session is currently running
    pub running: bool,
}

/// RAVE Streaming Client
///
/// Manages the connection to the Python RAVE service and handles realtime
/// audio streaming through the neural audio codec.
pub struct RaveStreamingClient {
    config: RaveStreamingConfig,
    running: Arc<AtomicBool>,
    thread_handle: Option<JoinHandle<()>>,
    stats: Arc<RaveStreamingStats>,
    /// Current session info (if running)
    session: Option<RaveStreamingSession>,
}

impl RaveStreamingClient {
    /// Create a new RAVE streaming client with default configuration
    pub fn new() -> Self {
        Self::with_config(RaveStreamingConfig::default())
    }

    /// Create a new RAVE streaming client with custom configuration
    pub fn with_config(config: RaveStreamingConfig) -> Self {
        Self {
            config,
            running: Arc::new(AtomicBool::new(false)),
            thread_handle: None,
            stats: Arc::new(RaveStreamingStats::default()),
            session: None,
        }
    }

    /// Start a streaming session
    ///
    /// Returns a tuple of (input_producer, output_consumer) that should be
    /// integrated into the audio pipeline:
    /// - Write monitor audio to input_producer
    /// - Read RAVE-processed audio from output_consumer
    pub fn start(
        &mut self,
        stream_id: String,
        model_name: String,
        input_identity: String,
        output_identity: String,
    ) -> Result<(AudioRingProducer, AudioRingConsumer), RaveStreamingError> {
        if self.running.load(Ordering::Acquire) {
            return Err(RaveStreamingError::AlreadyRunning);
        }

        info!(
            "Starting RAVE streaming: stream_id={}, model={}, endpoint={}",
            stream_id, model_name, self.config.endpoint
        );

        // Create ring buffer pairs for input and output
        // Size: ~1 second of stereo audio
        let ring_capacity = self.config.sample_rate as usize * self.config.channels as usize;

        // Input ring: monitor -> rave client
        let (input_producer_for_caller, input_consumer) = audio_ring_pair(ring_capacity);

        // Output ring: rave client -> output mixer
        let (output_producer, output_consumer_for_caller) = audio_ring_pair(ring_capacity);

        // Clone for thread
        let running = Arc::clone(&self.running);
        let stats = Arc::clone(&self.stats);
        let config = self.config.clone();
        let stream_id_clone = stream_id.clone();

        // Set running flag
        self.running.store(true, Ordering::Release);

        // Spawn the streaming thread
        let thread_handle = thread::Builder::new()
            .name(format!("rave-streaming-{}", stream_id))
            .spawn(move || {
                if let Err(e) = run_streaming_loop(
                    config,
                    stream_id_clone,
                    input_consumer,
                    output_producer,
                    running,
                    stats,
                ) {
                    error!("RAVE streaming thread failed: {}", e);
                }
            })?;

        self.thread_handle = Some(thread_handle);
        self.session = Some(RaveStreamingSession {
            stream_id,
            model_name,
            input_identity,
            output_identity,
            started_at: std::time::SystemTime::now(),
            running: true,
        });

        Ok((input_producer_for_caller, output_consumer_for_caller))
    }

    /// Stop the current streaming session
    pub fn stop(&mut self) -> Result<RaveStreamingSession, RaveStreamingError> {
        if !self.running.load(Ordering::Acquire) {
            return Err(RaveStreamingError::NotRunning);
        }

        info!("Stopping RAVE streaming session");

        // Signal thread to stop
        self.running.store(false, Ordering::Release);

        // Wait for thread to finish
        if let Some(handle) = self.thread_handle.take() {
            // Give thread time to notice the stop signal
            std::thread::sleep(std::time::Duration::from_millis(100));
            let _ = handle.join();
        }

        // Take and return session info
        let mut session = self.session.take().ok_or(RaveStreamingError::NotRunning)?;
        session.running = false;

        info!(
            "RAVE streaming stopped: {} chunks processed",
            self.stats.chunks_received.load(Ordering::Relaxed)
        );

        Ok(session)
    }

    /// Get the current session info
    pub fn session(&self) -> Option<&RaveStreamingSession> {
        self.session.as_ref()
    }

    /// Check if streaming is currently running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }

    /// Get streaming statistics
    pub fn stats(&self) -> &Arc<RaveStreamingStats> {
        &self.stats
    }
}

impl Default for RaveStreamingClient {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for RaveStreamingClient {
    fn drop(&mut self) {
        if self.is_running() {
            let _ = self.stop();
        }
    }
}

/// Run the main streaming loop in a dedicated thread
fn run_streaming_loop(
    config: RaveStreamingConfig,
    stream_id: String,
    mut input_consumer: AudioRingConsumer,
    mut output_producer: AudioRingProducer,
    running: Arc<AtomicBool>,
    stats: Arc<RaveStreamingStats>,
) -> Result<(), RaveStreamingError> {
    // Create ZMQ context and socket
    let ctx = zmq::Context::new();
    let socket = ctx.socket(zmq::PAIR)?;

    // Set socket options for low latency
    socket.set_sndhwm(2)?; // Only buffer 2 messages
    socket.set_rcvhwm(2)?;
    socket.set_linger(0)?; // Don't wait on close

    // Connect to RAVE service
    socket.connect(&config.endpoint).map_err(|e| {
        RaveStreamingError::ConnectionFailed {
            endpoint: config.endpoint.clone(),
            message: e.to_string(),
        }
    })?;

    info!(
        "RAVE streaming connected to {}, stream_id={}",
        config.endpoint, stream_id
    );

    let samples_per_chunk = config.buffer_frames * config.channels as usize;
    let mut input_buffer = vec![0.0f32; samples_per_chunk];
    let mut output_buffer = vec![0.0f32; samples_per_chunk];

    // Track timing for adaptive processing
    let mut last_send = Instant::now();
    let chunk_duration_ms = (config.buffer_frames as f64 / config.sample_rate as f64 * 1000.0) as u64;

    while running.load(Ordering::Acquire) {
        // Read from input ring buffer
        let samples_read = input_consumer.read(&mut input_buffer);

        if samples_read < samples_per_chunk {
            // Not enough samples yet - wait a bit
            stats.input_underruns.fetch_add(1, Ordering::Relaxed);
            std::thread::sleep(std::time::Duration::from_millis(chunk_duration_ms / 2));
            continue;
        }

        // Convert f32 samples to bytes for ZMQ
        let input_bytes: Vec<u8> = input_buffer
            .iter()
            .flat_map(|&s| s.to_le_bytes())
            .collect();

        // Send to RAVE service
        if let Err(e) = socket.send(&input_bytes, zmq::DONTWAIT) {
            if e == zmq::Error::EAGAIN {
                // Socket would block - RAVE service is falling behind
                debug!("RAVE service falling behind, dropping chunk");
                continue;
            }
            stats.zmq_errors.fetch_add(1, Ordering::Relaxed);
            warn!("ZMQ send error: {}", e);
            continue;
        }
        stats.chunks_sent.fetch_add(1, Ordering::Relaxed);

        // Receive processed audio from RAVE
        // Use poll with timeout to avoid blocking forever
        let poll_result = socket.poll(zmq::POLLIN, chunk_duration_ms as i64 * 2);

        match poll_result {
            Ok(events) if events > 0 => {
                match socket.recv_bytes(zmq::DONTWAIT) {
                    Ok(output_bytes) => {
                        // Convert bytes back to f32 samples
                        let expected_bytes = samples_per_chunk * 4; // 4 bytes per f32
                        if output_bytes.len() >= expected_bytes {
                            for (i, chunk) in output_bytes.chunks_exact(4).take(samples_per_chunk).enumerate() {
                                output_buffer[i] = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                            }

                            // Write to output ring buffer
                            let written = output_producer.write(&output_buffer);
                            if written < samples_per_chunk {
                                stats.output_overruns.fetch_add(1, Ordering::Relaxed);
                            }

                            stats.chunks_received.fetch_add(1, Ordering::Relaxed);
                            stats.samples_processed.fetch_add(samples_per_chunk as u64, Ordering::Relaxed);
                        }
                    }
                    Err(e) if e == zmq::Error::EAGAIN => {
                        // No data available despite poll - unusual
                        debug!("Poll indicated data but recv would block");
                    }
                    Err(e) => {
                        stats.zmq_errors.fetch_add(1, Ordering::Relaxed);
                        warn!("ZMQ recv error: {}", e);
                    }
                }
            }
            Ok(_) => {
                // Poll timeout - no response from RAVE
                debug!("RAVE response timeout");
            }
            Err(e) => {
                stats.zmq_errors.fetch_add(1, Ordering::Relaxed);
                warn!("ZMQ poll error: {}", e);
            }
        }

        // Track timing
        let elapsed = last_send.elapsed();
        if elapsed.as_millis() > chunk_duration_ms as u128 * 3 {
            debug!("Slow RAVE round-trip: {:?}", elapsed);
        }
        last_send = Instant::now();
    }

    info!("RAVE streaming loop exiting, stream_id={}", stream_id);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = RaveStreamingConfig::default();
        assert_eq!(config.endpoint, DEFAULT_RAVE_STREAMING_ENDPOINT);
        assert_eq!(config.buffer_frames, 2048);
        assert_eq!(config.sample_rate, 48000);
        assert_eq!(config.channels, 2);
    }

    #[test]
    fn test_client_creation() {
        let client = RaveStreamingClient::new();
        assert!(!client.is_running());
        assert!(client.session().is_none());
    }

    #[test]
    fn test_already_running_error() {
        // This test would require a mock RAVE service
        // For now, just verify the type exists
        let err = RaveStreamingError::AlreadyRunning;
        assert!(err.to_string().contains("already running"));
    }
}
