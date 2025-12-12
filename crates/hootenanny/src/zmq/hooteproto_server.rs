//! ZMQ ROUTER server for Hootenanny using hooteproto
//!
//! Exposes all hootenanny tools over ZMQ for Holler to route to.
//! This is the primary interface for the MCP-over-ZMQ architecture where
//! holler acts as the MCP gateway and hootenanny handles the actual tool execution.

use anyhow::{Context, Result};
use cas::ContentStore;
use hooteproto::{Envelope, Payload, ToolInfo};
use std::sync::{Arc, RwLock};
use std::time::Instant;
use tracing::{debug, error, info, warn, Instrument};
use tracing_opentelemetry::OpenTelemetrySpanExt;
use uuid::Uuid;
use zeromq::{RouterSocket, Socket, SocketRecv, SocketSend, ZmqMessage};

use crate::api::service::EventDualityServer;
use crate::artifact_store::{self, ArtifactStore as _};
use crate::cas::FileStore;
use crate::telemetry;

/// ZMQ server for hooteproto messages
///
/// Can operate in two modes:
/// 1. Standalone - with direct CAS/artifact access (legacy, for basic operations)
/// 2. Full - with EventDualityServer for complete tool dispatch
pub struct HooteprotoServer {
    bind_address: String,
    cas: Arc<FileStore>,
    artifacts: Arc<RwLock<artifact_store::FileStore>>,
    start_time: Instant,
    /// Optional EventDualityServer for full tool dispatch
    event_server: Option<Arc<EventDualityServer>>,
}

impl HooteprotoServer {
    /// Create a new server in standalone mode (CAS + artifacts only)
    pub fn new(
        bind_address: String,
        cas: Arc<FileStore>,
        artifacts: Arc<RwLock<artifact_store::FileStore>>,
    ) -> Self {
        Self {
            bind_address,
            cas,
            artifacts,
            start_time: Instant::now(),
            event_server: None,
        }
    }

    /// Create a new server with full tool dispatch via EventDualityServer
    pub fn with_event_server(
        bind_address: String,
        cas: Arc<FileStore>,
        artifacts: Arc<RwLock<artifact_store::FileStore>>,
        event_server: Arc<EventDualityServer>,
    ) -> Self {
        Self {
            bind_address,
            cas,
            artifacts,
            start_time: Instant::now(),
            event_server: Some(event_server),
        }
    }

    /// Run the server until shutdown signal
    pub async fn run(self, mut shutdown_rx: tokio::sync::broadcast::Receiver<()>) -> Result<()> {
        let mut socket = RouterSocket::new();
        socket
            .bind(&self.bind_address)
            .await
            .with_context(|| format!("Failed to bind to {}", self.bind_address))?;

        info!("Hootenanny ZMQ server listening on {}", self.bind_address);

        loop {
            tokio::select! {
                result = socket.recv() => {
                    match result {
                        Ok(msg) => {
                            if let Err(e) = self.handle_message(&mut socket, msg).await {
                                error!("Error handling message: {}", e);
                            }
                        }
                        Err(e) => {
                            error!("Error receiving message: {}", e);
                        }
                    }
                }
                _ = shutdown_rx.recv() => {
                    info!("Shutdown signal received");
                    break;
                }
            }
        }

        Ok(())
    }

    async fn handle_message(&self, socket: &mut RouterSocket, msg: ZmqMessage) -> Result<()> {
        let identity = msg.get(0).context("Missing identity frame")?.to_vec();
        let payload_bytes = msg.get(1).context("Missing payload frame")?;
        
        // Debug log size instead of content for binary protocols
        debug!("Received {} bytes", payload_bytes.len());

        let envelope: Envelope = rmp_serde::from_slice(payload_bytes)
            .with_context(|| "Failed to parse MsgPack envelope")?;

        // Create a span with the incoming traceparent as the parent
        let span = tracing::info_span!(
            "zmq_request",
            otel.name = payload_type_name(&envelope.payload),
            message_id = %envelope.id,
        );

        // If we have a traceparent, set it as the parent context
        if let Some(parent_ctx) = telemetry::parse_traceparent(envelope.traceparent.as_deref()) {
            span.set_parent(parent_ctx);
        }

        // Execute dispatch within the span
        let response_payload = self.dispatch(envelope.payload).instrument(span).await;

        let response = Envelope {
            id: envelope.id,
            traceparent: envelope.traceparent,
            payload: response_payload,
        };

        let response_bytes = rmp_serde::to_vec(&response)?;
        debug!("Sending {} bytes", response_bytes.len());

        let mut reply = ZmqMessage::from(identity);
        reply.push_back(response_bytes.into());
        socket.send(reply).await?;

        Ok(())
    }

