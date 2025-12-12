//! MCP Handler implementation for Luanette.
//!
//! Implements the baton::Handler trait to expose Lua scripting tools.

use async_trait::async_trait;
use baton::{
    CallToolResult, Content, ErrorData, Handler, Implementation, Prompt, PromptMessage, Resource,
    ResourceContents, ResourceTemplate, ServerCapabilities, Tool, ToolContext, ToolSchema,
};
use baton::types::completion::{CompletionResult, CompleteParams, CompletionRef};
use baton::types::prompt::GetPromptResult;
use baton::types::resource::ReadResourceResult;
use std::collections::HashMap;
use baton::types::progress::{ProgressNotification, ProgressToken};
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;

use crate::clients::ClientManager;
use crate::error::format_lua_error;
use crate::job_system::JobStore;
use crate::runtime::LuaRuntime;
use hooteproto::{JobId, JobStatus};
use crate::schema::{
    JobCancelRequest, JobCancelResponse, JobListRequest, JobListResponse, JobPollRequest,
    JobPollResponse, JobStatsRequest, JobStatsResponse, JobStatusRequest, JobStatusResponse,
    LuaDescribeRequest, LuaDescribeResponse, LuaEvalRequest, LuaEvalResponse, LuaExecuteRequest,
    LuaExecuteResponse, ParamDefinition, ScriptInfo, ScriptSearchRequest, ScriptSearchResponse,
    ScriptStoreRequest, ScriptStoreResponse, ToolsRefreshRequest, ToolsRefreshResponse,
    UpstreamRemoveRequest, UpstreamRemoveResponse,
};

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

/// Get a human-readable type name for a JSON value.
fn value_type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "table",
    }
}

/// Luanette MCP handler.
pub struct LuanetteHandler {
    runtime: Arc<LuaRuntime>,
    clients: Arc<ClientManager>,
    jobs: Arc<JobStore>,
}

impl LuanetteHandler {
    /// Create a new handler with the given Lua runtime, client manager, and job store.
    pub fn new(
        runtime: Arc<LuaRuntime>,
        clients: Arc<ClientManager>,
        jobs: Arc<JobStore>,
    ) -> Self {
        Self {
            runtime,
            clients,
            jobs,
        }
    }
}

#[async_trait]
impl Handler for LuanetteHandler {
    fn tools(&self) -> Vec<Tool> {
        vec![
            Tool::new(
                "lua_eval",
                "Evaluate Lua code directly and return the result. For quick scripts and debugging.",
            )
            .with_input_schema(schema_for::<LuaEvalRequest>())
            .with_output_schema(schema_for::<LuaEvalResponse>())
            .with_icon("🌙")
            .with_category("Scripting"),
            Tool::new(
                "job_execute",
                "Execute a Lua script from CAS asynchronously. Returns a job ID for polling.",
            )
            .with_input_schema(schema_for::<LuaExecuteRequest>())
            .with_output_schema(schema_for::<LuaExecuteResponse>())
            .with_icon("🚀")
            .with_category("Jobs"),
            Tool::new(
                "lua_describe",
                "Describe a Lua script's interface by calling its describe() function",
            )
            .with_input_schema(schema_for::<LuaDescribeRequest>())
            .with_output_schema(schema_for::<LuaDescribeResponse>())
            .with_icon("📖")
            .with_category("Scripting")
            .read_only(),
            Tool::new("job_status", "Get the status of a background job")
                .with_input_schema(schema_for::<JobStatusRequest>())
                .with_output_schema(schema_for::<JobStatusResponse>())
                .with_icon("📊")
                .with_category("Jobs")
                .read_only(),
            Tool::new(
                "job_poll",
                "Poll for job completion. Waits until jobs complete or timeout.",
            )
            .with_input_schema(schema_for::<JobPollRequest>())
            .with_output_schema(schema_for::<JobPollResponse>())
            .with_icon("⏳")
            .with_category("Jobs")
            .read_only(),
            Tool::new("job_cancel", "Cancel a running background job")
                .with_input_schema(schema_for::<JobCancelRequest>())
                .with_output_schema(schema_for::<JobCancelResponse>())
                .with_icon("🛑")
                .with_category("Jobs"),
            Tool::new("job_list", "List all background jobs")
                .with_input_schema(schema_for::<JobListRequest>())
                .with_output_schema(schema_for::<JobListResponse>())
                .with_icon("📋")
                .with_category("Jobs")
                .read_only(),
            Tool::new(
                "script_store",
                "Store a Lua script in CAS. Returns hash for use with job_execute.",
            )
            .with_input_schema(schema_for::<ScriptStoreRequest>())
            .with_output_schema(schema_for::<ScriptStoreResponse>())
            .with_icon("💾")
            .with_category("Scripts"),
            Tool::new(
                "script_search",
                "Search for Lua scripts by tag, creator, or vibe.",
            )
            .with_input_schema(schema_for::<ScriptSearchRequest>())
            .with_output_schema(schema_for::<ScriptSearchResponse>())
            .with_icon("🔍")
            .with_category("Scripts")
            .read_only(),
            Tool::new("job_stats", "Get job store statistics")
                .with_input_schema(schema_for::<JobStatsRequest>())
                .with_output_schema(schema_for::<JobStatsResponse>())
                .with_icon("📈")
                .with_category("Jobs")
                .read_only(),
            Tool::new(
                "tools_refresh",
                "Re-discover tools from an upstream namespace.",
            )
            .with_input_schema(schema_for::<ToolsRefreshRequest>())
            .with_output_schema(schema_for::<ToolsRefreshResponse>())
            .with_icon("🔄")
            .with_category("Upstream"),
            Tool::new(
                "upstream_remove",
                "Disconnect an upstream namespace.",
            )
            .with_input_schema(schema_for::<UpstreamRemoveRequest>())
            .with_output_schema(schema_for::<UpstreamRemoveResponse>())
            .with_icon("🔌")
            .with_category("Upstream"),
        ]
    }

