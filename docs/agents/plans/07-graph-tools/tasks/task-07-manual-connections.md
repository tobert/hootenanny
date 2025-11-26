# Task 07: Manual Connection Tracking

**Status**: ✅ Complete (2 tests passing)
**Estimated effort**: 2 hours
**Prerequisites**: Task 01 (SQLite foundation)
**Depends on**: Database layer (Task 01)
**Enables**: Full signal path visibility (software + hardware)

> **Note**: This task only requires the SQLite layer, not the full MCP stack. Can be done after Task 01 or in parallel with Tasks 02-05.

## 🎯 Goal

Implement MCP tools to record and query **manual connections**—physical patch cables that the system can't auto-detect.

**Why this matters:** Eurorack modules connect via patch cables (CV, gate, audio). The computer can't see these. Agents need to know the full signal path to understand the setup.

## 📋 Context

### What We're Tracking

```
Poly 2 (CV Out 1) --[patch cable]--> Doepfer A-110 (V/Oct In)
Doepfer A-110 (Audio Out) --[patch cable]--> Bitbox (Input 1)
```

This becomes:
```sql
INSERT INTO manual_connections (
    from_identity, from_port,
    to_identity, to_port,
    transport_kind, signal_direction
) VALUES (
    'poly2', 'cv_out_1',
    'doepfer_a110', 'voct_in',
    'patch_cable_cv', 'forward'
);
```

### Use Cases

1. **Signal tracing**: "Show me the path from MIDI input to Bitbox"
   - MIDI → Poly 2 → (manual: CV) → A-110 → (manual: audio) → Bitbox
2. **Troubleshooting**: "Why isn't A-110 responding?"
   - Agent checks: no manual connection from Poly 2 CV out
3. **Patch documentation**: "Record this patch for later"
   - Agent captures all manual connections as a "patch snapshot"

## 🎨 MCP Tools

```rust
#[tool]
async fn graph_connect(
    /// Source identity (e.g., "poly2")
    from_identity: String,
    /// Source port (e.g., "cv_out_1")
    from_port: String,
    /// Destination identity
    to_identity: String,
    /// Destination port
    to_port: String,
    /// Optional transport type: "patch_cable_cv", "patch_cable_audio", "din_midi", etc.
    transport: Option<String>,
) -> Result<ManualConnection, String>;

#[tool]
async fn graph_disconnect(
    /// Connection ID to remove
    connection_id: Option<String>,
    /// Or specify endpoints to find and remove
    from_identity: Option<String>,
    from_port: Option<String>,
    to_identity: Option<String>,
    to_port: Option<String>,
) -> Result<(), String>;

#[tool]
async fn graph_connections(
    /// Optional identity to filter by (show all connections involving this identity)
    identity: Option<String>,
) -> Result<Vec<ManualConnection>, String>;
```

## 🔨 Implementation (src/mcp_tools/connections.rs)

```rust
use crate::db::Database;
use crate::types::ManualConnection;
use std::sync::Arc;
use uuid::Uuid;

pub async fn graph_connect(
    db: Arc<Database>,
    from_identity: String,
    from_port: String,
    to_identity: String,
    to_port: String,
    transport: Option<String>,
) -> Result<ManualConnection, String> {
    // Validate identities exist
    db.get_identity(&from_identity)
        .map_err(|e| format!("Database error: {}", e))?
        .ok_or_else(|| format!("From identity not found: {}", from_identity))?;

    db.get_identity(&to_identity)
        .map_err(|e| format!("Database error: {}", e))?
        .ok_or_else(|| format!("To identity not found: {}", to_identity))?;

    // Create connection
    let id = Uuid::new_v4().to_string();
    let conn = db.add_manual_connection(
        &id,
        &from_identity,
        &from_port,
        &to_identity,
        &to_port,
        transport.as_deref(),
        Some("forward"),
        "mcp_tool",
    ).map_err(|e| format!("Failed to create connection: {}", e))?;

    // Log to changelog
    db.log_event(
        "mcp_tool",
        "connection_add",
        "connection",
        &id,
        serde_json::json!({
            "from": format!("{}:{}", from_identity, from_port),
            "to": format!("{}:{}", to_identity, to_port),
            "transport": transport,
        }),
    ).ok();

    Ok(conn)
}

pub async fn graph_disconnect(
    db: Arc<Database>,
    connection_id: Option<String>,
    from_identity: Option<String>,
    from_port: Option<String>,
    to_identity: Option<String>,
    to_port: Option<String>,
) -> Result<(), String> {
    if let Some(id) = connection_id {
        db.remove_manual_connection(&id)
            .map_err(|e| format!("Failed to remove connection: {}", e))?;
    } else {
        // Find by endpoints
        let connections = db.get_manual_connections(None)
            .map_err(|e| format!("Failed to query connections: {}", e))?;

        let matching = connections.iter().find(|c| {
            from_identity.as_ref().map_or(true, |fi| &c.from_identity == fi)
                && from_port.as_ref().map_or(true, |fp| &c.from_port == fp)
                && to_identity.as_ref().map_or(true, |ti| &c.to_identity == ti)
                && to_port.as_ref().map_or(true, |tp| &c.to_port == tp)
        });

        if let Some(conn) = matching {
            db.remove_manual_connection(&conn.id)
                .map_err(|e| format!("Failed to remove connection: {}", e))?;
        } else {
            return Err("No matching connection found".into());
        }
    }

    Ok(())
}

pub async fn graph_connections(
    db: Arc<Database>,
    identity: Option<String>,
) -> Result<Vec<ManualConnection>, String> {
    db.get_manual_connections(identity.as_deref())
        .map_err(|e| format!("Failed to query connections: {}", e))
}
```

