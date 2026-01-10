# Luanette MIDI API Design Document: Updated MVP with Custom Tools Integration

*Version 1.1*  
*Date: December 7, 2025*  
*Authors: Exa Research Assistant (in collaboration with user)*  
*Purpose*: Updated blueprint for the native Lua MIDI API in Luanette, incorporating custom tools: in-house ABC crate, MCP (MIDI Control Protocol?) utilities, Beat This! model for beat/bar detection, and rustysynth-based SF2 rendering. This refines the MVP to leverage existing infrastructure, reducing custom dev needs. Focus remains on the FF MIDI workflow (load → analyze/extract → slice/dedupe → metadata/ABC → render). Guessed APIs for Beat This! and rustysynth are simple structs/functions; actuals can be swapped in. Includes testing guidance with provided SF2/MIDI files.

The structure supports inline Lua scripting on high-RAM hardware, with Rust builtins for perf-critical ops. Extensibility for VST3/CLAP is preserved.

## 1. Project Overview and Goals

### Vision (Unchanged)
Luanette enables Lua-scripted music AI workflows: Load FF MIDIs/soundfonts → Extract drums/bass with Beat This! → Slice stems accurately → Dedupe → Generate metadata → Export ABC (via custom crate) → Render WAVs via rustysynth. This sets up libraries for AI remixing, variation gen (e.g., via LLM prompts), and future VST layering (e.g., SurgeXT presets in parallel).

### MVP Scope (Updated)
- **Must-Haves** (Leveraging Customs):
  1. Load/save SMF via `midly`; integrate custom MCP for enhanced control (e.g., event filtering).
  2. Parse to Lua tables (events as `{type, channel, pitch, time, vel}`).
  3. Analysis: Channel extraction + Beat This! for precise beat/bar boundaries (replaces heuristic slicing).
  4. Extraction: Drums (Ch. 10), bass (low-pitch heuristic, refined by Beat This! tempo).
  5. Slicing: Use Beat This! outputs for 4/8/16-bar stems; dedupe via hashing.
  6. Metadata: Auto-gen from Beat This! (e.g., {bpm, beats_per_bar, downbeats}).
  7. Export: Custom ABC crate for notation; rustysynth for SF2→WAV rendering.
  8. Batch: Process 100+ FF files, with Lua progress hooks.
- **Nice-to-Haves** (Updated):
  - MCP extensions for real-time-ish scripting (e.g., live event injection).
  - Fine-grained rustysynth params in Lua (e.g., per-stem volume, reverb).
  - Beat This! confidence thresholds for stem validation.
- **Non-Goals** (Unchanged): MIDI 2.0, full real-time I/O, large-sample manip.

### Constraints and Assumptions (Updated)
- Customs: Assume ABC crate exposes `abc::from_midi(events) -> String`; MCP as `mcp::filter_track(track, rules) -> Track`. Beat This! and rustysynth integrated via Rust APIs (guessed below).
- Hardware: High-RAM → Batch rendering fine; embed fine-grained Lua interfaces for snappiness.
- Testing: Use provided SF2 (e.g., FF4.sf2) and MIDIs (e.g., ff1.mid) for verification. Include sample scripts to validate end-to-end.

## 2. API Design Philosophy (Updated)

### Guiding Principles (Refined)
- **Lua-Natural + Custom Glue**: Idiomatic tables/methods (e.g., `seq:analyzeWithBeatThis():slice(8)`). Embed customs as seamless builtins (e.g., `beatthis.analyze(seq)` returns beat grid).
- **1:1 Midly + Enhancements**: Core from `midly`; layer Beat This! for accuracy, rustysynth for rendering, ABC/MCP for I/O.
- **Structured/Explicit**: Event lists primary; customs add optional smarts (e.g., Beat This! beat positions as explicit table).
- **Inline Builtins**: Rust-wrapped customs for perf (e.g., `rustysynth.render_batch(stems, sf2_path)`).
- **Error Handling**: `nil, err` pattern; customs propagate (e.g., low Beat This! confidence → warning metadata).

### Data Structures (Updated)
- **Sequence**: `{header: {format, tracks, ticks}, tracks: [Track], tempo=120, timeSig={4,4}, beats: [BeatThisOutput]}` (adds beat grid).
- **Track/Stem**: As before; add `beat_positions: [float]` from Beat This! for slicing.
- **BeatThisOutput**: `{tempo: 120.0, beats: [{bar=1, beat=1, position_ticks=0, confidence=0.95}], time_sig: {4,4}}`.
- **RustysynthHandle**: `{path: 'ff4.sf2', channels: 16, sample_rate: 44100}`.
- **Metadata**: `{bpm=120, key='C', bars=8, beats=from Beat This!, unique_id='hash', render_info={duration_sec=10.2}}`.

