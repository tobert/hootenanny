# Canvas Test Strategy

Comprehensive test plan for the canvas crate. Tests should exercise the public API vigorously before integration with any daemon.

---

## Test Organization

```
crates/canvas/
├── src/
│   └── lib.rs
└── tests/
    ├── creation.rs      # Canvas create/open lifecycle
    ├── pages.rs         # Page allocation and access
    ├── lanes.rs         # Lane read/write operations
    ├── refs.rs          # CanvasRef resolution
    ├── concurrent.rs    # Multi-reader/writer scenarios
    ├── sparse.rs        # Sparse file behavior
    ├── stress.rs        # Performance and sustained load
    └── scenarios.rs     # Full workflow simulations
```

Note: Playhead tests belong in the attention module, not canvas.

---

## Test Categories

### 1. Creation & Lifecycle (`creation.rs`)

**Basic operations:**
- `test_create_new_canvas` - Create canvas, verify header
- `test_open_existing_canvas` - Create, close, reopen, verify state
- `test_open_nonexistent_fails` - Graceful error for missing file
- `test_open_corrupt_header` - Handle corrupt Cap'n Proto header
- `test_open_wrong_magic` - Reject non-canvas files
- `test_open_incompatible_version` - Version mismatch handling

**Edge cases:**
- `test_create_in_readonly_dir` - Permission error handling
- `test_create_on_full_disk` - Disk full handling
- `test_concurrent_create_same_path` - Race condition handling
- `test_reopen_after_crash` - Simulated crash recovery

### 2. Page Management (`pages.rs`)

**Allocation:**
- `test_allocate_first_page` - Page 0 at time 0
- `test_allocate_sequential_pages` - Pages 0, 1, 2, ...
- `test_allocate_sparse_pages` - Pages 0, 5, 10 (gaps)
- `test_allocate_many_pages` - 1000+ pages for 8+ hour canvas
- `test_page_time_boundaries` - Verify time ranges are correct

**Access:**
- `test_access_existing_page` - Read allocated page
- `test_access_unallocated_page` - Returns None, doesn't allocate
- `test_page_iteration` - Iterate over all pages
- `test_page_by_time` - Find page containing timestamp

**Boundaries:**
- `test_page_exact_boundary` - Sample at exactly 30.000s
- `test_page_near_boundary` - Samples at 29.999s and 30.001s
- `test_cross_page_read` - Read spanning page boundary

### 3. Lane Operations (`lanes.rs`)

**Write operations:**
- `test_write_audio_samples` - Write f32 stereo data
- `test_write_at_offset` - Write to middle of lane
- `test_write_sequential_chunks` - Multiple writes, verify continuity
- `test_write_overwrite` - Overwrite existing data
- `test_write_beyond_capacity` - Partial write, return count

**Read operations:**
- `test_read_audio_samples` - Read back what was written
- `test_read_unwritten_region` - Returns zeros
- `test_read_partial` - Read subset of written data
- `test_read_at_offset` - Read from middle of lane
- `test_read_into_small_buffer` - Buffer smaller than available

**Lane types:**
- `test_lane_type_audio_stereo` - 2-channel audio
- `test_lane_type_audio_mono` - 1-channel audio
- `test_lane_type_audio_surround` - 6+ channel audio
- `test_lane_type_midi` - MIDI event lane
- `test_lane_type_metadata` - Generic metadata lane

**Lane allocation:**
- `test_allocate_first_lane` - Lane 0 in fresh page
- `test_allocate_multiple_lanes` - Several lanes per page
- `test_lane_limit_enforcement` - Max lanes per page
- `test_lane_persists_across_reopen` - Close, reopen, lanes exist

### 4. Reference Operations (`refs.rs`)

**Basic refs:**
- `test_ref_same_page_lane` - Reference another lane in same page
- `test_ref_other_page` - Reference lane in different page
- `test_ref_self_offset` - Reference same lane at different offset (delay)
- `test_ref_cas_content` - Reference CAS hash not yet in canvas

**Resolution:**
- `test_resolve_direct_ref` - Simple pass-through
- `test_resolve_gain_ref` - Apply gain transform
- `test_resolve_mix_ref` - Mix with existing content
- `test_resolve_chain` - Ref A → Ref B → source
- `test_resolve_cycle_detection` - Circular refs error gracefully

**Ref storage:**
- `test_refs_persist` - Close/reopen, refs still exist
- `test_refs_in_metadata` - Refs stored in Cap'n Proto, not lane data
- `test_ref_update` - Modify ref, resolution changes
- `test_ref_delete` - Remove ref, lane becomes empty

**Cross-page refs:**
- `test_cross_page_ref_resolution` - Page 5 refs Page 2
- `test_cross_page_dependency_order` - Resolve in correct order
- `test_cross_page_sparse` - Ref to unallocated page

### 5. Concurrent Access (`concurrent.rs`)

**Multi-reader:**
- `test_two_readers_same_page` - No interference
- `test_many_readers` - 16+ concurrent readers
- `test_readers_different_pages` - Spread across canvas

**Reader + Writer:**
- `test_read_while_writing_different_lane` - No interference
- `test_read_while_writing_same_lane` - Defined behavior (see what?)
- `test_read_catches_up_to_write` - Writer ahead, reader follows
- `test_write_overtakes_read` - Handle gracefully

**Multi-writer:**
- `test_two_writers_different_lanes` - No interference
- `test_two_writers_different_pages` - No interference
- `test_writer_coordination` - Same lane, different offsets

