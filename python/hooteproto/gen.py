#!/usr/bin/env python3
"""
Generate typed client from Hootenanny's list_tools.

Usage:
    uv run hooteproto-gen [--broker URL] [--check]
"""
from __future__ import annotations

import argparse
import asyncio
import hashlib
import json
import sys
from datetime import datetime
from pathlib import Path
from typing import Any

from .protocol import Connection


def snake_to_pascal(s: str) -> str:
    """Convert snake_case to PascalCase."""
    return "".join(word.capitalize() for word in s.split("_"))


def json_type_to_python(schema: dict[str, Any]) -> str:
    """Map JSON Schema type to Python type annotation."""
    t = schema.get("type")
    if t == "string":
        return "str"
    elif t == "integer":
        return "int"
    elif t == "number":
        return "float"
    elif t == "boolean":
        return "bool"
    elif t == "array":
        items = schema.get("items", {})
        inner = json_type_to_python(items) if items else "Any"
        return f"list[{inner}]"
    elif t == "object":
        return "dict[str, Any]"
    elif t is None and "anyOf" in schema:
        types = [json_type_to_python(s) for s in schema["anyOf"] if s.get("type") != "null"]
        return types[0] if types else "Any"
    elif t is None and "$ref" in schema:
        # Reference to another type - treat as Any for now
        return "Any"
    return "Any"


def extract_schema_properties(schema: dict[str, Any]) -> tuple[dict[str, Any], set[str]]:
    """Extract properties and required fields from JSON Schema."""
    props = schema.get("properties", {})
    required = set(schema.get("required", []))
    return props, required


def generate_types(tools: list[dict[str, Any]]) -> str:
    """Generate types.py content."""
    lines = [
        '"""',
        "Hootenanny tool parameter types.",
        "",
        f"Generated: {datetime.now().isoformat()}",
        "Regenerate: uv run hooteproto-gen",
        '"""',
        "from __future__ import annotations",
        "",
        "from dataclasses import dataclass, field",
        "from typing import Any, Optional",
        "",
    ]

    for tool in sorted(tools, key=lambda t: t["name"]):
        tool_name = tool["name"]
        schema = tool.get("input_schema", {})
        desc = tool.get("description", f"Parameters for {tool_name}")

        class_name = snake_to_pascal(tool_name) + "Params"
        props, required = extract_schema_properties(schema)

        lines.append("@dataclass")
        lines.append(f"class {class_name}:")
        lines.append(f'    """{desc}"""')

        if not props:
            lines.append("    pass")
            lines.append("")
            continue

        # Required fields first
        for prop_name, prop_schema in props.items():
            if prop_name in required:
                py_type = json_type_to_python(prop_schema)
                prop_desc = prop_schema.get("description", "")
                comment = f"  # {prop_desc}" if prop_desc else ""
                lines.append(f"    {prop_name}: {py_type}{comment}")

        # Optional fields with defaults
        for prop_name, prop_schema in props.items():
            if prop_name not in required:
                py_type = json_type_to_python(prop_schema)
                prop_desc = prop_schema.get("description", "")
                comment = f"  # {prop_desc}" if prop_desc else ""

                if py_type.startswith("list"):
                    default = "field(default_factory=list)"
                else:
                    py_type = f"Optional[{py_type}]"
                    default = "None"

                lines.append(f"    {prop_name}: {py_type} = {default}{comment}")

        lines.append("")

    return "\n".join(lines)


