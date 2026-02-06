"""
Anticipatory Music Transformer service implementation.

Provides polyphonic MIDI generation, continuation, and embedding extraction
using Stanford's Anticipatory Music Transformer over HOOT01/ZMQ.

Models: stanford-crfm/music-{small,medium,large}-800k (~2GB VRAM)
"""

import asyncio
import logging
import os
import tempfile
from typing import Any

import numpy as np
import torch

from hootpy import ModelService, ServiceConfig, NotFoundError, ValidationError, cas

log = logging.getLogger(__name__)

# Default IPC socket
def _socket_dir() -> str:
    xdg = os.environ.get("XDG_RUNTIME_DIR")
    if xdg:
        return f"{xdg}/hootenanny"
    return os.path.expanduser("~/.hootenanny/run")

DEFAULT_ENDPOINT = f"ipc://{_socket_dir()}/anticipatory.sock"

MODEL_IDS = {
    "small": "stanford-crfm/music-small-800k",
    "medium": "stanford-crfm/music-medium-800k",
    "large": "stanford-crfm/music-large-800k",
}

MAX_SEQ_LEN = 1024


class AnticipatoryService(ModelService):
    """
    Anticipatory Music Transformer service.

    Tools:
    - anticipatory_generate: Generate MIDI from scratch
    - anticipatory_continue: Continue existing MIDI
    - anticipatory_embed: Extract hidden-state embeddings from MIDI
    """

    TOOLS = ["anticipatory_generate", "anticipatory_continue", "anticipatory_embed"]

    RESPONSE_TYPES = {
        "anticipatory_generate": "anticipatory_generated",
        "anticipatory_continue": "anticipatory_generated",
        "anticipatory_embed": "anticipatory_embedded",
    }

    def __init__(self, endpoint: str | None = None):
        super().__init__(ServiceConfig(
            name="anticipatory",
            endpoint=endpoint or os.environ.get("ANTICIPATORY_ENDPOINT", DEFAULT_ENDPOINT),
        ))
        self.models: dict[str, Any] = {}
        self.device = torch.device("cuda" if torch.cuda.is_available() else "cpu")

    async def load_model(self):
        """Pre-load the small model at startup."""
        log.info(f"Loading Anticipatory small model on {self.device}...")
        await asyncio.to_thread(self._load_model, "small")
        log.info("Anticipatory small model loaded")

    def _load_model(self, size: str):
        """Load a model by size (small/medium/large)."""
        if size in self.models:
            return self.models[size]

        model_id = MODEL_IDS.get(size)
        if not model_id:
            raise ValidationError(
                message=f"Unknown model size: {size}. Use small, medium, or large.",
                field_name="model_size",
            )

        from transformers import AutoModelForCausalLM

        model = AutoModelForCausalLM.from_pretrained(model_id).to(self.device)
        model.eval()
        self.models[size] = model
        log.info(f"Loaded {size} model: {model_id}")
        return model

    def _get_model(self, size: str | None):
        if not size:
            size = "small"
        if size not in self.models:
            self._load_model(size)
        return self.models[size]

    def get_response_type(self, tool_name: str) -> str:
        return self.RESPONSE_TYPES.get(tool_name, tool_name + "_response")

    async def handle_request(self, tool_name: str, params: dict[str, Any]) -> dict[str, Any]:
        match tool_name:
            case "anticipatory_generate":
                return await self._generate(params)
            case "anticipatory_continue":
                return await self._continue(params)
            case "anticipatory_embed":
                return await self._embed(params)
            case _:
                raise ValidationError(message=f"Unknown tool: {tool_name}")

    async def _generate(self, params: dict[str, Any]) -> dict[str, Any]:
        """Generate MIDI from scratch."""
        from anticipation.convert import events_to_midi
        from anticipation.sample import generate

        length_seconds = params.get("length_seconds", 20.0)
        top_p = params.get("top_p", 0.95)
        model_size = params.get("model_size", "small")

        model = self._get_model(model_size)

        log.info(f"Generating {length_seconds}s MIDI with {model_size} model")

        with torch.inference_mode():
            events = await asyncio.to_thread(
                generate, model, length_seconds, top_p=top_p
            )

        # Convert events to MIDI bytes via temp file
        midi_bytes = await asyncio.to_thread(self._events_to_bytes, events)

        content_hash = cas.store(midi_bytes)
        log.info(f"Generated MIDI, CAS: {content_hash}")

        return {
            "artifact_id": "",
            "content_hash": content_hash,
            "duration_seconds": length_seconds,
            "model_size": model_size,
        }

    async def _continue(self, params: dict[str, Any]) -> dict[str, Any]:
        """Continue existing MIDI."""
        from anticipation.convert import midi_to_events, events_to_midi
        from anticipation.sample import generate

        input_hash = params.get("input_hash")
        if not input_hash:
            raise ValidationError(message="input_hash is required", field_name="input_hash")

        length_seconds = params.get("length_seconds", 20.0)
        prime_seconds = params.get("prime_seconds", 5.0)
        top_p = params.get("top_p", 0.95)
        model_size = params.get("model_size", "small")

        model = self._get_model(model_size)

        # Fetch MIDI from CAS
        midi_data = cas.fetch(input_hash)
        if midi_data is None:
            raise NotFoundError(
                message=f"MIDI not found in CAS: {input_hash}",
                resource_type="midi",
                resource_id=input_hash,
            )

        # Convert to events via temp file (anticipation needs file paths)
        events = await asyncio.to_thread(self._bytes_to_events, midi_data)

        log.info(
            f"Continuing MIDI ({len(events)} events) for {length_seconds}s "
            f"with {prime_seconds}s prime"
        )

        with torch.inference_mode():
            continued = await asyncio.to_thread(
                generate, model, length_seconds,
                top_p=top_p, primer=events, prime_seconds=prime_seconds,
            )

        midi_bytes = await asyncio.to_thread(self._events_to_bytes, continued)
        content_hash = cas.store(midi_bytes)
        log.info(f"Continued MIDI, CAS: {content_hash}")

        return {
            "artifact_id": "",
            "content_hash": content_hash,
            "duration_seconds": length_seconds,
            "model_size": model_size,
        }

    async def _embed(self, params: dict[str, Any]) -> dict[str, Any]:
        """Extract embeddings from MIDI."""
        from anticipation.convert import midi_to_events
        from anticipation.tokenize import tokenize

        input_hash = params.get("input_hash")
        if not input_hash:
            raise ValidationError(message="input_hash is required", field_name="input_hash")

        model_size = params.get("model_size", "small")
        embed_layer = params.get("embed_layer", -3)

        model = self._get_model(model_size)

        # Fetch MIDI from CAS
        midi_data = cas.fetch(input_hash)
        if midi_data is None:
            raise NotFoundError(
                message=f"MIDI not found in CAS: {input_hash}",
                resource_type="midi",
                resource_id=input_hash,
            )

        events = await asyncio.to_thread(self._bytes_to_events, midi_data)
        tokens = tokenize(events)

        # Truncate to max sequence length
        if len(tokens) > MAX_SEQ_LEN:
            tokens = tokens[:MAX_SEQ_LEN]

        input_ids = torch.tensor([tokens], device=self.device)

        with torch.inference_mode():
            outputs = await asyncio.to_thread(
                model, input_ids, output_hidden_states=True,
            )

        # Extract hidden states from specified layer, mean-pool
        hidden = outputs.hidden_states[embed_layer]
        embedding = hidden.mean(dim=1).squeeze(0).cpu().numpy()

        log.info(f"Extracted {len(embedding)}-dim embedding from layer {embed_layer}")

        return {
            "embeddings": embedding.tolist(),
            "embed_dim": len(embedding),
            "model_size": model_size,
        }

    def _bytes_to_events(self, midi_bytes: bytes) -> list:
        """Convert MIDI bytes to anticipation events via temp file."""
        from anticipation.convert import midi_to_events

        with tempfile.NamedTemporaryFile(suffix=".mid", delete=False) as f:
            f.write(midi_bytes)
            temp_path = f.name

        try:
            return midi_to_events(temp_path)
        finally:
            os.unlink(temp_path)

    def _events_to_bytes(self, events: list) -> bytes:
        """Convert anticipation events to MIDI bytes via temp file."""
        from anticipation.convert import events_to_midi

        with tempfile.NamedTemporaryFile(suffix=".mid", delete=False) as f:
            temp_path = f.name

        try:
            events_to_midi(events, temp_path)
            with open(temp_path, "rb") as f:
                return f.read()
        finally:
            os.unlink(temp_path)


async def main():
    """Run the Anticipatory service."""
    import sys

    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s %(levelname)s %(name)s: %(message)s",
    )

    endpoint = os.environ.get("ANTICIPATORY_ENDPOINT", DEFAULT_ENDPOINT)
    if len(sys.argv) > 1:
        endpoint = sys.argv[1]

    service = AnticipatoryService(endpoint=endpoint)
    await service.start()


if __name__ == "__main__":
    asyncio.run(main())
