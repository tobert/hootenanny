//! Audio file playback node
//!
//! Streams decoded audio from CAS-stored files. Supports WAV (via hound) and
//! optionally MP3/FLAC (via symphonia when `symphonia-decode` feature is enabled).
//!
//! Key design points:
//! - Audio is pre-decoded on `preload()` (not in RT path)
//! - Playback is sample-accurate seeking
//! - Looping is optional
//! - End-of-file outputs silence, doesn't fail

use std::io::Cursor;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use uuid::Uuid;

use crate::primitives::{
    Node, NodeCapabilities, NodeDescriptor, Port, ProcessContext, ProcessError, SignalBuffer,
    SignalType,
};

/// Decoded audio ready for playback
#[derive(Debug, Clone)]
pub struct DecodedAudio {
    /// Interleaved samples (L, R, L, R, ...)
    pub samples: Vec<f32>,
    /// Original sample rate
    pub sample_rate: u32,
    /// Number of channels (1 = mono, 2 = stereo)
    pub channels: u8,
}

impl DecodedAudio {
    /// Total number of frames (samples per channel)
    pub fn frames(&self) -> usize {
        if self.channels == 0 {
            0
        } else {
            self.samples.len() / self.channels as usize
        }
    }

    /// Duration in seconds
    pub fn duration_seconds(&self) -> f64 {
        self.frames() as f64 / self.sample_rate as f64
    }
}

/// How to access content from CAS
pub trait ContentResolver: Send + Sync {
    /// Resolve content hash to raw bytes
    fn resolve(&self, content_hash: &str) -> Result<Vec<u8>>;
}

/// Direct filesystem access to CAS
///
/// CAS is stored as: `{base_path}/{hash[0..2]}/{hash}`
pub struct FileCasClient {
    base_path: PathBuf,
}

impl FileCasClient {
    pub fn new(base_path: impl Into<PathBuf>) -> Self {
        Self {
            base_path: base_path.into(),
        }
    }
}

impl ContentResolver for FileCasClient {
    fn resolve(&self, content_hash: &str) -> Result<Vec<u8>> {
        // CAS layout: {base}/{prefix}/{hash}
        let prefix = &content_hash[0..2.min(content_hash.len())];
        let path = self.base_path.join(prefix).join(content_hash);
        std::fs::read(&path).with_context(|| format!("CAS read failed: {}", path.display()))
    }
}

/// In-memory content resolver for testing
pub struct MemoryResolver {
    content: std::collections::HashMap<String, Vec<u8>>,
}

impl MemoryResolver {
    pub fn new() -> Self {
        Self {
            content: std::collections::HashMap::new(),
        }
    }

    pub fn insert(&mut self, hash: impl Into<String>, data: Vec<u8>) {
        self.content.insert(hash.into(), data);
    }
}

impl Default for MemoryResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl ContentResolver for MemoryResolver {
    fn resolve(&self, content_hash: &str) -> Result<Vec<u8>> {
        self.content
            .get(content_hash)
            .cloned()
            .ok_or_else(|| anyhow!("content not found: {}", content_hash))
    }
}

/// Decode WAV audio using hound (always available)
pub fn decode_wav(data: &[u8]) -> Result<DecodedAudio> {
    let cursor = Cursor::new(data);
    let reader = hound::WavReader::new(cursor).context("failed to parse WAV header")?;

    let spec = reader.spec();
    let channels = spec.channels as u8;
    let sample_rate = spec.sample_rate;

    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .into_samples::<f32>()
            .collect::<Result<Vec<_>, _>>()
            .context("failed to read float samples")?,
        hound::SampleFormat::Int => {
            let bits = spec.bits_per_sample;
            let max_val = (1 << (bits - 1)) as f32;
            reader
                .into_samples::<i32>()
                .map(|s| s.map(|v| v as f32 / max_val))
                .collect::<Result<Vec<_>, _>>()
                .context("failed to read int samples")?
        }
    };

    Ok(DecodedAudio {
        samples,
        sample_rate,
        channels,
    })
}

