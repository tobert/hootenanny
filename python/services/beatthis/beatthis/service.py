"""
Beat-this service implementation.

Provides beat and downbeat detection using the CPJKU/beat_this model.

Audio requirements (validated by this service, conversion done by Rust):
- Sample rate: 22050 Hz (exact)
- Channels: Mono
- Max duration: 30 seconds
"""

from __future__ import annotations

import asyncio
import io
import logging
import os
import wave
from typing import Any

import numpy as np
import torch

from hootpy import ModelService, ServiceConfig, NotFoundError, ValidationError, cas

log = logging.getLogger(__name__)

# Model constants
REQUIRED_SAMPLE_RATE = 22050
MAX_DURATION_SECONDS = 30.0
FRAME_RATE = 50  # fps for beat_this output

# Default endpoint (IPC socket)
DEFAULT_ENDPOINT = f"ipc://{os.environ.get('XDG_RUNTIME_DIR', os.path.expanduser('~/.hootenanny/run'))}/hootenanny/beatthis.sock"


def decode_wav(data: bytes) -> tuple[np.ndarray, int]:
    """Decode WAV bytes to numpy array and sample rate."""
    buffer = io.BytesIO(data)
    with wave.open(buffer, "rb") as wav:
        sample_rate = wav.getframerate()
        n_channels = wav.getnchannels()
        sample_width = wav.getsampwidth()
        n_frames = wav.getnframes()
        audio_bytes = wav.readframes(n_frames)

    # Convert to float32 based on sample width
    if sample_width == 2:  # 16-bit
        audio_np = np.frombuffer(audio_bytes, dtype=np.int16).astype(np.float32) / 32768.0
    elif sample_width == 4:  # 32-bit
        audio_np = np.frombuffer(audio_bytes, dtype=np.int32).astype(np.float32) / 2147483648.0
    else:  # 8-bit or other
        audio_np = np.frombuffer(audio_bytes, dtype=np.uint8).astype(np.float32) / 128.0 - 1.0

    # Convert stereo to mono if needed
    if n_channels > 1:
        audio_np = audio_np.reshape(-1, n_channels).mean(axis=1)

    return audio_np, sample_rate


def pick_peaks(probs: np.ndarray, threshold: float = 0.5) -> np.ndarray:
    """Pick peaks from probability array. Returns times in seconds."""
    above_threshold = probs > threshold
    peaks = []
    neighborhood = 7  # ±70ms at 50fps

    for i in range(len(probs)):
        if not above_threshold[i]:
            continue

        start = max(0, i - neighborhood)
        end = min(len(probs), i + neighborhood + 1)

        if probs[i] == probs[start:end].max():
            peaks.append(i / FRAME_RATE)

    return np.array(peaks)


def estimate_bpm(beats: np.ndarray) -> float | None:
    """Estimate BPM from beat times."""
    if len(beats) < 2:
        return None
    intervals = np.diff(beats)
    median_interval = np.median(intervals)
    return round(60.0 / median_interval, 1) if median_interval > 0 else None


