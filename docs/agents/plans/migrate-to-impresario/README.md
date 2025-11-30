# 🎭 Migrate Hootenanny to Impresario

Route all GPU work through impresario. Aggressive migration - no backward compatibility.

## Architecture

```
Hootenanny (MCP) → Impresario (1337) → Model Services (2000+)
```

## Tasks

| Task | Status | Description |
|:-----|:------:|:------------|
| [01-impresario-client](./01-impresario-client.md) | ⏳ | Rust HTTP client |
| [02-gpu-tools](./02-gpu-tools.md) | ⏳ | All GPU tools via impresario |
| [03-job-unification](./03-job-unification.md) | ⏳ | Single job tracking system |

## Session Log

| Date | Agent | Summary |
|:-----|:------|:--------|
| 2024-11-30 | Claude | Created migration plan |

## Handoffs

### Current State
- Plan created, not started

### Next Steps
1. Build impresario client
2. Gut LocalModels, wire up impresario
3. Update job polling to use impresario status
