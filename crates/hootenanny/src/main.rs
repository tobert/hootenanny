mod api;
mod artifact_store;
mod cas;
mod conversation;
mod domain;
mod job_system;
mod mcp_tools;
mod persistence;
mod realization;
mod telemetry;
mod web;

use anyhow::{Context, Result};
use audio_graph_mcp::{AudioGraphAdapter, Database as AudioGraphDb};
use clap::Parser;
use persistence::journal::{Journal, SessionEvent};
use api::service::{ConversationState, EventDualityServer};
use cas::Cas;
use mcp_tools::local_models::LocalModels;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;
use tokio_util::sync::CancellationToken;

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

    /// DeepSeek Model Port
    #[arg(long, default_value = "2001")]
    deepseek_port: u16,

    /// OTLP gRPC endpoint for OpenTelemetry (e.g., "127.0.0.1:35991")
    #[arg(long, default_value = "127.0.0.1:35991")]
    otlp_endpoint: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize OpenTelemetry with OTLP exporter
    telemetry::init(&cli.otlp_endpoint)
        .context("Failed to initialize OpenTelemetry")?;

    // Determine state directory - default to persistent location
    let state_dir = cli.state_dir.unwrap_or_else(|| {
        let default_base = if let Ok(home) = std::env::var("HOME") {
            PathBuf::from(home).join(".local/share/hrmcp")
        } else {
            PathBuf::from("/tank/halfremembered/hrmcp/default")
        };
        default_base
    });

    std::fs::create_dir_all(&state_dir).context("Failed to create state directory")?;
    tracing::info!("Using state directory: {}", state_dir.display());

    // --- Persistence / Journal ---
    tracing::info!("🗄️  Initializing sled journal...");
    let journal_dir = state_dir.join("journal");
    std::fs::create_dir_all(&journal_dir)?;
    let mut journal = Journal::new(&journal_dir)?;

    tracing::info!("📝 Writing 'sessionStarted' event...");
    let event = SessionEvent {
        timestamp: SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_nanos() as u64,
        event_type: "sessionStarted".to_string(),
    };
    let event_id = journal.write_session_event(&event)?;
    tracing::info!("   ✅ Event written with ID: {}", event_id);

    tracing::info!("📖 Reading all events from journal...");
    let events = journal.read_events()?;
    tracing::info!("   Found {} event(s) in the journal.", events.len());

    for (i, event) in events.iter().enumerate() {
        tracing::info!(
            "   Event {}: timestamp={} type={}",
            i,
            event.timestamp,
            event.event_type
        );
    }

    journal.flush()?;
    tracing::info!("💾 Journal flushed to disk");

    // --- CAS Initialization ---
    tracing::info!("📦 Initializing Content Addressable Storage (CAS)...");
    let cas_dir = state_dir.join("cas");
    std::fs::create_dir_all(&cas_dir)?;
    let cas = Cas::new(&cas_dir)?;
    tracing::info!("   CAS ready at: {}", cas_dir.display());

    // --- Local Models Initialization ---
    tracing::info!("🤖 Initializing Local Models client...");
    let local_models = Arc::new(LocalModels::new(
        cas.clone(),
        cli.orpheus_port,
        cli.deepseek_port
    ));
    tracing::info!("   Orpheus: port {}", cli.orpheus_port);
    tracing::info!("   DeepSeek: port {}", cli.deepseek_port);

    // --- Artifact Store Initialization ---
    tracing::info!("📚 Initializing Artifact Store...");
    let artifact_store_path = state_dir.join("artifacts.json");
    let artifact_store = Arc::new(
        artifact_store::FileStore::new(&artifact_store_path)
            .context("Failed to initialize artifact store")?
    );
    tracing::info!("   Artifact store at: {}", artifact_store_path.display());

    // --- Job Store Initialization ---
    tracing::info!("⚙️  Initializing shared Job Store...");
    let job_store = Arc::new(job_system::JobStore::new());
    tracing::info!("   Job store ready (shared across connections)");

    // --- Audio Graph Initialization ---
    tracing::info!("🎛️  Initializing Audio Graph...");
    let audio_graph_db = Arc::new(AudioGraphDb::in_memory().context("Failed to create audio graph db")?);
    let audio_graph_adapter = Arc::new(
        AudioGraphAdapter::new_without_pipewire(audio_graph_db.clone())
            .context("Failed to create audio graph adapter")?
    );
    tracing::info!("   Audio graph ready (in-memory)");

    let addr = format!("0.0.0.0:{}", cli.port);

    tracing::info!("🎵 Event Duality Server starting on http://{}", addr);
    tracing::info!("   MCP SSE: GET http://{}/mcp/sse", addr);
    tracing::info!("   MCP Message: POST http://{}/mcp/message", addr);
    tracing::info!("   CAS Upload: POST http://{}/cas", addr);
    tracing::info!("   CAS Download: GET http://{}/cas/:hash", addr);

    // Create shared conversation state
    tracing::info!("🌳 Initializing conversation tree...");
    let conversation_dir = state_dir.join("conversation");
    std::fs::create_dir_all(&conversation_dir)?;
    let conversation_state = ConversationState::new(conversation_dir)
        .context("Failed to initialize conversation state")?;
    let shared_state = Arc::new(Mutex::new(conversation_state));

    // Create the EventDualityServer
    let event_duality_server = Arc::new(EventDualityServer::new_with_state(
        shared_state.clone(),
        local_models.clone(),
        artifact_store.clone(),
        job_store.clone(),
        audio_graph_adapter.clone(),
        audio_graph_db.clone(),
    ));

    // Create AppState for web handlers
    let app_state = Arc::new(web::state::AppState::new(
        event_duality_server,
        Arc::new(journal),
    ));

    let shutdown_token = CancellationToken::new();

    // Create routers with their respective state types
    let cas_router = web::router(cas.clone());
    let mcp_router = web::mcp::router().with_state(app_state.clone());

    // Handler for OAuth discovery - return 404 with JSON to indicate no OAuth required
    async fn no_oauth() -> impl axum::response::IntoResponse {
        (
            axum::http::StatusCode::NOT_FOUND,
            [(axum::http::header::CONTENT_TYPE, "application/json")],
            r#"{"error": "not_found", "error_description": "This MCP server does not require authentication"}"#
        )
    }

    // Build the main application router
    // Each sub-router has its own state type, so we use nest() for CAS
    let app_router = axum::Router::new()
        .route("/mcp/.well-known/oauth-authorization-server", axum::routing::get(no_oauth))
        .route("/mcp/.well-known/oauth-protected-resource", axum::routing::get(no_oauth))
        .route("/.well-known/oauth-authorization-server", axum::routing::get(no_oauth))
        .route("/.well-known/oauth-protected-resource", axum::routing::get(no_oauth))
        .nest("/mcp", mcp_router)
        .merge(cas_router);

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
    let stats_ct = shutdown_token.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    let stats = stats_job_store.stats();
                    tracing::info!(
                        jobs.total = stats.total,
                        jobs.pending = stats.pending,
                        jobs.running = stats.running,
                        jobs.completed = stats.completed,
                        jobs.failed = stats.failed,
                        jobs.cancelled = stats.cancelled,
                        "Job store statistics"
                    );
                }
                _ = stats_ct.cancelled() => {
                    break;
                }
            }
        }
    });

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

    tracing::info!("Shutdown complete");

    // Shutdown OpenTelemetry and flush remaining spans
    telemetry::shutdown()?;

    Ok(())
}
