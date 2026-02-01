"""
Orpheus service implementation.

A unified ZMQ service handling all Orpheus tools with on-demand model loading.
Models are loaded on first use and cached in memory.
"""

from __future__ import annotations

import asyncio
import logging
import os
from pathlib import Path
from typing import Any

import numpy as np
import torch
import torch.nn.functional as F

from hootpy import ModelService, ServiceConfig, NotFoundError, ValidationError, cas
from hootpy.orpheus_tokenizer import OrpheusTokenizer
from hootpy.orpheus_models import load_single_model, MODEL_PATHS

log = logging.getLogger(__name__)

# Default models directory - check env, then legacy paths, then default
def _get_models_dir() -> Path:
    env_val = os.environ.get("ORPHEUS_MODELS_DIR")
    if env_val:
        return Path(env_val)
    # Legacy paths (first existing wins)
    for legacy in ["/tank/ml/music-models/models"]:
        p = Path(legacy)
        if p.exists():
            return p
    return Path.home() / "halfremembered" / "models"

MODELS_DIR = _get_models_dir()

# Default endpoint (IPC socket)
DEFAULT_ENDPOINT = f"ipc://{os.environ.get('XDG_RUNTIME_DIR', os.path.expanduser('~/.hootenanny/run'))}/hootenanny/orpheus.sock"

# EOS tokens differ by model
EOS_TOKEN_BASE = 18817  # base, bridge, children, mono
EOS_TOKEN_LOOPS = 18818  # loops model uses different EOS


def top_p_sampling(logits: torch.Tensor, thres: float = 0.9) -> torch.Tensor:
    """Top-p (nucleus) sampling filter."""
    sorted_logits, sorted_indices = torch.sort(logits, descending=True)
    cum_probs = torch.cumsum(F.softmax(sorted_logits, dim=-1), dim=-1)

    sorted_indices_to_remove = cum_probs > thres
    sorted_indices_to_remove[..., 1:] = sorted_indices_to_remove[..., :-1].clone()
    sorted_indices_to_remove[..., 0] = 0

    indices_to_remove = torch.zeros_like(logits, dtype=torch.bool)
    indices_to_remove.scatter_(-1, sorted_indices, sorted_indices_to_remove)

    logits[indices_to_remove] = float("-inf")
    return logits


