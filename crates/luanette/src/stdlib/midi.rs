//! MIDI manipulation functions for Lua scripts.
//!
//! Provides `midi.*` namespace with functions for reading, writing, and
//! transforming MIDI data.
//!
//! # Lua Table Structure
//!
//! ```lua
//! {
//!   format = 1,           -- MIDI format (0, 1, or 2)
//!   ticks_per_beat = 480, -- Resolution
//!   tracks = {
//!     {
//!       events = {
//!         { type = "note_on", channel = 0, note = 60, velocity = 100, delta = 0 },
//!         { type = "note_off", channel = 0, note = 60, velocity = 0, delta = 480 },
//!         { type = "control_change", channel = 0, controller = 7, value = 100, delta = 0 },
//!         { type = "program_change", channel = 0, program = 0, delta = 0 },
//!         { type = "meta", meta_type = "tempo", tempo = 500000, delta = 0 },
//!         { type = "meta", meta_type = "time_signature", numerator = 4, denominator = 4, delta = 0 },
//!         ...
//!       }
//!     },
//!     ...
//!   }
//! }
//! ```

use anyhow::{Context, Result};
use midly::{Format, Header, MidiMessage, Smf, Timing, Track, TrackEvent, TrackEventKind};
use mlua::{Lua, Table, Value as LuaValue};
use std::fs;
use std::path::Path;

/// Register the `midi` global table.
pub fn register_midi_globals(lua: &Lua) -> Result<()> {
    let globals = lua.globals();
    let midi_table = lua.create_table()?;

    // midi.read(filepath) -> table
    let read_fn = lua.create_function(|lua, path: String| {
        read_midi_file(lua, &path).map_err(mlua::Error::external)
    })?;
    midi_table.set("read", read_fn)?;

    // midi.write(filepath, table)
    let write_fn = lua.create_function(|_, (path, table): (String, Table)| {
        write_midi_file(&path, table).map_err(mlua::Error::external)
    })?;
    midi_table.set("write", write_fn)?;

    // midi.transpose(events, semitones) -> modifies in place
    let transpose_fn = lua.create_function(|_, (events, semitones): (Table, i32)| {
        transpose_events(events, semitones).map_err(mlua::Error::external)
    })?;
    midi_table.set("transpose", transpose_fn)?;

    // midi.quantize(events, grid_ticks) -> modifies in place
    let quantize_fn = lua.create_function(|_, (events, grid): (Table, u32)| {
        quantize_events(events, grid).map_err(mlua::Error::external)
    })?;
    midi_table.set("quantize", quantize_fn)?;

    // midi.merge(tracks) -> merged track table
    let merge_fn = lua.create_function(|lua, tracks: Table| {
        merge_tracks(lua, tracks).map_err(mlua::Error::external)
    })?;
    midi_table.set("merge", merge_fn)?;

    // midi.filter(events, predicate_fn) -> filtered events
    let filter_fn = lua.create_function(|lua, (events, predicate): (Table, mlua::Function)| {
        filter_events(lua, events, predicate).map_err(mlua::Error::external)
    })?;
    midi_table.set("filter", filter_fn)?;

    globals.set("midi", midi_table)?;
    Ok(())
}

/// Read a MIDI file and convert to Lua table.
fn read_midi_file(lua: &Lua, path: &str) -> Result<Table> {
    let data = fs::read(path).context("Failed to read MIDI file")?;
    let smf = Smf::parse(&data).context("Failed to parse MIDI file")?;

    let result = lua.create_table()?;

    // Format
    let format = match smf.header.format {
        Format::SingleTrack => 0,
        Format::Parallel => 1,
        Format::Sequential => 2,
    };
    result.set("format", format)?;

    // Timing
    let ticks_per_beat = match smf.header.timing {
        Timing::Metrical(tpb) => tpb.as_int(),
        Timing::Timecode(fps, tpf) => (fps.as_int() as u16) * (tpf as u16),
    };
    result.set("ticks_per_beat", ticks_per_beat)?;

    // Tracks
    let tracks_table = lua.create_table()?;
    for (track_idx, track) in smf.tracks.iter().enumerate() {
        let track_table = lua.create_table()?;
        let events_table = lua.create_table()?;

        for (event_idx, event) in track.iter().enumerate() {
            if let Some(event_table) = track_event_to_lua(lua, event)? {
                events_table.set(event_idx + 1, event_table)?;
            }
        }

        track_table.set("events", events_table)?;
        tracks_table.set(track_idx + 1, track_table)?;
    }
    result.set("tracks", tracks_table)?;

    Ok(result)
}

