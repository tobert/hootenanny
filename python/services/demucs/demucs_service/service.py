"""
Demucs audio source separation service implementation.

Provides stem separation (drums, bass, vocals, other) using
Meta's Demucs model over HOOT01/ZMQ.

Models: htdemucs, htdemucs_ft, htdemucs_6s (~3GB VRAM)
"""

import asyncio
import logging
import os
from typing import Any

import numpy as np
import torch

from hootpy import ModelService, ServiceConfig, NotFoundError, ValidationError, cas
from hootpy.audio import decode_wav, encode_wav, resample

log = logging.getLogger(__name__)

# Enable experimental ROCm attention kernels
os.environ.setdefault("TORCH_ROCM_AOTRITON_ENABLE_EXPERIMENTAL", "1")

# Default IPC socket
def _socket_dir() -> str:
    xdg = os.environ.get("XDG_RUNTIME_DIR")
    if xdg:
        return f"{xdg}/hootenanny"
    return os.path.expanduser("~/.hootenanny/run")

DEFAULT_ENDPOINT = f"ipc://{_socket_dir()}/demucs.sock"

DEFAULT_MODEL = "htdemucs"
DEMUCS_SAMPLE_RATE = 44100

# Standard stem names per model
STEMS_4 = ["drums", "bass", "other", "vocals"]
STEMS_6 = ["drums", "bass", "other", "vocals", "guitar", "piano"]


class DemucsService(ModelService):
    """
    Demucs audio source separation service.

    Tool:
    - demucs_separate: Separate audio into stems
    """

    TOOLS = ["demucs_separate"]

    RESPONSE_TYPES = {
        "demucs_separate": "demucs_separated",
    }

    def __init__(self, endpoint: str | None = None):
        super().__init__(ServiceConfig(
            name="demucs",
            endpoint=endpoint or os.environ.get("DEMUCS_ENDPOINT", DEFAULT_ENDPOINT),
        ))
        self.models: dict[str, Any] = {}
        self.device = torch.device("cuda" if torch.cuda.is_available() else "cpu")

    async def load_model(self):
        """Pre-load the default model at startup."""
        log.info(f"Loading Demucs model '{DEFAULT_MODEL}' on {self.device}...")
        await asyncio.to_thread(self._load_model, DEFAULT_MODEL)
        log.info(f"Demucs {DEFAULT_MODEL} loaded. Requires {DEMUCS_SAMPLE_RATE}Hz input.")

    def _load_model(self, name: str):
        """Load a Demucs model by name."""
        if name in self.models:
            return self.models[name]

        from demucs.pretrained import get_model

        model = get_model(name)
        model.to(self.device)
        model.eval()

        self.models[name] = model
        log.info(f"Loaded model: {name}")
        return model

    def _get_model(self, name: str | None):
        if not name:
            name = DEFAULT_MODEL
        if name not in self.models:
            self._load_model(name)
        return self.models[name], name

    def get_response_type(self, tool_name: str) -> str:
        return self.RESPONSE_TYPES.get(tool_name, tool_name + "_response")

    async def handle_request(self, tool_name: str, params: dict[str, Any]) -> dict[str, Any]:
        match tool_name:
            case "demucs_separate":
                return await self._separate(params)
            case _:
                raise ValidationError(message=f"Unknown tool: {tool_name}")

    async def _separate(self, params: dict[str, Any]) -> dict[str, Any]:
        """Separate audio into stems."""
        from demucs.apply import apply_model

        audio_hash = params.get("audio_hash")
        if not audio_hash:
            raise ValidationError(message="audio_hash is required", field_name="audio_hash")

        model_name = params.get("model")
        stems_filter = params.get("stems", [])
        two_stems = params.get("two_stems")

        model, actual_model = self._get_model(model_name)

        # Fetch and decode audio
        audio_data = cas.fetch(audio_hash)
        if audio_data is None:
            raise NotFoundError(
                message=f"Audio not found in CAS: {audio_hash}",
                resource_type="audio",
                resource_id=audio_hash,
            )

        audio, sample_rate = await asyncio.to_thread(decode_wav, audio_data)

        # Resample to 44.1kHz if needed
        if sample_rate != DEMUCS_SAMPLE_RATE:
            audio = await asyncio.to_thread(resample, audio, sample_rate, DEMUCS_SAMPLE_RATE)

        # Demucs expects stereo (2, samples). If mono, duplicate.
        audio_tensor = torch.from_numpy(audio).float()
        if audio_tensor.dim() == 1:
            audio_tensor = audio_tensor.unsqueeze(0).repeat(2, 1)  # mono -> stereo
        elif audio_tensor.shape[0] == 1:
            audio_tensor = audio_tensor.repeat(2, 1)

        # Normalize
        ref = audio_tensor.mean(0)
        audio_tensor = (audio_tensor - ref.mean()) / (ref.std() + 1e-8)

        duration_seconds = audio_tensor.shape[1] / DEMUCS_SAMPLE_RATE
        log.info(
            f"Separating {duration_seconds:.1f}s audio with {actual_model} "
            f"(stems_filter={stems_filter}, two_stems={two_stems})"
        )

        # Apply model
        with torch.no_grad(), torch.autocast(device_type="cuda", dtype=torch.float16):
            sources = await asyncio.to_thread(
                apply_model,
                model,
                audio_tensor[None],
                device=self.device,
                segment=7.8,
                overlap=0.25,
                split=True,
                progress=False,
            )

        # Denormalize
        sources = sources * (ref.std() + 1e-8) + ref.mean()

        # Get stem names
        stem_names = STEMS_6 if "6s" in actual_model else STEMS_4

        # Process stems
        stem_results = []

        if two_stems:
            # Karaoke mode: target stem + accompaniment
            target_idx = stem_names.index(two_stems) if two_stems in stem_names else 0
            target = sources[0, target_idx]
            accompaniment = sources[0].sum(dim=0) - target

            for name, audio_out in [(two_stems, target), ("accompaniment", accompaniment)]:
                stem_np = audio_out.cpu().numpy()
                # Mix to mono for storage
                if stem_np.ndim > 1:
                    stem_np = stem_np.mean(axis=0)
                wav_bytes = encode_wav(stem_np.astype(np.float32), DEMUCS_SAMPLE_RATE)
                stem_hash = cas.store(wav_bytes)
                stem_results.append({
                    "name": name,
                    "content_hash": stem_hash,
                    "duration_seconds": duration_seconds,
                })
        else:
            for i, name in enumerate(stem_names):
                if stems_filter and name not in stems_filter:
                    continue

                stem_audio = sources[0, i]
                stem_np = stem_audio.cpu().numpy()
                # Mix to mono for storage
                if stem_np.ndim > 1:
                    stem_np = stem_np.mean(axis=0)
                wav_bytes = encode_wav(stem_np.astype(np.float32), DEMUCS_SAMPLE_RATE)
                stem_hash = cas.store(wav_bytes)
                stem_results.append({
                    "name": name,
                    "content_hash": stem_hash,
                    "duration_seconds": duration_seconds,
                })

        log.info(f"Separated into {len(stem_results)} stems")

        return {
            "stems": stem_results,
            "model": actual_model,
            "duration_seconds": duration_seconds,
        }


async def main():
    """Run the Demucs service."""
    import sys

    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s %(levelname)s %(name)s: %(message)s",
    )

    endpoint = os.environ.get("DEMUCS_ENDPOINT", DEFAULT_ENDPOINT)
    if len(sys.argv) > 1:
        endpoint = sys.argv[1]

    service = DemucsService(endpoint=endpoint)
    await service.start()


if __name__ == "__main__":
    asyncio.run(main())