**Stress:**
- `test_concurrent_chaos` - Random readers/writers, verify no corruption
- `test_concurrent_with_seeks` - Readers seeking while writers write

### 6. Sparse File Behavior (`sparse.rs`)

**Disk usage:**
- `test_empty_canvas_minimal_disk` - Just header, no page data
- `test_sparse_pages_minimal_disk` - Pages 0, 100, 200 don't fill gaps
- `test_disk_usage_proportional_to_content` - Measure actual disk usage
- `test_punch_hole_reclaims_space` - Delete page, disk shrinks

**mmap behavior:**
- `test_read_unallocated_returns_zeros` - No page fault panic
- `test_write_allocates_backing` - Writing creates physical pages
- `test_large_canvas_small_footprint` - 8-hour canvas, minimal RAM

### 7. Performance & Stress (`stress.rs`)

**Write throughput:**
- `test_sustained_write_single_lane` - 48kHz stereo for 10 minutes
- `test_sustained_write_16_lanes` - 16 lanes simultaneously
- `test_burst_write` - 1 second of audio in tight loop

**Read throughput:**
- `test_sustained_read_single_playhead` - Real-time playback rate
- `test_sustained_read_8_playheads` - 8 simultaneous reads
- `test_read_throughput_exceeds_realtime` - Must be >> 1x realtime

**Latency:**
- `test_read_latency_p99` - 99th percentile read latency
- `test_seek_latency` - Time to seek across canvas
- `test_page_fault_latency` - First access to cold page

**Memory:**
- `test_memory_usage_scales` - Verify no memory leaks
- `test_large_canvas_memory_stable` - 45-min canvas, stable footprint

### 8. Full Scenarios (`scenarios.rs`)

Note: These test canvas storage operations only. Playhead/attention logic is tested separately.

**Orpheus render workflow:**
```rust
#[test]
fn test_orpheus_render_workflow() {
    // 1. Create canvas
    // 2. Allocate pages for 2-minute piece
    // 3. Write latent params to metadata
    // 4. Later: write rendered audio to lane
    // 5. Verify audio readable at position
    // 6. Verify latent params still accessible
}
```

**Multi-writer coordination:**
```rust
#[test]
fn test_multi_writer_workflow() {
    // 1. Create canvas
    // 2. Process A writes to lane 0
    // 3. Process B writes to lane 1 (concurrent)
    // 4. Both processes read each other's lanes
    // 5. Verify no corruption
}
```

**Reference-based arrangement:**
```rust
#[test]
fn test_reference_arrangement() {
    // 1. Create canvas
    // 2. Write drum loop to page 0, lane 0
    // 3. Add ref in page 1 pointing to page 0 (repeat)
    // 4. Add ref in page 2 with gain transform
    // 5. Resolve all refs
    // 6. Verify audio in all pages
}
```

**Long-form canvas:**
```rust
#[test]
fn test_45_minute_canvas() {
    // 1. Create canvas (sparse)
    // 2. Allocate pages 0, 10, 20, 50, 80 (sparse)
    // 3. Write audio to each
    // 4. Verify disk usage proportional to content
    // 5. Read back all content
    // 6. Verify no corruption
}
```

**Recording archive:**
```rust
#[test]
fn test_recording_archive() {
    // 1. Create canvas
    // 2. Simulate live input: write small chunks sequentially
    // 3. Write 5 minutes of "live" audio
    // 4. Read back, verify continuity
    // 5. Add refs for "best moments"
}
```

---

## Test Infrastructure

### Fixtures

```rust
/// Create a temporary canvas for testing
fn temp_canvas() -> (TempDir, Canvas) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.canvas");
    let canvas = Canvas::create(&path, CanvasConfig::default()).unwrap();
    (dir, canvas)
}

/// Create canvas with pre-written audio
fn canvas_with_audio(duration_secs: u32, lane_count: usize) -> (TempDir, Canvas) {
    let (dir, mut canvas) = temp_canvas();
    // Allocate pages
    // Write sine waves to lanes
    (dir, canvas)
}

/// Generate test audio (sine wave)
fn sine_wave(frequency: f32, sample_rate: u32, duration_secs: f32) -> Vec<f32> {
    // ...
}
```

### Concurrency helpers

```rust
/// Run N threads doing F, verify no panics
fn concurrent_stress<F: Fn() + Send + Clone>(n: usize, f: F) {
    let handles: Vec<_> = (0..n)
        .map(|_| std::thread::spawn(f.clone()))
        .collect();
    for h in handles {
        h.join().unwrap();
    }
}
```

### Timing helpers

```rust
/// Measure operation latency distribution
fn measure_latency<F: FnMut()>(iterations: usize, mut f: F) -> LatencyStats {
    let mut times = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let start = Instant::now();
        f();
        times.push(start.elapsed());
    }
    LatencyStats::from(times)
}
```

---

## Definition of Done

All tests must pass:

```bash
cargo fmt --check
cargo clippy -- -D warnings
cargo test --release  # Some tests need release for timing
cargo test --release -- --ignored  # Long-running stress tests
```

### Coverage targets

- Line coverage: >80%
- Branch coverage: >70%
- All public API methods have at least one test
- All error paths have tests

---

## Notes

- **Stress tests are `#[ignore]`** - Run explicitly, not in CI by default
- **RT tests may need `CAP_SYS_NICE`** - For thread priority testing
- **Sparse tests need filesystem support** - Skip on filesystems without hole punching
- **Large canvas tests need disk space** - At least 10GB free recommended