## 🔨 Database Extension (src/db/connections.rs)

```rust
impl Database {
    pub fn add_manual_connection(
        &self,
        id: &str,
        from_identity: &str,
        from_port: &str,
        to_identity: &str,
        to_port: &str,
        transport_kind: Option<&str>,
        signal_direction: Option<&str>,
        created_by: &str,
    ) -> Result<ManualConnection> {
        self.conn.execute(
            "INSERT INTO manual_connections
             (id, from_identity, from_port, to_identity, to_port,
              transport_kind, signal_direction, created_by)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![id, from_identity, from_port, to_identity, to_port,
                    transport_kind, signal_direction, created_by],
        )?;

        self.get_manual_connection(id)?
            .ok_or_else(|| anyhow::anyhow!("Failed to retrieve created connection"))
    }

    pub fn remove_manual_connection(&self, id: &str) -> Result<()> {
        self.conn.execute("DELETE FROM manual_connections WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn get_manual_connections(&self, identity: Option<&str>) -> Result<Vec<ManualConnection>> {
        let query = if let Some(id) = identity {
            format!(
                "SELECT * FROM manual_connections
                 WHERE from_identity = '{}' OR to_identity = '{}'",
                id, id
            )
        } else {
            "SELECT * FROM manual_connections".to_string()
        };

        let mut stmt = self.conn.prepare(&query)?;
        let conns = stmt.query_map([], |row| {
            Ok(ManualConnection {
                id: row.get(0)?,
                from_identity: row.get(1)?,
                from_port: row.get(2)?,
                to_identity: row.get(3)?,
                to_port: row.get(4)?,
                transport_kind: row.get(5)?,
                signal_direction: row.get(6)?,
                created_at: row.get(7)?,
                created_by: row.get(8)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

        Ok(conns)
    }
}
```

## 🧪 Testing

```rust
#[tokio::test]
async fn test_graph_connect() {
    let db = setup_test_db();

    // Create identities
    db.create_identity("poly2", "Polyend Poly 2", json!({})).unwrap();
    db.create_identity("a110", "Doepfer A-110", json!({})).unwrap();

    let conn = graph_connect(
        db.clone(),
        "poly2".into(),
        "cv_out_1".into(),
        "a110".into(),
        "voct_in".into(),
        Some("patch_cable_cv".into()),
    ).await.unwrap();

    assert_eq!(conn.from_identity, "poly2");
    assert_eq!(conn.to_identity, "a110");
}

#[tokio::test]
async fn test_graph_connections_filter() {
    let db = setup_test_db();

    db.create_identity("poly2", "Polyend Poly 2", json!({})).unwrap();
    db.create_identity("a110", "Doepfer A-110", json!({})).unwrap();
    db.create_identity("bitbox", "1010music Bitbox", json!({})).unwrap();

    // Add connections
    graph_connect(db.clone(), "poly2".into(), "cv_out_1".into(),
                  "a110".into(), "voct_in".into(), None).await.unwrap();
    graph_connect(db.clone(), "a110".into(), "audio_out".into(),
                  "bitbox".into(), "input_1".into(), None).await.unwrap();

    // Query connections for a110
    let conns = graph_connections(db.clone(), Some("a110".into())).await.unwrap();

    assert_eq!(conns.len(), 2);  // Both incoming and outgoing
}
```

## ✅ Acceptance Criteria

1. ✅ `graph_connect` creates manual connection
2. ✅ `graph_disconnect` removes connection by ID or endpoints
3. ✅ `graph_connections` returns all or filtered connections
4. ✅ Validates that identities exist before creating connection
5. ✅ Logs connections to changelog

## 💡 Future Enhancements

- **Patch snapshots**: Save/load entire patch configurations
- **Connection validation**: Warn if port types don't match (CV → audio)
- **Visual diagram**: Generate graphviz/mermaid diagram from connections

## 🎬 Next Task

**[Task 08: Testing with Virtual Devices](task-08-testing-fixtures.md)** - Reproducible test environment
