//! ABC notation parsing for Lua scripts.
//!
//! Provides `abc.*` namespace with local ABC parsing (no ZMQ overhead).
//! Uses the abc crate directly for fast parsing and validation.
//!
//! # Usage
//!
//! ```lua
//! -- Parse ABC notation
//! local result = abc.parse([[
//! X:1
//! T:My Tune
//! M:4/4
//! L:1/8
//! K:G
//! GABc dedB|
//! ]])
//!
//! if result.valid then
//!     log.info("Title: " .. result.title)
//!     log.info("Key: " .. result.key)
//! else
//!     for _, err in ipairs(result.errors) do
//!         log.error(err)
//!     end
//! end
//!
//! -- Validate only
//! local valid, errors = abc.validate("X:1\nK:C\nCDEF|")
//!
//! -- Convert to MIDI bytes (returns binary string)
//! local midi_bytes = abc.to_midi("X:1\nK:C\nCDEF|", {
//!     velocity = 100,
//!     channel = 0
//! })
//! ```

use abc::{parse as abc_parse, to_midi, Element, MidiParams};
use anyhow::Result;
use mlua::{Lua, Table};

/// Register the `abc` global table.
pub fn register_abc_globals(lua: &Lua) -> Result<()> {
    let globals = lua.globals();
    let abc_table = lua.create_table()?;

    // abc.parse(notation) -> table
    // Parse ABC notation and return structured result
    let parse_fn = lua.create_function(|lua, notation: String| {
        let result = abc_parse(&notation);

        let table = lua.create_table()?;

        // Basic validity
        table.set("valid", !result.has_errors())?;

        // Collect errors
        let errors = lua.create_table()?;
        let mut err_idx = 1;
        for fb in result.feedback.iter() {
            if fb.level == abc::FeedbackLevel::Error {
                errors.set(err_idx, fb.message.clone())?;
                err_idx += 1;
            }
        }
        table.set("errors", errors)?;

        // Collect warnings
        let warnings = lua.create_table()?;
        let mut warn_idx = 1;
        for fb in result.feedback.iter() {
            if fb.level == abc::FeedbackLevel::Warning {
                warnings.set(warn_idx, fb.message.clone())?;
                warn_idx += 1;
            }
        }
        table.set("warnings", warnings)?;

        // Extract tune metadata
        let tune = &result.value;

        // Title
        table.set("title", tune.header.title.clone())?;

        // Composer
        if let Some(composer) = &tune.header.composer {
            table.set("composer", composer.clone())?;
        }

        // Key signature
        let key_str = format!("{:?}{:?}",
            tune.header.key.root,
            tune.header.key.mode
        );
        table.set("key", key_str)?;

        // Meter
        if let Some(meter) = &tune.header.meter {
            let meter_str = match meter {
                abc::Meter::Simple { numerator, denominator } => {
                    format!("{}/{}", numerator, denominator)
                }
                abc::Meter::Common => "C".to_string(),
                abc::Meter::Cut => "C|".to_string(),
                abc::Meter::None => "none".to_string(),
            };
            table.set("meter", meter_str)?;
        }

        // Tempo
        if let Some(tempo) = &tune.header.tempo {
            table.set("tempo", tempo.bpm)?;
        }

        // Count notes across all voices
        let mut note_count = 0usize;
        let mut bar_count = 0usize;
        for voice in &tune.voices {
            for element in &voice.elements {
                match element {
                    Element::Note(_) => note_count += 1,
                    Element::Chord(chord) => note_count += chord.notes.len(),
                    Element::Bar(_) => bar_count += 1,
                    _ => {}
                }
            }
        }
        table.set("note_count", note_count)?;
        table.set("bar_count", bar_count)?;

        Ok(table)
    })?;
    abc_table.set("parse", parse_fn)?;

    // abc.validate(notation) -> valid, errors
    // Quick validation
    let validate_fn = lua.create_function(|lua, notation: String| {
        let result = abc_parse(&notation);
        let valid = !result.has_errors();

        let errors = lua.create_table()?;
        let mut idx = 1;
        for fb in result.feedback.iter() {
            if fb.level == abc::FeedbackLevel::Error {
                errors.set(idx, fb.message.clone())?;
                idx += 1;
            }
        }

        Ok((valid, errors))
    })?;
    abc_table.set("validate", validate_fn)?;

    // abc.to_midi(notation, params?) -> string (midi bytes)
    // Convert ABC to MIDI bytes
    let to_midi_fn = lua.create_function(|lua, args: (String, Option<Table>)| {
        let (notation, params_table) = args;

        let result = abc_parse(&notation);

        if result.has_errors() {
            let err_msgs: Vec<String> = result.feedback.iter()
                .filter(|fb| fb.level == abc::FeedbackLevel::Error)
                .map(|fb| fb.message.clone())
                .collect();
            return Err(mlua::Error::external(format!(
                "ABC parse errors: {}", err_msgs.join("; ")
            )));
        }

        let tune = &result.value;

        // Build MidiParams from Lua table
        let mut params = MidiParams::default();
        if let Some(t) = params_table {
            if let Ok(v) = t.get::<u8>("velocity") {
                params.velocity = v;
            }
            if let Ok(c) = t.get::<u8>("channel") {
                params.channel = c;
            }
            if let Ok(tpb) = t.get::<u16>("ticks_per_beat") {
                params.ticks_per_beat = tpb;
            }
        }

        let midi_bytes = to_midi(tune, &params);

        // Return as Lua string (binary safe)
        Ok(lua.create_string(&midi_bytes)?)
    })?;
    abc_table.set("to_midi", to_midi_fn)?;

    // abc.to_midi_file(notation, path, params?) -> boolean
    // Convert ABC to MIDI and write to file
    let to_midi_file_fn = lua.create_function(|_, args: (String, String, Option<Table>)| {
        let (notation, path, params_table) = args;

        let result = abc_parse(&notation);

        if result.has_errors() {
            let err_msgs: Vec<String> = result.feedback.iter()
                .filter(|fb| fb.level == abc::FeedbackLevel::Error)
                .map(|fb| fb.message.clone())
                .collect();
            return Err(mlua::Error::external(format!(
                "ABC parse errors: {}", err_msgs.join("; ")
            )));
        }

        let tune = &result.value;

        let mut params = MidiParams::default();
        if let Some(t) = params_table {
            if let Ok(v) = t.get::<u8>("velocity") {
                params.velocity = v;
            }
            if let Ok(c) = t.get::<u8>("channel") {
                params.channel = c;
            }
            if let Ok(tpb) = t.get::<u16>("ticks_per_beat") {
                params.ticks_per_beat = tpb;
            }
        }

        let midi_bytes = to_midi(tune, &params);

        std::fs::write(&path, &midi_bytes)
            .map_err(|e| mlua::Error::external(format!("Failed to write MIDI: {}", e)))?;

        Ok(true)
    })?;
    abc_table.set("to_midi_file", to_midi_file_fn)?;

    // abc.transpose(notation, semitones_or_target_key) -> string
    // Transpose ABC notation
    let transpose_fn = lua.create_function(|_, args: (String, mlua::Value)| {
        let (notation, amount) = args;

        let result = abc_parse(&notation);
        if result.has_errors() {
            let err_msgs: Vec<String> = result.feedback.iter()
                .filter(|fb| fb.level == abc::FeedbackLevel::Error)
                .map(|fb| fb.message.clone())
                .collect();
            return Err(mlua::Error::external(format!(
                "ABC parse errors: {}", err_msgs.join("; ")
            )));
        }

        let tune = &result.value;

        let semitones: i8 = match amount {
            mlua::Value::Integer(i) => i as i8,
            mlua::Value::Number(n) => n as i8,
            mlua::Value::String(s) => {
                // Target key like "Am" or "Bb"
                let target = s.to_str().map_err(mlua::Error::external)?;
                abc::semitones_to_key(&tune.header.key, &target)
                    .map_err(mlua::Error::external)?
            }
            _ => return Err(mlua::Error::external("Expected number or key string")),
        };

        let transposed = abc::transpose(tune, semitones);
        let abc_string = abc::to_abc(&transposed);

        Ok(abc_string)
    })?;
    abc_table.set("transpose", transpose_fn)?;

    globals.set("abc", abc_table)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_abc_globals() {
        let lua = Lua::new();
        register_abc_globals(&lua).unwrap();

        let globals = lua.globals();
        let abc: Table = globals.get("abc").unwrap();
        assert!(abc.contains_key("parse").unwrap());
        assert!(abc.contains_key("validate").unwrap());
        assert!(abc.contains_key("to_midi").unwrap());
        assert!(abc.contains_key("to_midi_file").unwrap());
        assert!(abc.contains_key("transpose").unwrap());
    }

    #[test]
    fn test_abc_parse() {
        let lua = Lua::new();
        register_abc_globals(&lua).unwrap();

        let code = r#"
            local result = abc.parse([[
X:1
T:Test
M:4/4
K:C
CDEF|
]])
            return result.valid, result.title, result.note_count
        "#;

        let (valid, title, notes): (bool, String, i64) = lua.load(code).eval().unwrap();
        assert!(valid);
        assert_eq!(title, "Test");
        assert_eq!(notes, 4);
    }
}
