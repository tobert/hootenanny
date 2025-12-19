//! Model-native vocabulary for Lua scripts.
//!
//! Provides the model-native API: `space()`, `context()`, `sample()`, `project()`,
//! `schedule()`, `extend()`, `bridge()`, `analyze()`.
//!
//! # Usage
//!
//! ```lua
//! -- Spaces as first-class values
//! local orpheus = space("orpheus")
//! local audio = space("audio")
//!
//! -- Inference context with composability
//! local ctx = context({
//!     temperature = 0.8,
//!     seed = 12345,
//!     max_tokens = 512,
//! })
//!
//! -- Sample from a space (returns encoding)
//! local midi = sample(orpheus, ctx)
//!
//! -- Project between spaces
//! local wav = project(midi, audio, { soundfont = "..." })
//!
//! -- Schedule on timeline
//! schedule(wav, { at = 0 })
//! ```

use anyhow::Result;
use mlua::{Lua, Table, UserData, UserDataMethods, Value as LuaValue};

/// Space - a generative domain
#[derive(Debug, Clone)]
pub struct Space {
    pub name: String,
}

impl UserData for Space {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method("__tostring", |_, this, ()| {
            Ok(format!("Space({})", this.name))
        });

        methods.add_method("name", |_, this, ()| Ok(this.name.clone()));

        methods.add_method("can_sample", |_, this, ()| {
            Ok(matches!(
                this.name.as_str(),
                "orpheus" | "orpheus_children" | "orpheus_mono_melodies"
                | "orpheus_loops" | "musicgen" | "yue"
            ))
        });

        methods.add_method("can_continue", |_, this, ()| {
            Ok(matches!(
                this.name.as_str(),
                "orpheus" | "orpheus_children" | "orpheus_mono_melodies" | "orpheus_loops"
            ))
        });

        methods.add_method("output_type", |_, this, ()| {
            Ok(match this.name.as_str() {
                "orpheus" | "orpheus_children" | "orpheus_mono_melodies"
                | "orpheus_loops" | "orpheus_bridge" => "midi",
                "musicgen" | "yue" => "audio",
                "abc" => "symbolic",
                _ => "unknown",
            })
        });
    }
}

/// InferenceContext - sampling parameters
#[derive(Debug, Clone, Default)]
pub struct InferenceContext {
    pub temperature: Option<f64>,
    pub top_p: Option<f64>,
    pub top_k: Option<u32>,
    pub seed: Option<u64>,
    pub max_tokens: Option<u32>,
    pub duration_seconds: Option<f64>,
    pub guidance_scale: Option<f64>,
    pub variant: Option<String>,
}

impl UserData for InferenceContext {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method("__tostring", |_, this, ()| {
            Ok(format!(
                "Context(temp={:?}, top_p={:?}, max_tokens={:?})",
                this.temperature, this.top_p, this.max_tokens
            ))
        });

        // ctx:with({ temperature = 1.2 }) -> new context with overrides
        methods.add_method("with", |lua, this, overrides: Table| {
            let mut new_ctx = this.clone();

            if let Ok(v) = overrides.get::<f64>("temperature") {
                new_ctx.temperature = Some(v);
            }
            if let Ok(v) = overrides.get::<f64>("top_p") {
                new_ctx.top_p = Some(v);
            }
            if let Ok(v) = overrides.get::<u32>("top_k") {
                new_ctx.top_k = Some(v);
            }
            if let Ok(v) = overrides.get::<u64>("seed") {
                new_ctx.seed = Some(v);
            }
            if let Ok(v) = overrides.get::<u32>("max_tokens") {
                new_ctx.max_tokens = Some(v);
            }
            if let Ok(v) = overrides.get::<f64>("duration_seconds") {
                new_ctx.duration_seconds = Some(v);
            }
            if let Ok(v) = overrides.get::<f64>("guidance_scale") {
                new_ctx.guidance_scale = Some(v);
            }
            if let Ok(v) = overrides.get::<String>("variant") {
                new_ctx.variant = Some(v);
            }

            Ok(new_ctx)
        });

