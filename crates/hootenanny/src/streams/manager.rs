//! Stream manager - coordinates stream lifecycle, chunk rotation, and manifest updates.

use super::manifest::{ChunkReference, StreamManifest};
use super::types::{StreamDefinition, StreamStatus, StreamUri};
use anyhow::{Context, Result};
use cas::{ContentHash, ContentStore, FileStore, StagingChunk};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use tracing::info;

/// Active stream state
struct ActiveStream {
    definition: StreamDefinition,
    manifest: StreamManifest,
    current_chunk: Option<StagingChunk>,
    status: StreamStatus,
}

/// Manager for stream lifecycle and chunk coordination
pub struct StreamManager {
    cas: Arc<FileStore>,
    active_streams: Arc<RwLock<HashMap<StreamUri, ActiveStream>>>,
}

impl StreamManager {
    pub fn new(cas: Arc<FileStore>) -> Self {
        Self {
            cas,
            active_streams: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Start a new stream
    ///
    /// Returns the path to the first staging chunk that chaosgarden should write to.
    pub fn start_stream(&self, definition: StreamDefinition) -> Result<PathBuf> {
        let uri = definition.uri.clone();

        let mut streams = self.active_streams.write().unwrap();

        if streams.contains_key(&uri) {
            anyhow::bail!("stream already exists: {}", uri.as_str());
        }

        // Store definition in CAS
        let definition_json =
            serde_json::to_vec(&definition).context("failed to serialize stream definition")?;
        let definition_hash = self
            .cas
            .store(&definition_json, "application/json")
            .context("failed to store stream definition")?;

        // Create manifest
        let manifest = StreamManifest::new(uri.clone(), definition_hash);

        // Create first staging chunk and close it for chaosgarden to mmap
        let mut chunk = self
            .cas
            .create_staging()
            .context("failed to create staging chunk")?;
        chunk.close(); // Close file handle so chaosgarden can mmap it

        let chunk_path = chunk.path().clone();

        let active_stream = ActiveStream {
            definition,
            manifest,
            current_chunk: Some(chunk),
            status: StreamStatus::Recording,
        };

        streams.insert(uri.clone(), active_stream);

        info!(
            "started stream: {} (first chunk: {})",
            uri,
            chunk_path.display()
        );

        Ok(chunk_path)
    }

    /// Handle a chunk-full notification from chaosgarden
    ///
    /// Seals the current chunk and creates a new staging chunk.
    /// Returns the path to the new chunk.
    pub fn handle_chunk_full(
        &self,
        uri: &StreamUri,
        bytes_written: u64,
        samples_written: Option<u64>,
    ) -> Result<PathBuf> {
        let mut streams = self.active_streams.write().unwrap();

        let active = streams
            .get_mut(uri)
            .with_context(|| format!("stream not found: {}", uri))?;

        let chunk = active.current_chunk.take().context("no active chunk")?;

        let chunk_id = chunk.id().clone();

        // Add staging chunk to manifest
        active.manifest.add_chunk(ChunkReference::Staging {
            id: chunk_id.clone(),
            bytes_written,
            samples_written,
        });

        // Seal the chunk
        let mime_type = match &active.definition.format {
            crate::streams::types::StreamFormat::Audio(_) => "audio/raw",
            crate::streams::types::StreamFormat::Midi => "audio/midi",
        };

        let seal_result = self
            .cas
            .seal(&chunk, mime_type)
            .with_context(|| format!("failed to seal chunk {:?}", chunk_id))?;

        // Update manifest to mark chunk as sealed
        active
            .manifest
            .seal_last_chunk(seal_result.content_hash.clone())?;

        // Create new staging chunk and close it for chaosgarden
        let mut new_chunk = self
            .cas
            .create_staging()
            .context("failed to create new staging chunk")?;
        new_chunk.close();
        let new_chunk_path = new_chunk.path().clone();

        active.current_chunk = Some(new_chunk);

        info!(
            "rotated chunk for stream: {} (new chunk: {})",
            uri,
            new_chunk_path.display()
        );

        Ok(new_chunk_path)
    }

    /// Update head position for a stream (from periodic notifications)
    pub fn update_head_position(
        &self,
        uri: &StreamUri,
        _bytes_written: u64,
        _samples_written: Option<u64>,
    ) -> Result<()> {
        let streams = self.active_streams.read().unwrap();

        streams
            .get(uri)
            .with_context(|| format!("stream not found: {}", uri))?;

        // Position tracking happens in chaosgarden; we just verify stream exists
        Ok(())
    }

    /// Stop a stream
    ///
    /// Seals the final chunk and returns the manifest hash.
    pub fn stop_stream(&self, uri: &StreamUri) -> Result<ContentHash> {
        let mut streams = self.active_streams.write().unwrap();

        let mut active = streams
            .remove(uri)
            .with_context(|| format!("stream not found: {}", uri))?;

        // Seal final chunk if it exists
        if let Some(chunk) = active.current_chunk.take() {
            let mime_type = match &active.definition.format {
                crate::streams::types::StreamFormat::Audio(_) => "audio/raw",
                crate::streams::types::StreamFormat::Midi => "audio/midi",
            };

            let chunk_id = chunk.id().clone();
            let seal_result = self
                .cas
                .seal(&chunk, mime_type)
                .context("failed to seal final chunk")?;

            // Add chunk to manifest if it's not already there (e.g., if we're stopping before first chunk_full)
            if active.manifest.chunks.is_empty()
                || !matches!(active.manifest.chunks.last(), Some(ChunkReference::Staging { id, .. }) if id == &chunk_id)
            {
                active.manifest.add_chunk(ChunkReference::Staging {
                    id: chunk_id,
                    bytes_written: 0,
                    samples_written: None,
                });
            }

            active.manifest.seal_last_chunk(seal_result.content_hash)?;
        }

        active.status = StreamStatus::Stopped;

        // Store manifest as artifact
        let manifest_json =
            serde_json::to_vec(&active.manifest).context("failed to serialize manifest")?;
        let manifest_hash = self
            .cas
            .store(&manifest_json, "application/json")
            .context("failed to store manifest")?;

        info!("stopped stream: {} (manifest: {})", uri, manifest_hash);

        Ok(manifest_hash)
    }

    /// Get current manifest for a stream
    pub fn get_manifest(&self, uri: &StreamUri) -> Result<Option<StreamManifest>> {
        let streams = self.active_streams.read().unwrap();
        Ok(streams.get(uri).map(|s| s.manifest.clone()))
    }

    /// Get list of active stream URIs
    pub fn active_streams(&self) -> Vec<StreamUri> {
        let streams = self.active_streams.read().unwrap();
        streams.keys().cloned().collect()
    }

    /// Get stream status
    pub fn stream_status(&self, uri: &StreamUri) -> Option<StreamStatus> {
        let streams = self.active_streams.read().unwrap();
        streams.get(uri).map(|s| s.status)
    }
}

#[cfg(test)]
mod tests {
    use super::super::types::{AudioFormat, SampleFormat, StreamFormat};
    use super::*;
    use tempfile::TempDir;

    fn setup_test_store() -> (TempDir, Arc<FileStore>) {
        let temp_dir = TempDir::new().unwrap();
        let store = Arc::new(FileStore::at_path(temp_dir.path()).unwrap());
        (temp_dir, store)
    }

    #[test]
    fn test_start_stream() {
        let (_temp, store) = setup_test_store();
        let manager = StreamManager::new(store);

        let uri = StreamUri::from("stream://test/audio");
        let definition = StreamDefinition {
            uri: uri.clone(),
            device_identity: "test-device".to_string(),
            format: StreamFormat::Audio(AudioFormat {
                sample_rate: 48000,
                channels: 2,
                sample_format: SampleFormat::F32,
            }),
            chunk_size_bytes: 1024,
        };

        let chunk_path = manager.start_stream(definition).unwrap();
        assert!(chunk_path.exists());

        let active = manager.active_streams();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0], uri);

        let status = manager.stream_status(&uri);
        assert_eq!(status, Some(StreamStatus::Recording));
    }

