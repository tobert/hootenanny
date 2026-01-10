# Pre-Rendered Timeline Architecture

## Problem Statement

RT audio callbacks cannot:
- Make syscalls (file I/O, network)
- Allocate memory
- Block on locks

But timeline playback needs:
- Loading audio from CAS (syscalls)
- Decoding WAV/FLAC (CPU-intensive, may allocate)
- Dynamic region activation

**Solution:** Pre-render timeline segments ahead of playback position, hand off to RT via lock-free structures.

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         CONTROL PLANE                                    │
│                                                                          │
│   Transport        Regions           TempoMap                           │
│   (position,      (timeline         (beat↔sample                        │
│    playing)        content)          conversion)                         │
│       │               │                  │                               │
│       └───────────────┼──────────────────┘                               │
│                       ▼                                                  │
│              ┌─────────────────┐                                         │
│              │ LOOKAHEAD       │  ← Async thread, syscalls OK            │
│              │ SCHEDULER       │                                         │
│              └────────┬────────┘                                         │
│                       │                                                  │
└───────────────────────┼──────────────────────────────────────────────────┘
                        │
          ┌─────────────▼─────────────┐
          │      BUFFER POOL          │  ← Pre-allocated at startup
          │                           │
          │  ┌─────┐ ┌─────┐ ┌─────┐  │     N buffers × M samples each
          │  │ buf │ │ buf │ │ buf │  │     e.g., 32 buffers × 4096 samples
          │  │  0  │ │  1  │ │  2  │  │     = 2.7 seconds at 48kHz
          │  └─────┘ └─────┘ └─────┘  │
          │                           │
          │  free_list: AtomicQueue   │  ← Lock-free free list
          │  ready_segments: AtomicQueue │ ← Lock-free ready queue
          │                           │
          └─────────────┬─────────────┘
                        │ lock-free read
                        ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                         RT CALLBACK                                      │
│                                                                          │
│   1. Read transport position (atomic)                                    │
│   2. Find segments covering [position, position + buffer_size]          │
│   3. Mix pre-rendered audio into output buffer                          │
│   4. Return consumed segments to free list                              │
│                                                                          │
│   Zero allocations. Zero syscalls. Zero blocking.                        │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## Data Structures

### Buffer Pool

```rust
/// Pre-allocated audio buffer pool
/// All allocations happen at startup
pub struct BufferPool {
    /// Fixed-size audio buffers, pre-allocated
    /// Each buffer holds `samples_per_buffer` interleaved stereo samples
    buffers: Vec<AlignedBuffer>,

    /// Lock-free queue of available buffer indices
    free_list: ArrayQueue<usize>,

    /// Configuration
    samples_per_buffer: usize,
    channels: usize,
}

/// Cache-line aligned buffer to prevent false sharing
#[repr(align(64))]
pub struct AlignedBuffer {
    samples: Box<[f32]>,
}

impl BufferPool {
    /// Create pool with N buffers of M samples each
    /// All memory allocated here, once, at startup
    pub fn new(num_buffers: usize, samples_per_buffer: usize, channels: usize) -> Self {
        let mut buffers = Vec::with_capacity(num_buffers);
        let free_list = ArrayQueue::new(num_buffers);

        for i in 0..num_buffers {
            buffers.push(AlignedBuffer {
                samples: vec![0.0; samples_per_buffer * channels].into_boxed_slice(),
            });
            free_list.push(i).unwrap();
        }

        Self { buffers, free_list, samples_per_buffer, channels }
    }

    /// Allocate a buffer (non-RT safe - may fail if pool exhausted)
    pub fn alloc(&self) -> Option<usize> {
        self.free_list.pop()
    }

    /// Return buffer to pool (RT-safe)
    pub fn free(&self, idx: usize) {
        let _ = self.free_list.push(idx);
    }

    /// Get buffer by index (RT-safe, just pointer arithmetic)
    pub fn get(&self, idx: usize) -> &[f32] {
        &self.buffers[idx].samples
    }

    /// Get mutable buffer by index (non-RT, for rendering)
    pub fn get_mut(&mut self, idx: usize) -> &mut [f32] {
        &mut self.buffers[idx].samples
    }
}
```

### Rendered Segment

```rust
/// A pre-rendered segment of timeline audio
/// Handed from lookahead thread to RT callback
#[derive(Debug, Clone, Copy)]
pub struct RenderedSegment {
    /// Absolute sample position where this segment starts
    pub start_sample: u64,

    /// Number of samples in this segment
    pub length_samples: usize,

    /// Index into BufferPool
    pub buffer_idx: usize,

    /// Generation counter for cache invalidation on seek
    pub generation: u64,
}

/// Timeline cache - the bridge between lookahead and RT
pub struct TimelineCache {
    /// The buffer pool
    pool: BufferPool,

    /// Ready segments, ordered by start_sample
    /// RT callback reads from here
    ready: ArrayQueue<RenderedSegment>,

    /// Segments being consumed by RT, awaiting return to pool
    consuming: ArrayQueue<RenderedSegment>,

    /// Current generation (incremented on seek)
    generation: AtomicU64,

    /// Current playback position (written by RT, read by lookahead)
    playback_position: AtomicU64,

    /// Lookahead distance in samples
    lookahead_samples: u64,
}
```

