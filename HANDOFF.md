# Handoff Notes

## What lives here

This repository now contains two Rust crates:

- `zvec`: the raw bindgen-based FFI crate at the repository root
- `zvec-rs`: the safe wrapper crate in `zvec-rs/`

The wrapper is the internal Zephyr API that powered production usage. It was imported into this repo so Alibaba can continue from working Rust bindings instead of starting from the C API alone.

## Release Ownership

- The existing GitHub release workflow is still anchored on the root `zvec` crate.
- Release artifacts are built from `alibaba/zvec` and published as prebuilt `zvec_c_api` archives.
- The workflow now explicitly reads the version from the root `zvec` package so adding workspace members does not affect release automation.

## Safe Wrapper Scope

The wrapper intentionally exposes the API surface already used internally:

- `Collection`
- `SearchResult`
- `WriteResult`
- `CollectionStats`
- `builder` helpers

It depends on the in-repo raw crate through a path dependency, so both crates stay aligned to the same generated bindings and prebuilt binary artifacts.

## Notable Cleanup During Import

- Removed the internal Tauri-specific `build.rs` from the safe wrapper
- Kept the raw root crate behavior intact for existing users
- Added repository-level documentation so external maintainers can understand the two-layer layout quickly

## Suggested Next Steps

- Decide whether `zvec-rs` should eventually publish independently to crates.io or remain Git-only.
- Add more end-to-end examples once the Alibaba team confirms the intended public API shape.
- If the raw header evolves materially, re-run wrapper validation before cutting the next release PR.
