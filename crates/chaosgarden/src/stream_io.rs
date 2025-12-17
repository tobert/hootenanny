//! Stream I/O - Real-time capture to mmap'd chunks
//!
//! This module handles the RT-safe I/O path for stream capture:
//! - Opens and mmaps staging files provided by hootenanny
//! - Writes audio/MIDI samples to mmap'd regions
//! - Sends notifications via ZMQ when chunks are full
//! - Never blocks, never allocates in the hot path
//!
//! ## Ownership Model
//!
//! - **Hootenanny**: Creates staging files, manages lifecycle, handles sealing
//! - **Chaosgarden**: Only does I/O - open, mmap, write, close
//!
//! ## Message Flow
//!
//! ```text
//! hootenanny → StreamStart{uri, definition, chunk_path}
//!            → chaosgarden opens/mmaps file
//! chaosgarden → StreamHeadPosition (periodic updates)
//! chaosgarden → StreamChunkFull when chunk reaches target size
//! hootenanny → StreamSwitchChunk{uri, new_chunk_path}
//!            → chaosgarden closes old, opens new
//! hootenanny → StreamStop{uri}
//!            → chaosgarden closes file, stops stream
//! ```

use anyhow::{Context, Result};
use memmap2::{MmapMut, MmapOptions};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::SystemTime;
use tracing::{debug, info};

/// URI identifying a stream
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StreamUri(pub String);

impl StreamUri {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for StreamUri {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for StreamUri {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// Stream format specification
#[derive(Debug, Clone)]
pub enum StreamFormat {
    Audio {
        sample_rate: u32,
        channels: u8,
        sample_format: SampleFormat,
    },
    Midi,
}

/// Sample format for audio streams
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleFormat {
    F32,
    I16,
    I24,
}

impl SampleFormat {
    /// Size of one sample in bytes
    pub fn bytes_per_sample(self) -> usize {
        match self {
            SampleFormat::F32 => 4,
            SampleFormat::I16 => 2,
            SampleFormat::I24 => 3,
        }
    }
}

/// Static definition of a stream (provided by hootenanny)
#[derive(Debug, Clone)]
pub struct StreamDefinition {
    pub uri: StreamUri,
    pub device_identity: String,
    pub format: StreamFormat,
    pub chunk_size_bytes: u64,
}

/// Handle to an active chunk being written
struct ChunkHandle {
    path: PathBuf,
    file: File,
    mmap: MmapMut,
    bytes_written: u64,
    samples_written: u64,
}

impl ChunkHandle {
    fn open(path: impl AsRef<Path>, chunk_size_bytes: u64) -> Result<Self> {
        let path = path.as_ref().to_path_buf();

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .with_context(|| format!("failed to open chunk file: {}", path.display()))?;

        // Set file size (hootenanny should pre-allocate, but ensure it's correct)
        file.set_len(chunk_size_bytes)
            .context("failed to set chunk file size")?;

        // Create mmap
        let mmap = unsafe {
            MmapOptions::new()
                .len(chunk_size_bytes as usize)
                .map_mut(&file)
                .context("failed to mmap chunk file")?
        };

        Ok(Self {
            path,
            file,
            mmap,
            bytes_written: 0,
            samples_written: 0,
        })
    }

    fn write_samples(&mut self, data: &[u8]) -> Result<usize> {
        let remaining = self.mmap.len() - self.bytes_written as usize;
        let to_write = data.len().min(remaining);

        if to_write == 0 {
            return Ok(0);
        }

        let offset = self.bytes_written as usize;
        self.mmap[offset..offset + to_write].copy_from_slice(&data[..to_write]);

        self.bytes_written += to_write as u64;
        Ok(to_write)
    }

    fn is_full(&self) -> bool {
        self.bytes_written >= self.mmap.len() as u64
    }

    fn flush(&mut self) -> Result<()> {
        self.mmap.flush().context("failed to flush mmap")
    }
}

/// Active stream state
struct StreamHandle {
    definition: StreamDefinition,
    current_chunk: Option<ChunkHandle>,
    total_samples: u64,
    total_bytes: u64,
    started_at: SystemTime,
}

impl StreamHandle {
    fn new(definition: StreamDefinition) -> Self {
        Self {
            definition,
            current_chunk: None,
            total_samples: 0,
            total_bytes: 0,
            started_at: SystemTime::now(),
        }
    }

