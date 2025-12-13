//! Tests for CAS script execution
//!
//! Verifies that scripts can be stored in CAS and executed via job_execute.

use cas::{ContentStore, FileStore};
use luanette::{Dispatcher, JobStatus};
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;

/// Create a test dispatcher with CAS
fn create_test_dispatcher() -> (Arc<Dispatcher>, Arc<FileStore>, TempDir) {
    use luanette::clients::ClientManager;
    use luanette::job_system::JobStore;
    use luanette::runtime::{LuaRuntime, SandboxConfig};

    let config = SandboxConfig {
        timeout: Duration::from_secs(5),
    };
    let clients = Arc::new(ClientManager::new());
    let runtime = Arc::new(LuaRuntime::with_mcp_bridge(config, clients));
    let jobs = Arc::new(JobStore::new());

    // Create a temporary CAS for testing
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let cas = Arc::new(FileStore::at_path(temp_dir.path()).expect("Failed to create test CAS"));

    let dispatcher = Arc::new(Dispatcher::new(runtime, jobs, cas.clone()));

    (dispatcher, cas, temp_dir)
}

#[tokio::test]
async fn test_job_execute_with_cas() {
    let (dispatcher, cas, _temp_dir) = create_test_dispatcher();

    // 1. Store a Lua script in CAS
    let script = r#"
        function main(params)
            return {
                greeting = "Hello, " .. (params.name or "World") .. "!",
                value = (params.value or 0) * 2
            }
        end
    "#;

    let hash = cas
        .store(script.as_bytes(), "text/x-lua")
        .expect("Failed to store script");

    // 2. Execute the script via job_execute
    let params = serde_json::json!({"name": "CAS", "value": 21});
    let result = dispatcher
        .job_execute(hash.as_str(), params, None)
        .await;

    // 3. Verify job was created
    let job_id = match result {
        hooteproto::Payload::Success { result } => {
            let job_id_str = result
                .get("job_id")
                .and_then(|v| v.as_str())
                .expect("Should have job_id");
            job_id_str.to_string()
        }
        hooteproto::Payload::Error { code, message, .. } => {
            panic!("job_execute failed: {} - {}", code, message);
        }
        _ => panic!("Unexpected payload type"),
    };

    // 4. Poll for completion
    let job_id = hooteproto::JobId::from(job_id);

    // Wait up to 2 seconds for job to complete
    let mut attempts = 0;
    loop {
        if attempts > 20 {
            panic!("Job did not complete in time");
        }

        let status_result = dispatcher.job_status(&job_id.to_string()).await;
        let status = match status_result {
            hooteproto::Payload::Success { result } => {
                let info: serde_json::Value = result;
                let status_str = info
                    .get("status")
                    .and_then(|s| s.as_str())
                    .expect("Should have status");

                match status_str {
                    "pending" => JobStatus::Pending,
                    "running" => JobStatus::Running,
                    "complete" => {
                        // Job complete - verify result
                        let result_value = info.get("result").expect("Should have result");
                        let greeting = result_value
                            .get("greeting")
                            .and_then(|g| g.as_str())
                            .expect("Should have greeting");
                        let value = result_value
                            .get("value")
                            .and_then(|v| v.as_i64())
                            .expect("Should have value");

                        assert_eq!(greeting, "Hello, CAS!");
                        assert_eq!(value, 42);
                        break;
                    }
                    "failed" => {
                        let error = info.get("error").expect("Should have error");
                        panic!("Job failed: {:?}", error);
                    }
                    _ => panic!("Unexpected status: {}", status_str),
                }
            }
            hooteproto::Payload::Error { code, message, .. } => {
                panic!("job_status failed: {} - {}", code, message);
            }
            _ => panic!("Unexpected payload type"),
        };

        if status == JobStatus::Running || status == JobStatus::Pending {
            tokio::time::sleep(Duration::from_millis(100)).await;
            attempts += 1;
        }
    }
}

#[tokio::test]
async fn test_job_execute_invalid_hash() {
    let (dispatcher, _cas, _temp_dir) = create_test_dispatcher();

    // Try to execute with an invalid hash
    let result = dispatcher
        .job_execute("not_a_valid_hash", serde_json::json!({}), None)
        .await;

    match result {
        hooteproto::Payload::Error { code, .. } => {
            assert_eq!(code, "invalid_hash");
        }
        _ => panic!("Expected error for invalid hash"),
    }
}

#[tokio::test]
async fn test_job_execute_missing_script() {
    let (dispatcher, _cas, _temp_dir) = create_test_dispatcher();

    // Use a valid hash format but non-existent content
    let missing_hash = "00000000000000000000000000000000";
    let result = dispatcher
        .job_execute(missing_hash, serde_json::json!({}), None)
        .await;

    match result {
        hooteproto::Payload::Error { code, .. } => {
            assert_eq!(code, "script_not_found");
        }
        _ => panic!("Expected error for missing script"),
    }
}

#[tokio::test]
async fn test_job_execute_non_utf8_content() {
    let (dispatcher, cas, _temp_dir) = create_test_dispatcher();

    // Store invalid UTF-8 content
    let invalid_bytes = vec![0xFF, 0xFE, 0xFD];
    let hash = cas
        .store(&invalid_bytes, "application/octet-stream")
        .expect("Failed to store content");

    let result = dispatcher
        .job_execute(hash.as_str(), serde_json::json!({}), None)
        .await;

    match result {
        hooteproto::Payload::Error { code, .. } => {
            assert_eq!(code, "invalid_script");
        }
        _ => panic!("Expected error for non-UTF8 content"),
    }
}

#[tokio::test]
async fn test_job_execute_lua_error() {
    let (dispatcher, cas, _temp_dir) = create_test_dispatcher();

    // Store a script with a runtime error
    let script = r#"
        function main(params)
            return params.nonexistent.nested.value
        end
    "#;

    let hash = cas
        .store(script.as_bytes(), "text/x-lua")
        .expect("Failed to store script");

    let result = dispatcher
        .job_execute(hash.as_str(), serde_json::json!({}), None)
        .await;

    // Job should be created
    let job_id = match result {
        hooteproto::Payload::Success { result } => result
            .get("job_id")
            .and_then(|v| v.as_str())
            .expect("Should have job_id")
            .to_string(),
        _ => panic!("Expected success creating job"),
    };

    // Wait for job to fail
    let job_id = hooteproto::JobId::from(job_id);
    let mut attempts = 0;
    loop {
        if attempts > 20 {
            panic!("Job did not fail in time");
        }

        let status_result = dispatcher.job_status(&job_id.to_string()).await;
        match status_result {
            hooteproto::Payload::Success { result } => {
                let status_str = result
                    .get("status")
                    .and_then(|s| s.as_str())
                    .expect("Should have status");

                if status_str == "failed" {
                    // Verify we got an error message
                    let error = result.get("error").expect("Should have error");
                    let error_str = error.as_str().expect("Error should be string");
                    // Just verify we got an error - don't check specific message
                    assert!(!error_str.is_empty(), "Error message should not be empty");
                    break;
                } else if status_str == "complete" {
                    panic!("Job should have failed, not completed");
                }
            }
            _ => {}
        }

        tokio::time::sleep(Duration::from_millis(100)).await;
        attempts += 1;
    }
}
