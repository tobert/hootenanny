//! MCP Handler implementation for Luanette.
//!
//! Implements the baton::Handler trait to expose Lua scripting tools.

use async_trait::async_trait;
use baton::{
    CallToolResult, Content, ErrorData, Handler, Implementation, ServerCapabilities, Tool,
    ToolSchema,
};
use serde_json::Value;
use std::sync::Arc;

use crate::clients::ClientManager;
use crate::runtime::LuaRuntime;
use crate::schema::{LuaDescribeRequest, LuaEvalRequest, LuaEvalResponse};

/// Generate a ToolSchema from a type that implements schemars::JsonSchema.
fn schema_for<T: schemars::JsonSchema>() -> ToolSchema {
    let settings = schemars::generate::SchemaSettings::draft07().with(|s| {
        s.inline_subschemas = true;
    });
    let gen = settings.into_generator();
    let schema = gen.into_root_schema_for::<T>();
    let value = serde_json::to_value(&schema).unwrap_or_default();
    ToolSchema::from_value(value)
}

/// Luanette MCP handler.
pub struct LuanetteHandler {
    runtime: Arc<LuaRuntime>,
    clients: Arc<ClientManager>,
}

impl LuanetteHandler {
    /// Create a new handler with the given Lua runtime and client manager.
    pub fn new(runtime: Arc<LuaRuntime>, clients: Arc<ClientManager>) -> Self {
        Self { runtime, clients }
    }
}

#[async_trait]
impl Handler for LuanetteHandler {
    fn tools(&self) -> Vec<Tool> {
        vec![
            Tool::new("lua_eval", "Evaluate Lua code directly and return the result. For quick scripts and debugging.")
                .with_input_schema(schema_for::<LuaEvalRequest>())
                .with_output_schema(schema_for::<LuaEvalResponse>())
                .with_icon("🌙")
                .with_category("Scripting"),
            Tool::new("lua_describe", "Describe a Lua script's interface by calling its describe() function")
                .with_input_schema(schema_for::<LuaDescribeRequest>())
                .with_icon("📖")
                .with_category("Scripting")
                .read_only(),
            // TODO: lua_execute - requires CAS integration from hootenanny
            // TODO: script_search - requires graph_context from hootenanny
        ]
    }

    async fn call_tool(&self, name: &str, args: Value) -> Result<CallToolResult, ErrorData> {
        match name {
            "lua_eval" => {
                let request: LuaEvalRequest = serde_json::from_value(args)
                    .map_err(|e| ErrorData::invalid_params(e.to_string()))?;

                self.lua_eval(request).await
            }
            "lua_describe" => {
                let request: LuaDescribeRequest = serde_json::from_value(args)
                    .map_err(|e| ErrorData::invalid_params(e.to_string()))?;

                self.lua_describe(request).await
            }
            _ => Err(ErrorData::tool_not_found(name)),
        }
    }

    fn server_info(&self) -> Implementation {
        Implementation::new("luanette", env!("CARGO_PKG_VERSION"))
            .with_title("Luanette - Lua Scripting MCP Server")
    }

    fn instructions(&self) -> Option<String> {
        Some(
            "Luanette is a Lua scripting server for composing MCP tools. \
            Use lua_eval for quick scripts. Scripts must define a main(params) function."
                .to_string(),
        )
    }

    fn capabilities(&self) -> ServerCapabilities {
        ServerCapabilities::default()
            .enable_tools()
            .enable_logging()
    }
}

impl LuanetteHandler {
    /// Evaluate Lua code directly.
    async fn lua_eval(&self, request: LuaEvalRequest) -> Result<CallToolResult, ErrorData> {
        let result = if let Some(ref params) = request.params {
            // If params are provided, execute as a script with main()
            self.runtime
                .execute(&request.code, params.clone())
                .await
        } else {
            // Just evaluate the code directly
            self.runtime.eval(&request.code).await
        };

        match result {
            Ok(exec_result) => {
                let response = LuaEvalResponse {
                    result: exec_result.result.clone(),
                    duration_ms: exec_result.duration.as_millis() as u64,
                };

                let text = format!(
                    "Execution completed in {}ms\nResult: {}",
                    response.duration_ms,
                    serde_json::to_string_pretty(&exec_result.result).unwrap_or_default()
                );

                Ok(CallToolResult::success(vec![Content::text(text)])
                    .with_structured(serde_json::to_value(&response).unwrap()))
            }
            Err(e) => {
                // Format error for AI consumption
                let error_text = format_lua_error(&e);
                Ok(CallToolResult::error(error_text))
            }
        }
    }

    /// Describe a Lua script's interface.
    async fn lua_describe(&self, request: LuaDescribeRequest) -> Result<CallToolResult, ErrorData> {
        // TODO: Fetch script from CAS via hootenanny
        // For now, return a placeholder
        Err(ErrorData::internal_error(format!(
            "lua_describe requires CAS integration. Script hash: {}",
            request.script_hash
        )))
    }
}

/// Format a Lua error for AI consumption.
fn format_lua_error(error: &anyhow::Error) -> String {
    let mut message = String::new();
    message.push_str("Lua Runtime Error\n\n");

    // Extract the error chain
    let mut current: Option<&dyn std::error::Error> = Some(error.as_ref());
    while let Some(err) = current {
        message.push_str(&format!("  {}\n", err));
        current = err.source();
    }

    message.push_str("\nTroubleshooting:\n");
    message.push_str("- Ensure the script defines a main(params) function\n");
    message.push_str("- Check for syntax errors in Lua code\n");
    message.push_str("- Verify parameter types match expected values\n");

    message
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::SandboxConfig;

    fn create_test_handler() -> LuanetteHandler {
        let runtime = Arc::new(LuaRuntime::new(SandboxConfig::default()));
        let clients = Arc::new(ClientManager::new());
        LuanetteHandler::new(runtime, clients)
    }

    #[tokio::test]
    async fn test_lua_eval_simple() {
        let handler = create_test_handler();

        let args = serde_json::json!({
            "code": "return 2 + 2"
        });

        let result = handler.call_tool("lua_eval", args).await.unwrap();
        assert!(!result.is_error);

        // Check structured content
        if let Some(structured) = result.structured_content {
            let response: LuaEvalResponse = serde_json::from_value(structured).unwrap();
            assert_eq!(response.result, 4);
        }
    }

    #[tokio::test]
    async fn test_lua_eval_with_main() {
        let handler = create_test_handler();

        let args = serde_json::json!({
            "code": r#"
                function main(params)
                    return { message = "Hello, " .. params.name }
                end
            "#,
            "params": { "name": "World" }
        });

        let result = handler.call_tool("lua_eval", args).await.unwrap();
        assert!(!result.is_error);
    }

    #[tokio::test]
    async fn test_tool_list() {
        let handler = create_test_handler();
        let tools = handler.tools();

        assert!(tools.iter().any(|t| t.name == "lua_eval"));
        assert!(tools.iter().any(|t| t.name == "lua_describe"));
    }

    #[tokio::test]
    async fn test_unknown_tool() {
        let handler = create_test_handler();

        let result = handler
            .call_tool("nonexistent_tool", serde_json::json!({}))
            .await;

        assert!(result.is_err());
    }
}