/// Convert a track event to a Lua table.
fn track_event_to_lua(lua: &Lua, event: &TrackEvent) -> Result<Option<Table>> {
    let table = lua.create_table()?;
    table.set("delta", event.delta.as_int())?;

    match event.kind {
        TrackEventKind::Midi { channel, message } => {
            let ch = channel.as_int();
            match message {
                MidiMessage::NoteOn { key, vel } => {
                    table.set("type", "note_on")?;
                    table.set("channel", ch)?;
                    table.set("note", key.as_int())?;
                    table.set("velocity", vel.as_int())?;
                }
                MidiMessage::NoteOff { key, vel } => {
                    table.set("type", "note_off")?;
                    table.set("channel", ch)?;
                    table.set("note", key.as_int())?;
                    table.set("velocity", vel.as_int())?;
                }
                MidiMessage::Aftertouch { key, vel } => {
                    table.set("type", "aftertouch")?;
                    table.set("channel", ch)?;
                    table.set("note", key.as_int())?;
                    table.set("pressure", vel.as_int())?;
                }
                MidiMessage::Controller { controller, value } => {
                    table.set("type", "control_change")?;
                    table.set("channel", ch)?;
                    table.set("controller", controller.as_int())?;
                    table.set("value", value.as_int())?;
                }
                MidiMessage::ProgramChange { program } => {
                    table.set("type", "program_change")?;
                    table.set("channel", ch)?;
                    table.set("program", program.as_int())?;
                }
                MidiMessage::ChannelAftertouch { vel } => {
                    table.set("type", "channel_aftertouch")?;
                    table.set("channel", ch)?;
                    table.set("pressure", vel.as_int())?;
                }
                MidiMessage::PitchBend { bend } => {
                    table.set("type", "pitch_bend")?;
                    table.set("channel", ch)?;
                    table.set("value", bend.as_int())?;
                }
            }
        }
        TrackEventKind::Meta(meta) => {
            table.set("type", "meta")?;
            match meta {
                midly::MetaMessage::Tempo(tempo) => {
                    table.set("meta_type", "tempo")?;
                    table.set("tempo", tempo.as_int())?;
                }
                midly::MetaMessage::TimeSignature(num, denom, _, _) => {
                    table.set("meta_type", "time_signature")?;
                    table.set("numerator", num)?;
                    table.set("denominator", 1 << denom)?;
                }
                midly::MetaMessage::KeySignature(sf, minor) => {
                    table.set("meta_type", "key_signature")?;
                    table.set("sharps_flats", sf)?;
                    table.set("minor", minor)?;
                }
                midly::MetaMessage::TrackName(name) => {
                    table.set("meta_type", "track_name")?;
                    table.set("name", String::from_utf8_lossy(name).to_string())?;
                }
                midly::MetaMessage::Text(text) => {
                    table.set("meta_type", "text")?;
                    table.set("text", String::from_utf8_lossy(text).to_string())?;
                }
                midly::MetaMessage::EndOfTrack => {
                    table.set("meta_type", "end_of_track")?;
                }
                _ => {
                    table.set("meta_type", "unknown")?;
                }
            }
        }
        TrackEventKind::SysEx(_) => {
            table.set("type", "sysex")?;
        }
        TrackEventKind::Escape(_) => {
            return Ok(None); // Skip escape sequences
        }
    }

    Ok(Some(table))
}

