"""
hooteproto - Python client for Hootenanny

Static module that works without code generation.
Run `uv run hooteproto-gen` to generate typed client from server schemas.
"""
from .protocol import Connection, HootError, HootFrame, ProtocolError, Command, ContentType
from .tools import TOOLS, ToolCategory, ToolDef, get_tool, list_tools, tools_by_category

__version__ = "0.1.0"

__all__ = [
    # Protocol
    "Connection",
    "HootError",
    "HootFrame",
    "ProtocolError",
    "Command",
    "ContentType",
    # Tools
    "TOOLS",
    "ToolCategory",
    "ToolDef",
    "get_tool",
    "list_tools",
    "tools_by_category",
]

# Try to import generated client if available
try:
    from .client import HootClient
    from . import types
    __all__.append("HootClient")
    __all__.append("types")
except ImportError:
    # Generated client not available - use Connection directly
    HootClient = None  # type: ignore
