# HTTP 429 Retry Logic Implementation

## Context

The Orpheus API services (port 2000-2005) now support:
1. **HTTP 429 Too Many Requests** - Returned when GPU is busy
2. **`client_job_id` parameter** - Pass MCP job ID through to API for tracking

Our MCP server needs to be updated to leverage these features.

## Current State

### What Works ✅
- Async job system with JobStore in `crates/hootenanny/src/job_system.rs`
- All Orpheus tools return job IDs immediately
- Background tasks spawn and track results
- Tools: `orpheus_generate`, `orpheus_continue`, `orpheus_bridge`, `orpheus_loops`, `orpheus_generate_seeded`
- Job management: `get_job_status`, `wait_for_job`, `list_jobs`, `poll`, `sleep`

### What Needs Implementation 🚧

**File:** `crates/hootenanny/src/mcp_tools/local_models.rs`

**Function:** `run_orpheus_generate()` (line 172-220)

Currently does NOT:
1. Pass `client_job_id` to API
2. Handle HTTP 429 responses
3. Retry with backoff when GPU busy

## API Details

### Request Format (from `/tank/ml/music-models/services/orpheus-base/api.py`)

```python
# Line 89: API extracts client_job_id
"client_job_id": self.extract_client_job_id(request)

# Line 98: Raises BusyError for HTTP 429
with self.acquire_or_busy():
    # ... model inference
```

**Request should include:**
```json
{
  "task": "generate",
  "max_tokens": 128,
  "temperature": 1.0,
  "client_job_id": "7582596e-3cdf-4c8d-a2e7-65279f491c57"
}
```

**HTTP 429 Response:**
```
HTTP/1.1 429 Too Many Requests
Retry-After: 30

{
  "error": "GPU busy",
  "message": "Another request is being processed"
}
```

## Implementation Plan

### 1. Update Function Signature

**Current:**
```rust
pub async fn run_orpheus_generate(
    &self,
    model: String,
    task: String,
    input_hash: Option<String>,
    params: OrpheusGenerateParams,
) -> Result<OrpheusGenerateResult>
```

**New:**
```rust
pub async fn run_orpheus_generate(
    &self,
    model: String,
    task: String,
    input_hash: Option<String>,
    params: OrpheusGenerateParams,
    client_job_id: Option<String>,  // NEW: Pass through from MCP job
) -> Result<OrpheusGenerateResult>
```

### 2. Add client_job_id to Request Body

**Location:** Line 179-202

```rust
// Add after line 194:
if let Some(job_id) = client_job_id {
    request_body.insert("client_job_id".to_string(), serde_json::json!(job_id));
}
```

### 3. Handle HTTP 429 with Retry Logic

**Location:** Line 211-216 (current error handling)

**Replace with:**
```rust
let status = resp.status();

// Handle HTTP 429 - GPU busy, retry
if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
    let retry_after = resp.headers()
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(5); // Default 5 seconds

    tracing::warn!(
        client_job_id = ?client_job_id,
        retry_after = retry_after,
        "GPU busy, retrying after {}s",
        retry_after
    );

    tokio::time::sleep(tokio::time::Duration::from_secs(retry_after)).await;

    // Retry the request (could add max retry logic here)
    // For now, just fail and let the job system handle it
    anyhow::bail!("GPU busy, retry after {}s", retry_after);
}

if !status.is_success() {
    let error_body = resp.text().await
        .unwrap_or_else(|_| "<failed to read error body>".to_string());
    anyhow::bail!("Orpheus API error {}: {}", status, error_body);
}
```

### 4. Update All Call Sites

**Files to update:**
- `crates/hootenanny/src/server.rs`

**Search for:** `local_models.run_orpheus_generate(`

**Update from:**
```rust
let result = self.local_models.run_orpheus_generate(
    model.clone(),
    "generate".to_string(),
    None,
    params
).await
```

**Update to:**
```rust
let result = self.local_models.run_orpheus_generate(
    model.clone(),
    "generate".to_string(),
    None,
    params,
    Some(job_id_clone.as_str().to_string())  // Pass job ID
).await
```

**Locations in server.rs:**
- Line ~1337: `orpheus_generate`
- Line ~1498: `orpheus_generate_seeded`
- Line ~1619: `orpheus_continue`
- Line ~1882: `orpheus_bridge`
- Line ~1978: `orpheus_loops` (note: uses `model.clone()` call format)

## Testing

### Manual Test
```rust
// Start server
cargo run -p hootenanny

// From MCP client, launch 2 jobs quickly:
orpheus_generate({temp: 1.0, max_tokens: 128})
orpheus_generate({temp: 1.0, max_tokens: 128})  // Should get 429

// Check job status
get_job_status(job_id)
// Should show "GPU busy, retry after 5s" or similar
```

### Verify Telemetry
```rust
// Check OTLP traces for warnings
mcp__otlp-mcp__query({log_severity: "WARN", has_attribute: "client_job_id"})
```

## Expected Behavior After Implementation

### Before (Current)
- Second concurrent request hangs/times out
- No visibility into GPU busy state
- Connection drops with unclear errors

### After (With Changes)
- Second request gets HTTP 429
- Job marked as failed with "GPU busy" message
- Clear logs showing retry behavior
- User can see job is waiting for GPU

## Advanced: Retry Loop (Optional)

For better UX, could add retry loop in `run_orpheus_generate`:

```rust
const MAX_RETRIES: u32 = 3;

for attempt in 0..MAX_RETRIES {
    let resp = builder.send().await?;

    if resp.status() == 429 {
        if attempt < MAX_RETRIES - 1 {
            let retry_after = extract_retry_after(&resp);
            tracing::warn!("GPU busy, retry {}/{}", attempt + 1, MAX_RETRIES);
            tokio::time::sleep(Duration::from_secs(retry_after)).await;
            continue;
        } else {
            anyhow::bail!("GPU busy after {} retries", MAX_RETRIES);
        }
    }

    // Process success/other errors
    return process_response(resp).await;
}
```

## Related Files

- **API Implementation:** `/tank/ml/music-models/services/orpheus-base/api.py`
- **Test Suite:** `/tank/ml/music-models/test_all_apis.py`
- **MCP Client:** `crates/hootenanny/src/mcp_tools/local_models.rs`
- **MCP Tools:** `crates/hootenanny/src/server.rs`

## Success Criteria

✅ `client_job_id` passed to API in all requests
✅ HTTP 429 responses logged with job ID
✅ Retry logic sleeps and tries again
✅ Failed jobs show clear "GPU busy" errors
✅ Telemetry shows GPU contention patterns

## Notes

- The API services were updated to support this on 2025-11-22
- Job system is async-by-design (returns job IDs immediately)
- GPU contention was discovered during victory fanfare composition
- Current workaround: Wait between parallel requests
- This enhancement makes contention transparent and automatic

---

**Created:** 2025-11-22
**Author:** Claude (claude@anthropic.com)
**Status:** Ready for implementation
**Estimated effort:** 2-3 hours