/// Decode audio using symphonia (MP3, FLAC, OGG, etc.)
#[cfg(feature = "symphonia-decode")]
pub fn decode_audio_symphonia(data: &[u8]) -> Result<DecodedAudio> {
    use symphonia::core::audio::SampleBuffer;
    use symphonia::core::codecs::DecoderOptions;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    let cursor = Cursor::new(data.to_vec());
    let mss = MediaSourceStream::new(Box::new(cursor), Default::default());

    let probed = symphonia::default::get_probe()
        .format(
            &Hint::new(),
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .context("failed to probe audio format")?;

    let mut format = probed.format;
    let track = format
        .default_track()
        .ok_or_else(|| anyhow!("no audio track found"))?;

    let sample_rate = track
        .codec_params
        .sample_rate
        .ok_or_else(|| anyhow!("no sample rate"))?;
    let channels = track
        .codec_params
        .channels
        .map(|c| c.count() as u8)
        .unwrap_or(2);

    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .context("failed to create decoder")?;

    let track_id = track.id;
    let mut samples: Vec<f32> = Vec::new();

    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(symphonia::core::errors::Error::IoError(e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break;
            }
            Err(e) => return Err(e).context("failed to read packet"),
        };

        if packet.track_id() != track_id {
            continue;
        }

        let decoded = decoder.decode(&packet).context("failed to decode packet")?;

        let spec = *decoded.spec();
        let duration = decoded.capacity();

        let mut sample_buf = SampleBuffer::<f32>::new(duration as u64, spec);
        sample_buf.copy_interleaved_ref(decoded);

        samples.extend(sample_buf.samples());
    }

    Ok(DecodedAudio {
        samples,
        sample_rate,
        channels,
    })
}

/// Decode audio from raw bytes
///
/// Tries WAV first (hound), then symphonia formats if feature is enabled.
pub fn decode_audio(data: &[u8]) -> Result<DecodedAudio> {
    // Try WAV first (cheap check)
    if data.len() >= 4 && &data[0..4] == b"RIFF" {
        return decode_wav(data);
    }

    // Try symphonia for other formats
    #[cfg(feature = "symphonia-decode")]
    {
        return decode_audio_symphonia(data);
    }

    #[cfg(not(feature = "symphonia-decode"))]
    {
        Err(anyhow!(
            "unsupported audio format (enable symphonia-decode feature for MP3/FLAC)"
        ))
    }
}

/// Audio file playback node
///
/// Streams pre-decoded audio samples. Call `preload()` before RT playback
/// to ensure audio is ready.
pub struct AudioFileNode {
    descriptor: NodeDescriptor,
    content_hash: String,
    resolver: Arc<dyn ContentResolver>,

    /// Decoded audio (loaded on preload)
    audio: Option<DecodedAudio>,

    /// Current playhead position (in frames, not samples)
    playhead: usize,

    /// Whether to loop at end of file
    looping: bool,

    /// Gain (linear)
    gain: f32,
}

impl AudioFileNode {
    pub fn new(content_hash: impl Into<String>, resolver: Arc<dyn ContentResolver>) -> Self {
        let id = Uuid::new_v4();
        let content_hash = content_hash.into();

        Self {
            descriptor: NodeDescriptor {
                id,
                name: format!("AudioFile:{}", &content_hash[..8.min(content_hash.len())]),
                type_id: "audio_file".to_string(),
                inputs: vec![],
                outputs: vec![Port {
                    name: "out".to_string(),
                    signal_type: SignalType::Audio,
                }],
                latency_samples: 0,
                capabilities: NodeCapabilities {
                    realtime: true,
                    offline: true,
                },
            },
            content_hash,
            resolver,
            audio: None,
            playhead: 0,
            looping: false,
            gain: 1.0,
        }
    }

