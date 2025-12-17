//! Stream slicing - extract time ranges from continuous streams.
//!
//! Supports both materialized slices (new WAV/MIDI files) and virtual slices
//! (chunk-reference manifests that can be rendered on demand).

use super::manifest::{ChunkReference, StreamManifest};
use super::types::{AudioFormat, SampleFormat, StreamFormat, StreamUri};
use anyhow::{Context, Result};
use cas::{ContentHash, ContentStore, FileStore};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::ops::Range;
use std::time::SystemTime;
use tracing::{debug, info};

/// Request to slice a time range from a stream
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SliceRequest {
    pub stream_uri: StreamUri,
    pub from: TimeSpec,
    pub to: TimeSpec,
    pub output: SliceOutput,
}

/// Time specification for slice boundaries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TimeSpec {
    /// Absolute wall clock time
    Absolute(SystemTime),
    /// Relative time from now (seconds ago)
    Relative { seconds_ago: f64 },
    /// Absolute sample position in stream
    SamplePosition(u64),
    /// Start of stream
    StreamStart,
    /// Current head position (for live streams)
    StreamHead,
}

/// Output format for slice
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SliceOutput {
    /// Create a new CAS blob (WAV or MIDI file)
    Materialize,
    /// Create a chunk-reference manifest (virtual slice)
    Virtual,
}

/// Result of a slice operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SliceResult {
    /// Content hash of the slice artifact
    pub content_hash: ContentHash,
    /// Sample range that was sliced (None for MIDI)
    pub sample_range: Option<Range<u64>>,
    /// Source chunks that were referenced
    pub source_chunks: Vec<ContentHash>,
    /// MIME type of the result
    pub mime_type: String,
}

/// Virtual slice manifest - references chunks without copying data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VirtualSliceManifest {
    pub source_stream: StreamUri,
    pub source_manifest: ContentHash,
    pub sample_range: Option<Range<u64>>,
    pub byte_range: Range<u64>,
    pub chunks: Vec<ChunkSlice>,
    pub created_at: SystemTime,
}

/// Reference to a portion of a chunk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkSlice {
    pub chunk_hash: ContentHash,
    pub byte_offset: u64,
    pub byte_length: u64,
    pub sample_offset: Option<u64>,
    pub sample_length: Option<u64>,
}

/// Engine for slicing streams
pub struct SlicingEngine {
    cas: FileStore,
}

impl SlicingEngine {
    pub fn new(cas: FileStore) -> Self {
        Self { cas }
    }

    /// Slice a stream based on a request
    pub fn slice(&self, request: SliceRequest, manifest: &StreamManifest) -> Result<SliceResult> {
        // Resolve time specs to sample positions
        let sample_range = self.resolve_sample_range(&request.from, &request.to, manifest)?;

        match request.output {
            SliceOutput::Materialize => self.materialize_slice(manifest, sample_range),
            SliceOutput::Virtual => self.create_virtual_slice(manifest, sample_range),
        }
    }

