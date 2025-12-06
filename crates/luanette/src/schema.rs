//! Request and response types for Luanette MCP tools.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Request to execute a Lua script stored in CAS.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LuaExecuteRequest {
    /// CAS hash of the Lua script to execute.
    pub script_hash: String,

    /// Parameters to pass to the script's main() function.
    #[serde(default)]
    pub params: Option<Value>,

    /// Creator identifier for artifact tracking.
    #[serde(default)]
    pub creator: Option<String>,

    /// Tags for the execution result artifact.
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Request to evaluate Lua code directly (for debugging/quick scripts).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LuaEvalRequest {
    /// Lua code to evaluate.
    pub code: String,

    /// Parameters to pass to the script's main() function.
    #[serde(default)]
    pub params: Option<Value>,
}

/// Request to describe a Lua script's interface.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LuaDescribeRequest {
    /// CAS hash of the Lua script to describe.
    pub script_hash: String,
}

/// Response from lua_execute tool.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LuaExecuteResponse {
    /// Job ID for tracking the execution.
    pub job_id: String,
}

/// Response from lua_eval tool.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LuaEvalResponse {
    /// Result returned by the script (JSON-serialized).
    pub result: Value,

    /// Execution duration in milliseconds.
    pub duration_ms: u64,
}

/// Response from lua_describe tool.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LuaDescribeResponse {
    /// Script name from describe() function.
    pub name: Option<String>,

    /// Script description from describe() function.
    pub description: Option<String>,

    /// Parameter schema from describe() function.
    pub params: Option<Value>,

    /// Return type description from describe() function.
    pub returns: Option<String>,
}

/// Script parameter definition (from describe() function).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ParamDefinition {
    /// Parameter type (string, number, boolean, table).
    #[serde(rename = "type")]
    pub param_type: String,

    /// Whether the parameter is required.
    #[serde(default)]
    pub required: bool,

    /// Default value if not provided.
    #[serde(default)]
    pub default: Option<Value>,

    /// Description of the parameter.
    #[serde(default)]
    pub description: Option<String>,
}
