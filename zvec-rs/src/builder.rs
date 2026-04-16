//! JSON builder helpers.
//!
//! These functions construct the JSON values expected by [`Collection`]'s
//! methods without requiring callers to hand-write JSON strings.
//!
//! [`Collection`]: crate::Collection

use serde_json::{json, Value};

// ── Schema builders ──────────────────────────────────────────────────────────

/// Build a dense-vector field schema object.
///
/// `data_type` is e.g. `"VECTOR_FP32"`, `"VECTOR_FP16"`, `"VECTOR_INT8"`.
/// `metric` is one of `"L2"`, `"IP"`, `"COSINE"`, `"MIPSL2"`.
/// `index_type` is one of `"HNSW"`, `"IVF"`, `"FLAT"`.
pub fn vector_field(
    name: &str,
    data_type: &str,
    dimension: u32,
    metric: &str,
    index_type: &str,
) -> Value {
    json!({
        "name": name,
        "data_type": data_type,
        "dimension": dimension,
        "nullable": false,
        "index": { "type": index_type, "metric": metric }
    })
}

/// Build a dense-vector field with HNSW index and custom parameters.
pub fn hnsw_field(
    name: &str,
    data_type: &str,
    dimension: u32,
    metric: &str,
    m: u32,
    ef_construction: u32,
) -> Value {
    json!({
        "name": name,
        "data_type": data_type,
        "dimension": dimension,
        "nullable": false,
        "index": {
            "type": "HNSW",
            "metric": metric,
            "m": m,
            "ef_construction": ef_construction
        }
    })
}

/// Build a scalar field schema object without an index.
pub fn scalar_field(name: &str, data_type: &str, nullable: bool) -> Value {
    json!({
        "name": name,
        "data_type": data_type,
        "nullable": nullable
    })
}

/// Build a scalar field schema object with an inverted index (enables
/// filtered queries on this field).
pub fn scalar_field_indexed(name: &str, data_type: &str, nullable: bool) -> Value {
    json!({
        "name": name,
        "data_type": data_type,
        "nullable": nullable,
        "index": { "type": "INVERT" }
    })
}

/// Serialize a collection schema to a JSON string ready for
/// [`Collection::create_and_open`].
///
/// [`Collection::create_and_open`]: crate::Collection::create_and_open
pub fn collection_schema(name: &str, fields: Vec<Value>) -> String {
    serde_json::to_string(&json!({
        "name": name,
        "fields": fields
    }))
    .expect("schema serialization is infallible")
}

// ── Document builders ────────────────────────────────────────────────────────

/// Build a document value with a primary key and a map of field values.
///
/// ```
/// use zvec_rs::builder;
/// use serde_json::json;
///
/// let doc = builder::doc("id_1", json!({
///     "embedding": [0.1_f32, 0.2, 0.3, 0.4],
///     "title": "hello world"
/// }));
/// ```
pub fn doc(pk: &str, fields: Value) -> Value {
    json!({ "pk": pk, "fields": fields })
}

// ── Query builders ───────────────────────────────────────────────────────────

/// Build a basic vector similarity query.
pub fn vector_query(field_name: &str, vector: &[f32], topk: u32) -> Value {
    json!({
        "field_name": field_name,
        "vector": vector,
        "topk": topk
    })
}

/// Build a vector similarity query with a scalar pre-filter.
///
/// `filter` is a SQL-like expression, e.g. `"category = 'tech'"`.
/// The filtered field must have an inverted index.
pub fn vector_query_with_filter(
    field_name: &str,
    vector: &[f32],
    topk: u32,
    filter: &str,
) -> Value {
    json!({
        "field_name": field_name,
        "vector": vector,
        "topk": topk,
        "filter": filter
    })
}

/// Build a vector query that also returns the stored vector values.
pub fn vector_query_include_vector(field_name: &str, vector: &[f32], topk: u32) -> Value {
    json!({
        "field_name": field_name,
        "vector": vector,
        "topk": topk,
        "include_vector": true
    })
}

/// Build a vector query that returns only specific output fields.
pub fn vector_query_select(
    field_name: &str,
    vector: &[f32],
    topk: u32,
    output_fields: &[&str],
) -> Value {
    json!({
        "field_name": field_name,
        "vector": vector,
        "topk": topk,
        "output_fields": output_fields
    })
}

/// Build a vector query with a scalar pre-filter that returns only specific
/// output fields.
pub fn vector_query_select_with_filter(
    field_name: &str,
    vector: &[f32],
    topk: u32,
    filter: &str,
    output_fields: &[&str],
) -> Value {
    json!({
        "field_name": field_name,
        "vector": vector,
        "topk": topk,
        "filter": filter,
        "output_fields": output_fields
    })
}

/// Build a sparse vector similarity query.
///
/// `indices` and `values` must be the same length and represent a sparse
/// vector in coordinate format (token index → score).  The target field must
/// be `SPARSE_VECTOR_FP32` or `SPARSE_VECTOR_FP16`.
///
/// Use with [`Collection::sparse_query`].
pub fn sparse_vector_query(field_name: &str, indices: &[u32], values: &[f32], topk: u32) -> Value {
    json!({
        "field_name": field_name,
        "indices": indices,
        "values": values,
        "topk": topk
    })
}

/// Build a sparse vector similarity query with a scalar pre-filter.
pub fn sparse_vector_query_with_filter(
    field_name: &str,
    indices: &[u32],
    values: &[f32],
    topk: u32,
    filter: &str,
) -> Value {
    json!({
        "field_name": field_name,
        "indices": indices,
        "values": values,
        "topk": topk,
        "filter": filter
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::vector_query_select_with_filter;

    #[test]
    fn vector_query_select_with_filter_includes_requested_fields() {
        let query = vector_query_select_with_filter(
            "structural_emb",
            &[0.1_f32, 0.2, 0.3],
            25,
            "file != 'excluded.rs'",
            &["symbol_id"],
        );

        assert_eq!(
            query,
            json!({
                "field_name": "structural_emb",
                "vector": [0.1_f32, 0.2_f32, 0.3_f32],
                "topk": 25,
                "filter": "file != 'excluded.rs'",
                "output_fields": ["symbol_id"]
            })
        );
    }
}
