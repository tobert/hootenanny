"""
RAVE service implementation

Provides HOOT01-native audio encoding/decoding using RAVE models.
Supports both batch processing (via tool calls) and realtime streaming.
"""

import asyncio
import io
import logging
import os
import struct
import time
import uuid
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

import numpy as np
import torch
import torchaudio
import zmq
import zmq.asyncio

from hootpy import ModelService, ServiceConfig, NotFoundError, ValidationError, cas

log = logging.getLogger(__name__)

# Default model directory
MODELS_DIR = Path(os.environ.get(
    "RAVE_MODELS_DIR",
    os.path.expanduser("~/.hootenanny/models/rave")
))

# RAVE operates at 48kHz
RAVE_SAMPLE_RATE = 48000

# Default streaming endpoint
DEFAULT_STREAMING_ENDPOINT = "tcp://127.0.0.1:5592"


@dataclass
class StreamingSession:
    """State for an active audio streaming session."""
    stream_id: str
    model_name: str
    input_identity: str
    output_identity: str
    buffer_size: int = 2048
    started_at: float = field(default_factory=time.time)
    frames_processed: int = 0
    running: bool = True


class RaveService(ModelService):
    """
    RAVE audio encoder/decoder service.

    Tools (batch):
    - rave_encode: Audio waveform → latent codes
    - rave_decode: Latent codes → audio waveform
    - rave_reconstruct: Encode then decode (round-trip)
    - rave_generate: Sample from prior → audio

    Tools (streaming):
    - rave_stream_start: Start realtime audio streaming
    - rave_stream_stop: Stop a streaming session
    - rave_stream_status: Get streaming session status
    """

    TOOLS = [
        "rave_encode", "rave_decode", "rave_reconstruct", "rave_generate",
        "rave_stream_start", "rave_stream_stop", "rave_stream_status",
    ]

    # Map tool names to response type names (schema uses past-tense)
    RESPONSE_TYPES = {
        "rave_encode": "rave_encoded",
        "rave_decode": "rave_decoded",
        "rave_reconstruct": "rave_reconstructed",
        "rave_generate": "rave_generated",
        "rave_stream_start": "rave_stream_started",
        "rave_stream_stop": "rave_stream_stopped",
        "rave_stream_status": "rave_stream_status",
    }

    def __init__(
        self,
        endpoint: str = "tcp://127.0.0.1:5591",
        streaming_endpoint: str = DEFAULT_STREAMING_ENDPOINT,
    ):
        super().__init__(ServiceConfig(
            name="rave",
            endpoint=endpoint,
        ))
        self.models: dict[str, torch.jit.ScriptModule] = {}
        self.device = torch.device("cuda" if torch.cuda.is_available() else "cpu")
        self.models_dir = MODELS_DIR

        # Streaming state
        self.streaming_endpoint = streaming_endpoint
        self.streaming_socket: zmq.asyncio.Socket | None = None
        self.streaming_session: StreamingSession | None = None
        self._streaming_task: asyncio.Task | None = None

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
            case "rave_stream_start":
                return await self._stream_start(params)
            case "rave_stream_stop":
                return await self._stream_stop(params)
            case "rave_stream_status":
                return await self._stream_status(params)
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
            "duration_seconds": audio_np.size / RAVE_SAMPLE_RATE,
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
            "duration_seconds": audio_np.size / RAVE_SAMPLE_RATE,
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
            "duration_seconds": audio_np.size / RAVE_SAMPLE_RATE,
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

    # =========================================================================
    # Streaming Methods
    # =========================================================================

    async def _stream_start(self, params: dict[str, Any]) -> dict[str, Any]:
        """Start a realtime audio streaming session."""
        if self.streaming_session is not None:
            raise ValidationError(
                message="A streaming session is already active",
                field_name="stream_id",
            )

        model_name = params.get("model")
        input_identity = params.get("input_identity", "")
        output_identity = params.get("output_identity", "")
        buffer_size = params.get("buffer_size", 2048)

        # Load the model
        model = self._get_model(model_name)

        # Create session
        stream_id = f"stream_{uuid.uuid4().hex[:12]}"
        self.streaming_session = StreamingSession(
            stream_id=stream_id,
            model_name=model_name or "vintage",
            input_identity=input_identity,
            output_identity=output_identity,
            buffer_size=buffer_size,
        )

        # Setup streaming socket if not already done
        if self.streaming_socket is None:
            self.streaming_socket = self.ctx.socket(zmq.PAIR)
            self.streaming_socket.bind(self.streaming_endpoint)
            log.info(f"Streaming socket bound to {self.streaming_endpoint}")

        # Start the streaming processing task
        self._streaming_task = asyncio.create_task(
            self._streaming_loop(model)
        )

        log.info(f"Started streaming session {stream_id} with model {model_name}")

        return {
            "stream_id": stream_id,
            "model": self.streaming_session.model_name,
            "input_identity": input_identity,
            "output_identity": output_identity,
            "latency_ms": buffer_size * 1000 // RAVE_SAMPLE_RATE,
        }

    async def _stream_stop(self, params: dict[str, Any]) -> dict[str, Any]:
        """Stop a streaming session."""
        stream_id = params.get("stream_id", "")

        if self.streaming_session is None:
            raise ValidationError(
                message="No active streaming session",
                field_name="stream_id",
            )

        if self.streaming_session.stream_id != stream_id:
            raise ValidationError(
                message=f"Stream ID mismatch: expected {self.streaming_session.stream_id}",
                field_name="stream_id",
            )

        # Stop the session
        self.streaming_session.running = False
        duration = time.time() - self.streaming_session.started_at

        # Wait for streaming task to finish
        if self._streaming_task is not None:
            try:
                await asyncio.wait_for(self._streaming_task, timeout=2.0)
            except asyncio.TimeoutError:
                self._streaming_task.cancel()
            self._streaming_task = None

        log.info(f"Stopped streaming session {stream_id}, duration={duration:.1f}s")

        self.streaming_session = None

        return {
            "stream_id": stream_id,
            "duration_seconds": duration,
        }

    async def _stream_status(self, params: dict[str, Any]) -> dict[str, Any]:
        """Get streaming session status."""
        stream_id = params.get("stream_id", "")

        if self.streaming_session is None:
            return {
                "stream_id": stream_id,
                "running": False,
                "model": "",
                "input_identity": "",
                "output_identity": "",
                "frames_processed": 0,
                "latency_ms": 0,
            }

        session = self.streaming_session
        return {
            "stream_id": session.stream_id,
            "running": session.running,
            "model": session.model_name,
            "input_identity": session.input_identity,
            "output_identity": session.output_identity,
            "frames_processed": session.frames_processed,
            "latency_ms": session.buffer_size * 1000 // RAVE_SAMPLE_RATE,
        }

    async def _streaming_loop(self, model: torch.jit.ScriptModule):
        """
        Main streaming loop: receive audio chunks, process through RAVE, send back.

        Audio format (both directions):
        - Little-endian f32 samples
        - Interleaved stereo (L, R, L, R, ...)
        """
        log.info("Streaming loop started")
        session = self.streaming_session

        while session and session.running:
            try:
                # Poll for incoming audio with timeout
                if await self.streaming_socket.poll(timeout=100):
                    chunk_bytes = await self.streaming_socket.recv()

                    # Decode incoming audio: f32 stereo interleaved
                    audio_np = np.frombuffer(chunk_bytes, dtype=np.float32)

                    # Convert stereo to mono for RAVE (average channels)
                    if len(audio_np) % 2 == 0:
                        stereo = audio_np.reshape(-1, 2)
                        mono = stereo.mean(axis=1)
                    else:
                        mono = audio_np

                    # Prepare tensor: (batch=1, channels=1, samples)
                    audio_tensor = torch.from_numpy(mono).unsqueeze(0).unsqueeze(0)
                    audio_tensor = audio_tensor.to(self.device)

                    # Process through RAVE (forward = encode + decode)
                    with torch.inference_mode():
                        processed = model.forward(audio_tensor)

                    # Convert back to numpy
                    processed_np = processed.squeeze().cpu().numpy()

                    # Convert mono back to stereo (duplicate to both channels)
                    stereo_out = np.column_stack([processed_np, processed_np])
                    output_bytes = stereo_out.astype(np.float32).tobytes()

                    # Send processed audio back
                    await self.streaming_socket.send(output_bytes)

                    # Update stats
                    session.frames_processed += len(mono)

            except zmq.ZMQError as e:
                if e.errno == zmq.EAGAIN:
                    continue  # Timeout, check if still running
                log.error(f"ZMQ error in streaming loop: {e}")
                break
            except Exception as e:
                log.exception(f"Error in streaming loop: {e}")
                # Continue on errors to maintain stream

        log.info(f"Streaming loop ended, processed {session.frames_processed if session else 0} frames")


async def main():
    """Run the RAVE service"""
    import sys

    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s %(levelname)s %(name)s: %(message)s",
    )

    # Parse endpoints from args or environment
    endpoint = os.environ.get("RAVE_ENDPOINT", "tcp://127.0.0.1:5591")
    streaming_endpoint = os.environ.get("RAVE_STREAMING_ENDPOINT", DEFAULT_STREAMING_ENDPOINT)

    if len(sys.argv) > 1:
        endpoint = sys.argv[1]
    if len(sys.argv) > 2:
        streaming_endpoint = sys.argv[2]

    service = RaveService(endpoint=endpoint, streaming_endpoint=streaming_endpoint)
    await service.start()


if __name__ == "__main__":
    asyncio.run(main())
