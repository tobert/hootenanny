"""
Headless runner for model services.

Allows testing services without ZMQ by calling handle_request() directly.
Provides a file↔CAS bridge for human-friendly I/O.
"""

import logging
from pathlib import Path
from typing import Any, Type

from . import cas
from .service import ModelService

log = logging.getLogger(__name__)


class HeadlessRunner:
    """
    Run a ModelService without ZMQ infrastructure.

    Provides:
    - Automatic file→CAS bridging for inputs
    - Automatic CAS→file bridging for outputs
    - Direct handle_request() invocation

    Example:
        runner = HeadlessRunner(OrpheusService)
        await runner.initialize()
        result = await runner.run_tool(
            "orpheus_generate",
            {"temperature": 0.8},
            output_file=Path("output.mid")
        )
    """

    # Default mapping from human-friendly file param names to service hash param names
    DEFAULT_FILE_PARAM_MAPPING = {
        "input_file": "midi_hash",
        "midi_file": "midi_hash",
        "audio_file": "audio_hash",
        "section_a_file": "section_a_hash",
        "seed_file": "seed_hash",
    }

    def __init__(
        self,
        service_class: Type[ModelService],
        cas_dir: Path | None = None,
        extra_file_mapping: dict[str, str] | None = None,
        **service_kwargs: Any,
    ):
        """
        Create a headless runner.

        Args:
            service_class: The ModelService subclass to instantiate
            cas_dir: Optional CAS directory (uses default if None)
            extra_file_mapping: Additional file→hash param mappings to merge
            **service_kwargs: Passed to service constructor
        """
        self.service_class = service_class
        self.service_kwargs = service_kwargs
        self.cas_dir = cas_dir or cas.default_cas_dir()
        self.service: ModelService | None = None
        self._initialized = False

        # Build file mapping (default + extras)
        self.file_param_mapping = self.DEFAULT_FILE_PARAM_MAPPING.copy()
        if extra_file_mapping:
            self.file_param_mapping.update(extra_file_mapping)

    async def initialize(self) -> None:
        """
        Initialize the service and load models.

        Must be called before run_tool().
        """
        if self._initialized:
            return

        log.info(f"Initializing headless {self.service_class.__name__}...")
        self.service = self.service_class(**self.service_kwargs)
        await self.service.load_model()
        self._initialized = True
        log.info(f"Headless {self.service_class.__name__} ready")

    def _store_file_to_cas(self, file_path: Path) -> str:
        """Store a file in CAS and return its hash."""
        if not file_path.exists():
            raise FileNotFoundError(f"Input file not found: {file_path}")

        data = file_path.read_bytes()
        content_hash = cas.store(data, self.cas_dir)
        log.debug(f"Stored {file_path} to CAS: {content_hash}")
        return content_hash

    def _fetch_from_cas_to_file(self, content_hash: str, output_path: Path) -> None:
        """Fetch content from CAS and write to file."""
        data = cas.fetch(content_hash, self.cas_dir)
        if data is None:
            raise ValueError(f"Content not found in CAS: {content_hash}")

        output_path.parent.mkdir(parents=True, exist_ok=True)
        output_path.write_bytes(data)
        log.debug(f"Wrote {len(data)} bytes to {output_path}")

    def _convert_file_params(self, params: dict[str, Any]) -> dict[str, Any]:
        """
        Convert file parameters to CAS hashes.

        Looks for keys in file_param_mapping and converts them to corresponding
        hash parameters by storing the file content in CAS.

        Warns if both a file param and its corresponding hash param are provided.
        """
        converted = {}

        for key, value in params.items():
            if key in self.file_param_mapping and value is not None:
                # Convert file path to CAS hash
                file_path = Path(value) if isinstance(value, str) else value
                content_hash = self._store_file_to_cas(file_path)
                hash_key = self.file_param_mapping[key]

                # Warn on collision (both file and hash provided)
                if hash_key in params:
                    log.warning(
                        f"Both '{key}' and '{hash_key}' provided; "
                        f"using file '{file_path}' (ignoring explicit hash)"
                    )

                converted[hash_key] = content_hash
                log.info(f"Converted {key}={file_path} → {hash_key}={content_hash[:16]}...")
            elif key not in self.file_param_mapping.values() or key not in converted:
                # Don't overwrite a hash we just set from a file
                converted[key] = value

        return converted

    async def run_tool(
        self,
        tool_name: str,
        params: dict[str, Any] | None = None,
        output_file: Path | None = None,
    ) -> dict[str, Any]:
        """
        Run a tool on the service.

        Args:
            tool_name: Name of the tool to invoke
            params: Tool parameters (file params auto-converted to hashes)
            output_file: If provided and result has content_hash, write to this file

        Returns:
            The tool response dict

        Raises:
            RuntimeError: If initialize() hasn't been called
            FileNotFoundError: If input file doesn't exist
        """
        if not self._initialized or self.service is None:
            raise RuntimeError("HeadlessRunner not initialized. Call initialize() first.")

        params = params or {}
        converted_params = self._convert_file_params(params)

        log.info(f"Running {tool_name} with params: {list(converted_params.keys())}")
        result = await self.service.handle_request(tool_name, converted_params)

        # Write output file if requested
        if output_file is not None:
            if "content_hash" in result:
                self._fetch_from_cas_to_file(result["content_hash"], output_file)
                log.info(f"Wrote output to {output_file}")
            else:
                log.warning(
                    f"Output file requested but no 'content_hash' in result. "
                    f"Available keys: {list(result.keys())}"
                )

        return result

    @property
    def tools(self) -> list[str]:
        """Get the list of tools supported by this service."""
        return self.service_class.TOOLS
