"""
CLAP audio analysis service implementation.

Provides audio embeddings, zero-shot classification, similarity comparison,
and genre/mood classification using the CLAP model over HOOT01/ZMQ.

Model: laion/clap-htsat-unfused (~1GB VRAM)
"""

import asyncio
import logging
import os
from typing import Any

import numpy as np
import torch

from hootpy import ModelService, ServiceConfig, NotFoundError, ValidationError, cas
from hootpy.audio import decode_wav, resample

log = logging.getLogger(__name__)

# Default IPC socket
def _socket_dir() -> str:
    xdg = os.environ.get("XDG_RUNTIME_DIR")
    if xdg:
        return f"{xdg}/hootenanny"
    return os.path.expanduser("~/.hootenanny/run")

DEFAULT_ENDPOINT = f"ipc://{_socket_dir()}/clap.sock"

MODEL_ID = "laion/clap-htsat-unfused"
CLAP_SAMPLE_RATE = 48000

# Built-in label sets
GENRE_LABELS = [
    "ambient", "blues", "classical", "country", "dance", "drum and bass",
    "electronic", "folk", "funk", "hip hop", "house", "indie",
    "jazz", "latin", "lo-fi", "metal", "pop", "punk", "r&b",
    "reggae", "rock", "soul", "soundtrack", "techno", "world",
]

MOOD_LABELS = [
    "aggressive", "calm", "dark", "dreamy", "energetic", "epic",
    "ethereal", "funky", "gentle", "happy", "hypnotic", "intense",
    "melancholy", "mysterious", "nostalgic", "peaceful", "playful",
    "powerful", "romantic", "sad", "tense", "upbeat", "warm",
]


