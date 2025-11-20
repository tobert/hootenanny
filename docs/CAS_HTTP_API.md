# CAS HTTP API

Content Addressable Storage HTTP endpoints for storing and retrieving binary artifacts.

## Base URL

The CAS API is available at the `/cas` endpoint of the Hootenanny server.

## Endpoints

### Upload Content

Store binary content and receive a content-addressable hash.

**Endpoint:** `POST /cas`

**Headers:**
- `Content-Type`: MIME type of the content (e.g., `text/plain`, `audio/midi`, `image/jpeg`)
  - Defaults to `application/octet-stream` if not provided

**Request Body:** Raw binary content

**Response:**
- **200 OK**: Returns the BLAKE3 hash (32 hex characters)
- **500 Internal Server Error**: Error message as text

**Example:**
```bash
# Upload text
curl -X POST http://localhost:8080/cas \
  -H "Content-Type: text/plain" \
  --data "Hello, World!"

# Response: 17b2d4dd810b02cd9758fee2dd734638

# Upload binary file
curl -X POST http://localhost:8080/cas \
  -H "Content-Type: audio/midi" \
  --data-binary @music.mid

# Response: a7f8d9e6c1b2a3f4e5d6c7b8a9f0e1d2
```

---

### Download Content

Retrieve stored content by its hash.

**Endpoint:** `GET /cas/{hash}`

**Path Parameters:**
- `hash`: 32-character hex BLAKE3 hash (e.g., `17b2d4dd810b02cd9758fee2dd734638`)

**Response:**
- **200 OK**: Returns raw binary content with `Content-Type: application/octet-stream`
- **404 Not Found**: Hash not found
- **400 Bad Request**: Invalid hash format

**Example:**
```bash
# Download content
curl http://localhost:8080/cas/17b2d4dd810b02cd9758fee2dd734638

# Response: Hello, World!

# Download to file
curl -o output.mid \
  http://localhost:8080/cas/a7f8d9e6c1b2a3f4e5d6c7b8a9f0e1d2
```

---

## Content Addressing

- **Hashing**: BLAKE3 (truncated to 128 bits / 16 bytes / 32 hex chars)
- **Deduplication**: Identical content produces the same hash, stored once
- **Immutability**: Content cannot be modified; each change creates a new hash

## Storage Structure

Content is stored in a content-addressable filesystem:

```
.hootenanny/cas/
├── objects/
│   ├── 17/
│   │   └── b2d4dd810b02cd9758fee2dd734638
│   └── a7/
│       └── f8d9e6c1b2a3f4e5d6c7b8a9f0e1d2
└── metadata/
    ├── 17/
    │   └── b2d4dd810b02cd9758fee2dd734638.json
    └── a7/
        └── f8d9e6c1b2a3f4e5d6c7b8a9f0e1d2.json
```

**Metadata format:**
```json
{
  "mime_type": "audio/midi",
  "size": 2048
}
```

---

## Use Cases

### Model-Generated Content

Models can store generated artifacts directly via HTTP:

```python
import requests
import base64

# Generate MIDI with Orpheus (returns base64)
midi_base64 = orpheus_generate(...)
midi_bytes = base64.b64decode(midi_base64)

# Store in CAS
response = requests.post(
    "http://localhost:8080/cas",
    headers={"Content-Type": "audio/midi"},
    data=midi_bytes
)
hash = response.text

# Later retrieve it
midi_data = requests.get(f"http://localhost:8080/cas/{hash}").content
```

### Cross-Process Sharing

Use hashes as immutable references across processes:

```bash
# Process A stores content
HASH=$(echo "shared data" | curl -X POST http://localhost:8080/cas \
  -H "Content-Type: text/plain" --data-binary @-)

# Process B retrieves by hash
curl http://localhost:8080/cas/$HASH
```

---

## Notes

- **MIME Type Preservation**: Upload `Content-Type` is stored in metadata but not returned on download (currently returns `application/octet-stream`)
- **Validation**: Hash format must be exactly 32 hexadecimal characters
- **Streaming**: Large files are streamed efficiently (no size limits)
- **Idempotent**: Uploading the same content multiple times returns the same hash
