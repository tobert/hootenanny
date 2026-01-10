//! holler - MCP gateway and ZMQ CLI for the Hootenanny system
//!
//! Subcommands:
//! - `holler serve` - Run the MCP gateway (HTTP transport, stateful)
//! - `holler mcp` - Run MCP server over stdio (for Claude Code)
//! - `holler ping <endpoint>` - Test connectivity to a backend
//! - `holler send <endpoint> <json>` - Send raw hooteproto message
//! - `holler job <endpoint> <action>` - Query job status
//!
//! Configuration is loaded from (in order, later wins):
//! 1. Compiled defaults
//! 2. /etc/hootenanny/config.toml
//! 3. ~/.config/hootenanny/config.toml
//! 4. ./hootenanny.toml (or --config path)
//! 5. Environment variables (HOLLER_*)

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use hooteconf::HootConfig;
use std::path::PathBuf;

use holler::{commands, serve, stdio, telemetry};

/// MCP gateway and ZMQ CLI for Hootenanny
///
/// Configuration is loaded from (in order, later wins):
/// 1. Compiled defaults
/// 2. /etc/hootenanny/config.toml
/// 3. ~/.config/hootenanny/config.toml
/// 4. ./hootenanny.toml (or --config path)
/// 5. Environment variables (HOLLER_*)
#[derive(Parser)]
#[command(name = "holler")]
#[command(about = "MCP gateway and ZMQ CLI for Hootenanny")]
#[command(version)]
struct Cli {
    /// Path to config file (overrides ./hootenanny.toml)
    #[arg(short, long, global = true)]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Test connectivity to a ZMQ backend
    ///
    /// Example: holler ping tcp://localhost:5580
    #[command(after_help = "EXAMPLES:\n    holler ping tcp://localhost:5580\n    holler ping tcp://192.168.1.10:5580 -t 10000")]
    Ping {
        /// ZMQ endpoint URL (must be tcp://host:port format)
        #[arg(value_name = "URL")]
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

    /// Query job status
    Job {
        /// ZMQ endpoint
        endpoint: String,

        #[command(subcommand)]
        action: JobAction,
    },

    /// Run the MCP gateway server (HTTP transport)
    Serve {
        /// Show loaded configuration and exit
        #[arg(long)]
        show_config: bool,

        /// Only expose DAW tools (sample, extend, analyze, bridge, project, schedule)
        ///
        /// This mode provides a focused, high-level interface for DAW
        /// operations without exposing the full tool surface.
        #[arg(long)]
        daw_only: bool,
    },

    /// Run MCP server over stdio (for Claude Code)
    ///
    /// This transport is simpler than HTTP and works well with
    /// Claude Code and other stdio-based MCP clients.
    Mcp {
        /// Only expose DAW tools (sample, extend, analyze, bridge, project, schedule)
        ///
        /// This mode provides a focused, high-level interface for DAW
        /// operations without exposing the full tool surface.
        #[arg(long)]
        daw_only: bool,
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

    // For serve command, use full OTEL
    // For mcp (stdio), use stderr logging (stdout is for MCP protocol)
    // For CLI commands, use simple tracing
    let use_otel = matches!(cli.command, Commands::Serve { .. });
    let use_stderr = matches!(cli.command, Commands::Mcp { .. });

    if use_stderr {
        // Stdio transport: log to stderr to keep stdout clean for MCP
        tracing_subscriber::fmt()
            .with_writer(std::io::stderr)
            .with_env_filter(
                tracing_subscriber::EnvFilter::from_default_env()
                    .add_directive(tracing::Level::WARN.into()),
            )
            .init();
    } else if !use_otel {
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
        Commands::Serve { show_config, daw_only } => {
            // Load configuration from files + env
            let (config, sources) = HootConfig::load_with_sources_from(cli.config.as_deref())
                .context("Failed to load configuration")?;

            // Show config and exit if requested
            if show_config {
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

            // Initialize OTEL for serve mode
            telemetry::init(&config.infra.telemetry.otlp_endpoint)?;

            tracing::info!("📋 Configuration loaded from:");
            for path in &sources.files {
                tracing::info!("   - {}", path.display());
            }
            if !sources.env_overrides.is_empty() {
                tracing::info!("   Environment overrides: {:?}", sources.env_overrides);
            }
            if daw_only {
                tracing::info!("🎛️ DAW-only mode: exposing only DAW tools");
            }

            serve::run(serve::ServeConfig {
                port: config.infra.gateway.http_port,
                hootenanny: config.infra.gateway.hootenanny,
                hootenanny_pub: Some(config.infra.gateway.hootenanny_pub),
                timeout_ms: config.infra.gateway.timeout_ms,
                daw_only,
            })
            .await?;
        }
        Commands::Mcp { daw_only } => {
            // Load configuration for hootenanny endpoint
            let (config, _) = HootConfig::load_with_sources_from(cli.config.as_deref())
                .context("Failed to load configuration")?;

            if daw_only {
                eprintln!("🎛️ DAW-only mode: exposing only DAW tools");
            }

            stdio::run(stdio::StdioConfig {
                hootenanny: config.infra.gateway.hootenanny,
                timeout_ms: config.infra.gateway.timeout_ms,
                daw_only,
            })
            .await?;
        }
    }

    Ok(())
}
