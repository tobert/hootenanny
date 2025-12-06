//! Lua runtime with sandbox and async execution support.

use anyhow::{Context, Result};
use mlua::{Lua, Value as LuaValue};
use serde_json::Value as JsonValue;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::timeout;

use crate::clients::ClientManager;
use crate::otel_bridge::register_otel_globals;
use crate::tool_bridge::{register_mcp_globals, McpBridgeContext};

/// Configuration for the Lua sandbox.
#[derive(Debug, Clone)]
pub struct SandboxConfig {
    /// Maximum execution time before timeout.
    pub timeout: Duration,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
        }
    }
}

/// Result of executing a Lua script.
#[derive(Debug)]
pub struct ExecutionResult {
    /// The value returned by the script's main() function.
    pub result: JsonValue,

    /// How long the script took to execute.
    pub duration: Duration,
}

/// Lua runtime for executing scripts in a sandboxed environment.
pub struct LuaRuntime {
    config: SandboxConfig,

    /// Optional MCP bridge context for calling upstream tools.
    mcp_context: Option<McpBridgeContext>,
}

impl LuaRuntime {
    /// Create a new Lua runtime with the given configuration.
    pub fn new(config: SandboxConfig) -> Self {
        Self {
            config,
            mcp_context: None,
        }
    }

    /// Create a new Lua runtime with default configuration.
    pub fn with_defaults() -> Self {
        Self::new(SandboxConfig::default())
    }

    /// Create a new Lua runtime with MCP bridge enabled.
    ///
    /// This allows Lua scripts to call upstream MCP tools via `mcp.*` globals.
    pub fn with_mcp_bridge(config: SandboxConfig, clients: Arc<ClientManager>) -> Self {
        let runtime_handle = tokio::runtime::Handle::current();
        let mcp_context = McpBridgeContext::new(clients, runtime_handle);

        Self {
            config,
            mcp_context: Some(mcp_context),
        }
    }

    /// Execute Lua source code with parameters.
    ///
    /// The script must define a `main(params)` function that will be called
    /// with the provided parameters.
    pub async fn execute(&self, source: &str, params: JsonValue) -> Result<ExecutionResult> {
        let source = source.to_string();
        let config_timeout = self.config.timeout;
        let mcp_context = self.mcp_context.clone();

        // Run Lua in blocking thread pool to avoid blocking the async runtime
        let result = timeout(config_timeout, async {
            tokio::task::spawn_blocking(move || {
                execute_blocking(&source, params, mcp_context.as_ref())
            })
            .await
            .context("Lua execution task panicked")?
        })
        .await
        .context("Script execution timed out")??;

        Ok(result)
    }

    /// Evaluate a simple Lua expression and return the result.
    ///
    /// Unlike `execute`, this doesn't require a main() function.
    /// The last expression in the code is returned.
    pub async fn eval(&self, code: &str) -> Result<ExecutionResult> {
        let code = code.to_string();
        let config_timeout = self.config.timeout;
        let mcp_context = self.mcp_context.clone();

        let result = timeout(config_timeout, async {
            tokio::task::spawn_blocking(move || {
                eval_blocking(&code, mcp_context.as_ref())
            })
            .await
            .context("Lua eval task panicked")?
        })
        .await
        .context("Eval timed out")??;

        Ok(result)
    }
}

/// Execute Lua code synchronously (called from spawn_blocking).
fn execute_blocking(
    source: &str,
    params: JsonValue,
    mcp_context: Option<&McpBridgeContext>,
) -> Result<ExecutionResult> {
    let start = Instant::now();

    let lua = create_sandboxed_lua(mcp_context)?;

    // Load and execute the script to define functions
    lua.load(source)
        .exec()
        .context("Failed to load Lua script")?;

    // Get the main function
    let main_fn: mlua::Function = lua
        .globals()
        .get("main")
        .context("Script must define a main(params) function")?;

    // Convert JSON params to Lua value
    let lua_params = json_to_lua(&lua, &params)?;

    // Call main(params)
    let result: LuaValue = main_fn
        .call(lua_params)
        .context("Error calling main(params)")?;

    // Convert result back to JSON
    let json_result = lua_to_json(&result)?;

    Ok(ExecutionResult {
        result: json_result,
        duration: start.elapsed(),
    })
}

/// Evaluate Lua code synchronously (called from spawn_blocking).
fn eval_blocking(code: &str, mcp_context: Option<&McpBridgeContext>) -> Result<ExecutionResult> {
    let start = Instant::now();

    let lua = create_sandboxed_lua(mcp_context)?;

    // Evaluate the code
    let result: LuaValue = lua
        .load(code)
        .eval()
        .context("Failed to evaluate Lua code")?;

    // Convert result back to JSON
    let json_result = lua_to_json(&result)?;

    Ok(ExecutionResult {
        result: json_result,
        duration: start.elapsed(),
    })
}

