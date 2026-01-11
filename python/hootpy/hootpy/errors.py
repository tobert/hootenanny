"""
Typed errors for hootpy

Mirrors the error types from hooteproto::envelope::ToolError in Rust.
All errors are categorized for consistent handling across the protocol.
"""

from dataclasses import dataclass, field
from enum import Enum
from typing import Any


class ErrorCategory(Enum):
    """Error categories matching Rust ToolError variants"""

    VALIDATION = "validation"
    NOT_FOUND = "not_found"
    SERVICE = "service"
    INTERNAL = "internal"
    CANCELLED = "cancelled"
    TIMEOUT = "timeout"
    PERMISSION = "permission"


@dataclass
class ToolError(Exception):
    """Base error for all tool errors"""

    category: ErrorCategory = ErrorCategory.INTERNAL
    message: str = ""
    details: dict[str, Any] = field(default_factory=dict)

    def __str__(self) -> str:
        return f"[{self.category.value}] {self.message}"

    def to_dict(self) -> dict[str, Any]:
        """Serialize to dictionary for protocol encoding"""
        result = {
            "category": self.category.value,
            "message": self.message,
        }
        if self.details:
            result["details"] = self.details
        return result

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> "ToolError":
        """Deserialize from dictionary"""
        category = ErrorCategory(data.get("category", "internal"))
        message = data.get("message", "Unknown error")
        details = data.get("details", {})

        # Return appropriate subclass based on category
        error_classes = {
            ErrorCategory.VALIDATION: ValidationError,
            ErrorCategory.NOT_FOUND: NotFoundError,
            ErrorCategory.SERVICE: ServiceError,
            ErrorCategory.INTERNAL: InternalError,
            ErrorCategory.CANCELLED: CancelledError,
            ErrorCategory.TIMEOUT: TimeoutError,
            ErrorCategory.PERMISSION: PermissionError,
        }

        error_cls = error_classes.get(category, ToolError)
        return error_cls(category=category, message=message, details=details)


@dataclass
class ValidationError(ToolError):
    """Invalid input or parameters"""

    category: ErrorCategory = field(default=ErrorCategory.VALIDATION)
    field_name: str | None = None
    code: str = "invalid_input"

    def __post_init__(self):
        if self.field_name:
            self.details["field"] = self.field_name
        if self.code != "invalid_input":
            self.details["code"] = self.code


@dataclass
class NotFoundError(ToolError):
    """Resource not found (CAS hash, artifact, job, etc.)"""

    category: ErrorCategory = field(default=ErrorCategory.NOT_FOUND)
    resource_type: str = ""
    resource_id: str = ""

    def __post_init__(self):
        if self.resource_type:
            self.details["resource_type"] = self.resource_type
        if self.resource_id:
            self.details["resource_id"] = self.resource_id


@dataclass
class ServiceError(ToolError):
    """External service failure"""

    category: ErrorCategory = field(default=ErrorCategory.SERVICE)
    service_name: str = ""
    code: str = ""
    retryable: bool = False

    def __post_init__(self):
        if self.service_name:
            self.details["service"] = self.service_name
        if self.code:
            self.details["code"] = self.code
        self.details["retryable"] = self.retryable


@dataclass
class InternalError(ToolError):
    """Internal error (should not happen)"""

    category: ErrorCategory = field(default=ErrorCategory.INTERNAL)


@dataclass
class CancelledError(ToolError):
    """Operation was cancelled"""

    category: ErrorCategory = field(default=ErrorCategory.CANCELLED)
    job_id: str | None = None

    def __post_init__(self):
        if self.job_id:
            self.details["job_id"] = self.job_id


@dataclass
class TimeoutError(ToolError):
    """Operation timed out"""

    category: ErrorCategory = field(default=ErrorCategory.TIMEOUT)
    timeout_ms: int | None = None

    def __post_init__(self):
        if self.timeout_ms:
            self.details["timeout_ms"] = self.timeout_ms


@dataclass
class PermissionError(ToolError):
    """Access denied"""

    category: ErrorCategory = field(default=ErrorCategory.PERMISSION)
    resource: str | None = None
    action: str | None = None

    def __post_init__(self):
        if self.resource:
            self.details["resource"] = self.resource
        if self.action:
            self.details["action"] = self.action
