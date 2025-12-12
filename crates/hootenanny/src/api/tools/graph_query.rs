//! Trustfall query execution for the audio graph
//!
//! Provides `graph_query` tool that accepts raw Trustfall/GraphQL queries
//! and executes them against the audio graph adapter.
//!
//! Supports querying Identity and PipeWireNode types from the audio graph schema.

use crate::api::responses::GraphQueryResponse;
use crate::api::schema::GraphQueryRequest;
use crate::api::service::EventDualityServer;
use hooteproto::{ToolOutput, ToolResult, ToolError};
use std::collections::BTreeMap;
use std::sync::Arc;
use trustfall::{execute_query, FieldValue};

impl EventDualityServer {
    #[tracing::instrument(name = "mcp.tool.graph_query", skip(self, request))]
    pub async fn graph_query(
        &self,
        request: GraphQueryRequest,
    ) -> ToolResult {
        let variables = json_to_variables(&request.variables)
            .map_err(|e| ToolError::invalid_params(format!("Invalid variables: {}", e)))?;

        let schema = self.graph_adapter.schema();
        let adapter_arc: Arc<_> = Arc::clone(&self.graph_adapter);
        let results_iter = execute_query(schema, adapter_arc, &request.query, variables)
            .map_err(|e| ToolError::invalid_params(format!("Query execution failed: {}", e)))?;

        let limit = request.limit.unwrap_or(100);
        let results: Vec<_> = results_iter
            .take(limit)
            .map(result_to_json)
            .collect();

        let count = results.len();
        let response = GraphQueryResponse {
            results,
            count,
        };

        let json = serde_json::to_string_pretty(&response)
            .map_err(|e| ToolError::internal(format!("Failed to serialize: {}", e)))?;

        Ok(ToolOutput::new(json, &response))
    }
}

fn json_to_field_value(value: &serde_json::Value) -> Result<FieldValue, String> {
    match value {
        serde_json::Value::Null => Ok(FieldValue::Null),
        serde_json::Value::Bool(b) => Ok(FieldValue::Boolean(*b)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(FieldValue::Int64(i))
            } else if let Some(u) = n.as_u64() {
                Ok(FieldValue::Uint64(u))
            } else if let Some(f) = n.as_f64() {
                Ok(FieldValue::Float64(f))
            } else {
                Err("Number out of range".to_string())
            }
        }
        serde_json::Value::String(s) => Ok(FieldValue::String(s.as_str().into())),
        serde_json::Value::Array(arr) => {
            let items: Result<Vec<_>, _> = arr.iter().map(json_to_field_value).collect();
            Ok(FieldValue::List(items?.into()))
        }
        serde_json::Value::Object(_) => Err(
            "Objects not supported as FieldValue. Use flat variables only.".to_string(),
        ),
    }
}

fn json_to_variables(
    json: &serde_json::Value,
) -> Result<BTreeMap<Arc<str>, FieldValue>, String> {
    match json {
        serde_json::Value::Null => Ok(BTreeMap::new()),
        serde_json::Value::Object(map) => {
            let mut result = BTreeMap::new();
            for (key, value) in map {
                let field_value = json_to_field_value(value)
                    .map_err(|e| format!("Variable '{}': {}", key, e))?;
                result.insert(Arc::from(key.as_str()), field_value);
            }
            Ok(result)
        }
        _ => Err("Variables must be an object or null".to_string()),
    }
}

fn field_value_to_json(value: &FieldValue) -> serde_json::Value {
    match value {
        FieldValue::Null => serde_json::Value::Null,
        FieldValue::Boolean(b) => serde_json::Value::Bool(*b),
        FieldValue::Int64(i) => serde_json::json!(*i),
        FieldValue::Uint64(u) => serde_json::json!(*u),
        FieldValue::Float64(f) => serde_json::json!(*f),
        FieldValue::String(s) => serde_json::Value::String(s.to_string()),
        FieldValue::List(items) => {
            let arr: Vec<_> = items.iter().map(field_value_to_json).collect();
            serde_json::Value::Array(arr)
        }
        _ => serde_json::Value::String(format!("{:?}", value)),
    }
}

