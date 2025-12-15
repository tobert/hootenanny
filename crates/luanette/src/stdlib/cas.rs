//! CAS (Content Addressable Storage) access for Lua scripts.
//!
//! Provides `cas.*` namespace with functions for local CAS access.
//! Uses direct filesystem access for performance - no ZMQ overhead.
//!
//! # Usage
//!
//! ```lua
//! -- Get path to a CAS file (for midi.read, etc.)
//! local path = cas.path("abc123def456...")
//! local track = midi.read(path)
//!
//! -- Store a file in CAS
//! local hash = cas.store_file("/tmp/output.mid", "audio/midi")
//!
//! -- Check if content exists
//! if cas.exists(hash) then
//!     log.info("Content found")
//! end
//!
//! -- Get file size
//! local size = cas.size(hash)
//! ```

use anyhow::Result;
use cas::{CasConfig, ContentStore, FileStore};
use mlua::Lua;
use std::sync::OnceLock;

/// Global CAS store - initialized once on first access
static CAS_STORE: OnceLock<FileStore> = OnceLock::new();

/// Initialize the CAS store (called once)
fn init_store() -> FileStore {
    let config = CasConfig::from_env()
        .expect("Failed to load CAS config from environment");
    FileStore::new(config)
        .expect("Failed to initialize CAS store")
}

/// Get or initialize the CAS store
fn get_store() -> &'static FileStore {
    CAS_STORE.get_or_init(init_store)
}

/// Register the `cas` global table.
pub fn register_cas_globals(lua: &Lua) -> Result<()> {
    let globals = lua.globals();
    let cas_table = lua.create_table()?;

    // cas.path(hash) -> string|nil
    // Returns local filesystem path for a CAS hash
    let path_fn = lua.create_function(|_, hash: String| {
        let store = get_store();
        match store.path(&hash.parse().map_err(mlua::Error::external)?) {
            Some(path) => Ok(Some(path.to_string_lossy().to_string())),
            None => Ok(None),
        }
    })?;
    cas_table.set("path", path_fn)?;

    // cas.exists(hash) -> boolean
    // Check if content exists in CAS
    let exists_fn = lua.create_function(|_, hash: String| {
        let store = get_store();
        let content_hash = hash.parse().map_err(mlua::Error::external)?;
        Ok(store.path(&content_hash).is_some())
    })?;
    cas_table.set("exists", exists_fn)?;

    // cas.size(hash) -> number|nil
    // Get file size in bytes
    let size_fn = lua.create_function(|_, hash: String| {
        let store = get_store();
        let content_hash = hash.parse().map_err(mlua::Error::external)?;
        match store.path(&content_hash) {
            Some(path) => {
                let metadata = std::fs::metadata(&path).map_err(mlua::Error::external)?;
                Ok(Some(metadata.len()))
            }
            None => Ok(None),
        }
    })?;
    cas_table.set("size", size_fn)?;

    // cas.store_file(path, mime_type) -> hash
    // Store a local file in CAS, returns hash
    let store_file_fn = lua.create_function(|_, (path, mime_type): (String, String)| {
        let store = get_store();
        let data = std::fs::read(&path)
            .map_err(|e| mlua::Error::external(format!("Failed to read file {}: {}", path, e)))?;
        let hash = store.store(&data, &mime_type)
            .map_err(mlua::Error::external)?;
        Ok(hash.to_string())
    })?;
    cas_table.set("store_file", store_file_fn)?;

    // cas.store(data, mime_type) -> hash
    // Store raw bytes (as string) in CAS
    let store_fn = lua.create_function(|_, (data, mime_type): (mlua::String, String)| {
        let store = get_store();
        let bytes = data.as_bytes();
        let hash = store.store(bytes.as_ref(), &mime_type)
            .map_err(mlua::Error::external)?;
        Ok(hash.to_string())
    })?;
    cas_table.set("store", store_fn)?;

    // cas.read(hash) -> string|nil
    // Read raw bytes from CAS as string
    let read_fn = lua.create_function(|lua, hash: String| {
        let store = get_store();
        let content_hash = hash.parse().map_err(mlua::Error::external)?;
        match store.retrieve(&content_hash).map_err(mlua::Error::external)? {
            Some(data) => Ok(Some(lua.create_string(&data)?)),
            None => Ok(None),
        }
    })?;
    cas_table.set("read", read_fn)?;

    // cas.base_path() -> string
    // Get the CAS base directory path
    let base_path_fn = lua.create_function(|_, ()| {
        let store = get_store();
        Ok(store.config().base_path.to_string_lossy().to_string())
    })?;
    cas_table.set("base_path", base_path_fn)?;

    globals.set("cas", cas_table)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_cas_globals() {
        // This test requires HOOTENANNY_CAS_PATH to be set or will use default
        let lua = Lua::new();
        // Just test that registration doesn't panic
        // Actual functionality requires CAS to be configured
        let _ = register_cas_globals(&lua);
    }
}
