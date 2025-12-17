//! MCP tools for controlling chaosgarden (RT audio daemon)
//!
//! These tools provide playback control, transport state, and graph operations
//! via the ZMQ connection to chaosgarden.

use crate::api::service::EventDualityServer;
use hooteproto::{ToolOutput, ToolResult, ToolError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Request to connect to chaosgarden daemon
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GardenConnectRequest {
    /// Endpoint: "local" for IPC, or "tcp://host:port"
    pub endpoint: String,
}

/// Response from garden operations
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GardenResponse {
    pub success: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

/// Request to seek playback position
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GardenSeekRequest {
    /// Beat position to seek to
    pub beat: f64,
}

/// Request to set tempo
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GardenSetTempoRequest {
    /// Tempo in beats per minute
    pub bpm: f64,
}

/// Request to execute a Trustfall query on chaosgarden's graph
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GardenQueryRequest {
    /// GraphQL-style Trustfall query
    pub query: String,
    /// Query variables as JSON object
    #[serde(default)]
    pub variables: serde_json::Value,
}

/// Request to create a region on the timeline
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GardenCreateRegionRequest {
    /// Position in beats
    pub position: f64,
    /// Duration in beats
    pub duration: f64,
    /// Behavior type: "play_content" or "latent"
    pub behavior_type: String,
    /// For play_content: artifact_id. For latent: job_id
    pub content_id: String,
}

/// Request to delete a region
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GardenDeleteRegionRequest {
    /// Region UUID
    pub region_id: String,
}

/// Request to move a region
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GardenMoveRegionRequest {
    /// Region UUID
    pub region_id: String,
    /// New position in beats
    pub new_position: f64,
}

/// Request to get regions (optionally in a range)
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GardenGetRegionsRequest {
    /// Optional start beat (inclusive)
    pub start: Option<f64>,
    /// Optional end beat (exclusive)
    pub end: Option<f64>,
}

/// Request to attach PipeWire audio output
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GardenAttachAudioRequest {
    /// Device name hint (empty for default output)
    #[serde(default)]
    pub device_name: Option<String>,
    /// Sample rate in Hz (default: 48000)
    #[serde(default)]
    pub sample_rate: Option<u32>,
    /// Latency in frames (default: 256)
    #[serde(default)]
    pub latency_frames: Option<u32>,
}

impl EventDualityServer {
    fn require_garden(&self) -> Result<(), ToolError> {
        if self.garden_manager.is_none() {
            return Err(ToolError::validation(
                "not_connected",
                "Not connected to chaosgarden. Start hootenanny with --chaosgarden=local or --chaosgarden=tcp://host:port"
            ));
        }
        Ok(())
    }

    #[tracing::instrument(name = "mcp.tool.garden_status", skip(self))]
    pub async fn garden_status(&self) -> ToolResult {
        let response = match &self.garden_manager {
            Some(manager) => {
                let connected = manager.is_connected().await;

                if connected {
                    match manager.get_transport_state().await {
                        Ok(reply) => {
                            GardenResponse {
                                success: true,
                                message: "Connected to chaosgarden".to_string(),
                                data: Some(serde_json::to_value(&reply).unwrap_or_default()),
                            }
                        }
                        Err(e) => {
                            GardenResponse {
                                success: true,
                                message: format!("Connected but failed to get state: {}", e),
                                data: None,
                            }
                        }
                    }
                } else {
                    GardenResponse {
                        success: false,
                        message: "Connection to chaosgarden lost".to_string(),
                        data: None,
                    }
                }
            }
            None => {
                GardenResponse {
                    success: false,
                    message: "Not connected to chaosgarden".to_string(),
                    data: None,
                }
            }
        };

        let json = serde_json::to_string_pretty(&response)
            .map_err(|e| ToolError::internal(format!("Serialization error: {}", e)))?;

        Ok(ToolOutput::text_only(json))
    }

    #[tracing::instrument(name = "mcp.tool.garden_play", skip(self))]
    pub async fn garden_play(&self) -> ToolResult {
        self.require_garden()?;
        let manager = self.garden_manager.as_ref().unwrap();

        match manager.play().await {
            Ok(reply) => {
                let response = GardenResponse {
                    success: true,
                    message: "Playback started".to_string(),
                    data: Some(serde_json::to_value(&reply).unwrap_or_default()),
                };
                let json = serde_json::to_string_pretty(&response)
                    .map_err(|e| ToolError::internal(e.to_string()))?;
                Ok(ToolOutput::text_only(json))
            }
            Err(e) => {
                Err(ToolError::internal(format!("Play failed: {}", e)))
            }
        }
    }

    #[tracing::instrument(name = "mcp.tool.garden_pause", skip(self))]
    pub async fn garden_pause(&self) -> ToolResult {
        self.require_garden()?;
        let manager = self.garden_manager.as_ref().unwrap();

        match manager.pause().await {
            Ok(reply) => {
                let response = GardenResponse {
                    success: true,
                    message: "Playback paused".to_string(),
                    data: Some(serde_json::to_value(&reply).unwrap_or_default()),
                };
                let json = serde_json::to_string_pretty(&response)
                    .map_err(|e| ToolError::internal(e.to_string()))?;
                Ok(ToolOutput::text_only(json))
            }
            Err(e) => {
                Err(ToolError::internal(format!("Pause failed: {}", e)))
            }
        }
    }

    #[tracing::instrument(name = "mcp.tool.garden_stop", skip(self))]
    pub async fn garden_stop(&self) -> ToolResult {
        self.require_garden()?;
        let manager = self.garden_manager.as_ref().unwrap();

        match manager.stop().await {
            Ok(reply) => {
                let response = GardenResponse {
                    success: true,
                    message: "Playback stopped".to_string(),
                    data: Some(serde_json::to_value(&reply).unwrap_or_default()),
                };
                let json = serde_json::to_string_pretty(&response)
                    .map_err(|e| ToolError::internal(e.to_string()))?;
                Ok(ToolOutput::text_only(json))
            }
            Err(e) => {
                Err(ToolError::internal(format!("Stop failed: {}", e)))
            }
        }
    }

    #[tracing::instrument(name = "mcp.tool.garden_seek", skip(self), fields(beat = request.beat))]
    pub async fn garden_seek(&self, request: GardenSeekRequest) -> ToolResult {
        self.require_garden()?;
        let manager = self.garden_manager.as_ref().unwrap();

        match manager.seek(request.beat).await {
            Ok(reply) => {
                let response = GardenResponse {
                    success: true,
                    message: format!("Seeked to beat {}", request.beat),
                    data: Some(serde_json::to_value(&reply).unwrap_or_default()),
                };
                let json = serde_json::to_string_pretty(&response)
                    .map_err(|e| ToolError::internal(e.to_string()))?;
                Ok(ToolOutput::text_only(json))
            }
            Err(e) => {
                Err(ToolError::internal(format!("Seek failed: {}", e)))
            }
        }
    }

    #[tracing::instrument(name = "mcp.tool.garden_set_tempo", skip(self), fields(bpm = request.bpm))]
    pub async fn garden_set_tempo(&self, request: GardenSetTempoRequest) -> ToolResult {
        self.require_garden()?;
        let manager = self.garden_manager.as_ref().unwrap();

        if request.bpm <= 0.0 || request.bpm > 999.0 {
            return Err(ToolError::validation("invalid_params", "BPM must be between 0 and 999"));
        }

        match manager.set_tempo(request.bpm).await {
            Ok(reply) => {
                let response = GardenResponse {
                    success: true,
                    message: format!("Tempo set to {} BPM", request.bpm),
                    data: Some(serde_json::to_value(&reply).unwrap_or_default()),
                };
                let json = serde_json::to_string_pretty(&response)
                    .map_err(|e| ToolError::internal(e.to_string()))?;
                Ok(ToolOutput::text_only(json))
            }
            Err(e) => {
                Err(ToolError::internal(format!("Set tempo failed: {}", e)))
            }
        }
    }

    #[tracing::instrument(name = "mcp.tool.garden_query", skip(self, request))]
    pub async fn garden_query(&self, request: GardenQueryRequest) -> ToolResult {
        self.require_garden()?;
        let manager = self.garden_manager.as_ref().unwrap();

        let variables: std::collections::HashMap<String, serde_json::Value> = match request.variables {
            serde_json::Value::Object(map) => map.into_iter().collect(),
            serde_json::Value::Null => std::collections::HashMap::new(),
            _ => return Err(ToolError::validation("invalid_params", "variables must be a JSON object")),
        };

        match manager.query(&request.query, variables).await {
            Ok(reply) => {
                let response = GardenResponse {
                    success: true,
                    message: "Query executed".to_string(),
                    data: Some(serde_json::to_value(&reply).unwrap_or_default()),
                };
                let json = serde_json::to_string_pretty(&response)
                    .map_err(|e| ToolError::internal(e.to_string()))?;
                Ok(ToolOutput::text_only(json))
            }
            Err(e) => {
                Err(ToolError::internal(format!("Query failed: {}", e)))
            }
        }
    }

    #[tracing::instrument(name = "mcp.tool.garden_emergency_pause", skip(self))]
    pub async fn garden_emergency_pause(&self) -> ToolResult {
        self.require_garden()?;
        let manager = self.garden_manager.as_ref().unwrap();

        match manager.emergency_pause().await {
            Ok(reply) => {
                let response = GardenResponse {
                    success: true,
                    message: "Emergency pause triggered".to_string(),
                    data: Some(serde_json::to_value(&reply).unwrap_or_default()),
                };
                let json = serde_json::to_string_pretty(&response)
                    .map_err(|e| ToolError::internal(e.to_string()))?;
                Ok(ToolOutput::text_only(json))
            }
            Err(e) => {
                Err(ToolError::internal(format!("Emergency pause failed: {}", e)))
            }
        }
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Region Operations
    // ═══════════════════════════════════════════════════════════════════════

    #[tracing::instrument(name = "mcp.tool.garden_create_region", skip(self, request))]
    pub async fn garden_create_region(&self, request: GardenCreateRegionRequest) -> ToolResult {
        self.require_garden()?;
        let manager = self.garden_manager.as_ref().unwrap();

        use chaosgarden::ipc::{Beat, Behavior, ShellRequest};

        let behavior = match request.behavior_type.as_str() {
            "play_content" => Behavior::PlayContent { artifact_id: request.content_id },
            "latent" => Behavior::Latent { job_id: request.content_id },
            other => return Err(ToolError::validation("invalid_params", format!("Unknown behavior_type: {}. Use 'play_content' or 'latent'", other))),
        };

        let shell_req = ShellRequest::CreateRegion {
            position: Beat(request.position),
            duration: Beat(request.duration),
            behavior,
        };

        match manager.request(shell_req).await {
            Ok(chaosgarden::ipc::ShellReply::RegionCreated { region_id }) => {
                let response = GardenResponse {
                    success: true,
                    message: format!("Region created: {}", region_id),
                    data: Some(serde_json::json!({
                        "region_id": region_id.to_string(),
                        "position": request.position,
                        "duration": request.duration,
                    })),
                };
                let json = serde_json::to_string_pretty(&response)
                    .map_err(|e| ToolError::internal(e.to_string()))?;
                Ok(ToolOutput::text_only(json))
            }
            Ok(other) => Err(ToolError::internal(format!("Unexpected reply: {:?}", other))),
            Err(e) => Err(ToolError::internal(format!("Create region failed: {}", e))),
        }
    }

    #[tracing::instrument(name = "mcp.tool.garden_delete_region", skip(self))]
    pub async fn garden_delete_region(&self, request: GardenDeleteRegionRequest) -> ToolResult {
        self.require_garden()?;
        let manager = self.garden_manager.as_ref().unwrap();

        use chaosgarden::ipc::ShellRequest;
        use uuid::Uuid;

        let region_id = Uuid::parse_str(&request.region_id)
            .map_err(|e| ToolError::validation("invalid_params", format!("Invalid region_id: {}", e)))?;

        match manager.request(ShellRequest::DeleteRegion { region_id }).await {
            Ok(chaosgarden::ipc::ShellReply::Ok { .. }) => {
                let response = GardenResponse {
                    success: true,
                    message: format!("Region {} deleted", region_id),
                    data: Some(serde_json::json!({"deleted": region_id.to_string()})),
                };
                let json = serde_json::to_string_pretty(&response)
                    .map_err(|e| ToolError::internal(e.to_string()))?;
                Ok(ToolOutput::text_only(json))
            }
            Ok(chaosgarden::ipc::ShellReply::Error { error, .. }) => {
                Err(ToolError::internal(error))
            }
            Ok(other) => Err(ToolError::internal(format!("Unexpected reply: {:?}", other))),
            Err(e) => Err(ToolError::internal(format!("Delete region failed: {}", e))),
        }
    }

    #[tracing::instrument(name = "mcp.tool.garden_move_region", skip(self))]
    pub async fn garden_move_region(&self, request: GardenMoveRegionRequest) -> ToolResult {
        self.require_garden()?;
        let manager = self.garden_manager.as_ref().unwrap();

        use chaosgarden::ipc::{Beat, ShellRequest};
        use uuid::Uuid;

        let region_id = Uuid::parse_str(&request.region_id)
            .map_err(|e| ToolError::validation("invalid_params", format!("Invalid region_id: {}", e)))?;

        match manager.request(ShellRequest::MoveRegion {
            region_id,
            new_position: Beat(request.new_position),
        }).await {
            Ok(chaosgarden::ipc::ShellReply::Ok { .. }) => {
                let response = GardenResponse {
                    success: true,
                    message: format!("Region {} moved to beat {}", region_id, request.new_position),
                    data: Some(serde_json::json!({
                        "region_id": region_id.to_string(),
                        "new_position": request.new_position,
                    })),
                };
                let json = serde_json::to_string_pretty(&response)
                    .map_err(|e| ToolError::internal(e.to_string()))?;
                Ok(ToolOutput::text_only(json))
            }
            Ok(chaosgarden::ipc::ShellReply::Error { error, .. }) => {
                Err(ToolError::internal(error))
            }
            Ok(other) => Err(ToolError::internal(format!("Unexpected reply: {:?}", other))),
            Err(e) => Err(ToolError::internal(format!("Move region failed: {}", e))),
        }
    }

    #[tracing::instrument(name = "mcp.tool.garden_get_regions", skip(self))]
    pub async fn garden_get_regions(&self, request: GardenGetRegionsRequest) -> ToolResult {
        self.require_garden()?;
        let manager = self.garden_manager.as_ref().unwrap();

        use chaosgarden::ipc::{Beat, ShellRequest};

        let range = match (request.start, request.end) {
            (Some(s), Some(e)) => Some((Beat(s), Beat(e))),
            _ => None,
        };

        match manager.request(ShellRequest::GetRegions { range }).await {
            Ok(chaosgarden::ipc::ShellReply::Regions { regions }) => {
                let regions_json: Vec<serde_json::Value> = regions.iter().map(|r| {
                    serde_json::json!({
                        "region_id": r.region_id.to_string(),
                        "position": r.position.0,
                        "duration": r.duration.0,
                        "is_latent": r.is_latent,
                        "artifact_id": r.artifact_id,
                    })
                }).collect();

                let response = GardenResponse {
                    success: true,
                    message: format!("Found {} regions", regions.len()),
                    data: Some(serde_json::json!({
                        "count": regions.len(),
                        "regions": regions_json,
                    })),
                };
                let json = serde_json::to_string_pretty(&response)
                    .map_err(|e| ToolError::internal(e.to_string()))?;
                Ok(ToolOutput::text_only(json))
            }
            Ok(other) => Err(ToolError::internal(format!("Unexpected reply: {:?}", other))),
            Err(e) => Err(ToolError::internal(format!("Get regions failed: {}", e))),
        }
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Audio Output Attachment
    // ═══════════════════════════════════════════════════════════════════════

    #[tracing::instrument(name = "mcp.tool.garden_attach_audio", skip(self))]
    pub async fn garden_attach_audio(&self, request: GardenAttachAudioRequest) -> ToolResult {
        self.require_garden()?;
        let manager = self.garden_manager.as_ref().unwrap();

        use chaosgarden::ipc::ShellRequest;

        match manager.request(ShellRequest::AttachAudio {
            device_name: request.device_name,
            sample_rate: request.sample_rate,
            latency_frames: request.latency_frames,
        }).await {
            Ok(chaosgarden::ipc::ShellReply::Ok { result }) => {
                let response = GardenResponse {
                    success: true,
                    message: "Audio output attached".to_string(),
                    data: Some(result),
                };
                let json = serde_json::to_string_pretty(&response)
                    .map_err(|e| ToolError::internal(e.to_string()))?;
                Ok(ToolOutput::text_only(json))
            }
            Ok(chaosgarden::ipc::ShellReply::Error { error, .. }) => {
                Err(ToolError::internal(error))
            }
            Ok(other) => Err(ToolError::internal(format!("Unexpected reply: {:?}", other))),
            Err(e) => Err(ToolError::internal(format!("Attach audio failed: {}", e))),
        }
    }

    #[tracing::instrument(name = "mcp.tool.garden_detach_audio", skip(self))]
    pub async fn garden_detach_audio(&self) -> ToolResult {
        self.require_garden()?;
        let manager = self.garden_manager.as_ref().unwrap();

        use chaosgarden::ipc::ShellRequest;

        match manager.request(ShellRequest::DetachAudio).await {
            Ok(chaosgarden::ipc::ShellReply::Ok { result }) => {
                let response = GardenResponse {
                    success: true,
                    message: "Audio output detached".to_string(),
                    data: Some(result),
                };
                let json = serde_json::to_string_pretty(&response)
                    .map_err(|e| ToolError::internal(e.to_string()))?;
                Ok(ToolOutput::text_only(json))
            }
            Ok(chaosgarden::ipc::ShellReply::Error { error, .. }) => {
                Err(ToolError::internal(error))
            }
            Ok(other) => Err(ToolError::internal(format!("Unexpected reply: {:?}", other))),
            Err(e) => Err(ToolError::internal(format!("Detach audio failed: {}", e))),
        }
    }

    #[tracing::instrument(name = "mcp.tool.garden_audio_status", skip(self))]
    pub async fn garden_audio_status(&self) -> ToolResult {
        self.require_garden()?;
        let manager = self.garden_manager.as_ref().unwrap();

        use chaosgarden::ipc::ShellRequest;

        match manager.request(ShellRequest::GetAudioStatus).await {
            Ok(chaosgarden::ipc::ShellReply::AudioStatus {
                attached,
                device_name,
                sample_rate,
                latency_frames,
                callbacks,
                samples_written,
                underruns,
            }) => {
                let response = GardenResponse {
                    success: true,
                    message: if attached { "Audio attached".to_string() } else { "Audio not attached".to_string() },
                    data: Some(serde_json::json!({
                        "attached": attached,
                        "device_name": device_name,
                        "sample_rate": sample_rate,
                        "latency_frames": latency_frames,
                        "callbacks": callbacks,
                        "samples_written": samples_written,
                        "underruns": underruns,
                    })),
                };
                let json = serde_json::to_string_pretty(&response)
                    .map_err(|e| ToolError::internal(e.to_string()))?;
                Ok(ToolOutput::text_only(json))
            }
            Ok(chaosgarden::ipc::ShellReply::Error { error, .. }) => {
                Err(ToolError::internal(error))
            }
            Ok(other) => Err(ToolError::internal(format!("Unexpected reply: {:?}", other))),
            Err(e) => Err(ToolError::internal(format!("Get audio status failed: {}", e))),
        }
    }
}