    fn open_chunk(&mut self, path: impl AsRef<Path>) -> Result<()> {
        let chunk = ChunkHandle::open(path, self.definition.chunk_size_bytes)?;
        self.current_chunk = Some(chunk);
        Ok(())
    }

    fn close_chunk(&mut self) -> Result<()> {
        if let Some(mut chunk) = self.current_chunk.take() {
            chunk.flush()?;
            debug!(
                "closed chunk {} ({} bytes, {} samples)",
                chunk.path.display(),
                chunk.bytes_written,
                chunk.samples_written
            );
        }
        Ok(())
    }

    fn write_samples(&mut self, data: &[u8], sample_count: u64) -> Result<usize> {
        let chunk = self
            .current_chunk
            .as_mut()
            .context("no active chunk for writing")?;

        let written = chunk.write_samples(data)?;

        if written > 0 {
            self.total_bytes += written as u64;

            // For audio, we know the sample count from the data size
            // For MIDI, sample_count will be 0
            let samples = if sample_count > 0 {
                sample_count
            } else {
                match &self.definition.format {
                    StreamFormat::Audio {
                        channels,
                        sample_format,
                        ..
                    } => {
                        let bytes_per_frame =
                            (*channels as usize) * sample_format.bytes_per_sample();
                        (written / bytes_per_frame) as u64
                    }
                    StreamFormat::Midi => 0,
                }
            };

            chunk.samples_written += samples;
            self.total_samples += samples;
        }

        Ok(written)
    }

    fn is_chunk_full(&self) -> bool {
        self.current_chunk
            .as_ref()
            .map(|c| c.is_full())
            .unwrap_or(false)
    }

