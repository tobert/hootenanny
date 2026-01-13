"""
hootpy - HOOT01 protocol library for Python model services

Provides the wire protocol, Cap'n Proto integration, and ZMQ service base
classes for building model services that speak native hootenanny protocol.
"""

from .frame import HootFrame, Command, ContentType, PROTOCOL_VERSION
from .errors import (
    ToolError,
    ValidationError,
    ServiceError,
    NotFoundError,
    TimeoutError,
    CancelledError,
    InternalError,
    ErrorCategory,
)
from .service import ModelService, ServiceConfig, SingleJobGuard
from .client import HootClient, ClientConfig, request
from .protocol import encode_tool_request, decode_envelope
from . import cas

__version__ = "0.1.0"

__all__ = [
    # Frame protocol
    "HootFrame",
    "Command",
    "ContentType",
    "PROTOCOL_VERSION",
    # Errors
    "ToolError",
    "ValidationError",
    "ServiceError",
    "NotFoundError",
    "TimeoutError",
    "CancelledError",
    "InternalError",
    "ErrorCategory",
    # Service
    "ModelService",
    "ServiceConfig",
    "SingleJobGuard",
    # Client
    "HootClient",
    "ClientConfig",
    "request",
    # Protocol
    "encode_tool_request",
    "decode_envelope",
    # CAS
    "cas",
]
