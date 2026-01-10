# Research Prompt: Launchpad Pro Integration for Live Performance

## Context

Hootenanny is a live performance system blending real-time neural audio generation with batch-prepared stems. The human performer needs a tactile interface for:
- Selecting pre-generated stems to insert at phrase boundaries
- Triggering vibe/mood changes that affect generation parameters
- Controlling real-time model parameters (Notochord, RAVE)
- Managing the chaos/order balance in the performance

## Hardware

**Novation Launchpad Pro MK3** (64 pads in 8x8 grid, plus function buttons)
- RGB LED feedback per pad
- Velocity-sensitive pads
- MIDI over USB
- Standalone and DAW modes

## Research Questions

### 1. MIDI Protocol
- What MIDI messages does Launchpad Pro send/receive?
- How do we set pad colors programmatically?
- What's the latency profile for pad-to-MIDI-to-action?
- Are there sysex messages for bulk LED updates?

### 2. Layout Design
Consider a layout that serves the live performance workflow:

```
┌─────────────────────────────────────────────────────────────┐
│  Row 1: Stem Type Selection (melody, bass, drums, pad...)  │
│  Row 2-5: Available Stems (generated, color = vibe/energy) │
│  Row 6: Queued Stems (what's coming next)                  │
│  Row 7: Real-time Controls (Notochord/RAVE params)         │
│  Row 8: Transport/Global (tempo, chaos, regenerate)        │
└─────────────────────────────────────────────────────────────┘
```

### 3. Visual Feedback
- Color coding for stem vibes (warm=orange, dark=purple, energetic=red?)
- Brightness for stem readiness (dim=generating, bright=ready)
- Animation for active/playing stems
- Flash for phrase boundary approaching

### 4. Integration Points
- How does this connect to the Hootenanny graph?
- Is this a dedicated identity in the audio graph?
- Should it speak OSC, MIDI, or direct MCP calls?
- What state does it need to display? (stems available, queue, active)

### 5. Rust Crates to Evaluate
- `midir` - cross-platform MIDI I/O
- `midly` - MIDI parsing
- `launchpad` - if one exists for Pro MK3

## Deliverables

1. **Protocol Documentation**: How to talk to Launchpad Pro from Rust
2. **Layout Proposal**: Mapping of pads to functions with rationale
3. **State Machine**: What states can the controller be in, transitions
4. **Integration Design**: How it fits into the Hootenanny architecture
5. **Prototype**: Basic Rust code that lights up pads based on stem state

## References

- [Novation Launchpad Pro Programmer's Reference](https://resource.novationmusic.com/support/product-downloads?product=Launchpad+Pro)
- [midir crate](https://crates.io/crates/midir)
- docs/agents/next-models.md (live performance architecture)

## Notes

This should be relatively straightforward - it's essentially a MIDI controller adapter that:
1. Receives pad presses → translates to commands
2. Receives state updates → translates to LED colors

The complexity is in the UX design, not the implementation.
