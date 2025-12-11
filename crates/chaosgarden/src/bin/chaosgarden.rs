//! Chaosgarden daemon binary
//!
//! Realtime audio daemon that communicates with hootenanny via ZMQ.

use std::sync::Arc;

use anyhow::Result;
use chaosgarden::ipc::{
    server::{GardenServer, Handler},
    ControlReply, ControlRequest, GardenEndpoints, QueryReply, QueryRequest, ShellReply,
    ShellRequest,
};
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

/// Stub handler for initial testing
struct StubHandler;

impl Handler for StubHandler {
    fn handle_shell(&self, req: ShellRequest) -> ShellReply {
        info!("shell request: {:?}", req);
        match req {
            ShellRequest::GetTransportState => ShellReply::TransportState {
                playing: false,
                position: chaosgarden::ipc::Beat(0.0),
                tempo: 120.0,
            },
            ShellRequest::Play => ShellReply::Ok {
                result: serde_json::json!({"status": "playing"}),
            },
            ShellRequest::Pause => ShellReply::Ok {
                result: serde_json::json!({"status": "paused"}),
            },
            ShellRequest::Stop => ShellReply::Ok {
                result: serde_json::json!({"status": "stopped"}),
            },
            _ => ShellReply::Error {
                error: "not implemented".to_string(),
                traceback: None,
            },
        }
    }

    fn handle_control(&self, req: ControlRequest) -> ControlReply {
        info!("control request: {:?}", req);
        match req {
            ControlRequest::Shutdown => ControlReply::ShuttingDown,
            ControlRequest::Interrupt => ControlReply::Interrupted {
                was_running: "nothing".to_string(),
            },
            ControlRequest::EmergencyPause => ControlReply::Ok,
            ControlRequest::DebugDump => ControlReply::DebugDump {
                state: serde_json::json!({
                    "version": env!("CARGO_PKG_VERSION"),
                    "status": "stub handler"
                }),
            },
        }
    }

    fn handle_query(&self, req: QueryRequest) -> QueryReply {
        info!("query request: {}", req.query);
        QueryReply::Error {
            error: "trustfall queries not yet implemented".to_string(),
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::DEBUG)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    info!("chaosgarden {} starting", env!("CARGO_PKG_VERSION"));

    let endpoints = GardenEndpoints::local();
    info!("binding to endpoints: {:?}", endpoints);

    let server = GardenServer::bind(&endpoints).await?;
    let handler = Arc::new(StubHandler);

    server.run(handler).await?;

    info!("chaosgarden shutdown complete");
    Ok(())
}
