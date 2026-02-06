"""
MusicGen service implementation.

Provides text-to-music generation using Meta's MusicGen model
over the HOOT01/ZMQ protocol. No HTTP, no base64.

Model: facebook/musicgen-small (~2GB VRAM, 32kHz output)
"""

import asyncio
import logging
import os
from typing import Any

import numpy as np
import torch

from hootpy import ModelService, ServiceConfig, ValidationError, cas
from hootpy.audio import encode_wav, to_mono

log = logging.getLogger(__name__)

# Default IPC socket
def _socket_dir() -> str:
    xdg = os.environ.get("XDG_RUNTIME_DIR")
    if xdg:
        return f"{xdg}/hootenanny"
    return os.path.expanduser("~/.hootenanny/run")

DEFAULT_ENDPOINT = f"ipc://{_socket_dir()}/musicgen.sock"

MODEL_ID = "facebook/musicgen-small"
SAMPLE_RATE = 32000


class MusicgenService(ModelService):
    """
    MusicGen text-to-music generation service.

    Tool:
    - musicgen_generate: Generate audio from text prompt
    """

    TOOLS = ["musicgen_generate"]

    RESPONSE_TYPES = {
        "musicgen_generate": "audio_generated",
    }

    def __init__(self, endpoint: str | None = None):
        super().__init__(ServiceConfig(
            name="musicgen",
            endpoint=endpoint or os.environ.get("MUSICGEN_ENDPOINT", DEFAULT_ENDPOINT),
        ))
        self.model = None
        self.processor = None
        self.device = torch.device("cuda" if torch.cuda.is_available() else "cpu")

    async def load_model(self):
        """Load MusicGen model at startup."""
        log.info(f"Loading MusicGen model '{MODEL_ID}' on {self.device}...")

        from transformers import AutoProcessor, MusicgenForConditionalGeneration, MusicgenConfig

        # Workaround: transformers 4.57+ has wrong config_class
        MusicgenForConditionalGeneration.config_class = MusicgenConfig

        self.processor = await asyncio.to_thread(
            AutoProcessor.from_pretrained, MODEL_ID
        )
        self.model = await asyncio.to_thread(
            MusicgenForConditionalGeneration.from_pretrained, MODEL_ID
        )
        self.model = self.model.to(self.device)
        self.model.eval()

        log.info(f"MusicGen loaded. Output: {SAMPLE_RATE}Hz mono")

    def get_response_type(self, tool_name: str) -> str:
        return self.RESPONSE_TYPES.get(tool_name, tool_name + "_response")

    async def handle_request(self, tool_name: str, params: dict[str, Any]) -> dict[str, Any]:
        match tool_name:
            case "musicgen_generate":
                return await self._generate(params)
            case _:
                raise ValidationError(message=f"Unknown tool: {tool_name}")

    async def _generate(self, params: dict[str, Any]) -> dict[str, Any]:
        """Generate audio from text prompt."""
        prompt = params.get("prompt", "ambient electronic music")
        duration = params.get("duration", 8.0)
        temperature = params.get("temperature", 1.0)
        top_k = params.get("top_k", 250)
        top_p = params.get("top_p", 0.0)
        guidance_scale = params.get("guidance_scale", 3.0)
        do_sample = params.get("do_sample", True)

        # MusicGen uses ~50 tokens per second
        max_new_tokens = int(duration * 50)

        log.info(
            f"Generating: prompt='{prompt[:60]}...', "
            f"duration={duration}s, tokens={max_new_tokens}"
        )

        # Prepare inputs
        inputs = await asyncio.to_thread(
            self.processor,
            text=[prompt],
            padding=True,
            return_tensors="pt",
        )
        inputs = {k: v.to(self.device) for k, v in inputs.items()}

        # Generate
        with torch.inference_mode():
            audio_values = await asyncio.to_thread(
                self.model.generate,
                **inputs,
                max_new_tokens=max_new_tokens,
                temperature=temperature,
                top_k=top_k,
                top_p=top_p if top_p > 0 else None,
                guidance_scale=guidance_scale,
                do_sample=do_sample,
            )

        # Extract audio: shape is (batch, channels, samples)
        audio_np = audio_values[0].cpu().numpy()

        # Ensure mono
        if audio_np.ndim > 1:
            audio_np = to_mono(audio_np.T)  # (channels, samples) -> (samples, channels) -> mono

        actual_duration = len(audio_np) / SAMPLE_RATE

        # Encode to WAV and store in CAS
        wav_bytes = encode_wav(audio_np, SAMPLE_RATE)
        content_hash = cas.store(wav_bytes)

        log.info(f"Generated {actual_duration:.1f}s audio, CAS: {content_hash}")

        return {
            "content_hash": content_hash,
            "duration_seconds": actual_duration,
            "sample_rate": SAMPLE_RATE,
            "prompt": prompt,
        }


async def main():
    """Run the MusicGen service."""
    import sys

    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s %(levelname)s %(name)s: %(message)s",
    )

    endpoint = os.environ.get("MUSICGEN_ENDPOINT", DEFAULT_ENDPOINT)
    if len(sys.argv) > 1:
        endpoint = sys.argv[1]

    service = MusicgenService(endpoint=endpoint)
    await service.start()


if __name__ == "__main__":
    asyncio.run(main())
