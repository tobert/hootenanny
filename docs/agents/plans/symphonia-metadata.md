# Symphonia Metadata Capabilities

Reference document for designing metadata-aware audio tools in hootenanny.

## Supported Metadata Formats

Symphonia reads metadata from these container/tag formats:

| Format | Description | Common In |
|--------|-------------|-----------|
| **ID3v1** | Legacy 128-byte footer tags | MP3 |
| **ID3v2** | Rich extensible frame-based tags | MP3, AIFF, WAV |
| **Vorbis Comments** | Key=value pairs, UTF-8 | OGG, FLAC |
| **FLAC** | Native comment + picture blocks | FLAC |
| **RIFF INFO** | Chunk-based metadata | WAV |
| **iTunes** | Apple's proprietary atoms | M4A, AAC, ALAC |

## Core Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    MetadataLog                          │
│  Time-ordered container of MetadataRevision instances   │
└─────────────────────────┬───────────────────────────────┘
                          │
        ┌─────────────────┼─────────────────┐
        ▼                 ▼                 ▼
┌───────────────┐ ┌───────────────┐ ┌───────────────┐
│MetadataRevision│MetadataRevision│MetadataRevision│
│  - tags: []    │  - tags: []    │  - tags: []    │
│  - visuals: [] │  - visuals: [] │  - visuals: [] │
│  - vendor: []  │  - vendor: []  │  - vendor: []  │
└───────────────┘ └───────────────┘ └───────────────┘
```

**Why revisions?** Audio files can have multiple metadata blocks (e.g., ID3v2 at start + ID3v1 at end, or streaming metadata updates). Symphonia preserves this temporal ordering.

## Tag System

### Tag Structure

```rust
struct Tag {
    std_key: Option<StandardTagKey>,  // Normalized key (if mappable)
    key: String,                       // Original key from format
    value: Value,                      // Typed value
}
```

### Value Types

| Type | Rust Type | Use Case |
|------|-----------|----------|
| `String` | `String` | Most text fields, catch-all |
| `Binary` | `Box<[u8]>` | Raw data, hashes |
| `Boolean` | `bool` | Compilation flag, podcast flag |
| `Flag` | (unit) | Presence-only tags |
| `Float` | `f64` | ReplayGain values, BPM |
| `SignedInt` | `i64` | Relative values |
| `UnsignedInt` | `u64` | Track numbers, disc numbers |

### StandardTagKey (111 variants)

Symphonia normalizes format-specific keys to a common vocabulary.

#### Core Track Info
- `TrackTitle`, `TrackSubtitle`, `TrackNumber`, `TrackTotal`
- `Album`, `AlbumArtist`, `Artist`, `Arranger`
- `DiscNumber`, `DiscSubtitle`, `DiscTotal`
- `Date`, `ReleaseDate`, `OriginalDate`
- `Genre`, `Mood`, `Language`, `Script`

#### Credits & Personnel
- `Composer`, `Conductor`, `Lyricist`, `Writer`, `OriginalWriter`
- `Performer`, `Ensemble`, `Producer`, `Engineer`
- `MixDj`, `MixEngineer`, `Remixer`
- `EncodedBy`, `Encoder`, `EncoderSettings`, `EncodingDate`

#### Classical Music
- `Opus`, `MovementName`, `MovementNumber`
- `Part`, `PartTotal`, `ContentGroup`

#### Descriptive
- `Comment`, `Description`, `Lyrics`
- `Copyright`, `License`, `Owner`
- `Compilation` (boolean flag)

#### Sorting Keys
- `SortAlbum`, `SortAlbumArtist`, `SortArtist`
- `SortComposer`, `SortTrackTitle`

#### Audio Analysis
- `Bpm` - Beats per minute
- `ReplayGainAlbumGain`, `ReplayGainAlbumPeak`
- `ReplayGainTrackGain`, `ReplayGainTrackPeak`

#### Industry Identifiers
- `IdentIsrc` - International Standard Recording Code
- `IdentBarcode`, `IdentEanUpn`, `IdentUpc`
- `IdentCatalogNumber`, `IdentAsin`, `IdentPn`

#### MusicBrainz Integration (15 keys!)
- `MusicBrainzRecordingId`, `MusicBrainzTrackId`, `MusicBrainzWorkId`
- `MusicBrainzArtistId`, `MusicBrainzAlbumArtistId`
- `MusicBrainzAlbumId`, `MusicBrainzOriginalAlbumId`
- `MusicBrainzReleaseGroupId`, `MusicBrainzReleaseTrackId`
- `MusicBrainzLabelId`, `MusicBrainzDiscId`, `MusicBrainzGenreId`
- `MusicBrainzOriginalArtistId`
- `MusicBrainzReleaseStatus`, `MusicBrainzReleaseType`

#### URLs
- `Url`, `UrlOfficial`, `UrlArtist`, `UrlLabel`
- `UrlSource`, `UrlCopyright`, `UrlPayment`, `UrlPurchase`
- `UrlInternetRadio`, `UrlPodcast`

#### Podcast/Video
- `Podcast`, `PodcastCategory`, `PodcastDescription`, `PodcastKeywords`
- `IdentPodcast`
- `TvShowTitle`, `TvSeason`, `TvEpisode`, `TvEpisodeTitle`, `TvNetwork`

#### Publishing
- `Label`, `ReleaseCountry`, `MediaFormat`
- `PurchaseDate`, `TaggingDate`, `Rating`

#### Original Work
- `OriginalAlbum`, `OriginalArtist`, `OriginalDate`
- `OriginalFile`, `OriginalWriter`
- `Version`

## Visual Attachments

Embedded images with typed disposition.

### Visual Structure

```rust
struct Visual {
    media_type: String,           // MIME type (image/jpeg, image/png)
    dimensions: Option<Size>,     // Width × Height if known
    bits_per_pixel: Option<u32>,  // Color depth
    color_mode: Option<ColorMode>,// Indexed, Rgb, Rgba, etc.
    usage: Option<StandardVisualKey>,
    tags: Vec<Tag>,               // Image-specific metadata
    data: Box<[u8]>,              // Raw image bytes
}
```

### StandardVisualKey (19 variants)

Based on ID3v2 APIC frame types:

| Key | Description |
|-----|-------------|
| `FrontCover` | Album front cover art |
| `BackCover` | Album back cover |
| `Leaflet` | Booklet/liner notes page |
| `Media` | Image of the physical media (CD, vinyl) |
| `LeadArtistPerformerSoloist` | Photo of lead performer |
| `ArtistPerformer` | Photo of artist/performer |
| `Conductor` | Photo of conductor |
| `BandOrchestra` | Photo of band/orchestra |
| `Composer` | Photo of composer |
| `Lyricist` | Photo of lyricist |
| `RecordingLocation` | Photo of recording venue |
| `RecordingSession` | Photo from recording session |
| `Performance` | Photo from live performance |
| `ScreenCapture` | Video screenshot |
| `Illustration` | Artwork/illustration |
| `BandArtistLogo` | Artist/band logo |
| `PublisherStudioLogo` | Label/studio logo |
| `FileIcon` | Small icon for file browsers |
| `OtherIcon` | Alternative icon |

### ColorMode

- `Indexed` - Palette-based
- `Rgb` - 24-bit color
- `Rgba` - 24-bit + alpha
- (others for grayscale, etc.)

## VendorData

Proprietary binary blobs that don't map to standard tags:

```rust
struct VendorData {
    ident: String,    // Vendor identifier
    data: Box<[u8]>,  // Raw bytes
}
```

Useful for preserving format-specific data during transcoding.

## Hootenanny Integration Ideas

### 1. Metadata Inspection Tool

```
audio_inspect:
  - input: any supported format (CAS hash or path)
  - output: JSON with all metadata revisions, tags, visuals (as CAS refs)
