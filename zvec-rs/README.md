# `zvec-rs` Safe Wrapper

`zvec-rs` is the safe Rust layer on top of the raw `zvec` FFI crate in the repository root.

It keeps the underlying C API available, but replaces pointer-heavy usage with:

- JSON schema builders
- JSON document builders
- typed query and write results
- safe collection lifecycle helpers

## Example

```rust
use serde_json::json;
use zvec_rs::{Collection, builder};

fn main() -> zvec_rs::Result<()> {
    let schema = builder::collection_schema("docs", vec![
        builder::vector_field("embedding", "VECTOR_FP32", 4, "COSINE", "FLAT"),
        builder::scalar_field_indexed("title", "STRING", false),
    ]);

    let collection = Collection::create_and_open("./docs.zvdb", &schema)?;

    collection.upsert(&[builder::doc(
        "doc-1",
        json!({
            "embedding": [0.1_f32, 0.2_f32, 0.3_f32, 0.4_f32],
            "title": "hello",
        }),
    )])?;

    collection.flush()?;
    Ok(())
}
```

## Useful Entry Points

- `Collection::create_and_open`
- `Collection::open`
- `Collection::open_read_only`
- `Collection::insert`
- `Collection::upsert`
- `Collection::query`
- `Collection::fetch`
- `builder::*`

## Local Validation

```bash
cargo test -p zvec-rs --all-features
cargo run -p zvec-rs --example wrapper-basic
```
