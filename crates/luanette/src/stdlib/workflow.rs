//! Workflow helpers for Lua scripts.
//!
//! Provides `workflow.*` namespace with high-level convenience functions
//! that combine multiple tool calls into single operations.
//!
//! # Usage
//!
//! ```lua
//! -- Wait for a job to complete
//! local result = workflow.wait_job(job_id, 60000)  -- 60s timeout
//!
//! -- Generate MIDI and wait for result
//! local midi = workflow.orpheus_generate({ temperature = 1.0 })
//! -- midi.artifact_id, midi.content_hash available immediately
//!
//! -- Generate and render to WAV in one call
//! local wav = workflow.orpheus_to_wav({
//!     temperature = 1.0,
//!     soundfont_hash = "abc123..."
//! })
//! ```

use anyhow::Result;
use mlua::{Lua, Table, Value as LuaValue};

/// Register the `workflow` global table.
pub fn register_workflow_globals(lua: &Lua) -> Result<()> {
    let globals = lua.globals();
    let workflow_table = lua.create_table()?;

    // workflow.wait_job(job_id, timeout_ms) -> result table
    // Poll until job completes or timeout
    let wait_job_fn = lua.create_function(|lua, (job_id, timeout_ms): (String, Option<i64>)| -> mlua::Result<Table> {
        let globals = lua.globals();
        let hootenanny: Table = globals.get("hootenanny")
            .map_err(|_| mlua::Error::external("hootenanny namespace not available"))?;

        let job_poll: mlua::Function = hootenanny.get("job_poll")
            .map_err(|_| mlua::Error::external("hootenanny.job_poll not available"))?;

        let timeout = timeout_ms.unwrap_or(60000);

        let params = lua.create_table()?;
        let job_ids = lua.create_table()?;
        job_ids.set(1, job_id.clone())?;
        params.set("job_ids", job_ids)?;
        params.set("timeout_ms", timeout)?;

        let result: Table = job_poll.call(params)?;

        // Check if job completed
        let completed: Table = result.get("completed")?;
        if completed.raw_len() > 0 {
            // Get job details
            let job_status: mlua::Function = hootenanny.get("job_status")
                .map_err(|_| mlua::Error::external("hootenanny.job_status not available"))?;

            let status_params = lua.create_table()?;
            status_params.set("job_id", job_id)?;

            return job_status.call(status_params);
        }

        // Check if failed
        let failed: Table = result.get("failed")?;
        if failed.raw_len() > 0 {
            return Err(mlua::Error::external(format!("Job {} failed", job_id)));
        }

        // Timed out
        Err(mlua::Error::external(format!("Job {} timed out after {}ms", job_id, timeout)))
    })?;
    workflow_table.set("wait_job", wait_job_fn)?;

    // workflow.orpheus_generate(params) -> result with artifact_id, content_hash
    // Generate MIDI and wait for result
    let orpheus_generate_fn = lua.create_function(|lua, params: Table| -> mlua::Result<Table> {
        let globals = lua.globals();
        let hootenanny: Table = globals.get("hootenanny")
            .map_err(|_| mlua::Error::external("hootenanny namespace not available"))?;

        // Start generation
        let orpheus_generate: mlua::Function = hootenanny.get("orpheus_generate")
            .map_err(|_| mlua::Error::external("hootenanny.orpheus_generate not available"))?;

        let gen_result: Table = orpheus_generate.call(params.clone())?;
        let job_id: String = gen_result.get("job_id")?;

        // Get timeout from params or default
        let timeout: i64 = params.get("timeout_ms").unwrap_or(120000);

        // Poll for completion
        let job_poll: mlua::Function = hootenanny.get("job_poll")
            .map_err(|_| mlua::Error::external("hootenanny.job_poll not available"))?;

        let poll_params = lua.create_table()?;
        let job_ids = lua.create_table()?;
        job_ids.set(1, job_id.clone())?;
        poll_params.set("job_ids", job_ids)?;
        poll_params.set("timeout_ms", timeout)?;

        let poll_result: Table = job_poll.call(poll_params)?;

        let completed: Table = poll_result.get("completed")?;
        if completed.raw_len() == 0 {
            return Err(mlua::Error::external(format!("Orpheus generation timed out after {}ms", timeout)));
        }

        // Get job result
        let job_status: mlua::Function = hootenanny.get("job_status")
            .map_err(|_| mlua::Error::external("hootenanny.job_status not available"))?;

        let status_params = lua.create_table()?;
        status_params.set("job_id", job_id)?;

        job_status.call(status_params)
    })?;
    workflow_table.set("orpheus_generate", orpheus_generate_fn)?;

    // workflow.orpheus_to_wav(params) -> result with wav artifact_id
    // Generate MIDI, then render to WAV
    let orpheus_to_wav_fn = lua.create_function(|lua, params: Table| -> mlua::Result<Table> {
        let globals = lua.globals();
        let hootenanny: Table = globals.get("hootenanny")
            .map_err(|_| mlua::Error::external("hootenanny namespace not available"))?;

        let soundfont_hash: String = params.get("soundfont_hash")
            .map_err(|_| mlua::Error::external("soundfont_hash required"))?;

        // Step 1: Generate MIDI
        let orpheus_generate: mlua::Function = hootenanny.get("orpheus_generate")
            .map_err(|_| mlua::Error::external("hootenanny.orpheus_generate not available"))?;

        let gen_result: Table = orpheus_generate.call(params.clone())?;
        let gen_job_id: String = gen_result.get("job_id")?;

        let timeout: i64 = params.get("timeout_ms").unwrap_or(120000);

        // Poll for MIDI generation
        let job_poll: mlua::Function = hootenanny.get("job_poll")
            .map_err(|_| mlua::Error::external("hootenanny.job_poll not available"))?;

        let poll_params = lua.create_table()?;
        let job_ids = lua.create_table()?;
        job_ids.set(1, gen_job_id.clone())?;
        poll_params.set("job_ids", job_ids)?;
        poll_params.set("timeout_ms", timeout)?;

        let poll_result: Table = job_poll.call(poll_params)?;

        let completed: Table = poll_result.get("completed")?;
        if completed.raw_len() == 0 {
            return Err(mlua::Error::external("MIDI generation timed out"));
        }

        // Get MIDI hash from job result
        let job_status: mlua::Function = hootenanny.get("job_status")
            .map_err(|_| mlua::Error::external("hootenanny.job_status not available"))?;

        let status_params = lua.create_table()?;
        status_params.set("job_id", gen_job_id)?;
        let gen_status: Table = job_status.call(status_params)?;

        let result: Table = gen_status.get("result")?;
        let output_hashes: Table = result.get("output_hashes")?;
        let midi_hash: String = output_hashes.get(1)?;

        // Step 2: Render to WAV
        let convert_midi: mlua::Function = hootenanny.get("convert_midi_to_wav")
            .map_err(|_| mlua::Error::external("hootenanny.convert_midi_to_wav not available"))?;

        let convert_params = lua.create_table()?;
        convert_params.set("input_hash", midi_hash.clone())?;
        convert_params.set("soundfont_hash", soundfont_hash)?;
        convert_params.set("sample_rate", 44100)?;

        let convert_result: Table = convert_midi.call(convert_params)?;
        let convert_job_id: String = convert_result.get("job_id")?;

        // Poll for WAV conversion
        let poll_params2 = lua.create_table()?;
        let job_ids2 = lua.create_table()?;
        job_ids2.set(1, convert_job_id.clone())?;
        poll_params2.set("job_ids", job_ids2)?;
        poll_params2.set("timeout_ms", timeout)?;

        let poll_result2: Table = job_poll.call(poll_params2)?;

        let completed2: Table = poll_result2.get("completed")?;
        if completed2.raw_len() == 0 {
            return Err(mlua::Error::external("WAV conversion timed out"));
        }

        // Get WAV result
        let status_params2 = lua.create_table()?;
        status_params2.set("job_id", convert_job_id)?;
        let wav_status: Table = job_status.call(status_params2)?;

        // Build combined result
        let final_result = lua.create_table()?;
        final_result.set("midi_hash", midi_hash)?;

        let wav_result: Table = wav_status.get("result")?;
        final_result.set("wav_artifact_id", wav_result.get::<LuaValue>("artifact_id")?)?;
        final_result.set("wav_hash", wav_result.get::<LuaValue>("content_hash")?)?;

        Ok(final_result)
    })?;
    workflow_table.set("orpheus_to_wav", orpheus_to_wav_fn)?;

    // workflow.abc_to_wav(abc_notation, soundfont_hash, params?) -> result
    // Parse ABC, convert to MIDI, render to WAV
    let abc_to_wav_fn = lua.create_function(|lua, args: (String, String, Option<Table>)| -> mlua::Result<Table> {
        let (notation, soundfont_hash, params) = args;

        let globals = lua.globals();
        let hootenanny: Table = globals.get("hootenanny")
            .map_err(|_| mlua::Error::external("hootenanny namespace not available"))?;

        // Step 1: Convert ABC to MIDI via hootenanny (creates artifact)
        let abc_to_midi: mlua::Function = hootenanny.get("abc_to_midi")
            .map_err(|_| mlua::Error::external("hootenanny.abc_to_midi not available"))?;

        let abc_params = lua.create_table()?;
        abc_params.set("abc", notation)?;
        if let Some(ref p) = params {
            if let Ok(v) = p.get::<i64>("velocity") {
                abc_params.set("velocity", v)?;
            }
            if let Ok(c) = p.get::<i64>("channel") {
                abc_params.set("channel", c)?;
            }
            if let Ok(t) = p.get::<i64>("tempo_override") {
                abc_params.set("tempo_override", t)?;
            }
        }

        let midi_result: Table = abc_to_midi.call(abc_params)?;
        let midi_hash: String = midi_result.get("content_hash")?;
        let midi_artifact_id: String = midi_result.get("artifact_id")?;

        // Step 2: Render to WAV
        let convert_midi: mlua::Function = hootenanny.get("convert_midi_to_wav")
            .map_err(|_| mlua::Error::external("hootenanny.convert_midi_to_wav not available"))?;

        let convert_params = lua.create_table()?;
        convert_params.set("input_hash", midi_hash.clone())?;
        convert_params.set("soundfont_hash", soundfont_hash)?;
        convert_params.set("sample_rate", 44100)?;

        let convert_result: Table = convert_midi.call(convert_params)?;
        let convert_job_id: String = convert_result.get("job_id")?;

        // Poll for completion
        let job_poll: mlua::Function = hootenanny.get("job_poll")
            .map_err(|_| mlua::Error::external("hootenanny.job_poll not available"))?;

        let timeout: i64 = params.as_ref()
            .and_then(|p| p.get::<i64>("timeout_ms").ok())
            .unwrap_or(60000);

        let poll_params = lua.create_table()?;
        let job_ids = lua.create_table()?;
        job_ids.set(1, convert_job_id.clone())?;
        poll_params.set("job_ids", job_ids)?;
        poll_params.set("timeout_ms", timeout)?;

        let poll_result: Table = job_poll.call(poll_params)?;

        let completed: Table = poll_result.get("completed")?;
        if completed.raw_len() == 0 {
            return Err(mlua::Error::external("WAV conversion timed out"));
        }

        // Get WAV result
        let job_status: mlua::Function = hootenanny.get("job_status")
            .map_err(|_| mlua::Error::external("hootenanny.job_status not available"))?;

        let status_params = lua.create_table()?;
        status_params.set("job_id", convert_job_id)?;
        let wav_status: Table = job_status.call(status_params)?;

        // Build result
        let final_result = lua.create_table()?;
        final_result.set("midi_artifact_id", midi_artifact_id)?;
        final_result.set("midi_hash", midi_hash)?;

        let wav_result: Table = wav_status.get("result")?;
        final_result.set("wav_artifact_id", wav_result.get::<LuaValue>("artifact_id")?)?;
        final_result.set("wav_hash", wav_result.get::<LuaValue>("content_hash")?)?;

        Ok(final_result)
    })?;
    workflow_table.set("abc_to_wav", abc_to_wav_fn)?;

    globals.set("workflow", workflow_table)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_workflow_globals() {
        let lua = Lua::new();
        // Note: will fail at runtime without hootenanny namespace
        register_workflow_globals(&lua).unwrap();

        let globals = lua.globals();
        let workflow: Table = globals.get("workflow").unwrap();
        assert!(workflow.contains_key("wait_job").unwrap());
        assert!(workflow.contains_key("orpheus_generate").unwrap());
        assert!(workflow.contains_key("orpheus_to_wav").unwrap());
        assert!(workflow.contains_key("abc_to_wav").unwrap());
    }
}
