"""
AudioLDM2 service implementation.

Provides text-to-audio generation using a diffusion pipeline
over the HOOT01/ZMQ protocol.

Model: cvssp/audioldm2 (~6GB VRAM, 16kHz output)
"""

import asyncio
import logging
import os
from typing import Any

import numpy as np
import torch

from hootpy import ModelService, ServiceConfig, ValidationError, cas
from hootpy.audio import encode_wav

log = logging.getLogger(__name__)

# Default IPC socket
def _socket_dir() -> str:
    xdg = os.environ.get("XDG_RUNTIME_DIR")
    if xdg:
        return f"{xdg}/hootenanny"
    return os.path.expanduser("~/.hootenanny/run")

DEFAULT_ENDPOINT = f"ipc://{_socket_dir()}/audioldm2.sock"

MODEL_ID = "cvssp/audioldm2"
SAMPLE_RATE = 16000


class Audioldm2Service(ModelService):
    """
    AudioLDM2 text-to-audio diffusion service.

    Tool:
    - audioldm2_generate: Generate audio from text prompt using diffusion
    """

    TOOLS = ["audioldm2_generate"]

    RESPONSE_TYPES = {
        "audioldm2_generate": "audioldm2_generated",
    }

    def __init__(self, endpoint: str | None = None):
        super().__init__(ServiceConfig(
            name="audioldm2",
            endpoint=endpoint or os.environ.get("AUDIOLDM2_ENDPOINT", DEFAULT_ENDPOINT),
        ))
        self.pipe = None
        self.device = torch.device("cuda" if torch.cuda.is_available() else "cpu")

    async def load_model(self):
        """Load AudioLDM2 pipeline at startup."""
        log.info(f"Loading AudioLDM2 model '{MODEL_ID}' on {self.device}...")

        from diffusers import AudioLDM2Pipeline
        from transformers import GPT2Model, GPT2LMHeadModel

        self.pipe = await asyncio.to_thread(
            AudioLDM2Pipeline.from_pretrained,
            MODEL_ID,
            torch_dtype=torch.float16,
        )

        # Workaround: cvssp/audioldm2 ships GPT2Model but needs GPT2LMHeadModel
        if isinstance(self.pipe.language_model, GPT2Model):
            log.info("Patching language model: GPT2Model -> GPT2LMHeadModel")
            lm_head = await asyncio.to_thread(
                GPT2LMHeadModel.from_pretrained,
                MODEL_ID,
                subfolder="language_model",
                torch_dtype=torch.float16,
            )
            self.pipe.language_model = lm_head

        self.pipe = self.pipe.to(self.device)
        self.pipe.enable_attention_slicing()

        log.info(f"AudioLDM2 loaded. Output: {SAMPLE_RATE}Hz")

    def get_response_type(self, tool_name: str) -> str:
        return self.RESPONSE_TYPES.get(tool_name, tool_name + "_response")

    async def handle_request(self, tool_name: str, params: dict[str, Any]) -> dict[str, Any]:
        match tool_name:
            case "audioldm2_generate":
                return await self._generate(params)
            case _:
                raise ValidationError(message=f"Unknown tool: {tool_name}")

    async def _generate(self, params: dict[str, Any]) -> dict[str, Any]:
        """Generate audio from text prompt using diffusion."""
        prompt = params.get("prompt", "ambient soundscape")
        negative_prompt = params.get("negative_prompt", "Low quality, distorted, noise.")
        duration = params.get("duration", 5.0)
        num_inference_steps = params.get("num_inference_steps", 200)
        guidance_scale = params.get("guidance_scale", 3.5)
        seed = params.get("seed")

        # AudioLDM2 generates ~1s per waveform at 16kHz
        # audio_length_in_s controls the output duration
        log.info(
            f"Generating: prompt='{prompt[:60]}...', "
            f"duration={duration}s, steps={num_inference_steps}"
        )

        generator = None
        if seed is not None:
            generator = torch.Generator(device=self.device).manual_seed(int(seed))

        # Run diffusion pipeline
        with torch.inference_mode():
            result = await asyncio.to_thread(
                self.pipe,
                prompt=prompt,
                negative_prompt=negative_prompt,
                audio_length_in_s=duration,
                num_inference_steps=num_inference_steps,
                guidance_scale=guidance_scale,
                generator=generator,
            )

        # Extract audio: result.audios shape is (batch, samples)
        audio_np = result.audios[0]
        if audio_np.ndim > 1:
            audio_np = audio_np.flatten()
        audio_np = audio_np.astype(np.float32)

        actual_duration = len(audio_np) / SAMPLE_RATE

        # Encode to WAV and store in CAS
        wav_bytes = encode_wav(audio_np, SAMPLE_RATE)
        content_hash = cas.store(wav_bytes)

        log.info(f"Generated {actual_duration:.1f}s audio, CAS: {content_hash}")

        return {
            "artifact_id": "",
            "content_hash": content_hash,
            "duration_seconds": actual_duration,
            "sample_rate": SAMPLE_RATE,
            "prompt": prompt,
        }


async def main():
    """Run the AudioLDM2 service."""
    import sys

    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s %(levelname)s %(name)s: %(message)s",
    )

    endpoint = os.environ.get("AUDIOLDM2_ENDPOINT", DEFAULT_ENDPOINT)
    if len(sys.argv) > 1:
        endpoint = sys.argv[1]

    service = Audioldm2Service(endpoint=endpoint)
    await service.start()


if __name__ == "__main__":
    asyncio.run(main())
