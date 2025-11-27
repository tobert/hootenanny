#!/bin/bash
# abc_to_midi.sh - Convert ABC notation to MIDI
# Usage: ./abc_to_midi.sh input.abc [output.mid]
#
# This script converts ABC notation files to MIDI using abc2midi
# from the abcmidi package.
#
# ABC notation is a text-based music notation format. See:
# https://abcnotation.com/wiki/abc:standard:v2.1
#
# GM Drum channel is 10 (0-indexed: 9)
# Use %%MIDI channel 10 in ABC to write drums
#
# Example ABC drum notation:
#   C,  = Bass Drum 1 (note 36)
#   D,  = Snare (note 38)
#   ^F, = Closed Hi-Hat (note 42)
#   ^C  = Crash (note 49)

set -euo pipefail

INPUT="${1:-}"
OUTPUT="${2:-}"

if [[ -z "$INPUT" ]]; then
    echo "Usage: $0 input.abc [output.mid]"
    echo ""
    echo "Converts ABC notation to MIDI using abc2midi"
    exit 1
fi

if [[ ! -f "$INPUT" ]]; then
    echo "Error: File not found: $INPUT"
    exit 1
fi

# Default output: same name with .mid extension
if [[ -z "$OUTPUT" ]]; then
    OUTPUT="${INPUT%.abc}.mid"
fi

echo "Converting: $INPUT -> $OUTPUT"
abc2midi "$INPUT" -o "$OUTPUT"

if [[ -f "$OUTPUT" ]]; then
    echo "Success! Created: $OUTPUT"
    echo ""
    echo "File info:"
    ls -la "$OUTPUT"
    echo ""
    echo "Preview with abc2abc:"
    midi2abc "$OUTPUT" 2>&1 | head -20
else
    echo "Error: Conversion failed"
    exit 1
fi
