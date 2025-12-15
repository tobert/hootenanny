//! Artifact convenience functions for Lua scripts.
//!
//! Provides `artifact.*` namespace with local access to artifact content.
//! Uses CAS for direct file access while artifact metadata comes from hootenanny.
//!
//! # Usage
//!
//! ```lua
//! -- Get local path to artifact content (via CAS)
//! local path = artifact.path("artifact_abc123")
//! local track = midi.read(path)
//!
//! -- Get artifact with metadata (calls hootenanny)
//! local art = hootenanny.artifact_get({ id = "artifact_abc123" })
//! -- Then get path from content_hash
//! local path = cas.path(art.content_hash)
//! ```
//!
//! Note: For full artifact metadata, use `hootenanny.artifact_get()`.
//! This module provides the shortcut `artifact.path()` which:
//! 1. Calls hootenanny.artifact_get to get content_hash
//! 2. Returns cas.path(content_hash)

use anyhow::Result;
use mlua::Lua;

/// Register the `artifact` global table.
///
/// Note: artifact.path() requires the hootenanny tool bridge to be registered,
/// as it calls hootenanny.artifact_get internally.
pub fn register_artifact_globals(lua: &Lua) -> Result<()> {
    let globals = lua.globals();
    let artifact_table = lua.create_table()?;

    // artifact.path(id) -> string|nil
    // Get local filesystem path for an artifact's content
    // This is a convenience that combines hootenanny.artifact_get + cas.path
    let path_fn = lua.create_function(|lua, id: String| -> mlua::Result<Option<String>> {
        // Get the hootenanny namespace
        let globals = lua.globals();
        let hootenanny: mlua::Table = match globals.get("hootenanny") {
            Ok(t) => t,
            Err(_) => return Err(mlua::Error::external(
                "hootenanny namespace not available - tool bridge not registered"
            )),
        };

        // Call hootenanny.artifact_get({ id = id })
        let artifact_get: mlua::Function = hootenanny.get("artifact_get")
            .map_err(|_| mlua::Error::external("hootenanny.artifact_get not available"))?;

        let params = lua.create_table()?;
        params.set("id", id.clone())?;

        let result: mlua::Value = artifact_get.call(params)?;

        // Extract content_hash from result
        let content_hash: Option<String> = match result {
            mlua::Value::Table(t) => t.get("content_hash").ok(),
            _ => None,
        };

        let content_hash = match content_hash {
            Some(h) => h,
            None => return Ok(None::<String>), // Artifact not found or no content_hash
        };

        // Get the cas namespace and call cas.path
        let cas: mlua::Table = match globals.get("cas") {
            Ok(t) => t,
            Err(_) => return Err(mlua::Error::external("cas namespace not available")),
        };

        let cas_path: mlua::Function = cas.get("path")
            .map_err(|_| mlua::Error::external("cas.path not available"))?;

        cas_path.call(content_hash)
    })?;
    artifact_table.set("path", path_fn)?;

    // artifact.content_hash(id) -> string|nil
    // Get just the content hash for an artifact
    let hash_fn = lua.create_function(|lua, id: String| -> mlua::Result<Option<String>> {
        let globals = lua.globals();
        let hootenanny: mlua::Table = match globals.get("hootenanny") {
            Ok(t) => t,
            Err(_) => return Ok(None),
        };

        let artifact_get: mlua::Function = match hootenanny.get("artifact_get") {
            Ok(f) => f,
            Err(_) => return Ok(None),
        };

        let params = lua.create_table()?;
        params.set("id", id)?;

        let result: mlua::Value = artifact_get.call(params)?;

        match result {
            mlua::Value::Table(t) => Ok(t.get("content_hash").ok()),
            _ => Ok(None),
        }
    })?;
    artifact_table.set("content_hash", hash_fn)?;

    globals.set("artifact", artifact_table)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_artifact_globals() {
        let lua = Lua::new();
        // This will work but artifact.path() will fail without hootenanny
        register_artifact_globals(&lua).unwrap();

        let globals = lua.globals();
        let artifact: mlua::Table = globals.get("artifact").unwrap();
        assert!(artifact.contains_key("path").unwrap());
        assert!(artifact.contains_key("content_hash").unwrap());
    }
}
