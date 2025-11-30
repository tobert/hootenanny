# ABC Notation Implementation Plan

## Overview

Add ABC notation support to hootenanny via a new `crates/abc/` crate. ABC is a compact text-based music notation ideal for LLM generation - a complete melody fits in fewer tokens than prose describing it.

**Design principles:**
- Generous parser that degrades gracefully with feedback
- AST captures full ABC semantics; MIDI output starts simple
- Multi-voice support planned in AST, parsing deferred
- Integrate with existing CAS/Artifact infrastructure

---

## Part 1: Crate Structure

```
crates/abc/
├── Cargo.toml
├── src/
│   ├── lib.rs           # Public API: parse(), to_midi(), validate(), transpose()
│   ├── ast.rs           # AST types
│   ├── parser/
│   │   ├── mod.rs       # Top-level parse function
│   │   ├── header.rs    # X:, T:, K:, M:, L:, Q: fields
│   │   ├── body.rs      # Music code parsing
│   │   ├── note.rs      # Note, chord, rest parsing
│   │   └── key.rs       # Key signature parsing
│   ├── midi.rs          # AST → MIDI bytes
│   ├── transpose.rs     # Transposition logic
│   └── feedback.rs      # Warnings/errors with source locations
└── tests/
    ├── parser_tests.rs
    ├── midi_tests.rs
    └── fixtures/        # .abc test files
```

### Dependencies

```toml
[package]
name = "abc"
version = "0.1.0"
edition = "2021"

[dependencies]
winnow = "0.6"
serde = { version = "1", features = ["derive"] }
thiserror = "2"

[dev-dependencies]
pretty_assertions = "1"
```

No MIDI crate dependency - we write SMF format 0 directly (~100 lines).

---

## Part 2: ABC Notation Quick Reference

### File Structure

```abc
X:1                    ← Reference number (required, starts tune)
T:Whiskey Before Breakfast
M:4/4                  ← Meter
L:1/8                  ← Unit note length (eighth note)
Q:1/4=120              ← Tempo
K:D                    ← Key (required, ends header)
|:D2FA DFAF|G2BG dGBG|  ← Music body
```

### Notes and Octaves

```
C D E F G A B    ← Octave 0 (below middle C)
c d e f g a b    ← Octave 1 (middle C octave)
c' d' e'         ← Octave 2 (apostrophe = up)
C, D, E,         ← Octave -1 (comma = down)
```

### Accidentals

```
^c   C sharp       _c   C flat
^^c  C double sharp    __c  C double flat
=c   C natural (explicit)
```

### Durations (given L:1/8)

```
A    1 unit (eighth)     A2   2 units (quarter)
A4   4 units (half)      A/2  half unit (sixteenth)
A/   same as A/2         A3/2 dotted eighth
```

### Rests

```
z    visible rest (same duration rules)
z2   rest for 2 units
Z4   4 measures rest (multi-measure)
```

### Bars and Repeats

```
|    bar line            |:   start repeat
:|   end repeat          ::   end + start repeat
|1   first ending        :|2  second ending
|]   end bar             ||   double bar
```

### Chords and Ties

```
[CEG]   C major chord (simultaneous)
"G"     chord symbol (display only)
c-c     tie (same pitch held)
(cde)   slur (legato, for future use)
```

### Tuplets

```
(3abc   triplet (3 notes in time of 2)
```

### Decorations (captured in AST, MIDI output deferred)

```
.A   staccato    ~A   roll       HA   fermata
TA   trill       {g}A grace note
```

### Multi-voice (AST support, parsing deferred)

```
V:1 name="Melody"
GABc|
V:2 name="Bass"
G,2D2|
```

---

## Part 3: AST Schema

