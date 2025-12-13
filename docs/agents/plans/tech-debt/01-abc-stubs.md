# 01: ABC Stubs

**File:** `crates/abc/src/lib.rs`
**Focus:** Implement 3 stub functions (lines 70-85)
**Dependencies:** None

---

## Task

Implement the three stub functions in `abc/src/lib.rs`:

```rust
// Line 70 - Currently returns Tune::default()
pub fn transpose(tune: &Tune, semitones: i8) -> Tune

// Line 76 - Currently returns String::new()
pub fn to_abc(tune: &Tune) -> String

// Line 82 - Currently returns Err("Not implemented")
pub fn semitones_to_key(source: &Key, target: &str) -> Result<i8, String>
```

**Why this first?** These stubs block ABC round-tripping and transposition features.

**Deliverables:**
1. `transpose()` - Shift all notes by semitones
2. `to_abc()` - Serialize Tune back to ABC notation
3. `semitones_to_key()` - Calculate transposition interval

**Definition of Done:**
```bash
cargo fmt --check
cargo clippy -- -D warnings
cargo check -p abc
cargo test -p abc
```

## Out of Scope

- Parser changes (task 04)
- MIDI generation changes

---

## Implementation Notes

**transpose():**
- Clone the tune
- For each Voice → each Element → each Note:
  - Add semitones to note's pitch (use `NoteName::to_semitone()` + accidental offset)
  - Handle octave wrapping (semitone > 11 → octave++, semitone < 0 → octave--)
  - Reconstruct NoteName + Accidental from new semitone
- Update header key signature

**to_abc():**
- Output header fields: `X:`, `T:`, `M:`, `L:`, `K:`
- For each voice, output elements:
  - Note → pitch letter + accidental + octave markers + duration
  - Rest → `z` + duration
  - Barline → `|`, `||`, `|]`, etc.
  - Chord → `[notes]`

**semitones_to_key():**
- Parse target key string (e.g., "Am", "Bb", "F#m")
- Calculate source key's semitone (root + accidental + mode offset)
- Calculate target key's semitone
- Return difference

---

## Types Reference

```rust
pub struct Tune { header: Header, voices: Vec<Voice> }
pub struct Voice { id: Option<String>, elements: Vec<Element> }
pub enum Element { Note(Note), Rest(Rest), Chord(Chord), Barline(Barline), ... }
pub struct Note { pitch: Pitch, duration: Duration, ... }
pub struct Pitch { note: NoteName, accidental: Option<Accidental>, octave: i8 }
pub struct Key { root: NoteName, accidental: Option<Accidental>, mode: Mode, ... }

impl NoteName {
    pub fn to_semitone(&self) -> i8 { /* C=0, D=2, E=4, F=5, G=7, A=9, B=11 */ }
}

impl Accidental {
    pub fn to_semitone_offset(&self) -> i8 { /* DoubleSharp=2, Sharp=1, Natural=0, Flat=-1, DoubleFlat=-2 */ }
}
```

---

## Acceptance Criteria

- [ ] `transpose(&tune, 0)` returns equivalent tune
- [ ] `transpose(&tune, 12)` shifts all notes up one octave
- [ ] `to_abc(&parse(abc).value)` round-trips (ignoring whitespace)
- [ ] `semitones_to_key(&Key::default(), "G")` returns `Ok(7)`
- [ ] `semitones_to_key(&Key::default(), "F")` returns `Ok(5)` or `Ok(-7)`
