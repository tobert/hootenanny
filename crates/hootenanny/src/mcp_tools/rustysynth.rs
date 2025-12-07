//! RustySynth MIDI to WAV Rendering and SoundFont Inspection
//!
//! Provides functions for rendering MIDI files to WAV format using SoundFonts,
//! and inspecting SoundFont contents including preset/instrument mappings.

use anyhow::{Context, Result};
use hound::{WavSpec, WavWriter};
use rustysynth::{MidiFile, MidiFileSequencer, SoundFont, Synthesizer, SynthesizerSettings};
use serde::Serialize;
use std::io::Cursor;
use std::sync::Arc;

/// Render a MIDI file to WAV format using a SoundFont
///
/// # Arguments
/// * `midi_bytes` - MIDI file content
/// * `soundfont_bytes` - SoundFont file content
/// * `sample_rate` - Output sample rate (typically 44100 or 48000)
///
/// # Returns
/// WAV file as bytes
pub fn render_midi_to_wav(
    midi_bytes: &[u8],
    soundfont_bytes: &[u8],
    sample_rate: u32,
) -> Result<Vec<u8>> {
    // Load SoundFont from bytes
    let mut sf_cursor = Cursor::new(soundfont_bytes);
    let sound_font = Arc::new(SoundFont::new(&mut sf_cursor)
        .map_err(|e| {
            let error_msg = format!("{:?}", e);
            if error_msg.contains("SanityCheckFailed") {
                anyhow::anyhow!(
                    "SoundFont failed compatibility check (RustySynth SanityCheckFailed). \
                    This SF2 may use features not supported by RustySynth. \
                    Try a simpler SoundFont like GeneralUser GS, FluidR3, or TR-808 drums."
                )
            } else {
                anyhow::anyhow!("Failed to load SoundFont: {}", e)
            }
        })?);

    // Load MIDI from bytes
    let mut midi_cursor = Cursor::new(midi_bytes);
    let midi = Arc::new(MidiFile::new(&mut midi_cursor)
        .map_err(|e| anyhow::anyhow!("Failed to parse MIDI file: {}", e))?);

    // Create synthesizer
    let settings = SynthesizerSettings::new(sample_rate as i32);
    let synthesizer = Synthesizer::new(&sound_font, &settings)
        .map_err(|e| anyhow::anyhow!("Failed to create synthesizer: {}", e))?;

    // Create sequencer and play MIDI
    let mut sequencer = MidiFileSequencer::new(synthesizer);
    sequencer.play(&midi, false);

    // Calculate number of samples needed
    // Add extra time for note decay and reverb (3 seconds)
    let decay_time = 3.0;
    let total_time = midi.get_length() + decay_time;
    let sample_count = (sample_rate as f64 * total_time) as usize;

    // Render audio to stereo buffers
    let mut left = vec![0f32; sample_count];
    let mut right = vec![0f32; sample_count];
    sequencer.render(&mut left[..], &mut right[..]);

    // Convert to WAV bytes
    let wav_bytes = samples_to_wav(&left, &right, sample_rate)
        .context("Failed to encode WAV")?;

    Ok(wav_bytes)
}

