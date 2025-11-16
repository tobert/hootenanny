mod domain;
mod realization;
mod server;
pub mod persistence;

use anyhow::Result;
use rmcp::transport::sse_server::{SseServer, SseServerConfig};
use server::EventDualityServer;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive(tracing::Level::INFO.into()))
        .with_writer(std::io::stderr)
        .init();

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

    let ct = sse_server.with_service(EventDualityServer::new);

    tokio::signal::ctrl_c().await?;
    ct.cancel();

    Ok(())
}
