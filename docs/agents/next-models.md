# Next Models: Real-Time & Batch Music Generation

Research compiled December 2024 for Hootenanny live performance architecture.

## Hardware Context

- **AMD Strix Halo 8060S** with 96GB unified VRAM
- **ROCm 7.9** has early Strix Halo support (as of Oct 2025)
- **Novation Launchpad Pro MK3** (64 pads) for performance control
- **Eurorack** modular as potential input source
- Live instruments: keys, trombone, potentially bass/drums/vocals

## The Vision: Blended Real-Time + Batch Performance

```
┌──────────────────────────────────────────────────────────────────────────────┐
│                        LIVE PERFORMANCE ARCHITECTURE                         │
├──────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  ┌─────────────────────────────────────────────────────────────────────────┐ │
│  │                         HUMAN PERFORMERS                                │ │
│  │  Keys ─────┐                                                            │ │
│  │  Trombone ─┼──► Audio In ──► Beat Detection ──► Clock Sync             │ │
│  │  Bass ─────┤                      │                  │                  │ │
│  │  Drums ────┤                      ▼                  ▼                  │ │
│  │  Vocals ───┘              Phrase Boundaries    Graph Tempo              │ │
│  │                                                                          │ │
│  │  Launchpad Pro ──► Commands ──► Vibe Changes / Stem Selection          │ │
│  └─────────────────────────────────────────────────────────────────────────┘ │
│                                    │                                         │
│                                    ▼                                         │
│  ┌─────────────────────────────────────────────────────────────────────────┐ │
│  │                         REAL-TIME LAYER (<20ms)                         │ │
│  │                                                                          │ │
│  │  Notochord ──► Harmonization, responsive MIDI events                    │ │
│  │       │                                                                  │ │
│  │       ▼                                                                  │ │
│  │  RAVE ──► Drones, evolving textures, timbre transfer                    │ │
│  │       │                                                                  │ │
│  │  Eurorack ──► CV/Gate ──► Unpredictable input, chaos source             │ │
│  │                                                                          │ │
│  └─────────────────────────────────────────────────────────────────────────┘ │
│                                    │                                         │
│                                    ▼                                         │
│  ┌─────────────────────────────────────────────────────────────────────────┐ │
│  │                         BATCH LAYER (seconds-minutes)                   │ │
│  │                                                                          │ │
│  │  Claude ──► "Change vibe for next 2 minutes" ──► Prompt Engineering     │ │
│  │       │                                                                  │ │
│  │       ▼                                                                  │ │
│  │  Orpheus ──► Generate stems (melody, bass, percussion variations)       │ │
│  │       │                                                                  │ │
│  │       ▼                                                                  │ │
│  │  ACE-Step / MusicGen ──► Full audio segments, bridges, transitions      │ │
│  │       │                                                                  │ │
│  │       ▼                                                                  │ │
│  │  ┌─────────────────────────────────────────────────────────────────┐    │ │
│  │  │  STEM PICKER (displayed on screen)                              │    │ │
│  │  │                                                                  │    │ │
│  │  │  [Stem A: jazzy bass]  [Stem B: driving bass]  [Stem C: sparse] │    │ │
│  │  │  [Stem D: polyrhythm]  [Stem E: 4-on-floor]    [Stem F: swing]  │    │ │
│  │  │                                                                  │    │ │
│  │  │  Tap to queue ──► Inserted at next phrase boundary              │    │ │
│  │  └─────────────────────────────────────────────────────────────────┘    │ │
│  │                                                                          │ │
│  └─────────────────────────────────────────────────────────────────────────┘ │
│                                    │                                         │
│                                    ▼                                         │
│  ┌─────────────────────────────────────────────────────────────────────────┐ │
│  │                         MIX OUTPUT                                      │ │
│  │                                                                          │ │
│  │  Real-time layer ──┬──► Mixer ──► Main Out                              │ │
│  │  Batch stems ──────┤       │                                            │ │
│  │  Live instruments ─┘       └──► Recording (artifact capture)            │ │
│  │                                                                          │ │
│  └─────────────────────────────────────────────────────────────────────────┘ │
│                                                                              │
└──────────────────────────────────────────────────────────────────────────────┘
```

## Model Landscape

### Real-Time Models (Sub-20ms Latency)

