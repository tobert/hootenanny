mod artifact_store;
mod conversation;
mod domain;
mod realization;
mod server;
mod telemetry;
pub mod persistence;
pub mod cas;
pub mod mcp_tools;
pub mod web;
pub mod job_system;

use anyhow::{Context, Result};
use clap::Parser;
use persistence::journal::{Journal, SessionEvent};
use rmcp::transport::sse_server::{SseServer, SseServerConfig};
use server::{ConversationState, EventDualityServer};
use cas::Cas;
use mcp_tools::local_models::LocalModels;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};
use tokio_util::sync::CancellationToken;
use tracing_subscriber::EnvFilter;

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
        // Default to a persistent location in user's home or /tank
        let default_base = if let Ok(home) = std::env::var("HOME") {
            PathBuf::from(home).join(".local/share/hrmcp")
        } else {
            PathBuf::from("/tank/halfremembered/hrmcp/default")
        };
        default_base
    });

    std::fs::create_dir_all(&state_dir).context("Failed to create state directory")?;
    tracing::info!("Using state directory: {}", state_dir.display());

    // --- Persistence Test ---
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
    // --- End Persistence Test ---

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

    let addr = format!("0.0.0.0:{}", cli.port);

    tracing::info!("🎵 Event Duality Server starting on http://{}", addr);
    tracing::info!("   Connect via: GET http://{}/sse", addr);
    tracing::info!("   Send messages: POST http://{}/message?sessionId=<id>", addr);
    tracing::info!("   CAS Upload: POST http://{}/cas", addr);
    tracing::info!("   CAS Download: GET http://{}/cas/:hash", addr);


    // Create shared conversation state FIRST
    tracing::info!("🌳 Initializing conversation tree...");
    let conversation_dir = state_dir.join("conversation");
    std::fs::create_dir_all(&conversation_dir)?;
    let conversation_state = ConversationState::new(conversation_dir)
        .context("Failed to initialize conversation state")?;
    let shared_state = Arc::new(Mutex::new(conversation_state));

    let sse_config = SseServerConfig {
        bind: addr.parse().context("Failed to parse bind address")?,
        sse_path: "/sse".to_string(),
        post_path: "/message".to_string(),
        ct: CancellationToken::new(),
        sse_keep_alive: Some(Duration::from_secs(15)),
    };

    let (sse_server, sse_router) = SseServer::new(sse_config);

    // Create WEB Router for CAS
    let web_router = web::router(cas.clone());
    
    // Merge routers: WEB + SSE
    let app_router = sse_router.merge(web_router);

    // Save bind address before sse_server is moved
    let bind_addr = sse_server.config.bind;

    // Register the service BEFORE starting the server
    tracing::info!("Setting up SSE server with EventDualityServer service.");
    let local_models_clone = local_models.clone();
    let artifact_store_clone = artifact_store.clone();
    let ct = sse_server.with_service(move || {
        EventDualityServer::new_with_state(
            shared_state.clone(),
            local_models_clone.clone(),
            artifact_store_clone.clone()
        )
    });

    let listener = tokio::net::TcpListener::bind(bind_addr).await?;

    tracing::info!("Router created: {:?}", app_router);

    let server = axum::serve(listener, app_router).with_graceful_shutdown(async move {
        ct.cancelled().await;
        tracing::info!("SSE server cancelled");
    });

    tokio::spawn(async move {
        if let Err(e) = server.await {
            tracing::error!("SSE server shutdown with error: {:?}", e);
        }
    });

    tracing::info!("🎵 Server ready. Let's dance!");

    // Handle both SIGINT (Ctrl+C) and SIGTERM (cargo-watch, systemd, etc.)
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("Received SIGINT (Ctrl+C), shutting down gracefully...");
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
        }
    }

    // ConversationStore will flush via Drop trait
    tracing::info!("Shutdown complete");

    // Shutdown OpenTelemetry and flush remaining spans
    telemetry::shutdown()?;

    Ok(())
}