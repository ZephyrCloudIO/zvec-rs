# `zvec-rs`

Rust bindings for [`zvec`](https://github.com/alibaba/zvec), packaged in two layers:

- `zvec`: the low-level raw FFI crate generated from `include/zvec/c_api.h`
- `zvec-rs/`: a safe, JSON-first wrapper that is easier to use from Rust applications

This layout keeps the existing raw API stable while publishing the higher-level wrapper that Zephyr has been using internally as the basis for the upstream Rust experience.

## Repository Layout

- `Cargo.toml`: workspace root and the raw `zvec` crate manifest
- `build.rs`: downloads prebuilt `zvec_c_api` artifacts and generates bindgen output
- `include/zvec/c_api.h`: vendored C API header used for bindgen
- `examples/basic.rs`: raw FFI smoke-test example
- `zvec-rs/`: safe wrapper crate with builder helpers, collection operations, and tests

## Choosing A Crate

Use `zvec` when you want direct access to the C API surface.

Use `zvec-rs` when you want:

- JSON-based schema and query builders
- safe document insert, update, delete, fetch, and query helpers
- typed `SearchResult`, `WriteResult`, and `CollectionStats` values

## Quick Start

### Raw FFI crate

```toml
[dependencies]
zvec = { git = "https://github.com/ZephyrCloudIO/zvec-rs" }
```

Run the bundled example:

```bash
cargo run -p zvec --example basic
```

### Safe wrapper crate

```toml
[dependencies]
zvec-rs = { git = "https://github.com/ZephyrCloudIO/zvec-rs", package = "zvec-rs" }
serde_json = "1"
```

Run the safe example:

```bash
cargo run -p zvec-rs --example wrapper-basic
```

See [zvec-rs/README.md](zvec-rs/README.md) for the higher-level API.

## Validation

The main local checks for this repo are:

```bash
cargo test -p zvec-rs --all-features
cargo test --workspace
cargo run -p zvec --example basic
```

Android CI keeps validating the raw crate build so release artifacts stay aligned with the upstream `alibaba/zvec` C API.

## Handoff Notes

This repository is intended to be transferable: the raw crate remains the release anchor, while the safe wrapper is versioned in-tree and can evolve with the same upstream header and binary release process.

Additional repository context for maintainers lives in [HANDOFF.md](HANDOFF.md).
