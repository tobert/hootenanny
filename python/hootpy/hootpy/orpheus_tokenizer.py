"""
Orpheus MIDI tokenization.

Handles conversion between MIDI bytes and Orpheus token sequences.
"""

from __future__ import annotations

import os
import tempfile
from pathlib import Path

from . import tmidix


class OrpheusTokenizer:
    """
    MIDI ↔ Token conversion for Orpheus models.

    Token ranges:
    - 0-255: Delta time (in 16ms increments)
    - 256-16767: Pitch+patch combined (128 * patch) + pitch + 256
    - 16768-18815: Duration+velocity combined (8 * duration) + velocity + 16768
    - 18816: Start token
    - 18817: EOS token (base models)
    - 18818: EOS token (loops/drums)
    - 18819: PAD token
    """

    # Special tokens
    START_TOKEN = 18816
    EOS_TOKEN_BASE = 18817
    EOS_TOKEN_LOOPS = 18818
    PAD_TOKEN = 18819

    def encode_midi(self, midi_bytes: bytes) -> list[int]:
        """
        Convert MIDI bytes to Orpheus token sequence.

        Process:
        1. Load MIDI → single track score
        2. Apply sustain pedal processing
        3. Augment (drums last)
        4. Clean duplicates and fix durations
        5. Convert to delta time representation
        6. Chordify and encode to tokens

        Args:
            midi_bytes: Raw MIDI file bytes

        Returns:
            List of token integers
        """
        # Write to temp file for TMIDIX processing (some codepaths need file path)
        with tempfile.NamedTemporaryFile(mode="wb", suffix=".mid", delete=False) as f:
            temp_path = f.name
            f.write(midi_bytes)

        try:
            # Load and process MIDI
            raw_score = tmidix.midi2single_track_ms_score(temp_path)
            escore_notes = tmidix.advanced_score_processor(
                raw_score, return_enhanced_score_notes=True, apply_sustain=True
            )
            escore_notes = tmidix.augment_enhanced_score_notes(escore_notes[0], sort_drums_last=True)
            escore_notes = tmidix.remove_duplicate_pitches_from_escore_notes(escore_notes)
            escore_notes = tmidix.fix_escore_notes_durations(escore_notes, min_notes_gap=0)

            # Convert to delta time representation
            dscore = tmidix.delta_score_notes(escore_notes)
            dcscore = tmidix.chordify_score([d[1:] for d in dscore])

            # Encode to Orpheus tokens
            melody_chords = [self.START_TOKEN]

            for c in dcscore:
                delta_time = c[0][0]
                melody_chords.append(delta_time)

                for e in c:
                    # Extract and clamp values
                    dur = max(1, min(255, e[1]))
                    pat = max(0, min(128, e[5]))
                    ptc = max(1, min(127, e[3]))
                    vel = max(8, min(127, e[4]))
                    velocity = round(vel / 15) - 1

                    # Combine into tokens
                    pat_ptc = (128 * pat) + ptc
                    dur_vel = (8 * dur) + velocity

                    melody_chords.extend([pat_ptc + 256, dur_vel + 16768])

            return melody_chords

        finally:
            os.unlink(temp_path)

    def decode_tokens(self, tokens: list[int]) -> bytes:
        """
        Convert Orpheus tokens back to MIDI bytes.

        Process:
        1. Parse tokens into notes with timing
        2. Manage patch assignments to channels
        3. Build enhanced score
        4. Convert to MIDI structure
        5. Encode as bytes

        Args:
            tokens: List of token integers

        Returns:
            MIDI file bytes
        """
        song_f = []
        time = 0
        dur = 1
        vel = 90
        pitch = 60
        channel = 0
        patch = 0

        # Channel and patch tracking
        patches = [-1] * 16
        channels = [0] * 16
        channels[9] = 1  # Reserve channel 9 for drums

        # Parse tokens
        for ss in tokens:
            if 0 <= ss < 256:
                # Delta time
                time += ss * 16

            elif 256 <= ss < 16768:
                # Pitch + patch
                patch = (ss - 256) // 128

                if patch < 128:
                    # Melodic instrument
                    if patch not in patches:
                        # Assign new patch to available channel
                        if 0 in channels:
                            cha = channels.index(0)
                            channels[cha] = 1
                        else:
                            cha = 15
                        patches[cha] = patch
                        channel = patches.index(patch)
                    else:
                        channel = patches.index(patch)

                if patch == 128:
                    # Drums
                    channel = 9

                pitch = (ss - 256) % 128

            elif 16768 <= ss < 18816:
                # Duration + velocity
                dur = ((ss - 16768) // 8) * 16
                vel = (((ss - 16768) % 8) + 1) * 15
                song_f.append(["note", time, dur, channel, pitch, vel, patch])

        # Fix unassigned patches
        patches = [0 if x == -1 else x for x in patches]

        # Build MIDI structure
        output_score, patches, overflow_patches = tmidix.patch_enhanced_score_notes(song_f)

        output_header = [
            1000,  # Ticks per quarter note
            [
                ["set_tempo", 0, 1000000],  # 60 BPM
                ["time_signature", 0, 4, 2, 24, 8],  # 4/4 time
            ],
        ]

        patch_list = [["patch_change", 0, i, patches[i]] for i in range(16)]
        output = output_header + [patch_list + output_score]

        # Convert to MIDI bytes
        midi_data = tmidix.score2midi(output, "ISO-8859-1")

        return midi_data


# Module-level tokenizer instance for convenience
_tokenizer = None


def get_tokenizer() -> OrpheusTokenizer:
    """Get or create a global tokenizer instance."""
    global _tokenizer
    if _tokenizer is None:
        _tokenizer = OrpheusTokenizer()
    return _tokenizer


def encode_midi(midi_bytes: bytes) -> list[int]:
    """Encode MIDI bytes to token sequence (convenience function)."""
    return get_tokenizer().encode_midi(midi_bytes)


def decode_tokens(tokens: list[int]) -> bytes:
    """Decode token sequence to MIDI bytes (convenience function)."""
    return get_tokenizer().decode_tokens(tokens)