fn result_to_json(result: BTreeMap<Arc<str>, FieldValue>) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    for (key, value) in result {
        map.insert(key.to_string(), field_value_to_json(&value));
    }
    serde_json::Value::Object(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_to_field_value_primitives() {
        assert!(matches!(
            json_to_field_value(&serde_json::Value::Null),
            Ok(FieldValue::Null)
        ));

        assert!(matches!(
            json_to_field_value(&serde_json::json!(true)),
            Ok(FieldValue::Boolean(true))
        ));

        let result = json_to_field_value(&serde_json::json!(42)).unwrap();
        assert!(matches!(result, FieldValue::Int64(42)));

        let result = json_to_field_value(&serde_json::json!(3.14)).unwrap();
        assert!(matches!(result, FieldValue::Float64(f) if (f - 3.14).abs() < 0.001));

        let result = json_to_field_value(&serde_json::json!("hello")).unwrap();
        assert!(matches!(result, FieldValue::String(s) if s.as_ref() == "hello"));
    }

    #[test]
    fn test_json_to_field_value_array() {
        let json = serde_json::json!([1, 2, 3]);
        let result = json_to_field_value(&json).unwrap();

        if let FieldValue::List(items) = result {
            assert_eq!(items.len(), 3);
            assert!(matches!(items[0], FieldValue::Int64(1)));
            assert!(matches!(items[1], FieldValue::Int64(2)));
            assert!(matches!(items[2], FieldValue::Int64(3)));
        } else {
            panic!("Expected List variant");
        }
    }

    #[test]
    fn test_json_to_field_value_nested_array() {
        let json = serde_json::json!([["a", "b"], ["c", "d"]]);
        let result = json_to_field_value(&json).unwrap();

        if let FieldValue::List(outer) = result {
            assert_eq!(outer.len(), 2);
            if let FieldValue::List(inner) = &outer[0] {
                assert_eq!(inner.len(), 2);
            } else {
                panic!("Expected nested List");
            }
        } else {
            panic!("Expected List variant");
        }
    }

    #[test]
    fn test_json_to_field_value_object_error() {
        let json = serde_json::json!({"key": "value"});
        let result = json_to_field_value(&json);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Objects not supported"));
    }

    #[test]
    fn test_json_to_variables_empty() {
        let result = json_to_variables(&serde_json::Value::Null).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_json_to_variables_simple() {
        let json = serde_json::json!({
            "artifact_id": "artifact_123",
            "limit": 10,
            "include_deleted": false
        });

        let result = json_to_variables(&json).unwrap();
        assert_eq!(result.len(), 3);

        assert!(matches!(
            result.get("artifact_id" as &str),
            Some(FieldValue::String(s)) if s.as_ref() == "artifact_123"
        ));
        assert!(matches!(
            result.get("limit" as &str),
            Some(FieldValue::Int64(10))
        ));
        assert!(matches!(
            result.get("include_deleted" as &str),
            Some(FieldValue::Boolean(false))
        ));
    }

    #[test]
    fn test_json_to_variables_invalid_type() {
        let json = serde_json::json!({"nested": {"key": "value"}});
        let result = json_to_variables(&json);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Variable 'nested'"));
    }

    #[test]
    fn test_field_value_to_json_roundtrip() {
        let original = serde_json::json!({
            "string": "hello",
            "number": 42,
            "float": 3.14,
            "bool": true,
            "null": null,
            "array": [1, 2, 3]
        });

        let variables = json_to_variables(&original).unwrap();

        let mut roundtrip = serde_json::Map::new();
        for (key, value) in variables {
            roundtrip.insert(key.to_string(), field_value_to_json(&value));
        }
        let roundtrip_json = serde_json::Value::Object(roundtrip);

        assert_eq!(roundtrip_json["string"], original["string"]);
        assert_eq!(roundtrip_json["number"], original["number"]);
        assert_eq!(roundtrip_json["bool"], original["bool"]);
        assert_eq!(roundtrip_json["null"], original["null"]);
        assert_eq!(roundtrip_json["array"], original["array"]);
    }

    #[test]
    fn test_result_to_json() {
        let mut result = BTreeMap::new();
        result.insert("id".into(), FieldValue::String("artifact_123".into()));
        result.insert("count".into(), FieldValue::Int64(42));

        let json = result_to_json(result);

        assert_eq!(json["id"], "artifact_123");
        assert_eq!(json["count"], 42);
    }
}
