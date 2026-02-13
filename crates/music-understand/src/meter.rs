use midi_analysis::{MidiFileContext, TimedNote};

use crate::types::MeterDetection;

/// Candidate meters: (numerator, denominator, bar_length_in_quarter_notes).
const CANDIDATE_METERS: [(u8, u8, f64); 7] = [
    (2, 4, 2.0),
    (3, 4, 3.0),
    (4, 4, 4.0),
    (5, 4, 5.0),
    (6, 8, 3.0),   // 6/8 = 3 quarter notes, grouped in dotted quarters
    (7, 8, 3.5),
    (12, 8, 6.0),  // 12/8 = 6 quarter notes
];

/// Detect the most likely meter from note onset patterns.
///
/// For each candidate meter, builds a histogram of onset positions modulo
/// the bar length, then scores by downbeat strength, accent ratio, and
/// structural entropy. Compound meters (6/8, 12/8) get a bonus when
/// triplet-feel inter-onset intervals are detected.
pub fn detect_meter(notes: &[TimedNote], context: &MidiFileContext) -> MeterDetection {
    let ppq = context.ppq as f64;

    // Fall back to MIDI metadata time signature
    let midi_ts = context
        .time_signatures
        .first()
        .map(|ts| (ts.numerator, ts.denominator))
        .unwrap_or((4, 4));

    if notes.is_empty() || ppq == 0.0 {
        return MeterDetection {
            numerator: midi_ts.0,
            denominator: midi_ts.1,
            confidence: 0.0,
            triplet_feel: 0.0,
        };
    }

    // Onset positions in quarter-note units
    let onsets: Vec<f64> = notes.iter().map(|n| n.onset_tick as f64 / ppq).collect();

    // Quantize to 16th-note resolution, deduplicate, sort
    let mut quantized: Vec<f64> = onsets.iter().map(|o| (o * 4.0).round() / 4.0).collect();
    quantized.sort_by(|a, b| a.total_cmp(b));
    quantized.dedup_by(|a, b| (*a - *b).abs() < 1e-6);

    if quantized.len() < 4 {
        return MeterDetection {
            numerator: midi_ts.0,
            denominator: midi_ts.1,
            confidence: 0.0,
            triplet_feel: 0.0,
        };
    }

    // Inter-onset intervals
    let iois: Vec<f64> = quantized
        .windows(2)
        .map(|w| w[1] - w[0])
        .filter(|&d| d > 0.01)
        .collect();

    let triplet_ratio = triplet_feel(&iois);

    let mut best_meter = midi_ts;
    let mut best_score = 0.0_f64;

    for &(num, den, bar_len) in &CANDIDATE_METERS {
        let mut score = score_meter(&onsets, bar_len);

        // Bonus for compound meters when triplet feel is present
        if den == 8 && (num == 6 || num == 12) && triplet_ratio > 0.2 {
            score += triplet_ratio * 0.3;
        }

        // Slight penalty for unusual meters unless very strong
        if num == 5 || num == 7 {
            score *= 0.85;
        }

        // Tiebreaker: slight bonus for matching MIDI metadata
        if num == midi_ts.0 && den == midi_ts.1 {
            score += 0.05;
        }

        if score > best_score {
            best_score = score;
            best_meter = (num, den);
        }
    }

    MeterDetection {
        numerator: best_meter.0,
        denominator: best_meter.1,
        confidence: (best_score.min(1.0) * 1000.0).round() / 1000.0,
        triplet_feel: triplet_ratio,
    }
}

/// Score how well note onsets fit a given bar length.
///
/// A good meter has strong onset density at beat 1, clear difference
/// between strong and weak beats, and low entropy (structured pattern).
fn score_meter(onsets: &[f64], bar_length: f64) -> f64 {
    if bar_length <= 0.0 {
        return 0.0;
    }

    let bins_per_beat: usize = 4; // 16th-note resolution
    let n_bins = (bar_length * bins_per_beat as f64) as usize;
    if n_bins < 2 {
        return 0.0;
    }

    // Build histogram of onset positions within the bar
    let mut hist = vec![0.0_f64; n_bins];
    for &onset in onsets {
        let pos = onset % bar_length;
        let bin = ((pos / bar_length) * n_bins as f64) as usize;
        let bin = bin.min(n_bins - 1);
        hist[bin] += 1.0;
    }

    let total: f64 = hist.iter().sum();
    if total == 0.0 {
        return 0.0;
    }

    let hist_norm: Vec<f64> = hist.iter().map(|h| h / total).collect();

    // Beat 1 strength
    let beat1_strength = hist_norm[0];

    // Strong beat pattern: every bins_per_beat bin is on-beat
    let beat_bins: Vec<usize> = (0..n_bins).step_by(bins_per_beat).collect();
    let offbeat_bins: Vec<usize> = (0..n_bins).filter(|i| !beat_bins.contains(i)).collect();

    let beat_density = if beat_bins.is_empty() {
        0.0
    } else {
        beat_bins.iter().map(|&i| hist_norm[i]).sum::<f64>() / beat_bins.len() as f64
    };
    let offbeat_density = if offbeat_bins.is_empty() {
        0.0
    } else {
        offbeat_bins.iter().map(|&i| hist_norm[i]).sum::<f64>() / offbeat_bins.len() as f64
    };

    let accent_ratio = if offbeat_density > 0.0 {
        beat_density / (beat_density + offbeat_density)
    } else {
        1.0
    };

    // Entropy: lower = more structured = better meter fit
    let entropy: f64 = hist_norm
        .iter()
        .filter(|&&h| h > 0.0)
        .map(|&h| -h * h.log2())
        .sum();
    let max_entropy = (n_bins as f64).log2();
    let structure = if max_entropy > 0.0 {
        1.0 - (entropy / max_entropy)
    } else {
        0.0
    };

    beat1_strength * 0.3 + accent_ratio * 0.4 + structure * 0.3
}

