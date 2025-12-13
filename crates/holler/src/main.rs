//! holler - MCP gateway and ZMQ CLI for the Hootenanny system
//!
//! Subcommands:
//! - `holler serve` - Run the MCP gateway (HTTP → ZMQ bridge)
//! - `holler ping <endpoint>` - Test connectivity to a backend
//! - `holler send <endpoint> <json>` - Send raw hooteproto message
//! - `holler lua <endpoint> <code>` - Evaluate Lua code

use anyhow::Result;
use clap::{Parser, Subcommand};

mod backend;
mod client;
mod commands;
mod handler;
mod heartbeat;
mod serve;
mod subscriber;
mod telemetry;

#[derive(Parser)]
#[command(name = "holler")]
#[command(about = "MCP gateway and ZMQ CLI for Hootenanny")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Test connectivity to a ZMQ backend
    Ping {
        /// ZMQ endpoint (e.g., tcp://127.0.0.1:5570)
        endpoint: String,

        /// Timeout in milliseconds
        #[arg(short, long, default_value = "5000")]
        timeout: u64,
    },

    /// Send a raw hooteproto JSON message
    Send {
        /// ZMQ endpoint
        endpoint: String,

        /// JSON payload (Payload type, not Envelope)
        json: String,

        /// Timeout in milliseconds
        #[arg(short, long, default_value = "30000")]
        timeout: u64,
    },

    /// Evaluate Lua code on a Luanette backend
    Lua {
        /// ZMQ endpoint
        endpoint: String,

        /// Lua code to evaluate
        code: String,

        /// Optional JSON params to pass to the script
        #[arg(short, long)]
        params: Option<String>,

        /// Timeout in milliseconds
        #[arg(short, long, default_value = "30000")]
        timeout: u64,
    },

    /// Query job status
    Job {
        /// ZMQ endpoint
        endpoint: String,

        #[command(subcommand)]
        action: JobAction,
    },

    /// Run the MCP gateway server
    Serve {
        /// HTTP port to bind
        #[arg(short, long, default_value = "8080")]
        port: u16,

        /// Luanette ZMQ ROUTER endpoint (for tool calls)
        #[arg(long)]
        luanette: Option<String>,

        /// Hootenanny ZMQ ROUTER endpoint (for tool calls)
        #[arg(long)]
        hootenanny: Option<String>,

        /// Chaosgarden ZMQ ROUTER endpoint (for tool calls)
        #[arg(long)]
        chaosgarden: Option<String>,

        /// Luanette ZMQ PUB endpoint (for broadcasts/SSE)
        #[arg(long)]
        luanette_pub: Option<String>,

        /// Hootenanny ZMQ PUB endpoint (for broadcasts/SSE)
        #[arg(long)]
        hootenanny_pub: Option<String>,

        /// Chaosgarden ZMQ PUB endpoint (for broadcasts/SSE)
        #[arg(long)]
        chaosgarden_pub: Option<String>,

        /// OTLP gRPC endpoint for OpenTelemetry (e.g., "localhost:4317")
        #[arg(long, default_value = "localhost:4317")]
        otlp_endpoint: String,
    },
}

#[derive(Subcommand)]
enum JobAction {
    /// Get status of a specific job
    Status {
        /// Job ID
        job_id: String,

        /// Timeout in milliseconds
        #[arg(short, long, default_value = "5000")]
        timeout: u64,
    },

    /// List all jobs
    List {
        /// Filter by status
        #[arg(short, long)]
        status: Option<String>,

        /// Timeout in milliseconds
        #[arg(short, long, default_value = "5000")]
        timeout: u64,
    },

    /// Poll for job completion
    Poll {
        /// Job IDs to poll
        job_ids: Vec<String>,

        /// Timeout in milliseconds
        #[arg(short, long, default_value = "30000")]
        timeout: u64,

        /// Poll mode: any or all
        #[arg(short, long, default_value = "any")]
        mode: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // For serve command, use full OTEL; for CLI commands, use simple tracing
    let use_otel = matches!(cli.command, Commands::Serve { .. });

    if !use_otel {
        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::from_default_env()
                    .add_directive(tracing::Level::INFO.into()),
            )
            .init();
    }

    match cli.command {
        Commands::Ping { endpoint, timeout } => {
            commands::ping(&endpoint, timeout).await?;
        }
        Commands::Send {
            endpoint,
            json,
            timeout,
        } => {
            commands::send(&endpoint, &json, timeout).await?;
        }
        Commands::Lua {
            endpoint,
            code,
            params,
            timeout,
        } => {
            commands::lua_eval(&endpoint, &code, params.as_deref(), timeout).await?;
        }
        Commands::Job { endpoint, action } => match action {
            JobAction::Status { job_id, timeout } => {
                commands::job_status(&endpoint, &job_id, timeout).await?;
            }
            JobAction::List { status, timeout } => {
                commands::job_list(&endpoint, status.as_deref(), timeout).await?;
            }
            JobAction::Poll {
                job_ids,
                timeout,
                mode,
            } => {
                commands::job_poll(&endpoint, job_ids, timeout, &mode).await?;
            }
        },
        Commands::Serve {
            port,
            luanette,
            hootenanny,
            chaosgarden,
            luanette_pub,
            hootenanny_pub,
            chaosgarden_pub,
            otlp_endpoint,
        } => {
            // Initialize OTEL for serve mode
            telemetry::init(&otlp_endpoint)?;

            serve::run(serve::ServeConfig {
                port,
                luanette,
                hootenanny,
                chaosgarden,
                luanette_pub,
                hootenanny_pub,
                chaosgarden_pub,
            })
            .await?;
        }
    }

    Ok(())
}
