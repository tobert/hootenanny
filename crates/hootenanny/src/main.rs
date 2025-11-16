mod domain;
mod realization;
mod server;
pub mod persistence;

use anyhow::{Context, Result};
use clap::Parser;
use persistence::journal::{Journal, SessionEvent};
use rmcp::transport::sse_server::{SseServer, SseServerConfig};
use server::EventDualityServer;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};
use tokio_util::sync::CancellationToken;
use tracing_subscriber::EnvFilter;

/// The Hootenanny MCP Server
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Cli {
    /// The directory to store the journal and other state.
    #[arg(short, long, default_value = "/tank/halfremembered/hrmcp/1")]
    state_dir: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive(tracing::Level::INFO.into()))
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    std::fs::create_dir_all(&cli.state_dir).context("Failed to create state directory")?;
    tracing::info!("Using state directory: {}", cli.state_dir.display());

    // --- Persistence Test ---
    tracing::info!("🗄️  Initializing sled journal...");
    let mut journal = Journal::new(&cli.state_dir)?;

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

    let addr = "127.0.0.1:8080";

    tracing::info!("🎵 Event Duality Server starting on http://{}", addr);
    tracing::info!("   Connect via: GET http://{}/sse", addr);
    tracing::info!("   Send messages: POST http://{}/message?sessionId=<id>", addr);

    let sse_config = SseServerConfig {
        bind: addr.parse()?,
        sse_path: "/sse".to_string(),
        post_path: "/message".to_string(),
        ct: CancellationToken::new(),
        sse_keep_alive: Some(Duration::from_secs(15)),
    };

    let (sse_server, router) = SseServer::new(sse_config);

    let listener = tokio::net::TcpListener::bind(sse_server.config.bind).await?;

    let ct = sse_server.config.ct.child_token();

    let server = axum::serve(listener, router).with_graceful_shutdown(async move {
        ct.cancelled().await;
        tracing::info!("SSE server cancelled");
    });

    tokio::spawn(async move {
        if let Err(e) = server.await {
            tracing::error!("SSE server shutdown with error: {:?}", e);
        }
    });

    tracing::info!("🎵 Server ready. Let's dance!");

    let _ct = sse_server.with_service(EventDualityServer::new);

    tokio::signal::ctrl_c().await?;
    // ct.cancel(); // This is now handled by the _ct drop guard

    Ok(())
}