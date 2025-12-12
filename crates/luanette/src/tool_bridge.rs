//! Bridge between Lua scripts and upstream MCP tools.
//!
//! Registers `mcp.*` Lua globals that call upstream MCP servers via ClientManager.
//! Automatically propagates traceparent from stored span context to upstream calls.

use anyhow::Result;
use mlua::{Lua, Table, Value as LuaValue};
use serde_json::Value as JsonValue;
use std::sync::Arc;
use tokio::runtime::Handle;

use crate::clients::ClientManager;

/// Lua registry key for stored span context (must match otel_bridge.rs).
const SPAN_CONTEXT_KEY: &str = "otel_span_context";

/// Context passed to Lua for MCP tool calls.
///
/// Contains everything needed to make async MCP calls from synchronous Lua code.
#[derive(Clone)]
pub struct McpBridgeContext {
    /// The client manager for upstream MCP servers.
    pub clients: Arc<ClientManager>,

    /// Tokio runtime handle for running async code from blocking context.
    pub runtime_handle: Handle,
}

impl McpBridgeContext {
    /// Create a new MCP bridge context.
    pub fn new(clients: Arc<ClientManager>, runtime_handle: Handle) -> Self {
        Self {
            clients,
            runtime_handle,
        }
    }

    /// Call an upstream MCP tool synchronously (blocks on async).
    pub fn call_tool(
        &self,
        namespace: &str,
        tool_name: &str,
        arguments: JsonValue,
        traceparent: Option<&str>,
    ) -> Result<JsonValue> {
        self.runtime_handle.block_on(async {
            self.clients
                .call_tool_with_traceparent(namespace, tool_name, arguments, traceparent)
                .await
        })
    }
}

/// Get W3C traceparent from stored span context in Lua registry.
fn get_traceparent_from_lua(lua: &Lua) -> Option<String> {
    let table: Option<Table> = lua.named_registry_value(SPAN_CONTEXT_KEY).ok();
    table.and_then(|t| {
        let trace_id: String = t.get("trace_id").ok()?;
        let span_id: String = t.get("span_id").ok()?;
        let sampled: bool = t.get("sampled").ok()?;
        let flags = if sampled { "01" } else { "00" };
        Some(format!("00-{}-{}-{}", trace_id, span_id, flags))
    })
}

/// Register the `mcp` global table with namespace sub-tables.
///
/// Creates a structure like:
/// ```lua
/// mcp.hootenanny.orpheus_generate({ temperature = 1.0 })
/// mcp.hootenanny.cas_inspect({ hash = "abc123" })
/// ```
pub fn register_mcp_globals(lua: &Lua, ctx: McpBridgeContext) -> Result<()> {
    let globals = lua.globals();

    // Create the root mcp table
    let mcp_table = lua.create_table()?;

    // Get all namespaces and their tools
    let namespaces_and_tools = ctx.runtime_handle.block_on(async {
        ctx.clients.all_tools().await
    });

    // Group tools by namespace
    let mut tools_by_namespace: std::collections::HashMap<String, Vec<(String, String)>> =
        std::collections::HashMap::new();

    for (qualified_name, tool) in namespaces_and_tools {
        if let Some((namespace, _tool_name)) = ClientManager::parse_qualified_name(&qualified_name) {
            tools_by_namespace
                .entry(namespace.to_string())
                .or_default()
                .push((tool.name.clone(), tool.description.clone()));
        }
    }

    // Create namespace sub-tables with tool functions
    for (namespace, tools) in tools_by_namespace {
        let ns_table = create_namespace_table(lua, &ctx, &namespace, &tools)?;
        mcp_table.set(namespace.as_str(), ns_table)?;
    }

    globals.set("mcp", mcp_table)?;

    Ok(())
}

/// Create a namespace table with functions for each tool.
fn create_namespace_table(
    lua: &Lua,
    ctx: &McpBridgeContext,
    namespace: &str,
    tools: &[(String, String)],
) -> Result<Table> {
    let ns_table = lua.create_table()?;

    for (tool_name, _description) in tools {
        let ctx_clone = ctx.clone();
        let namespace_clone = namespace.to_string();
        let tool_name_clone = tool_name.clone();

        let func = lua.create_function(move |lua, args: LuaValue| {
            let json_args = lua_to_json(&args).map_err(mlua::Error::external)?;

            // Get traceparent from stored span context for distributed tracing
            let traceparent = get_traceparent_from_lua(lua);

            let result = ctx_clone
                .call_tool(
                    &namespace_clone,
                    &tool_name_clone,
                    json_args,
                    traceparent.as_deref(),
                )
                .map_err(mlua::Error::external)?;

            json_to_lua(lua, &result).map_err(mlua::Error::external)
        })?;

        ns_table.set(tool_name.as_str(), func)?;
    }

    Ok(ns_table)
}

/// Convert a Lua value to a JSON value.
fn lua_to_json(lua_val: &LuaValue) -> Result<JsonValue> {
    match lua_val {
        LuaValue::Nil => Ok(JsonValue::Null),
        LuaValue::Boolean(b) => Ok(JsonValue::Bool(*b)),
        LuaValue::Integer(i) => Ok(JsonValue::Number((*i).into())),
        LuaValue::Number(n) => serde_json::Number::from_f64(*n)
            .map(JsonValue::Number)
            .ok_or_else(|| anyhow::anyhow!("Cannot convert NaN/Inf to JSON")),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lua_to_json_primitives() {
        assert_eq!(lua_to_json(&LuaValue::Nil).unwrap(), JsonValue::Null);
        assert_eq!(
            lua_to_json(&LuaValue::Boolean(true)).unwrap(),
            JsonValue::Bool(true)
        );
        assert_eq!(
            lua_to_json(&LuaValue::Integer(42)).unwrap(),
            JsonValue::Number(42.into())
        );
    }

    #[test]
    fn test_json_to_lua_primitives() {
        let lua = Lua::new();

        let result = json_to_lua(&lua, &JsonValue::Null).unwrap();
        assert!(matches!(result, LuaValue::Nil));

        let result = json_to_lua(&lua, &JsonValue::Bool(false)).unwrap();
        assert!(matches!(result, LuaValue::Boolean(false)));

        let result = json_to_lua(&lua, &JsonValue::Number(123.into())).unwrap();
        assert!(matches!(result, LuaValue::Integer(123)));
    }

    #[test]
    fn test_json_roundtrip() {
        let lua = Lua::new();

        let original = serde_json::json!({
            "name": "test",
            "count": 42,
            "active": true,
            "items": [1, 2, 3]
        });

        let lua_val = json_to_lua(&lua, &original).unwrap();
        let back = lua_to_json(&lua_val).unwrap();

        assert_eq!(original, back);
    }
}