    /// Pre-load and decode audio (call before RT playback)
    pub fn preload(&mut self) -> Result<()> {
        let data = self.resolver.resolve(&self.content_hash)?;
        let decoded = decode_audio(&data)?;
        tracing::debug!(
            hash = %self.content_hash,
            frames = decoded.frames(),
            sample_rate = decoded.sample_rate,
            channels = decoded.channels,
            "audio preloaded"
        );
        self.audio = Some(decoded);
        Ok(())
    }

    /// Check if audio is loaded
    pub fn is_loaded(&self) -> bool {
        self.audio.is_some()
    }

    /// Get decoded audio info
    pub fn audio_info(&self) -> Option<(u32, u8, usize)> {
        self.audio
            .as_ref()
            .map(|a| (a.sample_rate, a.channels, a.frames()))
    }

    /// Seek to frame position
    pub fn seek(&mut self, frame: usize) {
        self.playhead = frame;
    }

    /// Seek to time in seconds
    pub fn seek_seconds(&mut self, seconds: f64) {
        if let Some(audio) = &self.audio {
            let frame = (seconds * audio.sample_rate as f64) as usize;
            self.playhead = frame.min(audio.frames());
        }
    }

    /// Set looping mode
    pub fn set_looping(&mut self, looping: bool) {
        self.looping = looping;
    }

    /// Set gain (linear, 1.0 = unity)
    pub fn set_gain(&mut self, gain: f32) {
        self.gain = gain;
    }

    /// Get current playhead position in frames
    pub fn playhead(&self) -> usize {
        self.playhead
    }

    /// Check if playback has reached end of file (non-looping)
    pub fn is_finished(&self) -> bool {
        if self.looping {
            return false;
        }
        self.audio
            .as_ref()
            .map(|a| self.playhead >= a.frames())
            .unwrap_or(true)
    }

    /// Get the content hash
    pub fn content_hash(&self) -> &str {
        &self.content_hash
    }
}

impl Node for AudioFileNode {
    fn descriptor(&self) -> &NodeDescriptor {
        &self.descriptor
    }

    fn process(
        &mut self,
        ctx: &ProcessContext,
        _inputs: &[SignalBuffer],
        outputs: &mut [SignalBuffer],
    ) -> Result<(), ProcessError> {
        let audio = self.audio.as_ref().ok_or(ProcessError::Skipped {
            reason: "not loaded",
        })?;

        let output = outputs
            .first_mut()
            .ok_or(ProcessError::Failed {
                reason: "no output buffer".to_string(),
            })?;

        let out_buf = match output {
            SignalBuffer::Audio(buf) => buf,
            _ => {
                return Err(ProcessError::Failed {
                    reason: "expected audio output".to_string(),
                })
            }
        };

        // Clear output first
        out_buf.clear();

        let frames_to_write = ctx.buffer_size;
        let total_frames = audio.frames();
        let out_channels = out_buf.channels as usize;
        let src_channels = audio.channels as usize;

        let mut frame_idx = 0;
        while frame_idx < frames_to_write {
            // Check if we've reached end of file
            if self.playhead >= total_frames {
                if self.looping {
                    self.playhead = 0;
                } else {
                    // Fill rest with silence (already cleared)
                    break;
                }
            }

            // How many frames can we copy in this iteration?
            let frames_available = total_frames - self.playhead;
            let frames_remaining = frames_to_write - frame_idx;
            let frames_to_copy = frames_available.min(frames_remaining);

            // Copy samples with channel conversion
            for f in 0..frames_to_copy {
                let src_frame = self.playhead + f;
                let dst_frame = frame_idx + f;

                for out_ch in 0..out_channels {
                    // Map output channel to source channel
                    let src_ch = if src_channels == 1 {
                        0 // Mono: duplicate to all output channels
                    } else {
                        out_ch % src_channels
                    };

                    let src_idx = src_frame * src_channels + src_ch;
                    let dst_idx = dst_frame * out_channels + out_ch;

                    if src_idx < audio.samples.len() && dst_idx < out_buf.samples.len() {
                        out_buf.samples[dst_idx] = audio.samples[src_idx] * self.gain;
                    }
                }
            }

            self.playhead += frames_to_copy;
            frame_idx += frames_to_copy;
        }

        Ok(())
    }

