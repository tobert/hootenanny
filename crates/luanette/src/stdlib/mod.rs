//! Standard library modules for Lua scripts.
//!
//! These provide domain-specific functionality beyond the core MCP proxying.
//! Each module is optional and can be enabled/disabled independently.

pub mod midi;
pub mod temp;

use anyhow::Result;
use mlua::Lua;

/// Register all stdlib modules in the Lua VM.
pub fn register_all(lua: &Lua) -> Result<()> {
    midi::register_midi_globals(lua)?;
    temp::register_temp_globals(lua)?;
    Ok(())
}