def generate_client(tools: list[dict[str, Any]]) -> str:
    """Generate client.py content."""
    lines = [
        '"""',
        "Typed Hootenanny client.",
        "",
        f"Generated: {datetime.now().isoformat()}",
        "Regenerate: uv run hooteproto-gen",
        '"""',
        "from __future__ import annotations",
        "",
        "from typing import Any",
        "",
        "from .protocol import Connection, HootError",
        "",
        "",
        "class HootClient:",
        '    """Async client for Hootenanny with typed methods."""',
        "",
        '    def __init__(self, broker_url: str = "tcp://localhost:5555"):',
        "        self._conn = Connection(broker_url)",
        "",
        "    @property",
        "    def broker_url(self) -> str:",
        '        """The broker URL this client connects to."""',
        "        return self._conn.broker_url",
        "",
        '    async def __aenter__(self) -> "HootClient":',
        "        await self._conn.connect()",
        "        return self",
        "",
        "    async def __aexit__(self, *args) -> None:",
        "        await self._conn.close()",
        "",
        "    async def connect(self) -> None:",
        '        """Explicitly connect (alternative to context manager)."""',
        "        await self._conn.connect()",
        "",
        "    async def close(self) -> None:",
        '        """Explicitly close (alternative to context manager)."""',
        "        await self._conn.close()",
        "",
    ]

    for tool in sorted(tools, key=lambda t: t["name"]):
        tool_name = tool["name"]
        schema = tool.get("input_schema", {})
        desc = tool.get("description", f"Call {tool_name}.")

        props, required = extract_schema_properties(schema)

        # Build signature
        params = ["self"]
        if props:
            params.append("*")

        for prop_name, prop_schema in props.items():
            if prop_name in required:
                py_type = json_type_to_python(prop_schema)
                params.append(f"{prop_name}: {py_type}")

        for prop_name, prop_schema in props.items():
            if prop_name not in required:
                py_type = json_type_to_python(prop_schema)
                params.append(f"{prop_name}: {py_type} | None = None")

        sig = ", ".join(params)
        kwargs = ", ".join(f"{p}={p}" for p in props)

        lines.extend([
            f"    async def {tool_name}({sig}) -> dict[str, Any]:",
            f'        """{desc}"""',
            f'        return await self._conn.call("{tool_name}", {kwargs})',
            "",
        ])

    # Dynamic fallback for tools not in generated list
    lines.extend([
        "    def __getattr__(self, name: str):",
        '        """Fallback for tools not in generated client."""',
        '        if name.startswith("_"):',
        "            raise AttributeError(name)",
        "",
        "        async def call(**kwargs: Any) -> dict[str, Any]:",
        "            return await self._conn.call(name, **kwargs)",
        "",
        "        return call",
    ])

    return "\n".join(lines)


def compute_hash(tools: list[dict[str, Any]]) -> str:
    """Compute hash of tool schemas for change detection."""
    return hashlib.sha256(
        json.dumps(tools, sort_keys=True).encode()
    ).hexdigest()[:12]


async def fetch_tools(broker_url: str) -> list[dict[str, Any]]:
    """Fetch tool list from hootenanny."""
    async with Connection(broker_url) as conn:
        result = await conn.call("list_tools")

        # Protocol layer unwraps ["tool_list", data] -> data
        # So result is either:
        #   - list of [name, desc, schema] tuples
        #   - list of dicts with name/description/input_schema
        #   - dict with "tools" key

        if isinstance(result, list):
            if not result:
                return []
            # List of dicts
            if isinstance(result[0], dict):
                return result
            # List of [name, desc, schema] tuples
            if isinstance(result[0], (list, tuple)):
                return [
                    {
                        "name": tool[0],
                        "description": tool[1],
                        "input_schema": tool[2] if len(tool) > 2 else {},
                    }
                    for tool in result
                ]

        # Handle dict format (wrapped in tools key)
        if isinstance(result, dict):
            tools = result.get("tools", [])
            if tools and isinstance(tools[0], dict):
                return tools
            return [
                {
                    "name": tool[0],
                    "description": tool[1],
                    "input_schema": tool[2] if len(tool) > 2 else {},
                }
                for tool in tools
            ]

        raise ValueError(f"Unexpected response format: {type(result)}")


def main() -> None:
    """Entry point for hooteproto-gen command."""
    parser = argparse.ArgumentParser(
        description="Generate hooteproto client from Hootenanny schemas"
    )
    parser.add_argument(
        "--broker",
        default="tcp://localhost:5555",
        help="Hootenanny broker URL (default: tcp://localhost:5555)",
    )
    parser.add_argument(
        "--check",
        action="store_true",
        help="Check if regeneration is needed (exit 1 if so)",
    )
    args = parser.parse_args()

    # Find output directory (same as this module)
    out_dir = Path(__file__).parent

    print(f"🔌 Connecting to {args.broker}...")

    try:
        tools = asyncio.run(fetch_tools(args.broker))
    except Exception as e:
        print(f"❌ Failed to fetch tools: {e}")
        print("   Is hootenanny running?")
        sys.exit(1)

    print(f"📋 Fetched {len(tools)} tools with schemas")

    schema_hash = compute_hash(tools)

    if args.check:
        hash_file = out_dir / ".schema_hash"
        if hash_file.exists() and hash_file.read_text().strip() == schema_hash:
            print(f"✅ Up to date ({schema_hash})")
            sys.exit(0)
        print(f"⚠️  Regeneration needed (hash: {schema_hash})")
        sys.exit(1)

    # Generate files
    types_content = generate_types(tools)
    client_content = generate_client(tools)

    (out_dir / "types.py").write_text(types_content)
    (out_dir / "client.py").write_text(client_content)
    (out_dir / "tools.json").write_text(json.dumps(tools, indent=2))
    (out_dir / ".schema_hash").write_text(schema_hash)

    print(f"✨ Generated {out_dir}/")
    print(f"   types.py   - {len(tools)} dataclasses")
    print(f"   client.py  - {len(tools)} typed methods")
    print(f"   tools.json - raw schemas")
    print(f"   hash: {schema_hash}")


if __name__ == "__main__":
    main()