    async fn dispatch(&self, payload: Payload) -> Payload {
        // Handle administrative messages first
        match &payload {
            Payload::Ping => {
                return Payload::Pong {
                    worker_id: Uuid::new_v4(),
                    uptime_secs: self.start_time.elapsed().as_secs(),
                };
            }
            Payload::ListTools => {
                return self.list_tools();
            }
            _ => {}
        }

        // If we have an EventDualityServer, route through it for full functionality
        if let Some(ref server) = self.event_server {
            return self.dispatch_via_server(server, payload).await;
        }

        // Fallback to standalone mode for basic CAS/artifact operations
        match payload {
            Payload::CasStore { data, mime_type } => self.cas_store(data, Some(mime_type)).await,
            Payload::CasInspect { hash } => self.cas_inspect(&hash).await,
            Payload::CasGet { hash } => self.cas_get(&hash).await,

            Payload::ArtifactList { tag, creator } => self.artifact_list(tag, creator).await,
            Payload::ArtifactGet { id } => self.artifact_get(&id).await,

            other => {
                warn!("Unhandled payload in standalone mode: {:?}", payload_type_name(&other));
                Payload::Error {
                    code: "not_implemented".to_string(),
                    message: format!(
                        "Tool '{}' requires EventDualityServer. Start hootenanny with full services.",
                        payload_type_name(&other)
                    ),
                    details: None,
                }
            }
        }
    }

    /// Dispatch via EventDualityServer for full tool functionality
    async fn dispatch_via_server(&self, server: &EventDualityServer, payload: Payload) -> Payload {
        use crate::api::dispatch::{dispatch_tool, DispatchError};

        // Convert Payload to tool name and arguments
        let (tool_name, args) = match payload_to_tool_args(payload) {
            Ok(v) => v,
            Err(e) => {
                return Payload::Error {
                    code: "conversion_error".to_string(),
                    message: e.to_string(),
                    details: None,
                };
            }
        };

        // Call the tool via dispatch
        match dispatch_tool(server, &tool_name, args).await {
            Ok(result) => Payload::Success { result },
            Err(DispatchError { code, message, details }) => Payload::Error {
                code,
                message,
                details,
            },
        }
    }

    async fn cas_store(&self, data: Vec<u8>, mime_type: Option<String>) -> Payload {
        let mime = mime_type.as_deref().unwrap_or("application/octet-stream");
        match self.cas.store(&data, mime) {
            Ok(hash) => Payload::Success {
                result: serde_json::json!({
                    "hash": hash.to_string(),
                    "size": data.len(),
                    "mime_type": mime,
                }),
            },
            Err(e) => Payload::Error {
                code: "cas_store_error".to_string(),
                message: e.to_string(),
                details: None,
            },
        }
    }

    async fn cas_inspect(&self, hash: &str) -> Payload {
        let content_hash: cas::ContentHash = match hash.parse() {
            Ok(h) => h,
            Err(e) => {
                return Payload::Error {
                    code: "invalid_hash".to_string(),
                    message: format!("{}", e),
                    details: None,
                }
            }
        };

        match self.cas.retrieve(&content_hash) {
            Ok(Some(data)) => {
                let preview = if data.len() <= 100 {
                    String::from_utf8_lossy(&data).to_string()
                } else {
                    format!(
                        "{}... ({} bytes total)",
                        String::from_utf8_lossy(&data[..100]),
                        data.len()
                    )
                };

                Payload::Success {
                    result: serde_json::json!({
                        "hash": hash,
                        "size": data.len(),
                        "preview": preview,
                        "exists": true,
                    }),
                }
            }
            Ok(None) => Payload::Success {
                result: serde_json::json!({
                    "hash": hash,
                    "exists": false,
                }),
            },
            Err(e) => Payload::Error {
                code: "cas_inspect_error".to_string(),
                message: e.to_string(),
                details: None,
            },
        }
    }