/// Create a sandboxed Lua VM with restricted globals.
fn create_sandboxed_lua(mcp_context: Option<&McpBridgeContext>) -> Result<Lua> {
    let lua = Lua::new();

    // Register standard library functions we want to allow
    register_stdlib(&lua)?;

    // Remove dangerous globals
    remove_dangerous_globals(&lua)?;

    // Register OpenTelemetry globals (otel.*)
    register_otel_globals(&lua)
        .context("Failed to register OpenTelemetry globals")?;

    // Register MCP globals if context is available
    if let Some(ctx) = mcp_context {
        register_mcp_globals(&lua, ctx.clone())
            .context("Failed to register MCP globals")?;
    }

    Ok(lua)
}

/// Register safe standard library functions.
fn register_stdlib(lua: &Lua) -> Result<()> {
    let globals = lua.globals();

    // Create log table with info, warn, error, debug functions
    let log_table = lua.create_table()?;

    let log_info = lua.create_function(|_, msg: String| {
        tracing::info!(target: "luanette.script", "{}", msg);
        Ok(())
    })?;

    let log_warn = lua.create_function(|_, msg: String| {
        tracing::warn!(target: "luanette.script", "{}", msg);
        Ok(())
    })?;

    let log_error = lua.create_function(|_, msg: String| {
        tracing::error!(target: "luanette.script", "{}", msg);
        Ok(())
    })?;

    let log_debug = lua.create_function(|_, msg: String| {
        tracing::debug!(target: "luanette.script", "{}", msg);
        Ok(())
    })?;

    log_table.set("info", log_info)?;
    log_table.set("warn", log_warn)?;
    log_table.set("error", log_error)?;
    log_table.set("debug", log_debug)?;

    globals.set("log", log_table)?;

    Ok(())
}

/// Remove dangerous globals that could be used to escape the sandbox.
fn remove_dangerous_globals(lua: &Lua) -> Result<()> {
    let globals = lua.globals();

    // Remove filesystem access
    globals.set("dofile", LuaValue::Nil)?;
    globals.set("loadfile", LuaValue::Nil)?;

    // Remove dynamic code loading (keep load for internal use only)
    // Note: require is not available by default in mlua

    // Restrict os table to safe functions only
    let os_table: mlua::Table = globals.get("os")?;
    os_table.set("execute", LuaValue::Nil)?;
    os_table.set("exit", LuaValue::Nil)?;
    os_table.set("remove", LuaValue::Nil)?;
    os_table.set("rename", LuaValue::Nil)?;
    os_table.set("setenv", LuaValue::Nil)?;
    os_table.set("setlocale", LuaValue::Nil)?;
    os_table.set("tmpname", LuaValue::Nil)?;
    // Keep: os.clock, os.date, os.difftime, os.getenv, os.time

    // Remove debug library entirely
    globals.set("debug", LuaValue::Nil)?;

    // Remove io library for now (can enable later with restrictions)
    globals.set("io", LuaValue::Nil)?;

    Ok(())
}

/// Convert a JSON value to a Lua value.
fn json_to_lua(lua: &Lua, json: &JsonValue) -> Result<LuaValue> {
    match json {
        JsonValue::Null => Ok(LuaValue::Nil),
        JsonValue::Bool(b) => Ok(LuaValue::Boolean(*b)),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(LuaValue::Integer(i))
            } else if let Some(f) = n.as_f64() {
                Ok(LuaValue::Number(f))
            } else {
                Ok(LuaValue::Nil)
            }
        }
        JsonValue::String(s) => Ok(LuaValue::String(lua.create_string(s)?)),
        JsonValue::Array(arr) => {
            let table = lua.create_table()?;
            for (i, v) in arr.iter().enumerate() {
                table.set(i + 1, json_to_lua(lua, v)?)?;
            }
            Ok(LuaValue::Table(table))
        }
        JsonValue::Object(obj) => {
            let table = lua.create_table()?;
            for (k, v) in obj {
                table.set(k.as_str(), json_to_lua(lua, v)?)?;
            }
            Ok(LuaValue::Table(table))
        }
    }
}

