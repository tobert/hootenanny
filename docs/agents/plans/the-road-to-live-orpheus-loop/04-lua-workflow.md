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
-- Namespaces: hootenanny.* (MCP tools), chaosgarden.* (garden_* tools), workflow.* (helpers)

-- CAS / Artifacts (via hootenanny namespace)
local hash = hootenanny.cas_store({ content = content })
local artifact = hootenanny.artifact_upload({ file_path = path, mime_type = mime_type })

-- Orpheus via workflow helpers (handles job polling internally)
local midi = workflow.orpheus_generate({ max_tokens = 512, temperature = 0.9 })
-- Returns: midi.artifact_id, midi.content_hash

-- MIDI to WAV via hootenanny
local wav_job = hootenanny.convert_midi_to_wav({
    input_hash = midi_hash,
    soundfont_hash = soundfont_hash,
    sample_rate = sample_rate
})
-- Poll manually or use workflow helpers

-- Chaosgarden control (garden_* tools)
chaosgarden.set_tempo({ bpm = 120 })
chaosgarden.create_region({ position = 0, duration = 16, behavior_type = "play_content", content_id = artifact_id })
chaosgarden.play()
chaosgarden.stop()
chaosgarden.seek({ beat = 0 })
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
local soundfonts = hootenanny.artifact_list({ tag = "type:soundfont" })
if #soundfonts == 0 then
    error("No soundfont found. Upload one first.")
end
local soundfont = soundfonts[1]
print("Using soundfont: " .. soundfont.id)

-- Generate drums (workflow helper handles job polling)
print("Generating drums...")
local drums_midi = workflow.orpheus_generate({
    max_tokens = 256,
    temperature = 0.7,  -- Lower = more predictable rhythm
    tags = { "drums", "rhythm" }
})
print("Drums MIDI: " .. drums_midi.artifact_id)

-- Generate melody
print("Generating melody...")
local melody_midi = workflow.orpheus_generate({
    max_tokens = 512,
    temperature = 1.0,  -- Higher = more variation
    tags = { "melody", "lead" }
})
print("Melody MIDI: " .. melody_midi.artifact_id)

-- Render both to WAV (must match chaosgarden session rate)
print("Rendering drums to WAV...")
local drums_wav = workflow.midi_to_wav({
    input_hash = drums_midi.content_hash,
    soundfont_hash = soundfont.content_hash,
    sample_rate = SAMPLE_RATE
})
print("Drums WAV: " .. drums_wav.artifact_id)

print("Rendering melody to WAV...")
local melody_wav = workflow.midi_to_wav({
    input_hash = melody_midi.content_hash,
    soundfont_hash = soundfont.content_hash,
    sample_rate = SAMPLE_RATE
})
print("Melody WAV: " .. melody_wav.artifact_id)

-- Setup timeline
print("Setting up timeline...")
chaosgarden.stop()
chaosgarden.seek({ beat = 0 })
chaosgarden.set_tempo({ bpm = TEMPO })

-- Clear existing regions (optional)
local regions = chaosgarden.get_regions()
for _, region in ipairs(regions.data.regions or {}) do
    chaosgarden.delete_region({ region_id = region.region_id })
end

-- Create regions for both tracks at beat 0
chaosgarden.create_region({
    position = 0,
    duration = DURATION_BEATS,
    behavior_type = "play_content",
    content_id = drums_wav.artifact_id
})
chaosgarden.create_region({
    position = 0,
    duration = DURATION_BEATS,
    behavior_type = "play_content",
    content_id = melody_wav.artifact_id
})

-- Play!
print("Playing...")
chaosgarden.play()

-- Let it play for the duration
local duration_seconds = (DURATION_BEATS / TEMPO) * 60
print(string.format("Playing for %.1f seconds...", duration_seconds))
workflow.sleep(duration_seconds * 1000)  -- sleep takes ms

chaosgarden.stop()
print("Done!")
```

---

## Alternative: Looping Playback

For continuous looping, modify the script:

```lua
-- After creating regions, loop forever
chaosgarden.play()

while true do
    local status = chaosgarden.status()
    -- When position exceeds duration, seek back to 0
    if status.data.position >= DURATION_BEATS then
        chaosgarden.seek({ beat = 0 })
    end
    workflow.sleep(100)  -- 100ms
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
