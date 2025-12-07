//! Temp file management for Lua scripts.
//!
//! Provides `temp.*` namespace with functions for creating and managing
//! temporary files and directories.
//!
//! # Usage
//!
//! ```lua
//! -- Get a path for a temp file
//! local path = temp.path("output.mid")
//!
//! -- Write some data
//! midi.write(path, track)
//!
//! -- Upload to CAS
//! local result = mcp.hootenanny.cas_upload_file({
//!     file_path = path,
//!     mime_type = "audio/midi"
//! })
//!
//! -- Temp files are cleaned up when the script ends
//! ```

use anyhow::Result;
use mlua::Lua;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tempfile::TempDir;

/// Temp directory manager.
///
/// Manages a temporary directory and tracks created files.
/// The directory is automatically cleaned up when the manager is dropped.
pub struct TempManager {
    dir: TempDir,
    created_files: Mutex<Vec<PathBuf>>,
}

impl TempManager {
    /// Create a new temp manager with a unique temp directory.
    pub fn new() -> Result<Self> {
        let dir = TempDir::with_prefix("luanette-")?;
        Ok(Self {
            dir,
            created_files: Mutex::new(Vec::new()),
        })
    }

    /// Get a path for a temp file with the given name.
    pub fn path(&self, filename: &str) -> PathBuf {
        let path = self.dir.path().join(filename);
        if let Ok(mut files) = self.created_files.lock() {
            files.push(path.clone());
        }
        path
    }

    /// Get the temp directory path.
    pub fn dir_path(&self) -> &std::path::Path {
        self.dir.path()
    }

    /// List all created temp files.
    pub fn list_files(&self) -> Vec<PathBuf> {
        self.created_files
            .lock()
            .map(|f| f.clone())
            .unwrap_or_default()
    }
}

/// Register the `temp` global table.
///
/// Creates a fresh TempManager for each Lua VM, ensuring script-level
/// isolation and automatic cleanup.
pub fn register_temp_globals(lua: &Lua) -> Result<()> {
    let globals = lua.globals();
    let temp_table = lua.create_table()?;

    // Create a temp manager for this Lua VM
    // Use Arc<Mutex> so closures can share it
    let manager = Arc::new(TempManager::new()?);

    // temp.path(filename) -> full path string
    let manager_clone = manager.clone();
    let path_fn = lua.create_function(move |_, filename: String| {
        Ok(manager_clone.path(&filename).to_string_lossy().to_string())
    })?;
    temp_table.set("path", path_fn)?;

    // temp.dir() -> temp directory path
    let manager_clone = manager.clone();
    let dir_fn = lua.create_function(move |_, ()| {
        Ok(manager_clone.dir_path().to_string_lossy().to_string())
    })?;
    temp_table.set("dir", dir_fn)?;

    // temp.list() -> list of created temp files
    let manager_clone = manager.clone();
    let list_fn = lua.create_function(move |lua, ()| {
        let files = manager_clone.list_files();
        let table = lua.create_table()?;
        for (i, path) in files.iter().enumerate() {
            table.set(i + 1, path.to_string_lossy().to_string())?;
        }
        Ok(table)
    })?;
    temp_table.set("list", list_fn)?;

    // temp.exists(path) -> bool
    let exists_fn = lua.create_function(|_, path: String| Ok(std::path::Path::new(&path).exists()))?;
    temp_table.set("exists", exists_fn)?;

    // temp.remove(path) -> removes a file
    let remove_fn = lua.create_function(|_, path: String| {
        std::fs::remove_file(&path).map_err(mlua::Error::external)
    })?;
    temp_table.set("remove", remove_fn)?;

    globals.set("temp", temp_table)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mlua::Table;

    #[test]
    fn test_temp_manager() {
        let manager = TempManager::new().unwrap();

        let path1 = manager.path("test.mid");
        let path2 = manager.path("output.wav");

        assert!(path1.ends_with("test.mid"));
        assert!(path2.ends_with("output.wav"));
        assert!(path1.starts_with(manager.dir_path()));

        let files = manager.list_files();
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn test_register_temp_globals() {
        let lua = Lua::new();
        register_temp_globals(&lua).unwrap();

        let globals = lua.globals();
        let temp: Table = globals.get("temp").unwrap();

        assert!(temp.contains_key("path").unwrap());
        assert!(temp.contains_key("dir").unwrap());
        assert!(temp.contains_key("list").unwrap());
        assert!(temp.contains_key("exists").unwrap());
        assert!(temp.contains_key("remove").unwrap());
    }

    #[test]
    fn test_temp_path_lua() {
        let lua = Lua::new();
        register_temp_globals(&lua).unwrap();

        let path: String = lua
            .load(r#"return temp.path("test.mid")"#)
            .eval()
            .unwrap();

        assert!(path.contains("luanette-"));
        assert!(path.ends_with("test.mid"));
    }

    #[test]
    fn test_temp_dir_lua() {
        let lua = Lua::new();
        register_temp_globals(&lua).unwrap();

        let dir: String = lua.load(r#"return temp.dir()"#).eval().unwrap();

        assert!(dir.contains("luanette-"));
    }
}