class ClapService(ModelService):
    """
    CLAP audio analysis service.

    Tool:
    - clap_analyze: Multi-task audio analysis (embeddings, zero_shot, similarity, genre, mood)
    """

    TOOLS = ["clap_analyze"]

    RESPONSE_TYPES = {
        "clap_analyze": "clap_analyzed",
    }

    # Note: The response type must match the Cap'n Proto schema variant name.
    # "clap_analyzed" maps to "clapAnalyzed" in the schema.

    def __init__(self, endpoint: str | None = None):
        super().__init__(ServiceConfig(
            name="clap",
            endpoint=endpoint or os.environ.get("CLAP_ENDPOINT", DEFAULT_ENDPOINT),
        ))
        self.model = None
        self.processor = None
        self.device = torch.device("cuda" if torch.cuda.is_available() else "cpu")

    async def load_model(self):
        """Load CLAP model at startup."""
        log.info(f"Loading CLAP model '{MODEL_ID}' on {self.device}...")

        from transformers import ClapModel, ClapProcessor

        self.processor = await asyncio.to_thread(
            ClapProcessor.from_pretrained, MODEL_ID
        )
        self.model = await asyncio.to_thread(
            ClapModel.from_pretrained, MODEL_ID
        )
        self.model = self.model.to(self.device)
        self.model.eval()

        log.info("CLAP loaded. Requires 48kHz audio input.")

    def get_response_type(self, tool_name: str) -> str:
        return self.RESPONSE_TYPES.get(tool_name, tool_name + "_response")

    async def handle_request(self, tool_name: str, params: dict[str, Any]) -> dict[str, Any]:
        match tool_name:
            case "clap_analyze":
                return await self._analyze(params)
            case _:
                raise ValidationError(message=f"Unknown tool: {tool_name}")

    async def _analyze(self, params: dict[str, Any]) -> dict[str, Any]:
        """Run CLAP analysis tasks on audio."""
        audio_hash = params.get("audio_hash")
        if not audio_hash:
            raise ValidationError(message="audio_hash is required", field_name="audio_hash")

        tasks = params.get("tasks", ["classification"])
        text_candidates = params.get("text_candidates", [])
        audio_b_hash = params.get("audio_b_hash")

        # Fetch and decode primary audio
        audio, sample_rate = await self._load_audio(audio_hash)

        # Resample to 48kHz if needed
        if sample_rate != CLAP_SAMPLE_RATE:
            audio = await asyncio.to_thread(resample, audio, sample_rate, CLAP_SAMPLE_RATE)

        result: dict[str, Any] = {}

        for task in tasks:
            match task:
                case "embeddings":
                    result["embeddings"] = await self._get_embeddings(audio)
                case "zero_shot":
                    if not text_candidates:
                        raise ValidationError(
                            message="text_candidates required for zero_shot task",
                            field_name="text_candidates",
                        )
                    result["zero_shot"] = await self._zero_shot(audio, text_candidates)
                case "similarity":
                    if not audio_b_hash:
                        raise ValidationError(
                            message="audio_b_hash required for similarity task",
                            field_name="audio_b_hash",
                        )
                    audio_b, sr_b = await self._load_audio(audio_b_hash)
                    if sr_b != CLAP_SAMPLE_RATE:
                        audio_b = await asyncio.to_thread(resample, audio_b, sr_b, CLAP_SAMPLE_RATE)
                    result["similarity"] = await self._similarity(audio, audio_b)
                case "genre" | "classification":
                    result["genre"] = await self._zero_shot(audio, GENRE_LABELS)
                case "mood":
                    result["mood"] = await self._zero_shot(audio, MOOD_LABELS)
                case _:
                    log.warning(f"Unknown task: {task}")

        return result

    async def _load_audio(self, audio_hash: str) -> tuple[np.ndarray, int]:
        """Fetch audio from CAS and decode."""
        audio_data = cas.fetch(audio_hash)
        if audio_data is None:
            raise NotFoundError(
                message=f"Audio not found in CAS: {audio_hash}",
                resource_type="audio",
                resource_id=audio_hash,
            )
        return await asyncio.to_thread(decode_wav, audio_data)

    async def _get_embeddings(self, audio: np.ndarray) -> list[float]:
        """Get 512-dim audio embeddings."""
        inputs = self.processor(
            audios=audio,
            sampling_rate=CLAP_SAMPLE_RATE,
            return_tensors="pt",
        ).to(self.device)

        with torch.inference_mode():
            features = await asyncio.to_thread(
                self.model.get_audio_features, **inputs
            )

        embedding = features[0].cpu().numpy()
        return embedding.tolist()

    async def _zero_shot(self, audio: np.ndarray, labels: list[str]) -> list[dict[str, Any]]:
        """Zero-shot classification with label list."""
        audio_inputs = self.processor(
            audios=audio,
            sampling_rate=CLAP_SAMPLE_RATE,
            return_tensors="pt",
        ).to(self.device)

        text_inputs = self.processor(
            text=labels,
            return_tensors="pt",
            padding=True,
        ).to(self.device)

        with torch.inference_mode():
            audio_features = await asyncio.to_thread(
                self.model.get_audio_features, **audio_inputs
            )
            text_features = await asyncio.to_thread(
                self.model.get_text_features, **text_inputs
            )

        # Normalize and compute similarity
        audio_features = torch.nn.functional.normalize(audio_features, dim=-1)
        text_features = torch.nn.functional.normalize(text_features, dim=-1)

        similarity = (audio_features @ text_features.T).squeeze(0)
        probs = torch.nn.functional.softmax(similarity * 100, dim=-1)

        scores = probs.cpu().numpy()
        results = [
            {"label": label, "score": float(score)}
            for label, score in sorted(zip(labels, scores), key=lambda x: -x[1])
        ]

        return results

    async def _similarity(self, audio_a: np.ndarray, audio_b: np.ndarray) -> float:
        """Compute cosine similarity between two audio clips."""
        inputs_a = self.processor(
            audios=audio_a,
            sampling_rate=CLAP_SAMPLE_RATE,
            return_tensors="pt",
        ).to(self.device)

        inputs_b = self.processor(
            audios=audio_b,
            sampling_rate=CLAP_SAMPLE_RATE,
            return_tensors="pt",
        ).to(self.device)

        with torch.inference_mode():
            features_a = await asyncio.to_thread(
                self.model.get_audio_features, **inputs_a
            )
            features_b = await asyncio.to_thread(
                self.model.get_audio_features, **inputs_b
            )

        features_a = torch.nn.functional.normalize(features_a, dim=-1)
        features_b = torch.nn.functional.normalize(features_b, dim=-1)

        similarity = (features_a @ features_b.T).squeeze().item()
        return float(similarity)


async def main():
    """Run the CLAP service."""
    import sys

    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s %(levelname)s %(name)s: %(message)s",
    )

    endpoint = os.environ.get("CLAP_ENDPOINT", DEFAULT_ENDPOINT)
    if len(sys.argv) > 1:
        endpoint = sys.argv[1]

    service = ClapService(endpoint=endpoint)
    await service.start()


if __name__ == "__main__":
    asyncio.run(main())
