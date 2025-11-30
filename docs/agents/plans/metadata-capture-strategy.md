# Metadata Capture Strategy

Capture rich metadata at the artifact level now. Synthesize lineage graphs later.

## Philosophy

> "Keep metadata close to the object"

Every artifact's `metadata` field should contain everything we'd need to:
1. Understand what it is without fetching related artifacts
2. Reconstruct its provenance later
3. Link to external systems (MusicBrainz, AcoustID) when ready

Don't build the graph now - just capture the facts.

## Current Artifact Structure

```rust
struct Artifact {
    id: ArtifactId,
    content_hash: ContentHash,      // CAS reference
    variation_set_id: Option<...>,  // Grouping
    parent_id: Option<ArtifactId>,  // Single parent link
    tags: Vec<String>,              // Prefixed tags (type:, phase:, role:)
    creator: String,                // Who made it
    metadata: serde_json::Value,    // <- THIS IS WHERE THE GOOD STUFF GOES
    created_at: DateTime<Utc>,
    access_count: u64,
    last_accessed: Option<...>,
}
```

## Metadata Schemas by Artifact Type

### Audio Import (future `audio_import` tool)

When importing external audio, capture everything Symphonia extracts:

```json
{
  "type": "audio_import",
  "source": {
    "original_filename": "jazz_loop.mp3",
    "original_path": "/path/to/file.mp3",  // optional, may be ephemeral
    "import_time": "2024-01-15T10:30:00Z"
  },
  "technical": {
    "codec": "mp3",
    "container": "mp3",
    "sample_rate": 44100,
    "channels": 2,
    "channel_layout": "stereo",
    "bit_depth": null,  // null for lossy
    "bitrate_kbps": 320,
    "duration_seconds": 12.5,
    "total_samples": 551250
  },
  "tags": {
    // Symphonia StandardTagKey -> value
    // Use snake_case versions of the enum variants
    "track_title": "Blue Monday Loop",
    "artist": "Unknown",
    "album": null,
    "bpm": 120,
    "genre": "Electronic",
    "date": "2023",
    "isrc": null,
    "musicbrainz_recording_id": "abc123-...",  // if present
    "musicbrainz_work_id": null,
    "musicbrainz_artist_id": null,
    "replay_gain_track_gain": -6.5,
    "replay_gain_track_peak": 0.95
    // ... all other tags present in source
  },
  "visuals": [
    // CAS refs to extracted images
    {
      "hash": "abc123...",
      "usage": "front_cover",
      "mime_type": "image/jpeg",
      "dimensions": [500, 500]
    }
  ],
  "fingerprint": {
    // Computed on import (optional, requires chromaprint)
    "chromaprint": "AQAA...",
    "duration_fp": 12.5,
    "acoustid_lookup": null  // filled in later if we query
  },
  "vendor_data": [
    // Opaque blobs from Symphonia, preserved for roundtrip
    { "ident": "com.apple.iTunes", "hash": "..." }
  ]
}
```

**Tags for audio imports:**
- `type:audio`
- `format:mp3` (or flac, wav, ogg, etc.)
- `imported:true`
- `has_musicbrainz:true` (if any MB tags present)

### MIDI Generation (Orpheus)

Already capturing most of this, but let's standardize:

```json
{
  "type": "orpheus_generation",
  "model": {
    "name": "base",
    "variant": "base"
  },
  "params": {
    "temperature": 1.0,
    "top_p": 0.95,
    "max_tokens": 1024
  },
  "seed": {
    // If seeded generation
    "hash": "def456...",
    "artifact_id": "artifact_def456"  // if we know it
  },
  "continuation": {
    // If continuation
    "input_hash": "ghi789...",
    "input_artifact_id": "artifact_ghi789"
  },
  "generation": {
    "job_id": "job_xxx",
    "duration_ms": 2340,
    "tokens_generated": 512
  },
  "analysis": {
    // Post-generation analysis (can add later)
    "duration_seconds": 8.5,
    "note_count": 47,
    "track_count": 1,
    "tempo_bpm": null,  // if detectable from MIDI
    "time_signature": null,
    "key_signature": null
  }
}
```

### ABC to MIDI (`abc_to_midi`)

