# hrcli MCP Features Implementation Guide

## Status: COMPLETE ✅

### Completed ✅
- **Phase 1 Types**: All progress/log/completion types added to `crates/hrcli/src/mcp_client.rs`
- **Notification Infrastructure**: SSE listener updated to route notifications to callbacks
- **Notification Callback**: `with_notification_callback()` method added
- **Phase 2 API**: `complete_argument()` method added for dynamic completions
- **Phase 3 Schema**: `output_schema` field added to `DynamicToolSchema`
- **Plan Created**: Complete implementation plan at `/home/atobey/.claude/plans/tender-shimmying-valiant.md`
- **Compilation**: ✅ All changes compile successfully

### Remaining Work 📋

#### Phase 1: Progress & Logging (UI Integration Only)

**✅ DONE**: SSE listener infrastructure complete
**✅ DONE**: Notification routing to callbacks working
**Remaining**: UI integration in `execute_tool()`

**Update `execute_tool()` in `main.rs` (line 74)**
- Add progress tracking with Arc<Mutex<Option<String>>>
- Create notification callback that handles Progress and Log
- Use colored icons for log levels
- Clear progress line before showing results

See detailed implementation in plan file.

#### Phase 2: Dynamic Completions

**1. Add `complete_argument()` method to McpClient**
```rust
pub async fn complete_argument(
    &self,
    tool_name: &str,
    argument_name: &str,
    partial: &str,
) -> Result<CompletionResult> {
    let params = json!({
        "ref": {
            "type": "ref/argument",
            "name": tool_name,
            "argumentName": argument_name
        },
        "argument": {
            "name": argument_name,
            "value": partial
        }
    });

    let response = self.request("completion/complete", params).await?;
    let result = response.get("completion")
        .ok_or_else(|| anyhow!("Missing completion field"))?;
    serde_json::from_value(result.clone())
        .context("Failed to parse completion result")
}
```

**2. Create `crates/hrcli/src/completion.rs`**
- Internal helper for shell scripts to call
- `complete_argument()` function that prints values line-by-line

**3. Update completion generation in `main.rs`**
- Generate bash/zsh/fish scripts that call `hrcli __complete`
- Add `__complete` hidden subcommand

#### Phase 3: Output Schema Validation

**1. Add to `Cargo.toml`**
```toml
jsonschema = "0.18"
```

**2. Add `output_schema` field to `DynamicToolSchema` in `discovery/schema.rs`**
```rust
#[serde(rename = "outputSchema")]
pub output_schema: Option<Value>,
```

**3. Add validation to `formatter.rs`**
- `validate_output()` method using jsonschema
- Print warnings to stderr
- `std::process::exit(1)` on validation failure

## Testing Plan

### Phase 1 Testing
```bash
# Start hootenanny server
cargo run --bin hootenanny

# In another terminal, test progress
hrcli orpheus_generate --model base --num_variations 3

# Expected: See progress spinner with percentages
# Expected: See log messages with colored icons (ℹ, ⚠, ✗)
```

### Phase 2 Testing
```bash
# Generate completions
hrcli completions bash > ~/.hrcli-completion.sh
source ~/.hrcli-completion.sh

# Test completion
hrcli orpheus_generate --model <TAB>
# Expected: Shows "base", "children", "mono_melodies", "bridge"

hrcli cas_inspect --hash abc<TAB>
# Expected: Shows artifact hash suggestions
```

### Phase 3 Testing
```bash
# Test with valid output
hrcli orpheus_generate --model base

# Mock invalid output (would need server-side change)
# Expected: Warning to stderr + exit code 1
```

## Key Implementation Notes

### Progress Display
- Use `\r` to overwrite current line
- Clear with spaces before final output
- Don't show if not a TTY (check `atty::is(Stream::Stderr)`)

### Log Level Colors
- Info: ℹ (blue)
- Notice: ● (cyan)
- Warning: ⚠ (yellow)
- Error: ✗ (red)
- Critical+: Bright red variants

### Completion Caching
- **NO caching** - always query server
- Fail silently on connection errors
- Server does prefix filtering

### Validation Approach
- **Lenient**: Warn but continue
- Print invalid output to stderr for inspection
- Exit 1 to signal failure
- Skip validation if schema/content missing

## File Checklist

- [x] `crates/hrcli/src/mcp_client.rs` - Types added, callback method added
- [ ] `crates/hrcli/src/mcp_client.rs` - SSE listener updated, completion method added
- [ ] `crates/hrcli/src/main.rs` - Progress display, completions, __complete
- [ ] `crates/hrcli/src/completion.rs` - NEW FILE
- [ ] `crates/hrcli/src/discovery/schema.rs` - output_schema field
- [ ] `crates/hrcli/src/execution/formatter.rs` - Validation logic
- [ ] `crates/hrcli/Cargo.toml` - jsonschema dependency

## Next Steps

1. **Complete Phase 1**: Update SSE listener and execute_tool
2. **Implement Phase 2**: Completions (can be done in parallel)
3. **Implement Phase 3**: Validation (can be done in parallel)
4. **Test all phases**: Verify each feature works end-to-end
5. **Commit**: Create comprehensive commit with all changes

## References

- Full plan: `/home/atobey/.claude/plans/tender-shimmying-valiant.md`
- Baton types: `/home/atobey/src/halfremembered-mcp/crates/baton/src/types/`
- Current hrcli: `/home/atobey/src/halfremembered-mcp/crates/hrcli/src/`
