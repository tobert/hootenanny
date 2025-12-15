//! Standard library modules for Lua scripts.
//!
//! These provide domain-specific functionality beyond the upstream tool calls.
//! Each module provides direct local access for performance.
//!
//! # Available Modules
//!
//! - `abc.*` - ABC notation parsing (local, no ZMQ)
//! - `artifact.*` - Artifact convenience functions (wraps cas + hootenanny)
//! - `cas.*` - Content Addressable Storage (local filesystem access)
//! - `midi.*` - MIDI read/write/transform
//! - `soundfont.*` - SoundFont inspection (local, no ZMQ)
//! - `temp.*` - Temporary file management
//! - `workflow.*` - High-level helpers (wait_job, etc.)

pub mod abc;
pub mod artifact;
pub mod cas;
pub mod midi;
pub mod soundfont;
pub mod temp;
pub mod workflow;

use anyhow::Result;
use mlua::Lua;

/// Register all stdlib modules in the Lua VM.
pub fn register_all(lua: &Lua) -> Result<()> {
    abc::register_abc_globals(lua)?;
    cas::register_cas_globals(lua)?;
    midi::register_midi_globals(lua)?;
    soundfont::register_soundfont_globals(lua)?;
    temp::register_temp_globals(lua)?;
    // Note: artifact and workflow are registered after tool_globals since they depend on hootenanny.*
    Ok(())
}

/// Register modules that depend on tool_globals (must be called after)
pub fn register_dependent_modules(lua: &Lua) -> Result<()> {
    artifact::register_artifact_globals(lua)?;
    workflow::register_workflow_globals(lua)?;
    Ok(())
}