```json
{
  "type": "abc_to_midi",
  "source": {
    "abc_text": "X:1\nT:Test\n...",  // or hash if stored separately
    "abc_hash": "abc123..."  // if we CAS the ABC
  },
  "params": {
    "channel": 0,
    "velocity": 80,
    "tempo_override": null,
    "transpose": 0
  },
  "parsed": {
    // From abc_parse
    "title": "Test Tune",
    "composer": null,
    "meter": "4/4",
    "key": "C",
    "tempo": 120
  }
}
```

### WAV Render (`convert_midi_to_wav`)

```json
{
  "type": "wav_render",
  "source": {
    "midi_hash": "abc123...",
    "midi_artifact_id": "artifact_abc123"
  },
  "soundfont": {
    "hash": "def456...",
    "name": "GeneralUser GS",  // if we can extract it
    "artifact_id": null  // if soundfont is an artifact
  },
  "params": {
    "sample_rate": 44100
  },
  "output": {
    "duration_seconds": 12.5,
    "channels": 2,
    "bit_depth": 16
  }
}
```

### Beat Analysis (`beatthis_analyze`)

```json
{
  "type": "beat_analysis",
  "source": {
    "audio_hash": "abc123...",
    "audio_artifact_id": "artifact_abc123"
  },
  "results": {
    "bpm": 120.5,
    "num_beats": 48,
    "num_downbeats": 12,
    "duration_seconds": 24.0,
    "beats_per_measure": 4,
    "beat_times": [0.0, 0.5, 1.0, ...],
    "downbeat_times": [0.0, 2.0, 4.0, ...]
  },
  "confidence": null  // if model provides it
}
```

### Audio Slice (future)

```json
{
  "type": "audio_slice",
  "source": {
    "audio_hash": "abc123...",
    "audio_artifact_id": "artifact_abc123"
  },
  "slice": {
    "start_seconds": 4.0,
    "end_seconds": 8.0,
    "duration_seconds": 4.0
  },
  "context": {
    "beat_aligned": true,
    "start_beat": 8,
    "end_beat": 16
  }
}
```

## Tag Conventions

Prefix-based for easy filtering:

| Prefix | Purpose | Examples |
|--------|---------|----------|
| `type:` | Content type | `type:midi`, `type:audio`, `type:wav` |
| `format:` | File format | `format:mp3`, `format:flac`, `format:mid` |
| `phase:` | Workflow stage | `phase:initial`, `phase:refined`, `phase:final` |
| `role:` | Musical role | `role:melody`, `role:drums`, `role:bass` |
| `source:` | Origin | `source:orpheus`, `source:import`, `source:abc` |
| `has:` | Feature flags | `has:musicbrainz`, `has:fingerprint`, `has:beats` |
| `bpm:` | Tempo bucket | `bpm:120`, `bpm:slow`, `bpm:fast` |
| `key:` | Musical key | `key:Cmaj`, `key:Amin` |
| `genre:` | Genre tags | `genre:electronic`, `genre:jazz` |

## What NOT to Store in Metadata

- Large binary data (put in CAS, reference by hash)
- Computed graphs/relationships (synthesize later)
- Mutable state (use separate tracking)

## Migration Path

For existing artifacts without rich metadata:

1. Add `metadata.type` to identify schema version
2. Backfill what we can from existing fields
3. New artifacts get full metadata from creation

## Future: Lineage Synthesis

When ready to build the graph:

```
For each artifact:
  - Look at metadata.source.*, metadata.seed.*, metadata.continuation.*
  - Find referenced artifacts by hash or ID
  - Build edges: "derived_from", "seeded_by", "continuation_of", "rendered_from"
  - Link to external IDs: metadata.tags.musicbrainz_*
```

The key insight: if we capture hashes and artifact IDs at creation time, we can reconstruct any relationship later without changing the artifact structure.

## Implementation Priority

1. **Now**: Standardize metadata for `orpheus_*` tools (already have most of this)
2. **Now**: Standardize metadata for `abc_to_midi`
3. **Now**: Standardize metadata for `convert_midi_to_wav`
4. **Soon**: Add `audio_import` tool with full Symphonia metadata extraction
5. **Later**: Add fingerprinting on import
6. **Later**: Add MusicBrainz lookup integration
7. **Eventually**: Build lineage graph from captured metadata
