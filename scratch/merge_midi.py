#!/usr/bin/env python3
"""
merge_midi.py - Merge multiple MIDI files into one

Usage: ./merge_midi.py output.mid input1.mid input2.mid [input3.mid ...]

This script merges MIDI files by combining all tracks from each input file.
Each input file's tracks are added to the output, preserving their original
timing, channels, and instruments.

Great for layering drums on orchestral tracks, etc.
"""

import sys
from mido import MidiFile, MidiTrack, merge_tracks

def merge_midi_files(output_path: str, input_paths: list[str]) -> None:
    """Merge multiple MIDI files into a single output file."""

    if not input_paths:
        print("Error: No input files provided")
        sys.exit(1)

    # Start with the first file as the base
    base_path = input_paths[0]
    print(f"Base file: {base_path}")
    output = MidiFile(base_path)

    print(f"  Ticks per beat: {output.ticks_per_beat}")
    print(f"  Tracks: {len(output.tracks)}")
    print(f"  Length: {output.length:.2f} seconds")

    # Add tracks from remaining files
    for path in input_paths[1:]:
        print(f"\nMerging: {path}")
        midi = MidiFile(path)
        print(f"  Ticks per beat: {midi.ticks_per_beat}")
        print(f"  Tracks: {len(midi.tracks)}")
        print(f"  Length: {midi.length:.2f} seconds")

        # Handle different ticks_per_beat by scaling if needed
        if midi.ticks_per_beat != output.ticks_per_beat:
            print(f"  Warning: Different ticks_per_beat, scaling from {midi.ticks_per_beat} to {output.ticks_per_beat}")
            scale = output.ticks_per_beat / midi.ticks_per_beat
            for track in midi.tracks:
                for msg in track:
                    if hasattr(msg, 'time'):
                        msg.time = int(msg.time * scale)

        # Add all tracks from this file
        for track in midi.tracks:
            output.tracks.append(track)

    print(f"\nWriting: {output_path}")
    print(f"  Total tracks: {len(output.tracks)}")
    output.save(output_path)
    print("Done!")

    # Verify
    verify = MidiFile(output_path)
    print(f"  Final length: {verify.length:.2f} seconds")

if __name__ == "__main__":
    if len(sys.argv) < 3:
        print(__doc__)
        sys.exit(1)

    output_path = sys.argv[1]
    input_paths = sys.argv[2:]

    merge_midi_files(output_path, input_paths)
