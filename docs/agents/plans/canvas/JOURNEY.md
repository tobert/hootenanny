# The Canvas Journey: From Audio Buffers to Latent Spaces

A design conversation between human and Claude, December 2024.

---

## Where We Started

We began debugging audio playback. The tick loop ran at 1ms but produced 5.33ms of audio per tick, causing 5x speed playback with corruption. A simple timing bug.

But fixing it led us to question the architecture: **tick() shouldn't be in the audio data path**. This sparked a redesign of chaosgarden's RT mixing.

## The Two-Layer Architecture

We designed a clean separation:

```
CONTROL PLANE (tick, async)     DATA PLANE (RT callback)
├── Transport position          ├── Ring buffer reads
├── Region management           ├── Mixing (just adds)
└── Parameter updates           └── Output to PipeWire
    via atomics ─────────────────▶
```

The RT callback became trivial: read from ring, mix, output. No syscalls, no allocations. Indestructible.

## The Canvas Emerges

We asked: what if we had a massive pre-render buffer? An mmap'd "performance canvas" where:
- Lookahead thread writes ahead of playback
- RT callback reads from pre-rendered content
- Multiple playheads could observe different realizations

This led to pages, lanes, frames, refs. An elaborate architecture.

## The Rethinking

Then we questioned: is this audio-centric view right? What if canvas isn't about samples but about **encodings in latent spaces**?

```
Intent (text)  →  Orpheus tokens  →  MIDI  →  Audio PCM
    ↓                   ↓              ↓          ↓
 (latent)           (latent)      (latent)   (latent!)
```

Even audio samples are latent - they only become music when decoded by a speaker. Your ear is an encoder; your auditory cortex runs inference.

## The Honest Realization

We already have the pieces:
- **CAS** = content storage (encodings as blobs)
- **Artifacts** = metadata, lineage, tags
- **Tools** = transformations between spaces (orpheus, clap, sf2 render)
- **Graph + Trustfall** = relationships and queries
- **Lua** = orchestration
- **Chaosgarden** = the final decode (samples → speaker → air → ear)

A Lua script calling tools IS a forward pass through a computational graph. The "model" is distributed across services, but it's still: input → transformations → output.

## What's Missing?

Not infrastructure. **Framing**.

Our tools feel like a bag of utilities. They should feel like **sampling from a unified model** where:
- Every piece of content lives in some latent space
- Transformations are projections between spaces
- Non-determinism is a feature (temperature, not bugs)
- The "performance" is a unique decode each time

---

## Ideas for Model-Native Tools

### 1. Unified Encoding Vocabulary

Rename things to align with how models think:

| Current | Model-Native |
|---------|--------------|
| `content_hash` | `encoding` |
| `artifact` | `latent` or `representation` |
| `convert_midi_to_wav` | `project(midi_space → audio_space)` |
| `orpheus_generate` | `sample(prior, temperature)` |
| `create_region` | `schedule_decode(encoding, time)` |

### 2. Space-Aware API

```lua
-- Current
local midi = orpheus_generate({ prompt = "jazz" })
local audio = convert_midi_to_wav({ input = midi })

-- Model-native
local encoding = sample("jazz", { space = "orpheus" })
local projected = project(encoding, "audio", { via = "sf2" })
schedule(projected, { at = beat(0) })
```

The API makes spaces explicit. You're moving through latent spaces.

### 3. Inference Context

Every operation happens in a context that carries:
- Temperature (variation amount)
- Seed (for reproducibility when wanted)
- History (what came before, for coherence)
- Style conditioning (persistent aesthetic)

```lua
local ctx = context({
    temperature = 0.8,
    style = embed("warm analog jazz"),
})

-- All operations inherit context
local a = sample("intro", ctx)
local b = continue(a, ctx)  -- coherent with a
```

### 4. Forward Pass as Primitive

The core operation is `forward()`:

```lua
-- A performance is a forward pass
local performance = forward({
    inputs = { intent = "mysterious piano" },
    steps = { "orpheus", "sf2", "speaker" },
    context = ctx,
})
```

Each `forward()` produces a unique realization. Same inputs, different outputs (unless seeded).

### 5. Attention as Playhead

Rename playhead to **attention**. It's not reading a tape - it's attending to latent content, collapsing possibilities into observations:

```lua
local attention = attend(canvas, { position = beat(0) })
local observed = attention:forward()  -- realize what's here
attention:advance(beats(32))
```

Multiple attentions = multiple observers = different realizations.

### 6. Everything is Encode/Decode

Even "playing audio" is a decode:

```
Speaker = DAC.decode(samples) → air pressure waves
Ear = Encoder(air) → neural spikes → auditory latent
Brain = Inference(auditory latent) → experience of music
```

Our system mirrors this:
```
sample(intent) → project(orpheus) → project(audio) → schedule(speaker)
```

The speaker is just another projection. Chaosgarden is the "decode head."

### 7. Graph as Model Architecture

The connections between services ARE the model architecture:

```
Text ──→ CLAP ──→ Conditioning
              ↘
               Orpheus ──→ MIDI ──→ SF2 ──→ Audio
              ↗                          ↘
         BeatThis ←── Analysis ←───────── Chaosgarden
```

We're not calling tools. We're routing activations through a distributed model.

---

## The Lingo Shift

| Old Thinking | New Thinking |
|--------------|--------------|
| Files and formats | Encodings in latent spaces |
| Tools and APIs | Projections and samplers |
| Playback | Decode/observation |
| Recording | Encode/embed |
| Timeline | Scheduled attention |
| Arrangement | Computation graph |
| Performance | Forward pass |
| Variation | Temperature |
| "Make music" | "Sample from the prior" |

---

## What This Enables

When we think in model terms:

1. **Natural agentic workflow** - Claude already thinks in embeddings, attention, sampling. The API matches the mental model.

2. **Principled non-determinism** - Temperature isn't a hack, it's the point. Each forward pass is unique.

3. **Composable representations** - Move between spaces freely. MIDI and audio aren't different things, just different projections.

4. **Coherent long-form** - Context carries through. The model "remembers" style, history, intent.

5. **Multi-agent natural** - Multiple attentions, multiple observers, same latent content. Collaboration is built in.

---

## Next Steps

1. **Vocabulary pass** - Update tool names, parameter names, docs to use model language
2. **Context primitive** - Add inference context to tool calls
3. **Space annotations** - Tag artifacts with their encoding space
4. **Forward() helper** - Lua function that chains projections
5. **Attention API** - Rename/redesign playhead as attention

The infrastructure exists. The reframe makes it sing.

---

*"The map is not the territory, but a good map makes the territory navigable."*

Our tools work. Making them feel like a model makes them *thinkable*.
