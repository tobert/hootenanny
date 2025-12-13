# 04: ABC Key Parsing

**File:** `crates/abc/src/parser/key.rs`
**Focus:** Complete explicit accidentals parsing (line 86 TODO)
**Dependencies:** 01-abc-stubs (uses same types)

---

## Task

The TODO at line 86 indicates incomplete parsing of explicit accidentals in key signatures. ABC allows:

- `K:Hp` - Highland bagpipe scale
- `K:C exp ^f =c` - Explicit accidentals override
- `K:Am ^g` - A minor with G# (melodic minor)

**Why this first?** Depends on task 01 types. Completes ABC key signature support.

**Deliverables:**
1. Parse explicit accidentals after key signature
2. Handle special keys like `K:Hp`
3. Populate `Key.explicit_accidentals` field

**Definition of Done:**
```bash
cargo fmt --check
cargo clippy -- -D warnings
cargo check -p abc
cargo test -p abc
```

## Out of Scope

- MIDI conversion changes for explicit accidentals

---

## ABC Key Syntax

```
K: <key> [exp] [<accidentals>] [clef=<clef>]

<key> := <note><accidental>?<mode>?
<accidentals> := (<accidental><note>)+
<accidental> := ^ (sharp) | = (natural) | _ (flat) | ^^ | __
```

Examples:
- `K:G` → G major
- `K:Am` → A minor
- `K:Bb` → Bb major
- `K:C exp ^f =c` → C major with F# and C natural
- `K:Am ^g` → A minor with G# (melodic minor ascending)
- `K:Hp` → Highland bagpipe preset

---

## Current Parser State

Read `crates/abc/src/parser/key.rs` to understand current implementation.

The `Key` struct already has:
```rust
pub struct Key {
    pub root: NoteName,
    pub accidental: Option<Accidental>,
    pub mode: Mode,
    pub explicit_accidentals: Vec<(Accidental, NoteName)>, // <- Needs populating
    pub clef: Option<Clef>,
}
```

---

## Acceptance Criteria

- [ ] `K:Hp` parses to Highland bagpipe preset
- [ ] `K:C exp ^f =c` parses with `explicit_accidentals = [(Sharp, F), (Natural, C)]`
- [ ] `K:Am ^g` parses to A minor with `explicit_accidentals = [(Sharp, G)]`
- [ ] Invalid accidental syntax returns helpful feedback