---

## Lookahead Scheduler

The lookahead thread runs in async context, can do syscalls freely.

```rust
impl LookaheadScheduler {
    /// Main loop - runs in async context
    pub async fn run(
        &self,
        cache: Arc<TimelineCache>,
        regions: Arc<RwLock<Vec<Region>>>,
        resolver: Arc<dyn ContentResolver>,
        tempo_map: Arc<RwLock<TempoMap>>,
    ) {
        // Pre-loaded audio cache (content_hash -> DecodedAudio)
        // This is where syscalls happen
        let mut audio_cache: HashMap<String, Arc<DecodedAudio>> = HashMap::new();

        loop {
            let current_gen = cache.generation.load(Ordering::Acquire);
            let current_pos = cache.playback_position.load(Ordering::Acquire);
            let target_pos = current_pos + cache.lookahead_samples;

            // Find regions in the lookahead window
            let regions = regions.read().unwrap();
            let tempo = tempo_map.read().unwrap();

            for region in regions.iter() {
                // Convert region bounds to samples
                let region_start = tempo.beat_to_sample(region.position);
                let region_end = tempo.beat_to_sample(region.position + region.duration);

                // Skip if outside lookahead window
                if region_end < current_pos || region_start > target_pos {
                    continue;
                }

                // Get or load audio content (syscalls happen here)
                let audio = self.get_or_load_audio(
                    &mut audio_cache,
                    &region.content_hash,
                    &resolver,
                ).await?;

                // Render segments for this region
                self.render_region_segments(
                    &cache,
                    &region,
                    &audio,
                    current_pos,
                    target_pos,
                    current_gen,
                );
            }

            // Reclaim consumed buffers
            while let Some(segment) = cache.consuming.pop() {
                if segment.generation == current_gen {
                    cache.pool.free(segment.buffer_idx);
                }
            }

            // Sleep before next iteration
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    /// Load audio from CAS, cache result
    async fn get_or_load_audio(
        &self,
        cache: &mut HashMap<String, Arc<DecodedAudio>>,
        content_hash: &str,
        resolver: &Arc<dyn ContentResolver>,
    ) -> Result<Arc<DecodedAudio>> {
        if let Some(audio) = cache.get(content_hash) {
            return Ok(Arc::clone(audio));
        }

        // Syscalls happen here - load from CAS
        let data = resolver.resolve(content_hash)?;
        let decoded = decode_audio(&data)?;
        let arc = Arc::new(decoded);
        cache.insert(content_hash.to_string(), Arc::clone(&arc));
        Ok(arc)
    }

    /// Render audio into buffer pool segments
    fn render_region_segments(
        &self,
        cache: &TimelineCache,
        region: &Region,
        audio: &DecodedAudio,
        window_start: u64,
        window_end: u64,
        generation: u64,
    ) {
        let samples_per_segment = cache.pool.samples_per_buffer;

        // Calculate which segments we need to render
        let region_start = /* ... */;
        let region_end = /* ... */;

        let render_start = window_start.max(region_start);
        let render_end = window_end.min(region_end);

        // Render in segment-sized chunks
        let mut pos = render_start;
        while pos < render_end {
            // Check if segment already rendered
            if self.segment_exists(cache, pos, generation) {
                pos += samples_per_segment as u64;
                continue;
            }

            // Allocate buffer from pool
            let buf_idx = match cache.pool.alloc() {
                Some(idx) => idx,
                None => {
                    // Pool exhausted, wait for RT to free some
                    break;
                }
            };

            // Render audio into buffer
            let buf = cache.pool.get_mut(buf_idx);
            self.render_audio_slice(audio, region, pos, buf);

            // Push to ready queue
            let segment = RenderedSegment {
                start_sample: pos,
                length_samples: samples_per_segment,
                buffer_idx: buf_idx,
                generation,
            };

            if cache.ready.push(segment).is_err() {
                // Queue full, return buffer
                cache.pool.free(buf_idx);
                break;
            }

            pos += samples_per_segment as u64;
        }
    }
}
```

---

## RT Callback

The RT callback is dead simple - just pointer arithmetic and mixing.

