//! RustySynth MIDI to WAV Rendering
//!
//! Provides functions for rendering MIDI files to WAV format using SoundFonts.

use anyhow::{Context, Result};
use hound::{WavSpec, WavWriter};
use rustysynth::{MidiFile, MidiFileSequencer, SoundFont, Synthesizer, SynthesizerSettings};
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