    /// Resolve TimeSpec values to actual sample positions
    fn resolve_sample_range(
        &self,
        from: &TimeSpec,
        to: &TimeSpec,
        manifest: &StreamManifest,
    ) -> Result<Option<Range<u64>>> {
        let total_samples = manifest.total_samples;

        let from_sample = match from {
            TimeSpec::SamplePosition(pos) => Some(*pos),
            TimeSpec::StreamStart => Some(0),
            TimeSpec::StreamHead => total_samples,
            TimeSpec::Absolute(_) => {
                anyhow::bail!("absolute time not yet supported - need clock correlation")
            }
            TimeSpec::Relative { seconds_ago } => {
                // Calculate sample offset from current head
                if let Some(total) = total_samples {
                    let format = match &manifest.chunks.first() {
                        Some(_) => self.get_audio_format(manifest)?,
                        None => anyhow::bail!("no chunks in manifest"),
                    };

                    if let Some(audio_format) = format {
                        let samples_ago = (*seconds_ago * audio_format.sample_rate as f64) as u64;
                        Some(total.saturating_sub(samples_ago))
                    } else {
                        anyhow::bail!("relative time not supported for MIDI streams")
                    }
                } else {
                    anyhow::bail!("cannot use relative time on MIDI stream")
                }
            }
        };

        let to_sample = match to {
            TimeSpec::SamplePosition(pos) => Some(*pos),
            TimeSpec::StreamStart => Some(0),
            TimeSpec::StreamHead => total_samples,
            TimeSpec::Absolute(_) => {
                anyhow::bail!("absolute time not yet supported - need clock correlation")
            }
            TimeSpec::Relative { seconds_ago } => {
                if let Some(total) = total_samples {
                    let format = self.get_audio_format(manifest)?;
                    if let Some(audio_format) = format {
                        let samples_ago = (*seconds_ago * audio_format.sample_rate as f64) as u64;
                        Some(total.saturating_sub(samples_ago))
                    } else {
                        anyhow::bail!("relative time not supported for MIDI streams")
                    }
                } else {
                    anyhow::bail!("cannot use relative time on MIDI stream")
                }
            }
        };

        match (from_sample, to_sample) {
            (Some(f), Some(t)) => {
                if f >= t {
                    anyhow::bail!("invalid range: from ({}) >= to ({})", f, t);
                }
                Ok(Some(f..t))
            }
            _ => Ok(None),
        }
    }

    /// Get audio format from manifest (by loading definition from CAS)
    fn get_audio_format(&self, manifest: &StreamManifest) -> Result<Option<AudioFormat>> {
        let def_bytes = self
            .cas
            .retrieve(&manifest.definition_hash)
            .context("failed to load stream definition")?
            .context("stream definition not found in CAS")?;

        let definition: crate::streams::types::StreamDefinition =
            serde_json::from_slice(&def_bytes).context("failed to parse stream definition")?;

        match definition.format {
            StreamFormat::Audio(format) => Ok(Some(format)),
            StreamFormat::Midi => Ok(None),
        }
    }

    /// Materialize a slice by copying chunk data to a new CAS blob
    fn materialize_slice(
        &self,
        manifest: &StreamManifest,
        sample_range: Option<Range<u64>>,
    ) -> Result<SliceResult> {
        let audio_format = self
            .get_audio_format(manifest)?
            .context("materialization only supported for audio streams")?;

        let sample_range = sample_range.context("sample range required for audio slicing")?;

        let bytes_per_sample =
            audio_format.sample_format.bytes_per_sample() * audio_format.channels as usize;

        // Find chunks that intersect with the sample range
        let chunk_slices = self.compute_chunk_slices(manifest, &sample_range, bytes_per_sample)?;

        if chunk_slices.is_empty() {
            anyhow::bail!("no chunks found in range {:?}", sample_range);
        }

        // Create a temporary file to write the WAV data
        let mut temp_file =
            tempfile::NamedTempFile::new().context("failed to create temp file")?;

        // Write WAV header
        let sample_count = (sample_range.end - sample_range.start) as u32;
        self.write_wav_header(&mut temp_file, &audio_format, sample_count)?;

        // Copy chunk data
        let mut source_chunks = Vec::new();
        for chunk_slice in &chunk_slices {
            source_chunks.push(chunk_slice.chunk_hash.clone());

            let chunk_data = self
                .cas
                .retrieve(&chunk_slice.chunk_hash)
                .with_context(|| format!("failed to load chunk {}", chunk_slice.chunk_hash))?
                .with_context(|| format!("chunk {} not found in CAS", chunk_slice.chunk_hash))?;

            let start = chunk_slice.byte_offset as usize;
            let end = start + chunk_slice.byte_length as usize;

            if end > chunk_data.len() {
                anyhow::bail!(
                    "chunk slice out of bounds: {}..{} (chunk size: {})",
                    start,
                    end,
                    chunk_data.len()
                );
            }

            temp_file
                .write_all(&chunk_data[start..end])
                .context("failed to write chunk data")?;
        }

        // Finalize and store
        temp_file.flush().context("failed to flush temp file")?;
        let (file, temp_path) = temp_file.into_parts();
        drop(file); // Close the file so we can read it

        let wav_data = std::fs::read(&temp_path).context("failed to read temp file")?;
        let content_hash = self
            .cas
            .store(&wav_data, "audio/wav")
            .context("failed to store materialized slice")?;

        info!(
            "materialized slice from {}: samples {:?}, {} source chunks",
            manifest.stream_uri,
            sample_range,
            source_chunks.len()
        );

        Ok(SliceResult {
            content_hash,
            sample_range: Some(sample_range),
            source_chunks,
            mime_type: "audio/wav".to_string(),
        })
    }

