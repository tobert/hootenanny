"""
YuE text-to-song generation service.

Generates full songs from lyrics + genre tags using YuE's dual-stage
model (7B semantic + 1B acoustic) via yue-inference. Audio is stored
directly in CAS — no base64, no HTTP.

Model: m-a-p/YuE-s1-7B + YuE-s2-1B (~15-24GB VRAM)
"""

import asyncio
import logging
import os
from typing import Any

from hootpy import ModelService, ServiceConfig, ValidationError, cas
from hootpy.audio import encode_wav

log = logging.getLogger(__name__)


def _socket_dir() -> str:
    xdg = os.environ.get("XDG_RUNTIME_DIR")
    if xdg:
        return f"{xdg}/hootenanny"
    return os.path.expanduser("~/.hootenanny/run")


DEFAULT_ENDPOINT = f"ipc://{_socket_dir()}/yue.sock"
DEVICE = "cuda"


class YueService(ModelService):
    """
    YuE text-to-song generation service.

    Tool:
    - yue_generate: Generate a full song from lyrics and genre tags
    """

    TOOLS = ["yue_generate"]

    RESPONSE_TYPES = {
        "yue_generate": "audio_generated",
    }

    def __init__(self, endpoint: str | None = None):
        super().__init__(ServiceConfig(
            name="yue",
            endpoint=endpoint or os.environ.get("YUE_ENDPOINT", DEFAULT_ENDPOINT),
        ))
        self.model = None

    async def load_model(self):
        """Load YuE model at startup."""
        log.info(f"Loading YuE model on {DEVICE}...")

        from yue_inference import YuE

        self.model = await asyncio.to_thread(
            YuE.from_pretrained, device=DEVICE
        )

        log.info("YuE model loaded")

    def get_response_type(self, tool_name: str) -> str:
        return self.RESPONSE_TYPES.get(tool_name, tool_name + "_response")

    async def handle_request(self, tool_name: str, params: dict[str, Any]) -> dict[str, Any]:
        match tool_name:
            case "yue_generate":
                return await self._generate(params)
            case _:
                raise ValidationError(message=f"Unknown tool: {tool_name}")

    async def _generate(self, params: dict[str, Any]) -> dict[str, Any]:
        """Generate a song from lyrics and genre."""
        lyrics = params.get("lyrics", "")
        if not lyrics.strip():
            raise ValidationError(message="lyrics is required")

        genre = params.get("genre", "pop")
        max_new_tokens = params.get("max_new_tokens", 3000)
        run_n_segments = params.get("run_n_segments", 2)
        seed = params.get("seed", 0)

        log.info(
            f"Generating: genre='{genre}', "
            f"lyrics_len={len(lyrics)}, tokens={max_new_tokens}, "
            f"segments={run_n_segments}, seed={seed}"
        )

        audio = await asyncio.to_thread(
            self.model.generate,
            lyrics=lyrics,
            genre=genre,
            max_tokens=max_new_tokens,
            run_n_segments=run_n_segments,
            seed=seed,
        )

        # Encode to WAV and store in CAS
        wav_bytes = encode_wav(audio.samples, audio.sample_rate)
        content_hash = cas.store(wav_bytes)

        log.info(
            f"Generated {audio.duration_seconds:.1f}s song, "
            f"CAS: {content_hash}"
        )

        return {
            "content_hash": content_hash,
            "duration_seconds": audio.duration_seconds,
            "sample_rate": audio.sample_rate,
        }


async def main():
    """Run the YuE service."""
    import sys

    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s %(levelname)s %(name)s: %(message)s",
    )

    endpoint = os.environ.get("YUE_ENDPOINT", DEFAULT_ENDPOINT)
    if len(sys.argv) > 1:
        endpoint = sys.argv[1]

    service = YueService(endpoint=endpoint)
    await service.start()


if __name__ == "__main__":
    asyncio.run(main())
