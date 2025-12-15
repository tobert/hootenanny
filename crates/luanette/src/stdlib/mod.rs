//! Standard library modules for Lua scripts.
//!
//! These provide domain-specific functionality beyond the upstream tool calls.
//! Each module provides direct local access for performance.
//!
//! # Available Modules
//!
//! - `artifact.*` - Artifact convenience functions (wraps cas + hootenanny)
//! - `cas.*` - Content Addressable Storage (local filesystem access)
//! - `midi.*` - MIDI read/write/transform
//! - `temp.*` - Temporary file management

pub mod artifact;
pub mod cas;
pub mod midi;
pub mod temp;

use anyhow::Result;
use mlua::Lua;

/// Register all stdlib modules in the Lua VM.
pub fn register_all(lua: &Lua) -> Result<()> {
    cas::register_cas_globals(lua)?;
    midi::register_midi_globals(lua)?;
    temp::register_temp_globals(lua)?;
    // Note: artifact is registered after tool_globals since it depends on hootenanny.*
    Ok(())
}

/// Register artifact module (must be called after tool_globals are registered)
pub fn register_artifact(lua: &Lua) -> Result<()> {
    artifact::register_artifact_globals(lua)?;
    Ok(())
}