    async fn cas_get(&self, hash: &str) -> Payload {
        let content_hash: cas::ContentHash = match hash.parse() {
            Ok(h) => h,
            Err(e) => {
                return Payload::Error {
                    code: "invalid_hash".to_string(),
                    message: format!("{}", e),
                    details: None,
                }
            }
        };

        match self.cas.retrieve(&content_hash) {
            Ok(Some(data)) => {
                use base64::Engine;
                Payload::Success {
                    result: serde_json::json!({
                        "hash": hash,
                        "size": data.len(),
                        "data": base64::engine::general_purpose::STANDARD.encode(&data),
                    }),
                }
            }
            Ok(None) => Payload::Error {
                code: "not_found".to_string(),
                message: format!("Hash not found: {}", hash),
                details: None,
            },
            Err(e) => Payload::Error {
                code: "cas_get_error".to_string(),
                message: e.to_string(),
                details: None,
            },
        }
    }

    async fn artifact_list(&self, tag: Option<String>, creator: Option<String>) -> Payload {
        let store = self.artifacts.read().unwrap();
        match store.all() {
            Ok(all_artifacts) => {
                let artifacts: Vec<_> = all_artifacts
                    .into_iter()
                    .filter(|a| {
                        let tag_match = tag.as_ref().is_none_or(|t| a.tags.iter().any(|at| at == t));
                        let creator_match = creator.as_ref().is_none_or(|c| a.creator.as_str() == c);
                        tag_match && creator_match
                    })
                    .collect();

                Payload::Success {
                    result: serde_json::to_value(&artifacts).unwrap_or_default(),
                }
            }
            Err(e) => Payload::Error {
                code: "artifact_list_error".to_string(),
                message: e.to_string(),
                details: None,
            },
        }
    }

    async fn artifact_get(&self, id: &str) -> Payload {
        let store = self.artifacts.read().unwrap();
        match store.get(id) {
            Ok(Some(artifact)) => Payload::Success {
                result: serde_json::to_value(&artifact).unwrap_or_default(),
            },
            Ok(None) => Payload::Error {
                code: "not_found".to_string(),
                message: format!("Artifact not found: {}", id),
                details: None,
            },
            Err(e) => Payload::Error {
                code: "artifact_get_error".to_string(),
                message: e.to_string(),
                details: None,
            },
        }
    }

    fn list_tools(&self) -> Payload {
        let tools = vec![
            ToolInfo {
                name: "cas_store".to_string(),
                description: "Store content in CAS".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "data": {"type": "string", "description": "Base64 encoded data"},
                        "mime_type": {"type": "string"}
                    },
                    "required": ["data"]
                }),
            },
            ToolInfo {
                name: "cas_inspect".to_string(),
                description: "Inspect content in CAS".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "hash": {"type": "string"}
                    },
                    "required": ["hash"]
                }),
            },
            ToolInfo {
                name: "cas_get".to_string(),
                description: "Get content from CAS".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "hash": {"type": "string"}
                    },
                    "required": ["hash"]
                }),
            },
            ToolInfo {
                name: "artifact_list".to_string(),
                description: "List artifacts".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "tag": {"type": "string"},
                        "creator": {"type": "string"}
                    }
                }),
            },
            ToolInfo {
                name: "artifact_get".to_string(),
                description: "Get artifact by ID".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "id": {"type": "string"}
                    },
                    "required": ["id"]
                }),
            },
        ];

        Payload::ToolList { tools }
    }
}

