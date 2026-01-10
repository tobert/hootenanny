# Canvas Design Rationale

**Purpose:** Deep context for revision sessions. Read when you need to understand *why*.

---

## Core Architecture

### Why mmap?

mmap gives us:
1. **Shared access** - Multiple processes can read/write the same canvas
2. **Lazy allocation** - Sparse files don't consume disk until written
3. **OS paging** - Kernel handles memory pressure, we don't manage it
4. **Zero-copy reads** - Data goes straight from page cache to user space

The tradeoff is we need fixed-size structures for efficient indexing.

### Why fixed page size?

Variable-size pages would require an indirection table, complicating the mmap layout and adding pointer chasing. Fixed 30-second pages:
- Simple math: `page_index = floor(time_seconds / 30)`
- Predictable memory layout
- Easy to reason about boundaries

We can tune the 30s later, but the fixed-size principle should stay.

### Why Cap'n Proto for metadata?

Cap'n Proto provides:
- Zero-copy reads (just cast bytes to struct)
- Forward/backward compatibility for versioning
- Compact binary format
- Schema-driven, less prone to parsing bugs

The canvas header and per-page metadata use Cap'n Proto. Audio data is raw f32.

### Why f32 PCM?

- No decode overhead in RT path
- IEEE 754 standard, portable
- Sufficient precision (24-bit equivalent dynamic range)
- Simple mixing (just addition and multiplication)

Compressed formats save space but add latency and CPU. We have RAM.

---

## Reference System

### The Problem

Audio processing needs relationships:
- **Sidechain compression** - Lane A's amplitude follows Lane B
- **Delays/reverbs** - Lane references itself N samples ago
- **Sampling** - Lane reuses audio from elsewhere with pitch shift
- **Submixes** - Lane is computed from other lanes
- **Cross-page** - Section B references audio from Section A

Copying audio everywhere is wasteful. We need references.

### Design: CanvasRef

A `CanvasRef` points to content elsewhere and describes how to use it:

```rust
/// A reference to content elsewhere in the canvas
#[derive(Debug, Clone)]
pub struct CanvasRef {
    /// Source location
    pub source: RefSource,

    /// How to use the referenced content
    pub transform: RefTransform,

    /// Time offset from reference point
    pub offset_samples: i64,  // Negative = backref

    /// Length of reference (0 = match source)
    pub length_samples: u64,
}

/// Where the reference points to
#[derive(Debug, Clone)]
pub enum RefSource {
    /// Another lane in the same page
    SamePage { lane: u16 },

    /// A lane in a different page
    OtherPage { page: u32, lane: u16 },

    /// The same lane, different time (for delays)
    Self_ { offset_samples: i64 },

    /// External CAS content (not yet in canvas)
    Cas { hash: [u8; 32] },

    /// A named output bus
    Bus { name: String },
}

/// How to transform the referenced content
#[derive(Debug, Clone)]
pub enum RefTransform {
    /// Direct pass-through
    Direct,

    /// Gain adjustment
    Gain { linear: f32 },

    /// Mix with existing lane content
    Mix { gain: f32 },

    /// Use as sidechain input
    Sidechain {
        ratio: f32,
        threshold_db: f32,
        attack_ms: f32,
        release_ms: f32,
    },

    /// Pitch shift
    PitchShift { ratio: f32 },

    /// Time stretch (without pitch change)
    TimeStretch { ratio: f32 },

    /// Delay line
    Delay {
        feedback: f32,
        wet_dry: f32,
    },
}
```

### Storage in Canvas

