"""
Cap'n Proto protocol integration for hootpy

Loads schemas from the shared schemas/ directory and provides
encode/decode functions for tool requests and responses.
"""

import re
from pathlib import Path
from typing import Any

import capnp

# Disable import hook - we load schemas explicitly
capnp.remove_import_hook()

# Schema directory (symlinked to top-level schemas/)
SCHEMA_DIR = Path(__file__).parent / "schemas"

# Load schemas
_common_capnp = capnp.load(str(SCHEMA_DIR / "common.capnp"))
_envelope_capnp = capnp.load(str(SCHEMA_DIR / "envelope.capnp"))
_tools_capnp = capnp.load(str(SCHEMA_DIR / "tools.capnp"))
_responses_capnp = capnp.load(str(SCHEMA_DIR / "responses.capnp"))
_jobs_capnp = capnp.load(str(SCHEMA_DIR / "jobs.capnp"))


def _to_snake_case(name: str) -> str:
    """Convert camelCase to snake_case"""
    return re.sub(r"(?<!^)(?=[A-Z])", "_", name).lower()


def _to_camel_case(name: str) -> str:
    """Convert snake_case to camelCase"""
    components = name.split("_")
    return components[0] + "".join(x.title() for x in components[1:])


def _capnp_struct_to_dict(struct: Any) -> dict[str, Any]:
    """Convert a Cap'n Proto struct to a Python dict recursively"""
    result = {}

    # Get all fields from the struct's schema
    try:
        schema = struct.schema
    except AttributeError:
        # struct might be a primitive or not have a schema
        return {"_value": struct}

    # schema.fields is a dict mapping field names to field schemas
    for field_name in schema.fields:
        try:
            value = getattr(struct, field_name)

            # Handle different types
            if hasattr(value, "schema"):
                # Nested struct
                result[_to_snake_case(field_name)] = _capnp_struct_to_dict(value)
            elif isinstance(value, (list, capnp.lib.capnp._DynamicListReader)):
                # List
                result[_to_snake_case(field_name)] = [
                    _capnp_struct_to_dict(item) if hasattr(item, "schema") else item
                    for item in value
                ]
            elif isinstance(value, bytes):
                result[_to_snake_case(field_name)] = value
            elif isinstance(value, capnp.lib.capnp._DynamicEnum):
                result[_to_snake_case(field_name)] = str(value)
            else:
                result[_to_snake_case(field_name)] = value
        except (AttributeError, capnp.KjException):
            # Field not set or union variant not active
            continue

    return result


def decode_envelope(body: bytes) -> tuple[str, dict[str, Any]]:
    """
    Decode a Cap'n Proto envelope body.

    Returns:
        Tuple of (payload_type, payload_dict)
        payload_type is the union variant name in snake_case
    """
    # pycapnp from_bytes returns a context manager
    with _envelope_capnp.Envelope.from_bytes(body) as msg:
        return _decode_envelope_inner(msg)


def _decode_envelope_inner(msg: Any) -> tuple[str, dict[str, Any]]:
    """Inner decode that works with the message object"""
    payload = msg.payload

    # Get the active union variant
    variant = payload.which()

    if variant == "toolRequest":
        tool_req = payload.toolRequest
        tool_variant = tool_req.which()
        params = getattr(tool_req, tool_variant)
        return (
            _to_snake_case(tool_variant),
            _capnp_struct_to_dict(params) if hasattr(params, "schema") else {},
        )
    elif variant == "toolResponse":
        tool_resp = payload.toolResponse
        resp_variant = tool_resp.which()
        params = getattr(tool_resp, resp_variant)
        return (
            _to_snake_case(resp_variant),
            _capnp_struct_to_dict(params) if hasattr(params, "schema") else {},
        )
    elif variant == "error":
        error = payload.error
        return (
            "error",
            {
                "code": error.code,
                "message": error.message,
            },
        )
    else:
        # Simple variants (ping, pong, etc.)
        return (_to_snake_case(variant), {})


def decode_tool_request(body: bytes) -> tuple[str, dict[str, Any]]:
    """
    Decode a Cap'n Proto tool request body.

    Returns:
        Tuple of (tool_name, params_dict)
    """
    return decode_envelope(body)


def encode_envelope(
    request_id_bytes: bytes,
    traceparent: str,
    payload_type: str,
    payload_data: dict[str, Any],
) -> bytes:
    """
    Encode a Cap'n Proto envelope.

    Args:
        request_id_bytes: 16-byte UUID
        traceparent: W3C trace context string (may be empty)
        payload_type: The envelope payload variant (e.g., "tool_response")
        payload_data: The payload data as a dict

    Returns:
        Serialized Cap'n Proto message bytes
    """
    msg = _envelope_capnp.Envelope.new_message()

    # Set ID
    id_builder = msg.init("id")
    id_builder.low = int.from_bytes(request_id_bytes[:8], "little")
    id_builder.high = int.from_bytes(request_id_bytes[8:16], "little")

    # Set traceparent
    msg.traceparent = traceparent

    # Set payload based on type
    payload = msg.init("payload")
    _set_payload_variant(payload, payload_type, payload_data)

    return msg.to_bytes()