/// Write a Lua table to a MIDI file.
fn write_midi_file(path: &str, table: Table) -> Result<()> {
    let format: u8 = table.get("format").unwrap_or(1);
    let ticks_per_beat: u16 = table.get("ticks_per_beat").unwrap_or(480);

    let format = match format {
        0 => Format::SingleTrack,
        1 => Format::Parallel,
        2 => Format::Sequential,
        _ => Format::Parallel,
    };

    let header = Header::new(format, Timing::Metrical(ticks_per_beat.into()));

    let tracks_table: Table = table.get("tracks")?;
    let mut tracks: Vec<Track> = Vec::new();

    for pair in tracks_table.pairs::<i64, Table>() {
        let (_, track_table) = pair?;
        let events_table: Table = track_table.get("events")?;
        let mut track_events: Vec<TrackEvent<'static>> = Vec::new();

        for pair in events_table.pairs::<i64, Table>() {
            let (_, event_table) = pair?;
            if let Some(event) = lua_to_track_event(&event_table)? {
                track_events.push(event);
            }
        }

        // Ensure track ends with EndOfTrack
        let has_end = track_events.iter().any(|e| {
            matches!(
                e.kind,
                TrackEventKind::Meta(midly::MetaMessage::EndOfTrack)
            )
        });
        if !has_end {
            track_events.push(TrackEvent {
                delta: 0.into(),
                kind: TrackEventKind::Meta(midly::MetaMessage::EndOfTrack),
            });
        }

        tracks.push(track_events);
    }

    let smf = Smf {
        header,
        tracks,
    };

    let mut buffer = Vec::new();
    smf.write(&mut buffer)
        .map_err(|e| anyhow::anyhow!("Failed to write MIDI: {}", e))?;

    let path = Path::new(path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).context("Failed to create parent directory")?;
    }
    fs::write(path, buffer).context("Failed to write MIDI file")?;

    Ok(())
}

/// Convert a Lua table to a track event.
fn lua_to_track_event(table: &Table) -> Result<Option<TrackEvent<'static>>> {
    let delta: u32 = table.get("delta").unwrap_or(0);
    let event_type: String = table.get("type")?;

    let kind = match event_type.as_str() {
        "note_on" => {
            let channel: u8 = table.get("channel")?;
            let note: u8 = table.get("note")?;
            let velocity: u8 = table.get("velocity")?;
            TrackEventKind::Midi {
                channel: channel.into(),
                message: MidiMessage::NoteOn {
                    key: note.into(),
                    vel: velocity.into(),
                },
            }
        }
        "note_off" => {
            let channel: u8 = table.get("channel")?;
            let note: u8 = table.get("note")?;
            let velocity: u8 = table.get("velocity").unwrap_or(0);
            TrackEventKind::Midi {
                channel: channel.into(),
                message: MidiMessage::NoteOff {
                    key: note.into(),
                    vel: velocity.into(),
                },
            }
        }
        "control_change" => {
            let channel: u8 = table.get("channel")?;
            let controller: u8 = table.get("controller")?;
            let value: u8 = table.get("value")?;
            TrackEventKind::Midi {
                channel: channel.into(),
                message: MidiMessage::Controller {
                    controller: controller.into(),
                    value: value.into(),
                },
            }
        }
        "program_change" => {
            let channel: u8 = table.get("channel")?;
            let program: u8 = table.get("program")?;
            TrackEventKind::Midi {
                channel: channel.into(),
                message: MidiMessage::ProgramChange {
                    program: program.into(),
                },
            }
        }
        "pitch_bend" => {
            let channel: u8 = table.get("channel")?;
            let value: i16 = table.get("value")?;
            TrackEventKind::Midi {
                channel: channel.into(),
                message: MidiMessage::PitchBend {
                    bend: midly::PitchBend::from_int(value),
                },
            }
        }
        "meta" => {
            let meta_type: String = table.get("meta_type")?;
            match meta_type.as_str() {
                "tempo" => {
                    let tempo: u32 = table.get("tempo")?;
                    TrackEventKind::Meta(midly::MetaMessage::Tempo(tempo.into()))
                }
                "time_signature" => {
                    let num: u8 = table.get("numerator")?;
                    let denom: u8 = table.get("denominator")?;
                    let denom_log = (denom as f32).log2() as u8;
                    TrackEventKind::Meta(midly::MetaMessage::TimeSignature(num, denom_log, 24, 8))
                }
                "end_of_track" => TrackEventKind::Meta(midly::MetaMessage::EndOfTrack),
                _ => return Ok(None),
            }
        }
        _ => return Ok(None),
    };

    Ok(Some(TrackEvent {
        delta: delta.into(),
        kind,
    }))
}

/// Transpose all note events by the given number of semitones.
fn transpose_events(events: Table, semitones: i32) -> Result<()> {
    for pair in events.pairs::<i64, Table>() {
        let (_, event) = pair?;
        let event_type: String = event.get("type").unwrap_or_default();

        if event_type == "note_on" || event_type == "note_off" {
            let note: i32 = event.get("note")?;
            let new_note = (note + semitones).clamp(0, 127);
            event.set("note", new_note)?;
        }
    }
    Ok(())
}

