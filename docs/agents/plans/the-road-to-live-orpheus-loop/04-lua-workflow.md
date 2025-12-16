# 04: Lua Workflow Orchestration

**File:** `scripts/orpheus_loop.lua` or `crates/luanette/examples/`
**Focus:** End-to-end workflow: generate → render → play
**Dependencies:** 01-pipewire-daemon, 02-content-resolver, 03-playback-mixing
**Unblocks:** None (final task)

---

## Task

Write a Luanette script that orchestrates the full workflow:
1. Generate drums MIDI with Orpheus
2. Generate melody MIDI with Orpheus
3. Render both to WAV via soundfont
4. Create timeline regions
5. Play the loop

**Why last?** This is the integration test. All prior tasks must work for this to succeed.

**Deliverables:**
1. Working Lua script demonstrating the workflow
2. Documentation of the pattern for future use

**Definition of Done:**
```bash
# Run the script, hear drums + melody playing together
luanette scripts/orpheus_loop.lua
```

## Out of Scope

- Real-time generation (pre-generate, then play)
- Dynamic tempo/key detection
- User interaction during playback

---

## Luanette API Reference

Check `crates/luanette/src/stdlib/` for available modules:

```lua
-- CAS / Artifacts
local hash = cas.store(content)
local data = cas.get(hash)
local artifact = artifact.upload(path, mime_type)

-- Orpheus (returns job object)
local job = orpheus.generate({ max_tokens = 512, temperature = 0.9 })
local result = job:poll(60000)  -- Wait up to 60s (colon = method call)
local midi_artifact = result.artifact_id

-- MIDI to WAV
local wav_job = convert.midi_to_wav(midi_hash, soundfont_hash, sample_rate)
local wav_result = wav_job:poll(30000)

-- Garden (chaosgarden control)
garden.set_tempo(120)
garden.create_region(position, duration, "play_content", artifact_id)
garden.play()
garden.stop()
garden.seek(beat)
```

---

## Script Structure

```lua
#!/usr/bin/env luanette
-- orpheus_loop.lua: Generate and play drums + melody

local TEMPO = 120
local DURATION_BEATS = 16  -- 8 bars of 4/4
local SAMPLE_RATE = 48000  -- Match chaosgarden config (48000 or 96000)

-- Find or use default soundfont
local soundfont = artifact.find_by_tag("type:soundfont")[1]
if not soundfont then
    error("No soundfont found. Upload one first.")
end
print("Using soundfont: " .. soundfont.id)

-- Generate drums
print("Generating drums...")
local drums_job = orpheus.generate({
    max_tokens = 256,
    temperature = 0.7,  -- Lower = more predictable rhythm
    tags = { "drums", "rhythm" }
})
local drums_midi = drums_job:poll(60000)
print("Drums MIDI: " .. drums_midi.artifact_id)

-- Generate melody
print("Generating melody...")
local melody_job = orpheus.generate({
    max_tokens = 512,
    temperature = 1.0,  -- Higher = more variation
    tags = { "melody", "lead" }
})
local melody_midi = melody_job:poll(60000)
print("Melody MIDI: " .. melody_midi.artifact_id)

-- Render both to WAV (must match chaosgarden session rate)
print("Rendering drums to WAV...")
local drums_wav_job = convert.midi_to_wav(
    drums_midi.content_hash,
    soundfont.content_hash,
    SAMPLE_RATE
)
local drums_wav = drums_wav_job:poll(30000)
print("Drums WAV: " .. drums_wav.artifact_id)

print("Rendering melody to WAV...")
local melody_wav_job = convert.midi_to_wav(
    melody_midi.content_hash,
    soundfont.content_hash,
    SAMPLE_RATE
)
local melody_wav = melody_wav_job:poll(30000)
print("Melody WAV: " .. melody_wav.artifact_id)

-- Setup timeline
print("Setting up timeline...")
garden.stop()
garden.seek(0)
garden.set_tempo(TEMPO)

-- Clear existing regions (optional)
for _, region in ipairs(garden.get_regions()) do
    garden.delete_region(region.region_id)
end

-- Create regions for both tracks at beat 0
garden.create_region(0, DURATION_BEATS, "play_content", drums_wav.artifact_id)
garden.create_region(0, DURATION_BEATS, "play_content", melody_wav.artifact_id)

-- Play!
print("Playing...")
garden.play()

-- Let it play for the duration
local duration_seconds = (DURATION_BEATS / TEMPO) * 60
print(string.format("Playing for %.1f seconds...", duration_seconds))
os.sleep(duration_seconds)

garden.stop()
print("Done!")
```

---

## Alternative: Looping Playback

For continuous looping, modify the script:

```lua
-- After creating regions, loop forever
garden.play()

while true do
    local status = garden.status()
    -- When position exceeds duration, seek back to 0
    if status.position >= DURATION_BEATS then
        garden.seek(0)
    end
    os.sleep(0.1)
end
```

Or add loop support to chaosgarden itself (future work).

---

## Testing

Manual testing workflow:

1. Ensure chaosgarden daemon is running
2. Ensure hootenanny MCP server is running
3. Run the script:
   ```bash
   luanette scripts/orpheus_loop.lua
   ```
4. Verify audio output (drums + melody mixed)

---

## Acceptance Criteria

- [ ] Script runs without errors
- [ ] Both MIDI files generate successfully
- [ ] Both WAV files render successfully
- [ ] Both regions play simultaneously
- [ ] Audio is audible through PipeWire
- [ ] Script is documented for future reference
