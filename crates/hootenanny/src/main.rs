#![allow(unused, clippy::unnecessary_cast, clippy::too_many_arguments)]

mod api;
mod artifact_store;
mod cas;
mod event_buffer;
mod gpu_monitor;
mod job_system;
mod mcp_tools;
mod persistence;
mod pipewire;
mod sessions;
mod streams;
mod telemetry;
mod types;
mod web;
mod zmq;

use anyhow::{Context, Result};
use api::service::EventDualityServer;
use audio_graph_mcp::{
    AudioGraphAdapter, Database as AudioGraphDb, PipeWireListener, PipeWireSnapshot, PipeWireSource,
};
use cas::FileStore;
use clap::Parser;
use hooteconf::HootConfig;
use mcp_tools::local_models::LocalModels;
use sessions::SessionManager;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Instant;
use streams::{SlicingEngine, StreamManager};
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
    let orpheus_url = config
        .bootstrap
        .models
        .get("orpheus")
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
            .context("Failed to initialize artifact store")?,
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
    let audio_graph_db =
        Arc::new(AudioGraphDb::in_memory().context("Failed to create audio graph db")?);
    let artifact_source = Arc::new(artifact_store::FileStoreSource::new(artifact_store.clone()));

    // Create shared PipeWire snapshot for live device tracking
    let pipewire_snapshot = Arc::new(tokio::sync::RwLock::new(PipeWireSnapshot::default()));

    // Take initial snapshot if PipeWire is available
    let pw_source = PipeWireSource::new();
    if pw_source.is_available() {
        match pw_source.snapshot() {
            Ok(initial) => {
                *pipewire_snapshot.write().await = initial;
                info!("   Initial PipeWire snapshot captured");
            }
            Err(e) => {
                tracing::warn!("   Failed to capture initial PipeWire snapshot: {}", e);
            }
        }
    }

    let graph_adapter = Arc::new(
        AudioGraphAdapter::new_with_live_snapshot(
            audio_graph_db.clone(),
            pipewire_snapshot.clone(),
            artifact_source,
        )
        .context("Failed to create audio graph adapter")?,
    );
    info!("   Audio graph ready (in-memory, with Trustfall adapter + artifacts + live PipeWire)");

    // --- Chaosgarden Connection (non-blocking) ---
    let chaosgarden_endpoint = &config.bootstrap.connections.chaosgarden;
    let garden_manager: Option<Arc<zmq::GardenManager>> = {
        info!("🌱 Connecting to chaosgarden ({})...", chaosgarden_endpoint);

        let manager: Option<zmq::GardenManager> = if chaosgarden_endpoint == "local" {
            let socket_dir = config.infra.paths.require_socket_dir()
                .context("Cannot connect to local chaosgarden without socket_dir configured")?;
            info!("   Using IPC sockets in {:?}", socket_dir);
            Some(zmq::GardenManager::from_socket_dir(&socket_dir.to_string_lossy()))
        } else if chaosgarden_endpoint.starts_with("tcp://") {
            let parts: Vec<&str> = chaosgarden_endpoint
                .trim_start_matches("tcp://")
                .split(':')
                .collect();
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
            match tokio::time::timeout(std::time::Duration::from_secs(2), manager.connect()).await {
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

    // --- Vibeweaver Connection (lazy - ZMQ connects when peer available) ---
    let vibeweaver_endpoint = &config.bootstrap.connections.vibeweaver;
    let vibeweaver_client: Option<Arc<zmq::VibeweaverClient>> = if !vibeweaver_endpoint.is_empty() {
        info!("🐍 Connecting to vibeweaver at {}...", vibeweaver_endpoint);
        let vibeweaver_config = zmq::vibeweaver_config(vibeweaver_endpoint, 30000);
        Some(zmq::VibeweaverClient::new(vibeweaver_config).await)
    } else {
        None
    };

    // --- RAVE Connection (lazy - ZMQ connects when peer available) ---
    let rave_endpoint = &config.bootstrap.connections.rave;
    let rave_client: Option<Arc<zmq::RaveClient>> = if !rave_endpoint.is_empty() {
        info!("🎛️  Connecting to RAVE at {}...", rave_endpoint);
        let rave_config = zmq::rave_config(rave_endpoint, zmq::DEFAULT_RAVE_TIMEOUT_MS);
        Some(zmq::RaveClient::new(rave_config).await)
    } else {
        None
    };

    // --- Orpheus Connection (lazy - ZMQ connects when peer available) ---
    let orpheus_endpoint = &config.bootstrap.connections.orpheus;
    let orpheus_client: Option<Arc<zmq::OrpheusClient>> = if !orpheus_endpoint.is_empty() {
        info!("🎼 Connecting to Orpheus at {}...", orpheus_endpoint);
        let orpheus_config = zmq::orpheus_config(orpheus_endpoint, zmq::DEFAULT_ORPHEUS_TIMEOUT_MS);
        Some(zmq::OrpheusClient::new(orpheus_config).await)
    } else {
        None
    };

    // --- Beat-this Connection (lazy - ZMQ connects when peer available) ---
    let beatthis_endpoint = &config.bootstrap.connections.beatthis;
    let beatthis_client: Option<Arc<zmq::BeatthisClient>> = if !beatthis_endpoint.is_empty() {
        info!("🥁 Connecting to beat-this at {}...", beatthis_endpoint);
        let beatthis_config = zmq::beatthis_config(beatthis_endpoint, zmq::DEFAULT_BEATTHIS_TIMEOUT_MS);
        Some(zmq::BeatthisClient::new(beatthis_config).await)
    } else {
        None
    };

    // --- MusicGen Connection (lazy - ZMQ connects when peer available) ---
    let musicgen_endpoint = &config.bootstrap.connections.musicgen;
    let musicgen_client: Option<Arc<zmq::MusicgenClient>> = if !musicgen_endpoint.is_empty() {
        info!("🎵 Connecting to MusicGen at {}...", musicgen_endpoint);
        let musicgen_config = zmq::musicgen_config(musicgen_endpoint, zmq::DEFAULT_MUSICGEN_TIMEOUT_MS);
        Some(zmq::MusicgenClient::new(musicgen_config).await)
    } else {
        None
    };

    // --- CLAP Connection (lazy - ZMQ connects when peer available) ---
    let clap_endpoint = &config.bootstrap.connections.clap;
    let clap_client: Option<Arc<zmq::ClapClient>> = if !clap_endpoint.is_empty() {
        info!("🔍 Connecting to CLAP at {}...", clap_endpoint);
        let clap_config = zmq::clap_config(clap_endpoint, zmq::DEFAULT_CLAP_TIMEOUT_MS);
        Some(zmq::ClapClient::new(clap_config).await)
    } else {
        None
    };

    // --- AudioLDM2 Connection (lazy - ZMQ connects when peer available) ---
    let audioldm2_endpoint = &config.bootstrap.connections.audioldm2;
    let audioldm2_client: Option<Arc<zmq::Audioldm2Client>> = if !audioldm2_endpoint.is_empty() {
        info!("🎶 Connecting to AudioLDM2 at {}...", audioldm2_endpoint);
        let audioldm2_config = zmq::audioldm2_config(audioldm2_endpoint, zmq::DEFAULT_AUDIOLDM2_TIMEOUT_MS);
        Some(zmq::Audioldm2Client::new(audioldm2_config).await)
    } else {
        None
    };

    // --- Anticipatory Connection (lazy - ZMQ connects when peer available) ---
    let anticipatory_endpoint = &config.bootstrap.connections.anticipatory;
    let anticipatory_client: Option<Arc<zmq::AnticipatoryClient>> = if !anticipatory_endpoint.is_empty() {
        info!("🎹 Connecting to Anticipatory at {}...", anticipatory_endpoint);
        let anticipatory_config = zmq::anticipatory_config(anticipatory_endpoint, zmq::DEFAULT_ANTICIPATORY_TIMEOUT_MS);
        Some(zmq::AnticipatoryClient::new(anticipatory_config).await)
    } else {
        None
    };

    // --- Demucs Connection (lazy - ZMQ connects when peer available) ---
    let demucs_endpoint = &config.bootstrap.connections.demucs;
    let demucs_client: Option<Arc<zmq::DemucsClient>> = if !demucs_endpoint.is_empty() {
        info!("🎚️ Connecting to Demucs at {}...", demucs_endpoint);
        let demucs_config = zmq::demucs_config(demucs_endpoint, zmq::DEFAULT_DEMUCS_TIMEOUT_MS);
        Some(zmq::DemucsClient::new(demucs_config).await)
    } else {
        None
    };

    let http_addr = config.infra.bind.http_bind_addr();
    let zmq_router = &config.infra.bind.zmq_router;
    let zmq_pub = &config.infra.bind.zmq_pub;

    // --- Event Buffer for cursor-based polling ---
    info!("📋 Creating event buffer...");
    let event_buffer = event_buffer::create_event_buffer(event_buffer::DEFAULT_CAPACITY);
    info!(
        "   Event buffer capacity: {} events",
        event_buffer::DEFAULT_CAPACITY
    );

    // --- ZMQ PUB socket for broadcasts ---
    info!("📢 Starting ZMQ PUB socket for broadcasts...");
    let (pub_server, broadcast_publisher) = zmq::PublisherServer::new(zmq_pub.clone(), 256);
    let pub_server = pub_server.with_event_buffer(event_buffer.clone());
    tokio::spawn(async move {
        if let Err(e) = pub_server.run().await {
            tracing::error!("ZMQ PUB server error: {}", e);
        }
    });
    info!("   ZMQ PUB: {} (with event buffer)", zmq_pub);

    // Wire up broadcaster to job store for job state change notifications
    job_store.set_broadcaster(broadcast_publisher.clone());
    info!("   Job store connected to broadcaster");

    // --- PipeWire Device Hot-Plug Listener ---
    info!("🔌 Starting PipeWire device listener...");
    let (device_event_tx, device_event_rx) = tokio::sync::mpsc::channel(256);

    // Spawn PipeWire listener (runs in blocking thread)
    let listener = PipeWireListener::new(pipewire_snapshot.clone(), device_event_tx);
    let _pipewire_handle = listener.spawn();
    info!("   PipeWire listener started");

    // Spawn device event manager (processes events and broadcasts)
    let event_manager = pipewire::DeviceEventManager::new(
        device_event_rx,
        audio_graph_db.clone(),
        broadcast_publisher.clone(),
    );
    tokio::spawn(event_manager.run());
    info!("   Device event manager started");

    // --- Stream Subsystems (for capture sessions) ---
    info!("🎙️  Initializing stream capture subsystems...");
    let cas_arc = Arc::new(cas.clone());
    let stream_manager = Arc::new(StreamManager::new(cas_arc.clone()));
    let session_manager = Arc::new(SessionManager::new(cas_arc.clone(), stream_manager.clone()));
    let slicing_engine = Arc::new(SlicingEngine::new(cas.clone()));
    info!("   Stream manager ready");
    info!("   Session manager ready");
    info!("   Slicing engine ready");

    // Create the EventDualityServer
    let event_duality_server = Arc::new(
        EventDualityServer::new(
            local_models.clone(),
            artifact_store.clone(),
            job_store.clone(),
            audio_graph_db.clone(),
            graph_adapter.clone(),
            gpu_monitor.clone(),
        )
        .with_garden(garden_manager.clone())
        .with_vibeweaver(vibeweaver_client.clone())
        .with_rave(rave_client.clone())
        .with_orpheus(orpheus_client.clone())
        .with_beatthis(beatthis_client.clone())
        .with_musicgen(musicgen_client.clone())
        .with_clap(clap_client.clone())
        .with_audioldm2(audioldm2_client.clone())
        .with_anticipatory(anticipatory_client.clone())
        .with_demucs(demucs_client.clone())
        .with_broadcaster(Some(broadcast_publisher))
        .with_stream_manager(Some(stream_manager.clone()))
        .with_session_manager(Some(session_manager.clone()))
        .with_slicing_engine(Some(slicing_engine.clone()))
        .with_event_buffer(Some(event_buffer)),
    );

    // --- Start Stream Event Handler (if chaosgarden connected) ---
    if garden_manager.is_some() {
        if let Err(e) = event_duality_server.start_stream_event_handler().await {
            tracing::warn!("   Failed to start stream event handler: {}", e);
        } else {
            info!("   Stream event handler started");
        }
    }

    // --- Hooteproto ZMQ Server ---
    info!("📡 Starting hooteproto ZMQ server...");
    let (shutdown_tx, shutdown_rx) = tokio::sync::broadcast::channel::<()>(1);
    let zmq_server = zmq::HooteprotoServer::with_event_server(
        zmq_router.clone(),
        Arc::new(cas.clone()),
        artifact_store.clone(),
        event_duality_server.clone(),
    );

    tokio::spawn(async move {
        if let Err(e) = zmq_server.run(shutdown_rx).await {
            tracing::error!("ZMQ server error: {}", e);
        }
    });
    info!("   ZMQ ROUTER: {}", zmq_router);
    if vibeweaver_client.is_some() {
        info!("   Vibeweaver proxy: enabled (via EventDualityServer)");
    }

    let tls_enabled = config.infra.bind.tls.enabled;
    let scheme = if tls_enabled { "https" } else { "http" };
    let ws_scheme = if tls_enabled { "wss" } else { "ws" };

    if tls_enabled {
        info!("🔐 Hootenanny starting on {}://{}", scheme, http_addr);
    } else {
        info!("🎵 Hootenanny starting on {}://{}", scheme, http_addr);
    }
    info!("   UI: {}://{}/ui", scheme, http_addr);
    info!("   Live Stream: {}://{}/stream/live", ws_scheme, http_addr);
    info!("   Artifact Content: GET {}://{}/artifact/:id", scheme, http_addr);
    info!("   Artifact Meta: GET {}://{}/artifact/:id/meta", scheme, http_addr);
    info!("   Artifacts List: GET {}://{}/artifacts", scheme, http_addr);
    info!("   Health: GET {}://{}/health", scheme, http_addr);
    info!("   ZMQ ROUTER: {} (for holler MCP gateway)", zmq_router);
    info!("   ZMQ PUB: {} (for SSE broadcasts)", zmq_pub);

    let shutdown_token = CancellationToken::new();

    // Create routers with their respective state types
    let web_state = web::WebState {
        artifact_store: artifact_store.clone(),
        cas: Arc::new(cas.clone()),
        garden_manager: garden_manager.clone(),
    };
    let artifact_router = web::router(web_state);

    // Track server start time for uptime
    let server_start = Instant::now();

    // Health endpoint state
    #[derive(Clone)]
    struct HealthState {
        job_store: Arc<job_system::JobStore>,
        start_time: Instant,
        vibeweaver: Option<Arc<zmq::VibeweaverClient>>,
        garden: Option<Arc<zmq::GardenManager>>,
    }

    async fn health_handler(
        axum::extract::State(state): axum::extract::State<HealthState>,
    ) -> axum::Json<serde_json::Value> {
        let job_stats = state.job_store.stats();
        let uptime = state.start_time.elapsed();

        let mut backends = serde_json::Map::new();

        if let Some(ref vibeweaver) = state.vibeweaver {
            backends.insert(
                "vibeweaver".to_string(),
                vibeweaver.health.health_summary().await,
            );
        }

        if let Some(ref garden) = state.garden {
            backends.insert(
                "chaosgarden".to_string(),
                serde_json::json!({
                    "connected": garden.is_connected().await,
                    "state": format!("{:?}", garden.state().await),
                }),
            );
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
        vibeweaver: vibeweaver_client.clone(),
        garden: garden_manager.clone(),
    };

    let health_router = axum::Router::new()
        .route("/health", axum::routing::get(health_handler))
        .with_state(health_state);

    let app_router = axum::Router::new()
        .merge(health_router)
        .merge(artifact_router);

    let bind_addr: std::net::SocketAddr = http_addr.parse().context("Failed to parse bind address")?;

    info!("🌐 Router created, starting server...");

    if tls_enabled {
        // Load TLS certificates
        let tls_config = &config.infra.bind.tls;
        let cert_path = tls_config
            .resolved_cert_path()
            .context("Could not determine certificate path")?;
        let key_path = tls_config
            .resolved_key_path()
            .context("Could not determine key path")?;

        if !cert_path.exists() || !key_path.exists() {
            anyhow::bail!(
                "TLS enabled but certificates not found.\n\
                 Expected:\n  cert: {}\n  key: {}\n\n\
                 Generate certificates with:\n  holler generate-cert --hostname <your-hostname>",
                cert_path.display(),
                key_path.display()
            );
        }

        let rustls_config = axum_server::tls_rustls::RustlsConfig::from_pem_file(&cert_path, &key_path)
            .await
            .with_context(|| format!(
                "Failed to load TLS config from {} and {}",
                cert_path.display(),
                key_path.display()
            ))?;

        let handle = axum_server::Handle::new();
        let shutdown_handle = handle.clone();
        let shutdown_token_srv = shutdown_token.clone();
        tokio::spawn(async move {
            shutdown_token_srv.cancelled().await;
            info!("Server shutdown signal received");
            shutdown_handle.graceful_shutdown(None);
        });

        tokio::spawn(async move {
            if let Err(e) = axum_server::bind_rustls(bind_addr, rustls_config)
                .handle(handle)
                .serve(app_router.into_make_service())
                .await
            {
                tracing::error!("TLS server shutdown with error: {:?}", e);
            }
        });
    } else {
        let listener = tokio::net::TcpListener::bind(bind_addr).await?;

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
    }

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