    async fn call_tool(&self, name: &str, args: Value) -> Result<CallToolResult, ErrorData> {
        match name {
            "lua_eval" => {
                let request: LuaEvalRequest = serde_json::from_value(args)
                    .map_err(|e| ErrorData::invalid_params(e.to_string()))?;
                self.lua_eval(request).await
            }
            "job_execute" => {
                let request: LuaExecuteRequest = serde_json::from_value(args)
                    .map_err(|e| ErrorData::invalid_params(e.to_string()))?;
                self.job_execute(request).await
            }
            "lua_describe" => {
                let request: LuaDescribeRequest = serde_json::from_value(args)
                    .map_err(|e| ErrorData::invalid_params(e.to_string()))?;
                self.lua_describe(request).await
            }
            "job_status" => {
                let request: JobStatusRequest = serde_json::from_value(args)
                    .map_err(|e| ErrorData::invalid_params(e.to_string()))?;
                self.job_status(request).await
            }
            "job_poll" => {
                let request: JobPollRequest = serde_json::from_value(args)
                    .map_err(|e| ErrorData::invalid_params(e.to_string()))?;
                self.job_poll(request).await
            }
            "job_cancel" => {
                let request: JobCancelRequest = serde_json::from_value(args)
                    .map_err(|e| ErrorData::invalid_params(e.to_string()))?;
                self.job_cancel(request).await
            }
            "job_list" => {
                let request: JobListRequest = serde_json::from_value(args)
                    .map_err(|e| ErrorData::invalid_params(e.to_string()))?;
                self.job_list(request).await
            }
            "script_store" => {
                let request: ScriptStoreRequest = serde_json::from_value(args)
                    .map_err(|e| ErrorData::invalid_params(e.to_string()))?;
                self.script_store(request).await
            }
            "script_search" => {
                let request: ScriptSearchRequest = serde_json::from_value(args)
                    .map_err(|e| ErrorData::invalid_params(e.to_string()))?;
                self.script_search(request).await
            }
            "job_stats" => {
                let _request: JobStatsRequest = serde_json::from_value(args)
                    .map_err(|e| ErrorData::invalid_params(e.to_string()))?;
                self.job_stats().await
            }
            "tools_refresh" => {
                let request: ToolsRefreshRequest = serde_json::from_value(args)
                    .map_err(|e| ErrorData::invalid_params(e.to_string()))?;
                self.tools_refresh(request).await
            }
            "upstream_remove" => {
                let request: UpstreamRemoveRequest = serde_json::from_value(args)
                    .map_err(|e| ErrorData::invalid_params(e.to_string()))?;
                self.upstream_remove(request).await
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
            Use lua_eval for quick scripts, lua_execute for CAS-stored scripts. \
            Scripts must define a main(params) function. \
            Job tools (job_status, job_poll, job_cancel, job_list) manage async executions."
                .to_string(),
        )
    }

    fn capabilities(&self) -> ServerCapabilities {
        ServerCapabilities::default()
            .enable_tools()
            .enable_logging()
            .enable_resources()
            .enable_prompts()
            .enable_completions()
    }

    async fn complete(&self, params: CompleteParams) -> Result<CompletionResult, ErrorData> {
        match &params.reference {
            CompletionRef::Argument { name, argument_name } => {
                self.complete_tool_argument(name, argument_name, &params.argument.value).await
            }
            CompletionRef::Prompt { name } => {
                self.complete_prompt_argument(name, &params.argument.name, &params.argument.value).await
            }
            CompletionRef::Resource { uri } => {
                self.complete_resource_uri(uri, &params.argument.value).await
            }
        }
    }

    fn prompts(&self) -> Vec<Prompt> {
        vec![
            Prompt::new("write-script")
                .with_title("Write Lua Script")
                .with_description("Generate a Lua script for a given task")
                .argument("task", "What the script should accomplish", true)
                .argument("style", "Script style: simple, detailed, or production", false),
            Prompt::new("debug-script")
                .with_title("Debug Script")
                .with_description("Help debug a failing Lua script")
                .argument("hash", "CAS hash of the script to debug", true)
                .argument("error", "Error message if available", false),
            Prompt::new("explain-script")
                .with_title("Explain Script")
                .with_description("Explain what a Lua script does")
                .argument("hash", "CAS hash of the script to explain", true),
            Prompt::new("midi-workflow")
                .with_title("MIDI Workflow")
                .with_description("Generate a MIDI processing workflow script")
                .argument("operation", "Operation: generate, transpose, quantize, continue, bridge", true)
                .argument("params", "Additional parameters as JSON", false),
        ]
    }

    async fn get_prompt(
        &self,
        name: &str,
        arguments: HashMap<String, String>,
    ) -> Result<GetPromptResult, ErrorData> {
        match name {
            "write-script" => self.prompt_write_script(arguments).await,
            "debug-script" => self.prompt_debug_script(arguments).await,
            "explain-script" => self.prompt_explain_script(arguments).await,
            "midi-workflow" => self.prompt_midi_workflow(arguments).await,
            _ => Err(ErrorData::invalid_params(format!("Unknown prompt: {}", name))),
        }
    }

    fn resources(&self) -> Vec<Resource> {
        vec![
            Resource::new("lua://jobs", "jobs")
                .with_description("List of recent Lua script jobs")
                .with_mime_type("application/json"),
            Resource::new("lua://jobs/stats", "job-stats")
                .with_description("Job store statistics")
                .with_mime_type("application/json"),
            Resource::new("lua://tools", "tools")
                .with_description("List of available MCP tools from upstream servers")
                .with_mime_type("application/json"),
            Resource::new("lua://namespaces", "namespaces")
                .with_description("List of connected upstream namespaces")
                .with_mime_type("application/json"),
        ]
    }

    fn resource_templates(&self) -> Vec<ResourceTemplate> {
        vec![
            ResourceTemplate::new("lua://scripts/{hash}", "script-by-hash")
                .with_description("Read a Lua script from CAS by hash")
                .with_mime_type("text/x-lua"),
            ResourceTemplate::new("lua://jobs/{id}", "job-by-id")
                .with_description("Get details and result of a specific job")
                .with_mime_type("application/json"),
            ResourceTemplate::new("lua://tools/{namespace}", "tools-by-namespace")
                .with_description("List tools for a specific upstream namespace")
                .with_mime_type("application/json"),
            ResourceTemplate::new("lua://namespaces/{name}", "namespace-info")
                .with_description("Get info about a specific upstream namespace")
                .with_mime_type("application/json"),
            ResourceTemplate::new("lua://examples/{name}", "example-by-name")
                .with_description("Read a bundled example script")
                .with_mime_type("text/x-lua"),
        ]
    }

    async fn read_resource(&self, uri: &str) -> Result<ReadResourceResult, ErrorData> {
        let (scheme, path) = uri.split_once("://")
            .ok_or_else(|| ErrorData::invalid_params(format!("Invalid URI: {}", uri)))?;

        if scheme != "lua" {
            return Err(ErrorData::invalid_params(format!("Unknown URI scheme: {}", scheme)));
        }

        // Parse path: jobs, jobs/stats, jobs/{id}, scripts/{hash}, tools, tools/{ns}, namespaces, examples/{name}
        let parts: Vec<&str> = path.splitn(2, '/').collect();
        let resource_type = parts[0];
        let resource_id = parts.get(1).copied();

        match (resource_type, resource_id) {
            ("jobs", None) => self.read_jobs_list().await,
            ("jobs", Some("stats")) => self.read_job_stats().await,
            ("jobs", Some(id)) => self.read_job_by_id(id).await,
            ("scripts", Some(hash)) => self.read_script_by_hash(hash).await,
            ("tools", None) => self.read_tools_list().await,
            ("tools", Some(namespace)) => self.read_tools_for_namespace(namespace).await,
            ("namespaces", None) => self.read_namespaces().await,
            ("namespaces", Some(name)) => self.read_namespace_info(name).await,
            ("examples", Some(name)) => self.read_example_by_name(name).await,
            _ => Err(ErrorData::invalid_params(format!("Unknown resource path: {}", path))),
        }
    }

    async fn call_tool_with_context(
        &self,
        name: &str,
        args: Value,
        context: ToolContext,
    ) -> Result<CallToolResult, ErrorData> {
        // Log tool execution to client
        context.log_debug(format!("Executing tool: {}", name)).await;

        if context.has_progress() {
            tracing::debug!(
                tool = name,
                session_id = %context.session_id,
                "Tool called with progress token"
            );
        }

        // Route progress-aware tools to specialized implementations
        match name {
            "job_execute" if context.has_progress() => {
                let request: LuaExecuteRequest = serde_json::from_value(args)
                    .map_err(|e| ErrorData::invalid_params(e.to_string()))?;
                self.job_execute_with_progress(request, context).await
            }
            "lua_eval" if context.has_progress() => {
                let request: LuaEvalRequest = serde_json::from_value(args)
                    .map_err(|e| ErrorData::invalid_params(e.to_string()))?;
                self.lua_eval_with_progress(request, context).await
            }
            // Fall back to standard implementation for non-progress tools
            _ => self.call_tool(name, args).await,
        }
    }
}

impl LuanetteHandler {
    /// Evaluate Lua code directly.
    async fn lua_eval(&self, request: LuaEvalRequest) -> Result<CallToolResult, ErrorData> {
        let result = if let Some(ref params) = request.params {
            self.runtime
                .execute(&request.code, params.clone())
                .await
        } else {
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
                let error_text = format_lua_error(&e);
                Ok(CallToolResult::error(error_text))
            }
        }
    }

