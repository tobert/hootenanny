//! MCP tools for controlling chaosgarden (RT audio daemon)
//!
//! These tools provide playback control, transport state, and graph operations
//! via the ZMQ connection to chaosgarden.

use crate::api::service::EventDualityServer;
use baton::{CallToolResult, Content, ErrorData as McpError};
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
    /// Check if garden connection is available
    fn require_garden(&self) -> Result<(), McpError> {
        if self.garden_manager.is_none() {
            return Err(McpError::invalid_request(
                "Not connected to chaosgarden. Start hootenanny with --chaosgarden=local or --chaosgarden=tcp://host:port".to_string()
            ));
        }
        Ok(())
    }

    /// garden_status - Get chaosgarden connection and transport state
    #[tracing::instrument(name = "mcp.tool.garden_status", skip(self))]
    pub async fn garden_status(&self) -> Result<CallToolResult, McpError> {
        let response = match &self.garden_manager {
            Some(manager) => {
                let connected = manager.is_connected().await;

                if connected {
                    // Try to get transport state
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
            .map_err(|e| McpError::internal_error(format!("Serialization error: {}", e)))?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    /// garden_play - Start playback
    #[tracing::instrument(name = "mcp.tool.garden_play", skip(self))]
    pub async fn garden_play(&self) -> Result<CallToolResult, McpError> {
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
                    .map_err(|e| McpError::internal_error(e.to_string()))?;
                Ok(CallToolResult::success(vec![Content::text(json)]))
            }
            Err(e) => {
                Err(McpError::internal_error(format!("Play failed: {}", e)))
            }
        }
    }

    /// garden_pause - Pause playback
    #[tracing::instrument(name = "mcp.tool.garden_pause", skip(self))]
    pub async fn garden_pause(&self) -> Result<CallToolResult, McpError> {
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
                    .map_err(|e| McpError::internal_error(e.to_string()))?;
                Ok(CallToolResult::success(vec![Content::text(json)]))
            }
            Err(e) => {
                Err(McpError::internal_error(format!("Pause failed: {}", e)))
            }
        }
    }

    /// garden_stop - Stop playback and reset position
    #[tracing::instrument(name = "mcp.tool.garden_stop", skip(self))]
    pub async fn garden_stop(&self) -> Result<CallToolResult, McpError> {
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
                    .map_err(|e| McpError::internal_error(e.to_string()))?;
                Ok(CallToolResult::success(vec![Content::text(json)]))
            }
            Err(e) => {
                Err(McpError::internal_error(format!("Stop failed: {}", e)))
            }
        }
    }

    /// garden_seek - Seek to beat position
    #[tracing::instrument(name = "mcp.tool.garden_seek", skip(self), fields(beat = request.beat))]
    pub async fn garden_seek(&self, request: GardenSeekRequest) -> Result<CallToolResult, McpError> {
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
                    .map_err(|e| McpError::internal_error(e.to_string()))?;
                Ok(CallToolResult::success(vec![Content::text(json)]))
            }
            Err(e) => {
                Err(McpError::internal_error(format!("Seek failed: {}", e)))
            }
        }
    }

    /// garden_set_tempo - Set tempo in BPM
    #[tracing::instrument(name = "mcp.tool.garden_set_tempo", skip(self), fields(bpm = request.bpm))]
    pub async fn garden_set_tempo(&self, request: GardenSetTempoRequest) -> Result<CallToolResult, McpError> {
        self.require_garden()?;
        let manager = self.garden_manager.as_ref().unwrap();

        if request.bpm <= 0.0 || request.bpm > 999.0 {
            return Err(McpError::invalid_params("BPM must be between 0 and 999".to_string()));
        }

        match manager.set_tempo(request.bpm).await {
            Ok(reply) => {
                let response = GardenResponse {
                    success: true,
                    message: format!("Tempo set to {} BPM", request.bpm),
                    data: Some(serde_json::to_value(&reply).unwrap_or_default()),
                };
                let json = serde_json::to_string_pretty(&response)
                    .map_err(|e| McpError::internal_error(e.to_string()))?;
                Ok(CallToolResult::success(vec![Content::text(json)]))
            }
            Err(e) => {
                Err(McpError::internal_error(format!("Set tempo failed: {}", e)))
            }
        }
    }

    /// garden_query - Execute a Trustfall query on chaosgarden's graph
    #[tracing::instrument(name = "mcp.tool.garden_query", skip(self, request))]
    pub async fn garden_query(&self, request: GardenQueryRequest) -> Result<CallToolResult, McpError> {
        self.require_garden()?;
        let manager = self.garden_manager.as_ref().unwrap();

        // Convert variables to HashMap
        let variables: std::collections::HashMap<String, serde_json::Value> = match request.variables {
            serde_json::Value::Object(map) => map.into_iter().collect(),
            serde_json::Value::Null => std::collections::HashMap::new(),
            _ => return Err(McpError::invalid_params("variables must be a JSON object".to_string())),
        };

        match manager.query(&request.query, variables).await {
            Ok(reply) => {
                let response = GardenResponse {
                    success: true,
                    message: "Query executed".to_string(),
                    data: Some(serde_json::to_value(&reply).unwrap_or_default()),
                };
                let json = serde_json::to_string_pretty(&response)
                    .map_err(|e| McpError::internal_error(e.to_string()))?;
                Ok(CallToolResult::success(vec![Content::text(json)]))
            }
            Err(e) => {
                Err(McpError::internal_error(format!("Query failed: {}", e)))
            }
        }
    }

    /// garden_emergency_pause - Emergency pause (priority channel)
    #[tracing::instrument(name = "mcp.tool.garden_emergency_pause", skip(self))]
    pub async fn garden_emergency_pause(&self) -> Result<CallToolResult, McpError> {
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
                    .map_err(|e| McpError::internal_error(e.to_string()))?;
                Ok(CallToolResult::success(vec![Content::text(json)]))
            }
            Err(e) => {
                Err(McpError::internal_error(format!("Emergency pause failed: {}", e)))
            }
        }
    }
}