    fn chunk_info(&self) -> Option<ChunkInfo> {
        self.current_chunk.as_ref().map(|c| ChunkInfo {
            path: c.path.clone(),
            bytes_written: c.bytes_written,
            samples_written: c.samples_written,
        })
    }
}

/// Information about a chunk (for notifications)
#[derive(Debug, Clone)]
pub struct ChunkInfo {
    pub path: PathBuf,
    pub bytes_written: u64,
    pub samples_written: u64,
}

/// Manager for all active streams
pub struct StreamManager {
    streams: Arc<RwLock<HashMap<StreamUri, StreamHandle>>>,
}

impl StreamManager {
    pub fn new() -> Self {
        Self {
            streams: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Start a new stream with the given definition and initial chunk path
    pub fn start_stream(
        &self,
        definition: StreamDefinition,
        chunk_path: impl AsRef<Path>,
    ) -> Result<()> {
        let uri = definition.uri.clone();

        let mut streams = self.streams.write().unwrap();

        if streams.contains_key(&uri) {
            anyhow::bail!("stream already exists: {}", uri.as_str());
        }

        let mut handle = StreamHandle::new(definition);
        handle
            .open_chunk(chunk_path)
            .with_context(|| format!("failed to open initial chunk for stream {}", uri.as_str()))?;

        info!("started stream: {}", uri.as_str());
        streams.insert(uri, handle);

        Ok(())
    }

    /// Switch to a new chunk for the given stream
    pub fn switch_chunk(&self, uri: &StreamUri, new_chunk_path: impl AsRef<Path>) -> Result<()> {
        let mut streams = self.streams.write().unwrap();

        let handle = streams
            .get_mut(uri)
            .with_context(|| format!("stream not found: {}", uri.as_str()))?;

        handle.close_chunk()?;
        handle
            .open_chunk(new_chunk_path)
            .with_context(|| format!("failed to open new chunk for stream {}", uri.as_str()))?;

        info!("switched chunk for stream: {}", uri.as_str());

        Ok(())
    }

    /// Stop a stream and close its current chunk
    pub fn stop_stream(&self, uri: &StreamUri) -> Result<()> {
        let mut streams = self.streams.write().unwrap();

        let mut handle = streams
            .remove(uri)
            .with_context(|| format!("stream not found: {}", uri.as_str()))?;

        handle.close_chunk()?;
        info!("stopped stream: {}", uri.as_str());

        Ok(())
    }

    /// Write samples to a stream
    ///
    /// Returns the number of bytes written and whether the chunk is now full.
    pub fn write_samples(
        &self,
        uri: &StreamUri,
        data: &[u8],
        sample_count: u64,
    ) -> Result<(usize, bool)> {
        let mut streams = self.streams.write().unwrap();

        let handle = streams
            .get_mut(uri)
            .with_context(|| format!("stream not found: {}", uri.as_str()))?;

        let written = handle.write_samples(data, sample_count)?;
        let is_full = handle.is_chunk_full();

        Ok((written, is_full))
    }

    /// Get current head position for a stream
    pub fn head_position(&self, uri: &StreamUri) -> Result<HeadPosition> {
        let streams = self.streams.read().unwrap();

        let handle = streams
            .get(uri)
            .with_context(|| format!("stream not found: {}", uri.as_str()))?;

        let chunk = handle.current_chunk.as_ref();

        Ok(HeadPosition {
            sample_position: handle.total_samples,
            byte_position: handle.total_bytes,
            chunk_bytes_written: chunk.map(|c| c.bytes_written).unwrap_or(0),
            chunk_samples_written: chunk.map(|c| c.samples_written).unwrap_or(0),
        })
    }

    /// Get info about the current chunk for a stream
    pub fn chunk_info(&self, uri: &StreamUri) -> Result<Option<ChunkInfo>> {
        let streams = self.streams.read().unwrap();

        let handle = streams
            .get(uri)
            .with_context(|| format!("stream not found: {}", uri.as_str()))?;

        Ok(handle.chunk_info())
    }

    /// Get list of active stream URIs
    pub fn active_streams(&self) -> Vec<StreamUri> {
        let streams = self.streams.read().unwrap();
        streams.keys().cloned().collect()
    }

    /// Check if a stream's chunk is full
    pub fn is_chunk_full(&self, uri: &StreamUri) -> Result<bool> {
        let streams = self.streams.read().unwrap();

        let handle = streams
            .get(uri)
            .with_context(|| format!("stream not found: {}", uri.as_str()))?;

        Ok(handle.is_chunk_full())
    }
}

impl Default for StreamManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Current head position in a stream
#[derive(Debug, Clone)]
pub struct HeadPosition {
    pub sample_position: u64,
    pub byte_position: u64,
    pub chunk_bytes_written: u64,
    pub chunk_samples_written: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_chunk(dir: &Path, size: u64) -> PathBuf {
        let path = dir.join("test_chunk.dat");
        let file = File::create(&path).unwrap();
        file.set_len(size).unwrap();
        path
    }

    #[test]
    fn test_chunk_handle_open() {
        let temp_dir = TempDir::new().unwrap();
        let chunk_path = create_test_chunk(temp_dir.path(), 1024);

        let chunk = ChunkHandle::open(&chunk_path, 1024).unwrap();
        assert_eq!(chunk.bytes_written, 0);
        assert_eq!(chunk.samples_written, 0);
        assert!(!chunk.is_full());
    }

    #[test]
    fn test_chunk_handle_write() {
        let temp_dir = TempDir::new().unwrap();
        let chunk_path = create_test_chunk(temp_dir.path(), 1024);

        let mut chunk = ChunkHandle::open(&chunk_path, 1024).unwrap();

        let data = vec![42u8; 512];
        let written = chunk.write_samples(&data).unwrap();
        assert_eq!(written, 512);
        assert_eq!(chunk.bytes_written, 512);
        assert!(!chunk.is_full());

        // Write more to fill
        let written = chunk.write_samples(&data).unwrap();
        assert_eq!(written, 512);
        assert_eq!(chunk.bytes_written, 1024);
        assert!(chunk.is_full());

        // Try to write more - should write 0 bytes
        let written = chunk.write_samples(&data).unwrap();
        assert_eq!(written, 0);
    }

    #[test]
    fn test_stream_manager_start_stop() {
        let temp_dir = TempDir::new().unwrap();
        let chunk_path = create_test_chunk(temp_dir.path(), 1024);

        let manager = StreamManager::new();
        let uri = StreamUri::from("stream://test/audio");

        let definition = StreamDefinition {
            uri: uri.clone(),
            device_identity: "test-device".to_string(),
            format: StreamFormat::Audio {
                sample_rate: 48000,
                channels: 1,
                sample_format: SampleFormat::F32,
            },
            chunk_size_bytes: 1024,
        };

        manager.start_stream(definition, &chunk_path).unwrap();

        let active = manager.active_streams();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0], uri);

        manager.stop_stream(&uri).unwrap();

        let active = manager.active_streams();
        assert_eq!(active.len(), 0);
    }