```rust
// === Top-level ===

/// A complete ABC tune with parse feedback
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ParseResult {
    pub tune: Tune,
    pub feedback: Vec<Feedback>,
}

/// A complete ABC tune
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Tune {
    pub header: Header,
    pub voices: Vec<Voice>,
}

// === Header ===

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Header {
    pub reference: u32,
    pub title: String,
    pub titles: Vec<String>,           // Additional T: fields
    pub key: Key,
    pub meter: Option<Meter>,
    pub unit_length: Option<UnitLength>,
    pub tempo: Option<Tempo>,
    pub composer: Option<String>,
    pub rhythm: Option<String>,        // R: field (reel, jig, etc.)
    pub source: Option<String>,        // S: field
    pub notes: Option<String>,         // N: field
    pub other_fields: Vec<InfoField>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Key {
    pub root: NoteName,
    pub accidental: Option<Accidental>,
    pub mode: Mode,
    pub explicit_accidentals: Vec<(Accidental, NoteName)>,
    pub clef: Option<Clef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum NoteName { C, D, E, F, G, A, B }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Accidental { DoubleSharp, Sharp, Natural, Flat, DoubleFlat }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Mode {
    Major, Minor,
    Ionian, Dorian, Phrygian, Lydian, Mixolydian, Aeolian, Locrian,
}

impl Default for Mode {
    fn default() -> Self { Mode::Major }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum Meter {
    Simple { numerator: u8, denominator: u8 },
    Common,      // C = 4/4
    Cut,         // C| = 2/2
    None,        // No meter
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct UnitLength {
    pub numerator: u8,
    pub denominator: u8,
}

impl Default for UnitLength {
    fn default() -> Self {
        UnitLength { numerator: 1, denominator: 8 }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Tempo {
    pub beat_unit: (u8, u8),  // e.g., (1, 4) for quarter note
    pub bpm: u16,
    pub text: Option<String>, // e.g., "Allegro"
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Clef { Treble, Bass, Alto, Tenor }

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct InfoField {
    pub field_type: char,
    pub value: String,
}

// === Voice/Body ===

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Voice {
    pub id: Option<String>,      // V:1, V:melody, etc.
    pub name: Option<String>,    // name="Melody"
    pub elements: Vec<Element>,
}

impl Default for Voice {
    fn default() -> Self {
        Voice { id: None, name: None, elements: Vec::new() }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum Element {
    Note(Note),
    Chord(Chord),
    Rest(Rest),
    Bar(Bar),
    Tuplet(Tuplet),
    GraceNotes { acciaccatura: bool, notes: Vec<Note> },
    ChordSymbol(String),
    InlineField(InfoField),
    Decoration(Decoration),
    Slur(SlurBoundary),
    Space,
    LineBreak,
}

// === Notes ===

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Note {
    pub pitch: NoteName,
    pub octave: i8,                    // 0 = C-B, 1 = c-b, etc.
    pub accidental: Option<Accidental>,
    pub duration: Duration,
    pub tie: bool,
    pub decorations: Vec<Decoration>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Duration {
    pub numerator: u16,
    pub denominator: u16,
}

impl Duration {
    pub fn unit() -> Self {
        Duration { numerator: 1, denominator: 1 }
    }

    pub fn to_ticks(&self, ticks_per_unit: u32) -> u32 {
        (ticks_per_unit * self.numerator as u32) / self.denominator as u32
    }
}

impl Default for Duration {
    fn default() -> Self { Self::unit() }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Chord {
    pub notes: Vec<Note>,
    pub duration: Duration,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Rest {
    pub duration: Duration,
    pub visible: bool,           // z vs x
    pub multi_measure: Option<u16>,  // Z4 = 4 bars
}

// === Structure ===

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum Bar {
    Single,
    Double,
    End,              // |]
    RepeatStart,      // |:
    RepeatEnd,        // :|
    RepeatBoth,       // ::
    FirstEnding,      // |1
    SecondEnding,     // :|2
    NthEnding(Vec<u8>), // [1,3 etc.
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Tuplet {
    pub p: u8,              // p notes
    pub q: u8,              // in time of q
    pub elements: Vec<Element>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum SlurBoundary { Start, End }

// === Decorations (captured for future MIDI enhancement) ===

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum Decoration {
    Staccato,
    Accent,
    Fermata,
    Trill,
    Roll,           // Irish ~
    Mordent { upper: bool },
    Turn,
    UpBow,
    DownBow,
    Dynamic(Dynamic),
    Crescendo { start: bool },
    Diminuendo { start: bool },
    Other(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Dynamic { PPP, PP, P, MP, MF, F, FF, FFF }

// === Feedback ===

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Feedback {
    pub level: FeedbackLevel,
    pub message: String,
    pub line: usize,
    pub column: usize,
    pub suggestion: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum FeedbackLevel {
    Error,      // Can't continue parsing
    Warning,    // Parsed with assumptions
    Info,       // Style suggestion
}
```

---

## Part 4: MCP Tools (MVP)

### `abc_parse`

Parse ABC notation into structured AST.

```json
{
  "name": "abc_parse",
  "description": "Parse ABC notation into a structured AST. Returns the parsed tune and any warnings/errors.",
  "input_schema": {
    "type": "object",
    "properties": {
      "abc": {
        "type": "string",
        "description": "ABC notation string to parse"
      }
    },
    "required": ["abc"]
  }
}
```

**Returns:**
```json
{
  "tune": { ... },
  "feedback": [
    {
      "level": "warning",
      "message": "Missing M: field, assuming 4/4",
      "line": 1,
      "column": 1,
      "suggestion": "Add M:4/4 after the title"
    }
  ],
  "cas_hash": "abc123..."
}
```

### `abc_to_midi`

Convert ABC to MIDI, store in CAS.

