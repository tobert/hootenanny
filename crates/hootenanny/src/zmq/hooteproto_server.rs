//! ZMQ ROUTER server for Hootenanny using hooteproto
//!
//! Exposes all hootenanny tools over ZMQ for Holler to route to.
//! This is the primary interface for the MCP-over-ZMQ architecture where
//! holler acts as the MCP gateway and hootenanny handles the actual tool execution.
//!
//! Supports both legacy MsgPack envelope format and new HOOT01 frame protocol.
//! The HOOT01 protocol enables:
//! - Routing without deserialization (fixed-width header fields)
//! - Efficient heartbeats (no MsgPack overhead)
//! - Native binary payloads (no base64 encoding)
//!
//! Bidirectional heartbeating:
//! - Tracks connected clients via ClientTracker
//! - Sends heartbeats to clients (holler → hootenanny and hootenanny → holler)
//! - Cleans up stale clients automatically

use anyhow::{Context, Result};
use bytes::Bytes;
use cas::ContentStore;
use hooteproto::{
    capnp_envelope_to_payload, envelope_capnp, payload_to_capnp_envelope, Command, ContentType,
    HootFrame, Payload, ToolInfo, PROTOCOL_VERSION,
};
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
use crate::zmq::client_tracker::ClientTracker;
use crate::zmq::LuanetteClient;

