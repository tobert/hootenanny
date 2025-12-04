//! Schema validation tests
//!
//! These tests validate that our MCP tool schemas are compatible with strict
//! MCP clients like Gemini CLI. They catch issues like arbitrary JSON types
//! (serde_json::Value) that fail validation.

use baton::Handler;
use hootenanny::api::handler::HootHandler;
use hootenanny::api::service::EventDualityServer;
use serde_json::Value;
use std::sync::Arc;

/// Create a minimal test handler
fn create_test_handler() -> HootHandler {
    use std::path::Path;

    // We don't need real implementations for schema validation
    let cas = hootenanny::cas::Cas::new(Path::new("/tmp/test_cas")).unwrap();

    let local_models = Arc::new(
        hootenanny::mcp_tools::local_models::LocalModels::new(cas, 2002),
    );

    let artifact_store = Arc::new(std::sync::RwLock::new(
        hootenanny::artifact_store::FileStore::new("/tmp/test_artifacts").unwrap(),
    ));

    let job_store = hootenanny::job_system::JobStore::new();

    let audio_graph_db = Arc::new(
        audio_graph_mcp::Database::in_memory().unwrap(),
    );

    let pipewire_snapshot = audio_graph_mcp::PipeWireSnapshot {
        nodes: vec![],
        ports: vec![],
        links: vec![],
    };

    let graph_adapter = Arc::new(
        audio_graph_mcp::AudioGraphAdapter::new(
            audio_graph_db.clone(),
            pipewire_snapshot,
        ).unwrap(),
    );

    let server = Arc::new(EventDualityServer::new(
        local_models,
        artifact_store,
        Arc::new(job_store),
        audio_graph_db,
        graph_adapter,
    ));

    HootHandler::new(server)
}

/// Validate that a JSON schema doesn't contain arbitrary Value types
/// that fail strict MCP client validation (like Gemini CLI)
fn validate_schema_structure(schema: &Value, path: &str) -> Result<(), String> {
    match schema {
        Value::Object(obj) => {
            // Check for problematic patterns
            if let Some(properties) = obj.get("properties").and_then(|v| v.as_object()) {
                for (prop_name, prop_schema) in properties {
                    let prop_path = format!("{}.{}", path, prop_name);

                    // Check for boolean schema (true = any value allowed)
                    if prop_schema.as_bool() == Some(true) {
                        return Err(format!(
                            "Property '{}' has boolean schema 'true' (unconstrained serde_json::Value). \
                             This fails validation in strict MCP clients.",
                            prop_path
                        ));
                    }

                    // Check if this is an arbitrary JSON value (no type constraint)
                    if let Some(prop_obj) = prop_schema.as_object() {
                        // If there's no "type" field and no "$ref", it might be a Value
                        let has_type = prop_obj.contains_key("type");
                        let has_ref = prop_obj.contains_key("$ref");
                        let has_any_of = prop_obj.contains_key("anyOf");
                        let has_one_of = prop_obj.contains_key("oneOf");

                        if !has_type && !has_ref && !has_any_of && !has_one_of {
                            // This looks like an unconstrained Value
                            return Err(format!(
                                "Property '{}' has no type constraint (likely serde_json::Value). \
                                 This fails validation in strict MCP clients.",
                                prop_path
                            ));
                        }

                        // Recursively validate nested schemas
                        validate_schema_structure(prop_schema, &prop_path)?;
                    }
                }
            }

            // Validate anyOf/oneOf branches
            if let Some(any_of) = obj.get("anyOf").and_then(|v| v.as_array()) {
                for (i, variant) in any_of.iter().enumerate() {
                    validate_schema_structure(variant, &format!("{}[anyOf:{}]", path, i))?;
                }
            }

            if let Some(one_of) = obj.get("oneOf").and_then(|v| v.as_array()) {
                for (i, variant) in one_of.iter().enumerate() {
                    validate_schema_structure(variant, &format!("{}[oneOf:{}]", path, i))?;
                }
            }

            Ok(())
        }
        Value::Array(arr) => {
            for (i, item) in arr.iter().enumerate() {
                validate_schema_structure(item, &format!("{}[{}]", path, i))?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

#[test]
fn test_all_tool_schemas_are_valid() {
    let handler = create_test_handler();
    let tools = handler.tools();

    let mut failures = Vec::new();

    for (index, tool) in tools.iter().enumerate() {
        // Validate input schema
        let schema_value = serde_json::to_value(&tool.input_schema).unwrap();
        if let Err(e) = validate_schema_structure(
            &schema_value,
            &format!("tools[{}:{}].inputSchema", index, tool.name),
        ) {
            failures.push(format!("Tool #{} '{}' input schema: {}", index, tool.name, e));
        }

        // Validate output schema if present
        if let Some(output_schema) = &tool.output_schema {
            let schema_value = serde_json::to_value(output_schema).unwrap();
            if let Err(e) = validate_schema_structure(
                &schema_value,
                &format!("tools[{}:{}].outputSchema", index, tool.name),
            ) {
                failures.push(format!("Tool #{} '{}' output schema: {}", index, tool.name, e));
            }
        }
    }

    if !failures.is_empty() {
        panic!(
            "\n\nSchema validation failures:\n{}\n\n\
             These failures indicate serde_json::Value usage in response types.\n\
             Replace with concrete types or use #[schemars(schema_with = \"...\")] attributes.\n",
            failures.join("\n")
        );
    }
}

#[test]
fn test_no_refs_in_schemas() {
    let handler = create_test_handler();
    let tools = handler.tools();

    // Verify that inline_subschemas is working (no $ref or $defs)
    for tool in tools.iter() {
        let json = serde_json::to_value(&tool.input_schema).unwrap();
        assert!(
            !json.to_string().contains("\"$ref\""),
            "Tool '{}' input schema contains $ref (inline_subschemas not working)",
            tool.name
        );

        if let Some(schema) = &tool.output_schema {
            let json = serde_json::to_value(schema).unwrap();
            assert!(
                !json.to_string().contains("\"$ref\""),
                "Tool '{}' output schema contains $ref (inline_subschemas not working)",
                tool.name
            );
        }
    }
}
