"""
MIDI role classifier service implementation.

Receives pre-extracted feature vectors from the Rust side and classifies
each voice into a musical role using a trained scikit-learn model.
"""

from __future__ import annotations

import asyncio
import json
import logging
import os
from typing import Any

from hootpy import ModelService, ServiceConfig, ValidationError

from .model import RoleClassifierModel

log = logging.getLogger(__name__)

DEFAULT_ENDPOINT = (
    f"ipc://{os.environ.get('XDG_RUNTIME_DIR', os.path.expanduser('~/.hootenanny/run'))}"
    "/hootenanny/midi-role.sock"
)

# Voice roles matching the Rust VoiceRole enum
VOICE_ROLES = [
    "melody", "bass", "countermelody", "harmonic_fill",
    "percussion", "rhythm", "primary_harmony", "secondary_harmony", "padding",
]


class MidiRoleClassifierService(ModelService):
    """
    MIDI voice role classification service.

    Receives feature vectors (not raw MIDI) and returns role predictions
    with confidence scores.

    Tool:
    - midi_classify_voices: Classify voice feature vectors into musical roles
    """

    TOOLS = ["midi_classify_voices"]

    RESPONSE_TYPES = {
        "midi_classify_voices": "midi_voices_classified",
    }

    def __init__(self, endpoint: str | None = None):
        super().__init__(
            ServiceConfig(
                name="midi-role-classifier",
                endpoint=endpoint or os.environ.get("MIDI_ROLE_ENDPOINT", DEFAULT_ENDPOINT),
            )
        )
        self.model = RoleClassifierModel()

    async def load_model(self):
        """Load the trained classification model at startup."""
        await asyncio.to_thread(self.model.load)

    def get_response_type(self, tool_name: str) -> str:
        """Get the response type name for a tool."""
        return self.RESPONSE_TYPES.get(tool_name, tool_name + "_response")

    async def handle_request(self, tool_name: str, params: dict[str, Any]) -> dict[str, Any]:
        """Route request to appropriate handler."""
        match tool_name:
            case "midi_classify_voices":
                return await self._classify(params)
            case _:
                raise ValidationError(message=f"Unknown tool: {tool_name}")

    async def _classify(self, params: dict[str, Any]) -> dict[str, Any]:
        """Classify voice feature vectors into musical roles."""
        voice_data = params.get("voice_data", params.get("voiceData"))
        if not voice_data:
            raise ValidationError(
                message="voice_data (feature vectors JSON) is required",
                field_name="voice_data",
            )

        # Parse feature vectors (sent as JSON from Rust)
        try:
            if isinstance(voice_data, str):
                features = json.loads(voice_data)
            else:
                features = voice_data
        except json.JSONDecodeError as e:
            raise ValidationError(
                message=f"Invalid feature vector JSON: {e}",
                field_name="voice_data",
            ) from e

        if not features:
            return {
                "classifications": [],
                "features_json": "[]",
                "method": "machine_learning",
                "summary": "No voices to classify",
            }

        # Run classification
        predictions = await asyncio.to_thread(self.model.predict, features)

        # Build response matching MidiVoicesClassifiedResponse schema
        classifications = []
        for i, pred in enumerate(predictions):
            classifications.append({
                "voice_index": i,
                "role": pred["role"],
                "confidence": pred["confidence"],
                "method": "machine_learning",
                "alternative_roles": pred.get("alternatives", []),
            })

        role_summary = ", ".join(
            f"v{c['voice_index']}: {c['role']} ({c['confidence']:.0%})"
            for c in classifications
        )

        return {
            "classifications": classifications,
            "features_json": json.dumps(features),
            "method": "machine_learning",
            "summary": f"Classified {len(classifications)} voices (ML): {role_summary}",
        }


async def main():
    """Run the MIDI role classifier service."""
    import sys

    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s %(levelname)s %(name)s: %(message)s",
    )

    endpoint = os.environ.get("MIDI_ROLE_ENDPOINT", DEFAULT_ENDPOINT)
    if len(sys.argv) > 1:
        endpoint = sys.argv[1]

    service = MidiRoleClassifierService(endpoint=endpoint)
    await service.start()


if __name__ == "__main__":
    asyncio.run(main())
