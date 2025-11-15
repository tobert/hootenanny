# HalfRemembered MCP: Conceptual Domain Model

## Overview

This document explores the conceptual foundations of the HalfRemembered MCP music system. We're designing a collaborative human-AI music ensemble that requires clear, expressive representations of musical concepts. Each concept here is examined from first principles, considering both the mathematical precision needed for computation and the semantic richness required for meaningful musical expression.

---

## Core Concepts

### `Note`

**Conceptual Definition:** A `Note` represents the atomic unit of musical expression - a single sonic event with defined pitch, loudness, duration, and timbre characteristics. It exists as both a theoretical musical concept (the idea of "middle C") and a temporal event (this specific middle C played at this moment). This duality is fundamental to how we think about music - notes are both abstract symbols on a page and concrete vibrations in time.

**Essential Attributes:**
* **Pitch:** The frequency identity, likely represented as both MIDI note number (0-127) and frequency in Hz for precision
* **Velocity:** The attack intensity/loudness (0-127 in MIDI terms), representing the force or energy of onset
* **Duration:** The length of the sustain, possibly in both musical time (quarter note) and absolute time (milliseconds)
* **Channel/Voice:** Which instrument voice plays this note (for polyphonic separation)
* **Articulation:** How the note begins and ends (staccato, legato, accent, etc.)
* **Micro-timing:** Subtle timing deviations from the grid for humanization

**Relationships:**
* Building block of `Chord`s, `Melody`s, and `Pattern`s
* Rendered by an `Instrument` 
* Positioned on a `Timeline` through an `Event`
* Constrained by `Scale` and `Key` contexts

**Open Questions & Considerations:**
* Should we model a note as an instantaneous trigger with a duration property, or as a span with explicit start/stop events? The trigger model is simpler but makes it harder to model overlapping notes and real-time modifications. The span model is more complex but better represents the reality of sustained sounds.
* How do we handle pitch bend and modulation that occur during a note's lifetime? Are these properties of the Note itself or separate Events?
* Should velocity be normalized (0.0-1.0) or use MIDI conventions (0-127)? Floating point gives us more expression but MIDI is the industry standard.
* Do we need a separate concept for "rests" (silence), or is that just the absence of notes?

---

### `Chord`

**Conceptual Definition:** A `Chord` is a vertical stack of simultaneous `Note`s that creates harmonic meaning. Unlike a mere collection of notes, a chord has musical identity - it can be major, minor, diminished, augmented, or extended. It represents both a theoretical harmonic concept ("C major") and a specific voicing (which exact notes in which octaves).

**Essential Attributes:**
* **Root:** The fundamental pitch that names the chord
* **Quality:** The harmonic character (major, minor, diminished, augmented, suspended, etc.)
* **Intervals:** The specific intervals from the root that define the chord type
* **Voicing:** The specific arrangement of notes (which octave each note appears in)
* **Inversion:** Which chord tone is in the bass
* **Extensions:** Additional color tones (7th, 9th, 11th, 13th)

**Relationships:**
* Composed of multiple `Note`s played simultaneously
* Can be part of a `Pattern` or chord progression
* Exists within a `Key` context that gives it harmonic function
* Played by one or more `Instrument`s (could be split across tracks)

**Open Questions & Considerations:**
* Should we store chords as abstract harmonic concepts that get realized into specific notes, or as concrete collections of notes? The abstract approach is more flexible for transposition and reharmonization, but the concrete approach is simpler.
* How do we handle guitar-specific concepts like barre chords or open strings?
* Should arpeggiated chords (broken chords) be a type of `Chord` or a `Pattern`?
* How do we model voice leading between chords?

---

### `Melody`

**Conceptual Definition:** A `Melody` is a horizontal sequence of `Note`s that creates a musical line or phrase. It's characterized not just by its pitches but by its contour, rhythm, and phrasing. A melody has intentionality - it moves toward goals, creates expectations, and resolves tensions. It's the part of music that can be hummed or sung.

