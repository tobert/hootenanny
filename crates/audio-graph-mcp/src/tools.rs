use std::collections::BTreeMap;
use std::sync::Arc;

use serde::Serialize;
use trustfall::{execute_query, FieldValue};

use crate::{AudioGraphAdapter, Database, HintKind, Identity, ManualConnection};

type Variables = BTreeMap<Arc<str>, FieldValue>;

#[derive(Debug, Serialize)]
pub struct QueryResult {
    pub results: Vec<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct IdentitySummary {
    pub id: String,
    pub name: String,
    pub tags: Vec<TagPair>,
}

#[derive(Debug, Serialize)]
pub struct TagPair {
    pub namespace: String,
    pub value: String,
}

/// Execute a GraphQL query against the audio graph
pub fn graph_query(
    adapter: &Arc<AudioGraphAdapter>,
    query: &str,
) -> Result<QueryResult, String> {
    let results = execute_query(adapter.schema(), adapter.clone(), query, Variables::new())
        .map_err(|e| format!("Query error: {e}"))?
        .map(|row| {
            let obj: serde_json::Map<String, serde_json::Value> = row
                .into_iter()
                .map(|(k, v)| (k.to_string(), field_value_to_json(v)))
                .collect();
            serde_json::Value::Object(obj)
        })
        .collect();

    Ok(QueryResult { results })
}

/// Find identities by name or tags
pub fn graph_find(
    db: &Database,
    name: Option<&str>,
    tag_namespace: Option<&str>,
    tag_value: Option<&str>,
) -> Result<Vec<IdentitySummary>, String> {
    let identities = if let (Some(ns), Some(val)) = (tag_namespace, tag_value) {
        db.find_identities_by_tag(ns, val)
            .map_err(|e| format!("Database error: {e}"))?
    } else {
        db.list_identities()
            .map_err(|e| format!("Database error: {e}"))?
    };

    let mut results: Vec<IdentitySummary> = identities
        .into_iter()
        .filter(|i| {
            name.map_or(true, |n| i.name.to_lowercase().contains(&n.to_lowercase()))
        })
        .map(|i| {
            let tags = db
                .get_tags(&i.id.0)
                .unwrap_or_default()
                .into_iter()
                .map(|t| TagPair {
                    namespace: t.namespace,
                    value: t.value,
                })
                .collect();
            IdentitySummary {
                id: i.id.0,
                name: i.name,
                tags,
            }
        })
        .collect();

    results.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(results)
}

/// Create a new identity or update hints on existing one
pub fn graph_bind(
    db: &Database,
    id: &str,
    name: &str,
    hints: Vec<(HintKind, String, f64)>,
) -> Result<Identity, String> {
    let identity = if let Some(existing) = db.get_identity(id).map_err(|e| e.to_string())? {
        existing
    } else {
        db.create_identity(id, name, serde_json::json!({}))
            .map_err(|e| format!("Failed to create identity: {e}"))?
    };

    let hints_count = hints.len();
    for (kind, value, confidence) in hints {
        db.add_hint(&identity.id.0, kind, &value, confidence)
            .map_err(|e| format!("Failed to add hint: {e}"))?;
    }

    db.log_event("mcp_tool", "bind", "identity", &identity.id.0, serde_json::json!({
        "hints_added": hints_count
    })).ok();

    Ok(identity)
}

/// Add or remove tags from an identity
pub fn graph_tag(
    db: &Database,
    identity_id: &str,
    add: Vec<(String, String)>,
    remove: Vec<(String, String)>,
) -> Result<Vec<TagPair>, String> {
    // Verify identity exists
    db.get_identity(identity_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Identity not found: {identity_id}"))?;

    for (ns, val) in &add {
        db.add_tag(identity_id, ns, val)
            .map_err(|e| format!("Failed to add tag: {e}"))?;
    }

    for (ns, val) in &remove {
        db.remove_tag(identity_id, ns, val)
            .map_err(|e| format!("Failed to remove tag: {e}"))?;
    }

    let tags = db
        .get_tags(identity_id)
        .map_err(|e| format!("Failed to get tags: {e}"))?
        .into_iter()
        .map(|t| TagPair {
            namespace: t.namespace,
            value: t.value,
        })
        .collect();

    Ok(tags)
}

/// Delete an identity
pub fn graph_unbind(db: &Database, identity_id: &str) -> Result<(), String> {
    db.delete_identity(identity_id)
        .map_err(|e| format!("Failed to delete identity: {e}"))
}

/// Record a manual connection (patch cable)
pub fn graph_connect(
    db: &Database,
    from_identity: &str,
    from_port: &str,
    to_identity: &str,
    to_port: &str,
    transport: Option<&str>,
) -> Result<ManualConnection, String> {
    // Verify identities exist
    db.get_identity(from_identity)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("From identity not found: {from_identity}"))?;
    db.get_identity(to_identity)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("To identity not found: {to_identity}"))?;

