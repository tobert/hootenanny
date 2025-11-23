# System Monitor MCP Tool Design

## Overview
An MCP tool for monitoring GPU and CPU activity during ML model inference, useful for understanding resource usage during long-running jobs like music generation.

## Use Cases
- Monitor GPU utilization during Orpheus generation (16K tokens = several minutes)
- Track CPU usage during MIDI rendering with RustySynth
- Detect if model is actually running or stuck
- Observe memory usage trends during batch operations

## Tool Interface

### `get_system_stats`
Returns current snapshot of system resources.

**Returns:**
```json
{
  "timestamp": "2025-11-25T22:45:00Z",
  "cpu": {
    "usage_percent": 45.2,
    "cores": 32,
    "load_avg": [2.5, 2.8, 3.1]
  },
  "memory": {
    "total_gb": 128,
    "used_gb": 48.5,
    "available_gb": 79.5,
    "percent": 37.9
  },
  "gpu": [
    {
      "id": 0,
      "name": "AMD Radeon RX 7900 XTX",
      "utilization_percent": 98,
      "memory_used_mb": 18432,
      "memory_total_mb": 24576,
      "temperature_c": 72,
      "power_draw_w": 285
    }
  ]
}
```

### `watch_system_stats`
Stream updates at specified interval (background job pattern).

**Parameters:**
- `interval_ms` (default: 1000) - Update frequency
- `duration_ms` (optional) - Auto-stop after duration

**Returns:** Job ID for polling

**Result when complete:**
```json
{
  "samples": [
    {"timestamp": "...", "cpu": {...}, "gpu": [...]},
    ...
  ],
  "summary": {
    "avg_gpu_utilization": 95.2,
    "max_gpu_memory_mb": 19200,
    "avg_cpu_percent": 42.1
  }
}
```

## Implementation Notes

### Linux (ROCm/CUDA)
- CPU: Read `/proc/stat` and `/proc/loadavg`
- Memory: Parse `/proc/meminfo`
- GPU (AMD): `rocm-smi` or direct `/sys/class/drm/card*/device/` reads
- GPU (NVIDIA): `nvidia-smi` XML output

### Cross-platform
- Use `sysinfo` crate for CPU/memory (portable)
- GPU detection: Try ROCm first, then CUDA, gracefully degrade if none

### OTLP Integration
- Emit metrics to OTLP endpoint
- Metric names: `system.cpu.utilization`, `system.gpu.utilization`, etc.
- Allows correlation with job traces in otlp-mcp

## Example Usage

```rust
// Start monitoring before long job
let monitor_job = mcp.watch_system_stats(WatchRequest {
    interval_ms: 500,
    duration_ms: Some(300000) // 5 minutes max
})?;

// Start ML job
let gen_job = mcp.orpheus_generate(...)?;

// Poll both
mcp.poll(&[gen_job.id, monitor_job.id], mode: "any")?;

// When gen completes, monitor data shows resource usage over time
```

## Future Extensions
- Disk I/O stats (useful for CAS operations)
- Network stats (useful for OTLP export monitoring)
- Process-specific stats (track specific PID like orpheus-base)
- Alerts/thresholds (notify if GPU thermal throttling)