    #[test]
    fn test_stop_stream() {
        let (_temp, store) = setup_test_store();
        let manager = StreamManager::new(store);

        let uri = StreamUri::from("stream://test/audio");
        let definition = StreamDefinition {
            uri: uri.clone(),
            device_identity: "test-device".to_string(),
            format: StreamFormat::Audio(AudioFormat {
                sample_rate: 48000,
                channels: 2,
                sample_format: SampleFormat::F32,
            }),
            chunk_size_bytes: 1024,
        };

        manager.start_stream(definition).unwrap();

        let manifest_hash = manager.stop_stream(&uri).unwrap();
        assert!(!manifest_hash.to_string().is_empty());

        let active = manager.active_streams();
        assert_eq!(active.len(), 0);
    }

    #[test]
    fn test_double_start_fails() {
        let (_temp, store) = setup_test_store();
        let manager = StreamManager::new(store);

        let uri = StreamUri::from("stream://test/audio");
        let definition = StreamDefinition {
            uri: uri.clone(),
            device_identity: "test-device".to_string(),
            format: StreamFormat::Audio(AudioFormat {
                sample_rate: 48000,
                channels: 2,
                sample_format: SampleFormat::F32,
            }),
            chunk_size_bytes: 1024,
        };

        manager.start_stream(definition.clone()).unwrap();

        let result = manager.start_stream(definition);
        assert!(result.is_err());
    }

    #[test]
    fn test_chunk_rotation() {
        let (_temp, store) = setup_test_store();
        let manager = StreamManager::new(store.clone());

        let uri = StreamUri::from("stream://test/audio");
        let definition = StreamDefinition {
            uri: uri.clone(),
            device_identity: "test-device".to_string(),
            format: StreamFormat::Audio(AudioFormat {
                sample_rate: 48000,
                channels: 2,
                sample_format: SampleFormat::F32,
            }),
            chunk_size_bytes: 1024,
        };

        let first_chunk = manager.start_stream(definition).unwrap();
        assert!(first_chunk.exists());

        // Simulate chunk full
        let new_chunk = manager.handle_chunk_full(&uri, 1024, Some(256)).unwrap();
        assert!(new_chunk.exists());
        assert_ne!(first_chunk, new_chunk);

        // Check manifest has sealed chunk
        let manifest = manager.get_manifest(&uri).unwrap().unwrap();
        assert_eq!(manifest.chunk_count(), 1);
        assert!(manifest.chunks[0].is_sealed());
        assert_eq!(manifest.total_bytes, 1024);
        assert_eq!(manifest.total_samples, Some(256));
    }
}