    fn reset(&mut self) {
        self.playhead = 0;
    }

    fn shutdown(&mut self) {
        self.audio = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives::AudioBuffer;

    /// Generate a simple sine wave WAV file in memory
    fn generate_test_wav(frequency: f32, duration_secs: f32, sample_rate: u32) -> Vec<u8> {
        let num_samples = (sample_rate as f32 * duration_secs) as usize;
        let mut samples = Vec::with_capacity(num_samples);

        for i in 0..num_samples {
            let t = i as f32 / sample_rate as f32;
            let sample = (2.0 * std::f32::consts::PI * frequency * t).sin();
            samples.push(sample);
        }

        let spec = hound::WavSpec {
            channels: 1,
            sample_rate,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };

        let mut cursor = Cursor::new(Vec::new());
        {
            let mut writer = hound::WavWriter::new(&mut cursor, spec).unwrap();
            for sample in samples {
                writer.write_sample(sample).unwrap();
            }
            writer.finalize().unwrap();
        }

        cursor.into_inner()
    }

    /// Generate stereo test WAV
    fn generate_stereo_wav(
        freq_l: f32,
        freq_r: f32,
        duration_secs: f32,
        sample_rate: u32,
    ) -> Vec<u8> {
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
                let sample_l = (2.0 * std::f32::consts::PI * freq_l * t).sin();
                let sample_r = (2.0 * std::f32::consts::PI * freq_r * t).sin();
                writer.write_sample(sample_l).unwrap();
                writer.write_sample(sample_r).unwrap();
            }
            writer.finalize().unwrap();
        }

