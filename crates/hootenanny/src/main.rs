mod api;
mod artifact_store;
mod cas;
mod gpu_monitor;
mod job_system;
mod mcp_tools;
mod persistence;
mod telemetry;
mod types;
mod web;
mod zmq;

use anyhow::{Context, Result};
use audio_graph_mcp::{AudioGraphAdapter, Database as AudioGraphDb};
use clap::Parser;
use api::composite::CompositeHandler;
use api::handler::HootHandler;
use api::service::EventDualityServer;
use mcp_tools::local_models::LocalModels;
use cas::FileStore;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

use llm_mcp_bridge::{AgentChatHandler, AgentManager, BackendConfig, BridgeConfig};
use llmchat::ConversationDb;

/// The Hootenanny MCP Server
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Cli {
    /// The directory to store the journal and other state.
    /// Sled databases are single-writer, so each instance needs its own directory.
    #[arg(short, long)]
    state_dir: Option<PathBuf>,

    /// Port to listen on
    #[arg(short, long, default_value = "8080")]
    port: u16,

    /// Orpheus Model Port
    #[arg(long, default_value = "2000")]
    orpheus_port: u16,

    /// LLM Model Port (vLLM OpenAI-compatible API, e.g. Qwen)
    #[arg(long, default_value = "2020")]
    llm_port: u16,

    /// OTLP gRPC endpoint for OpenTelemetry (e.g., "127.0.0.1:35991")
    #[arg(long, default_value = "127.0.0.1:35991")]
    otlp_endpoint: String,

    /// Connect to chaosgarden daemon at this endpoint (e.g., "tcp://127.0.0.1:5555" or "local" for IPC)
    #[arg(long)]
    chaosgarden: Option<String>,

    /// Bind a hooteproto ZMQ ROUTER for holler gateway (e.g., "tcp://0.0.0.0:5580")
    #[arg(long)]
    zmq_bind: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize OpenTelemetry with OTLP exporter
    telemetry::init(&cli.otlp_endpoint)
        .context("Failed to initialize OpenTelemetry")?;

    // Determine state directory - default to persistent location
    let state_dir = cli.state_dir.unwrap_or_else(|| {
        if let Ok(home) = std::env::var("HOME") {
            PathBuf::from(home).join(".local/share/hrmcp")
        } else {
            PathBuf::from("/tank/halfremembered/hrmcp/default")
        }
    });

    std::fs::create_dir_all(&state_dir).context("Failed to create state directory")?;
    tracing::info!("Using state directory: {}", state_dir.display());

    // --- CAS Initialization ---
    tracing::info!("📦 Initializing Content Addressable Storage (CAS)...");
    let cas_dir = state_dir.join("cas");
    std::fs::create_dir_all(&cas_dir)?;
    let cas = FileStore::at_path(&cas_dir)?;
    tracing::info!("   CAS ready at: {}", cas_dir.display());

    // --- Local Models Initialization ---
    tracing::info!("🤖 Initializing Local Models client...");
    let local_models = Arc::new(LocalModels::new(
        cas.clone(),
        cli.orpheus_port,
    ));
    tracing::info!("   Orpheus: port {}", cli.orpheus_port);

    // --- Artifact Store Initialization ---
    tracing::info!("📚 Initializing Artifact Store...");
    let artifact_store_path = state_dir.join("artifacts.json");
    let artifact_store = Arc::new(RwLock::new(
        artifact_store::FileStore::new(&artifact_store_path)
            .context("Failed to initialize artifact store")?
    ));
    tracing::info!("   Artifact store at: {}", artifact_store_path.display());

    // --- Job Store Initialization ---
    tracing::info!("⚙️  Initializing shared Job Store...");
    let job_store = Arc::new(job_system::JobStore::new());
    tracing::info!("   Job store ready (shared across connections)");

    // --- GPU Observer Client ---
    tracing::info!("🎮 Initializing GPU observer client...");
    let gpu_monitor = Arc::new(gpu_monitor::GpuMonitor::new());
    tracing::info!("   GPU observer client ready (localhost:2099)");

    // --- Audio Graph Initialization ---
    tracing::info!("🎛️  Initializing Audio Graph...");
    let audio_graph_db = Arc::new(AudioGraphDb::in_memory().context("Failed to create audio graph db")?);

    // Create artifact source wrapper for Trustfall adapter
    let artifact_source = Arc::new(artifact_store::FileStoreSource::new(artifact_store.clone()));

    let graph_adapter = Arc::new(
        AudioGraphAdapter::new_with_artifacts(
            audio_graph_db.clone(),
            audio_graph_mcp::PipeWireSnapshot::default(),
            artifact_source,
        )
        .context("Failed to create audio graph adapter")?
    );
    tracing::info!("   Audio graph ready (in-memory, with Trustfall adapter + artifacts)");

    // --- Chaosgarden Connection (optional) ---
    let garden_manager: Option<Arc<zmq::GardenManager>> = if let Some(endpoint) = &cli.chaosgarden {
        tracing::info!("🌱 Connecting to chaosgarden...");
        let manager = if endpoint == "local" {
            zmq::GardenManager::local()
        } else if endpoint.starts_with("tcp://") {
            // Parse tcp://host:port
            let parts: Vec<&str> = endpoint.trim_start_matches("tcp://").split(':').collect();
            if parts.len() == 2 {
                let host = parts[0];
                let port: u16 = parts[1].parse().context("Invalid port in chaosgarden endpoint")?;
                zmq::GardenManager::tcp(host, port)
            } else {
                anyhow::bail!("Invalid TCP endpoint format, expected tcp://host:port");
            }
        } else {
            anyhow::bail!("Invalid chaosgarden endpoint, use 'local' or 'tcp://host:port'");
        };

        match manager.connect().await {
            Ok(()) => {
                tracing::info!("   Connected to chaosgarden!");
                // Optionally start event listener
                if let Err(e) = manager.start_event_listener().await {
                    tracing::warn!("   Failed to start event listener: {}", e);
                }
                Some(Arc::new(manager))
            }
            Err(e) => {
                tracing::warn!("   Failed to connect to chaosgarden: {}", e);
                tracing::warn!("   Continuing without chaosgarden connection");
                None
            }
        }
    } else {
        None
    };

    let addr = format!("0.0.0.0:{}", cli.port);

    // --- Hooteproto ZMQ Server (optional, for holler gateway) ---
    let zmq_shutdown_tx: Option<tokio::sync::broadcast::Sender<()>> = if let Some(ref zmq_bind) = cli.zmq_bind {
        tracing::info!("📡 Starting hooteproto ZMQ server...");
        let (shutdown_tx, shutdown_rx) = tokio::sync::broadcast::channel::<()>(1);
        let zmq_server = zmq::HooteprotoServer::new(
            zmq_bind.clone(),
            Arc::new(cas.clone()),
            artifact_store.clone(),
        );
        tokio::spawn(async move {
            if let Err(e) = zmq_server.run(shutdown_rx).await {
                tracing::error!("ZMQ server error: {}", e);
            }
        });
        tracing::info!("   ZMQ ROUTER: {}", zmq_bind);
        Some(shutdown_tx)
    } else {
        None
    };

    tracing::info!("🎵 Event Duality Server starting on http://{}", addr);
    tracing::info!("   MCP Streamable HTTP: POST http://{}/mcp (recommended)", addr);
    tracing::info!("   MCP SSE (legacy): GET http://{}/mcp/sse", addr);
    tracing::info!("   Agent Chat: agent_chat_* tools via MCP");
    tracing::info!("   Artifact Content: GET http://{}/artifact/:id", addr);
    tracing::info!("   Artifact Meta: GET http://{}/artifact/:id/meta", addr);
    tracing::info!("   Artifacts List: GET http://{}/artifacts", addr);
    tracing::info!("   Health: GET http://{}/health", addr);
    if cli.zmq_bind.is_some() {
        tracing::info!("   ZMQ hooteproto: {} (for holler)", cli.zmq_bind.as_ref().unwrap());
    }

    // Create the EventDualityServer
    let event_duality_server = Arc::new(EventDualityServer::new(
        local_models.clone(),
        artifact_store.clone(),
        job_store.clone(),
        audio_graph_db.clone(),
        graph_adapter.clone(),
        gpu_monitor.clone(),
    ).with_garden(garden_manager.clone()));

    // --- LLM Agent Bridge Initialization ---
    tracing::info!("🤖 Initializing LLM Agent Bridge...");
    let conversations_db_path = state_dir.join("conversations.db");
    let conversations_db = ConversationDb::open(&conversations_db_path)
        .context("Failed to open conversations database")?;
    tracing::info!("   Conversations DB: {}", conversations_db_path.display());

    let bridge_config = BridgeConfig {
        mcp_url: format!("http://127.0.0.1:{}/mcp", cli.port),
        backends: vec![
            BackendConfig {
                id: "qwen".to_string(),
                display_name: "Qwen 2.5 Instruct".to_string(),
                base_url: format!("http://127.0.0.1:{}/v1", cli.llm_port),
                api_key: None,
                default_model: "Qwen2.5-7B-Instruct".to_string(),
                summary_model: None,
                supports_tools: true,
                max_tokens: Some(4096),
                default_temperature: Some(0.7),
            },
        ],
    };
    tracing::info!("   Qwen backend: http://127.0.0.1:{}/v1", cli.llm_port);

    let agent_manager = Arc::new(
        AgentManager::new(bridge_config, conversations_db)
            .context("Failed to create agent manager")?
    );
    let agent_handler = AgentChatHandler::new(agent_manager);

    // Create baton MCP handler and state (composite of Hoot + Agent)
    let hoot_handler = HootHandler::new(event_duality_server.clone());
    let composite_handler = CompositeHandler::new(hoot_handler, agent_handler);
    let mcp_state = Arc::new(baton::McpState::new(
        composite_handler,
        "hootenanny",
        env!("CARGO_PKG_VERSION"),
    ));

    let shutdown_token = CancellationToken::new();

    // Create routers with their respective state types
    let web_state = web::WebState {
        artifact_store: artifact_store.clone(),
        cas: Arc::new(cas.clone()),
    };
    let artifact_router = web::router(web_state);
    // dual_router supports both Streamable HTTP (POST /) and SSE (GET /sse + POST /message)
    let mcp_router = baton::dual_router(mcp_state.clone());

    // Track server start time for uptime
    let server_start = Instant::now();

    // Handler for OAuth discovery - return 404 with JSON to indicate no OAuth required
    async fn no_oauth() -> impl axum::response::IntoResponse {
        (
            axum::http::StatusCode::NOT_FOUND,
            [(axum::http::header::CONTENT_TYPE, "application/json")],
            r#"{"error": "not_found", "error_description": "This MCP server does not require authentication"}"#
        )
    }

    // Health endpoint state
    #[derive(Clone)]
    struct HealthState {
        job_store: Arc<job_system::JobStore>,
        sessions: Arc<dyn baton::SessionStore>,
        start_time: Instant,
    }

    async fn health_handler(
        axum::extract::State(state): axum::extract::State<HealthState>,
    ) -> axum::Json<serde_json::Value> {
        let job_stats = state.job_store.stats();
        let session_stats = state.sessions.stats();
        let uptime = state.start_time.elapsed();

        axum::Json(serde_json::json!({
            "status": "healthy",
            "uptime_secs": uptime.as_secs(),
            "version": env!("CARGO_PKG_VERSION"),
            "sessions": {
                "total": session_stats.total,
                "connected": session_stats.connected,
            },
            "jobs": {
                "pending": job_stats.pending,
                "running": job_stats.running,
            }
        }))
    }

    let health_state = HealthState {
        job_store: job_store.clone(),
        sessions: mcp_state.sessions.clone(),
        start_time: server_start,
    };

    // Build the main application router
    // Each sub-router has its own state type, so we use nest() for CAS
    let health_router = axum::Router::new()
        .route("/health", axum::routing::get(health_handler))
        .with_state(health_state);

    let app_router = axum::Router::new()
        .merge(health_router)
        .route("/mcp/.well-known/oauth-authorization-server", axum::routing::get(no_oauth))
        .route("/mcp/.well-known/oauth-protected-resource", axum::routing::get(no_oauth))
        .route("/.well-known/oauth-authorization-server", axum::routing::get(no_oauth))
        .route("/.well-known/oauth-protected-resource", axum::routing::get(no_oauth))
        .nest("/mcp", mcp_router)
        .merge(artifact_router);

    let bind_addr: std::net::SocketAddr = addr.parse().context("Failed to parse bind address")?;
    let listener = tokio::net::TcpListener::bind(bind_addr).await?;

    tracing::info!("🌐 Router created, starting server...");

    let shutdown_token_srv = shutdown_token.clone();
    let server = axum::serve(listener, app_router).with_graceful_shutdown(async move {
        shutdown_token_srv.cancelled().await;
        tracing::info!("Server shutdown signal received");
    });

    tokio::spawn(async move {
        if let Err(e) = server.await {
            tracing::error!("Server shutdown with error: {:?}", e);
        }
    });

    tracing::info!("🎵 Server ready. Let's dance!");

    // Spawn background task for periodic statistics logging
    let stats_job_store = job_store.clone();
    let stats_sessions = mcp_state.sessions.clone();
    let stats_ct = shutdown_token.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    let job_stats = stats_job_store.stats();
                    let session_stats = stats_sessions.stats();
                    tracing::info!(
                        jobs.total = job_stats.total,
                        jobs.pending = job_stats.pending,
                        jobs.running = job_stats.running,
                        jobs.completed = job_stats.completed,
                        jobs.failed = job_stats.failed,
                        jobs.cancelled = job_stats.cancelled,
                        sessions.total = session_stats.total,
                        sessions.connected = session_stats.connected,
                        sessions.disconnected = session_stats.disconnected,
                        "Server statistics"
                    );
                }
                _ = stats_ct.cancelled() => {
                    break;
                }
            }
        }
    });

    // Spawn background task for session cleanup
    baton::spawn_cleanup_task(
        mcp_state.sessions.clone(),
        Duration::from_secs(30),   // cleanup interval
        Duration::from_secs(1800), // 30 min max idle
        shutdown_token.clone(),
    );

    // Handle both SIGINT (Ctrl+C) and SIGTERM (cargo-watch, systemd, etc.)
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("Received SIGINT (Ctrl+C), shutting down gracefully...");
            shutdown_token.cancel();
        }
        _ = async {
            #[cfg(unix)]
            {
                use tokio::signal::unix::{signal, SignalKind};
                let mut sigterm = signal(SignalKind::terminate()).expect("Failed to setup SIGTERM handler");
                sigterm.recv().await;
            }
            #[cfg(not(unix))]
            {
                std::future::pending::<()>().await;
            }
        } => {
            tracing::info!("Received SIGTERM, shutting down gracefully...");
            shutdown_token.cancel();
        }
    }

    // Signal ZMQ server shutdown if running
    if let Some(zmq_shutdown) = zmq_shutdown_tx {
        let _ = zmq_shutdown.send(());
    }

    tracing::info!("Shutdown complete");

    // Shutdown OpenTelemetry and flush remaining spans
    telemetry::shutdown()?;

    Ok(())
}
