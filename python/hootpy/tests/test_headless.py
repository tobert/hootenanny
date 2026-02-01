"""Tests for HeadlessRunner."""

import pytest
from typing import Any

from hootpy.headless import HeadlessRunner
from hootpy.service import ModelService, ServiceConfig
from hootpy import cas


class MockService(ModelService):
    """Mock service for testing HeadlessRunner."""

    TOOLS = ["mock_generate", "mock_process"]

    # Allow injecting CAS dir for testing
    _test_cas_dir = None

    def __init__(self, **_kwargs: Any):
        super().__init__(ServiceConfig(name="mock", endpoint="tcp://127.0.0.1:5555"))
        self.load_called = False
        self.last_request: tuple[str, dict[str, Any]] | None = None

    async def load_model(self):
        self.load_called = True

    async def handle_request(self, tool_name: str, params: dict[str, Any]) -> dict[str, Any]:
        self.last_request = (tool_name, params)

        if tool_name == "mock_generate":
            # Simulate generating content and storing in CAS
            content = b"mock generated content"
            content_hash = cas.store(content, MockService._test_cas_dir)
            return {"content_hash": content_hash, "num_tokens": 42}

        elif tool_name == "mock_process":
            # Simulate processing input
            midi_hash = params.get("midi_hash")
            return {"processed": True, "input_hash": midi_hash}

        return {"tool": tool_name, "params": params}


class TestHeadlessRunnerInit:
    """Test HeadlessRunner initialization."""

    def test_create_runner(self):
        """Can create a runner with a service class."""
        runner = HeadlessRunner(MockService)
        assert runner.service_class == MockService
        assert runner.service is None
        assert not runner._initialized

    def test_custom_cas_dir(self, tmp_path):
        """Can specify custom CAS directory."""
        runner = HeadlessRunner(MockService, cas_dir=tmp_path)
        assert runner.cas_dir == tmp_path


class TestHeadlessRunnerInitialize:
    """Test HeadlessRunner.initialize()."""

    @pytest.mark.asyncio
    async def test_initialize_loads_model(self):
        """initialize() calls load_model() on service."""
        runner = HeadlessRunner(MockService)
        await runner.initialize()

        assert runner._initialized
        assert runner.service is not None
        assert runner.service.load_called

    @pytest.mark.asyncio
    async def test_initialize_idempotent(self):
        """Multiple initialize() calls don't reload."""
        runner = HeadlessRunner(MockService)
        await runner.initialize()

        service1 = runner.service
        await runner.initialize()

        assert runner.service is service1


class TestHeadlessRunnerRunTool:
    """Test HeadlessRunner.run_tool()."""

    @pytest.mark.asyncio
    async def test_run_tool_without_init_fails(self):
        """run_tool() before initialize() raises RuntimeError."""
        runner = HeadlessRunner(MockService)

        with pytest.raises(RuntimeError, match="not initialized"):
            await runner.run_tool("mock_generate", {})

    @pytest.mark.asyncio
    async def test_run_tool_basic(self, tmp_path):
        """Can run a tool and get results."""
        runner = HeadlessRunner(MockService, cas_dir=tmp_path)
        await runner.initialize()

        result = await runner.run_tool("mock_generate", {"temperature": 0.8})

        assert "content_hash" in result
        assert result["num_tokens"] == 42

    @pytest.mark.asyncio
    async def test_run_tool_with_output_file(self, tmp_path):
        """Output file is written when content_hash returned."""
        cas_dir = tmp_path / "cas"
        cas_dir.mkdir()
        output_file = tmp_path / "output.mid"

        # Point mock service to same CAS dir
        MockService._test_cas_dir = cas_dir

        try:
            runner = HeadlessRunner(MockService, cas_dir=cas_dir)
            await runner.initialize()

            await runner.run_tool("mock_generate", {}, output_file=output_file)

            assert output_file.exists()
            assert output_file.read_bytes() == b"mock generated content"
        finally:
            MockService._test_cas_dir = None


class TestFileParamConversion:
    """Test file→CAS parameter conversion."""

    @pytest.mark.asyncio
    async def test_input_file_converted_to_hash(self, tmp_path):
        """input_file param is converted to midi_hash."""
        cas_dir = tmp_path / "cas"
        cas_dir.mkdir()
        input_file = tmp_path / "input.mid"
        input_file.write_bytes(b"test midi data")

        runner = HeadlessRunner(MockService, cas_dir=cas_dir)
        await runner.initialize()

        await runner.run_tool("mock_process", {"input_file": input_file})

        # Check that the service received midi_hash, not input_file
        assert isinstance(runner.service, MockService)
        assert runner.service.last_request is not None
        _, params = runner.service.last_request
        assert "midi_hash" in params
        assert "input_file" not in params

        # Verify the hash matches
        expected_hash = cas.hash_bytes(b"test midi data")
        assert params["midi_hash"] == expected_hash

    @pytest.mark.asyncio
    async def test_missing_input_file_raises(self, tmp_path):
        """FileNotFoundError raised for missing input file."""
        runner = HeadlessRunner(MockService, cas_dir=tmp_path)
        await runner.initialize()

        with pytest.raises(FileNotFoundError, match="not found"):
            await runner.run_tool("mock_process", {"input_file": "/nonexistent/file.mid"})

    @pytest.mark.asyncio
    async def test_non_file_params_passed_through(self, tmp_path):
        """Non-file parameters are passed through unchanged."""
        runner = HeadlessRunner(MockService, cas_dir=tmp_path)
        await runner.initialize()

        await runner.run_tool("mock_generate", {
            "temperature": 0.8,
            "max_tokens": 1024,
        })

        assert isinstance(runner.service, MockService)
        assert runner.service.last_request is not None
        _, params = runner.service.last_request
        assert params["temperature"] == 0.8
        assert params["max_tokens"] == 1024


class TestToolsList:
    """Test tools property."""

    def test_tools_returns_service_tools(self):
        """tools property returns TOOLS from service class."""
        runner = HeadlessRunner(MockService)
        assert runner.tools == ["mock_generate", "mock_process"]
