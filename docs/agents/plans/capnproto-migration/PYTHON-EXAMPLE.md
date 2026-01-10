# Python ZMQ Client with Cap'n Proto

This document demonstrates how Python clients can use the same Cap'n Proto schemas as Rust, enabling type-safe cross-language communication without duplicating type definitions.

## Setup

```bash
pip install pycapnp pyzmq
```

## Example: Generate MIDI with Orpheus

```python
import capnp
import zmq
import uuid

# Load schemas - these are the SAME files Rust uses!
schema_dir = "crates/hooteproto/schemas"
common = capnp.load(f"{schema_dir}/common.capnp")
envelope = capnp.load(f"{schema_dir}/envelope.capnp")
tools = capnp.load(f"{schema_dir}/tools.capnp")

# Connect to hootenanny
ctx = zmq.Context()
sock = ctx.socket(zmq.REQ)
sock.connect("tcp://localhost:5555")

# Build request using generated Python classes
msg = envelope.Envelope.new_message()
msg.id.low, msg.id.high = uuid.uuid4().fields[:2]

# Access tool request (union type from tools.capnp)
req = msg.payload.toolRequest
orpheus = req.orpheusGenerate

# Set parameters - all types come from schemas!
orpheus.model = "base"
orpheus.temperature = 1.0
orpheus.topP = 0.95
orpheus.maxTokens = 1024

# Artifact metadata (from common.capnp)
metadata = orpheus.metadata
metadata.tags = ["python_client", "example"]
metadata.creator = "my_python_script"

# Send via HOOT01 frame (see examples/python_zmq_client.py for full implementation)
send_hoot_frame(sock, "hootenanny", msg)

# Receive and parse reply
reply = recv_hoot_frame(sock)
if reply.payload.which() == 'success':
    import json
    result = json.loads(reply.payload.success.result)
    print(f"Job ID: {result['job_id']}")
```

## Key Benefits

### 1. Type Safety Across Languages

**Schema definition** (common.capnp):
```capnp
enum JobStatus {
  pending @0;
  running @1;
  complete @2;
  failed @3;
  cancelled @4;
}
```

**Rust usage:**
```rust
use hooteproto::common_capnp::JobStatus;

let status = JobStatus::Running;
match status {
    JobStatus::Pending => { /* ... */ }
    JobStatus::Running => { /* ... */ }
    // Compiler enforces exhaustiveness
}
```

**Python usage:**
```python
import capnp
common = capnp.load("common.capnp")

status = common.JobStatus.running  # Auto-generated from schema
if status == common.JobStatus.complete:
    print("Job finished!")
```

### 2. No Type Duplication

You don't need to maintain parallel type definitions:

❌ **Without Cap'n Proto:**
```python
# Python types (must stay in sync with Rust manually!)
class JobStatus(Enum):
    PENDING = "pending"
    RUNNING = "running"
    # ...easy to get out of sync
```

✅ **With Cap'n Proto:**
```python
# Just load the schema Rust uses
common = capnp.load("common.capnp")
# All types available: JobStatus, WorkerType, etc.
```

### 3. Schema Evolution

Add new fields safely:

```capnp
struct JobInfo {
  jobId @0 :Text;
  status @1 :JobStatus;
  # New field - old clients ignore it gracefully
  priority @2 :UInt8 = 0;  # Default value for compatibility
}
```

Old Python/Rust clients continue working without changes.

### 4. IDE Support

Python IDEs understand the generated types:

```python
# Autocomplete works!
orpheus.model = "base"
orpheus.temperature = 1.0
orpheus.maxTokens = 1024  # IDE suggests valid fields

# Type checking works!
orpheus.temperature = "high"  # ← Error: expected Float32
```

## Complete Working Example

See `examples/python_zmq_client.py` for:
- HOOT01 frame building/parsing
- Sending tool requests
- Parsing typed responses
- Job status polling
- Schema introspection

## Testing

Run hootenanny:
```bash
cargo run -p hootenanny
```

Run Python client:
```bash
python examples/python_zmq_client.py
```

## Lua Example (Similar Pattern)

```lua
local capnp = require("capnp")

-- Load schemas
local common = capnp.load("crates/hooteproto/schemas/common.capnp")
local tools = capnp.load("crates/hooteproto/schemas/tools.capnp")

-- Use generated types
local status = common.JobStatus.running
local req = tools.ToolRequest.new()
req.orpheusGenerate.model = "base"
```

## Summary

By defining domain types in Cap'n Proto schemas:

1. ✅ **Single source of truth** - one schema, all languages
2. ✅ **Type safety** - compile-time errors in Rust, runtime validation in Python
3. ✅ **Zero duplication** - no manual JSON schema → Python classes conversion
4. ✅ **IDE support** - autocomplete, type hints work out of the box
5. ✅ **Forward compatibility** - schema evolution without breaking old clients

This is what "properly doing it in capnp" looks like! 🎉
