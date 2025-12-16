# Capture Sessions - Design Rationale

**Purpose:** Deep context for revision sessions. Read when you need to understand *why*.

---

## Overview

Capture sessions enable recording audio and MIDI streams from hardware devices (eurorack, synths, controllers) into the hootenanny artifact system. The design prioritizes:

1. **Seamless slicing** - Same API for archived chunks and live buffer edge
2. **Multi-clock capture** - Record all timing sources, correlate later
3. **Artifact lineage** - Sessions produce linked artifacts with clear provenance
4. **RT safety** - Chaosgarden never blocks, hootenanny handles orchestration

---

## Session Modes

Capture sessions support multiple interaction patterns:

### Request/Response Mode

Agent generates content, sends to hardware, captures response:

```
1. Agent creates MIDI artifact (Orpheus, ABC, etc.)
2. Session sends MIDI to Niftycase → CV/Gate → eurorack
3. Session captures audio response
4. Creates linked artifacts: MIDI sent → audio captured
```

Use case: "Play this sequence, record what the eurorack does"

### Passive Capture Mode

Continuous recording with retrospective slicing:

```
1. Streams record continuously to chunked storage
2. User/agent patches eurorack, experiments
3. "That was good" → slice last N seconds
4. Slice becomes artifact with provenance
```

Use case: Jam sessions, experimentation, "grab that moment"

### Triggered Mode (Future)

Activity detection suggests capture moments:

```
1. Streams record continuously
2. System detects audio threshold / MIDI activity
3. Suggests "interesting moment at T" to user/agent
4. User confirms → slice created
```

Use case: Unattended capture, overnight generative patches

**Initial implementation: Request/Response and Passive only.** Triggered mode builds on top once slicing works.

---

## Why Logical Containers?

Sessions capture multiple streams (audio in, MIDI in, MIDI out). We considered:

| Approach | Description |
|----------|-------------|
| Physical container | Tar/zip in CAS containing all streams |
| Logical container | Session artifact with child relationships |

**Decision: Logical containers.**

Rationale:
- Each stream is individually addressable, taggable, annotatable
- Deduplication works (same audio clip in 10 sessions = 1 CAS entry)
- External tools (ffmpeg, sox) work directly on CAS files without extraction
- Session is the organizing unit without being a physical blob
- Matches existing artifact parent/child model

Physical containers would require:
- Fragment addressing (`sha256:abc#path/to/child.wav`)
- Extraction for every downstream operation
- Loss of per-child metadata flexibility
- Custom tooling for everything

Export/archive (tarball of session) can be a rendering operation on demand.

---

## Why Streams as First-Class Objects?

A stream is continuous, potentially infinite, and sliceable. CAS is content-addressed (immutable blobs). We needed to bridge this.

**Decision: Stream definition in CAS, stream state in chaosgarden.**

```
┌─────────────────────────────────────────────────────────┐
│  Stream Definition (in CAS)                             │
│  {                                                      │
│    "uri": "stream://eurorack-audio/main",               │
│    "device_identity": "niftycase-audio-out",            │
│    "format": { "sample_rate": 48000, "channels": 1 },   │
│    "chunk_size_samples": 131072                         │
│  }                                                      │
│  hash: blake3:def_abc123                                │
├─────────────────────────────────────────────────────────┤
│  Artifact (stream-definition)                           │
│  content_hash: blake3:def_abc123                        │
│  tags: [device:eurorack, type:audio, role:capture]      │
└─────────────────────────────────────────────────────────┘
          │
          │ references
          ▼
┌─────────────────────────────────────────────────────────┐
│  Live State (in chaosgarden, queryable via ZMQ)         │
│  status: Recording | Paused | Stopped                   │
│  buffer_head: sample 47_232_000                         │
│  archived_chunks: [chunk_0..chunk_N]                    │
│  staging_chunk: blake3:staging_xyz (incomplete)         │
└─────────────────────────────────────────────────────────┘
```