        // ctx:to_table() -> Lua table for passing to tools
        methods.add_method("to_table", |lua, this, ()| {
            let t = lua.create_table()?;
            if let Some(v) = this.temperature {
                t.set("temperature", v)?;
            }
            if let Some(v) = this.top_p {
                t.set("top_p", v)?;
            }
            if let Some(v) = this.top_k {
                t.set("top_k", v)?;
            }
            if let Some(v) = this.seed {
                t.set("seed", v)?;
            }
            if let Some(v) = this.max_tokens {
                t.set("max_tokens", v)?;
            }
            if let Some(v) = this.duration_seconds {
                t.set("duration_seconds", v)?;
            }
            if let Some(v) = this.guidance_scale {
                t.set("guidance_scale", v)?;
            }
            if let Some(ref v) = this.variant {
                t.set("variant", v.clone())?;
            }
            Ok(t)
        });
    }
}

/// Encoding - a reference to content in a space
#[derive(Debug, Clone)]
pub struct Encoding {
    pub encoding_type: String, // "midi", "audio", "abc", "hash"
    pub artifact_id: Option<String>,
    pub content_hash: Option<String>,
    pub notation: Option<String>, // for ABC
    pub format: Option<String>,   // for hash
}

impl UserData for Encoding {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method("__tostring", |_, this, ()| {
            Ok(format!(
                "Encoding({}, artifact={:?})",
                this.encoding_type,
                this.artifact_id
            ))
        });

        methods.add_method("type", |_, this, ()| Ok(this.encoding_type.clone()));
        methods.add_method("artifact_id", |_, this, ()| Ok(this.artifact_id.clone()));
        methods.add_method("content_hash", |_, this, ()| Ok(this.content_hash.clone()));

        // enc:to_table() -> Lua table for passing to tools
        methods.add_method("to_table", |lua, this, ()| {
            let t = lua.create_table()?;
            t.set("type", this.encoding_type.clone())?;
            if let Some(ref v) = this.artifact_id {
                t.set("artifact_id", v.clone())?;
            }
            if let Some(ref v) = this.content_hash {
                t.set("content_hash", v.clone())?;
            }
            if let Some(ref v) = this.notation {
                t.set("notation", v.clone())?;
            }
            if let Some(ref v) = this.format {
                t.set("format", v.clone())?;
            }
            Ok(t)
        });
    }
}

/// Register the model-native globals.
pub fn register_native_globals(lua: &Lua) -> Result<()> {
    let globals = lua.globals();

    // space(name) -> Space userdata
    let space_fn = lua.create_function(|_, name: String| {
        let normalized = name.to_lowercase().replace('-', "_");
        Ok(Space { name: normalized })
    })?;
    globals.set("space", space_fn)?;

    // context(params?) -> InferenceContext userdata
    let context_fn = lua.create_function(|_, params: Option<Table>| {
        let mut ctx = InferenceContext::default();

        if let Some(p) = params {
            if let Ok(v) = p.get::<f64>("temperature") {
                ctx.temperature = Some(v);
            }
            if let Ok(v) = p.get::<f64>("top_p") {
                ctx.top_p = Some(v);
            }
            if let Ok(v) = p.get::<u32>("top_k") {
                ctx.top_k = Some(v);
            }
            if let Ok(v) = p.get::<u64>("seed") {
                ctx.seed = Some(v);
            }
            if let Ok(v) = p.get::<u32>("max_tokens") {
                ctx.max_tokens = Some(v);
            }
            if let Ok(v) = p.get::<f64>("duration_seconds") {
                ctx.duration_seconds = Some(v);
            }
            if let Ok(v) = p.get::<f64>("guidance_scale") {
                ctx.guidance_scale = Some(v);
            }
            if let Ok(v) = p.get::<String>("variant") {
                ctx.variant = Some(v);
            }
        }

        Ok(ctx)
    })?;
    globals.set("context", context_fn)?;

    // encoding(type, params) -> Encoding userdata
    // Helper for creating encodings
    let encoding_fn = lua.create_function(|_, (enc_type, params): (String, Table)| {
        let artifact_id: Option<String> = params.get("artifact_id").ok();
        let content_hash: Option<String> = params.get("content_hash").ok();
        let notation: Option<String> = params.get("notation").ok();
        let format: Option<String> = params.get("format").ok();

        Ok(Encoding {
            encoding_type: enc_type,
            artifact_id,
            content_hash,
            notation,
            format,
        })
    })?;
    globals.set("encoding", encoding_fn)?;

    Ok(())
}

