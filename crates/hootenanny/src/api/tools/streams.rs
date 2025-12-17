//! MCP tools for stream capture.
//!
//! Exposes stream lifecycle management, status, and slicing operations.

use crate::api::service::EventDualityServer;
use crate::streams::{
    AudioFormat, SampleFormat, SliceOutput, SliceRequest, StreamDefinition, StreamFormat,
    StreamUri, TimeSpec,
};
use hooteproto::{ToolError, ToolOutput, ToolResult};
use serde::{Deserialize, Serialize};
use tracing::info;

// ============================================================================
// Request/Response Types
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct StreamCreateRequest {
    pub uri: String,
    pub device_identity: String,
    pub format: StreamFormatSpec,
    #[serde(default = "default_chunk_size")]
    pub chunk_size_bytes: u64,
}

fn default_chunk_size() -> u64 {
    524_288 // 512 KB default
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum StreamFormatSpec {
    Audio {
        sample_rate: u32,
        channels: u8,
        sample_format: SampleFormatSpec,
    },
    Midi,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SampleFormatSpec {
    F32,
    I16,
    I24,
}

impl From<SampleFormatSpec> for SampleFormat {
    fn from(spec: SampleFormatSpec) -> Self {
        match spec {
            SampleFormatSpec::F32 => SampleFormat::F32,
            SampleFormatSpec::I16 => SampleFormat::I16,
            SampleFormatSpec::I24 => SampleFormat::I24,
        }
    }
}

impl From<SampleFormat> for SampleFormatSpec {
    fn from(format: SampleFormat) -> Self {
        match format {
            SampleFormat::F32 => SampleFormatSpec::F32,
            SampleFormat::I16 => SampleFormatSpec::I16,
            SampleFormat::I24 => SampleFormatSpec::I24,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct StreamCreateResponse {
    pub uri: String,
    pub chunk_path: String,
    pub status: String,
}

#[derive(Debug, Deserialize)]
pub struct StreamStartRequest {
    pub uri: String,
}

#[derive(Debug, Serialize)]
pub struct StreamStartResponse {
    pub uri: String,
    pub chunk_path: String,
}

#[derive(Debug, Deserialize)]
pub struct StreamStopRequest {
    pub uri: String,
}

#[derive(Debug, Serialize)]
pub struct StreamStopResponse {
    pub uri: String,
    pub manifest_hash: String,
    pub total_bytes: u64,
    pub total_samples: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct StreamStatusRequest {
    pub uri: String,
}

#[derive(Debug, Serialize)]
pub struct StreamStatusResponse {
    pub uri: String,
    pub status: String,
    pub chunk_count: Option<usize>,
    pub total_bytes: Option<u64>,
    pub total_samples: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct StreamSliceRequest {
    pub stream_uri: String,
    pub from: TimeSpecParam,
    pub to: TimeSpecParam,
    #[serde(default = "default_slice_output")]
    pub output: SliceOutputParam,
}

fn default_slice_output() -> SliceOutputParam {
    SliceOutputParam::Materialize
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum TimeSpecParam {
    Absolute { timestamp: u64 },
    Relative { seconds_ago: f64 },
    SamplePosition { position: u64 },
    StreamStart,
    StreamHead,
}

impl From<TimeSpecParam> for TimeSpec {
    fn from(param: TimeSpecParam) -> Self {
        match param {
            TimeSpecParam::Absolute { timestamp } => {
                use std::time::{Duration, SystemTime, UNIX_EPOCH};
                TimeSpec::Absolute(UNIX_EPOCH + Duration::from_secs(timestamp))
            }
            TimeSpecParam::Relative { seconds_ago } => TimeSpec::Relative { seconds_ago },
            TimeSpecParam::SamplePosition { position } => TimeSpec::SamplePosition(position),
            TimeSpecParam::StreamStart => TimeSpec::StreamStart,
            TimeSpecParam::StreamHead => TimeSpec::StreamHead,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SliceOutputParam {
    Materialize,
    Virtual,
}

impl From<SliceOutputParam> for SliceOutput {
    fn from(param: SliceOutputParam) -> Self {
        match param {
            SliceOutputParam::Materialize => SliceOutput::Materialize,
            SliceOutputParam::Virtual => SliceOutput::Virtual,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct StreamSliceResponse {
    pub content_hash: String,
    pub sample_range: Option<(u64, u64)>,
    pub source_chunks: Vec<String>,
    pub mime_type: String,
}

// ============================================================================
// Tool Implementations
// ============================================================================

impl EventDualityServer {
    /// Create a new stream definition and start recording
    #[tracing::instrument(
        name = "mcp.tool.stream_create",
        skip(self, request),
        fields(
            stream.uri = %request.uri,
            stream.device = %request.device_identity,
        )
    )]
    pub async fn stream_create(&self, request: StreamCreateRequest) -> ToolResult {
        let stream_manager = self
            .stream_manager
            .as_ref()
            .ok_or_else(|| ToolError::internal("stream manager not available"))?;

        let uri = StreamUri::from(request.uri.as_str());

        let format = match request.format {
            StreamFormatSpec::Audio {
                sample_rate,
                channels,
                sample_format,
            } => StreamFormat::Audio(AudioFormat {
                sample_rate,
                channels,
                sample_format: sample_format.into(),
            }),
            StreamFormatSpec::Midi => StreamFormat::Midi,
        };

        let definition = StreamDefinition {
            uri: uri.clone(),
            device_identity: request.device_identity,
            format,
            chunk_size_bytes: request.chunk_size_bytes,
        };

        let chunk_path = stream_manager
            .start_stream(definition)
            .map_err(|e| ToolError::internal(format!("failed to start stream: {}", e)))?;

        info!("created and started stream: {}", uri);

        let response = StreamCreateResponse {
            uri: uri.to_string(),
            chunk_path: chunk_path.display().to_string(),
            status: "recording".to_string(),
        };

        Ok(ToolOutput::new(
            format!("Stream {} created and recording", uri),
            &response,
        ))
    }

    /// Start an existing stream (if it was previously stopped)
    #[tracing::instrument(
        name = "mcp.tool.stream_start",
        skip(self, request),
        fields(stream.uri = %request.uri)
    )]
    pub async fn stream_start(&self, request: StreamStartRequest) -> ToolResult {
        // For now, stream_create handles both creation and starting
        // This tool exists for API completeness but currently just returns an error
        // suggesting to use stream_create instead
        Err(ToolError::validation(
            "not_supported",
            "Use stream_create to create and start a new stream. Restarting stopped streams is not yet supported.",
        ))
    }

    /// Stop a stream and seal all chunks
    #[tracing::instrument(
        name = "mcp.tool.stream_stop",
        skip(self, request),
        fields(stream.uri = %request.uri)
    )]
    pub async fn stream_stop(&self, request: StreamStopRequest) -> ToolResult {
        let stream_manager = self
            .stream_manager
            .as_ref()
            .ok_or_else(|| ToolError::internal("stream manager not available"))?;

        let uri = StreamUri::from(request.uri.as_str());

        // Get manifest before stopping to capture current state
        let manifest = stream_manager
            .get_manifest(&uri)
            .map_err(|e| ToolError::internal(format!("failed to get manifest: {}", e)))?
            .ok_or_else(|| {
                ToolError::validation("not_found", format!("stream not found: {}", uri))
            })?;

        let manifest_hash = stream_manager
            .stop_stream(&uri)
            .map_err(|e| ToolError::internal(format!("failed to stop stream: {}", e)))?;

        info!("stopped stream: {} (manifest: {})", uri, manifest_hash);

        let response = StreamStopResponse {
            uri: uri.to_string(),
            manifest_hash: manifest_hash.to_string(),
            total_bytes: manifest.total_bytes,
            total_samples: manifest.total_samples,
        };

        Ok(ToolOutput::new(
            format!(
                "Stream {} stopped ({} bytes, {} chunks)",
                uri,
                response.total_bytes,
                manifest.chunk_count()
            ),
            &response,
        ))
    }

    /// Get status of a stream
    #[tracing::instrument(
        name = "mcp.tool.stream_status",
        skip(self, request),
        fields(stream.uri = %request.uri)
    )]
    pub async fn stream_status(&self, request: StreamStatusRequest) -> ToolResult {
        let stream_manager = self
            .stream_manager
            .as_ref()
            .ok_or_else(|| ToolError::internal("stream manager not available"))?;

        let uri = StreamUri::from(request.uri.as_str());

        let status = stream_manager
            .stream_status(&uri)
            .ok_or_else(|| {
                ToolError::validation("not_found", format!("stream not found: {}", uri))
            })?;

        let manifest = stream_manager.get_manifest(&uri).map_err(|e| {
            ToolError::internal(format!("failed to get manifest: {}", e))
        })?;

        let (chunk_count, total_bytes, total_samples) = if let Some(m) = manifest {
            (
                Some(m.chunk_count()),
                Some(m.total_bytes),
                m.total_samples,
            )
        } else {
            (None, None, None)
        };

        let response = StreamStatusResponse {
            uri: uri.to_string(),
            status: format!("{:?}", status),
            chunk_count,
            total_bytes,
            total_samples,
        };

        Ok(ToolOutput::new(
            format!("Stream {} status: {:?}", uri, status),
            &response,
        ))
    }

    /// Slice a time range from a stream
    #[tracing::instrument(
        name = "mcp.tool.stream_slice",
        skip(self, request),
        fields(
            stream.uri = %request.stream_uri,
            slice.output = ?request.output,
        )
    )]
    pub async fn stream_slice(&self, request: StreamSliceRequest) -> ToolResult {
        let stream_manager = self
            .stream_manager
            .as_ref()
            .ok_or_else(|| ToolError::internal("stream manager not available"))?;

        let slicing_engine = self
            .slicing_engine
            .as_ref()
            .ok_or_else(|| ToolError::internal("slicing engine not available"))?;

        let uri = StreamUri::from(request.stream_uri.as_str());

        // Get manifest
        let manifest = stream_manager
            .get_manifest(&uri)
            .map_err(|e| ToolError::internal(format!("failed to get manifest: {}", e)))?
            .ok_or_else(|| {
                ToolError::validation("not_found", format!("stream not found: {}", uri))
            })?;

        // Build slice request
        let slice_request = SliceRequest {
            stream_uri: uri.clone(),
            from: request.from.into(),
            to: request.to.into(),
            output: request.output.into(),
        };

        // Execute slice
        let result = slicing_engine
            .slice(slice_request, &manifest)
            .map_err(|e| ToolError::internal(format!("failed to slice stream: {}", e)))?;

        info!(
            "sliced stream {} (hash: {}, chunks: {})",
            uri,
            result.content_hash,
            result.source_chunks.len()
        );

        let response = StreamSliceResponse {
            content_hash: result.content_hash.to_string(),
            sample_range: result.sample_range.map(|r| (r.start, r.end)),
            source_chunks: result
                .source_chunks
                .iter()
                .map(|h| h.to_string())
                .collect(),
            mime_type: result.mime_type,
        };

        Ok(ToolOutput::new(
            format!(
                "Sliced stream {} → {} ({} chunks)",
                uri,
                response.content_hash,
                response.source_chunks.len()
            ),
            &response,
        ))
    }
}
