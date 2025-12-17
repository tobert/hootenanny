//! Stream manifest - tracks chunks and metadata for an active or completed stream.

use super::types::StreamUri;
use anyhow::Result;
use cas::{ContentHash, StagingId};
use serde::{Deserialize, Serialize};
use std::time::SystemTime;

/// Reference to a chunk (either sealed in CAS or still staging)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChunkReference {
    /// Chunk has been sealed and is immutable in CAS
    Sealed {
        hash: ContentHash,
        byte_count: u64,
        sample_count: Option<u64>,
    },
    /// Chunk is still being written in staging area
    Staging {
        id: StagingId,
        bytes_written: u64,
        samples_written: Option<u64>,
    },
}

impl ChunkReference {
    pub fn byte_count(&self) -> u64 {
        match self {
            ChunkReference::Sealed { byte_count, .. } => *byte_count,
            ChunkReference::Staging { bytes_written, .. } => *bytes_written,
        }
    }

    pub fn sample_count(&self) -> Option<u64> {
        match self {
            ChunkReference::Sealed { sample_count, .. } => *sample_count,
            ChunkReference::Staging {
                samples_written, ..
            } => *samples_written,
        }
    }

    pub fn is_sealed(&self) -> bool {
        matches!(self, ChunkReference::Sealed { .. })
    }
}

/// Manifest for a stream - tracks all chunks and metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamManifest {
    pub stream_uri: StreamUri,
    pub definition_hash: ContentHash,
    pub chunks: Vec<ChunkReference>,
    pub total_bytes: u64,
    pub total_samples: Option<u64>,
    pub started_at: SystemTime,
    pub last_updated: SystemTime,
}

impl StreamManifest {
    pub fn new(stream_uri: StreamUri, definition_hash: ContentHash) -> Self {
        let now = SystemTime::now();
        Self {
            stream_uri,
            definition_hash,
            chunks: Vec::new(),
            total_bytes: 0,
            total_samples: None,
            started_at: now,
            last_updated: now,
        }
    }

    /// Add a chunk reference and update totals
    pub fn add_chunk(&mut self, chunk: ChunkReference) {
        self.total_bytes += chunk.byte_count();
        if let Some(samples) = chunk.sample_count() {
            self.total_samples = Some(self.total_samples.unwrap_or(0) + samples);
        }
        self.chunks.push(chunk);
        self.last_updated = SystemTime::now();
    }

    /// Update the last chunk (for staging chunks being written)
    pub fn update_last_chunk(&mut self, bytes_written: u64, samples_written: Option<u64>) -> Result<()> {
        let last = self.chunks.last_mut()
            .ok_or_else(|| anyhow::anyhow!("no chunks in manifest"))?;

        match last {
            ChunkReference::Staging { bytes_written: ref mut bw, samples_written: ref mut sw, .. } => {
                let byte_delta = bytes_written.saturating_sub(*bw);
                *bw = bytes_written;
                *sw = samples_written;

                self.total_bytes += byte_delta;
                if let Some(new_samples) = samples_written {
                    let old_samples = self.total_samples.unwrap_or(0);
                    self.total_samples = Some(old_samples + new_samples);
                }

                self.last_updated = SystemTime::now();
                Ok(())
            }
            ChunkReference::Sealed { .. } => {
                anyhow::bail!("cannot update sealed chunk")
            }
        }
    }

    /// Seal the last chunk (convert from staging to sealed)
    pub fn seal_last_chunk(&mut self, hash: ContentHash) -> Result<()> {
        let last = self.chunks.pop()
            .ok_or_else(|| anyhow::anyhow!("no chunks to seal"))?;

        match last {
            ChunkReference::Staging { bytes_written, samples_written, .. } => {
                self.chunks.push(ChunkReference::Sealed {
                    hash,
                    byte_count: bytes_written,
                    sample_count: samples_written,
                });
                self.last_updated = SystemTime::now();
                Ok(())
            }
            ChunkReference::Sealed { .. } => {
                self.chunks.push(last);
                anyhow::bail!("last chunk is already sealed")
            }
        }
    }