/// Register functions that depend on hootenanny tools (sample, project, etc.)
pub fn register_native_tool_functions(lua: &Lua) -> Result<()> {
    let globals = lua.globals();

    // sample(space, ctx?, opts?) -> Encoding
    // Synchronously samples from a space and returns the encoding
    let sample_fn = lua.create_function(|lua, args: mlua::MultiValue| {
        let globals = lua.globals();
        let hootenanny: Table = globals.get("hootenanny")
            .map_err(|_| mlua::Error::external("hootenanny namespace not available"))?;

        let sample_tool: mlua::Function = hootenanny.get("sample")
            .map_err(|_| mlua::Error::external("hootenanny.sample not available"))?;
        let job_poll: mlua::Function = hootenanny.get("job_poll")
            .map_err(|_| mlua::Error::external("hootenanny.job_poll not available"))?;
        let job_status: mlua::Function = hootenanny.get("job_status")
            .map_err(|_| mlua::Error::external("hootenanny.job_status not available"))?;

        // Parse arguments
        let mut args_iter = args.into_iter();

        let space: Space = args_iter.next()
            .ok_or_else(|| mlua::Error::external("sample() requires a space argument"))?
            .as_userdata()
            .ok_or_else(|| mlua::Error::external("first argument must be a Space"))?
            .borrow::<Space>()?
            .clone();

        // Optional context
        let ctx: Option<InferenceContext> = args_iter.next()
            .and_then(|v| {
                if let Some(ud) = v.as_userdata() {
                    ud.borrow::<InferenceContext>().ok().map(|c| c.clone())
                } else {
                    None
                }
            });

        // Optional opts table
        let opts: Option<Table> = args_iter.next()
            .and_then(|v| v.as_table().cloned());

        // Build request
        let params = lua.create_table()?;
        params.set("space", space.name.clone())?;

        // Build inference table
        let inference = lua.create_table()?;
        if let Some(ref c) = ctx {
            if let Some(v) = c.temperature {
                inference.set("temperature", v)?;
            }
            if let Some(v) = c.top_p {
                inference.set("top_p", v)?;
            }
            if let Some(v) = c.top_k {
                inference.set("top_k", v)?;
            }
            if let Some(v) = c.seed {
                inference.set("seed", v)?;
            }
            if let Some(v) = c.max_tokens {
                inference.set("max_tokens", v)?;
            }
            if let Some(v) = c.duration_seconds {
                inference.set("duration_seconds", v)?;
            }
            if let Some(v) = c.guidance_scale {
                inference.set("guidance_scale", v)?;
            }
        }
        params.set("inference", inference)?;

        // Copy opts
        if let Some(ref o) = opts {
            if let Ok(v) = o.get::<u32>("num_variations") {
                params.set("num_variations", v)?;
            }
            if let Ok(v) = o.get::<String>("prompt") {
                params.set("prompt", v)?;
            }
            if let Ok(v) = o.get::<Table>("tags") {
                params.set("tags", v)?;
            }
            if let Ok(v) = o.get::<String>("creator") {
                params.set("creator", v)?;
            }
        }

        params.set("creator", "lua")?;

        // Call sample tool
        let result: Table = sample_tool.call(params)?;
        let job_id: String = result.get("job_id")?;

        // Poll for completion
        let poll_params = lua.create_table()?;
        let job_ids = lua.create_table()?;
        job_ids.set(1, job_id.clone())?;
        poll_params.set("job_ids", job_ids)?;
        poll_params.set("timeout_ms", 120000)?;

        let poll_result: Table = job_poll.call(poll_params)?;
        let completed: Table = poll_result.get("completed")?;

        if completed.raw_len() == 0 {
            return Err(mlua::Error::external("sample() timed out"));
        }

        // Get job result
        let status_params = lua.create_table()?;
        status_params.set("job_id", job_id)?;
        let status: Table = job_status.call(status_params)?;
        let job_result: Table = status.get("result")?;

        let artifact_ids: Table = job_result.get("artifact_ids")?;
        let artifact_id: String = artifact_ids.get(1)?;

        let output_hashes: Table = job_result.get("output_hashes")?;
        let content_hash: String = output_hashes.get(1)?;

        // Return encoding
        let output_type = match space.name.as_str() {
            "orpheus" | "orpheus_children" | "orpheus_mono_melodies"
            | "orpheus_loops" | "orpheus_bridge" => "midi",
            "musicgen" | "yue" => "audio",
            _ => "midi",
        };

        Ok(Encoding {
            encoding_type: output_type.to_string(),
            artifact_id: Some(artifact_id),
            content_hash: Some(content_hash),
            notation: None,
            format: None,
        })
    })?;
    globals.set("sample", sample_fn)?;

    // project(encoding, target_space, opts?) -> Encoding
    let project_fn = lua.create_function(|lua, args: mlua::MultiValue| {
        let globals = lua.globals();
        let hootenanny: Table = globals.get("hootenanny")
            .map_err(|_| mlua::Error::external("hootenanny namespace not available"))?;

        let project_tool: mlua::Function = hootenanny.get("project")
            .map_err(|_| mlua::Error::external("hootenanny.project not available"))?;
        let job_poll: mlua::Function = hootenanny.get("job_poll")
            .map_err(|_| mlua::Error::external("hootenanny.job_poll not available"))?;
        let job_status: mlua::Function = hootenanny.get("job_status")
            .map_err(|_| mlua::Error::external("hootenanny.job_status not available"))?;

        let mut args_iter = args.into_iter();

        // First arg: encoding
        let enc: Encoding = args_iter.next()
            .ok_or_else(|| mlua::Error::external("project() requires an encoding argument"))?
            .as_userdata()
            .ok_or_else(|| mlua::Error::external("first argument must be an Encoding"))?
            .borrow::<Encoding>()?
            .clone();

        // Second arg: target space
        let target_space: Space = args_iter.next()
            .ok_or_else(|| mlua::Error::external("project() requires a target space"))?
            .as_userdata()
            .ok_or_else(|| mlua::Error::external("second argument must be a Space"))?
            .borrow::<Space>()?
            .clone();

        // Optional opts
        let opts: Option<Table> = args_iter.next()
            .and_then(|v| v.as_table().cloned());

        // Build request
        let params = lua.create_table()?;

        // Build encoding table
        let enc_table = lua.create_table()?;
        enc_table.set("type", enc.encoding_type.clone())?;
        if let Some(ref aid) = enc.artifact_id {
            enc_table.set("artifact_id", aid.clone())?;
        }
        params.set("encoding", enc_table)?;

        // Build target table based on space
        let target = lua.create_table()?;
        match target_space.name.as_str() {
            "audio" => {
                target.set("type", "audio")?;
                if let Some(ref o) = opts {
                    if let Ok(v) = o.get::<String>("soundfont") {
                        target.set("soundfont_hash", v)?;
                    }
                    if let Ok(v) = o.get::<String>("soundfont_hash") {
                        target.set("soundfont_hash", v)?;
                    }
                    if let Ok(v) = o.get::<u32>("sample_rate") {
                        target.set("sample_rate", v)?;
                    }
                }
            }
            "midi" => {
                target.set("type", "midi")?;
                if let Some(ref o) = opts {
                    if let Ok(v) = o.get::<u8>("channel") {
                        target.set("channel", v)?;
                    }
                    if let Ok(v) = o.get::<u8>("velocity") {
                        target.set("velocity", v)?;
                    }
                }
            }
            _ => {
                return Err(mlua::Error::external(format!(
                    "Cannot project to space: {}", target_space.name
                )));
            }
        }
        params.set("target", target)?;
        params.set("creator", "lua")?;

        // Call project tool
        let result: Table = project_tool.call(params)?;
        let job_id: String = result.get("job_id")?;

        // Poll for completion
        let poll_params = lua.create_table()?;
        let job_ids = lua.create_table()?;
        job_ids.set(1, job_id.clone())?;
        poll_params.set("job_ids", job_ids)?;
        poll_params.set("timeout_ms", 120000)?;

        let poll_result: Table = job_poll.call(poll_params)?;
        let completed: Table = poll_result.get("completed")?;

        if completed.raw_len() == 0 {
            return Err(mlua::Error::external("project() timed out"));
        }

        // Get job result
        let status_params = lua.create_table()?;
        status_params.set("job_id", job_id)?;
        let status: Table = job_status.call(status_params)?;
        let job_result: Table = status.get("result")?;

        let artifact_id: String = job_result.get("artifact_id")?;
        let content_hash: String = job_result.get("content_hash")?;

        Ok(Encoding {
            encoding_type: target_space.name.clone(),
            artifact_id: Some(artifact_id),
            content_hash: Some(content_hash),
            notation: None,
            format: None,
        })
    })?;
    globals.set("project", project_fn)?;

    // schedule(encoding, opts) -> region_id
    let schedule_fn = lua.create_function(|lua, (enc, opts): (mlua::AnyUserData, Table)| {
        let globals = lua.globals();
        let hootenanny: Table = globals.get("hootenanny")
            .map_err(|_| mlua::Error::external("hootenanny namespace not available"))?;

        let schedule_tool: mlua::Function = hootenanny.get("schedule")
            .map_err(|_| mlua::Error::external("hootenanny.schedule not available"))?;

        let enc: Encoding = enc.borrow::<Encoding>()?.clone();

        let params = lua.create_table()?;

        // Build encoding table
        let enc_table = lua.create_table()?;
        enc_table.set("type", enc.encoding_type.clone())?;
        if let Some(ref aid) = enc.artifact_id {
            enc_table.set("artifact_id", aid.clone())?;
        }
        params.set("encoding", enc_table)?;

        // Position and duration
        let at: f64 = opts.get("at").unwrap_or(0.0);
        params.set("at", at)?;

        if let Ok(duration) = opts.get::<f64>("duration") {
            params.set("duration", duration)?;
        }
        if let Ok(gain) = opts.get::<f64>("gain") {
            params.set("gain", gain)?;
        }
        if let Ok(rate) = opts.get::<f64>("rate") {
            params.set("rate", rate)?;
        }

        let result: Table = schedule_tool.call(params)?;

        // Parse the text field which contains JSON
        let text: String = result.get("text")?;
        let parsed: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| mlua::Error::external(format!("Failed to parse schedule result: {}", e)))?;

        let region_id = parsed["region_id"].as_str()
            .ok_or_else(|| mlua::Error::external("No region_id in response"))?;

        Ok(region_id.to_string())
    })?;
    globals.set("schedule", schedule_fn)?;

    // extend(encoding, ctx?, opts?) -> Encoding
    let extend_fn = lua.create_function(|lua, args: mlua::MultiValue| {
        let globals = lua.globals();
        let hootenanny: Table = globals.get("hootenanny")
            .map_err(|_| mlua::Error::external("hootenanny namespace not available"))?;

        let extend_tool: mlua::Function = hootenanny.get("extend")
            .map_err(|_| mlua::Error::external("hootenanny.extend not available"))?;
        let job_poll: mlua::Function = hootenanny.get("job_poll")
            .map_err(|_| mlua::Error::external("hootenanny.job_poll not available"))?;
        let job_status: mlua::Function = hootenanny.get("job_status")
            .map_err(|_| mlua::Error::external("hootenanny.job_status not available"))?;

        let mut args_iter = args.into_iter();

        let enc: Encoding = args_iter.next()
            .ok_or_else(|| mlua::Error::external("extend() requires an encoding argument"))?
            .as_userdata()
            .ok_or_else(|| mlua::Error::external("first argument must be an Encoding"))?
            .borrow::<Encoding>()?
            .clone();

        let ctx: Option<InferenceContext> = args_iter.next()
            .and_then(|v| {
                if let Some(ud) = v.as_userdata() {
                    ud.borrow::<InferenceContext>().ok().map(|c| c.clone())
                } else {
                    None
                }
            });

        let params = lua.create_table()?;

        // Build encoding table
        let enc_table = lua.create_table()?;
        enc_table.set("type", enc.encoding_type.clone())?;
        if let Some(ref aid) = enc.artifact_id {
            enc_table.set("artifact_id", aid.clone())?;
        }
        params.set("encoding", enc_table)?;

        // Build inference table
        let inference = lua.create_table()?;
        if let Some(ref c) = ctx {
            if let Some(v) = c.temperature {
                inference.set("temperature", v)?;
            }
            if let Some(v) = c.top_p {
                inference.set("top_p", v)?;
            }
            if let Some(v) = c.max_tokens {
                inference.set("max_tokens", v)?;
            }
        }
        params.set("inference", inference)?;
        params.set("creator", "lua")?;

        let result: Table = extend_tool.call(params)?;
        let job_id: String = result.get("job_id")?;

        // Poll for completion
        let poll_params = lua.create_table()?;
        let job_ids = lua.create_table()?;
        job_ids.set(1, job_id.clone())?;
        poll_params.set("job_ids", job_ids)?;
        poll_params.set("timeout_ms", 120000)?;

        let poll_result: Table = job_poll.call(poll_params)?;
        let completed: Table = poll_result.get("completed")?;

        if completed.raw_len() == 0 {
            return Err(mlua::Error::external("extend() timed out"));
        }

        // Get job result
        let status_params = lua.create_table()?;
        status_params.set("job_id", job_id)?;
        let status: Table = job_status.call(status_params)?;
        let job_result: Table = status.get("result")?;

        let artifact_ids: Table = job_result.get("artifact_ids")?;
        let artifact_id: String = artifact_ids.get(1)?;

        let output_hashes: Table = job_result.get("output_hashes")?;
        let content_hash: String = output_hashes.get(1)?;

        Ok(Encoding {
            encoding_type: enc.encoding_type,
            artifact_id: Some(artifact_id),
            content_hash: Some(content_hash),
            notation: None,
            format: None,
        })
    })?;
    globals.set("extend", extend_fn)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_space_creation() {
        let lua = Lua::new();
        register_native_globals(&lua).unwrap();

        lua.load(r#"
            local orpheus = space("orpheus")
            assert(orpheus:name() == "orpheus", "name mismatch")
            assert(orpheus:can_sample() == true, "should be able to sample")
            assert(orpheus:output_type() == "midi", "should output midi")

            local musicgen = space("musicgen")
            assert(musicgen:can_sample() == true, "musicgen should sample")
            assert(musicgen:output_type() == "audio", "musicgen outputs audio")

            local abc = space("abc")
            assert(abc:can_sample() == false, "abc cannot sample")
            assert(abc:output_type() == "symbolic", "abc is symbolic")
        "#).exec().unwrap();
    }

    #[test]
    fn test_context_creation() {
        let lua = Lua::new();
        register_native_globals(&lua).unwrap();

        lua.load(r#"
            local ctx = context({
                temperature = 0.8,
                max_tokens = 512,
                seed = 12345,
            })

            -- Test with() method
            local ctx2 = ctx:with({ temperature = 1.2 })

            -- Test to_table
            local t = ctx:to_table()
            assert(t.temperature == 0.8, "temperature mismatch")
            assert(t.max_tokens == 512, "max_tokens mismatch")
        "#).exec().unwrap();
    }

    #[test]
    fn test_encoding_creation() {
        let lua = Lua::new();
        register_native_globals(&lua).unwrap();

        lua.load(r#"
            local enc = encoding("midi", { artifact_id = "artifact_123" })
            assert(enc:type() == "midi", "type mismatch")
            assert(enc:artifact_id() == "artifact_123", "artifact_id mismatch")

            local enc2 = encoding("audio", {
                artifact_id = "artifact_456",
                content_hash = "hash_789"
            })
            assert(enc2:type() == "audio", "type mismatch")
            assert(enc2:content_hash() == "hash_789", "hash mismatch")
        "#).exec().unwrap();
    }

    #[test]
    fn test_space_normalization() {
        let lua = Lua::new();
        register_native_globals(&lua).unwrap();

        lua.load(r#"
            -- Test that hyphens are normalized to underscores
            local s1 = space("orpheus-children")
            assert(s1:name() == "orpheus_children", "should normalize hyphens")

            -- Test uppercase normalization
            local s2 = space("MUSICGEN")
            assert(s2:name() == "musicgen", "should lowercase")
        "#).exec().unwrap();
    }
}
