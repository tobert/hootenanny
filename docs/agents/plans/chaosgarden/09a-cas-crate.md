# 09a-cas-crate: Shared Content Addressable Storage

**Prerequisite for**: 10-hootenanny-zmq
**Status**: ✅ Complete

## Goal

Extract CAS into a shared crate so hootenanny, chaosgarden, and workers can all access the same content store. Works with any shared filesystem (e.g. `/tank`).

## Implementation Summary

Created `crates/cas/` with:
- `hash.rs`: `ContentHash` newtype with BLAKE3 hashing (128-bit truncated, 32 hex chars)
- `config.rs`: `CasConfig` with env var support (`HALFREMEMBERED_CAS_PATH`, `HALFREMEMBERED_CAS_READONLY`)
- `metadata.rs`: `CasMetadata` and `CasReference` types
- `store.rs`: `ContentStore` trait and `FileStore` implementation

Updated consumers:
- **hootenanny**: `cas.rs` now re-exports from shared crate, thin wrapper for backwards compat
- **chaosgarden**: `FileCasClient` now wraps `cas::FileStore` in read-only mode

## Acceptance Criteria

- [x] `ContentHash::from_data()` produces same hash as current hootenanny
- [x] `FileStore` passes all existing hootenanny CAS tests
- [x] `CasConfig::from_env()` respects `HALFREMEMBERED_CAS_PATH`
- [x] hootenanny compiles with `cas` dependency
- [x] chaosgarden compiles with `cas` dependency
- [x] Demo works with shared CAS path
- [ ] NFS scenario: write on machine A, read on machine B (manual test - untested)

## Design Decisions

- **TOML** for config files (standard in Rust ecosystem)
- **Metadata optional for reads** - chaosgarden works without `.json` sidecar files
- Path layout: `{base}/objects/{prefix}/{remainder}` with metadata in `{base}/metadata/{prefix}/{remainder}.json`

## Test Results

```
cargo test -p cas: 30 passed
cargo test -p hootenanny --lib: 32 passed
cargo test -p chaosgarden: 158 passed
```

## Signoff

- 🤖 Claude: 2025-12-11