### Core API Surface (Updated)
`local midi = require('luanette.midi')`; submodules: `abc`, `mcp`, `beatthis`, `rustysynth`.

#### Loading/Saving (Enhanced with MCP)
- `midi.load(file_path) -> seq, err`: Via `midly` + optional MCP parse (e.g., custom SysEx handling for FF).
- `mcp.filter(seq, rules_table) -> seq`: E.g., `{channels={10}, min_velocity=60}` for pre-analysis cleanup.
- `midi.save(seq, file_path)`: As before.
- `rustysynth.load(sf2_path) -> handle, err`: Prep SF2 (e.g., FF4).

#### Manipulation/Analysis (With Beat This!)
- `seq:analyzeWithBeatThis() -> beat_output`: Calls guessed `beatthis::analyze(&events) -> BeatGrid`.
- `seq:extractDrums() -> track`: Ch. 10 filter + MCP if needed.
- `seq:guessBass(pitch_thresh=48) -> track`: Heuristic + Beat This! tempo context.
- `seq:slice(bar_length=8, beat_grid) -> stems`: Use `beat_grid` for precise cuts (e.g., from `bar X to X+8`).
- `stems:dedupe(min_sim=0.8) -> unique_stems`: Hash events normalized to beats.
- `seq:generateMetadata(beat_output) -> metadata`: Incorporates beats, confidence.

#### Export/Render (With Customs)
- `abc.fromEvents(events, options) -> abc_str`: Custom crate; options `{key='C', tempo=120}`.
- `seq:toABC() -> str`: Wrapper: `abc.fromEvents(flatten(seq.tracks), {use_beats=true})`.
- `rustysynth.render(stem, handle, options) -> wav_path or bytes`: Guessed API; options `{volume=1.0, reverb=0.5}`.
- `rustysynth.batchRender(stems_array, handle, output_dir) -> files_array`: Parallel on high-RAM.
- `midi.batchProcess(files, sf2_path, callback) -> results`: Integrates all (load → Beat This! → slice → ABC → render).

#### Utilities (Updated)
- `beatthis.eventsToBeats(events) -> grid`: Fine-grained embedding.
- `mcp.eventsToString(events, format='debug')`: For LLM prompts.

### Guessed Custom APIs (For Prototyping)
- **Beat This!**: Simple model wrapper.
  ```rust
  pub struct BeatGrid { pub tempo: f32, pub beats: Vec<Beat>, pub confidence: f32 }
  pub struct Beat { pub bar: u32, pub beat: u32, pub position_ticks: u32, pub strength: f32 }
  pub fn analyze(events: &[midly::Event]) -> Result<BeatGrid, Error> { /* Model inference */ }
  ```
  - Lua Exposure: `beatthis.analyze(seq) -> {tempo, beats=[{bar, beat, position_ticks, strength}], confidence}`.

- **Rustysynth**: SF2 renderer based on rustysynth.
  ```rust
  pub struct Synth { pub sf2_path: String, pub sample_rate: u32 }
  impl Synth {
      pub fn new(sf2_path: &str) -> Result<Self> {}
      pub fn render(&self, events: &[midly::Event], options: RenderOptions) -> Result<Vec<f32>, Error> { /* PCM output */ }
  }
  pub struct RenderOptions { pub volume: f32, pub duration_sec: f32, pub channels: u16 }
  ```
  - Lua: `handle:render(stem.events, {volume=0.8}) -> pcm_table` (then to WAV via `hound`).

- **ABC Crate**: Assumed `pub fn from_midi(events: &[Event], meta: &Meta) -> String { /* Notation string */ }`.
- **MCP Tools**: `pub fn filter(events: &mut [Event], rules: &FilterRules) { /* Channel/pitch/vel filters */ }`; rules as HashMap.

## 3. Implementation Roadmap (Updated)

### Phase 1: Core MIDI + Custom Integration (1 Week)
- **Rust Setup**: Add customs to Cargo: Custom `abc = {path="crates/abc"}`, `mcp = {path="crates/mcp"}`, `beat-this = {path="..."}`, `rustysynth = "0.2"`.
- **Bindings**: `mlua` for all; expose structs as tables.
  - Example Rust Snippet (Beat This! Integration):
    ```rust
    use mlua::Lua;
    use beat_this::analyze; // Custom

    fn bind_beatthis(lua: &Lua) -> mlua::Result<()> {
        let table = lua.create_table()?;
        table.set("analyze", lua.create_function(|_, events: LuaTable| -> mlua::Result<LuaTable> {
            let midly_events: Vec<midly::Event> = convert_lua_events(events)?; // Custom converter
            let grid = analyze(&midly_events)?;
            // To Lua: {tempo=grid.tempo, beats=grid.beats.iter().map(to_table).collect()}
            Ok(grid_to_table(lua, &grid))
        })?)?;
        lua.globals().set("beatthis", table)?;
        Ok(())
    }
    ```