/// Get a human-readable name for a payload type (for span naming)
fn payload_type_name(payload: &Payload) -> &'static str {
    match payload {
        Payload::Register(_) => "register",
        Payload::Ping => "ping",
        Payload::Pong { .. } => "pong",
        Payload::Shutdown { .. } => "shutdown",
        Payload::LuaEval { .. } => "lua_eval",
        Payload::LuaDescribe { .. } => "lua_describe",
        Payload::ScriptStore { .. } => "script_store",
        Payload::ScriptSearch { .. } => "script_search",
        Payload::JobExecute { .. } => "job_execute",
        Payload::JobStatus { .. } => "job_status",
        Payload::JobPoll { .. } => "job_poll",
        Payload::JobCancel { .. } => "job_cancel",
        Payload::JobList { .. } => "job_list",
        Payload::JobSleep { .. } => "job_sleep",
        Payload::ReadResource { .. } => "read_resource",
        Payload::ListResources => "list_resources",
        Payload::GetPrompt { .. } => "get_prompt",
        Payload::ListPrompts => "list_prompts",
        Payload::Complete { .. } => "complete",
        Payload::TimelineEvent { .. } => "timeline_event",
        Payload::CasStore { .. } => "cas_store",
        Payload::CasInspect { .. } => "cas_inspect",
        Payload::CasGet { .. } => "cas_get",
        Payload::CasUploadFile { .. } => "cas_upload_file",
        Payload::ArtifactUpload { .. } => "artifact_upload",
        Payload::ArtifactGet { .. } => "artifact_get",
        Payload::ArtifactList { .. } => "artifact_list",
        Payload::ArtifactCreate { .. } => "artifact_create",
        Payload::GraphQuery { .. } => "graph_query",
        Payload::GraphBind { .. } => "graph_bind",
        Payload::GraphTag { .. } => "graph_tag",
        Payload::GraphConnect { .. } => "graph_connect",
        Payload::GraphFind { .. } => "graph_find",
        Payload::GraphContext { .. } => "graph_context",
        Payload::AddAnnotation { .. } => "add_annotation",
        Payload::OrpheusGenerate { .. } => "orpheus_generate",
        Payload::OrpheusGenerateSeeded { .. } => "orpheus_generate_seeded",
        Payload::OrpheusContinue { .. } => "orpheus_continue",
        Payload::OrpheusBridge { .. } => "orpheus_bridge",
        Payload::OrpheusLoops { .. } => "orpheus_loops",
        Payload::OrpheusClassify { .. } => "orpheus_classify",
        Payload::ConvertMidiToWav { .. } => "convert_midi_to_wav",
        Payload::SoundfontInspect { .. } => "soundfont_inspect",
        Payload::SoundfontPresetInspect { .. } => "soundfont_preset_inspect",
        Payload::AbcParse { .. } => "abc_parse",
        Payload::AbcToMidi { .. } => "abc_to_midi",
        Payload::AbcValidate { .. } => "abc_validate",
        Payload::AbcTranspose { .. } => "abc_transpose",
        Payload::BeatthisAnalyze { .. } => "beatthis_analyze",
        Payload::ClapAnalyze { .. } => "clap_analyze",
        Payload::MusicgenGenerate { .. } => "musicgen_generate",
        Payload::YueGenerate { .. } => "yue_generate",
        Payload::GardenStatus => "garden_status",
        Payload::GardenPlay => "garden_play",
        Payload::GardenPause => "garden_pause",
        Payload::GardenStop => "garden_stop",
        Payload::GardenSeek { .. } => "garden_seek",
        Payload::GardenSetTempo { .. } => "garden_set_tempo",
        Payload::GardenQuery { .. } => "garden_query",
        Payload::GardenEmergencyPause => "garden_emergency_pause",
        Payload::SampleLlm { .. } => "sample_llm",
        Payload::TransportPlay => "transport_play",
        Payload::TransportStop => "transport_stop",
        Payload::TransportSeek { .. } => "transport_seek",
        Payload::TransportStatus => "transport_status",
        Payload::TimelineQuery { .. } => "timeline_query",
        Payload::TimelineAddMarker { .. } => "timeline_add_marker",
        Payload::ListTools => "list_tools",
        Payload::ToolList { .. } => "tool_list",
        Payload::Success { .. } => "success",
        Payload::Error { .. } => "error",
    }
}

/// Convert a hooteproto Payload to a tool name and JSON arguments
fn payload_to_tool_args(payload: Payload) -> anyhow::Result<(String, serde_json::Value)> {
    // Serialize the payload to JSON, then extract the tool-specific fields
    let json = serde_json::to_value(&payload)?;
    let tool_name = payload_type_name(&payload).to_string();

    // The payload is tagged, so we need to extract the inner object
    // After serialization: {"type":"cas_store","data":"...","mime_type":"..."}
    // We want just: {"data":"...","mime_type":"..."}
    let mut args = json.as_object()
        .cloned()
        .unwrap_or_default();
    args.remove("type");  // Remove the discriminator tag

    Ok((tool_name, serde_json::Value::Object(args)))
}