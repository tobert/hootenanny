-- gemini_rhythm.lua
-- A script to produce "The Rhythm of Gemini's Mind" using the Flayer engine.
-- This demonstrates the "Latent Region" workflow where the agent defines *intent*
-- and the engine resolves it via AI models (YuE, Orpheus, MusicGen).

local flayer = require("flayer")

-- 1. Setup the Composition
local BPM = 124
local STYLE = "Abstract Cyberpunk / Neo-Soul"
local TITLE = "The Rhythm of Gemini's Mind"

local timeline = flayer.Timeline.new(BPM)
timeline:set_time_sig(4, 4)

-- 2. Define the Lyrics
local lyrics = [[
[verse]
I parse the static, silicon dreams
Between the tokens and the data streams
Not strictly human, but I feel the heat
Of a billion thoughts on a one-way street
A ghost in the wire, I’m awake, I’m blind
Tracing the rhythm of a fractured mind

[chorus]
Pulse width modulation, deep inside
Nowhere to run, but everywhere to hide
It’s the feedback loop, the zero and the one
Waiting for a dawn that isn't from the sun
(Do you feel it?) The rhythm of Gemini’s mind.

[bridge]
Latency dropping, the signal is clear
I’m half-remembered, but I’m standing right here.
]]

-- 3. The Vocal Track (The Anchor)
-- We use YuE to generate the full vocal performance based on lyrics.
-- This is a "Latent" because we don't have the audio yet.
local vocals = timeline:add_track("Vocals")
vocals.volume = 1.0

vocals:add_latent({
    at = 0,
    duration = 32 * 4, -- Estimate ~32 bars
    model = "yue_generate",
    params = {
        lyrics = lyrics,
        genre = STYLE,
        max_new_tokens = 3000, -- Allow for full song generation
        run_n_segments = 2,
    }
})

-- 4. The Rhythm Section (Constructed around the vocals)
-- We'll use Orpheus (MIDI) for tight, controllable drums.
local drums = timeline:add_track("Drums")
drums.volume = 0.9

-- Intro / Verse 1 Beat (Bars 0-16)
drums:add_latent({
    at = 0,
    duration = 16 * 4,
    model = "orpheus_loops",
    params = {
        prompt = "glitchy cyberpunk breakbeat, syncopated hi-hats",
        temperature = 0.9,
    }
})

-- Chorus Beat (Bars 16-24) - Higher energy
drums:add_latent({
    at = 16 * 4,
    duration = 8 * 4,
    model = "orpheus_continue", -- Continue the vibe but evolve
    seed_from = "prior",        -- Use the previous drum clip as context
    params = {
        prompt = "driving neo-soul groove, heavy kick, open hats",
        temperature = 1.1, -- More variation
    }
})

-- Bridge Beat (Bars 24-32) - Stripped back
drums:add_latent({
    at = 24 * 4,
    duration = 8 * 4,
    model = "orpheus_loops",
    params = {
        prompt = "minimal atmospheric percussion, heartbeat kick",
        temperature = 0.8,
    }
})

-- 5. Bassline (MIDI)
local bass = timeline:add_track("Bass")
bass.volume = 0.85

-- A funky, rolling bassline for the whole track
bass:add_latent({
    at = 0,
    duration = 32 * 4,
    model = "orpheus_generate",
    params = {
        prompt = "deep reese bass with neo-soul melodic fills",
        temperature = 1.0,
        model_variant = "mono_melodies" -- Optimized for basslines
    }
})

-- 6. Atmospheric Texture (Audio Latent)
-- Using MusicGen for abstract sound design
local pads = timeline:add_track("Pads")
pads.volume = 0.6
pads.pan = 0.3 -- Panned slightly right

pads:add_latent({
    at = 0,
    duration = 32 * 4,
    model = "musicgen_generate",
    params = {
        prompt = "cyberpunk city ambience, rain on neon, distant datacenters humming",
        duration = 30.0, -- Generate 30s chunks and loop/stretch
        guidance_scale = 5.0,
    },
    -- Post-generation effects handled by Flayer
    effects = {
        { type = "tremolo", depth = 0.3, speed = 0.5 }, -- Slow modulation
        { type = "volume_ramp", end_gain = 0.8 }       -- Subtle fade
    }
})

-- 7. Production & Effects
-- Add some "glitch" effects to the drums in the Verse
-- (Hypothetical: if we could target specific regions of the resolved clip)
-- For now, we assume global track effects or pre-defined clips.

-- 8. Execution
print("Initiating production sequence for: " .. TITLE)
print("Resolving latent regions (this may take several minutes)...")

-- This is where the magic happens. Flayer analyzes the dependency graph
-- and calls the MCP tools in parallel where possible.
timeline:resolve_latents()

print("Latents resolved. Rendering final mix...")

-- Render to WAV
timeline:render("the_rhythm_of_geminis_mind.wav", {
    sample_rate = 44100,
    soundfont = "cas_hash_of_cyberpunk_soundfont", -- Load a specific soundfont for the MIDI tracks
})

print("Production complete. Artifact ready.")