/// Quantize event deltas to the nearest grid position.
fn quantize_events(events: Table, grid: u32) -> Result<()> {
    if grid == 0 {
        return Ok(());
    }

    let mut accumulated_time: u32 = 0;

    for pair in events.pairs::<i64, Table>() {
        let (_, event) = pair?;
        let delta: u32 = event.get("delta").unwrap_or(0);

        accumulated_time += delta;

        // Quantize the accumulated time
        let quantized = ((accumulated_time + grid / 2) / grid) * grid;
        let new_delta = quantized.saturating_sub(accumulated_time - delta);

        event.set("delta", new_delta)?;
        accumulated_time = quantized;
    }
    Ok(())
}

/// Merge multiple tracks into a single track.
fn merge_tracks(lua: &Lua, tracks: Table) -> Result<Table> {
    // Collect all events with absolute times
    let mut all_events: Vec<(u32, Table)> = Vec::new();

    for pair in tracks.pairs::<i64, Table>() {
        let (_, track) = pair?;
        let events: Table = track.get("events")?;
        let mut time: u32 = 0;

        for pair in events.pairs::<i64, Table>() {
            let (_, event) = pair?;
            let delta: u32 = event.get("delta").unwrap_or(0);
            time += delta;

            // Clone the event table
            let new_event = lua.create_table()?;
            for pair in event.pairs::<String, LuaValue>() {
                let (k, v) = pair?;
                new_event.set(k, v)?;
            }
            new_event.set("_abs_time", time)?;

            all_events.push((time, new_event));
        }
    }

    // Sort by absolute time
    all_events.sort_by_key(|(time, _)| *time);

    // Convert back to delta times
    let result = lua.create_table()?;
    let events_table = lua.create_table()?;
    let mut prev_time: u32 = 0;

    for (idx, (abs_time, event)) in all_events.into_iter().enumerate() {
        let delta = abs_time - prev_time;
        event.set("delta", delta)?;
        event.set("_abs_time", LuaValue::Nil)?; // Remove temp field
        events_table.set(idx + 1, event)?;
        prev_time = abs_time;
    }

    result.set("events", events_table)?;
    Ok(result)
}

/// Filter events using a Lua predicate function.
fn filter_events(lua: &Lua, events: Table, predicate: mlua::Function) -> Result<Table> {
    let result = lua.create_table()?;
    let mut idx = 1;

    for pair in events.pairs::<i64, Table>() {
        let (_, event) = pair?;
        let keep: bool = predicate.call(event.clone())?;
        if keep {
            result.set(idx, event)?;
            idx += 1;
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_midi_globals() {
        let lua = Lua::new();
        register_midi_globals(&lua).unwrap();

        let globals = lua.globals();
        let midi: Table = globals.get("midi").unwrap();

        assert!(midi.contains_key("read").unwrap());
        assert!(midi.contains_key("write").unwrap());
        assert!(midi.contains_key("transpose").unwrap());
        assert!(midi.contains_key("quantize").unwrap());
        assert!(midi.contains_key("merge").unwrap());
        assert!(midi.contains_key("filter").unwrap());
    }

    #[test]
    fn test_transpose_events() {
        let lua = Lua::new();
        let events = lua.create_table().unwrap();

        let note1 = lua.create_table().unwrap();
        note1.set("type", "note_on").unwrap();
        note1.set("note", 60).unwrap();
        events.set(1, note1).unwrap();

        let note2 = lua.create_table().unwrap();
        note2.set("type", "note_off").unwrap();
        note2.set("note", 60).unwrap();
        events.set(2, note2).unwrap();

        transpose_events(events.clone(), 7).unwrap();

        let e1: Table = events.get(1).unwrap();
        let e2: Table = events.get(2).unwrap();
        assert_eq!(e1.get::<i32>("note").unwrap(), 67);
        assert_eq!(e2.get::<i32>("note").unwrap(), 67);
    }

    #[test]
    fn test_transpose_clamp() {
        let lua = Lua::new();
        let events = lua.create_table().unwrap();

        let note = lua.create_table().unwrap();
        note.set("type", "note_on").unwrap();
        note.set("note", 120).unwrap();
        events.set(1, note).unwrap();

        transpose_events(events.clone(), 20).unwrap();

        let e1: Table = events.get(1).unwrap();
        assert_eq!(e1.get::<i32>("note").unwrap(), 127); // Clamped to max
    }
}