    /// Create a virtual slice (chunk-reference manifest)
    fn create_virtual_slice(
        &self,
        manifest: &StreamManifest,
        sample_range: Option<Range<u64>>,
    ) -> Result<SliceResult> {
        let audio_format = self.get_audio_format(manifest)?;

        let bytes_per_sample = if let Some(ref format) = audio_format {
            format.sample_format.bytes_per_sample() * format.channels as usize
        } else {
            anyhow::bail!("virtual slicing not yet implemented for MIDI")
        };

        let sample_range = sample_range.context("sample range required")?;

        // Compute which chunks are needed
        let chunk_slices = self.compute_chunk_slices(manifest, &sample_range, bytes_per_sample)?;

        if chunk_slices.is_empty() {
            anyhow::bail!("no chunks found in range {:?}", sample_range);
        }

        let source_chunks: Vec<_> = chunk_slices
            .iter()
            .map(|cs| cs.chunk_hash.clone())
            .collect();

        // Create virtual slice manifest
        let byte_range = {
            let first = chunk_slices.first().unwrap();
            let last = chunk_slices.last().unwrap();
            first.byte_offset..(last.byte_offset + last.byte_length)
        };

        let virtual_manifest = VirtualSliceManifest {
            source_stream: manifest.stream_uri.clone(),
            source_manifest: manifest.definition_hash.clone(),
            sample_range: Some(sample_range.clone()),
            byte_range,
            chunks: chunk_slices,
            created_at: SystemTime::now(),
        };

        // Serialize and store
        let manifest_json = serde_json::to_vec(&virtual_manifest)
            .context("failed to serialize virtual slice manifest")?;

        let content_hash = self
            .cas
            .store(&manifest_json, "application/x-hootenanny-virtual-slice")
            .context("failed to store virtual slice manifest")?;

        info!(
            "created virtual slice from {}: samples {:?}, {} chunks",
            manifest.stream_uri,
            sample_range,
            source_chunks.len()
        );

        Ok(SliceResult {
            content_hash,
            sample_range: Some(sample_range),
            source_chunks,
            mime_type: "application/x-hootenanny-virtual-slice".to_string(),
        })
    }

    /// Compute which chunks and byte ranges are needed for a sample range
    fn compute_chunk_slices(
        &self,
        manifest: &StreamManifest,
        sample_range: &Range<u64>,
        bytes_per_sample: usize,
    ) -> Result<Vec<ChunkSlice>> {
        let mut chunk_slices = Vec::new();
        let mut current_sample = 0u64;

        for chunk_ref in &manifest.chunks {
            // Only process sealed chunks for now
            let (chunk_hash, chunk_samples) = match chunk_ref {
                ChunkReference::Sealed {
                    hash,
                    sample_count,
                    ..
                } => (
                    hash.clone(),
                    sample_count.context("audio chunk missing sample count")?,
                ),
                ChunkReference::Staging { .. } => {
                    // Skip staging chunks for now - they're still being written
                    debug!("skipping staging chunk during slicing");
                    continue;
                }
            };

            let chunk_end_sample = current_sample + chunk_samples;

            // Check if this chunk intersects with our desired range
            if chunk_end_sample > sample_range.start && current_sample < sample_range.end {
                // Calculate the slice within this chunk
                let slice_start_sample = sample_range.start.max(current_sample) - current_sample;
                let slice_end_sample = sample_range.end.min(chunk_end_sample) - current_sample;

                let byte_offset = slice_start_sample * bytes_per_sample as u64;
                let byte_length =
                    (slice_end_sample - slice_start_sample) * bytes_per_sample as u64;

                chunk_slices.push(ChunkSlice {
                    chunk_hash,
                    byte_offset,
                    byte_length,
                    sample_offset: Some(slice_start_sample),
                    sample_length: Some(slice_end_sample - slice_start_sample),
                });
            }

            current_sample = chunk_end_sample;

            // Early exit if we've gone past the range
            if current_sample >= sample_range.end {
                break;
            }
        }

        Ok(chunk_slices)
    }