class BeatthisService(ModelService):
    """
    Beat-this beat/downbeat detection service.

    Tool:
    - beatthis_analyze: Detect beats and downbeats in audio

    Audio requirements (Rust side handles conversion):
    - Sample rate: 22050 Hz
    - Channels: Mono
    - Max duration: 30 seconds
    """

    TOOLS = ["beatthis_analyze"]

    RESPONSE_TYPES = {
        "beatthis_analyze": "beats_analyzed",
    }

    def __init__(self, endpoint: str | None = None):
        super().__init__(
            ServiceConfig(
                name="beatthis",
                endpoint=endpoint or os.environ.get("BEATTHIS_ENDPOINT", DEFAULT_ENDPOINT),
            )
        )
        self.model = None
        self.device = torch.device("cuda" if torch.cuda.is_available() else "cpu")
        self.model_name = os.environ.get("BEAT_THIS_MODEL", "final0")

    async def load_model(self):
        """Load beat_this model at startup."""
        log.info(f"Loading beat_this model '{self.model_name}' on {self.device}...")

        from beat_this.inference import Audio2Frames

        self.model = Audio2Frames(checkpoint_path=self.model_name, device=str(self.device))
        log.info(
            f"beat_this loaded. Requirements: {REQUIRED_SAMPLE_RATE}Hz mono, max {MAX_DURATION_SECONDS}s"
        )

    def get_response_type(self, tool_name: str) -> str:
        """Get the response type name for a tool."""
        return self.RESPONSE_TYPES.get(tool_name, tool_name + "_response")

    async def handle_request(self, tool_name: str, params: dict[str, Any]) -> dict[str, Any]:
        """Route request to appropriate handler."""
        match tool_name:
            case "beatthis_analyze":
                return await self._analyze(params)
            case _:
                raise ValidationError(message=f"Unknown tool: {tool_name}")

    async def _analyze(self, params: dict[str, Any]) -> dict[str, Any]:
        """Detect beats and downbeats in audio."""
        audio_hash = params.get("audio_hash")
        if not audio_hash:
            raise ValidationError(message="audio_hash is required", field_name="audio_hash")

        # Fetch audio from CAS
        audio_data = cas.fetch(audio_hash)
        if audio_data is None:
            raise NotFoundError(
                message=f"Audio not found in CAS: {audio_hash}",
                resource_type="audio",
                resource_id=audio_hash,
            )

        log.info(f"Fetched {len(audio_data)} bytes from CAS: {audio_hash}")

        # Decode WAV
        audio, sample_rate = await asyncio.to_thread(decode_wav, audio_data)

        # Validate audio requirements
        if sample_rate != REQUIRED_SAMPLE_RATE:
            raise ValidationError(
                message=f"Sample rate must be {REQUIRED_SAMPLE_RATE}Hz, got {sample_rate}Hz. "
                "Rust side should resample before calling this service.",
                field_name="audio_hash",
            )

        if audio.ndim != 1:
            raise ValidationError(
                message=f"Audio must be mono (1D array), got shape {audio.shape}",
                field_name="audio_hash",
            )

        duration = len(audio) / sample_rate
        if duration > MAX_DURATION_SECONDS:
            raise ValidationError(
                message=f"Duration exceeds {MAX_DURATION_SECONDS}s limit: {duration:.1f}s",
                field_name="audio_hash",
            )

        # Run inference
        beat_probs, downbeat_probs = await asyncio.to_thread(
            self._run_inference, audio, sample_rate
        )

        # Peak picking
        beats = pick_peaks(beat_probs)
        downbeats = pick_peaks(downbeat_probs)

        # BPM estimation
        bpm = estimate_bpm(beats)

        log.info(
            f"Detected {len(beats)} beats, {len(downbeats)} downbeats, "
            f"BPM: {bpm if bpm else 'N/A'}"
        )

        return {
            "beats": beats.tolist(),
            "downbeats": downbeats.tolist(),
            "bpm": bpm,
            "num_beats": len(beats),
            "num_downbeats": len(downbeats),
            "duration_seconds": duration,
            "frame_rate": FRAME_RATE,
            "beat_probs": beat_probs.tolist(),
            "downbeat_probs": downbeat_probs.tolist(),
        }

    def _run_inference(
        self, audio: np.ndarray, sample_rate: int
    ) -> tuple[np.ndarray, np.ndarray]:
        """Run beat detection (blocking, for use with asyncio.to_thread)."""
        audio_tensor = torch.from_numpy(audio).float()

        with torch.inference_mode():
            beat_logits, downbeat_logits = self.model(audio_tensor, sample_rate)

        beat_probs = torch.sigmoid(beat_logits).cpu().numpy()
        downbeat_probs = torch.sigmoid(downbeat_logits).cpu().numpy()

        return beat_probs, downbeat_probs


async def main():
    """Run the Beat-this service."""
    import sys

    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s %(levelname)s %(name)s: %(message)s",
    )

    endpoint = os.environ.get("BEATTHIS_ENDPOINT", DEFAULT_ENDPOINT)
    if len(sys.argv) > 1:
        endpoint = sys.argv[1]

    service = BeatthisService(endpoint=endpoint)
    await service.start()


if __name__ == "__main__":
    asyncio.run(main())