def encode_tool_request(
    request_id_bytes: bytes,
    tool_name: str,
    params: dict[str, Any],
    traceparent: str = "",
) -> bytes:
    """
    Encode a tool request into an envelope.

    Args:
        request_id_bytes: 16-byte UUID
        tool_name: Tool name in snake_case (e.g., "rave_encode")
        params: Request parameters as a dict
        traceparent: W3C trace context string (optional)

    Returns:
        Serialized Cap'n Proto message bytes
    """
    msg = _envelope_capnp.Envelope.new_message()

    # Set ID
    id_builder = msg.init("id")
    id_builder.low = int.from_bytes(request_id_bytes[:8], "little")
    id_builder.high = int.from_bytes(request_id_bytes[8:16], "little")

    # Set traceparent
    msg.traceparent = traceparent

    # Set payload as tool request
    payload = msg.init("payload")
    tool_request = payload.init("toolRequest")

    # Set the tool variant (convert snake_case to camelCase)
    camel_tool = _to_camel_case(tool_name)
    try:
        request_builder = tool_request.init(camel_tool)
        _dict_to_capnp_struct(request_builder, params)
    except Exception as e:
        raise ValueError(f"Failed to encode tool request '{tool_name}': {e}")

    return msg.to_bytes()


def encode_tool_response(
    request_id_bytes: bytes,
    response_type: str,
    response_data: dict[str, Any],
) -> bytes:
    """
    Encode a tool response into an envelope.

    Args:
        request_id_bytes: 16-byte UUID from the request
        response_type: Response variant name in snake_case (e.g., "rave_encoded")
        response_data: Response data as a dict

    Returns:
        Serialized Cap'n Proto message bytes
    """
    msg = _envelope_capnp.Envelope.new_message()

    # Set ID
    id_builder = msg.init("id")
    id_builder.low = int.from_bytes(request_id_bytes[:8], "little")
    id_builder.high = int.from_bytes(request_id_bytes[8:16], "little")

    # Set traceparent (empty for responses)
    msg.traceparent = ""

    # Set payload as tool response
    payload = msg.init("payload")
    tool_response = payload.init("toolResponse")

    # Set the response variant
    camel_type = _to_camel_case(response_type)
    response_builder = tool_response.init(camel_type)
    _dict_to_capnp_struct(response_builder, response_data)

    return msg.to_bytes()


def encode_error_response(
    request_id_bytes: bytes,
    code: str,
    message: str,
) -> bytes:
    """
    Encode an error response into an envelope.

    Args:
        request_id_bytes: 16-byte UUID from the request
        code: Error code string
        message: Human-readable error message

    Returns:
        Serialized Cap'n Proto message bytes
    """
    msg = _envelope_capnp.Envelope.new_message()

    # Set ID
    id_builder = msg.init("id")
    id_builder.low = int.from_bytes(request_id_bytes[:8], "little")
    id_builder.high = int.from_bytes(request_id_bytes[8:16], "little")

    # Set traceparent (empty for responses)
    msg.traceparent = ""

    # Set payload as error
    payload = msg.init("payload")
    error = payload.init("error")
    error.code = code
    error.message = message

    return msg.to_bytes()


def _set_payload_variant(
    payload: Any, variant_name: str, data: dict[str, Any]
) -> None:
    """Set a payload union variant"""
    camel_name = _to_camel_case(variant_name)

    if variant_name == "tool_response":
        # Tool response needs special handling
        tool_response = payload.init("toolResponse")
        if "type" in data:
            resp_type = _to_camel_case(data["type"])
            resp_builder = tool_response.init(resp_type)
            _dict_to_capnp_struct(resp_builder, data.get("data", {}))
    elif variant_name == "error":
        error = payload.init("error")
        error.code = data.get("code", "unknown")
        error.message = data.get("message", "Unknown error")
    elif hasattr(payload, camel_name):
        # Simple variant or struct
        try:
            builder = payload.init(camel_name)
            if data:
                _dict_to_capnp_struct(builder, data)
        except capnp.KjException:
            # Void variant
            setattr(payload, camel_name, None)


def _dict_to_capnp_struct(builder: Any, data: dict[str, Any]) -> None:
    """Convert a Python dict to Cap'n Proto struct fields"""
    for key, value in data.items():
        camel_key = _to_camel_case(key)
        if value is None:
            continue

        try:
            if isinstance(value, dict):
                # Nested struct
                nested = builder.init(camel_key)
                _dict_to_capnp_struct(nested, value)
            elif isinstance(value, list):
                # List
                if value and isinstance(value[0], dict):
                    list_builder = builder.init(camel_key, len(value))
                    for i, item in enumerate(value):
                        _dict_to_capnp_struct(list_builder[i], item)
                else:
                    setattr(builder, camel_key, value)
            elif isinstance(value, bytes):
                setattr(builder, camel_key, value)
            else:
                setattr(builder, camel_key, value)
        except (AttributeError, capnp.KjException):
            # Field doesn't exist or wrong type - skip
            pass