```rust
impl TimelineCache {
    /// Called from RT callback
    /// MUST be lock-free, allocation-free, syscall-free
    #[inline]
    pub fn mix_into_output(
        &self,
        output: &mut [f32],
        position: u64,
        channels: usize,
    ) {
        let current_gen = self.generation.load(Ordering::Relaxed);
        let frames = output.len() / channels;

        // Update position for lookahead thread
        self.playback_position.store(position, Ordering::Relaxed);

        // Scan ready segments for ones that overlap our window
        // Note: In production, use a more efficient structure than scanning
        let mut segments_to_consume = ArrayVec::<RenderedSegment, 8>::new();

        // Pop segments from ready queue
        while let Some(segment) = self.ready.pop() {
            // Skip stale segments (from before a seek)
            if segment.generation != current_gen {
                self.pool.free(segment.buffer_idx);
                continue;
            }

            let seg_end = segment.start_sample + segment.length_samples as u64;

            // Skip segments entirely before our window
            if seg_end <= position {
                self.pool.free(segment.buffer_idx);
                continue;
            }

            // Skip segments entirely after our window
            if segment.start_sample >= position + frames as u64 {
                // Put it back - we'll need it later
                // (In practice, use a sorted structure instead)
                let _ = self.ready.push(segment);
                break;
            }

            // This segment overlaps our window - mix it
            let buf = self.pool.get(segment.buffer_idx);

            // Calculate overlap
            let mix_start = segment.start_sample.max(position);
            let mix_end = seg_end.min(position + frames as u64);

            let src_offset = (mix_start - segment.start_sample) as usize * channels;
            let dst_offset = (mix_start - position) as usize * channels;
            let mix_samples = (mix_end - mix_start) as usize * channels;

            // Mix (just pointer arithmetic and adds)
            for i in 0..mix_samples {
                output[dst_offset + i] += buf[src_offset + i];
            }

            // Mark for consumption
            segments_to_consume.push(segment);
        }

        // Return consumed segments
        for segment in segments_to_consume {
            let _ = self.consuming.push(segment);
        }
    }
}
```

---

## Seek Handling

When transport seeks, we need to invalidate the cache.

```rust
impl TimelineCache {
    /// Called when transport seeks to new position
    /// NOT RT-safe - called from control plane
    pub fn seek(&self, new_position: u64) {
        // Increment generation to invalidate all cached segments
        self.generation.fetch_add(1, Ordering::Release);

        // Update position
        self.playback_position.store(new_position, Ordering::Release);

        // Drain ready queue, return buffers to pool
        while let Some(segment) = self.ready.pop() {
            self.pool.free(segment.buffer_idx);
        }

        // Drain consuming queue
        while let Some(segment) = self.consuming.pop() {
            self.pool.free(segment.buffer_idx);
        }
    }
}
```

---

## Configuration

```rust
pub struct PreRenderConfig {
    /// Number of buffers in pool
    /// More buffers = more lookahead, more memory
    pub num_buffers: usize,  // Default: 32

    /// Samples per buffer
    /// Larger = fewer segments, but coarser granularity
    pub samples_per_buffer: usize,  // Default: 4096 (85ms at 48kHz)

    /// Lookahead distance in seconds
    pub lookahead_seconds: f32,  // Default: 2.0

    /// How often lookahead thread runs
    pub lookahead_interval_ms: u64,  // Default: 10
}

impl Default for PreRenderConfig {
    fn default() -> Self {
        Self {
            num_buffers: 32,
            samples_per_buffer: 4096,
            lookahead_seconds: 2.0,
            lookahead_interval_ms: 10,
        }
    }
}

// Memory usage: 32 buffers × 4096 samples × 2 channels × 4 bytes = 1 MB
// Lookahead: 32 × 4096 / 48000 = 2.7 seconds
```

---

## Integration with Current Code

### Phase 1: Add TimelineCache alongside current ring buffer
- Create `TimelineCache` in daemon
- Lookahead thread fills cache
- RT callback reads from cache instead of ring

### Phase 2: Remove ring buffer for timeline
- Timeline audio comes directly from cache
- Ring buffer only for monitor input mixing

### Phase 3: Optimize segment lookup
- Replace linear scan with sorted structure
- Consider lock-free skip list or similar

---

## Dependencies

```toml
[dependencies]
crossbeam-queue = "0.3"  # ArrayQueue for lock-free queues
```

---

## Benefits

1. **RT-safe**: Zero allocations, zero syscalls in RT callback
2. **Predictable**: Fixed memory usage, no runtime allocation
3. **Efficient**: Direct buffer access, minimal copying
4. **Robust**: Generation counter handles seek gracefully
5. **Debuggable**: Clear separation of concerns

## Tradeoffs

1. **Memory overhead**: Pre-allocated buffers (~1MB default)
2. **Latency**: Lookahead adds small startup delay
3. **Complexity**: More moving parts than simple ring buffer
4. **Seek latency**: Need to re-render after seek
