use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::json;
use zvec_rs::{builder, Collection};

fn main() -> zvec_rs::Result<()> {
    let now_nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after UNIX_EPOCH")
        .as_nanos();
    let collection_path = PathBuf::from("target/zvec-rs-examples")
        .join(format!("basic-{}-{now_nanos}", std::process::id()));
    if let Some(parent) = collection_path.parent() {
        fs::create_dir_all(parent).expect("example data directory should be created");
    }
    let collection_path_str = collection_path.to_string_lossy().replace('\\', "/");

    let schema = builder::collection_schema(
        "articles",
        vec![
            builder::vector_field("embedding", "VECTOR_FP32", 4, "COSINE", "FLAT"),
            builder::scalar_field_indexed("title", "STRING", false),
            builder::scalar_field("body", "STRING", false),
        ],
    );

    let collection = Collection::create_and_open(&collection_path_str, &schema)?;

    let docs = vec![
        builder::doc(
            "doc-1",
            json!({
                "embedding": [0.9_f32, 0.1_f32, 0.1_f32, 0.1_f32],
                "title": "match",
                "body": "first document",
            }),
        ),
        builder::doc(
            "doc-2",
            json!({
                "embedding": [0.1_f32, 0.9_f32, 0.1_f32, 0.1_f32],
                "title": "other",
                "body": "second document",
            }),
        ),
    ];

    let write_results = collection.upsert(&docs)?;
    assert!(write_results.iter().all(|result| result.is_ok()));
    collection.flush()?;

    let results = collection.query(&builder::vector_query_select_with_filter(
        "embedding",
        &[0.8_f32, 0.1_f32, 0.1_f32, 0.1_f32],
        5,
        "title = 'match'",
        &["title", "body"],
    ))?;

    println!("Found {} result(s)", results.len());
    for result in &results {
        println!("{} -> {}", result.pk, result.score);
    }

    drop(collection);
    let _ = fs::remove_dir_all(collection_path);

    Ok(())
}
