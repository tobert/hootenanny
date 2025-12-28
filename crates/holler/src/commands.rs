//! CLI command implementations

use anyhow::{bail, Context, Result};
use hooteproto::Payload;
use hooteproto::request::{JobStatusRequest, JobListRequest, JobPollRequest, ToolRequest};

use crate::client::Client;

/// Validate that an endpoint looks like a ZMQ URI
fn validate_endpoint(endpoint: &str) -> Result<()> {
    if !endpoint.starts_with("tcp://") && !endpoint.starts_with("ipc://") {
        bail!(
            "Invalid endpoint: '{}'\n\n\
             ZMQ endpoints must be URIs like:\n  \
             tcp://localhost:5580\n  \
             tcp://192.168.1.10:5580\n  \
             ipc:///tmp/hootenanny.sock\n\n\
             Hootenanny default: tcp://localhost:5580",
            endpoint
        );
    }
    Ok(())
}

/// Test connectivity with a Ping/Pong exchange
pub async fn ping(endpoint: &str, timeout_ms: u64) -> Result<()> {
    validate_endpoint(endpoint)?;
    let client = Client::connect(endpoint, timeout_ms).await?;

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
    validate_endpoint(endpoint)?;
    let payload: Payload = serde_json::from_str(json)
        .context("Failed to parse JSON as Payload")?;

    let client = Client::connect(endpoint, timeout_ms).await?;
    let response = client.request(payload).await?;

    let output = serde_json::to_string_pretty(&response.payload)?;
    println!("{}", output);

    Ok(())
}

/// Get status of a specific job
pub async fn job_status(endpoint: &str, job_id: &str, timeout_ms: u64) -> Result<()> {
    validate_endpoint(endpoint)?;
    let payload = Payload::ToolRequest(ToolRequest::JobStatus(JobStatusRequest {
        job_id: job_id.to_string(),
    }));

    let client = Client::connect(endpoint, timeout_ms).await?;
    let response = client.request(payload).await?;

    match response.payload {
        Payload::TypedResponse(envelope) => {
            let result = envelope.to_json();
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
    validate_endpoint(endpoint)?;
    let payload = Payload::ToolRequest(ToolRequest::JobList(JobListRequest {
        status: status.map(|s| s.to_string()),
    }));

    let client = Client::connect(endpoint, timeout_ms).await?;
    let response = client.request(payload).await?;

    match response.payload {
        Payload::TypedResponse(envelope) => {
            let result = envelope.to_json();
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
    validate_endpoint(endpoint)?;
    // Mode is "any" or "all" string in CLI
    let payload = Payload::ToolRequest(ToolRequest::JobPoll(JobPollRequest {
        job_ids,
        timeout_ms,
        mode: Some(mode.to_string()),
    }));

    let client = Client::connect(endpoint, timeout_ms + 5000).await?;
    let response = client.request(payload).await?;

    match response.payload {
        Payload::TypedResponse(envelope) => {
            let result = envelope.to_json();
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