class OrpheusService(ModelService):
    """
    Unified Orpheus MIDI generation service.

    Tools:
    - orpheus_generate: Generate MIDI from scratch
    - orpheus_generate_seeded: Use MIDI as inspiration (extracts style)
    - orpheus_continue: Continue existing MIDI sequence
    - orpheus_bridge: Generate musical bridge from section_a
    - orpheus_loops: Generate drum/percussion loops
    - orpheus_classify: Detect human vs AI-composed MIDI

    Models are loaded on-demand and cached:
    - base: Used for generate, generate_seeded, continue
    - bridge: Used for bridge tool
    - loops: Used for loops tool (different EOS token!)
    - classifier: Used for classify tool
    """

    TOOLS = [
        "orpheus_generate",
        "orpheus_generate_seeded",
        "orpheus_continue",
        "orpheus_bridge",
        "orpheus_loops",
        "orpheus_classify",
    ]

    # Map tool names to response type names
    RESPONSE_TYPES = {
        "orpheus_generate": "orpheus_generated",
        "orpheus_generate_seeded": "orpheus_generated",
        "orpheus_continue": "orpheus_generated",
        "orpheus_bridge": "orpheus_generated",
        "orpheus_loops": "orpheus_generated",
        "orpheus_classify": "orpheus_classified",
    }

    def __init__(self, endpoint: str | None = None):
        super().__init__(
            ServiceConfig(
                name="orpheus",
                endpoint=endpoint or os.environ.get("ORPHEUS_ENDPOINT", DEFAULT_ENDPOINT),
            )
        )
        self.models: dict[str, torch.nn.Module] = {}
        self.device = torch.device("cuda" if torch.cuda.is_available() else "cpu")
        self.models_dir = MODELS_DIR
        self.tokenizer = OrpheusTokenizer()

    async def load_model(self):
        """Load default model (base) at startup."""
        if not self.models_dir.exists():
            log.warning(
                f"Models directory {self.models_dir} does not exist. "
                "Set ORPHEUS_MODELS_DIR to your models location."
            )
            return

        # Preload the base model
        log.info(f"Loading Orpheus base model on {self.device}...")
        self._load_model_sync("base")
        log.info("Orpheus base model ready")

    def _load_model_sync(self, name: str) -> torch.nn.Module:
        """Load a model synchronously (blocking)."""
        if name in self.models:
            return self.models[name]

        if name not in MODEL_PATHS:
            raise NotFoundError(
                message=f"Unknown model: {name}",
                resource_type="model",
                resource_id=name,
            )

        checkpoint_path = self.models_dir / MODEL_PATHS[name]
        if not checkpoint_path.exists():
            raise NotFoundError(
                message=f"Model checkpoint not found: {checkpoint_path}",
                resource_type="model_checkpoint",
                resource_id=name,
            )

        model = load_single_model(name, self.models_dir, self.device)
        self.models[name] = model
        return model

    def _get_model(self, name: str) -> torch.nn.Module:
        """Get a model, loading if necessary."""
        if name not in self.models:
            log.info(f"Loading model '{name}' on demand...")
            self._load_model_sync(name)
        return self.models[name]

    def get_response_type(self, tool_name: str) -> str:
        """Get the response type name for a tool."""
        return self.RESPONSE_TYPES.get(tool_name, tool_name + "_response")

    async def handle_request(self, tool_name: str, params: dict[str, Any]) -> dict[str, Any]:
        """Route request to appropriate handler."""
        match tool_name:
            case "orpheus_generate":
                return await self._generate(params, seeded=False, continue_mode=False)
            case "orpheus_generate_seeded":
                return await self._generate(params, seeded=True, continue_mode=False)
            case "orpheus_continue":
                return await self._generate(params, seeded=False, continue_mode=True)
            case "orpheus_bridge":
                return await self._bridge(params)
            case "orpheus_loops":
                return await self._loops(params)
            case "orpheus_classify":
                return await self._classify(params)
            case _:
                raise ValidationError(message=f"Unknown tool: {tool_name}")

    async def _generate(
        self, params: dict[str, Any], seeded: bool, continue_mode: bool
    ) -> dict[str, Any]:
        """
        Generate MIDI using the base model.

        Modes:
        - seeded=False, continue=False: Generate from scratch
        - seeded=True, continue=False: Use MIDI as style inspiration
        - seeded=False, continue=True: Continue existing MIDI sequence
        """
        model = self._get_model("base")

        temperature = params.get("temperature", 1.0)
        top_p = params.get("top_p", 0.95)
        max_tokens = params.get("max_tokens", 1024)

        # Parse seed MIDI if needed
        seed_tokens: list[int] = []
        if seeded or continue_mode:
            midi_hash = params.get("midi_hash")
            if not midi_hash:
                raise ValidationError(
                    message="midi_hash is required for seeded/continue modes",
                    field_name="midi_hash",
                )

            midi_data = cas.fetch(midi_hash)
            if midi_data is None:
                raise NotFoundError(
                    message=f"MIDI not found in CAS: {midi_hash}",
                    resource_type="midi",
                    resource_id=midi_hash,
                )

            seed_tokens = await asyncio.to_thread(self.tokenizer.encode_midi, midi_data)
            log.info(f"Encoded {len(seed_tokens)} tokens from seed MIDI")

        # Generate
        tokens = await asyncio.to_thread(
            self._generate_tokens,
            model,
            seed_tokens if (seeded or continue_mode) else [],
            max_tokens,
            temperature,
            top_p,
            EOS_TOKEN_BASE,
        )

        # Decode to MIDI
        midi_bytes = await asyncio.to_thread(self.tokenizer.decode_tokens, tokens)

        # Store to CAS
        content_hash = cas.store(midi_bytes)
        log.info(f"Generated {len(tokens)} tokens, stored MIDI to CAS: {content_hash}")

        return {
            "artifact_id": "",  # Artifact creation happens upstream
            "content_hash": content_hash,
            "num_tokens": len(tokens),
            "model": "base",
        }

    async def _bridge(self, params: dict[str, Any]) -> dict[str, Any]:
        """Generate a musical bridge from section_a."""
        model = self._get_model("bridge")

        temperature = params.get("temperature", 1.0)
        top_p = params.get("top_p", 0.95)
        max_tokens = params.get("max_tokens", 1024)

        # section_a is required
        section_a_hash = params.get("section_a_hash")
        if not section_a_hash:
            raise ValidationError(
                message="section_a_hash is required",
                field_name="section_a_hash",
            )

        midi_data = cas.fetch(section_a_hash)
        if midi_data is None:
            raise NotFoundError(
                message=f"MIDI not found in CAS: {section_a_hash}",
                resource_type="midi",
                resource_id=section_a_hash,
            )

        seed_tokens = await asyncio.to_thread(self.tokenizer.encode_midi, midi_data)
        log.info(f"Encoded {len(seed_tokens)} tokens from section_a")

        # Generate bridge continuation
        tokens = await asyncio.to_thread(
            self._generate_tokens,
            model,
            seed_tokens,
            max_tokens,
            temperature,
            top_p,
            EOS_TOKEN_BASE,
        )

        # Decode to MIDI
        midi_bytes = await asyncio.to_thread(self.tokenizer.decode_tokens, tokens)

        # Store to CAS
        content_hash = cas.store(midi_bytes)
        log.info(f"Generated bridge with {len(tokens)} tokens, stored to CAS: {content_hash}")

        return {
            "artifact_id": "",
            "content_hash": content_hash,
            "num_tokens": len(tokens),
            "model": "bridge",
        }

    async def _loops(self, params: dict[str, Any]) -> dict[str, Any]:
        """Generate drum/percussion loops."""
        model = self._get_model("loops")

        temperature = params.get("temperature", 1.0)
        top_p = params.get("top_p", 0.95)
        max_tokens = params.get("max_tokens", 1024)

        # Optional seed MIDI
        seed_tokens: list[int] = []
        seed_hash = params.get("seed_hash")
        if seed_hash:
            midi_data = cas.fetch(seed_hash)
            if midi_data is None:
                raise NotFoundError(
                    message=f"MIDI not found in CAS: {seed_hash}",
                    resource_type="midi",
                    resource_id=seed_hash,
                )
            seed_tokens = await asyncio.to_thread(self.tokenizer.encode_midi, midi_data)
            log.info(f"Encoded {len(seed_tokens)} tokens from seed MIDI")

        # Generate with loops EOS token
        tokens = await asyncio.to_thread(
            self._generate_tokens,
            model,
            seed_tokens,
            max_tokens,
            temperature,
            top_p,
            EOS_TOKEN_LOOPS,  # Different EOS for loops!
        )

        # Decode to MIDI
        midi_bytes = await asyncio.to_thread(self.tokenizer.decode_tokens, tokens)

        # Store to CAS
        content_hash = cas.store(midi_bytes)
        log.info(f"Generated loops with {len(tokens)} tokens, stored to CAS: {content_hash}")

        return {
            "artifact_id": "",
            "content_hash": content_hash,
            "num_tokens": len(tokens),
            "model": "loops",
        }

    async def _classify(self, params: dict[str, Any]) -> dict[str, Any]:
        """Classify MIDI as human vs AI composed."""
        model = self._get_model("classifier")

        midi_hash = params.get("midi_hash")
        if not midi_hash:
            raise ValidationError(
                message="midi_hash is required",
                field_name="midi_hash",
            )

        midi_data = cas.fetch(midi_hash)
        if midi_data is None:
            raise NotFoundError(
                message=f"MIDI not found in CAS: {midi_hash}",
                resource_type="midi",
                resource_id=midi_hash,
            )

        tokens = await asyncio.to_thread(self.tokenizer.encode_midi, midi_data)

        if len(tokens) < 10:
            raise ValidationError(
                message=f"MIDI too short: {len(tokens)} tokens, need at least 10",
                field_name="midi_hash",
            )

        # Truncate to classifier's max length
        max_len = 1024
        if len(tokens) > max_len:
            tokens = tokens[:max_len]

        # Classify
        prob = await asyncio.to_thread(self._classify_tokens, model, tokens)

        is_human = prob > 0.5
        confidence = prob if is_human else 1 - prob

        log.info(f"Classified: {'human' if is_human else 'AI'} ({confidence:.1%}), {len(tokens)} tokens")

        return {
            "is_human": is_human,
            "confidence": confidence,
            "probabilities": {"human": prob, "ai": 1 - prob},
            "num_tokens": len(tokens),
        }

    def _generate_tokens(
        self,
        model: torch.nn.Module,
        seed_tokens: list[int],
        max_tokens: int,
        temperature: float,
        top_p: float,
        eos_token: int,
    ) -> list[int]:
        """Generate tokens using the model (blocking, run in thread)."""
        model.eval()

        if not seed_tokens:
            input_tokens = torch.LongTensor([[18816]]).to(self.device)  # Start token
            num_prime = 1
        else:
            input_tokens = torch.LongTensor([seed_tokens]).to(self.device)
            num_prime = len(seed_tokens)

        with torch.inference_mode():
            out = model.generate(
                input_tokens,
                seq_len=num_prime + max_tokens,
                temperature=max(0.01, temperature),
                filter_logits_fn=top_p_sampling,
                filter_kwargs={"thres": top_p},
                eos_token=eos_token,
            )

        return out[0].tolist()[num_prime:]

    def _classify_tokens(self, model: torch.nn.Module, tokens: list[int]) -> float:
        """Classify tokens (blocking, run in thread)."""
        input_tensor = torch.LongTensor([tokens]).to(self.device)

        with torch.inference_mode():
            logits = model(input_tensor)
            prob = torch.sigmoid(logits).item()

        return prob


async def main():
    """Run the Orpheus service."""
    import sys

    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s %(levelname)s %(name)s: %(message)s",
    )

    # Parse endpoint from args or environment
    endpoint = os.environ.get("ORPHEUS_ENDPOINT", DEFAULT_ENDPOINT)
    if len(sys.argv) > 1:
        endpoint = sys.argv[1]

    service = OrpheusService(endpoint=endpoint)
    await service.start()


if __name__ == "__main__":
    asyncio.run(main())