/// ZMQ server for hooteproto messages
///
/// Can operate in two modes:
/// 1. Standalone - with direct CAS/artifact access (legacy, for basic operations)
/// 2. Full - with EventDualityServer for full tool dispatch
///
/// Optionally proxies lua_* payloads to luanette if configured.
/// Tracks connected clients for bidirectional heartbeating.
pub struct HooteprotoServer {
    bind_address: String,
    cas: Arc<FileStore>,
    artifacts: Arc<RwLock<artifact_store::FileStore>>,
    start_time: Instant,
    /// Optional EventDualityServer for full tool dispatch
    event_server: Option<Arc<EventDualityServer>>,
    /// Optional luanette client for Lua scripting proxy
    luanette: Option<Arc<LuanetteClient>>,
    /// Connected client tracker for bidirectional heartbeats
    client_tracker: Arc<ClientTracker>,
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
            luanette: None,
            client_tracker: Arc::new(ClientTracker::new()),
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
            luanette: None,
            client_tracker: Arc::new(ClientTracker::new()),
        }
    }

    /// Add luanette client for Lua scripting proxy
    pub fn with_luanette(mut self, luanette: Option<Arc<LuanetteClient>>) -> Self {
        self.luanette = luanette;
        self
    }

    /// Get the client tracker for monitoring connected clients
    pub fn client_tracker(&self) -> Arc<ClientTracker> {
        Arc::clone(&self.client_tracker)
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
        // Convert ZmqMessage frames to Bytes for parsing
        let frames: Vec<Bytes> = msg.iter().map(|f| Bytes::copy_from_slice(f)).collect();

        // Only accept HOOT01 frame protocol (scan for protocol marker)
        if frames.iter().any(|f| f.as_ref() == PROTOCOL_VERSION) {
            return self.handle_hoot_frame(socket, &frames).await;
        }

        // Reject non-HOOT01 frames
        warn!("Received non-HOOT01 message, rejecting");
        Err(anyhow::anyhow!("Only HOOT01 protocol is supported"))
    }

    /// Handle HOOT01 frame protocol message
    async fn handle_hoot_frame(&self, socket: &mut RouterSocket, frames: &[Bytes]) -> Result<()> {
        let (identity, frame) = HootFrame::from_frames_with_identity(frames)
            .with_context(|| "Failed to parse HOOT01 frame")?;

        debug!(
            "HOOT01 {:?} from service={} request_id={}",
            frame.command, frame.service, frame.request_id
        );

        match frame.command {
            Command::Heartbeat => {
                // Record activity from this client (use first identity frame as key)
                if let Some(client_id) = identity.first() {
                    self.client_tracker.record_activity(client_id).await;
                }

                // Respond immediately with heartbeat
                let response = HootFrame::heartbeat("hootenanny");
                let reply_frames = response.to_frames_with_identity(&identity);
                let reply = frames_to_zmq_message(&reply_frames);
                socket.send(reply).await?;
                debug!("Heartbeat response sent");
            }

            Command::Request => {
                // Create span with traceparent
                let span = tracing::info_span!(
                    "hoot_request",
                    otel.name = "hoot_request",
                    request_id = %frame.request_id,
                    service = %frame.service,
                );

                if let Some(ref tp) = frame.traceparent {
                    if let Some(parent_ctx) = telemetry::parse_traceparent(Some(tp.as_str())) {
                        span.set_parent(parent_ctx);
                    }
                }

                // Dispatch based on content type
                let response = match frame.content_type {
                    ContentType::CapnProto => {
                        // First, parse the entire capnp message to Payload
                        // (must complete before await to avoid holding reader across await point)
                        let payload_result: Result<Payload, capnp::Error> = match frame.read_capnp()
                        {
                            Ok(reader) => {
                                match reader.get_root::<envelope_capnp::envelope::Reader>() {
                                    Ok(envelope_reader) => {
                                        capnp_envelope_to_payload(envelope_reader)
                                    }
                                    Err(e) => Err(e),
                                }
                            }
                            Err(e) => {
                                Err(capnp::Error::failed(format!("Failed to read capnp: {}", e)))
                            }
                        };

                        // Reader is dropped here, now we can safely await
                        match payload_result {
                            Ok(payload) => {
                                // Dispatch the request
                                let result = self.dispatch(payload).instrument(span).await;

                                // Convert response back to capnp
                                match payload_to_capnp_envelope(frame.request_id, &result) {
                                    Ok(response_msg) => {
                                        // Serialize to bytes
                                        let bytes =
                                            capnp::serialize::write_message_to_words(&response_msg);
                                        HootFrame {
                                            command: Command::Reply,
                                            content_type: ContentType::CapnProto,
                                            request_id: frame.request_id,
                                            service: "hootenanny".to_string(),
                                            traceparent: None,
                                            body: bytes.into(),
                                        }
                                    }
                                    Err(e) => {
                                        error!("Failed to convert response to capnp: {}", e);
                                        let error_payload = Payload::Error {
                                            code: "capnp_serialize_error".to_string(),
                                            message: e.to_string(),
                                            details: None,
                                        };
                                        let error_msg = payload_to_capnp_envelope(
                                            frame.request_id,
                                            &error_payload,
                                        )
                                        .expect("Failed to serialize error");
                                        HootFrame::reply_capnp(frame.request_id, &error_msg)
                                    }
                                }
                            }
                            Err(e) => {
                                error!("Failed to parse capnp envelope: {}", e);
                                let error_payload = Payload::Error {
                                    code: "capnp_parse_error".to_string(),
                                    message: e.to_string(),
                                    details: None,
                                };
                                let error_msg =
                                    payload_to_capnp_envelope(frame.request_id, &error_payload)
                                        .expect("Failed to serialize error");
                                HootFrame::reply_capnp(frame.request_id, &error_msg)
                            }
                        }
                    }
                    ContentType::RawBinary | ContentType::Empty | ContentType::Json => {
                        // Not supported - return error
                        let error_payload = Payload::Error {
                            code: "unsupported_content_type".to_string(),
                            message: format!(
                                "Content type {:?} not supported. Use CapnProto.",
                                frame.content_type
                            ),
                            details: None,
                        };
                        let error_msg = payload_to_capnp_envelope(frame.request_id, &error_payload)
                            .expect("Failed to serialize error");
                        HootFrame::reply_capnp(frame.request_id, &error_msg)
                    }
                };

                let reply_frames = response.to_frames_with_identity(&identity);
                let reply = frames_to_zmq_message(&reply_frames);
                socket.send(reply).await?;
            }

            Command::Ready => {
                // Register client for bidirectional heartbeating (use first identity frame)
                let service = frame.service.clone();
                if let Some(client_id) = identity.first() {
                    self.client_tracker
                        .register(client_id.clone(), service.clone())
                        .await;
                }

                info!("Client registered: service={}", service);
            }

            Command::Disconnect => {
                // Remove client from tracker
                if let Some(client_id) = identity.first() {
                    self.client_tracker.remove(client_id).await;
                }
                info!("Client disconnected: service={}", frame.service);
            }

            Command::Reply => {
                // Unexpected - we're the server, we shouldn't receive replies
                warn!("Unexpected Reply command received at server");
            }
        }

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

        // Intercept tools that HooteprotoServer handles directly
        // This ensures they work even if dispatch_tool doesn't implement them
        // CasStore is intercepted here because Payload::CasStore uses `data` field
        // but dispatch_tool expects CasStoreRequest with `content_base64` field
        match &payload {
            Payload::CasStore { data, mime_type } => {
                return self.cas_store(data.clone(), Some(mime_type.clone())).await
            }
            Payload::CasGet { hash } => return self.cas_get(hash).await,
            Payload::ArtifactList { tag, creator } => {
                return self.artifact_list(tag.clone(), creator.clone()).await
            }
            Payload::ArtifactGet { id } => return self.artifact_get(id).await,
            _ => {}
        }

        // Route lua_*, job_*, script_* payloads to luanette if connected
        if self.should_route_to_luanette(&payload) {
            if let Some(ref luanette) = self.luanette {
                return self.dispatch_via_luanette(luanette, payload).await;
            } else {
                return Payload::Error {
                    code: "luanette_not_connected".to_string(),
                    message: "Lua scripting requires luanette connection. Start hootenanny with --luanette tcp://localhost:5570".to_string(),
                    details: None,
                };
            }
        }

        // If we have an EventDualityServer, route through it for full functionality
        if let Some(ref server) = self.event_server {
            return self.dispatch_via_server(server, payload).await;
        }

        // Fallback to standalone mode for basic CAS/artifact operations
        match payload {
            Payload::CasStore { data, mime_type } => self.cas_store(data, Some(mime_type)).await,
            Payload::CasInspect { hash } => self.cas_inspect(&hash).await,
            // (Intercepted above, but here for completeness/fallback if interception logic changes)
            Payload::CasGet { hash } => self.cas_get(&hash).await,
            Payload::ArtifactList { tag, creator } => self.artifact_list(tag, creator).await,
            Payload::ArtifactGet { id } => self.artifact_get(&id).await,

            other => {
                warn!(
                    "Unhandled payload in standalone mode: {:?}",
                    payload_type_name(&other)
                );
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
    ///
    /// Tries the typed dispatch path first (Protocol v2), falling back to
    /// JSON dispatch for tools not yet converted.
    async fn dispatch_via_server(&self, server: &EventDualityServer, payload: Payload) -> Payload {
        use crate::api::dispatch::{dispatch_tool, DispatchError};
        use crate::api::typed_dispatcher::TypedDispatcher;
        use hooteproto::{envelope_to_payload, payload_to_request};

        // Fast path: ToolCall goes directly to name-based dispatch
        // This is the preferred way for holler to call tools
        if let Payload::ToolCall { name, args } = payload {
            debug!("ToolCall dispatch: {}", name);
            return match dispatch_tool(server, &name, args).await {
                Ok(result) => Payload::Success { result },
                Err(DispatchError {
                    code,
                    message,
                    details,
                }) => Payload::Error {
                    code,
                    message,
                    details,
                },
            };
        }

        // Try typed dispatch path first (Protocol v2)
        match payload_to_request(&payload) {
            Ok(Some(request)) => {
                // Typed path available - use TypedDispatcher
                debug!("Using typed dispatch for: {}", request.name());
                let dispatcher = TypedDispatcher::new(std::sync::Arc::new(server.clone()));
                let envelope = dispatcher.dispatch(request).await;
                return envelope_to_payload(envelope);
            }
            Ok(None) => {
                // Tool not yet converted - fall back to JSON dispatch
                debug!(
                    "Falling back to JSON dispatch for: {:?}",
                    payload_type_name(&payload)
                );
            }
            Err(e) => {
                // Conversion error
                return Payload::Error {
                    code: e.code().to_string(),
                    message: e.message().to_string(),
                    details: None,
                };
            }
        }

        // Fall back to legacy JSON dispatch
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
            Err(DispatchError {
                code,
                message,
                details,
            }) => Payload::Error {
                code,
                message,
                details,
            },
        }
    }

    /// Check if a payload should be routed to luanette
    fn should_route_to_luanette(&self, payload: &Payload) -> bool {
        // Only Lua scripting tools go to luanette
        // Job tools (JobStatus, JobPoll, JobCancel, JobList) are handled locally
        // because the job store lives in hootenanny
        matches!(
            payload,
            Payload::LuaEval { .. }
                | Payload::LuaDescribe { .. }
                | Payload::ScriptStore { .. }
                | Payload::ScriptSearch { .. }
                | Payload::JobExecute { .. }
        )
    }

    /// Dispatch a payload to luanette via ZMQ proxy
    async fn dispatch_via_luanette(&self, luanette: &LuanetteClient, payload: Payload) -> Payload {
        debug!("Proxying to luanette: {}", payload_type_name(&payload));

        match luanette.request(payload, None).await {
            Ok(response) => response,
            Err(e) => {
                warn!("Luanette proxy error: {}", e);
                Payload::Error {
                    code: "luanette_proxy_error".to_string(),
                    message: e.to_string(),
                    details: None,
                }
            }
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
                        let tag_match =
                            tag.as_ref().is_none_or(|t| a.tags.iter().any(|at| at == t));
                        let creator_match =
                            creator.as_ref().is_none_or(|c| a.creator.as_str() == c);
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
        // Start with tools from dispatch if available
        let mut tools = if self.event_server.is_some() {
            crate::api::dispatch::list_tools()
        } else {
            vec![]
        };

        // Define basic tools that HooteprotoServer handles directly
        let basic_tools = vec![
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

        // Merge basic tools, avoiding duplicates (prefer dispatch schemas if present)
        for tool in basic_tools {
            if !tools.iter().any(|t| t.name == tool.name) {
                tools.push(tool);
            }
        }

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
        Payload::ToolCall { .. } => "tool_call",
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
        Payload::ConfigGet { .. } => "config_get",
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
        Payload::GardenCreateRegion { .. } => "garden_create_region",
        Payload::GardenDeleteRegion { .. } => "garden_delete_region",
        Payload::GardenMoveRegion { .. } => "garden_move_region",
        Payload::GardenGetRegions { .. } => "garden_get_regions",
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
        Payload::StreamStart { .. } => "stream_start",
        Payload::StreamSwitchChunk { .. } => "stream_switch_chunk",
        Payload::StreamStop { .. } => "stream_stop",
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
    let mut args = json.as_object().cloned().unwrap_or_default();
    args.remove("type"); // Remove the discriminator tag

    Ok((tool_name, serde_json::Value::Object(args)))
}

/// Convert a Vec<Bytes> to a ZmqMessage
fn frames_to_zmq_message(frames: &[Bytes]) -> ZmqMessage {
    // ZmqMessage doesn't implement Default, so we build from first frame
    if frames.is_empty() {
        return ZmqMessage::from(Vec::<u8>::new());
    }

    let mut msg = ZmqMessage::from(frames[0].to_vec());
    for frame in frames.iter().skip(1) {
        msg.push_back(frame.to_vec().into());
    }
    msg
}
