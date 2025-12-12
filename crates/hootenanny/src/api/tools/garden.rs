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

impl EventDualityServer {
    fn require_garden(&self) -> Result<(), ToolError> {
        if self.garden_manager.is_none() {
            return Err(ToolError::invalid_params(
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
            return Err(ToolError::invalid_params("BPM must be between 0 and 999"));
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
            _ => return Err(ToolError::invalid_params("variables must be a JSON object")),
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
}
