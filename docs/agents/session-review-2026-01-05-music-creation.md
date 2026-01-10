# Session Review: Music Creation with Holler/Vibeweaver
**Date:** 2026-01-05
**Participants:** Claude, Amy Tobey
**Duration:** ~2 hours
**Context:** 158k/200k tokens (79%)

## Summary

Extended music creation session exploring holler's capabilities: drum patterns, chorales, blues rock, counterpoint, and an original composition. Identified workflow patterns, pain points, and improvement opportunities.

---

## What We Built

| Piece | Bars | Key | Instruments | Notes |
|-------|------|-----|-------------|-------|
| Four-on-the-floor rock beat | 16 | - | Drums | GM drums via Timber SF |
| Synth chorale (I-IV-V-bVII-I) | 3 | C | 4 voices | FF4 Square/Saw leads |
| Blues rock song | 32 | E | Drums, bass, guitar | 12-bar blues, ~1 min |
| Counterpoint octet | 36 | E minor | 8 voices | Canon, species counterpoint |
| Claude's Song | 36 | D major | Piano, strings, flute, bass, harp | Modal jazz ballad, ~2 min |

**Total artifacts created:** 15+ (MIDI, audio, PDF, LilyPond source)

---

## What Worked Well

### 1. The Core Loop
```
weave_eval (Python/mido)
  → artifact_upload
  → project (render with soundfont)
  → job_poll
  → schedule
  → garden_play
```
This pattern was reliable once established. Each step has clear inputs/outputs.

### 2. Vibeweaver + mido/pretty_midi
- **Persistent kernel** - imports and helpers stay available across calls
- **Full control** - can construct any MIDI structure programmatically
- **Immediate feedback** - `weave_eval` returns stdout for debugging

### 3. Soundfont Rendering
- `project` with `target: {type: audio, soundfont_hash: ...}` just works
- FF4 soundfont provided great retro game sounds
- Timber GM for realistic instruments

### 4. Chaosgarden Transport
- `schedule(at=beat)` - intuitive beat-based positioning
- `seek`, `play`, `stop` - simple transport controls
- Multiple regions can overlap for layering

### 5. Artifact System
- Everything tracked with IDs, tags, creator
- HTTP accessible (`/artifact/{id}`)
- Content-addressable deduplication

---

## Pain Points

### 1. No Timeline Cleanup ✅ FIXED
**Problem:** Can't clear or delete regions from chaosgarden.
**Workaround:** Keep seeking to higher beat numbers.
**Impact:** Timeline accumulates cruft; no way to start fresh.

**Fix (2026-01-05):** Implemented `garden_clear_regions` tool. Also added MCP schemas for
the existing `garden_delete_region`, `garden_move_region`, and `garden_get_regions` tools
that were missing from the registry.

Commit: `17d328f feat(chaosgarden): add garden_clear_regions tool`

### 2. ABC Notation Flattens Polyphony ✅ FIXED
**Problem:** Multi-voice ABC (`V:1`, `V:2`) serializes sequentially instead of playing simultaneously.
**Workaround:** Abandoned ABC; used mido directly for polyphonic content.
**Impact:** ABC is unusable for anything beyond monophonic melodies.

**Example that failed:**
```abc
V:1
e2 f2 | g2 f2 |
V:2
c2 c2 | d2 d2 |
```
Played as: e-f-g-f-c-c-d-d (sequential) instead of chords.

**Root cause:** `route_elements_to_voices()` in the ABC parser required voice definitions
in the header. When `V:` switches appeared only in the body (without header definitions),
all elements were merged into a single voice and VoiceSwitch elements were filtered out.

**Fix (2026-01-05):** Modified parser to detect VoiceSwitch elements and create separate
voices even without header definitions. Added tests to verify multi-voice MIDI output.

Commit: `0d7d1e7 fix(abc): handle multi-voice ABC without header definitions`

### 3. Python User Site-Packages Not in sys.path ✅ FIXED
**Problem:** PyO3's embedded Python doesn't include `~/.local/lib/python3.13/site-packages`.
**Fix applied:** Added `PYTHONPATH` to vibeweaver.service.
**Better solution:** Detect and add user site-packages automatically in kernel.rs.

**Fix (2026-01-05):** Added `ensure_user_site_packages()` to vibeweaver's kernel.rs that
uses Python's `site` module to auto-detect and add user site-packages. The PYTHONPATH
workaround in vibeweaver.service is no longer needed.

Commit: `f57a54d feat(vibeweaver): auto-detect user site-packages in Python kernel`

### 4. LilyPond Rhythm Math is Error-Prone
**Problem:** Manually translating melody to LilyPond notation led to barcheck errors.
**Root cause:** Wrote `g2.~ g8 fis8 e8 r8` (5 beats) instead of `g2. fis4` (4 beats).

**Improvement ideas:**
- Helper to validate rhythm per bar before rendering
- Generate LilyPond from Python data structure (single source of truth)
- Add `lily_from_midi(artifact_id)` tool