    /// Execute a Lua script from CAS asynchronously.
    async fn job_execute(&self, request: LuaExecuteRequest) -> Result<CallToolResult, ErrorData> {
        // Fetch script content from hootenanny CAS
        let script_content = self
            .fetch_script_from_cas(&request.script_hash)
            .await
            .map_err(|e| ErrorData::internal_error(format!("Failed to fetch script: {}", e)))?;

        // Validate params against script's describe() schema
        let params = request.params.clone().unwrap_or(Value::Object(Default::default()));
        if let Err(validation_errors) = self.validate_script_params(&script_content, &params).await {
            return Err(ErrorData::invalid_params(format!(
                "Parameter validation failed:\n{}",
                validation_errors.join("\n")
            )));
        }

        // Create a job
        let job_id = self.jobs.create_job(request.script_hash.clone());

        // Clone what we need for the spawned task
        let runtime = self.runtime.clone();
        let jobs = self.jobs.clone();
        let job_id_clone = job_id.clone();

        // Spawn the execution task
        let handle = tokio::spawn(async move {
            // Mark as running
            if let Err(e) = jobs.mark_running(&job_id_clone) {
                tracing::error!(error = %e, "Failed to mark job as running");
                return;
            }

            // Execute the script
            match runtime.execute(&script_content, params).await {
                Ok(exec_result) => {
                    if let Err(e) = jobs.mark_complete(&job_id_clone, exec_result.result) {
                        tracing::error!(error = %e, "Failed to mark job as complete");
                    }
                }
                Err(e) => {
                    if let Err(mark_err) = jobs.mark_failed(&job_id_clone, e.to_string()) {
                        tracing::error!(error = %mark_err, "Failed to mark job as failed");
                    }
                }
            }
        });

        // Store the handle for potential cancellation
        self.jobs.store_handle(&job_id, handle);

        let response = LuaExecuteResponse {
            job_id: job_id.to_string(),
        };

        let text = format!("Script execution started. Job ID: {}", job_id);

        Ok(CallToolResult::success(vec![Content::text(text)])
            .with_structured(serde_json::to_value(&response).unwrap()))
    }

    /// Execute a Lua script with progress reporting.
    async fn job_execute_with_progress(
        &self,
        request: LuaExecuteRequest,
        context: ToolContext,
    ) -> Result<CallToolResult, ErrorData> {
        let progress_token = context.progress_token.clone()
            .unwrap_or_else(|| ProgressToken::String("progress".to_string()));

        // Send initial progress
        context.send_progress(ProgressNotification::normalized(
            progress_token.clone(),
            0.0,
            "Fetching script from CAS...",
        )).await;

        // Fetch script content from hootenanny CAS
        let script_content = self
            .fetch_script_from_cas(&request.script_hash)
            .await
            .map_err(|e| ErrorData::internal_error(format!("Failed to fetch script: {}", e)))?;

        context.send_progress(ProgressNotification::normalized(
            progress_token.clone(),
            0.1,
            "Script fetched, creating job...",
        )).await;

        // Create a job
        let job_id = self.jobs.create_job(request.script_hash.clone());

        // Clone what we need for the spawned task
        let runtime = self.runtime.clone();
        let jobs = self.jobs.clone();
        let job_id_clone = job_id.clone();
        let params = request.params.clone().unwrap_or(Value::Object(Default::default()));
        let progress_sender = context.progress_sender.clone();
        let progress_token_clone = progress_token.clone();

        // Spawn the execution task with progress
        let handle = tokio::spawn(async move {
            // Send progress update
            if let Some(ref sender) = progress_sender {
                let _ = sender.send(ProgressNotification::normalized(
                    progress_token_clone.clone(),
                    0.2,
                    "Executing script...",
                )).await;
            }

            // Mark as running
            if let Err(e) = jobs.mark_running(&job_id_clone) {
                tracing::error!(error = %e, "Failed to mark job as running");
                return;
            }

            // Execute the script
            match runtime.execute(&script_content, params).await {
                Ok(exec_result) => {
                    // Send completion progress
                    if let Some(ref sender) = progress_sender {
                        let _ = sender.send(ProgressNotification::normalized(
                            progress_token_clone,
                            1.0,
                            "Complete",
                        )).await;
                    }

                    if let Err(e) = jobs.mark_complete(&job_id_clone, exec_result.result) {
                        tracing::error!(error = %e, "Failed to mark job as complete");
                    }
                }
                Err(e) => {
                    // Send error in progress
                    if let Some(ref sender) = progress_sender {
                        let _ = sender.send(ProgressNotification::normalized(
                            progress_token_clone,
                            1.0,
                            format!("Failed: {}", e),
                        )).await;
                    }

                    if let Err(mark_err) = jobs.mark_failed(&job_id_clone, e.to_string()) {
                        tracing::error!(error = %mark_err, "Failed to mark job as failed");
                    }
                }
            }
        });

        // Store the handle for potential cancellation
        self.jobs.store_handle(&job_id, handle);

        let response = LuaExecuteResponse {
            job_id: job_id.to_string(),
        };

        let text = format!("Script execution started with progress tracking. Job ID: {}", job_id);

        Ok(CallToolResult::success(vec![Content::text(text)])
            .with_structured(serde_json::to_value(&response).unwrap()))
    }