Benefits:
- Stream URI is stable identity
- Definition is immutable (CAS'd)
- Live state is queryable without polling files
- Same artifact model for streams and static content

---

## Why Chunked Storage with Staging?

Continuous streams don't have a hash until complete. We considered:

| Approach | Description |
|----------|-------------|
| Ephemeral buffer only | Ring buffer in memory, slice materializes to CAS |
| Chunked from start | Every N samples becomes a CAS blob |
| Hybrid | Ring buffer for hot, chunks for archive |

**Decision: Chunked from start, with staging for incomplete chunks.**

### Staging Directory

CAS gets a staging area with identical structure to content:

```
cas/
├── content/
│   └── blake3/
│       └── ab/cd/abcd1234...  # sealed, immutable
└── staging/
    └── blake3/
        └── ef/gh/efgh5678...  # writable, temporary
```

Key properties:
- **Same hash algorithm** (blake3) for both staging and content
- **Same sharding structure** (2-char/2-char prefix directories)
- **Staging ID = hash of random data** (UUID or entropy), so addresses look identical
- **Seal operation = rename() or copy** - O(1) on same filesystem, falls back to copy if cross-filesystem

### Crate Ownership

**CAS staging logic lives in `crates/cas`:**

- `create_staging()` - allocate staging file, return path
- `seal()` - compute hash, move/copy to content, return content address
- `staging/` and `content/` layout encapsulated in cas crate

**Hootenanny manages file lifecycle:**

- Creates staging files via CAS API
- Provides file paths to chaosgarden
- Triggers seal when chunk is complete
- Handles cross-filesystem copies transparently

**Chaosgarden only does I/O:**

- Receives file path from hootenanny
- Opens, mmaps, writes samples, closes
- Notifies hootenanny of write progress via ZMQ
- Does NOT manage files, directories, or sealing

This separation keeps chaosgarden simple and RT-focused. All filesystem orchestration happens in hootenanny where blocking is acceptable.

```rust
impl Cas {
    fn create_staging(&self) -> StagingChunk {
        let random_id = blake3::hash(&Uuid::new_v4().as_bytes());
        let path = self.staging_dir.join(shard_path(&random_id));
        StagingChunk { id: random_id, path, file: File::create(path) }
    }
}

impl StagingChunk {
    fn address(&self) -> CasAddress {
        CasAddress::Staging(self.id)  // usable immediately
    }

    fn seal(self) -> CasAddress {
        let content_hash = blake3::hash_file(&self.path);
        let final_path = content_path(&content_hash);
        fs::rename(self.path, final_path)?;  // atomic, O(1)
        CasAddress::Content(content_hash)
    }
}
```

### Why Not Just Ring Buffer?

A pure ring buffer approach would:
- Require copying on every slice (can't reference in-place)
- Lose history when buffer wraps
- Need special "stream" vs "blob" distinction throughout

Chunked approach gives us:
- References work for archived and live content
- Hour retention = keep manifest references, normal GC for old chunks
- Slicing can reference chunk ranges without copying

---

## Why mmap with ZMQ Notifications?

Chaosgarden (RT thread) writes chunks. Hootenanny (control plane) reads for slicing. We needed a coordination mechanism.

**Decision: mmap'd chunks aligned to SSD blocks, ZMQ notification of head position.**

Considered alternatives:
- File locking (blocks RT thread - unacceptable)
- Polling file size (works but latency, no push)
- Shared memory with atomics (complex, still need IPC for fanout)

The design:
1. Chaosgarden mmaps staging chunks with appropriate flags (non-blocking for RT)
2. Chunks sized to align with SSD blocks (likely 4KB aligned, 128K samples = ~512KB)
3. As head moves, chaosgarden sends ZMQ message with new position
4. Hootenanny maintains subscriber list, fans out notifications
5. Readers trust notified position as safe-to-read watermark

```
chaosgarden (RT)          hootenanny (control)         subscribers
      │                          │                          │
      │──── head @ sample N ────▶│                          │
      │                          │──── head @ sample N ────▶│
      │                          │                          │
      │──── chunk sealed ───────▶│                          │
      │      blake3:abc          │──── chunk sealed ───────▶│
      │                          │                          │
```

### mmap Considerations

- `MAP_SHARED` for visibility across processes
- `MAP_NORESERVE` to avoid blocking on memory pressure
- Preallocate chunk files to avoid allocation in RT path (future optimization)
- Consider `madvise(MADV_SEQUENTIAL)` for write pattern
- Align chunk boundaries to page size (4KB) for clean mmap regions

Start simple, measure, then optimize. The ZMQ notification path gives us flexibility.

---

## Why Session Timeline with Multi-Clock Capture?

Sessions coordinate multiple streams with different time bases:
- Audio: 48kHz sample clock
- MIDI: event-based, often with its own clock
- Wall clock: system time

We considered:
- Audio as master clock (tight sync but couples streams)
- Wall clock only (simple but drift-prone)
- Session timeline abstraction (clean but another layer)

**Decision: Session timeline as abstraction, capture all available clocks.**

The session maintains:
```rust
pub struct SessionTimeline {
    pub started_at: Instant,
    pub clock_snapshots: Vec<ClockSnapshot>,
}

pub struct ClockSnapshot {
    pub checkpoint: SessionCheckpoint,  // start, end, or named checkpoint
    pub wall_clock: SystemTime,
    pub audio_sample_position: Option<u64>,
    pub midi_clock_ticks: Option<u64>,
    // ... other clock sources
}
```

Rationale:
- We don't know what alignment we'll need until we try it
- Capturing all clocks preserves options
- Could use ML/latent techniques for alignment later
- Checkpoints at session start/end, possibly at interesting moments

Correlation happens at slice time, not capture time. The raw clock data is there for whatever approach works best.

---

## Type Definitions

Core types for the capture session system:

### Stream Types

```rust
/// Canonical identifier for a stream
pub struct StreamUri(String);  // e.g., "stream://eurorack-audio/main"

/// Static definition of a stream, stored in CAS
#[derive(Serialize, Deserialize)]
pub struct StreamDefinition {
    pub uri: StreamUri,
    pub device_identity: IdentityId,
    pub format: StreamFormat,
    pub chunk_size_bytes: u64,  // target chunk size for rotation
}

#[derive(Serialize, Deserialize)]
pub enum StreamFormat {
    Audio {
        sample_rate: u32,
        channels: u8,
        sample_format: SampleFormat,  // F32, I16, etc.
    },
    Midi {
        // MIDI chunks are sized by bytes, not time
    },
}

/// Reference to a chunk (sealed or staging)
#[derive(Serialize, Deserialize)]
pub enum ChunkRef {
    Sealed {
        hash: Blake3Hash,
        byte_count: u64,
        sample_count: Option<u64>,  // None for MIDI
    },
    Staging {
        id: Blake3Hash,  // hash of random data
        bytes_written: u64,
        samples_written: Option<u64>,
    },
}

/// Stream manifest - a staging artifact, updated atomically
#[derive(Serialize, Deserialize)]
pub struct StreamManifest {
    pub stream_uri: StreamUri,
    pub definition_hash: Blake3Hash,  // points to StreamDefinition in CAS
    pub chunks: Vec<ChunkRef>,
    pub total_bytes: u64,
    pub total_samples: Option<u64>,
    pub started_at: SystemTime,
    pub last_updated: SystemTime,
}
```

### Session Types

```rust
/// A capture session groups multiple streams with timing
#[derive(Serialize, Deserialize)]
pub struct CaptureSession {
    pub id: SessionId,
    pub mode: SessionMode,
    pub streams: Vec<StreamUri>,
    pub segments: Vec<SessionSegment>,
    pub timeline: SessionTimeline,
    pub status: SessionStatus,
}

#[derive(Serialize, Deserialize)]
pub enum SessionMode {
    Passive,          // continuous capture, slice on demand
    RequestResponse {
        midi_out: StreamUri,
        audio_in: StreamUri,
    },
}

/// A segment is a contiguous recording period
#[derive(Serialize, Deserialize)]
pub struct SessionSegment {
    pub id: SegmentId,
    pub started_at: ClockSnapshot,
    pub ended_at: Option<ClockSnapshot>,
    pub chunk_range: Range<usize>,  // indices into stream manifests
}

#[derive(Serialize, Deserialize)]
pub enum SessionStatus {
    Recording,  // actively capturing
    Stopped,    // finalized, no more segments
}
```

### Slicing Types

```rust
/// Request to slice a time range from a stream
pub struct SliceRequest {
    pub stream: StreamUri,
    pub from: TimeSpec,
    pub to: TimeSpec,
    pub output: SliceOutput,
}

pub enum TimeSpec {
    Absolute(SystemTime),
    Relative { seconds_ago: f64 },
    SamplePosition(u64),
    SessionStart { segment: Option<SegmentId> },
    SessionEnd { segment: Option<SegmentId> },
}

pub enum SliceOutput {
    Materialize,  // create new CAS blob (WAV/MIDI file)
    Virtual,      // create chunk-reference manifest
}

pub struct SliceResult {
    pub artifact_id: ArtifactId,
    pub content_hash: Blake3Hash,
    pub sample_range: Range<u64>,
    pub source_chunks: Vec<ChunkRef>,
}
```

---

## Session Lifecycle

Sessions have a simple state model:

```
                    ┌─────────────────────┐
                    │                     │
      play()        ▼        stop()       │
  ──────────▶  Recording  ───────────▶ Stopped
                    │                     │
                    │ play() again        │
                    │ (new segment)       │
                    └─────────────────────┘
```

**Key behaviors:**

- **play()** = start new segment, begin recording to chunks
- **stop()** = finalize current segment, session complete
- **pause()** = same as stop() (we don't have a paused state)
- **resume after stop** = new session (or call play() for new segment before stop)

### Segments

Sessions consist of one or more segments. Each segment is a contiguous recording period:

```
Session: session_abc
├── Segment 0: 10:00:00 - 10:05:23 (chunks 0-12)
├── Segment 1: 10:07:45 - 10:08:02 (chunks 13-14)
└── Segment 2: 10:10:00 - 10:45:30 (chunks 15-87)
```

Gaps between segments are fine. Downstream tools stitch segments seamlessly using clock data.

### Stream Independence

Streams record independently of device status:

- Device unplugged → stream records silence/empty buffer
- Device muted → stream records silence
- Device reconnected → stream continues (may need timestamp reconciliation)

This keeps the stream timeline clean. Device events can be recorded as metadata, not as stream interruptions.

---

## MIDI Chunking Strategy

MIDI streams are event-based, not sample-based. We chunk by size, not time:

**Decision: Rotate MIDI chunks on byte size alone.**

```
MIDI events arrive:
  [NoteOn, CC, NoteOff, Clock, NoteOn, ...]
       │
       ▼
Accumulate in staging chunk until size threshold (~512KB)
       │
       ▼
Seal chunk, start new staging chunk
```

Rationale:
- Simple implementation (no time-based logic)
- Chunk boundaries don't need to align with audio
- Correlation happens at slice time using clock data
- Variable event density doesn't cause tiny or huge chunks

MIDI chunks contain raw MIDI bytes with timestamps. Slicing extracts events by timestamp range, not byte range.

---

## Boundary Specification

Clear separation between chaosgarden (RT) and hootenanny (control plane):

### Chaosgarden Responsibilities

```
┌─────────────────────────────────────────────────────────────┐
│  CHAOSGARDEN (RT-safe, never blocks, no file management)    │
├─────────────────────────────────────────────────────────────┤
│  Does:                                                      │
│  - Opens/mmaps files provided by hootenanny                 │
│  - PipeWire/ALSA stream callbacks                           │
│  - Writes samples/events to mmap'd region                   │
│  - Notifies hootenanny when chunk is full                   │
│  - Closes file when told to switch                          │
│                                                             │
│  Does NOT:                                                  │
│  - Create files or directories                              │
│  - Seal/rename/move files                                   │
│  - Manage chunk rotation (hootenanny decides)               │
│                                                             │
│  Sends via ZMQ IOPub:                                       │
│  - HeadPosition { stream, sample_position, timestamp }      │
│  - ChunkFull { stream, path, bytes_written }                │
│  - StreamError { stream, error }                            │
│                                                             │
│  Receives via ZMQ Control:                                  │
│  - StartStream { uri, definition, chunk_path }              │
│  - SwitchChunk { uri, new_chunk_path }                      │
│  - StopStream { uri }                                       │
└─────────────────────────────────────────────────────────────┘
```

### Hootenanny Responsibilities

```
┌─────────────────────────────────────────────────────────────┐
│  HOOTENANNY (control plane, can block, manages files)       │
├─────────────────────────────────────────────────────────────┤
│  File Lifecycle (via CAS API):                              │
│  - Creates staging files for new chunks                     │
│  - Provides paths to chaosgarden                            │
│  - Seals chunks when notified full (rename or copy)         │
│  - Handles cross-filesystem transparently                   │
│                                                             │
│  Owns:                                                      │
│  - Stream manifests (staging artifacts, atomic updates)     │
│  - Session state (segments, timeline, status)               │
│  - Slice operations (read chunks, materialize)              │
│  - Artifact creation (slices, sessions)                     │
│  - Subscriber fanout (forward IOPub to clients)             │
│                                                             │
│  Exposes MCP tools:                                         │
│  - stream_create, stream_start, stream_stop                 │
│  - session_create, session_play, session_stop               │
│  - stream_slice, session_export                             │
│  - stream_status, session_status                            │
│                                                             │
│  Subscribes to chaosgarden IOPub:                           │
│  - On ChunkFull: seal chunk, create new, send SwitchChunk   │
│  - Tracks head position for slicing                         │
│  - Updates manifests                                        │
│  - Logs errors                                              │
└─────────────────────────────────────────────────────────────┘
```

### ZMQ Message Types

Messages use Cap'n Proto schemas in `crates/hooteproto/schemas/`.

**New file: `streams.capnp`**

```capnp
@0x...; # generate with `capnp id`

using Common = import "common.capnp";

# Stream definition stored in CAS
struct StreamDefinition {
  uri @0 :Text;
  deviceIdentity @1 :Text;
  format @2 :StreamFormat;
  chunkSizeBytes @3 :UInt64;
}

struct StreamFormat {
  union {
    audio @0 :AudioFormat;
    midi @1 :Void;
  }
}

struct AudioFormat {
  sampleRate @0 :UInt32;
  channels @1 :UInt8;
  sampleFormat @2 :SampleFormat;
}

enum SampleFormat {
  f32 @0;
  i16 @1;
  i24 @2;
}

# Stream commands (add to envelope.capnp Payload union)
struct StreamStart {
  uri @0 :Text;
  definition @1 :StreamDefinition;
  chunkPath @2 :Text;
}

struct StreamSwitchChunk {
  uri @0 :Text;
  newChunkPath @1 :Text;
}

struct StreamStop {
  uri @0 :Text;
}

# Stream events (add to broadcast.capnp Broadcast union)
struct StreamHeadPosition {
  streamUri @0 :Text;
  samplePosition @1 :UInt64;
  bytePosition @2 :UInt64;
  wallClock @3 :Common.Timestamp;
}

struct StreamChunkFull {
  streamUri @0 :Text;
  path @1 :Text;
  bytesWritten @2 :UInt64;
  samplesWritten @3 :UInt64;  # 0 for MIDI
  wallClock @4 :Common.Timestamp;
}

struct StreamError {
  streamUri @0 :Text;
  error @1 :Text;
  recoverable @2 :Bool;
}
```

**Add to `envelope.capnp` Payload union:**
```capnp
    # === Stream Capture (Hootenanny → Chaosgarden) ===
    streamStart @48 :Streams.StreamStart;
    streamSwitchChunk @49 :Streams.StreamSwitchChunk;
    streamStop @50 :Streams.StreamStop;
```

**Add to `broadcast.capnp` Broadcast union:**
```capnp
    # === Stream Capture Events ===
    streamHeadPosition @10 :Streams.StreamHeadPosition;
    streamChunkFull @11 :Streams.StreamChunkFull;
    streamError @12 :Streams.StreamError;
```

### Chunk Handoff Sequence

```
hootenanny                               chaosgarden
    │                                         │
    │── StreamStart{uri, def, path_0} ───────▶│
    │                                         │ mmap(path_0), write samples
    │                                         │
    │◀──── StreamHeadPosition ────────────────│ (periodic, via Broadcast)
    │                                         │
    │◀──── StreamChunkFull{path_0, N} ────────│ chunk full
    │                                         │
    │  seal(path_0) → content_hash            │
    │  create staging path_1                  │
    │                                         │
    │── StreamSwitchChunk{uri, path_1} ──────▶│
    │                                         │ close(path_0), mmap(path_1)
    │                                         │
    │◀──── StreamHeadPosition ────────────────│ (continues)
```

### IOPub vs Dedicated Socket

We use IOPub for stream events because:
- Fits existing "event broadcast" pattern
- Subscribers already wired up
- Low frequency (head position every ~100ms, chunk sealed every ~2.7s)

### RT-Safe ZMQ Sending

The RT thread must avoid heap allocations. Cap'n Proto helps here:

**Cap'n Proto characteristics:**
- Zero-copy reads (read directly from wire buffer)
- Message building allocates, but can use scratch space
- Wire format is naturally aligned, no parsing needed

**Approach: Pre-allocated scratch space**

```rust
// Pre-allocate at startup (not in RT path)
thread_local! {
    static SCRATCH: RefCell<capnp::message::ScratchSpaceHeapAllocator<'static>> =
        RefCell::new(ScratchSpaceHeapAllocator::new(
            Box::leak(vec![capnp::Word::NULL_WORD; 64].into_boxed_slice())
        ));
}

// In RT callback - uses pre-allocated scratch
fn send_head_position(socket: &zmq::Socket, uri: &str, pos: u64, bytes: u64) -> Result<()> {
    SCRATCH.with(|scratch| {
        let mut message = capnp::message::Builder::new(scratch.borrow_mut());
        let mut broadcast = message.init_root::<broadcast_capnp::broadcast::Builder>();
        let mut head = broadcast.init_stream_head_position();
        head.set_stream_uri(uri);
        head.set_sample_position(pos);
        head.set_byte_position(bytes);
        // ... set wall_clock

        let words = message.get_segments_for_output();
        // Send via ZMQ - capnp segments are already contiguous
        socket.send(words_to_bytes(words), zmq::DONTWAIT)?;
        Ok(())
    })
}
```

Why this works:
- Scratch space pre-allocated, reused per message
- No heap allocation in hot path
- Cap'n Proto wire format requires no serialization step
- `DONTWAIT` ensures we don't block

**Note:** The 64-word scratch space (~512 bytes) is plenty for our small notification messages. Increase if messages grow.

**The real zero-copy path is the mmap'd chunks.** The ZMQ notification is just metadata - "safe to read up to position X". The actual audio/MIDI data never crosses ZMQ.

---

## Staging Manifests

Stream manifests are **staging artifacts** - mutable until archived:

```
cas/
├── content/
│   └── blake3/ab/cd/...     # sealed, immutable
└── staging/
    ├── blake3/ef/gh/...     # staging chunks (audio/midi data)
    └── blake3/12/34/...     # staging manifests (updated atomically)
```

**Atomic manifest updates:**

1. Write new manifest to temp file
2. `rename()` over existing manifest path
3. Readers see either old or new, never partial

This gives clean reads without locking. Manifest churn is fine - they're small JSON files.

**Archival:**

When a session stops:
1. All staging chunks → seal to content
2. Manifest → seal to content (final snapshot)
3. Session artifact created with references

---

## Cross-Cutting Concerns

### Garbage Collection

Sealed chunks are protected by manifest references. When a stream manifest drops old chunks (retention window), those chunks become eligible for GC. Staging chunks without references can be cleaned up more aggressively.

Retention policy options (to be designed):
- Time-based: "retain 1 hour"
- Count-based: "retain N chunks"
- Size-based: "retain 1GB per stream"

### Failure Recovery

If chaosgarden crashes mid-chunk:
- Staging file exists but may be truncated
- On restart, scan staging directory
- Truncated chunks can be:
  - Discarded (lose partial data)
  - Recovered up to last valid sample (complex)
  - Kept as-is with "incomplete" flag

Start with discard, add recovery if needed.

### Artifact Lineage for Slices

When a slice is created from a stream:
```
Artifact: slice_xyz (audio/wav)
├── content_hash: blake3:materialized_content
├── parent: artifact_stream_abc (stream-definition)
├── source_window: { from: T1, to: T2 }
├── source_chunks: [chunk_47, chunk_48, staging_49]
└── tags: [source:eurorack, sliced-from:stream_abc]
```

Virtual slices (referencing chunks without materializing):
```
Artifact: slice_xyz (application/x-chunk-manifest)
├── content_hash: blake3:manifest_content
├── manifest: { chunks: [...], sample_range: [...] }
└── materializable: true  # can be rendered to WAV on demand
```

---

## Clock Correlation Strategy

Both chaosgarden and hootenanny capture `SystemTime::now()` at checkpoints. Correlation is approximate but sufficient:

- **Audio sample position is authoritative** - the true timeline for audio data
- **Wall clock snapshots** - captured at segment start/end, chunk boundaries
- **MIDI clock ticks** - if available from device, captured alongside wall clock

Correlation happens at slice time, not capture time. Given a time range request:
1. Map wall clock range to approximate sample positions
2. Use sample positions as the precise slice boundaries
3. Include clock snapshot data in slice artifact for provenance

This is good enough for our use case. If sub-millisecond sync is needed later, we can add PTP/hardware clock support.

---

## Assumptions

- **Same user/permissions**: hootenanny and chaosgarden run as the same user with shared filesystem access
- **Filesystem performance**: NVMe or similar fast storage for staging directory
- **Process lifetime**: hootenanny outlives chaosgarden (control plane manages daemon lifecycle)

---

## Open Questions

| Question | Context | Status |
|----------|---------|--------|
| Chunk size tuning | 128K samples (~2.7s) is initial guess, may need adjustment | Experiment |
| mmap flag optimization | Which flags work best for RT + reader pattern | Experiment |
| Retention policy syntax | How users express "keep last hour" | Design needed |
| Virtual slice rendering | On-demand WAV generation from chunk manifest | Design needed |
| Multi-device clock sync | If multiple audio interfaces, how to align | Future |
| Device disconnect handling | What to record when device unplugged | Design needed |

### Resolved Questions

| Question | Decision |
|----------|----------|
| CAS staging ownership | CAS crate owns staging logic |
| File lifecycle management | Hootenanny manages, chaosgarden only mmaps |
| Cross-filesystem support | Copy fallback in hootenanny, transparent |
| Clock correlation | Capture all clocks, correlate at slice time |
| Manifest ownership | Hootenanny owns, updated atomically |
