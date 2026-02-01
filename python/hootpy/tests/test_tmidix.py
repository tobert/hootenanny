"""Tests for tmidix MIDI utilities."""

import pytest
from pathlib import Path

from hootpy import tmidix


# Sample MIDI files in scratch/
# Path: tests/test_tmidix.py -> hootpy/ -> python/ -> hootenanny-merge-models/
REPO_ROOT = Path(__file__).parent.parent.parent.parent
SCRATCH_DIR = REPO_ROOT / "scratch"
SAMPLE_MIDI = SCRATCH_DIR / "mars_drums.mid"


class TestMidiRoundtrip:
    """Test MIDI encoding/decoding round-trips."""

    def test_score2midi_opus2score_roundtrip(self):
        """Test score → opus → score preserves structure."""
        # Create a simple score
        score = [
            480,  # ticks per quarter
            [
                ["note", 0, 480, 0, 60, 100],  # C4, quarter note
                ["note", 480, 480, 0, 64, 100],  # E4
                ["note", 960, 480, 0, 67, 100],  # G4
            ],
        ]

        # Convert to opus and back
        opus = tmidix.score2opus(score)
        score_back = tmidix.opus2score(opus)

        # Should have same structure
        assert score_back[0] == score[0]  # ticks
        assert len(score_back) == 2  # ticks + 1 track

        # Notes should be preserved (order may differ)
        notes = [e for e in score_back[1] if e[0] == "note"]
        assert len(notes) == 3

    def test_score2midi_midi2score_roundtrip(self):
        """Test score → MIDI bytes → score."""
        score = [
            480,
            [
                ["patch_change", 0, 0, 0],  # Piano
                ["note", 0, 240, 0, 60, 80],
                ["note", 240, 240, 0, 62, 80],
                ["note", 480, 480, 0, 64, 80],
            ],
        ]

        midi_bytes = tmidix.score2midi(score)
        assert midi_bytes.startswith(b"MThd")  # Valid MIDI header

        score_back = tmidix.midi2score(midi_bytes)
        assert score_back[0] == 480

        notes = [e for e in score_back[1] if e[0] == "note"]
        assert len(notes) == 3

    def test_opus2midi_midi2opus_roundtrip(self):
        """Test opus → MIDI bytes → opus."""
        opus = [
            480,
            [
                ["note_on", 0, 0, 60, 100],
                ["note_off", 480, 0, 60, 0],
                ["note_on", 0, 0, 64, 100],
                ["note_off", 480, 0, 64, 0],
            ],
        ]

        midi_bytes = tmidix.opus2midi(opus)
        assert midi_bytes.startswith(b"MThd")

        opus_back = tmidix.midi2opus(midi_bytes)
        assert opus_back[0] == 480


class TestChordify:
    """Test chordify_score function."""

    def test_chordify_groups_simultaneous_notes(self):
        """Notes at same time should be grouped into chords."""
        score = [
            480,
            [
                ["note", 0, 480, 0, 60, 100],
                ["note", 0, 480, 0, 64, 100],
                ["note", 0, 480, 0, 67, 100],
                ["note", 480, 480, 0, 62, 100],
            ],
        ]

        chords = tmidix.chordify_score(score)

        assert len(chords) == 2  # Two time points
        assert len(chords[0]) == 3  # First chord has 3 notes
        assert len(chords[1]) == 1  # Second has 1 note


class TestDeltaScore:
    """Test delta time conversion."""

    def test_delta_score_notes(self):
        """Convert absolute times to delta times."""
        notes = [
            ["note", 0, 100, 0, 60, 100, 0],
            ["note", 100, 100, 0, 62, 100, 0],
            ["note", 300, 100, 0, 64, 100, 0],
        ]

        delta = tmidix.delta_score_notes(notes)

        assert delta[0][1] == 0  # First note at time 0
        assert delta[1][1] == 100  # Delta from 0 to 100
        assert delta[2][1] == 200  # Delta from 100 to 300


class TestRealMidi:
    """Tests using real MIDI files."""

    @pytest.mark.skipif(not SAMPLE_MIDI.exists(), reason="Sample MIDI not found")
    def test_load_real_midi(self):
        """Load and parse a real MIDI file."""
        midi_bytes = SAMPLE_MIDI.read_bytes()
        score = tmidix.midi2score(midi_bytes)

        assert score[0] > 0  # Has ticks
        assert len(score) >= 2  # Has at least one track

    @pytest.mark.skipif(not SAMPLE_MIDI.exists(), reason="Sample MIDI not found")
    def test_single_track_ms_score(self):
        """Convert MIDI to single-track millisecond score."""
        ms_score = tmidix.midi2single_track_ms_score(str(SAMPLE_MIDI))

        assert ms_score[0] == 1000  # 1000 ticks = 1 second
        notes = [e for e in ms_score[1] if e[0] == "note"]
        assert len(notes) > 0


class TestHelpers:
    """Test helper functions."""

    def test_ordered_set(self):
        """ordered_set preserves order and removes duplicates."""
        result = tmidix.ordered_set([3, 1, 2, 1, 3, 4])
        assert result == [3, 1, 2, 4]

    def test_pick_peaks_basic(self):
        """Test basic peak detection functionality exists."""
        # This is tested more thoroughly in beat-this tests
        pass
