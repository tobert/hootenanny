//! Integration tests for Luanette
//!
//! These tests verify end-to-end functionality of the Lua scripting server.
//! Tests are designed to run without external dependencies (hootenanny).

mod common {
    use std::sync::Arc;
    use std::time::Duration;

    /// Create a test runtime for integration tests
    pub fn create_test_runtime() -> (
        Arc<luanette::runtime::LuaRuntime>,
        Arc<luanette::clients::ClientManager>,
        Arc<luanette::job_system::JobStore>,
    ) {
        use luanette::clients::ClientManager;
        use luanette::job_system::JobStore;
        use luanette::runtime::{LuaRuntime, SandboxConfig};

        let config = SandboxConfig {
            timeout: Duration::from_secs(5),
        };
        let clients = Arc::new(ClientManager::new());
        let runtime = Arc::new(LuaRuntime::with_mcp_bridge(config, clients.clone()));
        let jobs = Arc::new(JobStore::new());

        (runtime, clients, jobs)
    }
}

// ============================================================================
// Basic Execution Tests
// ============================================================================

#[tokio::test]
async fn test_hello_world() {
    let (runtime, _, _) = common::create_test_runtime();

    let code = r#"
        function main(params)
            return "Hello, " .. (params.name or "World") .. "!"
        end
    "#;

    let result = runtime
        .execute(code, serde_json::json!({"name": "Luanette"}))
        .await
        .expect("Execution should succeed");

    assert_eq!(result.result, "Hello, Luanette!");
}

#[tokio::test]
async fn test_simple_math() {
    let (runtime, _, _) = common::create_test_runtime();

    let code = r#"
        function main(params)
            return params.a + params.b
        end
    "#;

    let result = runtime
        .execute(code, serde_json::json!({"a": 10, "b": 32}))
        .await
        .expect("Execution should succeed");

    assert_eq!(result.result, 42);
}

#[tokio::test]
async fn test_table_manipulation() {
    let (runtime, _, _) = common::create_test_runtime();

    let code = r#"
        function main(params)
            local result = {}
            for i, v in ipairs(params.items) do
                table.insert(result, v * 2)
            end
            return result
        end
    "#;

    let result = runtime
        .execute(code, serde_json::json!({"items": [1, 2, 3, 4, 5]}))
        .await
        .expect("Execution should succeed");

    assert_eq!(result.result, serde_json::json!([2, 4, 6, 8, 10]));
}

#[tokio::test]
async fn test_nested_tables() {
    let (runtime, _, _) = common::create_test_runtime();

    let code = r#"
        function main(params)
            return {
                name = params.name,
                nested = {
                    value = params.value,
                    doubled = params.value * 2
                }
            }
        end
    "#;

    let result = runtime
        .execute(code, serde_json::json!({"name": "test", "value": 21}))
        .await
        .expect("Execution should succeed");

    let expected = serde_json::json!({
        "name": "test",
        "nested": {
            "value": 21,
            "doubled": 42
        }
    });
    assert_eq!(result.result, expected);
}

// ============================================================================
// Error Handling Tests
// ============================================================================