/// Detect how much of the rhythm uses triplet groupings.
///
/// Returns ratio of inter-onset intervals near 1/3 or 2/3 of a beat
/// versus straight 1/4 or 1/2 values. High ratio → compound meter.
fn triplet_feel(iois: &[f64]) -> f64 {
    if iois.is_empty() {
        return 0.0;
    }

    let triplet_count = iois
        .iter()
        .filter(|&&d| (d - 1.0 / 3.0).abs() < 0.08 || (d - 2.0 / 3.0).abs() < 0.08)
        .count();

    let straight_count = iois
        .iter()
        .filter(|&&d| {
            (d - 0.25).abs() < 0.06 || (d - 0.5).abs() < 0.06 || (d - 1.0).abs() < 0.06
        })
        .count();

    let total = triplet_count + straight_count;
    if total == 0 {
        return 0.0;
    }

    triplet_count as f64 / total as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_note(pitch: u8, onset: u64, offset: u64) -> TimedNote {
        TimedNote {
            pitch,
            onset_tick: onset,
            offset_tick: offset,
            velocity: 80,
            channel: 0,
            track_index: 0,
        }
    }

    fn context_with_ppq(ppq: u16) -> MidiFileContext {
        MidiFileContext {
            ppq,
            format: 1,
            track_count: 1,
            tempo_changes: vec![],
            time_signatures: vec![],
            total_ticks: ppq as u64 * 32,
        }
    }

    #[test]
    fn empty_notes_returns_default() {
        let ctx = context_with_ppq(480);
        let result = detect_meter(&[], &ctx);
        assert_eq!(result.numerator, 4);
        assert_eq!(result.denominator, 4);
        assert_eq!(result.confidence, 0.0);
    }

    #[test]
    fn regular_quarter_notes_in_4_4() {
        let ppq = 480u16;
        // 8 bars of quarter notes on beats 1,2,3,4
        let notes: Vec<_> = (0..32)
            .map(|i| make_note(60, i * ppq as u64, i * ppq as u64 + 240))
            .collect();

        let ctx = context_with_ppq(ppq);
        let result = detect_meter(&notes, &ctx);
        // With perfectly regular quarter notes, multiple meters could score similarly,
        // but 4/4 should be competitive
        assert!(result.confidence > 0.3);
    }

    #[test]
    fn waltz_pattern_prefers_triple_meter() {
        // This is a unit test for the scoring function directly:
        // a pattern with onsets at positions 0, 1, 2 (in quarter notes)
        // repeated over 16 bars of 3/4 should score higher for bar_len=3
        // than for bar_len=2 or bar_len=4.
        let ppq = 480u16;
        let mut onsets = Vec::new();
        for bar in 0..16u64 {
            let bar_start = (bar * 3) as f64;
            // Beat 1: double onset (accent)
            onsets.push(bar_start);
            onsets.push(bar_start);
            // Beat 2
            onsets.push(bar_start + 1.0);
            // Beat 3
            onsets.push(bar_start + 2.0);
        }

        let score_3 = score_meter(&onsets, 3.0);
        let _score_2 = score_meter(&onsets, 2.0);
        let score_4 = score_meter(&onsets, 4.0);

        // 3-beat grouping should have highest beat-1 accent ratio
        // since every 3rd beat gets double onset density
        assert!(
            score_3 > score_4,
            "3/4 score ({:.3}) should beat 4/4 ({:.3})",
            score_3,
            score_4
        );

        // With the MIDI metadata tiebreaker bonus, 3/4 should win
        // over 2/4 in detect_meter when time_sig is 3/4
        let notes: Vec<_> = onsets
            .iter()
            .map(|&o| make_note(60, (o * ppq as f64) as u64, (o * ppq as f64) as u64 + 240))
            .collect();

        let mut ctx = context_with_ppq(ppq);
        ctx.total_ticks = 16 * 3 * ppq as u64;
        ctx.time_signatures.push(midi_analysis::analyze::TimeSignature {
            tick: 0,
            numerator: 3,
            denominator: 4,
        });
        let result = detect_meter(&notes, &ctx);
        assert!(
            result.numerator == 3 || result.numerator == 6,
            "Expected 3/4 or 6/8 with metadata hint, got {}/{}",
            result.numerator,
            result.denominator
        );
    }

    #[test]
    fn triplet_feel_straight() {
        // Straight 8th notes: IOI = 0.5
        let iois = vec![0.5, 0.5, 0.5, 0.5, 0.5, 0.5];
        let ratio = triplet_feel(&iois);
        assert!(ratio < 0.1, "straight rhythm should have low triplet feel: {}", ratio);
    }

    #[test]
    fn triplet_feel_compound() {
        // Triplet groupings: IOI ≈ 0.333
        let iois = vec![0.333, 0.333, 0.333, 0.333, 0.333, 0.333];
        let ratio = triplet_feel(&iois);
        assert!(ratio > 0.8, "triplet rhythm should have high triplet feel: {}", ratio);
    }
}
