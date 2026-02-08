use crate::note::{SeparatedVoice, SeparationMethod, TimedNote, VoiceStats};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Parameters controlling voice separation behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(Default)]
pub struct SeparationParams {
    /// Maximum pitch jump (semitones) before starting a new voice. Default: 12.
    pub max_pitch_jump: Option<u8>,
    /// Maximum gap (ticks) before a voice is considered stale. Default: ppq * 4.
    pub max_gap_ticks: Option<u64>,
    /// Force a specific separation method instead of auto-detecting.
    pub method: Option<SeparationMethod>,
    /// Maximum number of voices to extract. Default: 8.
    pub max_voices: Option<usize>,
}


/// State of an active voice during pitch contiguity separation.
struct VoiceState {
    notes: Vec<TimedNote>,
    last_pitch: u8,
    last_offset_tick: u64,
}

/// Separate notes from a single track into distinct musical voices.
///
/// Strategy selection (when `params.method` is None):
/// 1. Multiple MIDI channels → channel split
/// 2. Max polyphony ≤ 1 → already monophonic
/// 3. Otherwise → pitch contiguity
pub fn separate_voices(
    notes: &[TimedNote],
    ppq: u16,
    params: &SeparationParams,
) -> Vec<SeparatedVoice> {
    if notes.is_empty() {
        return Vec::new();
    }

    let method = params.method.clone().unwrap_or_else(|| {
        auto_select_method(notes)
    });

    let source_track = notes.first().map(|n| n.track_index);

    match method {
        SeparationMethod::AlreadyMonophonic => {
            vec![SeparatedVoice {
                notes: notes.to_vec(),
                method: SeparationMethod::AlreadyMonophonic,
                voice_index: 0,
                stats: VoiceStats::from_notes(notes),
                source_channel: notes.first().map(|n| n.channel),
                source_track,
            }]
        }
        SeparationMethod::ChannelSplit => channel_split(notes, source_track),
        SeparationMethod::PitchContiguity => {
            pitch_contiguity(notes, ppq, params, source_track)
        }
        SeparationMethod::Skyline => skyline(notes, source_track),
        SeparationMethod::Bassline => bassline(notes, source_track),
    }
}

fn auto_select_method(notes: &[TimedNote]) -> SeparationMethod {
    // Check for multiple channels
    let channels: Vec<u8> = notes
        .iter()
        .map(|n| n.channel)
        .collect::<std::collections::HashSet<u8>>()
        .into_iter()
        .collect();

    if channels.len() > 1 {
        return SeparationMethod::ChannelSplit;
    }

    // Check polyphony
    if max_polyphony(notes) <= 1 {
        return SeparationMethod::AlreadyMonophonic;
    }

    SeparationMethod::PitchContiguity
}

fn max_polyphony(notes: &[TimedNote]) -> usize {
    let mut events: Vec<(u64, i32)> = Vec::with_capacity(notes.len() * 2);
    for note in notes {
        events.push((note.onset_tick, 1));
        events.push((note.offset_tick, -1));
    }
    // Sort: at same tick, offsets (-1) before onsets (+1)
    events.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));

    let mut current = 0i32;
    let mut max = 0i32;
    for &(_, delta) in &events {
        current += delta;
        max = max.max(current);
    }
    max.max(0) as usize
}

/// Split notes by MIDI channel.
fn channel_split(notes: &[TimedNote], source_track: Option<usize>) -> Vec<SeparatedVoice> {
    let mut by_channel: HashMap<u8, Vec<TimedNote>> = HashMap::new();
    for note in notes {
        by_channel.entry(note.channel).or_default().push(note.clone());
    }

    let mut channels: Vec<u8> = by_channel.keys().copied().collect();
    channels.sort_unstable();

    channels
        .iter()
        .enumerate()
        .map(|(voice_index, &ch)| {
            let voice_notes = by_channel.remove(&ch).unwrap_or_default();
            SeparatedVoice {
                stats: VoiceStats::from_notes(&voice_notes),
                notes: voice_notes,
                method: SeparationMethod::ChannelSplit,
                voice_index,
                source_channel: Some(ch),
                source_track,
            }
        })
        .collect()
}

