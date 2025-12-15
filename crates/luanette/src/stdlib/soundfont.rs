//! SoundFont inspection for Lua scripts.
//!
//! Provides `soundfont.*` namespace with local SF2 inspection (no ZMQ overhead).
//! Uses rustysynth for parsing SoundFont files.
//!
//! # Usage
//!
//! ```lua
//! -- Inspect a SoundFont by CAS hash
//! local sf = soundfont.inspect("abc123...")
//! log.info("SoundFont: " .. sf.info.name)
//! log.info("Presets: " .. sf.info.preset_count)
//!
//! -- List all presets
//! for _, preset in ipairs(sf.presets) do
//!     log.info(string.format("Bank %d Prog %d: %s",
//!         preset.bank, preset.program, preset.name))
//! end
//!
//! -- Get drum mappings (bank 128)
//! local drums = soundfont.drums("abc123...")
//! for _, mapping in ipairs(drums) do
//!     log.info(mapping.preset_name)
//!     for _, region in ipairs(mapping.regions) do
//!         log.info(string.format("  %s: %s", region.key_range, region.instrument))
//!     end
//! end
//!
//! -- Inspect a specific preset
//! local preset = soundfont.preset("abc123...", 0, 0)  -- bank 0, program 0
//! log.info("Preset: " .. preset.name)
//! for _, region in ipairs(preset.regions) do
//!     log.info(string.format("  Keys %s: %s", region.keys, region.instrument))
//! end
//!
//! -- Also accepts file paths
//! local sf = soundfont.inspect("/path/to/soundfont.sf2")
//! ```

use anyhow::{Context, Result};
use cas::ContentStore;
use mlua::Lua;
use rustysynth::SoundFont;
use std::io::Cursor;

use super::cas::get_store;

/// Load SoundFont bytes from hash or path.
fn load_soundfont_bytes(hash_or_path: &str) -> Result<Vec<u8>> {
    // Try as CAS hash first (64 char hex)
    if hash_or_path.len() == 64 && hash_or_path.chars().all(|c| c.is_ascii_hexdigit()) {
        let store = get_store();
        let content_hash = hash_or_path.parse()
            .context("Invalid CAS hash")?;
        if let Some(path) = store.path(&content_hash) {
            return std::fs::read(&path)
                .with_context(|| format!("Failed to read SoundFont from CAS: {}", hash_or_path));
        }
    }

    // Try as file path
    std::fs::read(hash_or_path)
        .with_context(|| format!("Failed to read SoundFont: {}", hash_or_path))
}

/// Convert MIDI note number to note name.
fn midi_note_to_name(note: i32) -> String {
    const NOTE_NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
    let octave = (note / 12) - 1;
    let note_idx = (note % 12) as usize;
    format!("{}{}", NOTE_NAMES[note_idx], octave)
}