**Essential Attributes:**
* **Phrase Structure:** How the melody divides into musical sentences
* **Contour:** The shape of pitch movement over time
* **Rhythm Pattern:** The durational relationships between notes
* **Range:** The span from lowest to highest pitch
* **Motifs:** Recurring small patterns that give the melody identity
* **Dynamics:** How loudness changes across the phrase

**Relationships:**
* Composed of a sequence of `Note`s or rests
* Often follows or implies a `Scale` or mode
* Can be harmonized by `Chord`s
* Performed by a single `Instrument` (typically)
* Can contain or reference smaller `Pattern`s

**Open Questions & Considerations:**
* Should melodies know about their harmonic context, or should harmony be layered on separately?
* How do we represent melodic transformations (inversion, retrograde, augmentation)?
* Should we distinguish between "foreground" melodies and "background" countermelodies?
* How do we handle melodic ornamentation (trills, turns, grace notes)?

---

### `Pattern`

**Conceptual Definition:** A `Pattern` is a reusable musical fragment that can be transformed and combined to build larger structures. It's the musical equivalent of a function or module - a piece of musical logic that can be parameterized and applied in different contexts. Patterns can be rhythmic (drum patterns), melodic (riffs), harmonic (chord progressions), or composite (complete phrases).

**Essential Attributes:**
* **Content:** The sequence of musical elements (notes, chords, rests)
* **Length:** Duration in musical time (bars, beats)
* **Transformation Rules:** How it can be modified (transposable, scalable, invertible)
* **Metadata:** Name, tags, stylistic associations
* **Parameters:** Variable aspects that can be controlled (velocity scaling, timing shuffle)
* **Loop Properties:** Whether and how it repeats

**Relationships:**
* Can contain `Note`s, `Chord`s, or other `Pattern`s (recursive)
* Positioned on `Track`s via `Event`s
* Can be constrained or transformed by `Scale`/`Key`
* Associated with specific `Instrument`s or instrument-agnostic

**Open Questions & Considerations:**
* Should patterns be purely data or should they include behavior/generation logic?
* How do we handle pattern variations - are they separate patterns or parameters of one pattern?
* Can patterns be probabilistic (e.g., "70% chance of playing this note")?
* Should we support pattern algebra (combining patterns with operations like AND, OR, XOR)?
* How do we handle patterns that span multiple tracks (like call-and-response)?

---

### `Event`

**Conceptual Definition:** An `Event` is anything that happens at a specific point in musical time. While `Note`s are the most common events, the concept encompasses any change in the musical state - parameter automation, tempo changes, cue markers, or even semantic annotations. Events are the bridge between abstract musical concepts and their temporal realization.

**Essential Attributes:**
* **Timestamp:** When the event occurs (absolute time and/or musical time)
* **Type:** The kind of event (note on/off, control change, marker, etc.)
* **Payload:** The type-specific data (which note, what value, etc.)
* **Duration:** For events that span time rather than being instantaneous
* **Priority:** For handling simultaneous events
* **Source:** What generated this event (human, AI, pattern playback)

**Relationships:**
* Contained within `Track`s
* Can reference `Note`s, `Pattern`s, or automation data
* Positioned on a `Timeline`
* May target specific `Instrument` parameters

**Open Questions & Considerations:**
* Should events use absolute time (milliseconds) or musical time (bars:beats:ticks)?
* How do we handle event conflicts (two events trying to play the same note)?
* Should events be immutable once created, or can they be modified during playback?
* Do we need a separate event type for "meta" events (tempo changes, time signature)?
* How do we handle continuous events (like smooth parameter automation curves)?

---

### `Track`

**Conceptual Definition:** A `Track` is a container for a single stream of musical performance, typically associated with one instrument or voice. It provides both organizational structure (grouping related events) and mixing capabilities (volume, pan, effects). A track is like a single performer in an ensemble - it has its own part to play but coordinates with others to create the whole.