#### Notochord - MIDI Generation
- **Latency**: <10ms
- **Architecture**: RNN, probabilistic
- **Capabilities**: Harmonization, steerable generation, machine improvisation
- **Key Feature**: Sub-event level interventions (condition on partial notes)
- **Source**: [Intelligent Instruments Lab, Iceland](https://github.com/Intelligent-Instruments-Lab/notochord)
- **Paper**: [arXiv:2403.12000](https://arxiv.org/abs/2403.12000)

Perfect for: Responding to live input, adding harmonies, generating complementary MIDI events.

#### RAVE - Audio Synthesis/Transfer
- **Latency**: ~2ms possible
- **Sample Rate**: 48kHz native
- **Architecture**: VAE with multi-band PQMF decomposition
- **Key Feature**: Controllable latent space, train on custom timbres
- **Source**: [ACIDS/IRCAM](https://github.com/acids-ircam/RAVE)
- **Paper**: [arXiv:2111.05011](https://arxiv.org/abs/2111.05011)

Perfect for: Drones, evolving textures, timbre transfer from live input, neural synthesis.

#### BRAVE - Low-Latency Timbre Transfer
- **Latency**: Optimized for instrumental interaction
- **Source**: [GitHub](https://github.com/fcaspe/brave)

Perfect for: Processing live instruments through trained timbres in real-time.

#### Magenta RealTime (NEW)
- **Type**: Continuous streaming generation
- **Control**: Text/audio prompt steering
- **Weights**: Open (Magenta RealTime), API (Lyria RealTime)
- **Paper**: [arXiv:2508.04651](https://arxiv.org/html/2508.04651v1)

Perfect for: Background texture generation that evolves with prompts.

### Batch Models (Seconds to Minutes)

#### Orpheus Music Transformer
- **Parameters**: 479M
- **Context**: 8k tokens
- **Architecture**: RoPE + Flash Attention
- **Training Data**: 2.31M MIDIs (Godzilla dataset)
- **Source**: [HuggingFace](https://huggingface.co/asigalov61/Orpheus-Music-Transformer)

**We already have this integrated!** Use for generating stems, variations, continuations.

#### ACE-Step
- **Parameters**: 3.5B
- **Speed**: 4 minutes of music in 20 seconds (A100)
- **Architecture**: Diffusion + DCAE + linear transformer
- **License**: Apache 2.0
- **Training Docs**: Available
- **Source**: [GitHub](https://github.com/ace-step/ACE-Step)

Perfect for: Full audio segments, high-quality generation when you have seconds to spare.

#### MusicGen
- **Parameters**: 300M - 3.3B
- **Fine-tuning**: Well-documented LoRA
- **Source**: [Meta AudioCraft](https://github.com/facebookresearch/audiocraft)

Perfect for: Text-conditioned audio generation, style transfer.

#### YuE
- **Architecture**: LLaMA2-based
- **Capability**: Full song with lyrics
- **Source**: [GitHub](https://github.com/multimodal-art-projection/YuE)

Perfect for: When you need vocals/lyrics in generated content.

### Symbolic/MIDI Specialists

| Model | Focus | Notes |
|-------|-------|-------|
| **Text2midi** | Text→MIDI | LLM encoder + transformer decoder |
| **MuPT** | ABC notation | Explores scaling laws |
| **MIDI-GPT** | Controllable infilling | Bar-level, track-level control |
| **Anticipatory Music Transformer** | Infilling/accompaniment | Async control conditioning |

## Acceleration Techniques

### Presto! (Step + Layer Distillation)
- **Speedup**: 10-15×
- **Method**: Score-based DMD + layer distillation
- **Paper**: [arXiv:2410.05167](https://arxiv.org/abs/2410.05167)

Could make ACE-Step/MusicGen fast enough for tighter iteration.

### Speculative Decoding
- **Speedup**: 2-3×
- **Method**: Draft model predicts, main model verifies in parallel
- **Tradeoff**: No quality loss, needs separate draft model

### Multi-Token Prediction
- **Speedup**: Linear with number of heads
- **Method**: Multiple prediction heads per step
- **Tradeoff**: Requires training with multiple heads

## AMD ROCm Compatibility

| Status | Notes |
|--------|-------|
| **ROCm 7.9** | Early Strix Halo support (Oct 2025) |
| **ROCm 7.0** | Benchmarks show working on Strix Halo |
| **Vulkan/RADV** | Sometimes outperforms HIP for inference |
| **96GB Unified** | Fits models that won't fit on discrete GPUs |

**Caveat**: Audio model training on ROCm less battle-tested than CUDA. Expect friction with custom kernels.

## Latency Budget for Live Performance

| Category | Latency | Feel |
|----------|---------|------|
| Acoustic instruments | 0ms | Reference |
| Hardware synths | <1ms | Instant |
| Software synths | 3-10ms | Responsive |
| **Notochord** | <10ms | **Feels like instrument** |
| **RAVE** | ~2ms | **Transparent** |
| "Acceptable" threshold | <20ms | Playable |
| "Noticeable but usable" | 20-50ms | Slight lag |
| "Feels like lag" | >50ms | Problematic |

## Implementation Phases

### Phase 1: Real-Time Foundation
1. Integrate **Notochord** as graph identity for MIDI processing
2. Train **RAVE** on custom drone/texture sounds
3. Wire Launchpad Pro as command surface
4. Implement beat detection for phrase boundaries

### Phase 2: Batch Generation Pipeline
1. **Orpheus** stem generation with variation sets
2. Stem picker UI (could be web, could be on-device display)
3. Queue system for inserting stems at phrase boundaries
4. Vibe prompt interface (text → generation parameters)

### Phase 3: Claude Integration
1. Natural language vibe control ("darker, more tension")
2. Claude watches performance state, suggests stems
3. Auto-generation of variations based on what's working
4. Session logging for post-performance analysis

### Phase 4: Multi-Performer
1. Additional audio inputs (bass, drums, vocals)
2. Source separation for clean stems from live input
3. Per-performer RAVE instances for timbre processing
4. Collaborative stem selection

## The forward() Pass Concept

The key insight: **time flows through the system like a render pipeline**.

```python
class LivePerformance:
    def forward(self, t: Timestamp) -> AudioFrame:
        # Real-time layer: always runs
        rt_midi = self.notochord.step(self.midi_in, t)
        rt_audio = self.rave.process(self.audio_in, t)

        # Check for queued stems at phrase boundaries
        if self.is_phrase_boundary(t):
            if stem := self.stem_queue.pop():
                self.active_stems.insert(stem)

        # Mix everything
        return self.mixer.combine(
            live_instruments=self.audio_in,
            realtime_generation=rt_audio,
            midi_rendered=self.render_midi(rt_midi),
            active_stems=self.active_stems.at(t),
        )
```

Batch generation happens **off the critical path**:
- Orpheus generates stems in background
- Stems appear in picker when ready
- Human selects, stem queues for next boundary
- `forward()` inserts at musically appropriate moment

## Training Priorities

Given the 96GB VRAM and live performance focus:

### High Priority (Do First)
1. **RAVE** trained on your preferred drone/texture sounds
2. **Notochord** fine-tuned on your harmonic preferences

### Medium Priority (After basics work)
3. **Orpheus** fine-tuned on your compositional style
4. **ACE-Step** LoRA for your genre

### Exploration (When curious)
5. **Magenta RealTime** for continuous background
6. **Presto!** distillation for faster batch generation

## References

### Real-Time
- [Notochord Paper](https://arxiv.org/abs/2403.12000)
- [Notochord Homunculus (ICMC 2025)](https://iil.is/pdf/2025_icmc_notochord_homunculus.pdf)
- [RAVE Paper](https://arxiv.org/abs/2111.05011)
- [RAVE GitHub](https://github.com/acids-ircam/RAVE)
- [BRAVE GitHub](https://github.com/fcaspe/brave)
- [Live Music Models](https://arxiv.org/html/2508.04651v1)
- [Designing Neural Synths for Low-Latency](https://arxiv.org/abs/2503.11562)

### Batch Generation
- [Orpheus Music Transformer](https://huggingface.co/asigalov61/Orpheus-Music-Transformer)
- [ACE-Step](https://github.com/ace-step/ACE-Step)
- [ACE-Step Training Docs](https://github.com/ace-step/ACE-Step/blob/main/TRAIN_INSTRUCTION.md)
- [MusicGen Fine-tuning](https://huggingface.co/blog/theeseus-ai/musicgen-lora-large)
- [YuE](https://github.com/multimodal-art-projection/YuE)

### Acceleration
- [Presto! Distillation](https://arxiv.org/abs/2410.05167)
- [Speculative Decoding](https://arxiv.org/abs/2211.17192)
- [Anticipatory Music Transformer](https://arxiv.org/abs/2306.08620)

### AMD/ROCm
- [ROCm 7.9 Strix Halo](https://www.phoronix.com/news/AMD-ROCm-7.9-Strix-Halo)
- [ROCm Strix Halo Benchmarks](https://www.phoronix.com/review/amd-strix-halo-rocm-benchmarks)
- [ROCm Compatibility Matrix](https://rocm.docs.amd.com/en/latest/compatibility/compatibility-matrix.html)