#[tokio::test]
async fn test_syntax_error() {
    let (runtime, _, _) = common::create_test_runtime();

    let code = r#"
        function main(params
            return params
        end
    "#;

    let result = runtime.execute(code, serde_json::json!({})).await;
    assert!(result.is_err(), "Should fail with syntax error");

    // Just verify we got an error - specific message varies by Lua version
    let error = result.unwrap_err().to_string();
    assert!(!error.is_empty(), "Error message should not be empty: {}", error);
}

#[tokio::test]
async fn test_runtime_error() {
    let (runtime, _, _) = common::create_test_runtime();

    let code = r#"
        function main(params)
            return params.nonexistent.nested.value
        end
    "#;

    let result = runtime.execute(code, serde_json::json!({})).await;
    assert!(result.is_err(), "Should fail with runtime error");

    let error = result.unwrap_err().to_string();
    // The error wraps the Lua error - it may contain "nil", "index", or be wrapped in "Error calling main"
    assert!(
        error.contains("nil") || error.contains("index") || error.contains("main"),
        "Error should indicate runtime failure: {}",
        error
    );
}

#[tokio::test]
async fn test_missing_main() {
    let (runtime, _, _) = common::create_test_runtime();

    let code = r#"
        function helper(x)
            return x * 2
        end
    "#;

    let result = runtime.execute(code, serde_json::json!({})).await;
    assert!(result.is_err(), "Should fail without main function");

    let error = result.unwrap_err().to_string();
    assert!(
        error.contains("main") || error.contains("nil"),
        "Error should mention missing main: {}",
        error
    );
}

#[tokio::test]
async fn test_type_error() {
    let (runtime, _, _) = common::create_test_runtime();

    let code = r#"
        function main(params)
            return params.value + "string"
        end
    "#;

    let result = runtime.execute(code, serde_json::json!({"value": 42})).await;
    assert!(result.is_err(), "Should fail with type error");
}

// ============================================================================
// Sandbox Tests
// ============================================================================

#[tokio::test]
async fn test_sandbox_blocks_os_execute() {
    let (runtime, _, _) = common::create_test_runtime();

    let code = r#"
        function main(params)
            os.execute("echo hello")
            return "should not reach"
        end
    "#;

    let result = runtime.execute(code, serde_json::json!({})).await;
    assert!(result.is_err(), "os.execute should be blocked");
}

#[tokio::test]
async fn test_sandbox_blocks_dofile() {
    let (runtime, _, _) = common::create_test_runtime();

    let code = r#"
        function main(params)
            dofile("/etc/passwd")
            return "should not reach"
        end
    "#;

    let result = runtime.execute(code, serde_json::json!({})).await;
    assert!(result.is_err(), "dofile should be blocked");
}

#[tokio::test]
async fn test_sandbox_allows_math() {
    let (runtime, _, _) = common::create_test_runtime();

    let code = r#"
        function main(params)
            return {
                sqrt = math.sqrt(16),
                sin = math.sin(0),
                pi = math.pi,
                random = math.random(1, 10)
            }
        end
    "#;

    let result = runtime
        .execute(code, serde_json::json!({}))
        .await
        .expect("math.* should be allowed");

    let obj = result.result.as_object().unwrap();
    assert_eq!(obj.get("sqrt").unwrap(), 4.0);
    assert_eq!(obj.get("sin").unwrap(), 0.0);
    assert!(obj.get("pi").unwrap().as_f64().unwrap() > 3.14);
}

#[tokio::test]
async fn test_sandbox_allows_string() {
    let (runtime, _, _) = common::create_test_runtime();

    let code = r#"
        function main(params)
            return {
                upper = string.upper("hello"),
                lower = string.lower("WORLD"),
                len = string.len("test"),
                sub = string.sub("hello", 1, 3)
            }
        end
    "#;

    let result = runtime
        .execute(code, serde_json::json!({}))
        .await
        .expect("string.* should be allowed");

    let obj = result.result.as_object().unwrap();
    assert_eq!(obj.get("upper").unwrap(), "HELLO");
    assert_eq!(obj.get("lower").unwrap(), "world");
    assert_eq!(obj.get("len").unwrap(), 4);
    assert_eq!(obj.get("sub").unwrap(), "hel");
}

// ============================================================================
// Standard Library Tests
// ============================================================================

#[tokio::test]
async fn test_logging() {
    let (runtime, _, _) = common::create_test_runtime();

    let code = r#"
        function main(params)
            log.info("Test info message")
            log.debug("Test debug message")
            log.warn("Test warning")
            return "logged"
        end
    "#;

    let result = runtime
        .execute(code, serde_json::json!({}))
        .await
        .expect("Logging should work");

    assert_eq!(result.result, "logged");
}

#[tokio::test]
async fn test_describe_function() {
    let (runtime, _, _) = common::create_test_runtime();

    let code = r#"
        function describe()
            return {
                name = "test_script",
                description = "A test script for validation",
                params = {
                    input = { type = "string", required = true },
                    count = { type = "number", required = false, default = 1 }
                },
                returns = "Processed result"
            }
        end

        function main(params)
            return params.input
        end
    "#;

    // First execute to load the script
    let _ = runtime.execute(code, serde_json::json!({"input": "test"})).await;

    // Test that describe function works
    let describe_code = format!(
        r#"
        {}

        if type(describe) == "function" then
            return describe()
        else
            return nil
        end
        "#,
        code
    );

    let result = runtime
        .eval(&describe_code)
        .await
        .expect("describe() should work");

    let obj = result.result.as_object().unwrap();
    assert_eq!(obj.get("name").unwrap(), "test_script");
    assert!(obj.get("description").is_some());
    assert!(obj.get("params").is_some());
}

// ============================================================================
// Job System Tests
// ============================================================================

#[tokio::test]
async fn test_job_lifecycle() {
    use luanette::{JobStatus, JobStore};

    let store = JobStore::new();

    // Create job
    let job_id = store.create_job("test_script".to_string());

    // Check pending
    let info = store.get_job(&job_id).expect("Job should exist");
    assert_eq!(info.status, JobStatus::Pending);

    // Mark running
    store.mark_running(&job_id).expect("Should mark running");
    let info = store.get_job(&job_id).expect("Job should exist");
    assert_eq!(info.status, JobStatus::Running);

    // Mark complete
    store
        .mark_complete(&job_id, serde_json::json!({"result": "done"}))
        .expect("Should mark complete");
    let info = store.get_job(&job_id).expect("Job should exist");
    assert_eq!(info.status, JobStatus::Complete);
    assert_eq!(info.result, Some(serde_json::json!({"result": "done"})));
}

#[tokio::test]
async fn test_job_cancellation() {
    use luanette::{JobStatus, JobStore};

    let store = JobStore::new();
    let job_id = store.create_job("long_script".to_string());

    store.mark_running(&job_id).unwrap();
    store.cancel_job(&job_id).expect("Should cancel");

    let info = store.get_job(&job_id).expect("Job should exist");
    assert_eq!(info.status, JobStatus::Cancelled);
}

// ============================================================================
// Complex Script Tests
// ============================================================================

#[tokio::test]
async fn test_recursive_function() {
    let (runtime, _, _) = common::create_test_runtime();

    let code = r#"
        function factorial(n)
            if n <= 1 then
                return 1
            else
                return n * factorial(n - 1)
            end
        end

        function main(params)
            return factorial(params.n)
        end
    "#;

    let result = runtime
        .execute(code, serde_json::json!({"n": 5}))
        .await
        .expect("Recursive function should work");

    assert_eq!(result.result, 120);
}

#[tokio::test]
async fn test_closures() {
    let (runtime, _, _) = common::create_test_runtime();

    let code = r#"
        function make_counter(start)
            local count = start
            return function()
                count = count + 1
                return count
            end
        end

        function main(params)
            local counter = make_counter(params.start)
            return {
                first = counter(),
                second = counter(),
                third = counter()
            }
        end
    "#;

    let result = runtime
        .execute(code, serde_json::json!({"start": 10}))
        .await
        .expect("Closures should work");

    let obj = result.result.as_object().unwrap();
    assert_eq!(obj.get("first").unwrap(), 11);
    assert_eq!(obj.get("second").unwrap(), 12);
    assert_eq!(obj.get("third").unwrap(), 13);
}

#[tokio::test]
async fn test_error_handling_pcall() {
    let (runtime, _, _) = common::create_test_runtime();

    let code = r#"
        function risky_operation()
            error("Something went wrong!")
        end

        function main(params)
            local ok, err = pcall(risky_operation)
            return {
                success = ok,
                error_caught = not ok
            }
        end
    "#;

    let result = runtime
        .execute(code, serde_json::json!({}))
        .await
        .expect("pcall should handle errors");

    let obj = result.result.as_object().unwrap();
    assert_eq!(obj.get("success").unwrap(), false);
    assert_eq!(obj.get("error_caught").unwrap(), true);
}

// ============================================================================
// MIDI Library Tests (if available)
// ============================================================================

#[tokio::test]
async fn test_midi_transpose_available() {
    let (runtime, _, _) = common::create_test_runtime();

    let code = r#"
        function main(params)
            -- Test that midi module is available
            if midi and midi.transpose then
                local events = {
                    { type = "note_on", note = 60 },
                    { type = "note_on", note = 64 }
                }
                midi.transpose(events, 12)
                return events
            else
                return { error = "midi module not available" }
            end
        end
    "#;

    let result = runtime
        .execute(code, serde_json::json!({}))
        .await
        .expect("MIDI test should run");

    // If midi is available, notes should be transposed
    if let Some(arr) = result.result.as_array() {
        if arr.len() > 0 && arr[0].get("note").is_some() {
            assert_eq!(arr[0].get("note").unwrap(), 72);
            assert_eq!(arr[1].get("note").unwrap(), 76);
        }
    }
}

// ============================================================================
// Temp File Tests
// ============================================================================

#[tokio::test]
async fn test_temp_path_available() {
    let (runtime, _, _) = common::create_test_runtime();

    let code = r#"
        function main(params)
            if temp and temp.path then
                local path = temp.path("test.txt")
                return {
                    has_temp = true,
                    path_type = type(path)
                }
            else
                return { has_temp = false }
            end
        end
    "#;

    let result = runtime
        .execute(code, serde_json::json!({}))
        .await
        .expect("Temp test should run");

    let obj = result.result.as_object().unwrap();
    if obj.get("has_temp").unwrap() == true {
        assert_eq!(obj.get("path_type").unwrap(), "string");
    }
}

// ============================================================================
// Live Server Integration Tests
// ============================================================================
// These tests require a running luanette server on port 8081.
// Run with: cargo test --test integration -- --ignored

mod live_server {
    use reqwest::Client;
    use serde_json::{json, Value};
    use std::time::Duration;

    const LUANETTE_URL: &str = "http://localhost:8081/mcp";
    const SESSION_ID: &str = "integration-test-session";

    async fn mcp_request(client: &Client, method: &str, params: Value) -> Result<Value, String> {
        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params
        });

        let response = client
            .post(LUANETTE_URL)
            .header("Content-Type", "application/json")
            .header("Mcp-Session-Id", SESSION_ID)
            .json(&request)
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?;

        let body: Value = response
            .json()
            .await
            .map_err(|e| format!("Parse failed: {}", e))?;

        if let Some(error) = body.get("error") {
            return Err(format!("MCP error: {}", error));
        }

        Ok(body.get("result").cloned().unwrap_or(Value::Null))
    }

    async fn initialize_session(client: &Client) -> Result<(), String> {
        let result = mcp_request(
            client,
            "initialize",
            json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                    "name": "integration-test",
                    "version": "1.0.0"
                }
            }),
        )
        .await?;

        if result.get("protocolVersion").is_some() {
            Ok(())
        } else {
            Err("Initialize failed: no protocolVersion".to_string())
        }
    }

    fn is_server_available() -> bool {
        std::process::Command::new("curl")
            .args(["-s", "-o", "/dev/null", "-w", "%{http_code}", "http://localhost:8081/health"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "200")
            .unwrap_or(false)
    }

    #[tokio::test]
    #[ignore] // Run with: cargo test --test integration -- --ignored
    async fn test_live_health_endpoint() {
        if !is_server_available() {
            eprintln!("Skipping: luanette server not running on port 8081");
            return;
        }

        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap();

        let response = client
            .get("http://localhost:8081/health")
            .send()
            .await
            .expect("Health check should succeed");

        assert!(response.status().is_success());

        let body: Value = response.json().await.expect("Should parse JSON");
        assert_eq!(body.get("status").and_then(|v| v.as_str()), Some("healthy"));
    }

    #[tokio::test]
    #[ignore]
    async fn test_live_mcp_initialize() {
        if !is_server_available() {
            eprintln!("Skipping: luanette server not running on port 8081");
            return;
        }

        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap();

        let result = initialize_session(&client).await;
        assert!(result.is_ok(), "Initialize should succeed: {:?}", result);
    }

    #[tokio::test]
    #[ignore]
    async fn test_live_tools_list() {
        if !is_server_available() {
            eprintln!("Skipping: luanette server not running on port 8081");
            return;
        }

        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap();

        initialize_session(&client).await.expect("Init should work");

        let result = mcp_request(&client, "tools/list", json!({}))
            .await
            .expect("tools/list should succeed");

        let tools = result.get("tools").and_then(|t| t.as_array());
        assert!(tools.is_some(), "Should have tools array");

        let tools = tools.unwrap();
        assert!(!tools.is_empty(), "Should have at least one tool");

        // Check for expected tools
        let tool_names: Vec<&str> = tools
            .iter()
            .filter_map(|t| t.get("name").and_then(|n| n.as_str()))
            .collect();

        assert!(
            tool_names.contains(&"lua_eval"),
            "Should have lua_eval tool"
        );
        assert!(
            tool_names.contains(&"lua_describe"),
            "Should have lua_describe tool"
        );
    }

    #[tokio::test]
    #[ignore]
    async fn test_live_lua_eval() {
        if !is_server_available() {
            eprintln!("Skipping: luanette server not running on port 8081");
            return;
        }

        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap();

        initialize_session(&client).await.expect("Init should work");

        let result = mcp_request(
            &client,
            "tools/call",
            json!({
                "name": "lua_eval",
                "arguments": {
                    "code": "return 2 + 2"
                }
            }),
        )
        .await
        .expect("lua_eval should succeed");

        // Result is in content array
        let content = result.get("content").and_then(|c| c.as_array());
        assert!(content.is_some(), "Should have content array");

        let text = content
            .unwrap()
            .iter()
            .find(|c| c.get("type").and_then(|t| t.as_str()) == Some("text"))
            .and_then(|c| c.get("text").and_then(|t| t.as_str()));

        assert!(text.is_some(), "Should have text content");
        let parsed: Value = serde_json::from_str(text.unwrap()).expect("Should parse result");
        assert_eq!(parsed.get("result"), Some(&json!(4)));
    }

    #[tokio::test]
    #[ignore]
    async fn test_live_lua_eval_with_main() {
        if !is_server_available() {
            eprintln!("Skipping: luanette server not running on port 8081");
            return;
        }

        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap();

        initialize_session(&client).await.expect("Init should work");

        let code = r#"
            function main(params)
                return {
                    greeting = "Hello, " .. (params.name or "World") .. "!",
                    computed = params.value * 2
                }
            end
        "#;

        let result = mcp_request(
            &client,
            "tools/call",
            json!({
                "name": "lua_eval",
                "arguments": {
                    "code": code,
                    "params": {
                        "name": "Luanette",
                        "value": 21
                    }
                }
            }),
        )
        .await
        .expect("lua_eval with main should succeed");

        let content = result.get("content").and_then(|c| c.as_array());
        let text = content
            .unwrap()
            .iter()
            .find(|c| c.get("type").and_then(|t| t.as_str()) == Some("text"))
            .and_then(|c| c.get("text").and_then(|t| t.as_str()));

        let parsed: Value = serde_json::from_str(text.unwrap()).expect("Should parse result");
        let result_obj = parsed.get("result").and_then(|r| r.as_object());
        assert!(result_obj.is_some());

        let result_obj = result_obj.unwrap();
        assert_eq!(
            result_obj.get("greeting"),
            Some(&json!("Hello, Luanette!"))
        );
        assert_eq!(result_obj.get("computed"), Some(&json!(42)));
    }

    #[tokio::test]
    #[ignore]
    async fn test_live_lua_eval_error_handling() {
        if !is_server_available() {
            eprintln!("Skipping: luanette server not running on port 8081");
            return;
        }

        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap();

        initialize_session(&client).await.expect("Init should work");

        let result = mcp_request(
            &client,
            "tools/call",
            json!({
                "name": "lua_eval",
                "arguments": {
                    "code": "this is not valid lua syntax!"
                }
            }),
        )
        .await;

        // Should return an error response, not crash
        assert!(
            result.is_err() || {
                let r = result.unwrap();
                r.get("isError") == Some(&json!(true))
            },
            "Should indicate error for invalid syntax"
        );
    }

    #[tokio::test]
    #[ignore]
    async fn test_live_otel_functions() {
        if !is_server_available() {
            eprintln!("Skipping: luanette server not running on port 8081");
            return;
        }

        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap();

        initialize_session(&client).await.expect("Init should work");

        let code = r#"
            function main(params)
                local trace_id = otel.trace_id()
                local span_id = otel.span_id()
                local traceparent = otel.traceparent()

                otel.set_attribute("test.attribute", "value")
                otel.event("test_event", { key = "value" })
                otel.record_metric("test.metric", 42)

                return {
                    has_trace_id = trace_id ~= nil,
                    has_span_id = span_id ~= nil,
                    has_traceparent = traceparent ~= nil,
                    trace_id_len = trace_id and #trace_id or 0
                }
            end
        "#;

        let result = mcp_request(
            &client,
            "tools/call",
            json!({
                "name": "lua_eval",
                "arguments": {
                    "code": code
                }
            }),
        )
        .await
        .expect("otel test should succeed");

        let content = result.get("content").and_then(|c| c.as_array());
        let text = content
            .unwrap()
            .iter()
            .find(|c| c.get("type").and_then(|t| t.as_str()) == Some("text"))
            .and_then(|c| c.get("text").and_then(|t| t.as_str()));

        let parsed: Value = serde_json::from_str(text.unwrap()).expect("Should parse result");
        let result_obj = parsed.get("result").and_then(|r| r.as_object());
        assert!(result_obj.is_some());

        let result_obj = result_obj.unwrap();
        // Trace IDs should be available
        assert_eq!(result_obj.get("has_trace_id"), Some(&json!(true)));
        assert_eq!(result_obj.get("has_span_id"), Some(&json!(true)));
        assert_eq!(result_obj.get("has_traceparent"), Some(&json!(true)));
        // Trace ID should be 32 hex chars
        assert_eq!(result_obj.get("trace_id_len"), Some(&json!(32)));
    }
}