        cursor.into_inner()
    }

    #[test]
    fn test_decode_wav_mono() {
        let wav_data = generate_test_wav(440.0, 0.1, 48000);
        let decoded = decode_wav(&wav_data).unwrap();

        assert_eq!(decoded.channels, 1);
        assert_eq!(decoded.sample_rate, 48000);
        assert_eq!(decoded.frames(), 4800); // 0.1s * 48000
    }

    #[test]
    fn test_decode_wav_stereo() {
        let wav_data = generate_stereo_wav(440.0, 880.0, 0.1, 48000);
        let decoded = decode_wav(&wav_data).unwrap();

        assert_eq!(decoded.channels, 2);
        assert_eq!(decoded.sample_rate, 48000);
        assert_eq!(decoded.frames(), 4800);
    }

    #[test]
    fn test_decode_audio_detects_wav() {
        let wav_data = generate_test_wav(440.0, 0.1, 48000);
        let decoded = decode_audio(&wav_data).unwrap();

        assert_eq!(decoded.channels, 1);
        assert_eq!(decoded.sample_rate, 48000);
    }

    #[test]
    fn test_memory_resolver() {
        let mut resolver = MemoryResolver::new();
        let wav_data = generate_test_wav(440.0, 0.1, 48000);
        resolver.insert("test_hash", wav_data.clone());

        let retrieved = resolver.resolve("test_hash").unwrap();
        assert_eq!(retrieved, wav_data);

        assert!(resolver.resolve("missing").is_err());
    }

    #[test]
    fn test_audio_file_node_preload() {
        let mut resolver = MemoryResolver::new();
        let wav_data = generate_test_wav(440.0, 0.5, 48000);
        resolver.insert("audio_001", wav_data);

        let mut node = AudioFileNode::new("audio_001", Arc::new(resolver));
        assert!(!node.is_loaded());

        node.preload().unwrap();
        assert!(node.is_loaded());

        let (sample_rate, channels, frames) = node.audio_info().unwrap();
        assert_eq!(sample_rate, 48000);
        assert_eq!(channels, 1);
        assert_eq!(frames, 24000); // 0.5s * 48000
    }

    #[test]
    fn test_audio_file_node_process() {
        use std::sync::Arc;

        let mut resolver = MemoryResolver::new();
        let wav_data = generate_test_wav(440.0, 0.1, 48000);
        resolver.insert("audio_002", wav_data);

        let mut node = AudioFileNode::new("audio_002", Arc::new(resolver));
        node.preload().unwrap();

        let ctx = ProcessContext {
            sample_rate: 48000,
            buffer_size: 1024,
            position_samples: crate::primitives::Sample(0),
            position_beats: crate::primitives::Beat(0.0),
            tempo_map: Arc::new(crate::primitives::TempoMap::new(
                120.0,
                crate::primitives::TimeSignature::default(),
            )),
            mode: crate::primitives::ProcessingMode::Offline,
            transport: crate::primitives::TransportState::Playing,
        };

        let output_buf = AudioBuffer::new(1024, 2);
        let mut outputs = vec![SignalBuffer::Audio(output_buf)];

        // First process call
        node.process(&ctx, &[], &mut outputs).unwrap();
        assert_eq!(node.playhead(), 1024);

        // Check output has non-zero samples (sine wave)
        if let SignalBuffer::Audio(buf) = &outputs[0] {
            let has_nonzero = buf.samples.iter().any(|&s| s.abs() > 0.001);
            assert!(has_nonzero, "output should contain audio data");
        }
    }

    #[test]
    fn test_audio_file_node_looping() {
        let mut resolver = MemoryResolver::new();
        // Very short audio: 100 frames
        let wav_data = generate_test_wav(440.0, 100.0 / 48000.0, 48000);
        resolver.insert("short", wav_data);

        let mut node = AudioFileNode::new("short", Arc::new(resolver));
        node.preload().unwrap();
        node.set_looping(true);

        let ctx = ProcessContext {
            sample_rate: 48000,
            buffer_size: 256,
            position_samples: crate::primitives::Sample(0),
            position_beats: crate::primitives::Beat(0.0),
            tempo_map: Arc::new(crate::primitives::TempoMap::new(
                120.0,
                crate::primitives::TimeSignature::default(),
            )),
            mode: crate::primitives::ProcessingMode::Offline,
            transport: crate::primitives::TransportState::Playing,
        };

        let output_buf = AudioBuffer::new(256, 2);
        let mut outputs = vec![SignalBuffer::Audio(output_buf)];

        // Process more than the audio length
        node.process(&ctx, &[], &mut outputs).unwrap();

        // With looping, should have wrapped around
        // 256 frames requested, 100 available, so playhead should be 256 % 100 = 56
        assert_eq!(node.playhead(), 56);
        assert!(!node.is_finished());
    }

    #[test]
    fn test_audio_file_node_no_loop_finish() {
        let mut resolver = MemoryResolver::new();
        let wav_data = generate_test_wav(440.0, 100.0 / 48000.0, 48000);
        resolver.insert("short2", wav_data);

        let mut node = AudioFileNode::new("short2", Arc::new(resolver));
        node.preload().unwrap();
        node.set_looping(false);

        let ctx = ProcessContext {
            sample_rate: 48000,
            buffer_size: 256,
            position_samples: crate::primitives::Sample(0),
            position_beats: crate::primitives::Beat(0.0),
            tempo_map: Arc::new(crate::primitives::TempoMap::new(
                120.0,
                crate::primitives::TimeSignature::default(),
            )),
            mode: crate::primitives::ProcessingMode::Offline,
            transport: crate::primitives::TransportState::Playing,
        };

        let output_buf = AudioBuffer::new(256, 2);
        let mut outputs = vec![SignalBuffer::Audio(output_buf)];

        // Process more than the audio length (no looping)
        node.process(&ctx, &[], &mut outputs).unwrap();

        // Should stop at end
        assert_eq!(node.playhead(), 100);
        assert!(node.is_finished());

        // Output after 100 frames should be silent
        if let SignalBuffer::Audio(buf) = &outputs[0] {
            // Samples 100*2 to 256*2 should be ~0
            let tail_samples = &buf.samples[200..];
            let max_tail = tail_samples.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
            assert!(max_tail < 0.0001, "tail should be silent");
        }
    }

    #[test]
    fn test_audio_file_node_seek() {
        let mut resolver = MemoryResolver::new();
        let wav_data = generate_test_wav(440.0, 1.0, 48000);
        resolver.insert("long", wav_data);

        let mut node = AudioFileNode::new("long", Arc::new(resolver));
        node.preload().unwrap();

        // Seek by frame
        node.seek(24000);
        assert_eq!(node.playhead(), 24000);

        // Seek by time
        node.seek_seconds(0.25);
        assert_eq!(node.playhead(), 12000); // 0.25 * 48000
    }

    #[test]
    fn test_audio_file_node_gain() {
        let mut resolver = MemoryResolver::new();
        let wav_data = generate_test_wav(440.0, 0.1, 48000);
        resolver.insert("gain_test", wav_data);

        let mut node = AudioFileNode::new("gain_test", Arc::new(resolver));
        node.preload().unwrap();
        node.set_gain(0.5);

        let ctx = ProcessContext {
            sample_rate: 48000,
            buffer_size: 1024,
            position_samples: crate::primitives::Sample(0),
            position_beats: crate::primitives::Beat(0.0),
            tempo_map: Arc::new(crate::primitives::TempoMap::new(
                120.0,
                crate::primitives::TimeSignature::default(),
            )),
            mode: crate::primitives::ProcessingMode::Offline,
            transport: crate::primitives::TransportState::Playing,
        };

        let output_buf = AudioBuffer::new(1024, 2);
        let mut outputs = vec![SignalBuffer::Audio(output_buf)];

        node.process(&ctx, &[], &mut outputs).unwrap();

        // Check max amplitude is ~0.5 (original sine peaks at 1.0)
        if let SignalBuffer::Audio(buf) = &outputs[0] {
            let max_amp = buf.samples.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
            assert!(max_amp < 0.6, "gain should reduce amplitude");
            assert!(max_amp > 0.4, "gain should preserve some signal");
        }
    }

    #[test]
    fn test_audio_file_node_reset() {
        let mut resolver = MemoryResolver::new();
        let wav_data = generate_test_wav(440.0, 0.5, 48000);
        resolver.insert("reset_test", wav_data);

        let mut node = AudioFileNode::new("reset_test", Arc::new(resolver));
        node.preload().unwrap();
        node.seek(10000);

        assert_eq!(node.playhead(), 10000);
        node.reset();
        assert_eq!(node.playhead(), 0);
    }

    #[test]
    fn test_file_cas_client_path_construction() {
        // Can't test actual file read without setup, but test path logic
        let client = FileCasClient::new("/tmp/cas");

        // resolve() will fail since file doesn't exist, but we can verify the path format
        // by checking the error message
        let err = client
            .resolve("abc123")
            .expect_err("should fail for missing file");
        let msg = err.to_string();
        assert!(msg.contains("/tmp/cas/ab/abc123"));
    }

    #[test]
    fn test_decoded_audio_duration() {
        let audio = DecodedAudio {
            samples: vec![0.0; 96000], // 2 seconds at 48kHz mono
            sample_rate: 48000,
            channels: 1,
        };

        assert_eq!(audio.frames(), 96000);
        assert!((audio.duration_seconds() - 2.0).abs() < 0.001);
    }
}