/// Chew & Wu inspired nearest-neighbor pitch contiguity.
///
/// Greedy assignment: at each onset, assign notes to the active voice
/// with the closest last pitch. New voices are created when all voices
/// are too far away or at capacity.
fn pitch_contiguity(
    notes: &[TimedNote],
    ppq: u16,
    params: &SeparationParams,
    source_track: Option<usize>,
) -> Vec<SeparatedVoice> {
    let max_jump = params.max_pitch_jump.unwrap_or(12) as i16;
    let max_gap = params.max_gap_ticks.unwrap_or(ppq as u64 * 4);
    let max_voices = params.max_voices.unwrap_or(8);

    let mut sorted = notes.to_vec();
    sorted.sort_by(|a, b| a.onset_tick.cmp(&b.onset_tick).then(a.pitch.cmp(&b.pitch)));

    let mut voices: Vec<VoiceState> = Vec::new();

    for note in &sorted {
        // Find best matching voice: closest pitch, not stale, with room
        let mut best_idx: Option<usize> = None;
        let mut best_distance = i16::MAX;

        for (idx, voice) in voices.iter().enumerate() {
            // Skip stale voices
            if note.onset_tick > voice.last_offset_tick + max_gap {
                continue;
            }

            let distance = (note.pitch as i16 - voice.last_pitch as i16).abs();
            if distance <= max_jump && distance < best_distance {
                best_distance = distance;
                best_idx = Some(idx);
            }
        }

        match best_idx {
            Some(idx) => {
                voices[idx].notes.push(note.clone());
                voices[idx].last_pitch = note.pitch;
                voices[idx].last_offset_tick = note.offset_tick;
            }
            None => {
                // Try to reuse a stale voice slot before creating new
                let stale_idx = voices.iter().position(|v| {
                    note.onset_tick > v.last_offset_tick + max_gap
                });

                if let Some(idx) = stale_idx {
                    // Stale voice found - start a new voice in this slot
                    // Actually, stale voices are separate musical phrases;
                    // keep them and create a new one
                    if voices.len() < max_voices {
                        voices.push(VoiceState {
                            notes: vec![note.clone()],
                            last_pitch: note.pitch,
                            last_offset_tick: note.offset_tick,
                        });
                    } else {
                        // At capacity: force into the nearest stale voice
                        voices[idx].notes.push(note.clone());
                        voices[idx].last_pitch = note.pitch;
                        voices[idx].last_offset_tick = note.offset_tick;
                    }
                } else if voices.len() < max_voices {
                    voices.push(VoiceState {
                        notes: vec![note.clone()],
                        last_pitch: note.pitch,
                        last_offset_tick: note.offset_tick,
                    });
                } else {
                    // Force into closest voice regardless of jump limit
                    let closest = voices
                        .iter()
                        .enumerate()
                        .min_by_key(|(_, v)| (note.pitch as i16 - v.last_pitch as i16).abs())
                        .map(|(idx, _)| idx)
                        .unwrap_or(0);

                    voices[closest].notes.push(note.clone());
                    voices[closest].last_pitch = note.pitch;
                    voices[closest].last_offset_tick = note.offset_tick;
                }
            }
        }
    }

    // Post-process: merge very short voices (< 4 notes) into nearest neighbor
    merge_short_voices(&mut voices, 4);

    voices
        .into_iter()
        .enumerate()
        .filter(|(_, v)| !v.notes.is_empty())
        .map(|(voice_index, mut v)| {
            v.notes.sort_by_key(|n| n.onset_tick);
            SeparatedVoice {
                stats: VoiceStats::from_notes(&v.notes),
                notes: v.notes,
                method: SeparationMethod::PitchContiguity,
                voice_index,
                source_channel: None,
                source_track,
            }
        })
        .collect()
}

