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
use api::service::EventDualityServer;
use mcp_tools::local_models::LocalModels;
use cas::FileStore;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Instant;
use tokio_util::sync::CancellationToken;

/// The Hootenanny ZMQ Server
///
/// Provides music generation and audio graph tools over ZMQ.
/// MCP gateway functionality is provided by holler.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Cli {
    /// The directory to store the journal and other state.
    /// Sled databases are single-writer, so each instance needs its own directory.
    #[arg(short, long)]
    state_dir: Option<PathBuf>,

    /// Port for HTTP endpoints (health, artifacts)
    #[arg(short, long, default_value = "8080")]
    port: u16,

    /// Orpheus Model Port
    #[arg(long, default_value = "2000")]
    orpheus_port: u16,

    /// OTLP gRPC endpoint for OpenTelemetry (e.g., "127.0.0.1:35991")
    #[arg(long, default_value = "127.0.0.1:35991")]
    otlp_endpoint: String,

    /// Connect to chaosgarden daemon at this endpoint (e.g., "tcp://127.0.0.1:5555" or "local" for IPC)
    #[arg(long)]
    chaosgarden: Option<String>,

    /// Bind a hooteproto ZMQ ROUTER for holler gateway (e.g., "tcp://0.0.0.0:5580")
    #[arg(long)]
    zmq_bind: Option<String>,

    /// Bind a ZMQ PUB socket for broadcasting events to holler (e.g., "tcp://0.0.0.0:5581")
    #[arg(long)]
    zmq_pub: Option<String>,
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

    // --- ZMQ PUB socket for broadcasts (optional, for holler SSE) ---
    // Create this first so we can pass it to EventDualityServer
    let broadcast_publisher: Option<zmq::BroadcastPublisher> = if let Some(ref zmq_pub) = cli.zmq_pub {
        tracing::info!("📢 Starting ZMQ PUB socket for broadcasts...");
        let (pub_server, publisher) = zmq::PublisherServer::new(zmq_pub.clone(), 256);
        tokio::spawn(async move {
            if let Err(e) = pub_server.run().await {
                tracing::error!("ZMQ PUB server error: {}", e);
            }
        });
        tracing::info!("   ZMQ PUB: {}", zmq_pub);
        Some(publisher)
    } else {
        None
    };

    // Create the EventDualityServer (before ZMQ server so we can pass it)
    let event_duality_server = Arc::new(EventDualityServer::new(
        local_models.clone(),
        artifact_store.clone(),
        job_store.clone(),
        audio_graph_db.clone(),
        graph_adapter.clone(),
        gpu_monitor.clone(),
    )
    .with_garden(garden_manager.clone())
    .with_broadcaster(broadcast_publisher));

    // --- Hooteproto ZMQ Server (required for tool dispatch) ---
    let zmq_shutdown_tx: Option<tokio::sync::broadcast::Sender<()>> = if let Some(ref zmq_bind) = cli.zmq_bind {
        tracing::info!("📡 Starting hooteproto ZMQ server (with full tool dispatch)...");
        let (shutdown_tx, shutdown_rx) = tokio::sync::broadcast::channel::<()>(1);
        let zmq_server = zmq::HooteprotoServer::with_event_server(
            zmq_bind.clone(),
            Arc::new(cas.clone()),
            artifact_store.clone(),
            event_duality_server.clone(),
        );
        tokio::spawn(async move {
            if let Err(e) = zmq_server.run(shutdown_rx).await {
                tracing::error!("ZMQ server error: {}", e);
            }
        });
        tracing::info!("   ZMQ ROUTER: {}", zmq_bind);
        Some(shutdown_tx)
    } else {
        tracing::warn!("⚠️  No --zmq-bind specified. Tools will not be available.");
        tracing::warn!("    Use --zmq-bind tcp://0.0.0.0:5580 to enable holler gateway.");
        None
    };

    tracing::info!("🎵 Hootenanny starting on http://{}", addr);
    tracing::info!("   Artifact Content: GET http://{}/artifact/:id", addr);
    tracing::info!("   Artifact Meta: GET http://{}/artifact/:id/meta", addr);
    tracing::info!("   Artifacts List: GET http://{}/artifacts", addr);
    tracing::info!("   Health: GET http://{}/health", addr);
    if cli.zmq_bind.is_some() {
        tracing::info!("   ZMQ ROUTER: {} (for holler MCP gateway)", cli.zmq_bind.as_ref().unwrap());
    }
    if cli.zmq_pub.is_some() {
        tracing::info!("   ZMQ PUB: {} (for SSE broadcasts)", cli.zmq_pub.as_ref().unwrap());
    }

    let shutdown_token = CancellationToken::new();

    // Create routers with their respective state types
    let web_state = web::WebState {
        artifact_store: artifact_store.clone(),
        cas: Arc::new(cas.clone()),
    };
    let artifact_router = web::router(web_state);

    // Track server start time for uptime
    let server_start = Instant::now();

    // Health endpoint state
    #[derive(Clone)]
    struct HealthState {
        job_store: Arc<job_system::JobStore>,
        start_time: Instant,
    }

    async fn health_handler(
        axum::extract::State(state): axum::extract::State<HealthState>,
    ) -> axum::Json<serde_json::Value> {
        let job_stats = state.job_store.stats();
        let uptime = state.start_time.elapsed();

        axum::Json(serde_json::json!({
            "status": "healthy",
            "uptime_secs": uptime.as_secs(),
            "version": env!("CARGO_PKG_VERSION"),
            "jobs": {
                "pending": job_stats.pending,
                "running": job_stats.running,
            }
        }))
    }

    let health_state = HealthState {
        job_store: job_store.clone(),
        start_time: server_start,
    };

    // Build the main application router
    let health_router = axum::Router::new()
        .route("/health", axum::routing::get(health_handler))
        .with_state(health_state);

    let app_router = axum::Router::new()
        .merge(health_router)
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
    let stats_ct = shutdown_token.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    let job_stats = stats_job_store.stats();
                    tracing::info!(
                        jobs.total = job_stats.total,
                        jobs.pending = job_stats.pending,
                        jobs.running = job_stats.running,
                        jobs.completed = job_stats.completed,
                        jobs.failed = job_stats.failed,
                        jobs.cancelled = job_stats.cancelled,
                        "Server statistics"
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

    // Signal ZMQ server shutdown if running
    if let Some(zmq_shutdown) = zmq_shutdown_tx {
        let _ = zmq_shutdown.send(());
    }

    tracing::info!("Shutdown complete");

    // Shutdown OpenTelemetry and flush remaining spans
    telemetry::shutdown()?;

    Ok(())
}
