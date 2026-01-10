# 02: Code Generation Setup

**Files:** `crates/hooteproto/build.rs`, `crates/hooteproto/Cargo.toml`
**Focus:** Build system only
**Dependencies:** 01-schemas
**Unblocks:** 03-frame-protocol, 04-helpers

---

## Task

Set up Cap'n Proto code generation in hooteproto crate.

**Deliverables:**
1. `build.rs` that compiles all `.capnp` schemas
2. Updated `Cargo.toml` with capnp dependencies
3. Generated Rust code compiles

**Definition of Done:**
```bash
cargo build -p hooteproto
```

## Out of Scope

- ❌ Modifying existing serde types — keep them for now
- ❌ Using generated types — that's later tasks

---

## Cargo.toml Changes

```toml
[dependencies]
capnp = "0.19"
# Keep existing deps for now — we'll remove msgpack in task 08

[build-dependencies]
capnpc = "0.19"
```

---

## build.rs

```rust
fn main() {
    // Rerun if schemas change
    println!("cargo:rerun-if-changed=schemas/");

    capnpc::CompilerCommand::new()
        .src_prefix("schemas")
        .file("schemas/common.capnp")
        .file("schemas/envelope.capnp")
        .file("schemas/tools.capnp")
        .file("schemas/garden.capnp")
        .file("schemas/broadcast.capnp")
        .file("schemas/jobs.capnp")
        .run()
        .expect("capnp compile failed");
}
```

---

## Module Structure

Create `src/capnp_gen.rs`:

```rust
#![allow(clippy::all)]
#![allow(dead_code)]

pub mod common_capnp {
    include!(concat!(env!("OUT_DIR"), "/common_capnp.rs"));
}

pub mod envelope_capnp {
    include!(concat!(env!("OUT_DIR"), "/envelope_capnp.rs"));
}

pub mod tools_capnp {
    include!(concat!(env!("OUT_DIR"), "/tools_capnp.rs"));
}

pub mod garden_capnp {
    include!(concat!(env!("OUT_DIR"), "/garden_capnp.rs"));
}

pub mod broadcast_capnp {
    include!(concat!(env!("OUT_DIR"), "/broadcast_capnp.rs"));
}

pub mod jobs_capnp {
    include!(concat!(env!("OUT_DIR"), "/jobs_capnp.rs"));
}
```

Add to `lib.rs`:
```rust
pub mod capnp_gen;
```

---

## Acceptance Criteria

- [ ] `cargo build -p hooteproto` succeeds
- [ ] Generated modules are accessible via `hooteproto::capnp_gen::*`
- [ ] No clippy warnings from build.rs