**Essential Attributes:**
* **Event Sequence:** Ordered list of `Event`s
* **Instrument Assignment:** Which `Instrument` renders this track
* **Mix Parameters:** Volume, pan, mute, solo states
* **Processing Chain:** Effects and transformations applied to the track
* **Playback State:** Current position, loop points, armed for recording
* **Metadata:** Name, color, comments, version history

**Relationships:**
* Contains multiple `Event`s in temporal order
* Associated with one primary `Instrument`
* Exists within a `Timeline`/`Sequence`
* Can reference shared `Pattern`s
* May have parent/child relationships with other tracks (for grouping)

**Open Questions & Considerations:**
* Should tracks be monophonic (one note at a time) or polyphonic?
* How do we handle track groups or buses for submixing?
* Can a track change instruments mid-performance?
* Should tracks own their events or reference them from a shared pool?
* How do we implement track freezing/bouncing for performance optimization?
* Do we need different track types (MIDI, audio, automation, marker)?

---

### `Timeline` / `Sequence`

**Conceptual Definition:** A `Timeline` is the master temporal container that orchestrates all `Track`s into a complete musical performance. It defines the global time grid, tempo map, and structural markers. The Timeline is both a coordinate system (where everything is positioned) and a playback engine (determining what happens when).

**Essential Attributes:**
* **Track Collection:** All `Track`s in the arrangement
* **Tempo Map:** BPM changes over time
* **Time Signature Map:** Meter changes over time
* **Markers:** Section labels, cue points, loop regions
* **Global Length:** Total duration of the sequence
* **Playback Position:** Current time cursor
* **Grid Settings:** Quantization and snap values

**Relationships:**
* Contains multiple `Track`s
* Provides temporal context for all `Event`s
* May reference a global `Key` or `Scale` context
* Interfaces with the playback/rendering system

**Open Questions & Considerations:**
* Should we support multiple simultaneous timelines (for A/B comparisons)?
* How do we handle tempo-synced vs. free-time sections?
* Should the timeline be linear or can it have non-linear structures (jumps, repeats)?
* How do we implement arrangement markers (verse, chorus, bridge)?
* Do we need a separate concept for "scenes" (horizontal slices of the timeline)?

---

### `Instrument`

**Conceptual Definition:** An `Instrument` is an abstract sound source that transforms musical instructions (`Note`s, `Event`s) into audio. It encapsulates both the timbral identity (what it sounds like) and the performance capabilities (how it responds to input). An Instrument might represent a physical model, a sample player, a synthesizer, or an external MIDI device.

**Essential Attributes:**
* **Voice Architecture:** Monophonic, polyphonic, or multitimbral capabilities
* **Parameter Set:** All controllable aspects (oscillators, filters, envelopes, effects)
* **Preset/Patch:** Current configuration of all parameters
* **Polyphony Limit:** Maximum simultaneous voices
* **MIDI Mapping:** How MIDI messages map to parameters
* **Modulation Routing:** Internal connections between parameters

**Relationships:**
* Receives `Event`s from one or more `Track`s
* Interprets `Note`s according to its voice architecture
* May be constrained by global `Scale`/`Key` settings
* Can be automated via control `Event`s

**Open Questions & Considerations:**
* Should instruments be stateful or stateless? Stateful is more realistic but harder to make deterministic.
* How do we handle multi-timbral instruments (different sounds on different MIDI channels)?
* Should we model acoustic instrument behaviors (like sympathetic resonance)?
* How do we implement instrument layering and splitting?
* Do we need different instrument types for different synthesis methods?
* How do we handle external instruments (hardware synths, VST plugins)?

---

### `Scale`

**Conceptual Definition:** A `Scale` is a collection of pitch classes that defines the tonal palette for musical creation. It's both a constraint (limiting which notes are "in key") and a generator (suggesting melodic and harmonic possibilities). Scales carry cultural and emotional associations - major scales sound "happy," minor scales sound "sad," pentatonic scales sound "Eastern" or "bluesy."

