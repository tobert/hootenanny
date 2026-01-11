"""
RAVE service implementation

Provides HOOT01-native audio encoding/decoding using RAVE models.
"""

import asyncio
import io
import logging
import os
import struct
from pathlib import Path
from typing import Any

import numpy as np
import torch
import torchaudio

from hootpy import ModelService, ServiceConfig, NotFoundError, ValidationError

log = logging.getLogger(__name__)

# Default model directory
MODELS_DIR = Path(os.environ.get(
    "RAVE_MODELS_DIR",
    os.path.expanduser("~/.hootenanny/models/rave")
))

# RAVE operates at 48kHz
RAVE_SAMPLE_RATE = 48000


class RaveService(ModelService):
    """
    RAVE audio encoder/decoder service.

    Tools:
    - rave_encode: Audio waveform → latent codes
    - rave_decode: Latent codes → audio waveform
    - rave_reconstruct: Encode then decode (round-trip)
    - rave_generate: Sample from prior → audio
    """

    TOOLS = ["rave_encode", "rave_decode", "rave_reconstruct", "rave_generate"]

    def __init__(self, endpoint: str = "tcp://127.0.0.1:5591"):
        super().__init__(ServiceConfig(
            name="rave",
            endpoint=endpoint,
        ))
        self.models: dict[str, torch.jit.ScriptModule] = {}
        self.device = torch.device("cuda" if torch.cuda.is_available() else "cpu")
        self.models_dir = MODELS_DIR

    async def load_model(self):
        """Load default RAVE model at startup"""
        # Check models directory exists
        if not self.models_dir.exists():
            log.warning(
                f"Models directory {self.models_dir} does not exist. "
                "Run 'just download-rave-models' to download models."
            )
            return

        # Load first available model as default
        for model_file in self.models_dir.glob("*.ts"):
            if "_streaming" not in model_file.stem:
                model_name = model_file.stem
                log.info(f"Loading default model: {model_name}")
                self._load_model(model_name)
                break

    def _load_model(self, name: str, streaming: bool = False) -> torch.jit.ScriptModule:
        """Load a RAVE model by name"""
        suffix = "_streaming" if streaming else ""
        cache_key = f"{name}{suffix}"

        if cache_key in self.models:
            return self.models[cache_key]

        model_path = self.models_dir / f"{name}{suffix}.ts"
        if not model_path.exists():
            raise NotFoundError(
                message=f"Model '{name}' not found (streaming={streaming})",
                resource_type="model",
                resource_id=name,
            )

        log.info(f"Loading model from {model_path}")
        model = torch.jit.load(str(model_path))
        model = model.to(self.device)
        model.eval()

        self.models[cache_key] = model
        return model

    def _get_model(self, name: str | None, streaming: bool = False) -> torch.jit.ScriptModule:
        """Get a model, loading if necessary"""
        if name is None:
            # Use first loaded model or load default
            if self.models:
                return next(iter(self.models.values()))
            name = "vintage"  # Default model
        return self._load_model(name, streaming)

    async def handle_request(self, tool_name: str, params: dict[str, Any]) -> dict[str, Any]:
        """Route request to appropriate handler"""
        match tool_name:
            case "rave_encode":
                return await self._encode(params)
            case "rave_decode":
                return await self._decode(params)
            case "rave_reconstruct":
                return await self._reconstruct(params)
            case "rave_generate":
                return await self._generate(params)
            case _:
                raise ValidationError(message=f"Unknown tool: {tool_name}")

    async def _encode(self, params: dict[str, Any]) -> dict[str, Any]:
        """
        Encode audio waveform to latent codes.

        Input: audio_hash (CAS hash of WAV file)
        Output: latent codes as artifact
        """
        audio_hash = params.get("audio_hash")
        if not audio_hash:
            raise ValidationError(message="audio_hash is required", field_name="audio_hash")

        model_name = params.get("model")
        model = self._get_model(model_name)

        # Load audio from CAS (placeholder - would call hootenanny CAS)
        # For now, expect audio_data to be passed directly for testing
        audio_data = params.get("audio_data")
        if audio_data is None:
            raise ValidationError(
                message="audio_data required (CAS integration pending)",
                field_name="audio_data",
            )

        # Decode audio
        audio, sample_rate = await asyncio.to_thread(
            self._decode_audio, audio_data
        )

        # Resample if needed
        if sample_rate != RAVE_SAMPLE_RATE:
            audio = await asyncio.to_thread(
                torchaudio.functional.resample,
                audio, sample_rate, RAVE_SAMPLE_RATE
            )

        # Ensure correct shape: (batch, channels, samples)
        if audio.dim() == 1:
            audio = audio.unsqueeze(0).unsqueeze(0)
        elif audio.dim() == 2:
            audio = audio.unsqueeze(0)

        audio = audio.to(self.device)

        # Encode
        with torch.no_grad():
            z = await asyncio.to_thread(model.encode, audio)

        # Convert to bytes for storage
        z_np = z.cpu().numpy()
        latent_bytes = self._pack_latent(z_np)

        return {
            "latent_data": latent_bytes,
            "latent_shape": list(z_np.shape),
            "latent_dim": z_np.shape[-1] if z_np.ndim > 0 else 0,
            "model": model_name or "default",
            "sample_rate": RAVE_SAMPLE_RATE,
        }

    async def _decode(self, params: dict[str, Any]) -> dict[str, Any]:
        """
        Decode latent codes to audio waveform.

        Input: latent_hash (CAS hash of latent file) or latent_data
        Output: audio as artifact
        """
        model_name = params.get("model")
        model = self._get_model(model_name)

        # Get latent data
        latent_data = params.get("latent_data")
        latent_shape = params.get("latent_shape")

        if latent_data is None:
            raise ValidationError(
                message="latent_data required",
                field_name="latent_data",
            )

        # Unpack latent
        z_np = self._unpack_latent(latent_data, latent_shape)
        z = torch.from_numpy(z_np).to(self.device)

        # Decode
        with torch.no_grad():
            audio = await asyncio.to_thread(model.decode, z)

        # Convert to WAV bytes
        audio_np = audio.squeeze().cpu().numpy()
        wav_bytes = self._encode_wav(audio_np, RAVE_SAMPLE_RATE)

        return {
            "audio_data": wav_bytes,
            "sample_rate": RAVE_SAMPLE_RATE,
            "duration_seconds": len(audio_np) / RAVE_SAMPLE_RATE,
            "model": model_name or "default",
        }

    async def _reconstruct(self, params: dict[str, Any]) -> dict[str, Any]:
        """
        Encode then decode audio (round-trip reconstruction).

        Input: audio_hash or audio_data
        Output: reconstructed audio
        """
        model_name = params.get("model")
        model = self._get_model(model_name)

        # Get audio
        audio_data = params.get("audio_data")
        if audio_data is None:
            raise ValidationError(
                message="audio_data required",
                field_name="audio_data",
            )

        # Decode input audio
        audio, sample_rate = await asyncio.to_thread(
            self._decode_audio, audio_data
        )

        # Resample if needed
        if sample_rate != RAVE_SAMPLE_RATE:
            audio = await asyncio.to_thread(
                torchaudio.functional.resample,
                audio, sample_rate, RAVE_SAMPLE_RATE
            )

        # Ensure correct shape
        if audio.dim() == 1:
            audio = audio.unsqueeze(0).unsqueeze(0)
        elif audio.dim() == 2:
            audio = audio.unsqueeze(0)

        audio = audio.to(self.device)

        # Reconstruct (encode then decode)
        with torch.no_grad():
            reconstructed = await asyncio.to_thread(model.forward, audio)

        # Convert to WAV bytes
        audio_np = reconstructed.squeeze().cpu().numpy()
        wav_bytes = self._encode_wav(audio_np, RAVE_SAMPLE_RATE)

        return {
            "audio_data": wav_bytes,
            "sample_rate": RAVE_SAMPLE_RATE,
            "duration_seconds": len(audio_np) / RAVE_SAMPLE_RATE,
            "model": model_name or "default",
        }

    async def _generate(self, params: dict[str, Any]) -> dict[str, Any]:
        """
        Generate audio by sampling from the prior.

        Input: duration_seconds, temperature
        Output: generated audio
        """
        model_name = params.get("model")
        model = self._get_model(model_name)

        duration_seconds = params.get("duration_seconds", 4.0)
        temperature = params.get("temperature", 1.0)

        # Calculate number of latent frames needed
        # RAVE typically has a compression ratio of ~2048 (48000/~23 latent fps)
        num_samples = int(duration_seconds * RAVE_SAMPLE_RATE)

        # Get latent dimension from model (usually 128)
        # Sample random latent codes
        # Note: This is a simplified approach - proper generation would
        # use the model's prior if available
        latent_dim = 128  # Standard RAVE latent dimension
        latent_length = num_samples // 2048 + 1  # Approximate compression ratio

        # Sample from standard normal, scale by temperature
        z = torch.randn(1, latent_dim, latent_length, device=self.device) * temperature

        # Decode
        with torch.no_grad():
            audio = await asyncio.to_thread(model.decode, z)

        # Trim to exact duration
        audio_np = audio.squeeze().cpu().numpy()
        audio_np = audio_np[:num_samples]

        wav_bytes = self._encode_wav(audio_np, RAVE_SAMPLE_RATE)

        return {
            "audio_data": wav_bytes,
            "sample_rate": RAVE_SAMPLE_RATE,
            "duration_seconds": len(audio_np) / RAVE_SAMPLE_RATE,
            "model": model_name or "default",
            "temperature": temperature,
        }

    def _decode_audio(self, data: bytes) -> tuple[torch.Tensor, int]:
        """Decode audio bytes (WAV format) to tensor"""
        buffer = io.BytesIO(data)
        audio, sample_rate = torchaudio.load(buffer)
        # Convert to mono if stereo
        if audio.shape[0] > 1:
            audio = audio.mean(dim=0, keepdim=True)
        return audio, sample_rate

    def _encode_wav(self, audio: np.ndarray, sample_rate: int) -> bytes:
        """Encode numpy array to WAV bytes"""
        buffer = io.BytesIO()
        # Ensure audio is in correct shape for torchaudio
        if audio.ndim == 1:
            audio_tensor = torch.from_numpy(audio).unsqueeze(0)
        else:
            audio_tensor = torch.from_numpy(audio)
        torchaudio.save(buffer, audio_tensor, sample_rate, format="wav")
        return buffer.getvalue()

    def _pack_latent(self, z: np.ndarray) -> bytes:
        """Pack latent codes to bytes with shape header"""
        # Header: ndim (1 byte) + shape (4 bytes each)
        header = struct.pack("B", z.ndim)
        for dim in z.shape:
            header += struct.pack("<I", dim)
        return header + z.astype(np.float32).tobytes()

    def _unpack_latent(self, data: bytes, shape: list[int] | None = None) -> np.ndarray:
        """Unpack latent codes from bytes"""
        if shape:
            # Shape provided externally
            return np.frombuffer(data, dtype=np.float32).reshape(shape)

        # Read shape from header
        ndim = struct.unpack("B", data[0:1])[0]
        shape = []
        offset = 1
        for _ in range(ndim):
            dim = struct.unpack("<I", data[offset:offset+4])[0]
            shape.append(dim)
            offset += 4
        return np.frombuffer(data[offset:], dtype=np.float32).reshape(shape)


async def main():
    """Run the RAVE service"""
    import sys

    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s %(levelname)s %(name)s: %(message)s",
    )

    # Parse endpoint from args or environment
    endpoint = os.environ.get("RAVE_ENDPOINT", "tcp://127.0.0.1:5591")
    if len(sys.argv) > 1:
        endpoint = sys.argv[1]

    service = RaveService(endpoint=endpoint)
    await service.start()


if __name__ == "__main__":
    asyncio.run(main())