### 5. No Incremental Preview
**Problem:** Must render entire piece to hear anything.
**Impact:** Slow iteration; ~2 minutes to hear a change.

**Wanted:**
- Preview a single bar or phrase
- Live MIDI playback without full audio render
- Scrub/shuttle through timeline

### 6. Tempo Mismatch
**Problem:** MIDI files have internal tempo; garden plays at its own BPM (default 120).
**Impact:** Pieces play at unexpected speeds; duration calculations are confusing.

**Solutions:**
- Extract tempo from MIDI, set garden tempo automatically
- Or always render to audio at correct tempo (current workaround)

---

## Feature Requests

### High Priority
| Feature | Rationale | Status |
|---------|-----------|--------|
| `garden_clear` | Essential for iteration | ✅ Done |
| `garden_delete_region(id)` | Selective cleanup | ✅ Already existed, added MCP schema |
| Fix ABC polyphony | Make ABC useful for real music | ✅ Done |
| Auto-detect user site-packages | Remove manual PYTHONPATH config | ✅ Done |

### Medium Priority
| Feature | Rationale | Status |
|---------|-----------|--------|
| `midi_to_lilypond` | Generate sheet music from MIDI | Not started |
| Live MIDI preview | Faster iteration without full render | Not started |
| Tempo sync | Match garden to MIDI tempo | Not started |
| Bar-level preview | Hear small sections quickly | Not started |

### Nice to Have
| Feature | Rationale | Status |
|---------|-----------|--------|
| Python music prelude | Built-in `DRUMS`, `chord()`, `scale()`, `humanize()` | Not started |
| `weave_eval` autocomplete | Show available functions in kernel | Not started |
| Variation generator | Create N variations of a pattern | Not started |

---

## Workflow Patterns Discovered

### Pattern: Layer and Play
```python
# Create base beat
beat = create_drum_pattern()
upload(beat) → render() → schedule(at=0)

# Add layer
melody = create_melody()
upload(melody) → render() → schedule(at=0)  # Same beat position

garden_seek(0)
garden_play()  # Both layers play together
```

### Pattern: Lead Sheet First
1. Design chord changes (text)
2. Design melody (text with beat positions)
3. Write LilyPond for visualization
4. Write Python/mido for MIDI
5. Render and iterate

This mirrors software development: design → implement → test.

### Pattern: Soundfont Selection
```python
# List available soundfonts
artifact_list(tag="type:soundfont")

# Inspect presets
soundfont_inspect(hash)

# Render with specific soundfont
project(encoding, target={type: audio, soundfont_hash: hash})
```

---

## Artifacts Summary

### Claude's Song (Final)
| Artifact | ID | Type |
|----------|-----|------|
| Lead sheet PDF | `artifact_0f2712e4c99d` | application/pdf |
| Lead sheet MIDI | `artifact_1b07daa02fa5` | audio/midi |
| LilyPond source | `artifact_48312ad09d0d` | text/x-lilypond |
| Full arrangement MIDI | `artifact_25c6fec9aae7` | audio/midi |
| Full arrangement audio | `artifact_2e0b1522eb1a` | audio/wav |

### Other Notable Artifacts
| Piece | Audio Artifact |
|-------|----------------|
| Rock beat (original) | `artifact_d5068998251a` |
| Rock beat (remixed) | `artifact_411b473cc239` |
| Blues rock | `artifact_8d756f0bd6e0` |
| Counterpoint octet | `artifact_bfeefceb6a35` |

---

## Next Steps

1. ~~**Implement `garden_clear` and `garden_delete_region`**~~ ✅ Done
2. ~~**Fix ABC polyphony**~~ ✅ Done
3. ~~**Auto-add user site-packages**~~ ✅ Done
4. **Add `midi_to_lilypond` tool** - Close the loop on sheet music generation
5. **Python music prelude** - Add `DRUMS`, `chord()`, `scale()` helpers to vibeweaver
6. **Tempo sync** - Extract tempo from MIDI when scheduling

---

## Appendix: Session Statistics

- **Tool calls:** ~100+
- **Python code blocks:** ~15
- **MIDI files created:** 8
- **Audio renders:** 6
- **LilyPond compiles:** 3
- **Bugs found:** 4 (ABC polyphony, PYTHONPATH, LilyPond rhythm, region accumulation)
- **Bugs fixed in session:** 2 (PYTHONPATH, LilyPond rhythm)
- **Bugs fixed post-session:** 3 (ABC polyphony, PYTHONPATH auto-detect, garden_clear)

---

## Follow-up Session: 2026-01-05 (evening)

Fixed all high-priority issues identified above:

| Issue | Fix | Commit |
|-------|-----|--------|
| No timeline cleanup | Added `garden_clear_regions` tool | `17d328f` |
| ABC polyphony bug | Fixed voice routing in parser | `0d7d1e7` |
| PYTHONPATH workaround | Auto-detect user site-packages | `f57a54d` |

All fixes verified with tests and live service restart.

---

*Report generated by Claude, 2026-01-05*
*Updated with fixes, 2026-01-05*