**Essential Attributes:**
* **Root Note:** The tonal center or tonic
* **Interval Pattern:** The sequence of intervals that defines the scale
* **Degree Names:** Labels for each scale degree (tonic, dominant, etc.)
* **Mode:** Which degree of the parent scale is treated as root
* **Chord Set:** The chords naturally built from the scale degrees
* **Alterations:** Chromatic modifications (like harmonic minor's raised 7th)

**Relationships:**
* Constrains `Note` choices in `Melody` and `Chord` generation
* Related to `Key` (a scale with additional harmonic context)
* Can modulate (change) over the course of a `Timeline`
* Influences `Pattern` transformations

**Open Questions & Considerations:**
* Should scales be purely theoretical or should they know about specific note mappings?
* How do we handle microtonal scales that don't fit 12-tone equal temperament?
* Should we support scale morphing/interpolation between different scales?
* How do we represent blue notes and other "outside" notes that are stylistically appropriate?
* Do we need different scale representations for different instruments (like guitar fretboard patterns)?

---

### `Key`

**Conceptual Definition:** A `Key` extends a `Scale` with harmonic context and functional relationships. While a scale is just a collection of notes, a key implies a tonal center, chord progressions, and voice-leading conventions. Being "in the key of C major" means more than using the C major scale - it means C is home, G creates tension, and F provides stability.

**Essential Attributes:**
* **Tonic:** The home pitch and chord
* **Scale Basis:** The underlying scale (major, minor, or modal)
* **Functional Harmony:** The roles of different chords (I, IV, V, etc.)
* **Cadence Patterns:** Common resolution sequences
* **Modulation Targets:** Related keys for smooth transitions
* **Borrowed Chords:** Common chromatic additions from parallel keys

**Relationships:**
* Builds upon a `Scale` with additional harmonic rules
* Influences `Chord` progressions and voice leading
* Can change over a `Timeline` (modulation)
* Affects how `Pattern`s are harmonized

**Open Questions & Considerations:**
* How do we handle modal mixture (borrowing from parallel major/minor)?
* Should key changes be events or properties of timeline regions?
* How do we represent pivot chords that belong to multiple keys?
* Do we need to distinguish between closely related and distantly related keys?
* How do we handle polytonal music (multiple keys simultaneously)?

---

## System-Wide Considerations

### Time Representation
The system needs to support both:
- **Musical Time:** Bars, beats, and subdivisions that follow tempo changes
- **Absolute Time:** Milliseconds for sample-accurate playback
- **Relative Time:** Offsets and durations independent of tempo

### Transformation Pipeline
Musical data should flow through transformations:
1. **Generation:** Creating raw musical material
2. **Constraint:** Applying scale/key limitations
3. **Humanization:** Adding timing and velocity variations
4. **Arrangement:** Structural organization
5. **Performance:** Real-time modifications

### Collaboration Model
Since this is a human-AI ensemble:
- Every musical decision should be attributable (who/what made it)
- All transformations should be reversible or at least traceable
- The system should support both real-time and step-time interaction
- There should be clear boundaries between human authority and AI suggestion

### Type Safety Goals
Our Rust implementation should:
- Make invalid musical states unrepresentable
- Use the type system to enforce musical rules
- Provide zero-cost abstractions for musical concepts
- Enable compile-time verification of musical constraints

---

## Next Steps

This conceptual model provides the foundation for implementing the HalfRemembered MCP. The next phase will involve:

1. Translating these concepts into Rust traits and structures
2. Defining the serialization format for persistence
3. Implementing the transformation pipeline
4. Building the MCP server interface
5. Creating example patterns and compositions

Each concept here should map to a well-defined Rust type that captures both its data and its behavior, creating a system that is both musically expressive and computationally efficient.