- **MCP Glue**: Wrapper for `midly::Track` filtering.

### Phase 2: Analysis and Processing (1 Week)
- **Beat This! Slicing**: Use `BeatGrid` positions for `slice()`: Accumulate events between `beat.position_ticks`.
- **Extraction**: Drums via MCP channel filter; bass heuristic post-Beat This! (align to downbeats).
- **Dedupe**: Hash normalized to beat positions (e.g., pitch/duration relative to grid).
- **ABC**: Direct crate call in `toABC()`; test with FF melodies.

### Phase 3: Rendering and Batch (1 Week)
- **Rustysynth**: Embed `Synth::render()` as builtin; batch with `rayon` for 100+ stems.
  - Fine-Grained Lua: `handle:renderAsync(stem, options, callback)` for parallel.
- **Batch**: `batchProcess` chains: load → MCP filter → Beat This! → extract/slice → metadata → ABC write → rustysynth render.

### Phase 4: Testing and Extensibility (Ongoing)
- **Verification**: Use provided files (e.g., FF4.sf2, ff1.mid–ff5.mid). Test: Load → Analyze beats (check BPM accuracy) → Extract drums → Slice 8 bars → Render WAV (validate audio length) → ABC (visualize in tool like EasyABC).
- **Tests**:
  - Unit: `assert_eq!(beatthis.analyze(test_events).tempo, 120.0)`.
  - Integration: Full Lua script on 3-5 provided MIDIs; compare stem WAVs to originals.
  - Edge: Low-confidence beats → fallback to midly tempo.
- **VST Prep**: Stubs like `stem:prepareForVST({preset='surge_xt_ff.json'})` (placeholder audio gen).
- **Custom Gaps**: If Beat This! needs fine-tuning for FF (orchestral), add Lua hooks for model params.

## 4. Research and Dependencies (Updated)

### Key Crates (Incorporating Customs)
| Crate | Purpose | Version/Status | Notes |
|-------|---------|----------------|-------|
| midly | SMF I/O | 0.6.2 | Core parsing; MCP extends for FF SysEx. |
| mlua | Bindings | 0.9.3 | For all customs. |
| hound | WAV | 3.5.0 | Post-rustysynth PCM to file. |
| abc (custom) | MIDI→ABC | Internal | Assumes simple event-to-notation; test FF chord support. |
| mcp (custom) | Event filtering | Internal | Channel/velocity rules; integrate with midly tracks. |
| beat-this (custom) | Beat/bar detection | Internal | Model-based; guessed API above. Handles variable tempo. |
| rustysynth | SF2 rendering | 0.2+ (based on rustysynth) | Lightweight synth; FF4.sf2 load/render. Add reverb via options. |
| seahash | Dedupe hashing | 4.1.0 | Normalize to beats. |
| rayon | Batch parallel | 1.10 | For rendering 100+ stems. |

- **Custom Research Notes**:
  - **ABC**: Since custom, ensure Lua exposure handles options (e.g., `abc.render_svg(str)` for viz if extended).
  - **MCP**: Ideal for FF quirks (e.g., custom instruments); expose rules as Lua tables.
  - **Beat This!**: Guessed as DNN model; if it outputs SMF-compatible meta-events, merge into `midly` tracks.
  - **Rustysynth**: Based on rustysynth crate; supports multi-channel, real-time render. For FF4, test polyphony (orchestral layers).
- **Performance**: Customs + high-RAM → <5s per FF file batch; parallelize renders.

