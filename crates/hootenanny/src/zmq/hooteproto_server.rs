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

use anyhow::{Context as AnyhowContext, Result};
use bytes::Bytes;
use cas::ContentStore;
use hooteproto::{
    capnp_envelope_to_payload, envelope_capnp, payload_to_capnp_envelope, Command, ContentType,
    HootFrame, Payload, ToolInfo, PROTOCOL_VERSION,
};
use rzmq::{Context, Msg, MsgFlags, Socket, SocketType};
use rzmq::socket::options::LINGER;
use std::sync::{Arc, RwLock};
use std::time::Instant;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn, Instrument};
use tracing_opentelemetry::OpenTelemetrySpanExt;
use uuid::Uuid;

use crate::api::service::EventDualityServer;
use crate::artifact_store::{self, ArtifactStore as _};
use crate::cas::FileStore;
use crate::telemetry;
use crate::zmq::client_tracker::ClientTracker;
use crate::zmq::LuanetteClient;
use crate::zmq::VibeweaverClient;

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
    /// Optional vibeweaver client for Python kernel proxy
    vibeweaver: Option<Arc<VibeweaverClient>>,
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
            vibeweaver: None,
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
            vibeweaver: None,
            client_tracker: Arc::new(ClientTracker::new()),
        }
    }

    /// Add luanette client for Lua scripting proxy
    pub fn with_luanette(mut self, luanette: Option<Arc<LuanetteClient>>) -> Self {
        self.luanette = luanette;
        self
    }

    /// Add vibeweaver client for Python kernel proxy
    pub fn with_vibeweaver(mut self, vibeweaver: Option<Arc<VibeweaverClient>>) -> Self {
        self.vibeweaver = vibeweaver;
        self
    }

    /// Get the client tracker for monitoring connected clients
    pub fn client_tracker(&self) -> Arc<ClientTracker> {
        Arc::clone(&self.client_tracker)
    }

    /// Run the server until shutdown signal
    ///
    /// Uses concurrent request handling to avoid deadlocks when proxied services
    /// (like vibeweaver) call back to hootenanny during request processing.
    pub async fn run(self, mut shutdown_rx: tokio::sync::broadcast::Receiver<()>) -> Result<()> {
        let context = Context::new()
            .with_context(|| "Failed to create ZMQ context")?;
        let socket = context
            .socket(SocketType::Router)
            .with_context(|| "Failed to create ROUTER socket")?;

        // Set LINGER to 0 for immediate close
        if let Err(e) = socket.set_option_raw(LINGER, &0i32.to_ne_bytes()).await {
            warn!("Failed to set LINGER: {}", e);
        }

        socket
            .bind(&self.bind_address)
            .await
            .with_context(|| format!("Failed to bind to {}", self.bind_address))?;

        info!("Hootenanny ZMQ server listening on {}", self.bind_address);

        // Channel for sending responses back to the main loop for transmission
        let (response_tx, mut response_rx) = mpsc::channel::<Vec<Msg>>(256);

        // Wrap self in Arc for sharing across spawned tasks
        let server = Arc::new(self);

        loop {
            tokio::select! {
                // Receive incoming messages
                result = socket.recv_multipart() => {
                    match result {
                        Ok(msgs) => {
                            let frames: Vec<Bytes> = msgs
                                .iter()
                                .map(|m| Bytes::from(m.data().map(|d| d.to_vec()).unwrap_or_default()))
                                .collect();

                            // Check for HOOT01 protocol
                            if !frames.iter().any(|f| f.as_ref() == PROTOCOL_VERSION) {
                                warn!("Received non-HOOT01 message, rejecting");
                                continue;
                            }

                            // Parse frame to check command type
                            match HootFrame::from_frames_with_identity(&frames) {
                                Ok((identity, frame)) => {
                                    debug!(
                                        "HOOT01 {:?} from service={} request_id={}",
                                        frame.command, frame.service, frame.request_id
                                    );

                                    match frame.command {
                                        Command::Heartbeat => {
                                            // Handle heartbeats synchronously (fast path)
                                            if let Some(client_id) = identity.first() {
                                                server.client_tracker.record_activity(client_id).await;
                                            }
                                            let response = HootFrame::heartbeat("hootenanny");
                                            let reply_frames = response.to_frames_with_identity(&identity);
                                            let reply = frames_to_msgs(&reply_frames);
                                            // Use individual send() - rzmq ROUTER send_multipart has a bug
                                            if let Err(e) = send_multipart_individually(&socket, reply).await {
                                                error!("Failed to send heartbeat response: {}", e);
                                            }
                                        }
                                        Command::Request => {
                                            // Spawn async task for request handling (allows concurrency)
                                            let server_clone = Arc::clone(&server);
                                            let tx = response_tx.clone();
                                            tokio::spawn(async move {
                                                let reply = server_clone.handle_request(identity, frame).await;
                                                if let Err(e) = tx.send(reply).await {
                                                    error!("Failed to queue response: {}", e);
                                                }
                                            });
                                        }
                                        Command::Ready => {
                                            // Register client for bidirectional heartbeating
                                            let service = frame.service.clone();
                                            if let Some(client_id) = identity.first() {
                                                server.client_tracker
                                                    .register(client_id.clone(), service.clone())
                                                    .await;
                                            }
                                            info!("Client registered: service={}", service);
                                        }
                                        Command::Disconnect => {
                                            // Remove client from tracker
                                            if let Some(client_id) = identity.first() {
                                                server.client_tracker.remove(client_id).await;
                                            }
                                            info!("Client disconnected: service={}", frame.service);
                                        }
                                        Command::Reply => {
                                            // Unexpected - we're the server, we shouldn't receive replies
                                            warn!("Unexpected Reply command received at server");
                                        }
                                    }
                                }
                                Err(e) => {
                                    error!("Failed to parse HOOT01 frame: {}", e);
                                }
                            }
                        }
                        Err(e) => {
                            error!("Error receiving message: {}", e);
                        }
                    }
                }

                // Send queued responses
                Some(reply) = response_rx.recv() => {
                    // Use individual send() - rzmq ROUTER send_multipart has a bug
                    if let Err(e) = send_multipart_individually(&socket, reply).await {
                        error!("Failed to send response: {}", e);
                    }
                }

                // Handle shutdown
                _ = shutdown_rx.recv() => {
                    info!("Shutdown signal received");
                    break;
                }
            }
        }

        Ok(())
    }

    /// Handle a request and return the ZMQ message to send as response
    async fn handle_request(&self, identity: Vec<Bytes>, frame: HootFrame) -> Vec<Msg> {
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

        let response_payload = self.dispatch_request(&frame).instrument(span).await;

        // Build response frame
        let response_msg = match payload_to_capnp_envelope(frame.request_id, &response_payload) {
            Ok(msg) => msg,
            Err(e) => {
                error!("Failed to encode response: {}", e);
                let err_payload = Payload::Error {
                    code: "encode_error".to_string(),
                    message: e.to_string(),
                    details: None,
                };
                payload_to_capnp_envelope(frame.request_id, &err_payload)
                    .expect("Error payload encoding should not fail")
            }
        };

        let bytes = capnp::serialize::write_message_to_words(&response_msg);
        let response_frame = HootFrame {
            command: Command::Reply,
            content_type: ContentType::CapnProto,
            request_id: frame.request_id,
            service: "hootenanny".to_string(),
            traceparent: None,
            body: bytes.into(),
        };

        let reply_frames = response_frame.to_frames_with_identity(&identity);
        frames_to_msgs(&reply_frames)
    }

    /// Dispatch a request and return the response payload
    async fn dispatch_request(&self, frame: &HootFrame) -> Payload {
        match frame.content_type {
            ContentType::CapnProto => {
                let payload_result: Result<Payload, String> = match frame.read_capnp() {
                    Ok(reader) => match reader.get_root::<envelope_capnp::envelope::Reader>() {
                        Ok(envelope_reader) => {
                            capnp_envelope_to_payload(envelope_reader).map_err(|e| e.to_string())
                        }
                        Err(e) => Err(e.to_string()),
                    },
                    Err(e) => Err(e.to_string()),
                };

                match payload_result {
                    Ok(payload) => self.dispatch(payload).await,
                    Err(e) => {
                        error!("Failed to parse capnp envelope: {}", e);
                        Payload::Error {
                            code: "capnp_parse_error".to_string(),
                            message: e,
                            details: None,
                        }
                    }
                }
            }
            other => {
                error!("Unsupported content type: {:?}", other);
                Payload::Error {
                    code: "unsupported_content_type".to_string(),
                    message: format!("{:?}", other),
                    details: None,
                }
            }
        }
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

        // Route weave_* payloads to vibeweaver if connected
        if self.should_route_to_vibeweaver(&payload) {
            if let Some(ref vibeweaver) = self.vibeweaver {
                return self.dispatch_via_vibeweaver(vibeweaver, payload).await;
            } else {
                return Payload::Error {
                    code: "vibeweaver_not_connected".to_string(),
                    message: "Python kernel requires vibeweaver connection. Configure bootstrap.connections.vibeweaver in config.".to_string(),
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

        match luanette.request(payload).await {
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

    /// Check if a payload should be routed to vibeweaver
    fn should_route_to_vibeweaver(&self, payload: &Payload) -> bool {
        // weave_* tools go to vibeweaver for Python kernel execution
        // These are ToolCall payloads with weave_* tool names
        if let Payload::ToolCall { name, .. } = payload {
            return name.starts_with("weave_");
        }
        false
    }

    /// Dispatch a payload to vibeweaver via ZMQ proxy
    async fn dispatch_via_vibeweaver(
        &self,
        vibeweaver: &VibeweaverClient,
        payload: Payload,
    ) -> Payload {
        debug!("Proxying to vibeweaver: {}", payload_type_name(&payload));

        match vibeweaver.request(payload).await {
            Ok(response) => response,
            Err(e) => {
                warn!("Vibeweaver proxy error: {}", e);
                Payload::Error {
                    code: "vibeweaver_proxy_error".to_string(),
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
        Payload::GardenAttachAudio { .. } => "garden_attach_audio",
        Payload::GardenDetachAudio => "garden_detach_audio",
        Payload::GardenAudioStatus => "garden_audio_status",
        Payload::GardenAttachInput { .. } => "garden_attach_input",
        Payload::GardenDetachInput => "garden_detach_input",
        Payload::GardenInputStatus => "garden_input_status",
        Payload::GardenSetMonitor { .. } => "garden_set_monitor",
        Payload::GetToolHelp { .. } => "get_tool_help",
        Payload::ToolHelpList { .. } => "tool_help_list",
        Payload::Schedule { .. } => "schedule",
        Payload::Analyze { .. } => "analyze",
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

/// Convert a Vec<Bytes> to Vec<Msg> for rzmq multipart
fn frames_to_msgs(frames: &[Bytes]) -> Vec<Msg> {
    frames.iter().map(|f| Msg::from_vec(f.to_vec())).collect()
}

/// Send a multipart message using individual send() calls with MORE flags.
///
/// rzmq's ROUTER socket has a bug in send_multipart that drops frames.
/// This workaround sends each frame individually with the MORE flag set
/// for all but the last frame.
async fn send_multipart_individually(socket: &Socket, msgs: Vec<Msg>) -> Result<()> {
    let last_idx = msgs.len().saturating_sub(1);
    for (i, mut msg) in msgs.into_iter().enumerate() {
        if i < last_idx {
            msg.set_flags(MsgFlags::MORE);
        }
        socket.send(msg).await
            .with_context(|| format!("Failed to send frame {} of multipart", i))?;
    }
    Ok(())
}