/// Convert stereo float samples to WAV format
///
/// # Arguments
/// * `left` - Left channel samples (normalized -1.0 to 1.0)
/// * `right` - Right channel samples (normalized -1.0 to 1.0)
/// * `sample_rate` - Sample rate in Hz
///
/// # Returns
/// WAV file as bytes
fn samples_to_wav(left: &[f32], right: &[f32], sample_rate: u32) -> Result<Vec<u8>> {
    let spec = WavSpec {
        channels: 2,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut cursor = Cursor::new(Vec::new());
    let mut writer = WavWriter::new(&mut cursor, spec)
        .context("Failed to create WAV writer")?;

    // Interleave samples and convert to i16
    for (&l, &r) in left.iter().zip(right.iter()) {
        let l_sample = (l.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        let r_sample = (r.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        writer.write_sample(l_sample)
            .context("Failed to write left sample")?;
        writer.write_sample(r_sample)
            .context("Failed to write right sample")?;
    }

    writer.finalize()
        .context("Failed to finalize WAV")?;

    Ok(cursor.into_inner())
}

/// Calculate the duration of a WAV file in seconds
///
/// # Arguments
/// * `wav_bytes` - WAV file content
/// * `sample_rate` - Sample rate in Hz
///
/// # Returns
/// Duration in seconds
pub fn calculate_wav_duration(wav_bytes: &[u8], sample_rate: u32) -> f64 {
    // For stereo 16-bit audio: 4 bytes per sample (2 channels * 2 bytes per sample)
    // WAV header is typically 44 bytes
    let header_size = 44;
    if wav_bytes.len() < header_size {
        return 0.0;
    }

    let audio_data_size = wav_bytes.len() - header_size;
    let samples = audio_data_size / 4; // 4 bytes per stereo sample
    samples as f64 / sample_rate as f64
}

// --- SoundFont Inspection Types ---

/// Information about the SoundFont file
#[derive(Debug, Serialize)]
pub struct SoundfontInfo {
    pub name: String,
    pub preset_count: usize,
    pub instrument_count: usize,
    pub sample_count: usize,
}

/// A preset in the SoundFont
#[derive(Debug, Serialize)]
pub struct PresetInfo {
    pub name: String,
    pub bank: i32,
    pub program: i32,
    pub is_drum_kit: bool,
}

/// A drum mapping entry (for bank 128 presets)
#[derive(Debug, Serialize)]
pub struct DrumMapping {
    pub preset_name: String,
    pub bank: i32,
    pub program: i32,
    pub regions: Vec<PresetRegion>,
}

/// A preset region - the actual zones defined in the SF2
#[derive(Debug, Serialize)]
pub struct PresetRegion {
    pub key_lo: i32,
    pub key_hi: i32,
    pub key_range: String,
    pub instrument: String,
}

/// Complete SoundFont inspection result
#[derive(Debug, Serialize)]
pub struct SoundfontInspection {
    pub info: SoundfontInfo,
    pub presets: Vec<PresetInfo>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub drum_mappings: Vec<DrumMapping>,
}

/// Convert MIDI note number to note name
fn midi_note_to_name(note: i32) -> String {
    const NOTE_NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
    let octave = (note / 12) - 1;
    let note_idx = (note % 12) as usize;
    format!("{}{}", NOTE_NAMES[note_idx], octave)
}

/// Inspect a SoundFont and return its structure
///
/// # Arguments
/// * `soundfont_bytes` - SoundFont file content
/// * `include_drum_map` - Whether to include detailed drum key mappings
///
/// # Returns
/// SoundfontInspection with presets, instruments, and optional drum mappings
pub fn inspect_soundfont(soundfont_bytes: &[u8], include_drum_map: bool) -> Result<SoundfontInspection> {
    let mut cursor = Cursor::new(soundfont_bytes);
    let soundfont = SoundFont::new(&mut cursor)
        .map_err(|e| anyhow::anyhow!("Failed to load SoundFont: {:?}", e))?;

    let sf_info = soundfont.get_info();
    let presets = soundfont.get_presets();
    let instruments = soundfont.get_instruments();
    let samples = soundfont.get_sample_headers();

    let info = SoundfontInfo {
        name: sf_info.get_bank_name().to_string(),
        preset_count: presets.len(),
        instrument_count: instruments.len(),
        sample_count: samples.len(),
    };

    let mut preset_infos: Vec<PresetInfo> = presets
        .iter()
        .map(|p| PresetInfo {
            name: p.get_name().to_string(),
            bank: p.get_bank_number(),
            program: p.get_patch_number(),
            is_drum_kit: p.get_bank_number() == 128,
        })
        .collect();

    // Sort by bank then program
    preset_infos.sort_by(|a, b| {
        a.bank.cmp(&b.bank).then(a.program.cmp(&b.program))
    });

    let mut drum_mappings = Vec::new();

    if include_drum_map {
        // Show regions for drum kits (bank 128) or small soundfonts (likely single-purpose)
        for preset in presets.iter() {
            if preset.get_bank_number() == 128 || presets.len() <= 2 {
                let mut regions = Vec::new();

                for region in preset.get_regions() {
                    let key_lo = region.get_key_range_start();
                    let key_hi = region.get_key_range_end();
                    let instrument_idx = region.get_instrument_id();

                    let instrument_name = if instrument_idx < instruments.len() {
                        instruments[instrument_idx].get_name().to_string()
                    } else {
                        format!("Instrument {}", instrument_idx)
                    };

                    let key_range = if key_lo == key_hi {
                        midi_note_to_name(key_lo)
                    } else {
                        format!("{}-{}", midi_note_to_name(key_lo), midi_note_to_name(key_hi))
                    };

                    regions.push(PresetRegion {
                        key_lo,
                        key_hi,
                        key_range,
                        instrument: instrument_name,
                    });
                }

                // Sort by key_lo
                regions.sort_by_key(|r| r.key_lo);

                if !regions.is_empty() {
                    drum_mappings.push(DrumMapping {
                        preset_name: preset.get_name().to_string(),
                        bank: preset.get_bank_number(),
                        program: preset.get_patch_number(),
                        regions,
                    });
                }
            }
        }
    }

    Ok(SoundfontInspection {
        info,
        presets: preset_infos,
        drum_mappings,
    })
}

/// Result of inspecting a single preset
#[derive(Debug, Serialize)]
pub struct PresetInspection {
    pub name: String,
    pub bank: i32,
    pub program: i32,
    pub regions: Vec<RegionDetail>,
}

/// Detailed region info for a preset
#[derive(Debug, Serialize)]
pub struct RegionDetail {
    pub keys: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub velocity: Option<String>,
    pub instrument: String,
}

/// Inspect a specific preset by bank/program
pub fn inspect_preset(soundfont_bytes: &[u8], bank: i32, program: i32) -> Result<PresetInspection> {
    let mut cursor = Cursor::new(soundfont_bytes);
    let soundfont = SoundFont::new(&mut cursor)
        .map_err(|e| anyhow::anyhow!("Failed to load SoundFont: {:?}", e))?;

    let presets = soundfont.get_presets();
    let instruments = soundfont.get_instruments();

    let preset = presets
        .iter()
        .find(|p| p.get_bank_number() == bank && p.get_patch_number() == program)
        .ok_or_else(|| anyhow::anyhow!("Preset not found: bank {} program {}", bank, program))?;

    let mut regions = Vec::new();
    for region in preset.get_regions() {
        let key_lo = region.get_key_range_start();
        let key_hi = region.get_key_range_end();
        let vel_lo = region.get_velocity_range_start();
        let vel_hi = region.get_velocity_range_end();
        let inst_id = region.get_instrument_id();

        let keys = if key_lo == key_hi {
            midi_note_to_name(key_lo)
        } else {
            format!("{}-{}", midi_note_to_name(key_lo), midi_note_to_name(key_hi))
        };

        let velocity = if vel_lo == 0 && vel_hi == 127 {
            None
        } else if vel_lo == vel_hi {
            Some(format!("{}", vel_lo))
        } else {
            Some(format!("{}-{}", vel_lo, vel_hi))
        };

        let instrument = if inst_id < instruments.len() {
            instruments[inst_id].get_name().to_string()
        } else {
            format!("#{}", inst_id)
        };

        regions.push(RegionDetail { keys, velocity, instrument });
    }

    regions.sort_by_key(|r| {
        r.keys.split('-').next()
            .and_then(|s| s.chars().last())
            .map(|c| c.to_digit(10).unwrap_or(0) as i32)
            .unwrap_or(0)
    });

    Ok(PresetInspection {
        name: preset.get_name().to_string(),
        bank,
        program,
        regions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_wav_duration() {
        // 44 byte header + 44100 * 4 bytes = 1 second of 44.1kHz stereo audio
        let wav_size = 44 + (44100 * 4);
        let wav_bytes = vec![0u8; wav_size];
        let duration = calculate_wav_duration(&wav_bytes, 44100);
        assert!((duration - 1.0).abs() < 0.01);
    }
}