```json
{
  "name": "abc_to_midi",
  "description": "Convert ABC notation to MIDI. Stores result in CAS.",
  "input_schema": {
    "type": "object",
    "properties": {
      "abc": {
        "type": "string",
        "description": "ABC notation to convert"
      },
      "tempo_override": {
        "type": "integer",
        "description": "Override tempo (BPM)"
      },
      "transpose": {
        "type": "integer",
        "description": "Semitones to transpose"
      },
      "velocity": {
        "type": "integer",
        "description": "MIDI velocity (1-127)",
        "default": 80
      }
    },
    "required": ["abc"]
  }
}
```

**Returns:**
```json
{
  "midi_hash": "def456...",
  "duration_seconds": 32.5,
  "note_count": 64,
  "feedback": [ ... ]
}
```

### `abc_validate`

Quick validation without full parse output.

```json
{
  "name": "abc_validate",
  "description": "Validate ABC notation and return feedback without full AST",
  "input_schema": {
    "type": "object",
    "properties": {
      "abc": {
        "type": "string"
      }
    },
    "required": ["abc"]
  }
}
```

**Returns:**
```json
{
  "valid": true,
  "feedback": [ ... ],
  "summary": {
    "bars": 16,
    "notes": 64,
    "key": "G major",
    "meter": "4/4"
  }
}
```

### `abc_transpose`

Transpose ABC to a different key.

```json
{
  "name": "abc_transpose",
  "description": "Transpose ABC notation by semitones or to a target key",
  "input_schema": {
    "type": "object",
    "properties": {
      "abc": {
        "type": "string"
      },
      "semitones": {
        "type": "integer",
        "description": "Semitones to transpose (positive = up)"
      },
      "target_key": {
        "type": "string",
        "description": "Target key (e.g., 'Am', 'Bb'). Alternative to semitones."
      }
    },
    "required": ["abc"]
  }
}
```

**Returns:**
```json
{
  "abc": "X:1\nT:...\nK:A\n...",
  "original_key": "G major",
  "new_key": "A major",
  "semitones": 2
}
```

---

## Part 5: MIDI Generation

### SMF Format 0 (Single Track)

Write MIDI bytes directly without a crate dependency:

```rust
pub fn to_midi(tune: &Tune, params: &MidiParams) -> Vec<u8> {
    let mut writer = MidiWriter::new(params.ticks_per_beat);

    // Track tempo
    if let Some(tempo) = &tune.header.tempo {
        writer.tempo(tempo.bpm);
    }

    // Track key signature
    writer.key_signature(&tune.header.key);

    // Process voice(s) - for MVP, merge all into track 0
    let key_accidentals = compute_key_accidentals(&tune.header.key);
    let unit_ticks = compute_unit_ticks(
        &tune.header.unit_length,
        params.ticks_per_beat
    );

    for voice in &tune.voices {
        let mut bar_accidentals = key_accidentals.clone();

        for element in &voice.elements {
            match element {
                Element::Note(note) => {
                    let midi_pitch = note_to_midi(note, &bar_accidentals);
                    let ticks = note.duration.to_ticks(unit_ticks);
                    writer.note(midi_pitch, params.velocity, ticks);

                    // Track accidentals for the bar
                    if let Some(acc) = note.accidental {
                        bar_accidentals.insert(note.pitch, acc);
                    }
                }
                Element::Rest(rest) => {
                    let ticks = rest.duration.to_ticks(unit_ticks);
                    writer.advance(ticks);
                }
                Element::Bar(_) => {
                    // Reset accidentals at bar line
                    bar_accidentals = key_accidentals.clone();
                }
                Element::Chord(chord) => {
                    // All notes start together, end together
                    let ticks = chord.duration.to_ticks(unit_ticks);
                    for note in &chord.notes {
                        let midi_pitch = note_to_midi(note, &bar_accidentals);
                        writer.note_on(midi_pitch, params.velocity);
                    }
                    writer.advance(ticks);
                    for note in &chord.notes {
                        let midi_pitch = note_to_midi(note, &bar_accidentals);
                        writer.note_off(midi_pitch);
                    }
                }
                // Tuplets, grace notes: handled in phase 2
                _ => {}
            }
        }
    }

    writer.finish()
}

fn note_to_midi(note: &Note, bar_accidentals: &HashMap<NoteName, Accidental>) -> u8 {
    let base = match note.pitch {
        NoteName::C => 0,
        NoteName::D => 2,
        NoteName::E => 4,
        NoteName::F => 5,
        NoteName::G => 7,
        NoteName::A => 9,
        NoteName::B => 11,
    };

    // Octave: ABC octave 1 (c-b) = MIDI octave 5 (middle C = 60)
    let octave_midi = (note.octave + 5) * 12;

    // Accidental: note's own, then bar context, then key signature
    let acc_offset = note.accidental
        .or_else(|| bar_accidentals.get(&note.pitch).copied())
        .map(|a| match a {
            Accidental::DoubleSharp => 2,
            Accidental::Sharp => 1,
            Accidental::Natural => 0,
            Accidental::Flat => -1,
            Accidental::DoubleFlat => -2,
        })
        .unwrap_or(0);

    (base + octave_midi as u8).saturating_add_signed(acc_offset)
}
```

