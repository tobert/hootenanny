use crate::gm;
use crate::note::TimedNote;
use midly::{MetaMessage, MidiMessage, Smf, TrackEventKind};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Parsed MIDI file context: timing, format, and tempo map.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MidiFileContext {
    pub ppq: u16,
    pub format: u8,
    pub track_count: usize,
    pub tempo_changes: Vec<TempoChange>,
    pub time_signatures: Vec<TimeSignature>,
    pub total_ticks: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TempoChange {
    pub tick: u64,
    pub microseconds_per_beat: u32,
    pub bpm: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeSignature {
    pub tick: u64,
    pub numerator: u8,
    pub denominator: u8,
}

/// Per-track structural profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackProfile {
    pub track_index: usize,
    pub name: Option<String>,
    pub instrument: Option<String>,
    pub programs_used: Vec<u8>,
    pub channels_used: Vec<u8>,
    pub is_percussion: bool,
    pub note_count: usize,
    pub pitch_range: PitchRange,
    pub polyphony: PolyphonyProfile,
    pub density: DensityProfile,
    pub merged_voices_likely: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PitchRange {
    pub min: u8,
    pub max: u8,
    pub median: u8,
    pub std_dev: f64,
}

impl Default for PitchRange {
    fn default() -> Self {
        Self {
            min: 0,
            max: 0,
            median: 0,
            std_dev: 0.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolyphonyProfile {
    pub max_simultaneous: usize,
    pub mean_simultaneous: f64,
    pub polyphonic_ratio: f64,
}

impl Default for PolyphonyProfile {
    fn default() -> Self {
        Self {
            max_simultaneous: 0,
            mean_simultaneous: 0.0,
            polyphonic_ratio: 0.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DensityProfile {
    pub notes_per_beat: f64,
    pub peak_density: f64,
}

impl Default for DensityProfile {
    fn default() -> Self {
        Self {
            notes_per_beat: 0.0,
            peak_density: 0.0,
        }
    }
}

/// Top-level analysis result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MidiAnalysis {
    pub context: MidiFileContext,
    pub tracks: Vec<TrackProfile>,
    pub tracks_needing_separation: Vec<usize>,
    pub channel_count: usize,
    pub has_multi_channel_tracks: bool,
    pub summary: String,
}

/// Extract all notes from MIDI bytes, pairing note-on/note-off events.
pub fn extract_notes(smf: &Smf) -> (Vec<TimedNote>, MidiFileContext) {
    let ppq = match smf.header.timing {
        midly::Timing::Metrical(ticks) => ticks.as_int(),
        midly::Timing::Timecode(_, _) => 480,
    };

    let format = match smf.header.format {
        midly::Format::SingleTrack => 0,
        midly::Format::Parallel => 1,
        midly::Format::Sequential => 2,
    };

    let mut all_notes = Vec::new();
    let mut tempo_changes = Vec::new();
    let mut time_signatures = Vec::new();
    let mut total_ticks: u64 = 0;

    for (track_index, track) in smf.tracks.iter().enumerate() {
        let mut current_tick: u64 = 0;
        // Map (channel, pitch) → Vec<(onset_tick, velocity)> for stacking
        let mut pending: HashMap<(u8, u8), Vec<(u64, u8)>> = HashMap::new();

        for event in track {
            current_tick += event.delta.as_int() as u64;

            match event.kind {
                TrackEventKind::Meta(MetaMessage::Tempo(tempo)) => {
                    let usec = tempo.as_int();
                    tempo_changes.push(TempoChange {
                        tick: current_tick,
                        microseconds_per_beat: usec,
                        bpm: 60_000_000.0 / usec as f64,
                    });
                }
                TrackEventKind::Meta(MetaMessage::TimeSignature(num, denom_pow, _, _)) => {
                    time_signatures.push(TimeSignature {
                        tick: current_tick,
                        numerator: num,
                        denominator: 1u8 << denom_pow,
                    });
                }
                TrackEventKind::Midi { channel, message } => {
                    let ch = channel.as_int();
                    match message {
                        MidiMessage::NoteOn { key, vel } if vel.as_int() > 0 => {
                            pending
                                .entry((ch, key.as_int()))
                                .or_default()
                                .push((current_tick, vel.as_int()));
                        }
                        MidiMessage::NoteOff { key, .. }
                        | MidiMessage::NoteOn { key, .. } => {
                            // vel=0 NoteOn is NoteOff
                            let key = (ch, key.as_int());
                            if let Some(stack) = pending.get_mut(&key) {
                                if let Some((onset, velocity)) = stack.pop() {
                                    all_notes.push(TimedNote {
                                        onset_tick: onset,
                                        offset_tick: current_tick,
                                        pitch: key.1,
                                        velocity,
                                        channel: ch,
                                        track_index,
                                    });
                                }
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }

            total_ticks = total_ticks.max(current_tick);
        }

        // Close any unclosed notes at the track's final tick
        for ((_, pitch), stack) in &pending {
            for &(onset, velocity) in stack {
                all_notes.push(TimedNote {
                    onset_tick: onset,
                    offset_tick: current_tick,
                    pitch: *pitch,
                    velocity,
                    channel: 0,
                    track_index,
                });
            }
        }
    }

    // Sort by onset, then pitch for determinism
    all_notes.sort_by(|a, b| a.onset_tick.cmp(&b.onset_tick).then(a.pitch.cmp(&b.pitch)));

    // Deduplicate tempo changes (multiple tracks may repeat them in format 1)
    tempo_changes.sort_by_key(|t| t.tick);
    tempo_changes.dedup_by(|a, b| a.tick == b.tick && a.microseconds_per_beat == b.microseconds_per_beat);

    time_signatures.sort_by_key(|t| t.tick);
    time_signatures.dedup_by(|a, b| a.tick == b.tick);

    let context = MidiFileContext {
        ppq,
        format,
        track_count: smf.tracks.len(),
        tempo_changes,
        time_signatures,
        total_ticks,
    };

    (all_notes, context)
}

/// Build a profile for each track in the MIDI file.
pub fn profile_tracks(
    smf: &Smf,
    notes: &[TimedNote],
    context: &MidiFileContext,
    polyphony_threshold: f64,
) -> Vec<TrackProfile> {
    let ppq = context.ppq as f64;

    smf.tracks
        .iter()
        .enumerate()
        .map(|(track_index, track)| {
            // Extract track metadata
            let mut name = None;
            let mut programs: Vec<u8> = Vec::new();
            let mut channels: Vec<u8> = Vec::new();

            for event in track {
                match event.kind {
                    TrackEventKind::Meta(MetaMessage::TrackName(bytes)) => {
                        name = String::from_utf8(bytes.to_vec()).ok();
                    }
                    TrackEventKind::Midi { channel, message } => {
                        let ch = channel.as_int();
                        if !channels.contains(&ch) {
                            channels.push(ch);
                        }
                        if let MidiMessage::ProgramChange { program } = message {
                            let p = program.as_int();
                            if !programs.contains(&p) {
                                programs.push(p);
                            }
                        }
                    }
                    _ => {}
                }
            }

            let track_notes: Vec<&TimedNote> = notes
                .iter()
                .filter(|n| n.track_index == track_index)
                .collect();

            let is_percussion = channels.contains(&9);

            let instrument = programs
                .first()
                .map(|&p| gm::program_name(p).to_string());

            let pitch_range = compute_pitch_range(&track_notes);
            let polyphony = compute_polyphony(&track_notes);
            let density = compute_density(&track_notes, ppq, context.total_ticks);

            let merged_voices_likely = !is_percussion
                && polyphony.max_simultaneous > 1
                && polyphony.polyphonic_ratio > polyphony_threshold;

            TrackProfile {
                track_index,
                name,
                instrument,
                programs_used: programs,
                channels_used: channels.clone(),
                is_percussion,
                note_count: track_notes.len(),
                pitch_range,
                polyphony,
                density,
                merged_voices_likely,
            }
        })
        .collect()
}

/// Full analysis pipeline: parse → extract → profile → report.
pub fn analyze(midi_bytes: &[u8], polyphony_threshold: Option<f64>) -> crate::Result<MidiAnalysis> {
    let smf = Smf::parse(midi_bytes).map_err(|e| crate::Error::MidiParse(e.to_string()))?;
    let threshold = polyphony_threshold.unwrap_or(0.3);

    let (notes, context) = extract_notes(&smf);
    let tracks = profile_tracks(&smf, &notes, &context, threshold);

    let tracks_needing_separation: Vec<usize> = tracks
        .iter()
        .filter(|t| t.merged_voices_likely)
        .map(|t| t.track_index)
        .collect();

    let has_multi_channel_tracks = tracks
        .iter()
        .any(|t| t.channels_used.len() > 1);

    let all_channels: Vec<u8> = tracks
        .iter()
        .flat_map(|t| t.channels_used.iter().copied())
        .collect::<std::collections::HashSet<u8>>()
        .into_iter()
        .collect();

    let summary = build_summary(&tracks, &tracks_needing_separation, &context);

    Ok(MidiAnalysis {
        context,
        tracks,
        tracks_needing_separation,
        channel_count: all_channels.len(),
        has_multi_channel_tracks,
        summary,
    })
}

fn compute_pitch_range(notes: &[&TimedNote]) -> PitchRange {
    if notes.is_empty() {
        return PitchRange::default();
    }

    let mut pitches: Vec<u8> = notes.iter().map(|n| n.pitch).collect();
    pitches.sort_unstable();

    let min = pitches[0];
    let max = pitches[pitches.len() - 1];
    let median = pitches[pitches.len() / 2];

    let mean = pitches.iter().map(|&p| p as f64).sum::<f64>() / pitches.len() as f64;
    let variance = pitches
        .iter()
        .map(|&p| {
            let diff = p as f64 - mean;
            diff * diff
        })
        .sum::<f64>()
        / pitches.len() as f64;

    PitchRange {
        min,
        max,
        median,
        std_dev: variance.sqrt(),
    }
}

fn compute_polyphony(notes: &[&TimedNote]) -> PolyphonyProfile {
    if notes.is_empty() {
        return PolyphonyProfile::default();
    }

    // Build event list: +1 at onset, -1 at offset
    let mut events: Vec<(u64, i32)> = Vec::with_capacity(notes.len() * 2);
    for note in notes {
        events.push((note.onset_tick, 1));
        events.push((note.offset_tick, -1));
    }
    events.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));

    let mut current = 0i32;
    let mut max_sim = 0usize;
    let mut polyphonic_onsets = 0usize;
    let mut total_onsets = 0usize;

    // Track polyphony at each onset
    let onset_set: std::collections::HashSet<u64> =
        notes.iter().map(|n| n.onset_tick).collect();

    let mut sim_sum = 0.0;
    let mut sim_count = 0usize;

    for &(tick, delta) in &events {
        current += delta;
        let sim = current.max(0) as usize;
        max_sim = max_sim.max(sim);

        if onset_set.contains(&tick) && delta > 0 {
            total_onsets += 1;
            if sim > 1 {
                polyphonic_onsets += 1;
            }
            sim_sum += sim as f64;
            sim_count += 1;
        }
    }

    let mean_simultaneous = if sim_count > 0 {
        sim_sum / sim_count as f64
    } else {
        0.0
    };

    let polyphonic_ratio = if total_onsets > 0 {
        polyphonic_onsets as f64 / total_onsets as f64
    } else {
        0.0
    };

    PolyphonyProfile {
        max_simultaneous: max_sim,
        mean_simultaneous,
        polyphonic_ratio,
    }
}

fn compute_density(notes: &[&TimedNote], ppq: f64, total_ticks: u64) -> DensityProfile {
    if notes.is_empty() || total_ticks == 0 {
        return DensityProfile::default();
    }

    let total_beats = total_ticks as f64 / ppq;
    let notes_per_beat = notes.len() as f64 / total_beats;

    // Windowed density: 4-beat windows
    let window_ticks = (ppq * 4.0) as u64;
    let mut peak_density: f64 = 0.0;

    if window_ticks > 0 {
        let mut window_start = 0u64;
        while window_start < total_ticks {
            let window_end = window_start + window_ticks;
            let count = notes
                .iter()
                .filter(|n| n.onset_tick >= window_start && n.onset_tick < window_end)
                .count();
            let density = count as f64 / 4.0; // notes per beat in this window
            peak_density = peak_density.max(density);
            window_start += window_ticks;
        }
    }

    DensityProfile {
        notes_per_beat,
        peak_density,
    }
}

fn build_summary(
    tracks: &[TrackProfile],
    needing_separation: &[usize],
    context: &MidiFileContext,
) -> String {
    let format_name = match context.format {
        0 => "Format 0 (single track)",
        1 => "Format 1 (multi-track)",
        2 => "Format 2 (sequential)",
        _ => "Unknown format",
    };

    let note_tracks: Vec<&TrackProfile> = tracks.iter().filter(|t| t.note_count > 0).collect();
    let total_notes: usize = note_tracks.iter().map(|t| t.note_count).sum();

    let mut summary = format!(
        "{}, {} tracks ({} with notes), {} total notes, PPQ {}",
        format_name,
        context.track_count,
        note_tracks.len(),
        total_notes,
        context.ppq,
    );

    if !needing_separation.is_empty() {
        summary.push_str(&format!(
            ". {} tracks likely contain merged voices and need separation",
            needing_separation.len()
        ));
    }

    summary
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_midi_format1() -> Vec<u8> {
        // Build a minimal format-1 MIDI with 2 tracks
        // Track 0: tempo + time sig
        // Track 1: a few notes on channel 0
        let mut buf = Vec::new();

        // Header: MThd, length 6, format 1, 2 tracks, 480 ppq
        buf.extend_from_slice(b"MThd");
        buf.extend_from_slice(&6u32.to_be_bytes());
        buf.extend_from_slice(&1u16.to_be_bytes()); // format 1
        buf.extend_from_slice(&2u16.to_be_bytes()); // 2 tracks
        buf.extend_from_slice(&480u16.to_be_bytes()); // 480 ppq

        // Track 0: tempo track
        let mut track0 = Vec::new();
        // Set tempo to 120 BPM (500000 usec/beat)
        track0.extend_from_slice(&[0x00, 0xFF, 0x51, 0x03, 0x07, 0xA1, 0x20]);
        // Time sig 4/4
        track0.extend_from_slice(&[0x00, 0xFF, 0x58, 0x04, 0x04, 0x02, 0x18, 0x08]);
        // End of track
        track0.extend_from_slice(&[0x00, 0xFF, 0x2F, 0x00]);

        buf.extend_from_slice(b"MTrk");
        buf.extend_from_slice(&(track0.len() as u32).to_be_bytes());
        buf.extend_from_slice(&track0);

        // Track 1: monophonic melody (C4, E4, G4)
        let mut track1 = Vec::new();
        // Note On C4
        track1.extend_from_slice(&[0x00, 0x90, 60, 100]);
        // Note Off C4 after 480 ticks
        track1.extend_from_slice(&[0x83, 0x60, 0x80, 60, 0]);
        // Note On E4
        track1.extend_from_slice(&[0x00, 0x90, 64, 100]);
        // Note Off E4 after 480 ticks
        track1.extend_from_slice(&[0x83, 0x60, 0x80, 64, 0]);
        // Note On G4
        track1.extend_from_slice(&[0x00, 0x90, 67, 100]);
        // Note Off G4 after 480 ticks
        track1.extend_from_slice(&[0x83, 0x60, 0x80, 67, 0]);
        // End of track
        track1.extend_from_slice(&[0x00, 0xFF, 0x2F, 0x00]);

        buf.extend_from_slice(b"MTrk");
        buf.extend_from_slice(&(track1.len() as u32).to_be_bytes());
        buf.extend_from_slice(&track1);

        buf
    }

    #[test]
    fn extract_notes_from_format1() {
        let midi = make_test_midi_format1();
        let smf = Smf::parse(&midi).unwrap();
        let (notes, context) = extract_notes(&smf);

        assert_eq!(context.ppq, 480);
        assert_eq!(context.format, 1);
        assert_eq!(context.track_count, 2);
        assert_eq!(notes.len(), 3);

        // All notes on track 1
        assert!(notes.iter().all(|n| n.track_index == 1));

        // Pitches: C4=60, E4=64, G4=67
        assert_eq!(notes[0].pitch, 60);
        assert_eq!(notes[1].pitch, 64);
        assert_eq!(notes[2].pitch, 67);

        // Each note is 480 ticks long
        assert_eq!(notes[0].duration_ticks(), 480);
    }

    #[test]
    fn analyze_monophonic_track_not_flagged() {
        let midi = make_test_midi_format1();
        let analysis = analyze(&midi, None).unwrap();

        assert!(analysis.tracks_needing_separation.is_empty());
        // Track 1 should not be flagged
        let track1 = &analysis.tracks[1];
        assert!(!track1.merged_voices_likely);
        assert_eq!(track1.polyphony.max_simultaneous, 1);
    }

    fn make_polyphonic_midi() -> Vec<u8> {
        // Format 1, single note track with chords (simultaneous notes)
        let mut buf = Vec::new();
        buf.extend_from_slice(b"MThd");
        buf.extend_from_slice(&6u32.to_be_bytes());
        buf.extend_from_slice(&1u16.to_be_bytes());
        buf.extend_from_slice(&2u16.to_be_bytes());
        buf.extend_from_slice(&480u16.to_be_bytes());

        // Track 0: tempo
        let mut track0 = Vec::new();
        track0.extend_from_slice(&[0x00, 0xFF, 0x51, 0x03, 0x07, 0xA1, 0x20]);
        track0.extend_from_slice(&[0x00, 0xFF, 0x2F, 0x00]);
        buf.extend_from_slice(b"MTrk");
        buf.extend_from_slice(&(track0.len() as u32).to_be_bytes());
        buf.extend_from_slice(&track0);

        // Track 1: repeated chords (C4+E4+G4 simultaneously, 4 times)
        let mut track1 = Vec::new();
        for _ in 0..4 {
            // Three simultaneous note-ons (delta=0 for 2nd and 3rd)
            track1.extend_from_slice(&[0x00, 0x90, 60, 100]); // C4
            track1.extend_from_slice(&[0x00, 0x90, 64, 100]); // E4
            track1.extend_from_slice(&[0x00, 0x90, 67, 100]); // G4
            // All off after 480 ticks
            track1.extend_from_slice(&[0x83, 0x60, 0x80, 60, 0]);
            track1.extend_from_slice(&[0x00, 0x80, 64, 0]);
            track1.extend_from_slice(&[0x00, 0x80, 67, 0]);
        }
        track1.extend_from_slice(&[0x00, 0xFF, 0x2F, 0x00]);
        buf.extend_from_slice(b"MTrk");
        buf.extend_from_slice(&(track1.len() as u32).to_be_bytes());
        buf.extend_from_slice(&track1);

        buf
    }

    #[test]
    fn polyphonic_track_flagged() {
        let midi = make_polyphonic_midi();
        let analysis = analyze(&midi, None).unwrap();

        let track1 = &analysis.tracks[1];
        assert_eq!(track1.polyphony.max_simultaneous, 3);
        assert!(track1.merged_voices_likely);
        assert!(analysis.tracks_needing_separation.contains(&1));
    }

    #[test]
    fn tempo_extraction() {
        let midi = make_test_midi_format1();
        let smf = Smf::parse(&midi).unwrap();
        let (_, context) = extract_notes(&smf);

        assert_eq!(context.tempo_changes.len(), 1);
        assert!((context.tempo_changes[0].bpm - 120.0).abs() < 0.1);
    }
}