### Workflow Example: Updated FF Processing Script
```lua
local midi = require('luanette.midi')
local beatthis = require('luanette.beatthis')  -- Submodule
local abc = require('luanette.abc')
local rustysynth = require('luanette.rustysynth')

-- Provided test files
local files = {'ff1.mid', 'ff2.mid'}  -- Expand to 100+
local sf2_path = 'ff4.sf2'
local sf = rustysynth.load(sf2_path)

local all_drums, all_bass = {}, {}
for _, file in ipairs(files) do
    local seq, err = midi.load(file)
    if not seq then goto continue end
    
    -- MCP cleanup (e.g., filter low-vel noise)
    seq = require('luanette.mcp').filter(seq, {min_velocity=40})
    
    -- Beat detection
    local beat_output = beatthis.analyze(seq)
    seq.beats = beat_output.beats  -- Attach for slicing
    
    -- Extract with context
    local drums = seq:extractDrums()  -- Ch. 10 + MCP
    local bass = seq:guessBass(48, beat_output.tempo)
    
    -- Precise slicing
    local drum_stems = drums:slice(8, beat_output)  -- Uses beat positions
    local bass_stems = bass:slice(8, beat_output)
    
    -- Dedupe
    drum_stems = drum_stems:dedupe(0.8)
    
    for _, stem in ipairs(drum_stems) do
        -- Metadata with beats
        stem.metadata = stem:generateMetadata(beat_output)  -- Includes confidence, downbeats
        
        -- ABC export
        local abc_str = abc.fromEvents(stem.events, {tempo=beat_output.tempo, key='C'})
        local abc_file = 'drum_' .. stem.metadata.unique_id .. '.abc'
        -- io.write(abc_file, abc_str)
        
        -- Render
        local wav_path = 'drum_' .. stem.metadata.unique_id .. '.wav'
        rustysynth.render(stem, sf, {volume=0.9, reverb=0.3}, wav_path)
        
        table.insert(all_drums, {path=wav_path, meta=stem.metadata})
    end
    
    ::continue::
end

print("Processed " .. #all_drums .. " drum stems. Ready for VST layering.")
```

## 5. Sample Code and Prototypes (Updated)

### Rust Binding Prototype (Custom Integration)
```rust
use mlua::prelude::*;
use midly::Smf;
use crate::beat_this::BeatGrid;  // Custom
use crate::rustysynth::Synth;

#[mlua(module)]
fn luanette_midi(lua: &Lua) -> Result<LuaTable> {
    let table = lua.create_table()?;
    table.set("load", lua.create_async_function(|_, path: String| async move {
        let bytes = tokio::fs::read(path).await?;  // For batch
        let smf = Smf::parse(&bytes)?;
        Ok(convert_smf_to_seq(lua, &smf)?)
    })?)?;
    // Similar for save, extractDrums (Ch. 10 filter)
    Ok(table)
}

fn bind_beatthis(lua: &Lua, globals: &LuaTable) -> Result<()> {
    let bt = lua.create_table()?;
    bt.set("analyze", lua.create_function(|_, seq: LuaTable| {
        let events = extract_events_from_seq(&seq)?;  // Flatten tracks
        let grid = beat_this::analyze(&events)?;
        Ok(beat_grid_to_lua(lua, &grid)?)
    })?)?;
    globals.set("beatthis", bt)?;
    Ok(())
}

// For rustysynth
fn bind_rustysynth(lua: &Lua) -> Result<()> {
    // Similar: load -> Synth, render -> PCM table
    Ok(())
}

fn convert_smf_to_seq<'lua>(lua: &'lua Lua, smf: &Smf) -> Result<LuaTable<'lua>> {
    let seq = lua.create_table()?;
    // Header, tracks as event tables
    // Attach MCP if applied
    Ok(seq)
}
```

### Lua Fine-Grained Example (Beat This! + Render)
```lua
local beat_output = beatthis.analyze(seq)
local downbeats = {}  -- Extract for slicing
for _, beat in ipairs(beat_output.beats) do
    if beat.strength > 0.8 then table.insert(downbeats, beat.position_ticks) end
end

-- Render option: Per-stem tweaks
local options = {volume=1.0, pan=0.0, reverb=0.4}  -- Fine-grained
rustysynth.render(stem, sf, options)
```

## 6. Open Questions and Next Steps
- **Custom APIs**: Confirm guessed Beat This!/rustysynth interfaces (e.g., does Beat This! handle multi-track? Rustysynth poly limits for FF?).
- **Provided Files**: Share 3-5 MIDIs/SF2s for initial tests; any FF-specific quirks (e.g., SysEx instruments via MCP)?
- **ABC/MCP Details**: What options does ABC support (e.g., chord symbols)? MCP rules syntax?
- **AI Hooks**: Embed LLM calls post-metadata (e.g., `llm.generate_variation(stem.meta)` for VST presets)?
- **Testing Focus**: Prioritize beat accuracy on FF tracks; render fidelity (e.g., compare WAV spectrograms).

This updated doc integrates your customs for a tighter MVP. Coding agent: Prototype Phase 1 with provided files, starting with `load + beatthis.analyze + rustysynth.render` on a single FF MIDI. Let's iterate!