/// Convert a Lua value to a JSON value.
fn lua_to_json(lua_val: &LuaValue) -> Result<JsonValue> {
    match lua_val {
        LuaValue::Nil => Ok(JsonValue::Null),
        LuaValue::Boolean(b) => Ok(JsonValue::Bool(*b)),
        LuaValue::Integer(i) => Ok(JsonValue::Number((*i).into())),
        LuaValue::Number(n) => {
            serde_json::Number::from_f64(*n)
                .map(JsonValue::Number)
                .ok_or_else(|| anyhow::anyhow!("Cannot convert NaN/Inf to JSON"))
        }
        LuaValue::String(s) => Ok(JsonValue::String(s.to_str()?.to_string())),
        LuaValue::Table(table) => {
            // Check if it's an array (sequential integer keys starting at 1)
            let len = table.raw_len();
            let is_array = len > 0 && {
                let mut is_seq = true;
                for i in 1..=len {
                    if table.raw_get::<LuaValue>(i as i64).is_err() {
                        is_seq = false;
                        break;
                    }
                }
                is_seq
            };

            if is_array && len > 0 {
                let mut arr = Vec::with_capacity(len);
                for i in 1..=len {
                    let v: LuaValue = table.raw_get(i as i64)?;
                    arr.push(lua_to_json(&v)?);
                }
                Ok(JsonValue::Array(arr))
            } else {
                let mut obj = serde_json::Map::new();
                for pair in table.pairs::<LuaValue, LuaValue>() {
                    let (k, v) = pair?;
                    let key = match k {
                        LuaValue::String(s) => s.to_str()?.to_string(),
                        LuaValue::Integer(i) => i.to_string(),
                        LuaValue::Number(n) => n.to_string(),
                        _ => continue, // Skip non-string/number keys
                    };
                    obj.insert(key, lua_to_json(&v)?);
                }
                Ok(JsonValue::Object(obj))
            }
        }
        LuaValue::Function(_) => Ok(JsonValue::String("[function]".to_string())),
        LuaValue::Thread(_) => Ok(JsonValue::String("[thread]".to_string())),
        LuaValue::UserData(_) => Ok(JsonValue::String("[userdata]".to_string())),
        LuaValue::LightUserData(_) => Ok(JsonValue::String("[lightuserdata]".to_string())),
        LuaValue::Error(e) => Ok(JsonValue::String(format!("[error: {}]", e))),
        _ => Ok(JsonValue::Null),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_simple_return() {
        let runtime = LuaRuntime::with_defaults();
        let result = runtime
            .execute(
                r#"
                function main(params)
                    return "Hello, " .. (params.name or "World") .. "!"
                end
                "#,
                serde_json::json!({"name": "Luanette"}),
            )
            .await
            .unwrap();

        assert_eq!(result.result, "Hello, Luanette!");
    }

    #[tokio::test]
    async fn test_table_return() {
        let runtime = LuaRuntime::with_defaults();
        let result = runtime
            .execute(
                r#"
                function main(params)
                    return {
                        greeting = "Hello",
                        count = params.n * 2,
                        items = {1, 2, 3}
                    }
                end
                "#,
                serde_json::json!({"n": 5}),
            )
            .await
            .unwrap();

        let obj = result.result.as_object().unwrap();
        assert_eq!(obj["greeting"], "Hello");
        assert_eq!(obj["count"], 10);
        assert_eq!(obj["items"], serde_json::json!([1, 2, 3]));
    }

    #[tokio::test]
    async fn test_logging() {
        let runtime = LuaRuntime::with_defaults();
        let result = runtime
            .execute(
                r#"
                function main(params)
                    log.info("Processing request")
                    log.debug("Debug info")
                    return { status = "ok" }
                end
                "#,
                serde_json::json!({}),
            )
            .await
            .unwrap();

        assert_eq!(result.result["status"], "ok");
    }

    #[tokio::test]
    async fn test_sandbox_blocks_os_execute() {
        let runtime = LuaRuntime::with_defaults();
        let result = runtime
            .execute(
                r#"
                function main(params)
                    os.execute("echo dangerous")
                    return "should not reach"
                end
                "#,
                serde_json::json!({}),
            )
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_sandbox_blocks_io() {
        let runtime = LuaRuntime::with_defaults();
        let result = runtime
            .execute(
                r#"
                function main(params)
                    local f = io.open("/etc/passwd", "r")
                    return "should not reach"
                end
                "#,
                serde_json::json!({}),
            )
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_eval_simple() {
        let runtime = LuaRuntime::with_defaults();
        let result = runtime.eval("return 2 + 2").await.unwrap();
        assert_eq!(result.result, 4);
    }

    #[tokio::test]
    async fn test_missing_main() {
        let runtime = LuaRuntime::with_defaults();
        let result = runtime
            .execute(
                r#"
                function helper()
                    return 42
                end
                "#,
                serde_json::json!({}),
            )
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("main"));
    }
}