References live in the page metadata (Cap'n Proto), not in lane data:

```
Page [1MB]
├── Header [4KB]
│   ├── time_range
│   ├── lane_count
│   └── refs: Vec<CanvasRef>  ← references stored here
├── Lane 0 [64KB] - raw audio (written)
├── Lane 1 [64KB] - raw audio (written)
├── Lane 2 [64KB] - EMPTY (will be filled by resolving refs)
├── Lane 3 [64KB] - EMPTY (sidechain result)
└── ...
```

When a playhead reads Lane 2:
1. Check if lane has data → no
2. Check if lane has refs → yes, resolve them
3. Follow refs, apply transforms, write result to lane
4. Return audio

### Resolution Order

References can chain. Resolution follows topological order:
1. Build dependency graph from refs
2. Sort topologically
3. Resolve in order (sources before dependents)
4. Detect cycles (error)

### Caching

Once a ref is resolved for a playhead, the result is cached in the lane. Future reads return the cached data. Cache invalidation:
- Source data changes → invalidate dependent lanes
- Playhead context changes (for latent refs)
- Explicit invalidation

### Cross-Page References

A page can reference another page. This creates page dependencies:
- Page 3 refs Page 1 → Page 1 must be resolved first
- Circular page refs are an error

For streaming playback, the scheduler ensures pages are resolved ahead of playheads.

---

## Separation of Concerns

### Canvas is Storage, Not Playback

Canvas is intentionally "dumb" about playback:
- It doesn't know about playheads
- It doesn't track "current position"
- It doesn't do real-time anything
- It's async by design - for past and future, not "now"

### What Uses Canvas

**Attention/Playhead module** (separate, likely in hootenanny):
- Tracks playhead positions
- Triggers latent realization
- Reads from canvas, produces audio buffers
- Feeds chaosgarden via ring buffers

**Chaosgarden** (daemon):
- Does NOT access canvas directly
- Receives audio via ring buffers
- Handles RT output, buses, mixing

This separation means:
- Canvas can be tested in isolation
- No RT concerns leak into canvas code
- Playhead logic can evolve independently
- Multiple consumers can use canvas differently

---

## Multi-Writer Coordination

### The Challenge

Multiple processes may write to the canvas:
- Orpheus daemon writing MIDI renders
- Live input daemon writing captured audio
- Hootenanny writing orchestration results

### Approach: Lane Ownership

Each lane has an owner (process ID or logical name):
- Only owner can write to lane
- Others can read
- Ownership transfers explicitly

This avoids fine-grained locking. Writers don't contend.

### Page-Level Locking (Optional)

For metadata updates (refs, latent params), use page-level advisory locks:
```rust
fn update_page_metadata(page: &mut Page) {
    let _lock = page.advisory_lock()?;
    // Modify metadata
}
```

Readers don't need locks (atomic visibility through mmap).

---

## RT Path Guarantees

### The Sacred Code

The RT read path must be:
1. **Lock-free** - No mutexes, only atomics
2. **Allocation-free** - No heap operations
3. **Syscall-free** - After initial mmap, no kernel calls
4. **Bounded time** - Worst case is predictable

### What Can Violate This?

- Page faults (first access to cold page) → mitigated by prefetch
- Ref resolution (may require computation) → done ahead by scheduler
- Cache misses → mitigated by sequential access patterns

### The Feeder Pattern

A non-RT thread "feeds" the RT ring buffer:
1. Monitors playhead position
2. Prefetches canvas pages (`madvise(MADV_WILLNEED)`)
3. Resolves refs ahead of time
4. Copies to RT ring buffer

RT callback only reads from ring buffer - guaranteed hot.

---

## Rejected Alternatives

| Alternative | Why Rejected |
|-------------|--------------|
| SQLite for metadata | Too heavy, not mmap-friendly |
| Compression in canvas | Decode latency in RT path |
| Single giant buffer | Can't sparse-allocate, memory waste |
| Shared mutex for lanes | Lock contention, priority inversion |
| Copy-on-write refs | Too complex for first version |
| Variable page sizes | Indirection table complexity |

---

## Open Questions

| Question | Context | Status |
|----------|---------|--------|
| Page size tuning | 30s is arbitrary starting point | Test and tune |
| Max lanes per page | Memory layout constraint | Need benchmarks |
| Ref resolution parallelism | Multiple refs could resolve in parallel | Future optimization |
| Cross-process ref resolution | Who resolves refs when multiple writers? | Need protocol |
| Garbage collection | How to reclaim unused pages? | Future feature |