    #[test]
    fn test_stream_manager_write() {
        let temp_dir = TempDir::new().unwrap();
        let chunk_path = create_test_chunk(temp_dir.path(), 1024);

        let manager = StreamManager::new();
        let uri = StreamUri::from("stream://test/audio");

        let definition = StreamDefinition {
            uri: uri.clone(),
            device_identity: "test-device".to_string(),
            format: StreamFormat::Audio {
                sample_rate: 48000,
                channels: 1,
                sample_format: SampleFormat::F32,
            },
            chunk_size_bytes: 1024,
        };

        manager.start_stream(definition, &chunk_path).unwrap();

        // Write some audio data (256 samples of F32 = 1024 bytes)
        let data = vec![0u8; 512];
        let (written, is_full) = manager.write_samples(&uri, &data, 128).unwrap();
        assert_eq!(written, 512);
        assert!(!is_full);

        let pos = manager.head_position(&uri).unwrap();
        assert_eq!(pos.byte_position, 512);
        assert_eq!(pos.sample_position, 128);

        // Write more to fill
        let (written, is_full) = manager.write_samples(&uri, &data, 128).unwrap();
        assert_eq!(written, 512);
        assert!(is_full);

        let pos = manager.head_position(&uri).unwrap();
        assert_eq!(pos.byte_position, 1024);
        assert_eq!(pos.sample_position, 256);

        manager.stop_stream(&uri).unwrap();
    }

    #[test]
    fn test_stream_manager_switch_chunk() {
        let temp_dir = TempDir::new().unwrap();
        let chunk_path1 = create_test_chunk(temp_dir.path(), 1024);
        let chunk_path2 = temp_dir.path().join("chunk2.dat");
        fs::write(&chunk_path2, vec![0u8; 1024]).unwrap();

        let manager = StreamManager::new();
        let uri = StreamUri::from("stream://test/audio");

        let definition = StreamDefinition {
            uri: uri.clone(),
            device_identity: "test-device".to_string(),
            format: StreamFormat::Audio {
                sample_rate: 48000,
                channels: 1,
                sample_format: SampleFormat::F32,
            },
            chunk_size_bytes: 1024,
        };

        manager.start_stream(definition, &chunk_path1).unwrap();

        // Write to first chunk
        let data = vec![0u8; 512];
        manager.write_samples(&uri, &data, 128).unwrap();

        let pos_before = manager.head_position(&uri).unwrap();
        assert_eq!(pos_before.byte_position, 512);

        // Switch to second chunk
        manager.switch_chunk(&uri, &chunk_path2).unwrap();

        // Total position should be preserved, but chunk bytes reset
        let pos_after = manager.head_position(&uri).unwrap();
        assert_eq!(pos_after.byte_position, 512); // total unchanged
        assert_eq!(pos_after.chunk_bytes_written, 0); // new chunk

        // Write to second chunk
        manager.write_samples(&uri, &data, 128).unwrap();

        let pos_final = manager.head_position(&uri).unwrap();
        assert_eq!(pos_final.byte_position, 1024); // total continues
        assert_eq!(pos_final.chunk_bytes_written, 512); // new chunk has data

        manager.stop_stream(&uri).unwrap();
    }

    #[test]
    fn test_stream_manager_double_start_fails() {
        let temp_dir = TempDir::new().unwrap();
        let chunk_path = create_test_chunk(temp_dir.path(), 1024);

        let manager = StreamManager::new();
        let uri = StreamUri::from("stream://test/audio");

        let definition = StreamDefinition {
            uri: uri.clone(),
            device_identity: "test-device".to_string(),
            format: StreamFormat::Audio {
                sample_rate: 48000,
                channels: 1,
                sample_format: SampleFormat::F32,
            },
            chunk_size_bytes: 1024,
        };

        manager.start_stream(definition.clone(), &chunk_path).unwrap();

        // Try to start again - should fail
        let result = manager.start_stream(definition, &chunk_path);
        assert!(result.is_err());

        manager.stop_stream(&uri).unwrap();
    }
}
