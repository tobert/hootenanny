# hooteproto

Python client for Hootenanny over ZMQ.

## Quick Start

```bash
# From repo root
just setup-python      # Install dependencies
just gen-python        # Generate typed client (needs hootenanny running)

# Or manually
cd python
uv sync
uv run hooteproto-gen --broker tcp://localhost:5580
```

## Usage

### Generated Typed Client (Recommended)

```python
import asyncio
from hooteproto import HootClient

async def main():
    async with HootClient("tcp://localhost:5580") as hoot:
        # Typed methods with IDE completion
        result = await hoot.abc_parse(abc="X:1\nK:C\nCDEF|")
        print(f"Key: {result['ast']['header']['key']['root']}")

        # Generate MIDI
        job = await hoot.orpheus_generate(
            temperature=1.0,
            max_tokens=512,
            tags=["experiment"]
        )
        print(f"Job: {job['job_id']}")

asyncio.run(main())
```

### Low-Level Connection (No Generation Required)

```python
import asyncio
from hooteproto import Connection

async def main():
    async with Connection("tcp://localhost:5580") as conn:
        result = await conn.call("abc_parse", abc="X:1\nK:C\nCDEF|")
        print(result)

asyncio.run(main())
```

## Development

```bash
# Using just (recommended)
just setup-python      # Install dependencies
just gen-python        # Generate client from hootenanny
just check-python      # Check if regeneration needed
just test-python       # Run tests

# Manual commands
cd python
uv sync --all-extras   # Install with dev deps
uv run hooteproto-gen --broker tcp://localhost:5580
uv run hooteproto-gen --check
uv run pytest
```

## Structure

```
hooteproto/
├── __init__.py    # Exports
├── protocol.py    # Static - HOOT01 ZMQ framing
├── tools.py       # Static - Tool registry (works offline)
├── gen.py         # Generator script
├── client.py      # Generated - Typed HootClient
├── types.py       # Generated - Dataclasses
└── tools.json     # Generated - Raw schemas
```

**Static files** work without hootenanny running. **Generated files** require
running `hooteproto-gen` with hootenanny available.
