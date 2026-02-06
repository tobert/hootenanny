"""
Shared audio utilities for hootpy model services.

Provides consistent WAV encoding/decoding, resampling, normalization,
and shape manipulation. All audio is represented as float32 numpy arrays
in the range [-1.0, 1.0].

Uses Python's built-in wave module for I/O (no torchaudio dependency)
and scipy for resampling.
"""

import io
import wave

import numpy as np


def decode_wav(data: bytes) -> tuple[np.ndarray, int]:
    """
    Decode WAV bytes to a float32 numpy array and sample rate.

    Returns mono audio in [-1.0, 1.0] range.
    Handles 8-bit, 16-bit, 24-bit, and 32-bit WAV files.
    Multi-channel audio is mixed down to mono.
    """
    buffer = io.BytesIO(data)
    with wave.open(buffer, "rb") as wav:
        sample_rate = wav.getframerate()
        n_channels = wav.getnchannels()
        sample_width = wav.getsampwidth()
        n_frames = wav.getnframes()
        audio_bytes = wav.readframes(n_frames)

    if sample_width == 2:
        audio = np.frombuffer(audio_bytes, dtype=np.int16).astype(np.float32) / 32768.0
    elif sample_width == 4:
        audio = np.frombuffer(audio_bytes, dtype=np.int32).astype(np.float32) / 2147483648.0
    elif sample_width == 3:
        # 24-bit: unpack 3-byte samples to int32
        n_samples = len(audio_bytes) // 3
        audio = np.zeros(n_samples, dtype=np.float32)
        for i in range(n_samples):
            sample = int.from_bytes(audio_bytes[i * 3 : i * 3 + 3], byteorder="little", signed=True)
            audio[i] = sample / 8388608.0
    elif sample_width == 1:
        audio = np.frombuffer(audio_bytes, dtype=np.uint8).astype(np.float32) / 128.0 - 1.0
    else:
        raise ValueError(f"Unsupported sample width: {sample_width}")

    if n_channels > 1:
        audio = to_mono(audio.reshape(-1, n_channels))

    return audio, sample_rate


def encode_wav(audio: np.ndarray, sample_rate: int, channels: int = 1) -> bytes:
    """
    Encode a float32 numpy array to WAV bytes (16-bit PCM).

    Input audio should be in [-1.0, 1.0] range.
    For multi-channel, audio shape should be (samples, channels).
    """
    if audio.ndim > 1 and channels == 1:
        audio = to_mono(audio)

    if audio.ndim == 1:
        audio = audio.reshape(-1)
    else:
        channels = audio.shape[-1]
        audio = audio.reshape(-1)

    audio = np.clip(audio, -1.0, 1.0)
    audio_int16 = (audio * 32767).astype(np.int16)

    buffer = io.BytesIO()
    with wave.open(buffer, "wb") as wav:
        wav.setnchannels(channels)
        wav.setsampwidth(2)
        wav.setframerate(sample_rate)
        wav.writeframes(audio_int16.tobytes())

    return buffer.getvalue()


def resample(audio: np.ndarray, orig_sr: int, target_sr: int) -> np.ndarray:
    """
    Resample audio from orig_sr to target_sr using scipy.

    Returns a new array at the target sample rate.
    No-op if sample rates match.
    """
    if orig_sr == target_sr:
        return audio

    from scipy.signal import resample_poly
    from math import gcd

    g = gcd(orig_sr, target_sr)
    up = target_sr // g
    down = orig_sr // g

    return resample_poly(audio, up, down).astype(np.float32)


def normalize_audio(audio: np.ndarray, target_db: float = -1.0) -> np.ndarray:
    """
    Peak-normalize audio to a target dB level.

    Default target is -1.0 dBFS (just below clipping).
    """
    peak = np.abs(audio).max()
    if peak == 0:
        return audio

    target_amplitude = 10.0 ** (target_db / 20.0)
    return (audio * (target_amplitude / peak)).astype(np.float32)


def to_mono(audio: np.ndarray) -> np.ndarray:
    """
    Convert multi-channel audio to mono by averaging channels.

    Input shape: (samples, channels) or (samples,)
    Output shape: (samples,)
    """
    if audio.ndim == 1:
        return audio
    return audio.mean(axis=-1).astype(np.float32)


def ensure_shape(audio: np.ndarray, target_dims: int) -> np.ndarray:
    """
    Reshape audio tensor to target dimensionality.

    Common shapes:
    - 1D: (samples,) - raw waveform
    - 2D: (channels, samples) - torch convention
    - 3D: (batch, channels, samples) - batched torch convention
    """
    if audio.ndim == target_dims:
        return audio

    if target_dims == 1:
        return audio.flatten()
    elif target_dims == 2:
        if audio.ndim == 1:
            return audio[np.newaxis, :]  # (1, samples)
        elif audio.ndim == 3:
            return audio.squeeze(0)  # remove batch
    elif target_dims == 3:
        if audio.ndim == 1:
            return audio[np.newaxis, np.newaxis, :]  # (1, 1, samples)
        elif audio.ndim == 2:
            return audio[np.newaxis, :, :]  # (1, channels, samples)

    raise ValueError(f"Cannot reshape {audio.ndim}D to {target_dims}D")