### MidiWriter Implementation Sketch

```rust
struct MidiWriter {
    ticks_per_beat: u16,
    current_tick: u32,
    events: Vec<MidiEvent>,
}

struct MidiEvent {
    delta: u32,
    data: Vec<u8>,
}

impl MidiWriter {
    fn new(ticks_per_beat: u16) -> Self { ... }

    fn tempo(&mut self, bpm: u16) {
        let us_per_beat = 60_000_000 / bpm as u32;
        self.meta_event(&[0x51,
            (us_per_beat >> 16) as u8,
            (us_per_beat >> 8) as u8,
            us_per_beat as u8,
        ]);
    }

    fn note_on(&mut self, pitch: u8, velocity: u8) {
        self.channel_event(&[0x90, pitch, velocity]);
    }

    fn note_off(&mut self, pitch: u8) {
        self.channel_event(&[0x80, pitch, 0]);
    }

    fn note(&mut self, pitch: u8, velocity: u8, duration: u32) {
        self.note_on(pitch, velocity);
        self.advance(duration);
        self.note_off(pitch);
    }

    fn advance(&mut self, ticks: u32) {
        self.current_tick += ticks;
    }

    fn finish(self) -> Vec<u8> {
        // Write SMF header + single track
        let mut out = Vec::new();

        // Header chunk: MThd
        out.extend_from_slice(b"MThd");
        out.extend_from_slice(&6u32.to_be_bytes()); // chunk length
        out.extend_from_slice(&0u16.to_be_bytes()); // format 0
        out.extend_from_slice(&1u16.to_be_bytes()); // 1 track
        out.extend_from_slice(&self.ticks_per_beat.to_be_bytes());

        // Track chunk: MTrk
        let track_data = self.encode_track();
        out.extend_from_slice(b"MTrk");
        out.extend_from_slice(&(track_data.len() as u32).to_be_bytes());
        out.extend(track_data);

        out
    }

    fn encode_track(&self) -> Vec<u8> {
        let mut out = Vec::new();
        let mut last_tick = 0u32;

        for event in &self.events {
            let delta = event.delta - last_tick;
            out.extend(encode_variable_length(delta));
            out.extend(&event.data);
            last_tick = event.delta;
        }

        // End of track
        out.extend(&[0x00, 0xFF, 0x2F, 0x00]);
        out
    }
}

fn encode_variable_length(mut value: u32) -> Vec<u8> {
    // MIDI variable-length quantity encoding
    let mut bytes = vec![value as u8 & 0x7F];
    value >>= 7;
    while value > 0 {
        bytes.push((value as u8 & 0x7F) | 0x80);
        value >>= 7;
    }
    bytes.reverse();
    bytes
}
```

---

## Part 6: Parser Strategy (winnow)

### Generous Parsing Philosophy

1. **Accept common mistakes** - Missing `X:` at start? Warn and assume `X:1`.
2. **Infer missing fields** - No `L:`? Infer from meter (4/4 → 1/8, 6/8 → 1/8).
3. **Skip unknown syntax** - Unrecognized decoration? Warn and continue.
4. **Preserve source info** - Every feedback item has line/column.

### Parser Structure