/// Merge voices with fewer than `min_notes` into their nearest neighbor by mean pitch.
fn merge_short_voices(voices: &mut [VoiceState], min_notes: usize) {
    if voices.len() <= 1 {
        return;
    }

    let mean_pitches: Vec<f64> = voices
        .iter()
        .map(|v| {
            if v.notes.is_empty() {
                0.0
            } else {
                v.notes.iter().map(|n| n.pitch as f64).sum::<f64>() / v.notes.len() as f64
            }
        })
        .collect();

    let mut to_merge: Vec<(usize, usize)> = Vec::new(); // (from, to)

    for (idx, voice) in voices.iter().enumerate() {
        if voice.notes.len() < min_notes && !voice.notes.is_empty() {
            // Find nearest voice by mean pitch that isn't also short
            let my_mean = mean_pitches[idx];
            let target = voices
                .iter()
                .enumerate()
                .filter(|(i, v)| *i != idx && v.notes.len() >= min_notes)
                .min_by(|(i, _), (j, _)| {
                    let dist_i = (mean_pitches[*i] - my_mean).abs();
                    let dist_j = (mean_pitches[*j] - my_mean).abs();
                    dist_i
                        .partial_cmp(&dist_j)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .map(|(i, _)| i);

            if let Some(target_idx) = target {
                to_merge.push((idx, target_idx));
            }
        }
    }

    // Apply merges (drain from short to target)
    for (from, to) in to_merge.iter().rev() {
        if *from < voices.len() && *to < voices.len() && from != to {
            let notes: Vec<TimedNote> = voices[*from].notes.drain(..).collect();
            voices[*to].notes.extend(notes);
        }
    }
}

/// Skyline: at each distinct onset time, keep only the highest-pitched note.
fn skyline(notes: &[TimedNote], source_track: Option<usize>) -> Vec<SeparatedVoice> {
    let (top, rest) = extract_extreme(notes, true);

    let mut result = vec![SeparatedVoice {
        stats: VoiceStats::from_notes(&top),
        notes: top,
        method: SeparationMethod::Skyline,
        voice_index: 0,
        source_channel: None,
        source_track,
    }];

    if !rest.is_empty() {
        result.push(SeparatedVoice {
            stats: VoiceStats::from_notes(&rest),
            notes: rest,
            method: SeparationMethod::Skyline,
            voice_index: 1,
            source_channel: None,
            source_track,
        });
    }

    result
}

/// Bassline: at each distinct onset time, keep only the lowest-pitched note.
fn bassline(notes: &[TimedNote], source_track: Option<usize>) -> Vec<SeparatedVoice> {
    let (bottom, rest) = extract_extreme(notes, false);

    let mut result = vec![SeparatedVoice {
        stats: VoiceStats::from_notes(&bottom),
        notes: bottom,
        method: SeparationMethod::Bassline,
        voice_index: 0,
        source_channel: None,
        source_track,
    }];

    if !rest.is_empty() {
        result.push(SeparatedVoice {
            stats: VoiceStats::from_notes(&rest),
            notes: rest,
            method: SeparationMethod::Bassline,
            voice_index: 1,
            source_channel: None,
            source_track,
        });
    }

    result
}

/// Extract the highest (or lowest) pitched note at each onset time.
/// Returns (extracted, remainder).
fn extract_extreme(notes: &[TimedNote], highest: bool) -> (Vec<TimedNote>, Vec<TimedNote>) {
    // Group notes by onset tick
    let mut by_onset: HashMap<u64, Vec<&TimedNote>> = HashMap::new();
    for note in notes {
        by_onset.entry(note.onset_tick).or_default().push(note);
    }

    let mut extracted = Vec::new();
    let mut remainder = Vec::new();

    for group in by_onset.values() {
        let extreme = if highest {
            group.iter().max_by_key(|n| n.pitch)
        } else {
            group.iter().min_by_key(|n| n.pitch)
        };

        if let Some(&chosen) = extreme {
            extracted.push(chosen.clone());
            for note in group {
                if !std::ptr::eq(*note, chosen) {
                    remainder.push((*note).clone());
                }
            }
        }
    }

    extracted.sort_by_key(|n| n.onset_tick);
    remainder.sort_by_key(|n| n.onset_tick);

    (extracted, remainder)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_notes(specs: &[(u64, u64, u8, u8)]) -> Vec<TimedNote> {
        specs
            .iter()
            .map(|&(onset, offset, pitch, channel)| TimedNote {
                onset_tick: onset,
                offset_tick: offset,
                pitch,
                velocity: 100,
                channel,
                track_index: 0,
            })
            .collect()
    }

    #[test]
    fn monophonic_passthrough() {
        let notes = make_notes(&[
            (0, 480, 60, 0),
            (480, 960, 64, 0),
            (960, 1440, 67, 0),
        ]);

        let voices = separate_voices(&notes, 480, &SeparationParams::default());
        assert_eq!(voices.len(), 1);
        assert_eq!(voices[0].method, SeparationMethod::AlreadyMonophonic);
        assert_eq!(voices[0].notes.len(), 3);
    }

    #[test]
    fn channel_split_format0() {
        let notes = make_notes(&[
            (0, 480, 60, 0),   // ch0
            (0, 480, 48, 1),   // ch1
            (480, 960, 64, 0), // ch0
            (480, 960, 52, 1), // ch1
        ]);

        let voices = separate_voices(&notes, 480, &SeparationParams::default());
        assert_eq!(voices.len(), 2);
        assert_eq!(voices[0].method, SeparationMethod::ChannelSplit);

        // Channel 0 should have higher pitches
        assert_eq!(voices[0].source_channel, Some(0));
        assert_eq!(voices[0].notes.len(), 2);
        assert_eq!(voices[1].source_channel, Some(1));
        assert_eq!(voices[1].notes.len(), 2);
    }

    #[test]
    fn pitch_contiguity_two_interleaved_melodies() {
        // Two melodies interleaved: one high (C5-E5-D5-F5), one low (C3-E3-D3-F3)
        let notes = make_notes(&[
            (0, 240, 72, 0),     // C5
            (0, 240, 48, 0),     // C3
            (240, 480, 76, 0),   // E5
            (240, 480, 52, 0),   // E3
            (480, 720, 74, 0),   // D5
            (480, 720, 50, 0),   // D3
            (720, 960, 77, 0),   // F5
            (720, 960, 53, 0),   // F3
        ]);

        let voices = separate_voices(&notes, 480, &SeparationParams::default());
        assert_eq!(voices.len(), 2);

        // One voice should be high (around C5), one low (around C3)
        let high_voice = voices.iter().find(|v| v.stats.mean_pitch > 60.0).unwrap();
        let low_voice = voices.iter().find(|v| v.stats.mean_pitch < 60.0).unwrap();

        assert_eq!(high_voice.notes.len(), 4);
        assert_eq!(low_voice.notes.len(), 4);
        assert!(high_voice.stats.mean_pitch > 70.0);
        assert!(low_voice.stats.mean_pitch < 55.0);
    }

    #[test]
    fn skyline_extracts_highest() {
        let notes = make_notes(&[
            (0, 480, 60, 0),   // C4 (lower)
            (0, 480, 72, 0),   // C5 (higher) ← skyline picks this
            (480, 960, 64, 0), // E4 (lower)
            (480, 960, 76, 0), // E5 (higher) ← skyline picks this
        ]);

        let params = SeparationParams {
            method: Some(SeparationMethod::Skyline),
            ..Default::default()
        };
        let voices = separate_voices(&notes, 480, &params);

        assert_eq!(voices.len(), 2);
        let top = &voices[0];
        assert_eq!(top.notes.len(), 2);
        assert!(top.notes.iter().all(|n| n.pitch >= 72));
    }

    #[test]
    fn bassline_extracts_lowest() {
        let notes = make_notes(&[
            (0, 480, 60, 0),   // C4 (higher)
            (0, 480, 36, 0),   // C2 (lower) ← bassline picks this
            (480, 960, 64, 0), // E4 (higher)
            (480, 960, 40, 0), // E2 (lower) ← bassline picks this
        ]);

        let params = SeparationParams {
            method: Some(SeparationMethod::Bassline),
            ..Default::default()
        };
        let voices = separate_voices(&notes, 480, &params);

        assert_eq!(voices.len(), 2);
        let bottom = &voices[0];
        assert_eq!(bottom.notes.len(), 2);
        assert!(bottom.notes.iter().all(|n| n.pitch <= 40));
    }

    #[test]
    fn empty_notes() {
        let voices = separate_voices(&[], 480, &SeparationParams::default());
        assert!(voices.is_empty());
    }

    #[test]
    fn single_note() {
        let notes = make_notes(&[(0, 480, 60, 0)]);
        let voices = separate_voices(&notes, 480, &SeparationParams::default());
        assert_eq!(voices.len(), 1);
        assert_eq!(voices[0].method, SeparationMethod::AlreadyMonophonic);
    }
}
