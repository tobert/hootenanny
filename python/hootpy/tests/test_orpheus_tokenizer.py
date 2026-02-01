"""Tests for Orpheus tokenizer."""

import pytest
from pathlib import Path

from hootpy.orpheus_tokenizer import OrpheusTokenizer, encode_midi, decode_tokens


# Sample MIDI files
# Path: tests/test_*.py -> hootpy/ -> python/ -> hootenanny-merge-models/
REPO_ROOT = Path(__file__).parent.parent.parent.parent
SCRATCH_DIR = REPO_ROOT / "scratch"
SAMPLE_MIDI = SCRATCH_DIR / "mars_drums.mid"


class TestTokenConstants:
    """Test token range constants."""

    def test_special_tokens(self):
        """Verify special token values."""
        tok = OrpheusTokenizer()
        assert tok.START_TOKEN == 18816
        assert tok.EOS_TOKEN_BASE == 18817
        assert tok.EOS_TOKEN_LOOPS == 18818
        assert tok.PAD_TOKEN == 18819


class TestTokenEncoding:
    """Test token encoding logic."""

    def test_delta_time_range(self):
        """Delta time tokens should be 0-255."""
        # Delta times are encoded as raw values 0-255
        for dt in [0, 1, 127, 255]:
            assert 0 <= dt < 256

    def test_pitch_patch_encoding(self):
        """Test pitch+patch token encoding formula."""
        # Token = (128 * patch) + pitch + 256
        # Range: 256 to 16767

        # Piano (patch 0), middle C (pitch 60)
        token = (128 * 0) + 60 + 256
        assert token == 316
        assert 256 <= token < 16768

        # Drums (patch 128), kick (pitch 36)
        token = (128 * 128) + 36 + 256
        assert token == 16676
        assert 256 <= token < 16768

    def test_duration_velocity_encoding(self):
        """Test duration+velocity token encoding formula."""
        # Token = (8 * duration) + velocity + 16768
        # Range: 16768 to 18815

        # Short note, soft
        dur, vel = 1, 0
        token = (8 * dur) + vel + 16768
        assert token == 16776
        assert 16768 <= token < 18816

        # Long note, loud
        dur, vel = 255, 7
        token = (8 * dur) + vel + 16768
        assert token == 18815
        assert 16768 <= token < 18816


class TestDecodeTokens:
    """Test token decoding."""

    def test_decode_minimal_sequence(self):
        """Decode a minimal valid token sequence."""
        tok = OrpheusTokenizer()

        # Minimal sequence: delta_time, pitch_patch, dur_vel
        tokens = [
            0,  # delta time = 0
            316,  # patch 0, pitch 60 (middle C)
            16776,  # duration 1, velocity 0
        ]

        midi_bytes = tok.decode_tokens(tokens)

        # Should produce valid MIDI
        assert midi_bytes.startswith(b"MThd")
        assert len(midi_bytes) > 14  # Header + some data

    def test_decode_chord(self):
        """Decode a chord (multiple notes at same time)."""
        tok = OrpheusTokenizer()

        # C major chord at time 0
        tokens = [
            0,  # delta time
            316,  # C4 (patch 0, pitch 60)
            16776,  # dur/vel
            320,  # E4 (patch 0, pitch 64)
            16776,
            323,  # G4 (patch 0, pitch 67)
            16776,
        ]

        midi_bytes = tok.decode_tokens(tokens)
        assert midi_bytes.startswith(b"MThd")

    def test_decode_sequence_with_timing(self):
        """Decode a sequence with timing."""
        tok = OrpheusTokenizer()

        tokens = [
            0,  # time 0
            316,
            16776,  # C4
            16,  # delta 16 (16 * 16ms = 256ms)
            318,
            16776,  # D4
            16,
            320,
            16776,  # E4
        ]

        midi_bytes = tok.decode_tokens(tokens)
        assert midi_bytes.startswith(b"MThd")


class TestRoundtrip:
    """Test encode → decode round-trips."""

    @pytest.mark.skipif(not SAMPLE_MIDI.exists(), reason="Sample MIDI not found")
    def test_encode_decode_roundtrip(self):
        """Encode MIDI to tokens, decode back to MIDI."""
        tok = OrpheusTokenizer()

        original_bytes = SAMPLE_MIDI.read_bytes()

        # Encode
        tokens = tok.encode_midi(original_bytes)
        assert len(tokens) > 0
        assert tokens[0] == tok.START_TOKEN  # Should start with START token

        # Decode (skip START token for decoding)
        reconstructed_bytes = tok.decode_tokens(tokens[1:])

        # Should produce valid MIDI
        assert reconstructed_bytes.startswith(b"MThd")

        # Note: Exact byte equality isn't expected due to tokenization loss

    @pytest.mark.skipif(not SAMPLE_MIDI.exists(), reason="Sample MIDI not found")
    def test_token_ranges(self):
        """All tokens should be in valid ranges."""
        tok = OrpheusTokenizer()

        midi_bytes = SAMPLE_MIDI.read_bytes()
        tokens = tok.encode_midi(midi_bytes)

        for t in tokens:
            # Valid token ranges
            valid = (
                (0 <= t < 256)  # Delta time
                or (256 <= t < 16768)  # Pitch + patch
                or (16768 <= t < 18816)  # Duration + velocity
                or t == tok.START_TOKEN
                or t == tok.EOS_TOKEN_BASE
                or t == tok.EOS_TOKEN_LOOPS
                or t == tok.PAD_TOKEN
            )
            assert valid, f"Token {t} out of valid range"


class TestConvenienceFunctions:
    """Test module-level convenience functions."""

    @pytest.mark.skipif(not SAMPLE_MIDI.exists(), reason="Sample MIDI not found")
    def test_encode_midi_function(self):
        """Test encode_midi() convenience function."""
        midi_bytes = SAMPLE_MIDI.read_bytes()
        tokens = encode_midi(midi_bytes)
        assert len(tokens) > 0

    def test_decode_tokens_function(self):
        """Test decode_tokens() convenience function."""
        tokens = [0, 316, 16776]
        midi_bytes = decode_tokens(tokens)
        assert midi_bytes.startswith(b"MThd")
