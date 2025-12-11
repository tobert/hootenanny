//! CLI command implementations

use anyhow::{bail, Context, Result};
use hooteproto::{Payload, PollMode};

use crate::client::Client;

/// Test connectivity with a Ping/Pong exchange
pub async fn ping(endpoint: &str, timeout_ms: u64) -> Result<()> {
    let mut client = Client::connect(endpoint, timeout_ms).await?;

    let start = std::time::Instant::now();
    let response = client.request(Payload::Ping).await?;
    let elapsed = start.elapsed();

    match response.payload {
        Payload::Pong {
            worker_id,
            uptime_secs,
        } => {
            println!("Pong from {} (uptime: {}s) in {:?}", worker_id, uptime_secs, elapsed);
            Ok(())
        }
        Payload::Error { code, message, .. } => {
            bail!("Error {}: {}", code, message);
        }
        other => {
            bail!("Unexpected response: {:?}", other);
        }
    }
}

/// Send a raw JSON payload and print the response
pub async fn send(endpoint: &str, json: &str, timeout_ms: u64) -> Result<()> {
    let payload: Payload = serde_json::from_str(json)
        .context("Failed to parse JSON as Payload")?;

    let mut client = Client::connect(endpoint, timeout_ms).await?;
    let response = client.request(payload).await?;

    let output = serde_json::to_string_pretty(&response.payload)?;
    println!("{}", output);

    Ok(())
}

/// Evaluate Lua code and print the result
pub async fn lua_eval(
    endpoint: &str,
    code: &str,
    params: Option<&str>,
    timeout_ms: u64,
) -> Result<()> {
    let params = match params {
        Some(p) => Some(serde_json::from_str(p).context("Failed to parse params as JSON")?),
        None => None,
    };

    let payload = Payload::LuaEval {
        code: code.to_string(),
        params,
    };

    let mut client = Client::connect(endpoint, timeout_ms).await?;
    let response = client.request(payload).await?;

    match response.payload {
        Payload::Success { result } => {
            let output = serde_json::to_string_pretty(&result)?;
            println!("{}", output);
            Ok(())
        }
        Payload::Error { code, message, details } => {
            eprintln!("Error {}: {}", code, message);
            if let Some(d) = details {
                eprintln!("Details: {}", serde_json::to_string_pretty(&d)?);
            }
            std::process::exit(1);
        }
        other => {
            bail!("Unexpected response: {:?}", other);
        }
    }
}

/// Get status of a specific job
pub async fn job_status(endpoint: &str, job_id: &str, timeout_ms: u64) -> Result<()> {
    let payload = Payload::JobStatus {
        job_id: job_id.to_string(),
    };

    let mut client = Client::connect(endpoint, timeout_ms).await?;
    let response = client.request(payload).await?;

    match response.payload {
        Payload::Success { result } => {
            let output = serde_json::to_string_pretty(&result)?;
            println!("{}", output);
            Ok(())
        }
        Payload::Error { code, message, .. } => {
            bail!("Error {}: {}", code, message);
        }
        other => {
            bail!("Unexpected response: {:?}", other);
        }
    }
}

/// List all jobs
pub async fn job_list(endpoint: &str, status: Option<&str>, timeout_ms: u64) -> Result<()> {
    let payload = Payload::JobList {
        status: status.map(|s| s.to_string()),
    };

    let mut client = Client::connect(endpoint, timeout_ms).await?;
    let response = client.request(payload).await?;

    match response.payload {
        Payload::Success { result } => {
            let output = serde_json::to_string_pretty(&result)?;
            println!("{}", output);
            Ok(())
        }
        Payload::Error { code, message, .. } => {
            bail!("Error {}: {}", code, message);
        }
        other => {
            bail!("Unexpected response: {:?}", other);
        }
    }
}

/// Poll for job completion
pub async fn job_poll(
    endpoint: &str,
    job_ids: Vec<String>,
    timeout_ms: u64,
    mode: &str,
) -> Result<()> {
    let mode = match mode.to_lowercase().as_str() {
        "any" => PollMode::Any,
        "all" => PollMode::All,
        _ => bail!("Invalid poll mode: {} (expected 'any' or 'all')", mode),
    };

    let payload = Payload::JobPoll {
        job_ids,
        timeout_ms,
        mode,
    };

    let mut client = Client::connect(endpoint, timeout_ms + 5000).await?;
    let response = client.request(payload).await?;

    match response.payload {
        Payload::Success { result } => {
            let output = serde_json::to_string_pretty(&result)?;
            println!("{}", output);
            Ok(())
        }
        Payload::Error { code, message, .. } => {
            bail!("Error {}: {}", code, message);
        }
        other => {
            bail!("Unexpected response: {:?}", other);
        }
    }
}