    /// Write a WAV file header
    fn write_wav_header(
        &self,
        writer: &mut impl Write,
        format: &AudioFormat,
        sample_count: u32,
    ) -> Result<()> {
        let bytes_per_sample = format.sample_format.bytes_per_sample() as u16;
        let num_channels = format.channels as u16;
        let byte_rate = format.sample_rate * num_channels as u32 * bytes_per_sample as u32;
        let block_align = num_channels * bytes_per_sample;
        let bits_per_sample = bytes_per_sample * 8;

        let data_size = sample_count * block_align as u32;
        let file_size = 36 + data_size;

        // RIFF header
        writer.write_all(b"RIFF")?;
        writer.write_all(&file_size.to_le_bytes())?;
        writer.write_all(b"WAVE")?;

        // fmt chunk
        writer.write_all(b"fmt ")?;
        writer.write_all(&16u32.to_le_bytes())?; // chunk size
        writer.write_all(&1u16.to_le_bytes())?; // audio format (1 = PCM)
        writer.write_all(&num_channels.to_le_bytes())?;
        writer.write_all(&format.sample_rate.to_le_bytes())?;
        writer.write_all(&byte_rate.to_le_bytes())?;
        writer.write_all(&block_align.to_le_bytes())?;
        writer.write_all(&bits_per_sample.to_le_bytes())?;

        // data chunk header
        writer.write_all(b"data")?;
        writer.write_all(&data_size.to_le_bytes())?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::types::StreamDefinition;
    use tempfile::TempDir;

    fn setup_test_store() -> (TempDir, FileStore) {
        let temp_dir = TempDir::new().unwrap();
        let store = FileStore::at_path(temp_dir.path()).unwrap();
        (temp_dir, store)
    }

    fn create_test_manifest(
        store: &FileStore,
        sample_rate: u32,
        samples_per_chunk: u64,
        num_chunks: usize,
    ) -> (StreamManifest, Vec<ContentHash>) {
        let uri = StreamUri::from("stream://test/audio");

        let audio_format = AudioFormat {
            sample_rate,
            channels: 1,
            sample_format: SampleFormat::F32,
        };

        let definition = StreamDefinition {
            uri: uri.clone(),
            device_identity: "test-device".to_string(),
            format: StreamFormat::Audio(audio_format.clone()),
            chunk_size_bytes: samples_per_chunk * 4,
        };

        let def_json = serde_json::to_vec(&definition).unwrap();
        let def_hash = store.store(&def_json, "application/json").unwrap();

        let mut manifest = StreamManifest::new(uri, def_hash);
        let mut chunk_hashes = Vec::new();

        for i in 0..num_chunks {
            // Create dummy audio data
            let samples: Vec<f32> = (0..samples_per_chunk)
                .map(|s| i as f32 + s as f32 / samples_per_chunk as f32)
                .collect();

            let bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();

            let chunk_hash = store.store(&bytes, "audio/raw").unwrap();
            chunk_hashes.push(chunk_hash.clone());

            manifest.add_chunk(ChunkReference::Sealed {
                hash: chunk_hash,
                byte_count: bytes.len() as u64,
                sample_count: Some(samples_per_chunk),
            });
        }

        (manifest, chunk_hashes)
    }

    #[test]
    fn test_slice_full_stream() {
        let (_temp, store) = setup_test_store();
        let engine = SlicingEngine::new(store.clone());

        let (manifest, _) = create_test_manifest(&store, 48000, 1000, 3);

        let request = SliceRequest {
            stream_uri: manifest.stream_uri.clone(),
            from: TimeSpec::StreamStart,
            to: TimeSpec::SamplePosition(3000),
            output: SliceOutput::Materialize,
        };

        let result = engine.slice(request, &manifest).unwrap();

        assert_eq!(result.sample_range, Some(0..3000));
        assert_eq!(result.source_chunks.len(), 3);
        assert_eq!(result.mime_type, "audio/wav");

        // Verify the slice was stored
        let slice_data = store
            .retrieve(&result.content_hash)
            .unwrap()
            .expect("slice not found");
        assert!(!slice_data.is_empty());
    }

    #[test]
    fn test_slice_middle_range() {
        let (_temp, store) = setup_test_store();
        let engine = SlicingEngine::new(store.clone());

        let (manifest, _) = create_test_manifest(&store, 48000, 1000, 5);

        let request = SliceRequest {
            stream_uri: manifest.stream_uri.clone(),
            from: TimeSpec::SamplePosition(1500),
            to: TimeSpec::SamplePosition(3500),
            output: SliceOutput::Materialize,
        };

        let result = engine.slice(request, &manifest).unwrap();

        assert_eq!(result.sample_range, Some(1500..3500));
        // Should span chunks 1, 2, and 3
        assert_eq!(result.source_chunks.len(), 3);
    }

    #[test]
    fn test_slice_single_chunk() {
        let (_temp, store) = setup_test_store();
        let engine = SlicingEngine::new(store.clone());

        let (manifest, _) = create_test_manifest(&store, 48000, 1000, 3);

        let request = SliceRequest {
            stream_uri: manifest.stream_uri.clone(),
            from: TimeSpec::SamplePosition(100),
            to: TimeSpec::SamplePosition(900),
            output: SliceOutput::Materialize,
        };

        let result = engine.slice(request, &manifest).unwrap();

        assert_eq!(result.sample_range, Some(100..900));
        assert_eq!(result.source_chunks.len(), 1);
    }

    #[test]
    fn test_virtual_slice() {
        let (_temp, store) = setup_test_store();
        let engine = SlicingEngine::new(store.clone());

        let (manifest, _) = create_test_manifest(&store, 48000, 1000, 3);

        let request = SliceRequest {
            stream_uri: manifest.stream_uri.clone(),
            from: TimeSpec::SamplePosition(500),
            to: TimeSpec::SamplePosition(2500),
            output: SliceOutput::Virtual,
        };

        let result = engine.slice(request, &manifest).unwrap();

        assert_eq!(result.sample_range, Some(500..2500));
        assert_eq!(result.source_chunks.len(), 3);
        assert_eq!(
            result.mime_type,
            "application/x-hootenanny-virtual-slice"
        );

        // Load and verify the virtual manifest
        let manifest_data = store
            .retrieve(&result.content_hash)
            .unwrap()
            .expect("virtual manifest not found");
        let virtual_manifest: VirtualSliceManifest =
            serde_json::from_slice(&manifest_data).unwrap();

        assert_eq!(virtual_manifest.chunks.len(), 3);
        assert_eq!(virtual_manifest.sample_range, Some(500..2500));
    }

    #[test]
    fn test_relative_time_spec() {
        let (_temp, store) = setup_test_store();
        let engine = SlicingEngine::new(store.clone());

        // 48000 samples/sec, 1000 samples/chunk, 5 chunks = 5000 samples total
        // 5000 samples / 48000 = 0.104 seconds
        let (manifest, _) = create_test_manifest(&store, 48000, 1000, 5);

        // Slice last 0.02 seconds (960 samples at 48kHz)
        let request = SliceRequest {
            stream_uri: manifest.stream_uri.clone(),
            from: TimeSpec::Relative { seconds_ago: 0.02 },
            to: TimeSpec::StreamHead,
            output: SliceOutput::Materialize,
        };

        let result = engine.slice(request, &manifest).unwrap();

        // Should be approximately 4040..5000 (960 samples)
        assert_eq!(result.sample_range, Some(4040..5000));
    }

    #[test]
    fn test_invalid_range() {
        let (_temp, store) = setup_test_store();
        let engine = SlicingEngine::new(store.clone());

        let (manifest, _) = create_test_manifest(&store, 48000, 1000, 3);

        let request = SliceRequest {
            stream_uri: manifest.stream_uri.clone(),
            from: TimeSpec::SamplePosition(2000),
            to: TimeSpec::SamplePosition(1000),
            output: SliceOutput::Materialize,
        };

        let result = engine.slice(request, &manifest);
        assert!(result.is_err());
    }
}
