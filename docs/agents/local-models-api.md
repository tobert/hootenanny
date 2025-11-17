# Local Model APIs

This document details the APIs for local machine learning models running on `localhost`. These models are exposed via `LitServe` and can be accessed via HTTP POST requests.

## 🎵 Orpheus Music Generation API

**Port:** `2000`
**Endpoint:** `POST http://localhost:2000/predict`
**Content-Type:** `application/json`

The Orpheus API provides access to a suite of music transformer models capable of generating, continuing, and classifying MIDI sequences.

### Request Parameters

| Parameter | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `model` | `string` | `"base"` | The model variant to use. Options: `"base"`, `"classifier"`, `"bridge"`, `"loops"`, `"children"`, `"mono_melodies"`. |
| `task` | `string` | `"generate"` | The operation to perform. Options: `"generate"`, `"continue"`, `"classify"`, `"bridge"`, `"loops"`. |
| `midi_input` | `string` | `null` | **Base64 encoded** MIDI data. Required for `continue`, `bridge`, and `classify` tasks. |
| `temperature` | `float` | `1.0` | Controls randomness. Higher values (e.g., 1.2) make output more chaotic; lower values (e.g., 0.8) make it more predictable. |
| `top_p` | `float` | `0.95` | Nucleus sampling parameter. Limits sampling to the top cumulative probability. |
| `max_tokens` | `int` | `1024` | Maximum number of tokens to generate. |
| `midi_a` | `string` | `null` | **Base64 encoded** MIDI data. Used specifically for the `bridge` task as the starting section. |
| `num_variations`| `int` | `1` | Number of variations to generate (functionality depends on model). |

### Response Format

The response is a JSON object.

**Success (Generation tasks):**

```json
{
  "midi_base64": "<Base64 Encoded MIDI String>",
  "num_tokens": 150,
  "task": "generate"
}
```

**Success (Classification task):**

```json
{
  "classification": {
    "is_human": true,
    "confidence": 0.98,
    "probabilities": {
      "human": 0.98,
      "ai": 0.02
    }
  },
  "task": "classify"
}
```

### Task Descriptions

*   **`generate`**: Generates music from scratch or from a seed (if `midi_input` provided).
*   **`continue`**: Continues a given MIDI sequence (`midi_input`).
*   **`classify`**: Determines if a MIDI sequence (`midi_input`) is human-composed or AI-generated.
*   **`bridge`**: Generates a musical bridge connecting two sections. Currently treats `midi_input` (or `midi_a`) as the context to continue from.
*   **`loops`**: Generates multi-instrumental loops.

---

## 💻 DeepSeek Coder API

**Port:** `2001`
**Endpoint:** `POST http://localhost:2001/predict` (Non-streaming)
**Stream Endpoint:** `POST http://localhost:2001/stream` (Streaming)
**Content-Type:** `application/json`

The DeepSeek API provides access to `DeepSeek-Coder-V2-Lite-Instruct` for code generation and general text tasks.

### Request Parameters

| Parameter | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `messages` | `array` | `[]` | List of message objects `{"role": "user", "content": "..."}` for chat-style interaction. Preferred over `prompt`. |
| `prompt` | `string` | `null` | Raw text prompt. Used if `messages` is empty. |
| `max_tokens` | `int` | `512` | Maximum number of new tokens to generate. |
| `temperature` | `float` | `0.7` | Controls randomness. |
| `top_p` | `float` | `0.95` | Nucleus sampling parameter. |
| `stream` | `boolean` | `false` | Whether to stream the response (use `/stream` endpoint for actual streaming). |

### Response Format

**Non-streaming (`/predict`):**

```json
{
  "text": "def hello_world():\n    print(\"Hello\")",
  "tokens": 12
}
```

**Streaming (`/stream`):**

Returns a stream of JSON objects (Server-Sent Events style or raw JSON stream depending on client/server negotiation, but LitServe usually yields JSON chunks).

```json
{"text": "def", "done": false}
{"text": " hello", "done": false}
...
{"done": true, "full_text": "...", "total_tokens": ...}
```

## Integration Notes

*   **Base64 Encoding**: Ensure all MIDI data is correctly Base64 encoded before sending.
*   **Batching**: The Orpheus server supports batching (up to 2 requests). Send a list of request objects to `/predict` for batch processing.
*   **Concurrency**: DeepSeek server runs in `spawn` mode and handles requests sequentially per worker, though LitServe can manage queues.