/// Register the `soundfont` global table.
pub fn register_soundfont_globals(lua: &Lua) -> Result<()> {
    let globals = lua.globals();
    let sf_table = lua.create_table()?;

    // soundfont.inspect(hash_or_path) -> table
    // Full inspection with info, presets, drum_mappings
    let inspect_fn = lua.create_function(|lua, hash_or_path: String| {
        let bytes = load_soundfont_bytes(&hash_or_path).map_err(mlua::Error::external)?;
        let mut cursor = Cursor::new(&bytes);
        let soundfont = SoundFont::new(&mut cursor)
            .map_err(|e| mlua::Error::external(format!("Failed to parse SoundFont: {:?}", e)))?;

        let result = lua.create_table()?;

        // Info section
        let info = lua.create_table()?;
        let sf_info = soundfont.get_info();
        info.set("name", sf_info.get_bank_name())?;
        info.set("preset_count", soundfont.get_presets().len())?;
        info.set("instrument_count", soundfont.get_instruments().len())?;
        info.set("sample_count", soundfont.get_sample_headers().len())?;
        result.set("info", info)?;

        // Presets
        let presets = soundfont.get_presets();
        let presets_table = lua.create_table()?;

        let mut preset_list: Vec<_> = presets.iter().collect();
        preset_list.sort_by(|a, b| {
            a.get_bank_number().cmp(&b.get_bank_number())
                .then(a.get_patch_number().cmp(&b.get_patch_number()))
        });

        for (idx, preset) in preset_list.iter().enumerate() {
            let p = lua.create_table()?;
            p.set("name", preset.get_name())?;
            p.set("bank", preset.get_bank_number())?;
            p.set("program", preset.get_patch_number())?;
            p.set("is_drum_kit", preset.get_bank_number() == 128)?;
            presets_table.set(idx + 1, p)?;
        }
        result.set("presets", presets_table)?;

        // Drum mappings for bank 128 presets
        let instruments = soundfont.get_instruments();
        let drum_mappings = lua.create_table()?;
        let mut drum_idx = 1;

        for preset in presets.iter() {
            if preset.get_bank_number() == 128 {
                let mapping = lua.create_table()?;
                mapping.set("preset_name", preset.get_name())?;
                mapping.set("bank", preset.get_bank_number())?;
                mapping.set("program", preset.get_patch_number())?;

                let regions_table = lua.create_table()?;
                let mut regions: Vec<_> = preset.get_regions().iter().collect();
                regions.sort_by_key(|r| r.get_key_range_start());

                for (region_idx, region) in regions.iter().enumerate() {
                    let key_lo = region.get_key_range_start();
                    let key_hi = region.get_key_range_end();
                    let inst_id = region.get_instrument_id();

                    let r = lua.create_table()?;
                    r.set("key_lo", key_lo)?;
                    r.set("key_hi", key_hi)?;

                    let key_range = if key_lo == key_hi {
                        midi_note_to_name(key_lo)
                    } else {
                        format!("{}-{}", midi_note_to_name(key_lo), midi_note_to_name(key_hi))
                    };
                    r.set("key_range", key_range)?;

                    let instrument_name = if inst_id < instruments.len() {
                        instruments[inst_id].get_name().to_string()
                    } else {
                        format!("Instrument {}", inst_id)
                    };
                    r.set("instrument", instrument_name)?;

                    regions_table.set(region_idx + 1, r)?;
                }

                mapping.set("regions", regions_table)?;
                drum_mappings.set(drum_idx, mapping)?;
                drum_idx += 1;
            }
        }
        result.set("drum_mappings", drum_mappings)?;

        Ok(result)
    })?;
    sf_table.set("inspect", inspect_fn)?;

    // soundfont.presets(hash_or_path) -> table of presets
    // Quick list of just presets
    let presets_fn = lua.create_function(|lua, hash_or_path: String| {
        let bytes = load_soundfont_bytes(&hash_or_path).map_err(mlua::Error::external)?;
        let mut cursor = Cursor::new(&bytes);
        let soundfont = SoundFont::new(&mut cursor)
            .map_err(|e| mlua::Error::external(format!("Failed to parse SoundFont: {:?}", e)))?;

        let presets = soundfont.get_presets();
        let result = lua.create_table()?;

        let mut preset_list: Vec<_> = presets.iter().collect();
        preset_list.sort_by(|a, b| {
            a.get_bank_number().cmp(&b.get_bank_number())
                .then(a.get_patch_number().cmp(&b.get_patch_number()))
        });

        for (idx, preset) in preset_list.iter().enumerate() {
            let p = lua.create_table()?;
            p.set("name", preset.get_name())?;
            p.set("bank", preset.get_bank_number())?;
            p.set("program", preset.get_patch_number())?;
            p.set("is_drum_kit", preset.get_bank_number() == 128)?;
            result.set(idx + 1, p)?;
        }

        Ok(result)
    })?;
    sf_table.set("presets", presets_fn)?;

    // soundfont.preset(hash_or_path, bank, program) -> preset detail
    // Detailed inspection of a specific preset
    let preset_fn = lua.create_function(|lua, (hash_or_path, bank, program): (String, i32, i32)| {
        let bytes = load_soundfont_bytes(&hash_or_path).map_err(mlua::Error::external)?;
        let mut cursor = Cursor::new(&bytes);
        let soundfont = SoundFont::new(&mut cursor)
            .map_err(|e| mlua::Error::external(format!("Failed to parse SoundFont: {:?}", e)))?;

        let presets = soundfont.get_presets();
        let instruments = soundfont.get_instruments();

        let preset = presets.iter()
            .find(|p| p.get_bank_number() == bank && p.get_patch_number() == program)
            .ok_or_else(|| mlua::Error::external(
                format!("Preset not found: bank {} program {}", bank, program)
            ))?;

        let result = lua.create_table()?;
        result.set("name", preset.get_name())?;
        result.set("bank", bank)?;
        result.set("program", program)?;

        let regions_table = lua.create_table()?;
        for (idx, region) in preset.get_regions().iter().enumerate() {
            let key_lo = region.get_key_range_start();
            let key_hi = region.get_key_range_end();
            let vel_lo = region.get_velocity_range_start();
            let vel_hi = region.get_velocity_range_end();
            let inst_id = region.get_instrument_id();

            let r = lua.create_table()?;

            let keys = if key_lo == key_hi {
                midi_note_to_name(key_lo)
            } else {
                format!("{}-{}", midi_note_to_name(key_lo), midi_note_to_name(key_hi))
            };
            r.set("keys", keys)?;

            if vel_lo != 0 || vel_hi != 127 {
                let velocity = if vel_lo == vel_hi {
                    format!("{}", vel_lo)
                } else {
                    format!("{}-{}", vel_lo, vel_hi)
                };
                r.set("velocity", velocity)?;
            }

            let instrument = if inst_id < instruments.len() {
                instruments[inst_id].get_name().to_string()
            } else {
                format!("#{}", inst_id)
            };
            r.set("instrument", instrument)?;

            regions_table.set(idx + 1, r)?;
        }
        result.set("regions", regions_table)?;

        Ok(result)
    })?;
    sf_table.set("preset", preset_fn)?;

    // soundfont.drums(hash_or_path) -> table of drum mappings
    // Quick access to bank 128 drum mappings
    let drums_fn = lua.create_function(|lua, hash_or_path: String| {
        let bytes = load_soundfont_bytes(&hash_or_path).map_err(mlua::Error::external)?;
        let mut cursor = Cursor::new(&bytes);
        let soundfont = SoundFont::new(&mut cursor)
            .map_err(|e| mlua::Error::external(format!("Failed to parse SoundFont: {:?}", e)))?;

        let presets = soundfont.get_presets();
        let instruments = soundfont.get_instruments();

        let result = lua.create_table()?;
        let mut idx = 1;

        for preset in presets.iter() {
            if preset.get_bank_number() == 128 {
                let mapping = lua.create_table()?;
                mapping.set("preset_name", preset.get_name())?;
                mapping.set("bank", 128)?;
                mapping.set("program", preset.get_patch_number())?;

                let regions_table = lua.create_table()?;
                let mut regions: Vec<_> = preset.get_regions().iter().collect();
                regions.sort_by_key(|r| r.get_key_range_start());

                for (region_idx, region) in regions.iter().enumerate() {
                    let key_lo = region.get_key_range_start();
                    let key_hi = region.get_key_range_end();
                    let inst_id = region.get_instrument_id();

                    let r = lua.create_table()?;
                    r.set("key_lo", key_lo)?;
                    r.set("key_hi", key_hi)?;

                    let key_range = if key_lo == key_hi {
                        midi_note_to_name(key_lo)
                    } else {
                        format!("{}-{}", midi_note_to_name(key_lo), midi_note_to_name(key_hi))
                    };
                    r.set("key_range", key_range)?;

                    let instrument = if inst_id < instruments.len() {
                        instruments[inst_id].get_name().to_string()
                    } else {
                        format!("Instrument {}", inst_id)
                    };
                    r.set("instrument", instrument)?;

                    regions_table.set(region_idx + 1, r)?;
                }

                mapping.set("regions", regions_table)?;
                result.set(idx, mapping)?;
                idx += 1;
            }
        }

        Ok(result)
    })?;
    sf_table.set("drums", drums_fn)?;

    // soundfont.info(hash_or_path) -> basic info only
    // Minimal inspection - just name and counts
    let info_fn = lua.create_function(|lua, hash_or_path: String| {
        let bytes = load_soundfont_bytes(&hash_or_path).map_err(mlua::Error::external)?;
        let mut cursor = Cursor::new(&bytes);
        let soundfont = SoundFont::new(&mut cursor)
            .map_err(|e| mlua::Error::external(format!("Failed to parse SoundFont: {:?}", e)))?;

        let sf_info = soundfont.get_info();
        let info = lua.create_table()?;
        info.set("name", sf_info.get_bank_name())?;
        info.set("preset_count", soundfont.get_presets().len())?;
        info.set("instrument_count", soundfont.get_instruments().len())?;
        info.set("sample_count", soundfont.get_sample_headers().len())?;

        Ok(info)
    })?;
    sf_table.set("info", info_fn)?;

    globals.set("soundfont", sf_table)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mlua::Table;

    #[test]
    fn test_register_soundfont_globals() {
        let lua = Lua::new();
        register_soundfont_globals(&lua).unwrap();

        let globals = lua.globals();
        let sf: Table = globals.get("soundfont").unwrap();
        assert!(sf.contains_key("inspect").unwrap());
        assert!(sf.contains_key("presets").unwrap());
        assert!(sf.contains_key("preset").unwrap());
        assert!(sf.contains_key("drums").unwrap());
        assert!(sf.contains_key("info").unwrap());
    }

    #[test]
    fn test_midi_note_to_name() {
        assert_eq!(midi_note_to_name(60), "C4");
        assert_eq!(midi_note_to_name(69), "A4");
        assert_eq!(midi_note_to_name(36), "C2");
        assert_eq!(midi_note_to_name(127), "G9");
    }
}