```rust
use winnow::prelude::*;
use winnow::combinator::{alt, opt, repeat, preceded, delimited};
use winnow::token::{any, one_of, take_while};
use winnow::ascii::{digit1, line_ending, space0};

type Stream<'a> = &'a str;

pub fn parse(input: &str) -> ParseResult {
    let mut feedback = Vec::new();

    match tune.parse(input) {
        Ok(tune) => ParseResult { tune, feedback },
        Err(e) => {
            feedback.push(Feedback {
                level: FeedbackLevel::Error,
                message: format!("Parse error: {}", e),
                line: 1, // TODO: extract from error
                column: 1,
                suggestion: None,
            });
            // Return partial result if possible
            ParseResult {
                tune: Tune::default(),
                feedback,
            }
        }
    }
}

fn tune(input: &mut Stream) -> PResult<Tune> {
    let header = header.parse_next(input)?;
    let body = body.parse_next(input)?;

    Ok(Tune {
        header,
        voices: vec![Voice { id: None, name: None, elements: body }],
    })
}

fn header(input: &mut Stream) -> PResult<Header> {
    // X: field (required but we're generous)
    let reference = opt(x_field).parse_next(input)?.unwrap_or(1);

    // Collect fields until K:
    let mut header = Header::default();
    header.reference = reference;

    loop {
        let field = info_field.parse_next(input)?;
        match field.field_type {
            'T' if header.title.is_empty() => header.title = field.value,
            'T' => header.titles.push(field.value),
            'M' => header.meter = Some(parse_meter(&field.value)),
            'L' => header.unit_length = Some(parse_unit_length(&field.value)),
            'Q' => header.tempo = Some(parse_tempo(&field.value)),
            'K' => {
                header.key = parse_key(&field.value);
                break;
            }
            'C' => header.composer = Some(field.value),
            'R' => header.rhythm = Some(field.value),
            _ => header.other_fields.push(field),
        }
    }

    Ok(header)
}

fn info_field(input: &mut Stream) -> PResult<InfoField> {
    let field_type = one_of('A'..='Z').parse_next(input)?;
    ':'.parse_next(input)?;
    space0.parse_next(input)?;
    let value = take_while(0.., |c| c != '\n' && c != '\r')
        .parse_next(input)?;
    opt(line_ending).parse_next(input)?;

    Ok(InfoField {
        field_type,
        value: value.trim().to_string(),
    })
}

fn body(input: &mut Stream) -> PResult<Vec<Element>> {
    repeat(0.., element).parse_next(input)
}

fn element(input: &mut Stream) -> PResult<Element> {
    alt((
        note.map(Element::Note),
        chord.map(Element::Chord),
        rest.map(Element::Rest),
        bar.map(Element::Bar),
        tuplet.map(Element::Tuplet),
        chord_symbol.map(Element::ChordSymbol),
        decoration.map(Element::Decoration),
        space0.map(|_| Element::Space),
    )).parse_next(input)
}

fn note(input: &mut Stream) -> PResult<Note> {
    let accidental = opt(accidental).parse_next(input)?;
    let (pitch, base_octave) = pitch.parse_next(input)?;
    let octave_mod = octave_modifier.parse_next(input)?;
    let duration = opt(duration).parse_next(input)?.unwrap_or_default();
    let tie = opt('-').parse_next(input)?.is_some();

    Ok(Note {
        pitch,
        octave: base_octave + octave_mod,
        accidental,
        duration,
        tie,
        decorations: Vec::new(),
    })
}

fn pitch(input: &mut Stream) -> PResult<(NoteName, i8)> {
    let c = any.parse_next(input)?;
    match c {
        'C' => Ok((NoteName::C, 0)),
        'D' => Ok((NoteName::D, 0)),
        'E' => Ok((NoteName::E, 0)),
        'F' => Ok((NoteName::F, 0)),
        'G' => Ok((NoteName::G, 0)),
        'A' => Ok((NoteName::A, 0)),
        'B' => Ok((NoteName::B, 0)),
        'c' => Ok((NoteName::C, 1)),
        'd' => Ok((NoteName::D, 1)),
        'e' => Ok((NoteName::E, 1)),
        'f' => Ok((NoteName::F, 1)),
        'g' => Ok((NoteName::G, 1)),
        'a' => Ok((NoteName::A, 1)),
        'b' => Ok((NoteName::B, 1)),
        _ => Err(winnow::error::ErrMode::Backtrack(
            winnow::error::ContextError::new()
        )),
    }
}

fn accidental(input: &mut Stream) -> PResult<Accidental> {
    alt((
        "^^".map(|_| Accidental::DoubleSharp),
        "^".map(|_| Accidental::Sharp),
        "__".map(|_| Accidental::DoubleFlat),
        "_".map(|_| Accidental::Flat),
        "=".map(|_| Accidental::Natural),
    )).parse_next(input)
}

fn octave_modifier(input: &mut Stream) -> PResult<i8> {
    let ups: Vec<_> = repeat(0.., '\'').parse_next(input)?;
    let downs: Vec<_> = repeat(0.., ',').parse_next(input)?;
    Ok(ups.len() as i8 - downs.len() as i8)
}

fn duration(input: &mut Stream) -> PResult<Duration> {
    let multiplier = opt(digit1.parse_to::<u16>()).parse_next(input)?;
    let divisor = opt(preceded('/', opt(digit1.parse_to::<u16>())))
        .parse_next(input)?;

    let num = multiplier.unwrap_or(1);
    let den = match divisor {
        Some(Some(d)) => d,
        Some(None) => 2,  // A/ means A/2
        None => 1,
    };

    Ok(Duration { numerator: num, denominator: den })
}
```

---

## Part 7: Integration with Hootenanny

### Request/Response Schemas

