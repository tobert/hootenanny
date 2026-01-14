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

from hootpy import ModelService, ServiceConfig, NotFoundError, ValidationError, cas

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

    # Map tool names to response type names (schema uses past-tense)
    RESPONSE_TYPES = {
        "rave_encode": "rave_encoded",
        "rave_decode": "rave_decoded",
        "rave_reconstruct": "rave_reconstructed",
        "rave_generate": "rave_generated",
    }

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

        # Disable gradient tracking on all parameters and buffers to avoid
        # in-place operation errors in cached_conv layers
        for param in model.parameters():
            param.requires_grad_(False)
        for buf in model.buffers():
            buf.requires_grad_(False)

        self.models[cache_key] = model
        return model

    def _get_model(self, name: str | None, streaming: bool = False) -> torch.jit.ScriptModule:
        """Get a model, loading if necessary"""
        if not name:  # Handle None and empty string
            # Use first loaded model or load default
            if self.models:
                return next(iter(self.models.values()))
            name = "vintage"  # Default model
        return self._load_model(name, streaming)

    def get_response_type(self, tool_name: str) -> str:
        """Get the response type name for a tool (overrides base class)"""
        return self.RESPONSE_TYPES.get(tool_name, tool_name + "_response")

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
        Output: latent codes stored in CAS
        """
        audio_hash = params.get("audio_hash")
        if not audio_hash:
            raise ValidationError(message="audio_hash is required", field_name="audio_hash")

        model_name = params.get("model")
        model = self._get_model(model_name)

        # Load audio from CAS
        audio_data = cas.fetch(audio_hash)
        if audio_data is None:
            raise NotFoundError(
                message=f"Audio not found in CAS: {audio_hash}",
                resource_type="audio",
                resource_id=audio_hash,
            )

        log.info(f"Fetched {len(audio_data)} bytes from CAS: {audio_hash}")

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
        with torch.inference_mode():
            z = await asyncio.to_thread(model.encode, audio)

        # Convert to bytes for storage
        z_np = z.cpu().numpy()
        latent_bytes = self._pack_latent(z_np)

        # Store to CAS
        content_hash = cas.store(latent_bytes)
        log.info(f"Stored {len(latent_bytes)} bytes latent to CAS: {content_hash}")

        return {
            "artifact_id": "",  # TODO: artifact creation
            "content_hash": content_hash,
            "latent_shape": list(z_np.shape),
            "latent_dim": z_np.shape[-1] if z_np.ndim > 0 else 0,
            "model": model_name or "vintage",
            "sample_rate": RAVE_SAMPLE_RATE,
        }

    async def _decode(self, params: dict[str, Any]) -> dict[str, Any]:
        """
        Decode latent codes to audio waveform.

        Input: latent_hash (CAS hash of latent file)
        Output: audio stored in CAS
        """
        model_name = params.get("model")
        model = self._get_model(model_name)

        # Get latent from CAS
        latent_hash = params.get("latent_hash")
        if not latent_hash:
            raise ValidationError(
                message="latent_hash is required",
                field_name="latent_hash",
            )

        latent_data = cas.fetch(latent_hash)
        if latent_data is None:
            raise NotFoundError(
                message=f"Latent not found in CAS: {latent_hash}",
                resource_type="latent",
                resource_id=latent_hash,
            )

        log.info(f"Fetched {len(latent_data)} bytes latent from CAS: {latent_hash}")

        # Unpack latent (shape is stored in the header)
        z_np = self._unpack_latent(latent_data)
        z = torch.from_numpy(z_np).to(self.device)

        # Decode
        with torch.inference_mode():
            audio = await asyncio.to_thread(model.decode, z)

        # Convert to WAV bytes
        audio_np = audio.squeeze().cpu().numpy()
        wav_bytes = self._encode_wav(audio_np, RAVE_SAMPLE_RATE)

        # Store to CAS
        content_hash = cas.store(wav_bytes)
        log.info(f"Stored {len(wav_bytes)} bytes to CAS: {content_hash}")

        return {
            "artifact_id": "",  # TODO: artifact creation
            "content_hash": content_hash,
            "duration_seconds": len(audio_np) / RAVE_SAMPLE_RATE,
            "sample_rate": RAVE_SAMPLE_RATE,
            "model": model_name or "vintage",
        }

    async def _reconstruct(self, params: dict[str, Any]) -> dict[str, Any]:
        """
        Encode then decode audio (round-trip reconstruction).

        Input: audio_hash (CAS hash of input audio)
        Output: reconstructed audio stored in CAS
        """
        model_name = params.get("model")
        model = self._get_model(model_name)

        # Get audio from CAS
        audio_hash = params.get("audio_hash")
        if not audio_hash:
            raise ValidationError(
                message="audio_hash is required",
                field_name="audio_hash",
            )

        audio_data = cas.fetch(audio_hash)
        if audio_data is None:
            raise NotFoundError(
                message=f"Audio not found in CAS: {audio_hash}",
                resource_type="audio",
                resource_id=audio_hash,
            )

        log.info(f"Fetched {len(audio_data)} bytes from CAS: {audio_hash}")

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
        with torch.inference_mode():
            reconstructed = await asyncio.to_thread(model.forward, audio)

        # Convert to WAV bytes
        audio_np = reconstructed.squeeze().cpu().numpy()
        wav_bytes = self._encode_wav(audio_np, RAVE_SAMPLE_RATE)

        # Store to CAS
        content_hash = cas.store(wav_bytes)
        log.info(f"Stored {len(wav_bytes)} bytes to CAS: {content_hash}")

        return {
            "artifact_id": "",  # Artifact creation requires hootenanny callback (TODO)
            "content_hash": content_hash,
            "duration_seconds": len(audio_np) / RAVE_SAMPLE_RATE,
            "sample_rate": RAVE_SAMPLE_RATE,
            "model": model_name or "vintage",
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
        # Use detach() to ensure no gradient tracking
        z = torch.randn(1, latent_dim, latent_length, device=self.device) * temperature
        z = z.detach()

        # Decode - use inference_mode to avoid gradient tracking issues with cached_conv
        with torch.inference_mode():
            audio = await asyncio.to_thread(model.decode, z)

        # Trim to exact duration
        audio_np = audio.squeeze().cpu().numpy()
        audio_np = audio_np[:num_samples]

        wav_bytes = self._encode_wav(audio_np, RAVE_SAMPLE_RATE)

        # Store to CAS
        content_hash = cas.store(wav_bytes)
        log.info(f"Stored {len(wav_bytes)} bytes to CAS: {content_hash}")

        return {
            "artifact_id": "",  # Artifact creation requires hootenanny callback (TODO)
            "content_hash": content_hash,
            "duration_seconds": len(audio_np) / RAVE_SAMPLE_RATE,
            "sample_rate": RAVE_SAMPLE_RATE,
            "model": model_name or "vintage",
            "temperature": temperature,
        }

    def _decode_audio(self, data: bytes) -> tuple[torch.Tensor, int]:
        """Decode audio bytes (WAV format) to tensor using Python wave module"""
        import wave

        buffer = io.BytesIO(data)
        with wave.open(buffer, 'rb') as wav:
            sample_rate = wav.getframerate()
            n_channels = wav.getnchannels()
            sample_width = wav.getsampwidth()
            n_frames = wav.getnframes()
            audio_bytes = wav.readframes(n_frames)

        # Convert to numpy based on sample width
        if sample_width == 2:  # 16-bit
            audio_np = np.frombuffer(audio_bytes, dtype=np.int16).astype(np.float32) / 32768.0
        elif sample_width == 4:  # 32-bit
            audio_np = np.frombuffer(audio_bytes, dtype=np.int32).astype(np.float32) / 2147483648.0
        else:  # 8-bit or other
            audio_np = np.frombuffer(audio_bytes, dtype=np.uint8).astype(np.float32) / 128.0 - 1.0

        # Reshape for stereo and convert to mono if needed
        if n_channels > 1:
            audio_np = audio_np.reshape(-1, n_channels).mean(axis=1)

        # Convert to torch tensor
        audio = torch.from_numpy(audio_np)
        return audio, sample_rate

    def _encode_wav(self, audio: np.ndarray, sample_rate: int) -> bytes:
        """Encode numpy array to WAV bytes using Python's wave module"""
        import wave

        buffer = io.BytesIO()
        # Ensure audio is 1D
        if audio.ndim > 1:
            audio = audio.flatten()

        # Convert float32 [-1, 1] to int16
        audio_int16 = (audio * 32767).astype(np.int16)

        with wave.open(buffer, 'wb') as wav:
            wav.setnchannels(1)  # mono
            wav.setsampwidth(2)  # 16-bit
            wav.setframerate(sample_rate)
            wav.writeframes(audio_int16.tobytes())

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
