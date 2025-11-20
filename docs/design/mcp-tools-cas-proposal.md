# MCP Tool Design Proposal: Local Models & Content Addressable Storage

## Philosophy: References over Blobs

Large Language Models are excellent at reasoning about relationships and intentions but inefficient at processing large binary blobs (like MIDI files or images) directly in their context window.

We propose a system where tools operate on **References** (Content Addressable Hashes) rather than raw data. The "heavy lifting" of storing, retrieving, and transmitting binary data is handled by the host application (Rust), allowing the agent to focus on the composition and logic.

## 1. The Storage Layer: `HootenannyCAS`

We will implement a simple Content Addressable Storage (CAS) system to manage artifacts.

*   **Location:** `.hootenanny/cas/objects/` (Git-style loose object storage)
*   **Addressing:** SHA-256 hash of the content.
*   **Immutability:** Content at a specific address never changes.
*   **Deduplication:** Identical generated content is stored only once.

### Domain Integration
The `ConcreteEvent` in our domain model will be updated to support a `CasReference` variant, allowing conversation trees to point to these immutable artifacts.

---

## 2. Proposed Tool Definitions

Tools are grouped by their domain/model family to allow for flexible expansion (e.g., adding `gemini_music` or `claude_coder` later).

### 📦 Tool Set: `storage`

These tools allow agents to manage and inspect artifacts.

#### `cas_store`
*   **Purpose:** Manually save content to CAS.
*   **Inputs:**
    *   `content_base64`: *String* (Base64 encoded data)
    *   `mime_type`: *String* (e.g., "audio/midi", "text/x-python")
*   **Returns:** `hash` (SHA-256 string, e.g., "sha256:a1b2...")

#### `cas_read`
*   **Purpose:** Retrieve full content (use sparingly).
*   **Inputs:**
    *   `hash`: *String*
*   **Returns:** `{ "content_base64": "...", "mime_type": "..." }`

#### `cas_inspect`
*   **Purpose:** Lightweight introspection of an artifact.
*   **Inputs:**
    *   `hash`: *String*
*   **Returns:** Metadata only.
    ```json
    {
      "size_bytes": 1024,
      "mime_type": "audio/midi",
      "preview_hex": "4d546864...",
      "preview_text": null
    }
    ```

---

### 🎵 Tool Set: `orpheus_music`

Wrappers for the local Orpheus music transformer models (Port 2000).
*Note: The wrapper handles resolving input hashes to bytes and storing output bytes to hashes.*

#### `orpheus_generate`
*   **Purpose:** Generate or transform music.
*   **Inputs:**
    *   `model`: *String* (Enum: `"base"`, `"bridge"`, `"loops"`, `"children"`, `"mono_melodies"`)
    *   `task`: *String* (Enum: `"generate"`, `"continue"`, `"bridge"`, `"loops"`)
    *   `input_hash`: *Optional String* (CAS reference to source MIDI for continuation/bridging)
    *   `params`: *Object*
        *   `temperature`: *Float* (Default: 1.0)
        *   `top_p`: *Float* (Default: 0.95)
        *   `max_tokens`: *Integer* (Default: 1024)
*   **Returns:**
    ```json
    {
      "status": "success",
      "output_hash": "sha256:8f9a...",
      "summary": "Generated 128 tokens."
    }
    ```

#### `orpheus_classify`
*   **Purpose:** Analyze MIDI content.
*   **Inputs:**
    *   `model`: *String* (Default: `"classifier"`)
    *   `input_hash`: *String* (CAS reference to MIDI)
*   **Returns:**
    ```json
    {
      "is_human": true,
      "confidence": 0.98,
      "probabilities": { "human": 0.98, "ai": 0.02 }
    }
    ```

---

### 💻 Tool Set: `deepseek_coder`

Wrappers for the local DeepSeek coder model (Port 2001).

#### `deepseek_query`
*   **Purpose:** specialized code generation or consultation.
*   **Inputs:**
    *   `model`: *String* (Default: `"deepseek-coder-v2-lite"`)
    *   `messages`: *Array* (Standard `{"role": "...", "content": "..."}` format)
    *   `stream`: *Boolean* (Proof of concept for streaming responses)
*   **Returns:**
    ```json
    {
      "text": "def make_music(): ...",
      "finish_reason": "stop"
    }
    ```

---

## 3. Implementation Plan

1.  **Core CAS Module:** Implement `crates/hootenanny/src/cas.rs` (hashing, disk I/O).
2.  **Domain Update:** Update `ConcreteEvent` to include `CasReference`.
3.  **Tool Implementation:** Build the MCP tool structs that interact with the CAS and the LitServe endpoints.