    /// Evaluate Lua code with progress reporting.
    async fn lua_eval_with_progress(
        &self,
        request: LuaEvalRequest,
        context: ToolContext,
    ) -> Result<CallToolResult, ErrorData> {
        let progress_token = context.progress_token.clone()
            .unwrap_or_else(|| ProgressToken::String("progress".to_string()));

        // Send initial progress
        context.send_progress(ProgressNotification::normalized(
            progress_token.clone(),
            0.0,
            "Parsing Lua code...",
        )).await;

        context.send_progress(ProgressNotification::normalized(
            progress_token.clone(),
            0.2,
            "Executing...",
        )).await;

        let result = if let Some(ref params) = request.params {
            self.runtime
                .execute(&request.code, params.clone())
                .await
        } else {
            self.runtime.eval(&request.code).await
        };

        context.send_progress(ProgressNotification::normalized(
            progress_token.clone(),
            1.0,
            "Complete",
        )).await;

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
                let error_text = format_lua_error(&e);
                Ok(CallToolResult::error(error_text))
            }
        }
    }

    /// Fetch script content from hootenanny CAS.
    async fn fetch_script_from_cas(&self, hash: &str) -> anyhow::Result<String> {
        // Call hootenanny's cas_inspect to get the content
        let args = serde_json::json!({ "hash": hash });
        let result = self.clients.call_tool("hootenanny", "cas_inspect", args).await?;

        // Extract content from result
        // cas_inspect returns { hash, mime_type, size, content_base64?, local_path? }
        let content_base64 = result
            .get("content_base64")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("CAS inspect did not return content_base64"))?;

        // Decode base64
        use base64::Engine;
        let bytes = base64::engine::general_purpose::STANDARD.decode(content_base64)?;
        let content = String::from_utf8(bytes)?;

        Ok(content)
    }

    /// Describe a Lua script's interface.
    async fn lua_describe(&self, request: LuaDescribeRequest) -> Result<CallToolResult, ErrorData> {
        // Fetch script from CAS
        let script_content = self
            .fetch_script_from_cas(&request.script_hash)
            .await
            .map_err(|e| ErrorData::internal_error(format!("Failed to fetch script: {}", e)))?;

        // Execute the script to get the describe() function result
        let describe_code = format!(
            r#"
            {}

            if type(describe) == "function" then
                return describe()
            else
                return nil
            end
            "#,
            script_content
        );

        match self.runtime.eval(&describe_code).await {
            Ok(exec_result) => {
                if exec_result.result.is_null() {
                    let response = LuaDescribeResponse {
                        name: None,
                        description: Some("Script does not define a describe() function".to_string()),
                        params: None,
                        returns: None,
                    };
                    let text = "Script does not define a describe() function".to_string();
                    return Ok(CallToolResult::success(vec![Content::text(text)])
                        .with_structured(serde_json::to_value(&response).unwrap()));
                }

                // Parse the Lua result into LuaDescribeResponse
                let response = LuaDescribeResponse {
                    name: exec_result.result.get("name").and_then(|v| v.as_str()).map(String::from),
                    description: exec_result.result.get("description").and_then(|v| v.as_str()).map(String::from),
                    params: exec_result.result.get("params").cloned(),
                    returns: exec_result.result.get("returns").and_then(|v| v.as_str()).map(String::from),
                };

                let text = format!(
                    "Script: {}\nDescription: {}\nReturns: {}",
                    response.name.as_deref().unwrap_or("(unnamed)"),
                    response.description.as_deref().unwrap_or("(no description)"),
                    response.returns.as_deref().unwrap_or("(unspecified)")
                );

                Ok(CallToolResult::success(vec![Content::text(text)])
                    .with_structured(serde_json::to_value(&response).unwrap()))
            }
            Err(e) => {
                let error_text = format_lua_error(&e);
                Ok(CallToolResult::error(error_text))
            }
        }
    }

    /// Get the status of a job.
    async fn job_status(&self, request: JobStatusRequest) -> Result<CallToolResult, ErrorData> {
        let job_id = JobId::from(request.job_id);

        match self.jobs.get_job(&job_id) {
            Ok(info) => {
                let response: JobStatusResponse = info.into();
                let text = format!(
                    "Job {}: {} (script: {})",
                    response.job_id, response.status, response.script_hash
                );

                Ok(CallToolResult::success(vec![Content::text(text)])
                    .with_structured(serde_json::to_value(&response).unwrap()))
            }
            Err(e) => Err(ErrorData::internal_error(e.to_string())),
        }
    }

    /// Poll for job completion.
    async fn job_poll(&self, request: JobPollRequest) -> Result<CallToolResult, ErrorData> {
        // Cap timeout at 30 seconds
        let timeout_ms = request.timeout_ms.min(30000);
        let poll_interval = Duration::from_millis(100);
        let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms);

        let job_ids: Vec<JobId> = request
            .job_ids
            .iter()
            .map(|s| JobId::from(s.clone()))
            .collect();

        let mode_all = request.mode == "all";

        loop {
            let mut completed = Vec::new();
            let mut pending = Vec::new();

            for job_id in &job_ids {
                if let Ok(info) = self.jobs.get_job(job_id) {
                    match info.status {
                        JobStatus::Complete | JobStatus::Failed | JobStatus::Cancelled => {
                            completed.push(JobStatusResponse::from(info));
                        }
                        _ => {
                            pending.push(job_id.to_string());
                        }
                    }
                } else {
                    // Job not found, treat as pending (might be race condition)
                    pending.push(job_id.to_string());
                }
            }

            // Check completion conditions
            let should_return = if mode_all {
                pending.is_empty()
            } else {
                !completed.is_empty() || pending.is_empty()
            };

            if should_return || tokio::time::Instant::now() >= deadline {
                let timed_out = !pending.is_empty() && tokio::time::Instant::now() >= deadline;
                let response = JobPollResponse {
                    completed,
                    pending,
                    timed_out,
                };

                let text = if response.timed_out {
                    format!(
                        "Poll timed out. {} completed, {} still pending.",
                        response.completed.len(),
                        response.pending.len()
                    )
                } else {
                    format!(
                        "Poll complete. {} jobs finished.",
                        response.completed.len()
                    )
                };

                return Ok(CallToolResult::success(vec![Content::text(text)])
                    .with_structured(serde_json::to_value(&response).unwrap()));
            }

            tokio::time::sleep(poll_interval).await;
        }
    }

    /// Cancel a running job.
    async fn job_cancel(&self, request: JobCancelRequest) -> Result<CallToolResult, ErrorData> {
        let job_id = JobId::from(request.job_id.clone());

        match self.jobs.cancel_job(&job_id) {
            Ok(()) => {
                let response = JobCancelResponse {
                    job_id: request.job_id,
                    success: true,
                };

                let text = format!("Job {} cancelled", response.job_id);

                Ok(CallToolResult::success(vec![Content::text(text)])
                    .with_structured(serde_json::to_value(&response).unwrap()))
            }
            Err(e) => {
                let response = JobCancelResponse {
                    job_id: request.job_id,
                    success: false,
                };

                let text = format!("Failed to cancel job: {}", e);

                Ok(CallToolResult::error(text)
                    .with_structured(serde_json::to_value(&response).unwrap()))
            }
        }
    }

    /// List all jobs.
    async fn job_list(&self, request: JobListRequest) -> Result<CallToolResult, ErrorData> {
        let all_jobs = self.jobs.list_jobs();

        // Filter by status if specified
        let filtered: Vec<_> = if let Some(ref status_filter) = request.status {
            all_jobs
                .into_iter()
                .filter(|job| {
                    let status_str = match job.status {
                        JobStatus::Pending => "pending",
                        JobStatus::Running => "running",
                        JobStatus::Complete => "complete",
                        JobStatus::Failed => "failed",
                        JobStatus::Cancelled => "cancelled",
                    };
                    status_str == status_filter
                })
                .collect()
        } else {
            all_jobs
        };

        let total = filtered.len();
        let jobs: Vec<JobStatusResponse> = filtered
            .into_iter()
            .take(request.limit)
            .map(JobStatusResponse::from)
            .collect();

        let response = JobListResponse {
            jobs: jobs.clone(),
            total,
        };

        let text = format!("Found {} jobs (showing {})", total, jobs.len());

        Ok(CallToolResult::success(vec![Content::text(text)])
            .with_structured(serde_json::to_value(&response).unwrap()))
    }

    /// Store a Lua script in CAS.
    async fn script_store(&self, request: ScriptStoreRequest) -> Result<CallToolResult, ErrorData> {
        // Encode content as base64
        use base64::Engine;
        let content_base64 =
            base64::engine::general_purpose::STANDARD.encode(request.content.as_bytes());

        // Build tags - always include type:lua
        let mut tags = request.tags.clone();
        if !tags.iter().any(|t| t.starts_with("type:")) {
            tags.push("type:lua".to_string());
        }

        // Call hootenanny's cas_store
        let args = serde_json::json!({
            "content_base64": content_base64,
            "mime_type": "text/x-lua"
        });

        let result = self
            .clients
            .call_tool("hootenanny", "cas_store", args)
            .await
            .map_err(|e| ErrorData::internal_error(format!("Failed to store script: {}", e)))?;

        let hash = result
            .get("hash")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ErrorData::internal_error("cas_store did not return hash"))?
            .to_string();

        // If tags or creator specified, create an artifact for discoverability
        let artifact_id = if !tags.is_empty() || request.creator.is_some() {
            let artifact_args = serde_json::json!({
                "file_path": format!("/cas/{}", hash),  // Virtual path
                "mime_type": "text/x-lua",
                "tags": tags,
                "creator": request.creator,
            });

            // Try to create artifact, but don't fail if it doesn't work
            match self
                .clients
                .call_tool("hootenanny", "artifact_upload", artifact_args)
                .await
            {
                Ok(artifact_result) => artifact_result
                    .get("artifact_id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                Err(_) => None,
            }
        } else {
            None
        };

        let response = ScriptStoreResponse {
            hash: hash.clone(),
            artifact_id: artifact_id.clone(),
        };

        let text = match &artifact_id {
            Some(id) => format!("Script stored. Hash: {}, Artifact: {}", hash, id),
            None => format!("Script stored. Hash: {}", hash),
        };

        Ok(CallToolResult::success(vec![Content::text(text)])
            .with_structured(serde_json::to_value(&response).unwrap()))
    }

    /// Search for Lua scripts.
    async fn script_search(
        &self,
        request: ScriptSearchRequest,
    ) -> Result<CallToolResult, ErrorData> {
        // Build graph_context query
        // Always filter by type:lua, plus any additional filters
        let mut tag_filter = Some("type:lua".to_string());
        if let Some(ref extra_tag) = request.tag {
            tag_filter = Some(format!("type:lua,{}", extra_tag));
        }

        let args = serde_json::json!({
            "tag": tag_filter,
            "creator": request.creator,
            "vibe_search": request.vibe,
            "limit": request.limit,
            "include_annotations": true,
            "include_metadata": false,
        });

        let result = self
            .clients
            .call_tool("hootenanny", "graph_context", args)
            .await
            .map_err(|e| ErrorData::internal_error(format!("Failed to search scripts: {}", e)))?;

        // Parse the artifacts from the result
        let artifacts = result
            .get("artifacts")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        let scripts: Vec<ScriptInfo> = artifacts
            .iter()
            .filter_map(|artifact| {
                let hash = artifact.get("hash").and_then(|v| v.as_str())?;
                let artifact_id = artifact.get("id").and_then(|v| v.as_str());
                let tags: Vec<String> = artifact
                    .get("tags")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|t| t.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                let creator = artifact
                    .get("creator")
                    .and_then(|v| v.as_str())
                    .map(String::from);

                // Try to extract name/description from annotations if available
                let annotations = artifact.get("annotations").and_then(|v| v.as_array());
                let (name, description) = if let Some(anns) = annotations {
                    let name = anns.iter().find_map(|a| {
                        a.get("vibe")
                            .and_then(|v| v.as_str())
                            .filter(|s| s.contains("name:"))
                            .map(|s| s.replace("name:", "").trim().to_string())
                    });
                    let desc = anns.iter().find_map(|a| {
                        a.get("message").and_then(|v| v.as_str()).map(String::from)
                    });
                    (name, desc)
                } else {
                    (None, None)
                };

                Some(ScriptInfo {
                    hash: hash.to_string(),
                    artifact_id: artifact_id.map(String::from),
                    tags,
                    creator,
                    name,
                    description,
                })
            })
            .collect();

        let total = scripts.len();
        let response = ScriptSearchResponse {
            scripts: scripts.clone(),
            total,
        };

        let text = format!("Found {} Lua scripts", total);

        Ok(CallToolResult::success(vec![Content::text(text)])
            .with_structured(serde_json::to_value(&response).unwrap()))
    }

    // === Resource Helper Methods ===

    /// Read list of all jobs.
    async fn read_jobs_list(&self) -> Result<ReadResourceResult, ErrorData> {
        let jobs = self.jobs.list_jobs();
        let job_summaries: Vec<serde_json::Value> = jobs
            .iter()
            .map(|job| {
                serde_json::json!({
                    "job_id": job.job_id.to_string(),
                    "status": format!("{:?}", job.status),
                    "script_hash": job.source,
                    "created_at_unix": job.created_at,
                })
            })
            .collect();

        let content = serde_json::to_string_pretty(&job_summaries)
            .map_err(|e| ErrorData::internal_error(e.to_string()))?;

        Ok(ReadResourceResult::single(
            ResourceContents::text_with_mime("lua://jobs", content, "application/json"),
        ))
    }

    /// Read a specific job by ID.
    async fn read_job_by_id(&self, id: &str) -> Result<ReadResourceResult, ErrorData> {
        let job_id = JobId::from(id.to_string());
        let job = self.jobs.get_job(&job_id)
            .map_err(|e| ErrorData::invalid_params(e.to_string()))?;

        let job_detail = serde_json::json!({
            "job_id": job.job_id.to_string(),
            "status": format!("{:?}", job.status),
            "script_hash": job.source,
            "created_at_unix": job.created_at,
            "started_at_unix": job.started_at,
            "completed_at_unix": job.completed_at,
            "result": job.result,
            "error": job.error,
        });

        let content = serde_json::to_string_pretty(&job_detail)
            .map_err(|e| ErrorData::internal_error(e.to_string()))?;

        Ok(ReadResourceResult::single(
            ResourceContents::text_with_mime(
                format!("lua://jobs/{}", id),
                content,
                "application/json",
            ),
        ))
    }

    /// Read a script from CAS by hash.
    async fn read_script_by_hash(&self, hash: &str) -> Result<ReadResourceResult, ErrorData> {
        let content = self.fetch_script_from_cas(hash)
            .await
            .map_err(|e| ErrorData::internal_error(format!("Failed to fetch script: {}", e)))?;

        Ok(ReadResourceResult::single(
            ResourceContents::text_with_mime(
                format!("lua://scripts/{}", hash),
                content,
                "text/x-lua",
            ),
        ))
    }

    /// Read list of available MCP tools.
    async fn read_tools_list(&self) -> Result<ReadResourceResult, ErrorData> {
        let tools = self.clients.all_tools().await;
        let tool_summaries: Vec<serde_json::Value> = tools
            .iter()
            .map(|(namespace, tool)| {
                serde_json::json!({
                    "namespace": namespace,
                    "name": tool.name,
                    "description": tool.description,
                    "lua_path": format!("mcp.{}.{}", namespace, tool.name),
                })
            })
            .collect();

        let content = serde_json::to_string_pretty(&tool_summaries)
            .map_err(|e| ErrorData::internal_error(e.to_string()))?;

        Ok(ReadResourceResult::single(
            ResourceContents::text_with_mime("lua://tools", content, "application/json"),
        ))
    }

    /// Read a bundled example script.
    async fn read_example_by_name(&self, name: &str) -> Result<ReadResourceResult, ErrorData> {
        // Map example names to their content
        let content = match name {
            "hello" => include_str!("../examples/hello.lua"),
            "tool_call" => include_str!("../examples/tool_call.lua"),
            "otel_example" => include_str!("../examples/otel_example.lua"),
            "midi_process" => include_str!("../examples/midi_process.lua"),
            "multi_variation" => include_str!("../examples/multi_variation.lua"),
            _ => return Err(ErrorData::invalid_params(format!("Unknown example: {}", name))),
        };

        Ok(ReadResourceResult::single(
            ResourceContents::text_with_mime(
                format!("lua://examples/{}", name),
                content,
                "text/x-lua",
            ),
        ))
    }

    /// Read job store statistics.
    async fn read_job_stats(&self) -> Result<ReadResourceResult, ErrorData> {
        let stats = self.jobs.stats();
        let content = serde_json::to_string_pretty(&serde_json::json!({
            "total": stats.total,
            "pending": stats.pending,
            "running": stats.running,
            "completed": stats.completed,
            "failed": stats.failed,
            "cancelled": stats.cancelled,
        }))
        .map_err(|e| ErrorData::internal_error(e.to_string()))?;

        Ok(ReadResourceResult::single(
            ResourceContents::text_with_mime("lua://jobs/stats", content, "application/json"),
        ))
    }

    /// Read list of connected namespaces.
    async fn read_namespaces(&self) -> Result<ReadResourceResult, ErrorData> {
        let namespaces = self.clients.namespaces().await;
        let content = serde_json::to_string_pretty(&namespaces)
            .map_err(|e| ErrorData::internal_error(e.to_string()))?;

        Ok(ReadResourceResult::single(
            ResourceContents::text_with_mime("lua://namespaces", content, "application/json"),
        ))
    }

    /// Read info about a specific namespace.
    async fn read_namespace_info(&self, name: &str) -> Result<ReadResourceResult, ErrorData> {
        if !self.clients.has_namespace(name).await {
            return Err(ErrorData::invalid_params(format!(
                "Unknown namespace '{}'. Available: {:?}",
                name,
                self.clients.namespaces().await
            )));
        }

        let url = self.clients.url_for_namespace(name).await;
        let tools = self.clients.tools_for_namespace(name).await.unwrap_or_default();

        let content = serde_json::to_string_pretty(&serde_json::json!({
            "namespace": name,
            "url": url,
            "tool_count": tools.len(),
            "tools": tools.iter().map(|t| &t.name).collect::<Vec<_>>(),
        }))
        .map_err(|e| ErrorData::internal_error(e.to_string()))?;

        Ok(ReadResourceResult::single(
            ResourceContents::text_with_mime(
                format!("lua://namespaces/{}", name),
                content,
                "application/json",
            ),
        ))
    }

    /// Read tools for a specific namespace.
    async fn read_tools_for_namespace(&self, namespace: &str) -> Result<ReadResourceResult, ErrorData> {
        let tools = self.clients.tools_for_namespace(namespace).await
            .ok_or_else(|| ErrorData::invalid_params(format!("Unknown namespace: {}", namespace)))?;

        let tool_summaries: Vec<serde_json::Value> = tools
            .iter()
            .map(|tool| {
                serde_json::json!({
                    "name": tool.name,
                    "description": tool.description,
                    "lua_path": format!("mcp.{}.{}", namespace, tool.name),
                })
            })
            .collect();

        let content = serde_json::to_string_pretty(&tool_summaries)
            .map_err(|e| ErrorData::internal_error(e.to_string()))?;

        Ok(ReadResourceResult::single(
            ResourceContents::text_with_mime(
                format!("lua://tools/{}", namespace),
                content,
                "application/json",
            ),
        ))
    }

    // === Validation Helpers ===

    /// Validate script params against the script's describe() schema.
    ///
    /// Returns Ok(()) if validation passes or script has no describe().
    /// Returns Err(Vec<String>) with validation error messages.
    async fn validate_script_params(
        &self,
        script_content: &str,
        params: &Value,
    ) -> Result<(), Vec<String>> {
        // Run describe() to get the schema
        let describe_code = format!(
            r#"
            {}

            if type(describe) == "function" then
                return describe()
            else
                return nil
            end
            "#,
            script_content
        );

        let describe_result = match self.runtime.eval(&describe_code).await {
            Ok(result) => result.result,
            Err(_) => return Ok(()), // If describe() fails, skip validation
        };

        if describe_result.is_null() {
            return Ok(()); // No describe() function, skip validation
        }

        // Extract params schema from describe result
        let param_schema = match describe_result.get("params") {
            Some(Value::Object(obj)) => obj,
            _ => return Ok(()), // No params defined, skip validation
        };

        let provided_params = match params {
            Value::Object(obj) => obj,
            _ => return Ok(()), // Params not an object, skip validation
        };

        let mut errors = Vec::new();

        // Validate each defined parameter
        for (param_name, param_def_value) in param_schema {
            let param_def: ParamDefinition = match serde_json::from_value(param_def_value.clone()) {
                Ok(def) => def,
                Err(_) => continue, // Skip malformed param definitions
            };

            let provided_value = provided_params.get(param_name);

            // Check required params
            if param_def.required && provided_value.is_none() {
                errors.push(format!(
                    "- Missing required parameter '{}' ({})",
                    param_name,
                    param_def.description.as_deref().unwrap_or(&param_def.param_type)
                ));
                continue;
            }

            // Type check if value provided
            if let Some(value) = provided_value {
                let type_ok = match param_def.param_type.as_str() {
                    "string" => value.is_string(),
                    "number" => value.is_number(),
                    "boolean" => value.is_boolean(),
                    "table" => value.is_object() || value.is_array(),
                    _ => true, // Unknown type, allow
                };

                if !type_ok {
                    errors.push(format!(
                        "- Parameter '{}' should be {} but got {}",
                        param_name,
                        param_def.param_type,
                        value_type_name(value)
                    ));
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    // === Tool Helper Methods (new) ===

    /// Get job store statistics.
    async fn job_stats(&self) -> Result<CallToolResult, ErrorData> {
        let stats = self.jobs.stats();
        let response = JobStatsResponse {
            total: stats.total,
            pending: stats.pending,
            running: stats.running,
            completed: stats.completed,
            failed: stats.failed,
            cancelled: stats.cancelled,
        };

        let text = format!(
            "Job stats: {} total ({} pending, {} running, {} completed, {} failed, {} cancelled)",
            stats.total, stats.pending, stats.running, stats.completed, stats.failed, stats.cancelled
        );

        Ok(CallToolResult::success(vec![Content::text(text)])
            .with_structured(serde_json::to_value(&response).unwrap()))
    }

    /// Refresh tools from an upstream namespace.
    async fn tools_refresh(&self, request: ToolsRefreshRequest) -> Result<CallToolResult, ErrorData> {
        // Validate namespace exists before attempting refresh
        if !self.clients.has_namespace(&request.namespace).await {
            return Err(ErrorData::invalid_params(format!(
                "Unknown namespace '{}'. Available: {:?}",
                request.namespace,
                self.clients.namespaces().await
            )));
        }

        self.clients.refresh_tools(&request.namespace).await
            .map_err(|e| ErrorData::internal_error(format!("Failed to refresh tools: {}", e)))?;

        let tools = self.clients.tools_for_namespace(&request.namespace).await
            .ok_or_else(|| ErrorData::internal_error("Namespace not found after refresh"))?;

        let tool_names: Vec<String> = tools.iter().map(|t| t.name.clone()).collect();
        let response = ToolsRefreshResponse {
            namespace: request.namespace.clone(),
            tool_count: tools.len(),
            tools: tool_names,
        };

        let text = format!("Refreshed {} tools from namespace '{}'", tools.len(), request.namespace);

        Ok(CallToolResult::success(vec![Content::text(text)])
            .with_structured(serde_json::to_value(&response).unwrap()))
    }

    /// Remove an upstream namespace.
    async fn upstream_remove(&self, request: UpstreamRemoveRequest) -> Result<CallToolResult, ErrorData> {
        let removed = self.clients.remove_upstream(&request.namespace).await;

        let response = UpstreamRemoveResponse {
            namespace: request.namespace.clone(),
            removed,
        };

        let text = if removed {
            format!("Removed upstream namespace '{}'", request.namespace)
        } else {
            format!("Namespace '{}' was not connected", request.namespace)
        };

        Ok(CallToolResult::success(vec![Content::text(text)])
            .with_structured(serde_json::to_value(&response).unwrap()))
    }

    // === Prompt Helper Methods ===

    /// Generate a prompt for writing a Lua script.
    async fn prompt_write_script(
        &self,
        args: HashMap<String, String>,
    ) -> Result<GetPromptResult, ErrorData> {
        let task = args.get("task")
            .ok_or_else(|| ErrorData::invalid_params("Missing required argument: task"))?;
        let style = args.get("style").map(|s| s.as_str()).unwrap_or("simple");

        // Get available tools to include in context
        let tools = self.clients.all_tools().await;
        let tools_summary: String = tools
            .iter()
            .map(|(ns, tool)| format!("- mcp.{}.{}: {}", ns, tool.name, tool.description.as_deref().unwrap_or("")))
            .collect::<Vec<_>>()
            .join("\n");

        let style_guidance = match style {
            "detailed" => "Include comprehensive error handling and logging. Add describe() function with full parameter documentation.",
            "production" => "Use robust error handling, input validation, progress reporting if applicable, and comprehensive logging. Structure code for maintainability.",
            _ => "Keep it simple and focused on the task. Include basic error handling.",
        };

        let prompt = format!(
            r#"Write a Lua script that: {}

## Style: {}
{}

## Available MCP Tools
These tools can be called via mcp.<namespace>.<tool>({{ args }}):

{}

## Script Structure
Scripts should follow this pattern:
```lua
function describe()
    return {{
        name = "script-name",
        description = "What it does",
        params = {{
            param_name = {{ type = "string", required = true, description = "..." }}
        }},
        returns = "What it returns"
    }}
end

function main(params)
    -- Implementation here
    log.info("Processing...")

    -- Call MCP tools like:
    -- local result = mcp.hootenanny.orpheus_generate({{ temperature = 1.0 }})

    return {{ success = true, data = result }}
end
```

## Standard Library
- `log.*`: log.info(), log.debug(), log.warn(), log.error()
- `otel.*`: otel.trace_id(), otel.event(), otel.set_attribute()
- `midi.*`: midi.read(), midi.write(), midi.transpose()
- `temp.*`: temp.path(), temp.dir()

Generate the complete script:"#,
            task, style, style_guidance, tools_summary
        );

        Ok(GetPromptResult::new(vec![PromptMessage::user_text(prompt)])
            .with_description(format!("Write Lua script: {}", task)))
    }

    /// Generate a prompt for debugging a script.
    async fn prompt_debug_script(
        &self,
        args: HashMap<String, String>,
    ) -> Result<GetPromptResult, ErrorData> {
        let hash = args.get("hash")
            .ok_or_else(|| ErrorData::invalid_params("Missing required argument: hash"))?;
        let error = args.get("error");

        // Fetch the script content
        let script_content = self.fetch_script_from_cas(hash)
            .await
            .map_err(|e| ErrorData::internal_error(format!("Failed to fetch script: {}", e)))?;

        let error_context = match error {
            Some(e) => format!("\n## Error Message\n```\n{}\n```\n", e),
            None => String::new(),
        };

        let prompt = format!(
            r#"Debug this Lua script that's failing:

## Script (hash: {})
```lua
{}
```
{}
## Task
1. Identify the likely cause of the error
2. Explain the issue clearly
3. Provide corrected code
4. Suggest any improvements

Please analyze the script and provide fixes:"#,
            hash, script_content, error_context
        );

        Ok(GetPromptResult::new(vec![PromptMessage::user_text(prompt)])
            .with_description(format!("Debug script {}", hash)))
    }

    /// Generate a prompt for explaining a script.
    async fn prompt_explain_script(
        &self,
        args: HashMap<String, String>,
    ) -> Result<GetPromptResult, ErrorData> {
        let hash = args.get("hash")
            .ok_or_else(|| ErrorData::invalid_params("Missing required argument: hash"))?;

        // Fetch the script content
        let script_content = self.fetch_script_from_cas(hash)
            .await
            .map_err(|e| ErrorData::internal_error(format!("Failed to fetch script: {}", e)))?;

        let prompt = format!(
            r#"Explain this Lua script:

## Script (hash: {})
```lua
{}
```

## Task
Provide a clear explanation covering:
1. **Purpose**: What does this script do overall?
2. **Inputs**: What parameters does it accept?
3. **Process**: Walk through the logic step-by-step
4. **Outputs**: What does it return?
5. **External Calls**: What MCP tools or external resources does it use?

Please explain the script:"#,
            hash, script_content
        );

        Ok(GetPromptResult::new(vec![PromptMessage::user_text(prompt)])
            .with_description(format!("Explain script {}", hash)))
    }

    /// Generate a prompt for MIDI workflow.
    async fn prompt_midi_workflow(
        &self,
        args: HashMap<String, String>,
    ) -> Result<GetPromptResult, ErrorData> {
        let operation = args.get("operation")
            .ok_or_else(|| ErrorData::invalid_params("Missing required argument: operation"))?;
        let params = args.get("params");

        let workflow_context = match operation.as_str() {
            "generate" => "Generate new MIDI using Orpheus. Use orpheus_generate with temperature and max_tokens.",
            "transpose" => "Transpose MIDI by semitones. Read MIDI, apply midi.transpose(), save back.",
            "quantize" => "Quantize MIDI to a grid. Read MIDI, apply midi.quantize(events, grid_ticks), save.",
            "continue" => "Continue existing MIDI. Use orpheus_continue with the input hash.",
            "bridge" => "Generate a transition between two MIDI sections. Use orpheus_bridge with section hashes.",
            _ => return Err(ErrorData::invalid_params(format!("Unknown operation: {}", operation))),
        };

        let params_note = match params {
            Some(p) => format!("\n## Additional Parameters\n```json\n{}\n```\n", p),
            None => String::new(),
        };

        let prompt = format!(
            r#"Create a Lua script for MIDI {operation}:

## Operation Context
{workflow_context}
{params_note}
## Available Orpheus Tools
- mcp.hootenanny.orpheus_generate: Generate MIDI from scratch
- mcp.hootenanny.orpheus_continue: Continue existing MIDI
- mcp.hootenanny.orpheus_bridge: Create transition between sections
- mcp.hootenanny.orpheus_loops: Generate drum loops
- mcp.hootenanny.convert_midi_to_wav: Render to audio

## Pattern
```lua
function main(params)
    -- 1. Set up parameters
    -- 2. Call Orpheus tool
    -- 3. Process/store result
    -- 4. Return artifact info
end
```

Generate the workflow script:"#,
            operation = operation,
            workflow_context = workflow_context,
            params_note = params_note
        );

        Ok(GetPromptResult::new(vec![PromptMessage::user_text(prompt)])
            .with_description(format!("MIDI {} workflow", operation)))
    }

    // === Completion Helper Methods ===

    /// Complete tool arguments.
    async fn complete_tool_argument(
        &self,
        tool: &str,
        arg: &str,
        partial: &str,
    ) -> Result<CompletionResult, ErrorData> {
        let values: Vec<String> = match (tool, arg) {
            // Script hash completion for script-related tools
            ("job_execute", "script_hash") |
            ("lua_describe", "script_hash") => {
                self.get_recent_script_hashes(10).await
            }

            // Job ID completion
            ("job_status", "job_id") |
            ("job_cancel", "job_id") => {
                self.get_active_job_ids()
            }

            // Job IDs for poll (multiple)
            ("job_poll", "job_ids") => {
                self.get_active_job_ids()
            }

            // Status filter for job_list
            ("job_list", "status") => {
                vec![
                    "pending".to_string(),
                    "running".to_string(),
                    "complete".to_string(),
                    "failed".to_string(),
                    "cancelled".to_string(),
                ]
            }

            // Tag completion for script_search
            ("script_search", "tag") => {
                vec![
                    "type:lua".to_string(),
                    "phase:generation".to_string(),
                    "phase:processing".to_string(),
                    "source:user".to_string(),
                    "source:agent".to_string(),
                ]
            }

            // Style for lua_eval params
            ("lua_eval", "style") => {
                vec![
                    "simple".to_string(),
                    "detailed".to_string(),
                    "production".to_string(),
                ]
            }

            _ => return Ok(CompletionResult::empty()),
        };

        // Filter by partial (case-insensitive prefix)
        let filtered: Vec<String> = values
            .into_iter()
            .filter(|v| v.to_lowercase().starts_with(&partial.to_lowercase()))
            .collect();

        Ok(CompletionResult::new(filtered))
    }

    /// Complete prompt arguments.
    async fn complete_prompt_argument(
        &self,
        prompt: &str,
        arg: &str,
        partial: &str,
    ) -> Result<CompletionResult, ErrorData> {
        let values: Vec<String> = match (prompt, arg) {
            // Hash completion for debug/explain prompts
            ("debug-script", "hash") |
            ("explain-script", "hash") => {
                self.get_recent_script_hashes(10).await
            }

            // Style for write-script
            ("write-script", "style") => {
                vec![
                    "simple".to_string(),
                    "detailed".to_string(),
                    "production".to_string(),
                ]
            }

            // Operations for midi-workflow
            ("midi-workflow", "operation") => {
                vec![
                    "generate".to_string(),
                    "transpose".to_string(),
                    "quantize".to_string(),
                    "continue".to_string(),
                    "bridge".to_string(),
                ]
            }

            _ => return Ok(CompletionResult::empty()),
        };

        let filtered: Vec<String> = values
            .into_iter()
            .filter(|v| v.to_lowercase().starts_with(&partial.to_lowercase()))
            .collect();

        Ok(CompletionResult::new(filtered))
    }

    /// Complete resource URIs.
    async fn complete_resource_uri(
        &self,
        _uri: &str,
        partial: &str,
    ) -> Result<CompletionResult, ErrorData> {
        // Suggest example names if they're typing lua://examples/
        let examples = vec![
            "hello".to_string(),
            "tool_call".to_string(),
            "otel_example".to_string(),
            "midi_process".to_string(),
            "multi_variation".to_string(),
        ];

        let filtered: Vec<String> = examples
            .into_iter()
            .filter(|v| v.to_lowercase().starts_with(&partial.to_lowercase()))
            .collect();

        Ok(CompletionResult::new(filtered))
    }

    /// Get recent script hashes for completion.
    async fn get_recent_script_hashes(&self, limit: usize) -> Vec<String> {
        // Try to get from hootenanny via graph_context
        let args = serde_json::json!({
            "tag": "type:lua",
            "limit": limit,
        });

        match self.clients.call_tool("hootenanny", "graph_context", args).await {
            Ok(result) => {
                result
                    .get("artifacts")
                    .and_then(|a| a.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|a| a.get("hash").and_then(|h| h.as_str()).map(String::from))
                            .collect()
                    })
                    .unwrap_or_default()
            }
            Err(_) => vec![],
        }
    }

    /// Get active job IDs for completion.
    fn get_active_job_ids(&self) -> Vec<String> {
        self.jobs.list_jobs()
            .iter()
            .filter(|j| matches!(j.status, JobStatus::Pending | JobStatus::Running))
            .map(|j| j.job_id.to_string())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::SandboxConfig;

    fn create_test_handler() -> LuanetteHandler {
        let runtime = Arc::new(LuaRuntime::new(SandboxConfig::default()));
        let clients = Arc::new(ClientManager::new());
        let jobs = Arc::new(JobStore::new());
        LuanetteHandler::new(runtime, clients, jobs)
    }

    #[tokio::test]
    async fn test_lua_eval_simple() {
        let handler = create_test_handler();

        let args = serde_json::json!({
            "code": "return 2 + 2"
        });

        let result = handler.call_tool("lua_eval", args).await.unwrap();
        assert!(!result.is_error);

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
        assert!(tools.iter().any(|t| t.name == "job_execute"));
        assert!(tools.iter().any(|t| t.name == "lua_describe"));
        assert!(tools.iter().any(|t| t.name == "job_status"));
        assert!(tools.iter().any(|t| t.name == "job_poll"));
        assert!(tools.iter().any(|t| t.name == "job_cancel"));
        assert!(tools.iter().any(|t| t.name == "job_list"));
        assert!(tools.iter().any(|t| t.name == "script_store"));
        assert!(tools.iter().any(|t| t.name == "script_search"));
    }

    #[tokio::test]
    async fn test_unknown_tool() {
        let handler = create_test_handler();

        let result = handler
            .call_tool("nonexistent_tool", serde_json::json!({}))
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_job_list_empty() {
        let handler = create_test_handler();

        let args = serde_json::json!({});
        let result = handler.call_tool("job_list", args).await.unwrap();
        assert!(!result.is_error);

        if let Some(structured) = result.structured_content {
            let response: JobListResponse = serde_json::from_value(structured).unwrap();
            assert_eq!(response.total, 0);
            assert!(response.jobs.is_empty());
        }
    }

    #[tokio::test]
    async fn test_job_status_not_found() {
        let handler = create_test_handler();

        let args = serde_json::json!({
            "job_id": "nonexistent-job-id"
        });
        let result = handler.call_tool("job_status", args).await;
        assert!(result.is_err());
    }
}