    let id = uuid::Uuid::new_v4().to_string();
    let conn = db.add_connection(&id, from_identity, from_port, to_identity, to_port, transport, "mcp_tool")
        .map_err(|e| format!("Failed to create connection: {e}"))?;

    db.log_event("mcp_tool", "connect", "connection", &id, serde_json::json!({
        "from": format!("{}:{}", from_identity, from_port),
        "to": format!("{}:{}", to_identity, to_port),
    })).ok();

    Ok(conn)
}

/// Remove a manual connection
pub fn graph_disconnect(db: &Database, connection_id: &str) -> Result<(), String> {
    db.remove_connection(connection_id)
        .map_err(|e| format!("Failed to remove connection: {e}"))
}

/// List manual connections
pub fn graph_connections(db: &Database, identity: Option<&str>) -> Result<Vec<ManualConnection>, String> {
    db.list_connections(identity)
        .map_err(|e| format!("Failed to list connections: {e}"))
}

fn field_value_to_json(fv: FieldValue) -> serde_json::Value {
    match fv {
        FieldValue::Null => serde_json::Value::Null,
        FieldValue::Int64(n) => serde_json::Value::Number(n.into()),
        FieldValue::Uint64(n) => serde_json::Value::Number(n.into()),
        FieldValue::Float64(f) => serde_json::Number::from_f64(f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        FieldValue::String(s) => serde_json::Value::String(s.to_string()),
        FieldValue::Boolean(b) => serde_json::Value::Bool(b),
        FieldValue::List(l) => {
            serde_json::Value::Array(l.iter().map(|v| field_value_to_json(v.clone())).collect())
        }
        _ => serde_json::Value::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (Arc<Database>, Arc<AudioGraphAdapter>) {
        let db = Arc::new(Database::in_memory().unwrap());
        db.create_identity("jdxi", "Roland JD-Xi", serde_json::json!({})).unwrap();
        db.add_hint("jdxi", HintKind::UsbDeviceId, "0582:0160", 1.0).unwrap();
        db.add_tag("jdxi", "manufacturer", "roland").unwrap();
        db.add_tag("jdxi", "role", "sound-source").unwrap();

        let adapter = Arc::new(AudioGraphAdapter::new_without_pipewire(db.clone()).unwrap());
        (db, adapter)
    }

    #[test]
    fn test_graph_query() {
        let (_db, adapter) = setup();

        let result = graph_query(&adapter, r#"
            query { Identity { name @output } }
        "#).unwrap();

        assert_eq!(result.results.len(), 1);
        assert_eq!(result.results[0]["name"], "Roland JD-Xi");
    }

    #[test]
    fn test_graph_find_by_name() {
        let (db, _adapter) = setup();

        let results = graph_find(&db, Some("roland"), None, None).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Roland JD-Xi");
    }

    #[test]
    fn test_graph_find_by_tag() {
        let (db, _adapter) = setup();

        let results = graph_find(&db, None, Some("manufacturer"), Some("roland")).unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_graph_bind_new() {
        let (db, _adapter) = setup();

        let identity = graph_bind(
            &db,
            "keystep",
            "Arturia Keystep",
            vec![(HintKind::MidiName, "Keystep Pro".into(), 0.9)],
        ).unwrap();

        assert_eq!(identity.name, "Arturia Keystep");
        let hints = db.get_hints("keystep").unwrap();
        assert_eq!(hints.len(), 1);
    }

    #[test]
    fn test_graph_tag() {
        let (db, _adapter) = setup();

        let tags = graph_tag(
            &db,
            "jdxi",
            vec![("capability".into(), "mpe".into())],
            vec![],
        ).unwrap();

        assert_eq!(tags.len(), 3); // original 2 + new 1
    }

    #[test]
    fn test_graph_connect() {
        let (db, _adapter) = setup();
        db.create_identity("poly2", "Polyend Poly 2", serde_json::json!({})).unwrap();

        let conn = graph_connect(&db, "jdxi", "cv_out", "poly2", "cv_in", Some("patch_cable")).unwrap();
        assert_eq!(conn.from_identity.0, "jdxi");
        assert_eq!(conn.to_identity.0, "poly2");

        let conns = graph_connections(&db, Some("jdxi")).unwrap();
        assert_eq!(conns.len(), 1);

        graph_disconnect(&db, &conn.id).unwrap();
        let after = graph_connections(&db, None).unwrap();
        assert!(after.is_empty());
    }
}