Add to `crates/hootenanny/src/api/schema.rs`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AbcParseRequest {
    #[schemars(description = "ABC notation string to parse")]
    pub abc: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AbcToMidiRequest {
    #[schemars(description = "ABC notation to convert")]
    pub abc: String,

    #[schemars(description = "Override tempo (BPM)")]
    pub tempo_override: Option<u16>,

    #[schemars(description = "Semitones to transpose")]
    pub transpose: Option<i8>,

    #[schemars(description = "MIDI velocity (1-127)")]
    #[serde(default = "default_velocity")]
    pub velocity: u8,

    // Standard artifact fields
    #[schemars(description = "Optional variation set ID")]
    pub variation_set_id: Option<String>,

    #[schemars(description = "Optional parent artifact ID")]
    pub parent_id: Option<String>,

    #[serde(default)]
    pub tags: Vec<String>,

    #[serde(default = "default_creator")]
    pub creator: Option<String>,
}

fn default_velocity() -> u8 { 80 }

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AbcValidateRequest {
    pub abc: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AbcTransposeRequest {
    pub abc: String,

    #[schemars(description = "Semitones to transpose (positive = up)")]
    pub semitones: Option<i8>,

    #[schemars(description = "Target key (e.g., 'Am', 'Bb')")]
    pub target_key: Option<String>,
}
```

### Tool Implementations

Add `crates/hootenanny/src/api/tools/abc.rs`:

```rust
use crate::api::schema::*;
use crate::api::service::EventDualityServer;
use baton::{ErrorData as McpError, CallToolResult, Content};

impl EventDualityServer {
    pub async fn abc_parse(
        &self,
        request: AbcParseRequest,
    ) -> Result<CallToolResult, McpError> {
        let result = abc::parse(&request.abc);

        // Store ABC source in CAS
        let abc_hash = self.cas.write(
            request.abc.as_bytes(),
            "text/vnd.abc"
        ).map_err(|e| McpError::internal(e.to_string()))?;

        let response = serde_json::json!({
            "tune": result.tune,
            "feedback": result.feedback,
            "cas_hash": abc_hash,
        });

        Ok(CallToolResult::success(vec![Content::text(response.to_string())]))
    }

    pub async fn abc_to_midi(
        &self,
        request: AbcToMidiRequest,
    ) -> Result<CallToolResult, McpError> {
        let parse_result = abc::parse(&request.abc);

        if parse_result.feedback.iter().any(|f| f.level == abc::FeedbackLevel::Error) {
            return Err(McpError::invalid_params(
                format!("ABC parse errors: {:?}", parse_result.feedback)
            ));
        }

        let mut tune = parse_result.tune;

        // Apply tempo override
        if let Some(bpm) = request.tempo_override {
            tune.header.tempo = Some(abc::Tempo {
                beat_unit: (1, 4),
                bpm,
                text: None,
            });
        }

        // Apply transposition
        if let Some(semitones) = request.transpose {
            tune = abc::transpose(&tune, semitones);
        }

        // Generate MIDI
        let midi_bytes = abc::to_midi(&tune, &abc::MidiParams {
            velocity: request.velocity,
            ticks_per_beat: 480,
        });

        // Store in CAS
        let midi_hash = self.cas.write(&midi_bytes, "audio/midi")
            .map_err(|e| McpError::internal(e.to_string()))?;

        // Create artifact
        let artifact_id = format!("artifact_{}", &midi_hash[..12]);
        let artifact = Artifact::new(
            &artifact_id,
            request.creator.as_deref().unwrap_or("unknown"),
            serde_json::json!({
                "hash": midi_hash,
                "source": "abc",
                "note_count": count_notes(&tune),
            })
        )
        .with_tags(vec!["type:midi", "source:abc", "tool:abc_to_midi"])
        .with_tags(request.tags);

        self.artifact_store.put(artifact)?;

        let response = serde_json::json!({
            "midi_hash": midi_hash,
            "artifact_id": artifact_id,
            "feedback": parse_result.feedback,
            "note_count": count_notes(&tune),
        });

        Ok(CallToolResult::success(vec![Content::text(response.to_string())]))
    }

    pub async fn abc_validate(
        &self,
        request: AbcValidateRequest,
    ) -> Result<CallToolResult, McpError> {
        let result = abc::parse(&request.abc);

        let valid = !result.feedback.iter()
            .any(|f| f.level == abc::FeedbackLevel::Error);

        let response = serde_json::json!({
            "valid": valid,
            "feedback": result.feedback,
            "summary": {
                "key": format!("{:?} {:?}", result.tune.header.key.root, result.tune.header.key.mode),
                "meter": result.tune.header.meter,
                "bars": count_bars(&result.tune),
                "notes": count_notes(&result.tune),
            }
        });

        Ok(CallToolResult::success(vec![Content::text(response.to_string())]))
    }

    pub async fn abc_transpose(
        &self,
        request: AbcTransposeRequest,
    ) -> Result<CallToolResult, McpError> {
        let parse_result = abc::parse(&request.abc);

        let semitones = if let Some(s) = request.semitones {
            s
        } else if let Some(target) = &request.target_key {
            abc::semitones_to_key(&parse_result.tune.header.key, target)
                .map_err(|e| McpError::invalid_params(e.to_string()))?
        } else {
            return Err(McpError::invalid_params(
                "Must specify either semitones or target_key"
            ));
        };

        let transposed = abc::transpose(&parse_result.tune, semitones);
        let abc_output = abc::to_abc(&transposed);

        let response = serde_json::json!({
            "abc": abc_output,
            "original_key": format!("{:?}", parse_result.tune.header.key),
            "new_key": format!("{:?}", transposed.header.key),
            "semitones": semitones,
        });

        Ok(CallToolResult::success(vec![Content::text(response.to_string())]))
    }
}
```

---

## Part 8: Testing Strategy

### Unit Tests (parser)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_note() {
        let result = parse("X:1\nT:Test\nK:C\nCDEF|");
        assert!(result.feedback.is_empty());
        assert_eq!(result.tune.voices[0].elements.len(), 5); // 4 notes + bar
    }

    #[test]
    fn parse_with_accidentals() {
        let result = parse("X:1\nT:Test\nK:C\n^C_D=E^^F|");
        let notes: Vec<_> = result.tune.voices[0].elements.iter()
            .filter_map(|e| match e {
                Element::Note(n) => Some(n),
                _ => None,
            })
            .collect();

        assert_eq!(notes[0].accidental, Some(Accidental::Sharp));
        assert_eq!(notes[1].accidental, Some(Accidental::Flat));
        assert_eq!(notes[2].accidental, Some(Accidental::Natural));
        assert_eq!(notes[3].accidental, Some(Accidental::DoubleSharp));
    }

    #[test]
    fn parse_octaves() {
        let result = parse("X:1\nT:Test\nK:C\nC,Cc'|");
        let notes: Vec<_> = result.tune.voices[0].elements.iter()
            .filter_map(|e| match e {
                Element::Note(n) => Some(n),
                _ => None,
            })
            .collect();

        assert_eq!(notes[0].octave, -1); // C,
        assert_eq!(notes[1].octave, 0);  // C
        assert_eq!(notes[2].octave, 2);  // c'
    }

    #[test]
    fn parse_durations() {
        let result = parse("X:1\nT:Test\nK:C\nA A2 A/2 A3/2|");
        let notes: Vec<_> = result.tune.voices[0].elements.iter()
            .filter_map(|e| match e {
                Element::Note(n) => Some(n),
                _ => None,
            })
            .collect();

        assert_eq!(notes[0].duration, Duration { numerator: 1, denominator: 1 });
        assert_eq!(notes[1].duration, Duration { numerator: 2, denominator: 1 });
        assert_eq!(notes[2].duration, Duration { numerator: 1, denominator: 2 });
        assert_eq!(notes[3].duration, Duration { numerator: 3, denominator: 2 });
    }

    #[test]
    fn parse_missing_meter_warns() {
        let result = parse("X:1\nT:Test\nK:C\nCDEF|");
        assert!(result.feedback.iter().any(|f|
            f.level == FeedbackLevel::Warning &&
            f.message.contains("M:")
        ));
    }

    #[test]
    fn parse_chord() {
        let result = parse("X:1\nT:Test\nK:C\n[CEG]2|");
        match &result.tune.voices[0].elements[0] {
            Element::Chord(c) => {
                assert_eq!(c.notes.len(), 3);
                assert_eq!(c.duration.numerator, 2);
            }
            _ => panic!("Expected chord"),
        }
    }
}
```

### Integration Tests (MIDI output)

```rust
#[test]
fn midi_roundtrip_with_rustysynth() {
    let abc = "X:1\nT:Test\nM:4/4\nL:1/4\nQ:1/4=120\nK:C\nCDEF|GABC|";
    let result = abc::parse(abc);
    let midi_bytes = abc::to_midi(&result.tune, &abc::MidiParams::default());

    // Verify it's valid MIDI by checking header
    assert_eq!(&midi_bytes[0..4], b"MThd");

    // Verify rustysynth can read it
    let mut cursor = std::io::Cursor::new(&midi_bytes);
    let midi_file = rustysynth::MidiFile::new(&mut cursor).unwrap();
    assert!(midi_file.get_length() > 0.0);
}
```

### Fixture Files

Create `crates/abc/tests/fixtures/`:

```
simple.abc          - Basic melody
accidentals.abc     - All accidental types
durations.abc       - Various note lengths
chords.abc          - Simultaneous notes
repeats.abc         - Bar lines and repeats
tuplets.abc         - Triplets
grace_notes.abc     - Ornaments (parse only, no MIDI)
multivoice.abc      - V: fields (parse deferred)
llm_quirks.abc      - Missing headers, extra whitespace
```

---

## Part 9: Implementation Phases

### Phase 1: Parser Foundation (MVP) ✅ COMPLETE
- [x] Create `crates/abc/` with Cargo.toml
- [x] Implement AST types in `ast.rs`
- [x] Implement header parsing (X, T, M, L, Q, K fields)
- [x] Implement note parsing (pitch, octave, accidental, duration)
- [x] Implement bar lines
- [x] Implement chord parsing `[CEG]`
- [x] Implement rest parsing
- [x] Feedback collection infrastructure
- [x] Unit tests for all parsers (61 tests)

### Phase 2: MIDI Generation ✅ COMPLETE
- [x] Implement MidiWriter (SMF format 0)
- [x] Note → MIDI pitch conversion with key signatures
- [x] Duration → ticks conversion
- [x] Tempo meta events
- [x] Tuplet support in MIDI
- [x] Unit tests (70 tests total)

### Phase 3: MCP Integration ✅ COMPLETE
- [x] Add request schemas to hootenanny
- [x] Implement `abc_parse` tool
- [x] Implement `abc_to_midi` tool
- [x] Implement `abc_validate` tool
- [x] Implement `abc_transpose` tool (stub - returns error for now)
- [x] Wire into handler dispatch
- [x] Fix MIDI tempo meta event (was missing length byte)

### Phase 4: Polish ✅ COMPLETE
- [x] Tie handling (merge note durations across `-`)
- [x] Repeat expansion in MIDI (`|:` ... `:|` → duplicated events)
- [ ] Better error messages with suggestions (deferred)
- [x] Fixture test suite (.abc files in tests/fixtures/) - 8 fixtures, 9 tests

### Phase 5: Multi-Voice Support ✅ COMPLETE

**ABC Multi-Voice Syntax:**
```abc
X:1
T:Two Voice Example
M:4/4
L:1/4
V:1 name="Melody" clef=treble
V:2 name="Bass" clef=bass
K:C
V:1
cdef|gabc'|
V:2
C,G,C,G,|C,G,C,G,|
```

**Implementation:**

1. **AST Changes** (`ast.rs`):
   - [x] Added `VoiceDef` struct with id, name, clef, octave, transpose, stem
   - [x] Added `voice_defs: Vec<VoiceDef>` to Header
   - [x] Added `Clef` enum (Treble, Bass, Alto, Tenor, Percussion)
   - [x] Added `StemDirection` enum
   - [x] Added `Element::VoiceSwitch(String)` variant

2. **Parser Changes** (`parser/header.rs`):
   - [x] Parse `V:` voice definition fields
   - [x] Extract voice ID, name, clef from `V:id name="..." clef=...`

3. **Parser Changes** (`parser/body.rs` + `parser/mod.rs`):
   - [x] Parse `V:id` standalone markers
   - [x] Parse `[V:id]` inline markers
   - [x] Route elements to correct Voice based on VoiceSwitch

4. **MIDI Changes** (`midi.rs`):
   - [x] SMF format 1 when multiple voices have content
   - [x] Track 0 = tempo/meta events
   - [x] Track 1+ = one per voice with unique MIDI channel
   - [x] Single voice = format 0 (backward compatible)

**Tests:** 87 total (75 unit + 11 fixture + 1 doc)

**Bug Fix:** Octave calculation was `(octave+4)*12`, should be `(octave+5)*12`.
ABC uppercase `C` = middle C (MIDI 60), verified against abc2midi.

### Future Phases (not MVP)
- Grace notes → MIDI (short notes before beat)
- Decorations → MIDI (staccato shortens, accents boost velocity)
- Dynamics → velocity (pp=40, mf=80, ff=120)
- Tune AST → ABC string (for transpose tool)

### Phase 6: MIDI → ABC (blocked on quantization research)
- [ ] Quantize MIDI ticks to nearest note values (1/4, 1/8, 1/16, triplets)
- [ ] Infer time signature and bar lines
- [ ] Detect key signature from note patterns
- [ ] Distinguish ties from repeated notes
- [ ] Handle polyphony → voices or chords
- [ ] `midi_to_abc` MCP tool

**Use case:** Edit Orpheus-generated MIDI in human-readable ABC notation.
**Blocker:** Quantization strategy TBD - research in progress.

---

## Summary

This plan delivers:

1. **`crates/abc/`** - Standalone crate with winnow parser, clean AST, direct MIDI writing
2. **4 MCP tools** - `abc_parse`, `abc_to_midi`, `abc_validate`, `abc_transpose`
3. **Generous parsing** - Degrades gracefully with actionable feedback
4. **CAS integration** - Both ABC source and MIDI output stored by hash
5. **Future-proof AST** - Multi-voice, decorations captured even before MIDI support

The MVP enables LLMs and humans to write ABC notation and hear it immediately via the existing `midi_to_wav` pipeline.
