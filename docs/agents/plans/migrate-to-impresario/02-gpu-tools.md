# Task 02: GPU Tools via Impresario

All GPU tools submit to impresario. Delete LocalModels GPU code.

## Tools to Migrate

| Tool | Service |
|:-----|:--------|
| `orpheus_generate` | `orpheus-base` |
| `orpheus_continue` | `orpheus-base` |
| `orpheus_bridge` | `orpheus-bridge` |
| `anticipatory_generate` | `anticipatory` |
| `anticipatory_continue` | `anticipatory` |
| `anticipatory_embed` | `anticipatory` |

## Pattern

```rust
async fn orpheus_generate(&self, req: Request) -> Result<CallToolResult> {
    let job = self.impresario.submit("orpheus-base", json!({
        "model": req.model,
        "task": "generate",
        "temperature": req.temperature,
        // ...
    })).await?;

    Ok(json!({"job_id": job.id, "status": "queued"}))
}
```

## Delete

- `LocalModels::run_orpheus_generate()`
- `LocalModels::orpheus_url`
- All 429 retry logic

## Keep

- `LocalModels` for DeepSeek (CPU)
- CAS operations (local)
- rustysynth MIDI rendering (CPU)

## New: Anticipatory Tools

Add three new tools for the Anticipatory service (port 2011):
- `anticipatory_generate` - MIDI from scratch
- `anticipatory_continue` - extend existing MIDI
- `anticipatory_embed` - 768-dim embedding

## Done When

- [ ] All Orpheus tools use impresario
- [ ] Anticipatory tools added
- [ ] LocalModels GPU code deleted
- [ ] No more 429 handling