    /// Get total duration in samples (if audio)
    pub fn duration_samples(&self) -> Option<u64> {
        self.total_samples
    }

    /// Get number of chunks
    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_hash(value: u8) -> ContentHash {
        ContentHash::from_data(&[value; 32])
    }

    fn make_test_staging_id() -> StagingId {
        StagingId::new()
    }

    #[test]
    fn test_new_manifest() {
        let uri = StreamUri::from("stream://test/audio");
        let hash = make_test_hash(42);

        let manifest = StreamManifest::new(uri.clone(), hash.clone());

        assert_eq!(manifest.stream_uri, uri);
        assert_eq!(manifest.definition_hash, hash);
        assert_eq!(manifest.chunks.len(), 0);
        assert_eq!(manifest.total_bytes, 0);
        assert_eq!(manifest.total_samples, None);
    }

    #[test]
    fn test_add_sealed_chunk() {
        let uri = StreamUri::from("stream://test/audio");
        let def_hash = make_test_hash(1);
        let mut manifest = StreamManifest::new(uri, def_hash);

        let chunk = ChunkReference::Sealed {
            hash: make_test_hash(2),
            byte_count: 1024,
            sample_count: Some(256),
        };

        manifest.add_chunk(chunk);

        assert_eq!(manifest.chunk_count(), 1);
        assert_eq!(manifest.total_bytes, 1024);
        assert_eq!(manifest.total_samples, Some(256));
    }

    #[test]
    fn test_add_staging_chunk() {
        let uri = StreamUri::from("stream://test/audio");
        let def_hash = make_test_hash(1);
        let mut manifest = StreamManifest::new(uri, def_hash);

        let chunk = ChunkReference::Staging {
            id: make_test_staging_id(),
            bytes_written: 512,
            samples_written: Some(128),
        };

        manifest.add_chunk(chunk);

        assert_eq!(manifest.chunk_count(), 1);
        assert_eq!(manifest.total_bytes, 512);
        assert_eq!(manifest.total_samples, Some(128));
    }

    #[test]
    fn test_seal_last_chunk() {
        let uri = StreamUri::from("stream://test/audio");
        let def_hash = make_test_hash(1);
        let mut manifest = StreamManifest::new(uri, def_hash);

        let chunk = ChunkReference::Staging {
            id: make_test_staging_id(),
            bytes_written: 512,
            samples_written: Some(128),
        };
        manifest.add_chunk(chunk);

        let seal_hash = make_test_hash(99);
        manifest.seal_last_chunk(seal_hash.clone()).unwrap();

        assert_eq!(manifest.chunk_count(), 1);
        assert!(manifest.chunks[0].is_sealed());
        match &manifest.chunks[0] {
            ChunkReference::Sealed { hash, byte_count, sample_count } => {
                assert_eq!(hash, &seal_hash);
                assert_eq!(*byte_count, 512);
                assert_eq!(*sample_count, Some(128));
            }
            _ => panic!("expected sealed chunk"),
        }
    }

    #[test]
    fn test_multiple_chunks() {
        let uri = StreamUri::from("stream://test/audio");
        let def_hash = make_test_hash(1);
        let mut manifest = StreamManifest::new(uri, def_hash);

        // Add first chunk (sealed)
        manifest.add_chunk(ChunkReference::Sealed {
            hash: make_test_hash(2),
            byte_count: 1024,
            sample_count: Some(256),
        });

        // Add second chunk (sealed)
        manifest.add_chunk(ChunkReference::Sealed {
            hash: make_test_hash(3),
            byte_count: 2048,
            sample_count: Some(512),
        });

        assert_eq!(manifest.chunk_count(), 2);
        assert_eq!(manifest.total_bytes, 3072);
        assert_eq!(manifest.total_samples, Some(768));
    }
}