```

### 2. Metadata Preservation Pipeline

```
source audio → symphonia decode → [transform] → encode with metadata

Key: extract metadata BEFORE processing, reapply AFTER
```

### 3. Artifact Metadata Enrichment

Map Symphonia tags to hootenanny artifact tags:
- `StandardTagKey::Bpm` → artifact tag `bpm:120`
- `StandardTagKey::Genre` → artifact tag `genre:electronic`
- MusicBrainz IDs → artifact provenance tracking

### 4. Visual Extraction

```
audio_extract_art:
  - input: audio file
  - output: CAS refs to each embedded image + their disposition
```

### 5. Metadata Evolution Tracking

As audio moves through the pipeline (generate → render → beat analysis → slice):
- Preserve original metadata
- Add processing history as new tags
- Track lineage via MusicBrainz-style IDs

### 6. Format-Aware Transcoding

When converting formats, map metadata intelligently:
```
ID3v2 (MP3) ←→ Vorbis Comments (OGG) ←→ iTunes atoms (M4A)
```

StandardTagKey is the Rosetta Stone.

## Technical Notes

### Crate Dependencies

```toml
[dependencies]
symphonia = { version = "0.5", features = ["all"] }
# or selective:
symphonia-core = "0.5"
symphonia-metadata = "0.5"
symphonia-bundle-mp3 = "0.5"  # etc.
```

### Feature Flags

- `aac` - AAC-LC decoder
- `adpcm` - ADPCM decoders
- `alac` - Apple Lossless
- `flac` - FLAC decoder
- `isomp4` - MP4/M4A container
- `mkv` - Matroska/WebM
- `mp1`, `mp2`, `mp3` - MPEG audio
- `ogg` - OGG container
- `pcm` - PCM decoder
- `vorbis` - Vorbis decoder
- `wav` - WAV container
- `aiff` - AIFF container
- `caf` - Core Audio Format
- `all` - Everything

### Memory Safety

`MetadataOptions` includes `Limit` settings to prevent DoS:
- Max tag size
- Max visual size
- Max revision count

Important when processing untrusted audio files.

## References

- [symphonia-core::meta docs](https://docs.rs/symphonia-core/latest/symphonia_core/meta/)
- [symphonia-metadata docs](https://docs.rs/symphonia-metadata/latest/symphonia_metadata/)
- [ID3v2 specification](https://id3.org/id3v2.4.0-frames)
- [Vorbis Comment spec](https://xiph.org/vorbis/doc/v-comment.html)
- [MusicBrainz Picard tags](https://picard-docs.musicbrainz.org/en/appendices/tag_mapping.html)
