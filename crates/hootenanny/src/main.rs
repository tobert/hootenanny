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
use cas::FileStore;
use hooteconf::HootConfig;
use mcp_tools::local_models::LocalModels;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Instant;
use tokio_util::sync::CancellationToken;
use tracing::info;

/// The Hootenanny ZMQ Server
///
/// Provides music generation and audio graph tools over ZMQ.
/// MCP gateway functionality is provided by holler.
///
/// Configuration is loaded from (in order, later wins):
/// 1. Compiled defaults
/// 2. /etc/hootenanny/config.toml
/// 3. ~/.config/hootenanny/config.toml
/// 4. ./hootenanny.toml (or --config path)
/// 5. Environment variables (HOOTENANNY_*)
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Cli {
    /// Path to config file (overrides ./hootenanny.toml)
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Show loaded configuration and exit
    #[arg(long)]
    show_config: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Load configuration from files + env
    let (config, sources) = HootConfig::load_with_sources_from(cli.config.as_deref())
        .context("Failed to load configuration")?;

    // Show config and exit if requested
    if cli.show_config {
        println!("# Configuration sources:");
        for path in &sources.files {
            println!("#   - {}", path.display());
        }
        if !sources.env_overrides.is_empty() {
            println!("# Environment overrides:");
            for var in &sources.env_overrides {
                println!("#   - {}", var);
            }
        }
        println!();
        println!("{}", config.to_toml());
        return Ok(());
    }

    // Initialize OpenTelemetry with OTLP exporter
    telemetry::init(&config.infra.telemetry.otlp_endpoint)
        .context("Failed to initialize OpenTelemetry")?;

    // Log config sources
    info!("📋 Configuration loaded from:");
    for path in &sources.files {
        info!("   - {}", path.display());
    }
    if !sources.env_overrides.is_empty() {
        info!("   Environment overrides: {:?}", sources.env_overrides);
    }

    // Create state directory
    let state_dir = &config.infra.paths.state_dir;
    std::fs::create_dir_all(state_dir).context("Failed to create state directory")?;
    info!("Using state directory: {}", state_dir.display());

    // --- CAS Initialization ---
    info!("📦 Initializing Content Addressable Storage (CAS)...");
    let cas_dir = &config.infra.paths.cas_dir;
    std::fs::create_dir_all(cas_dir)?;
    let cas = FileStore::at_path(cas_dir)?;
    info!("   CAS ready at: {}", cas_dir.display());

    // --- Local Models Initialization ---
    info!("🤖 Initializing Local Models client...");
    let orpheus_url = config.bootstrap.models.get("orpheus")
        .cloned()
        .unwrap_or_else(|| "http://127.0.0.1:2000".to_string());
    let orpheus_port: u16 = orpheus_url
        .rsplit(':')
        .next()
        .and_then(|s| s.parse().ok())
        .unwrap_or(2000);
    let local_models = Arc::new(LocalModels::new(cas.clone(), orpheus_port));
    info!("   Orpheus: {}", orpheus_url);

    // --- Artifact Store Initialization ---
    info!("📚 Initializing Artifact Store...");
    let artifact_store_path = state_dir.join("artifacts.json");
    let artifact_store = Arc::new(RwLock::new(
        artifact_store::FileStore::new(&artifact_store_path)
            .context("Failed to initialize artifact store")?
    ));
    info!("   Artifact store at: {}", artifact_store_path.display());

    // --- Job Store Initialization ---
    info!("⚙️  Initializing shared Job Store...");
    let job_store = Arc::new(job_system::JobStore::new());

    // Spawn background cleanup task (runs every 60s)
    let _cleanup_handle = job_system::spawn_cleanup_task(job_store.as_ref().clone(), 60);
    info!("   Job store ready (cleanup task spawned, TTL: garden=60s, other=300s)");

    // --- GPU Observer Client ---
    info!("🎮 Initializing GPU observer client...");
    let gpu_monitor = Arc::new(gpu_monitor::GpuMonitor::new());
    info!("   GPU observer client ready");

    // --- Audio Graph Initialization ---
    info!("🎛️  Initializing Audio Graph...");
    let audio_graph_db = Arc::new(AudioGraphDb::in_memory().context("Failed to create audio graph db")?);
    let artifact_source = Arc::new(artifact_store::FileStoreSource::new(artifact_store.clone()));
    let graph_adapter = Arc::new(
        AudioGraphAdapter::new_with_artifacts(
            audio_graph_db.clone(),
            audio_graph_mcp::PipeWireSnapshot::default(),
            artifact_source,
        )
        .context("Failed to create audio graph adapter")?
    );
    info!("   Audio graph ready (in-memory, with Trustfall adapter + artifacts)");

    // --- Chaosgarden Connection (non-blocking) ---
    let chaosgarden_endpoint = &config.bootstrap.connections.chaosgarden;
    let garden_manager: Option<Arc<zmq::GardenManager>> = {
        info!("🌱 Connecting to chaosgarden ({})...", chaosgarden_endpoint);

        let manager: Option<zmq::GardenManager> = if chaosgarden_endpoint == "local" {
            Some(zmq::GardenManager::local())
        } else if chaosgarden_endpoint.starts_with("tcp://") {
            let parts: Vec<&str> = chaosgarden_endpoint.trim_start_matches("tcp://").split(':').collect();
            if parts.len() == 2 {
                if let Ok(port) = parts[1].parse::<u16>() {
                    Some(zmq::GardenManager::tcp(parts[0], port))
                } else {
                    tracing::warn!("Invalid port in chaosgarden endpoint");
                    None
                }
            } else {
                tracing::warn!("Invalid TCP endpoint format, expected tcp://host:port");
                None
            }
        } else {
            tracing::warn!("Invalid chaosgarden endpoint, use 'local' or 'tcp://host:port'");
            None
        };

        if let Some(manager) = manager {
            // Non-blocking connect with timeout
            match tokio::time::timeout(
                std::time::Duration::from_secs(2),
                manager.connect()
            ).await {
                Ok(Ok(())) => {
                    info!("   Connected to chaosgarden!");
                    if let Err(e) = manager.start_event_listener().await {
                        tracing::warn!("   Failed to start event listener: {}", e);
                    }
                    Some(Arc::new(manager))
                }
                Ok(Err(e)) => {
                    tracing::warn!("   Failed to connect to chaosgarden: {}", e);
                    tracing::warn!("   Continuing without chaosgarden connection");
                    None
                }
                Err(_) => {
                    tracing::warn!("   Timeout connecting to chaosgarden (daemon not running?)");
                    tracing::warn!("   Continuing without chaosgarden connection");
                    None
                }
            }
        } else {
            tracing::warn!("   Continuing without chaosgarden connection");
            None
        }
    };

    // --- Luanette Connection (non-blocking) ---
    let luanette_endpoint = &config.bootstrap.connections.luanette;
    let luanette_client: Option<Arc<zmq::LuanetteClient>> = if !luanette_endpoint.is_empty() {
        info!("🌙 Connecting to luanette at {}...", luanette_endpoint);
        match zmq::LuanetteClient::connect(luanette_endpoint, 30000).await {
            Ok(client) => {
                info!("   Connected to luanette!");
                Some(Arc::new(client))
            }
            Err(e) => {
                tracing::warn!("   Failed to connect to luanette: {}", e);
                tracing::warn!("   Continuing without Lua scripting");
                None
            }
        }
    } else {
        None
    };

    let http_port = config.infra.bind.http_port;
    let zmq_router = &config.infra.bind.zmq_router;
    let zmq_pub = &config.infra.bind.zmq_pub;
    let addr = format!("0.0.0.0:{}", http_port);

    // --- ZMQ PUB socket for broadcasts ---
    info!("📢 Starting ZMQ PUB socket for broadcasts...");
    let (pub_server, broadcast_publisher) = zmq::PublisherServer::new(zmq_pub.clone(), 256);
    tokio::spawn(async move {
        if let Err(e) = pub_server.run().await {
            tracing::error!("ZMQ PUB server error: {}", e);
        }
    });
    info!("   ZMQ PUB: {}", zmq_pub);

    // Create the EventDualityServer
    let event_duality_server = Arc::new(EventDualityServer::new(
        local_models.clone(),
        artifact_store.clone(),
        job_store.clone(),
        audio_graph_db.clone(),
        graph_adapter.clone(),
        gpu_monitor.clone(),
    )
    .with_garden(garden_manager.clone())
    .with_broadcaster(Some(broadcast_publisher)));

    // --- Hooteproto ZMQ Server ---
    info!("📡 Starting hooteproto ZMQ server...");
    let (shutdown_tx, shutdown_rx) = tokio::sync::broadcast::channel::<()>(1);
    let zmq_server = zmq::HooteprotoServer::with_event_server(
        zmq_router.clone(),
        Arc::new(cas.clone()),
        artifact_store.clone(),
        event_duality_server.clone(),
    )
    .with_luanette(luanette_client.clone());

    tokio::spawn(async move {
        if let Err(e) = zmq_server.run(shutdown_rx).await {
            tracing::error!("ZMQ server error: {}", e);
        }
    });
    info!("   ZMQ ROUTER: {}", zmq_router);
    if luanette_client.is_some() {
        info!("   Luanette proxy: enabled");
    }

    info!("🎵 Hootenanny starting on http://{}", addr);
    info!("   Artifact Content: GET http://{}/artifact/:id", addr);
    info!("   Artifact Meta: GET http://{}/artifact/:id/meta", addr);
    info!("   Artifacts List: GET http://{}/artifacts", addr);
    info!("   Health: GET http://{}/health", addr);
    info!("   ZMQ ROUTER: {} (for holler MCP gateway)", zmq_router);
    info!("   ZMQ PUB: {} (for SSE broadcasts)", zmq_pub);

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
        luanette: Option<Arc<zmq::LuanetteClient>>,
        garden: Option<Arc<zmq::GardenManager>>,
    }

    async fn health_handler(
        axum::extract::State(state): axum::extract::State<HealthState>,
    ) -> axum::Json<serde_json::Value> {
        let job_stats = state.job_store.stats();
        let uptime = state.start_time.elapsed();

        let mut backends = serde_json::Map::new();

        if let Some(ref luanette) = state.luanette {
            backends.insert("luanette".to_string(), luanette.health.health_summary().await);
        }

        if let Some(ref garden) = state.garden {
            backends.insert("chaosgarden".to_string(), serde_json::json!({
                "connected": garden.is_connected().await,
                "state": format!("{:?}", garden.state().await),
            }));
        }

        axum::Json(serde_json::json!({
            "status": "healthy",
            "uptime_secs": uptime.as_secs(),
            "version": env!("CARGO_PKG_VERSION"),
            "jobs": {
                "pending": job_stats.pending,
                "running": job_stats.running,
            },
            "backends": backends,
        }))
    }

    let health_state = HealthState {
        job_store: job_store.clone(),
        start_time: server_start,
        luanette: luanette_client.clone(),
        garden: garden_manager.clone(),
    };

    let health_router = axum::Router::new()
        .route("/health", axum::routing::get(health_handler))
        .with_state(health_state);

    let app_router = axum::Router::new()
        .merge(health_router)
        .merge(artifact_router);

    let bind_addr: std::net::SocketAddr = addr.parse().context("Failed to parse bind address")?;
    let listener = tokio::net::TcpListener::bind(bind_addr).await?;

    info!("🌐 Router created, starting server...");

    let shutdown_token_srv = shutdown_token.clone();
    let server = axum::serve(listener, app_router).with_graceful_shutdown(async move {
        shutdown_token_srv.cancelled().await;
        info!("Server shutdown signal received");
    });

    tokio::spawn(async move {
        if let Err(e) = server.await {
            tracing::error!("Server shutdown with error: {:?}", e);
        }
    });

    info!("🎵 Server ready. Let's dance!");

    // Spawn background task for periodic statistics logging
    let stats_job_store = job_store.clone();
    let stats_ct = shutdown_token.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    let job_stats = stats_job_store.stats();
                    info!(
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

    // Handle both SIGINT (Ctrl+C) and SIGTERM
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            info!("Received SIGINT (Ctrl+C), shutting down gracefully...");
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
            info!("Received SIGTERM, shutting down gracefully...");
            shutdown_token.cancel();
        }
    }

    // Signal ZMQ server shutdown
    let _ = shutdown_tx.send(());

    info!("Shutdown complete");

    // Shutdown OpenTelemetry and flush remaining spans
    telemetry::shutdown()?;

    Ok(())
}
