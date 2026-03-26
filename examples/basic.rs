use std::ffi::{CStr, CString};
use std::ptr;
use zvec_rs::*;

unsafe fn check(code: ZVecErrorCode, context: &str) -> bool {
    if code == ZVecErrorCode_ZVEC_OK {
        return true;
    }
    let mut err_msg: *mut std::os::raw::c_char = ptr::null_mut();
    unsafe { zvec_get_last_error(&mut err_msg) };
    let msg = if !err_msg.is_null() {
        let s = unsafe { CStr::from_ptr(err_msg) }
            .to_string_lossy()
            .into_owned();
        unsafe { zvec_free_ptr(err_msg as *mut _) };
        s
    } else {
        "unknown".to_string()
    };
    eprintln!("Error in {context}: {code} - {msg}");
    false
}

unsafe fn create_simple_test_collection(collection: &mut *mut ZVecCollection) -> ZVecErrorCode {
    let schema_name = CString::new("test_collection").unwrap();
    let schema = unsafe { zvec_collection_schema_create(schema_name.as_ptr()) };
    if schema.is_null() {
        return ZVecErrorCode_ZVEC_ERROR_INTERNAL_ERROR;
    }

    // Invert index params for string fields
    let invert_params = unsafe { zvec_index_params_create(ZVecIndexType_ZVEC_INDEX_TYPE_INVERT) };
    if invert_params.is_null() {
        unsafe { zvec_collection_schema_destroy(schema) };
        return ZVecErrorCode_ZVEC_ERROR_RESOURCE_EXHAUSTED;
    }
    unsafe { zvec_index_params_set_invert_params(invert_params, true, false) };

    // HNSW params for embedding field
    let hnsw_params = unsafe { zvec_index_params_create(ZVecIndexType_ZVEC_INDEX_TYPE_HNSW) };
    if hnsw_params.is_null() {
        unsafe { zvec_index_params_destroy(invert_params) };
        unsafe { zvec_collection_schema_destroy(schema) };
        return ZVecErrorCode_ZVEC_ERROR_RESOURCE_EXHAUSTED;
    }
    unsafe {
        zvec_index_params_set_metric_type(hnsw_params, ZVecMetricType_ZVEC_METRIC_TYPE_COSINE)
    };
    unsafe { zvec_index_params_set_hnsw_params(hnsw_params, 16, 200) };

    // Field: id (STRING, primary key, invert index)
    let id_name = CString::new("id").unwrap();
    let id_field = unsafe {
        zvec_field_schema_create(
            id_name.as_ptr(),
            ZVecDataType_ZVEC_DATA_TYPE_STRING,
            false,
            0,
        )
    };
    unsafe { zvec_field_schema_set_index_params(id_field, invert_params) };
    let rc = unsafe { zvec_collection_schema_add_field(schema, id_field) };
    if rc != ZVecErrorCode_ZVEC_OK {
        unsafe { zvec_index_params_destroy(invert_params) };
        unsafe { zvec_index_params_destroy(hnsw_params) };
        unsafe { zvec_collection_schema_destroy(schema) };
        return rc;
    }

    // Field: text (STRING, forward field, invert index)
    let text_name = CString::new("text").unwrap();
    let text_field = unsafe {
        zvec_field_schema_create(
            text_name.as_ptr(),
            ZVecDataType_ZVEC_DATA_TYPE_STRING,
            true,
            0,
        )
    };
    unsafe { zvec_field_schema_set_index_params(text_field, invert_params) };
    let rc = unsafe { zvec_collection_schema_add_field(schema, text_field) };
    if rc != ZVecErrorCode_ZVEC_OK {
        unsafe { zvec_index_params_destroy(invert_params) };
        unsafe { zvec_index_params_destroy(hnsw_params) };
        unsafe { zvec_collection_schema_destroy(schema) };
        return rc;
    }

    // Field: embedding (VECTOR_FP32, dim=3, HNSW index)
    let emb_name = CString::new("embedding").unwrap();
    let emb_field = unsafe {
        zvec_field_schema_create(
            emb_name.as_ptr(),
            ZVecDataType_ZVEC_DATA_TYPE_VECTOR_FP32,
            false,
            3,
        )
    };
    unsafe { zvec_field_schema_set_index_params(emb_field, hnsw_params) };
    let rc = unsafe { zvec_collection_schema_add_field(schema, emb_field) };
    if rc != ZVecErrorCode_ZVEC_OK {
        unsafe { zvec_index_params_destroy(invert_params) };
        unsafe { zvec_index_params_destroy(hnsw_params) };
        unsafe { zvec_collection_schema_destroy(schema) };
        return rc;
    }

    unsafe { zvec_index_params_destroy(invert_params) };
    unsafe { zvec_index_params_destroy(hnsw_params) };

    // Default options
    let options = unsafe { zvec_collection_options_create() };
    if options.is_null() {
        unsafe { zvec_collection_schema_destroy(schema) };
        return ZVecErrorCode_ZVEC_ERROR_RESOURCE_EXHAUSTED;
    }

    let path = CString::new("./test_collection").unwrap();
    let rc = unsafe { zvec_collection_create_and_open(path.as_ptr(), schema, options, collection) };

    unsafe { zvec_collection_options_destroy(options) };
    unsafe { zvec_collection_schema_destroy(schema) };
    rc
}

fn main() {
    unsafe {
        println!("=== ZVec Rust FFI Basic Example ===\n");

        // Create collection
        let mut collection: *mut ZVecCollection = ptr::null_mut();
        let rc = create_simple_test_collection(&mut collection);
        if !check(rc, "creating collection") {
            std::process::exit(1);
        }
        println!("✓ Collection created successfully");

        // Prepare test data
        let vector1: [f32; 3] = [0.1, 0.2, 0.3];
        let vector2: [f32; 3] = [0.4, 0.5, 0.6];

        let id_name = CString::new("id").unwrap();
        let text_name = CString::new("text").unwrap();
        let emb_name = CString::new("embedding").unwrap();

        let doc1 = zvec_doc_create();
        let doc2 = zvec_doc_create();
        assert!(
            !doc1.is_null() && !doc2.is_null(),
            "Failed to create documents"
        );

        // Document 1
        let pk1 = CString::new("doc1").unwrap();
        let text1 = CString::new("First document").unwrap();
        zvec_doc_set_pk(doc1, pk1.as_ptr());
        zvec_doc_add_field_by_value(
            doc1,
            id_name.as_ptr(),
            ZVecDataType_ZVEC_DATA_TYPE_STRING,
            pk1.as_ptr() as *const _,
            4,
        );
        zvec_doc_add_field_by_value(
            doc1,
            text_name.as_ptr(),
            ZVecDataType_ZVEC_DATA_TYPE_STRING,
            text1.as_ptr() as *const _,
            14,
        );
        zvec_doc_add_field_by_value(
            doc1,
            emb_name.as_ptr(),
            ZVecDataType_ZVEC_DATA_TYPE_VECTOR_FP32,
            vector1.as_ptr() as *const _,
            3 * std::mem::size_of::<f32>(),
        );

        // Document 2
        let pk2 = CString::new("doc2").unwrap();
        let text2 = CString::new("Second document").unwrap();
        zvec_doc_set_pk(doc2, pk2.as_ptr());
        zvec_doc_add_field_by_value(
            doc2,
            id_name.as_ptr(),
            ZVecDataType_ZVEC_DATA_TYPE_STRING,
            pk2.as_ptr() as *const _,
            4,
        );
        zvec_doc_add_field_by_value(
            doc2,
            text_name.as_ptr(),
            ZVecDataType_ZVEC_DATA_TYPE_STRING,
            text2.as_ptr() as *const _,
            15,
        );
        zvec_doc_add_field_by_value(
            doc2,
            emb_name.as_ptr(),
            ZVecDataType_ZVEC_DATA_TYPE_VECTOR_FP32,
            vector2.as_ptr() as *const _,
            3 * std::mem::size_of::<f32>(),
        );

        // Insert documents
        let doc_ptrs: [*const ZVecDoc; 2] = [doc1 as *const _, doc2 as *const _];
        let mut success_count: usize = 0;
        let mut error_count: usize = 0;
        let rc = zvec_collection_insert(
            collection,
            doc_ptrs.as_ptr() as *mut *const _,
            2,
            &mut success_count,
            &mut error_count,
        );
        if !check(rc, "inserting documents") {
            zvec_collection_destroy(collection);
            std::process::exit(1);
        }
        println!("✓ Documents inserted - Success: {success_count}, Failed: {error_count}");
        zvec_doc_destroy(doc1);
        zvec_doc_destroy(doc2);

        // Flush
        let rc = zvec_collection_flush(collection);
        if !check(rc, "flushing collection") {
            println!("Collection flush failed");
        } else {
            println!("✓ Collection flushed successfully");
        }

        // Stats
        let mut stats: *mut ZVecCollectionStats = ptr::null_mut();
        let rc = zvec_collection_get_stats(collection, &mut stats);
        if check(rc, "getting collection stats") {
            let count = zvec_collection_stats_get_doc_count(stats);
            println!("✓ Collection stats - Document count: {count}");
            zvec_collection_stats_destroy(stats);
        }

        // Vector query
        println!("Testing vector query...");
        let query = zvec_vector_query_create();
        if query.is_null() {
            eprintln!("Failed to create vector query");
            zvec_collection_destroy(collection);
            std::process::exit(1);
        }

        let emb_field = CString::new("embedding").unwrap();
        let empty_filter = CString::new("").unwrap();
        zvec_vector_query_set_field_name(query, emb_field.as_ptr());
        zvec_vector_query_set_query_vector(
            query,
            vector1.as_ptr() as *const _,
            3 * std::mem::size_of::<f32>(),
        );
        zvec_vector_query_set_topk(query, 10);
        zvec_vector_query_set_filter(query, empty_filter.as_ptr());
        zvec_vector_query_set_include_vector(query, true);
        zvec_vector_query_set_include_doc_id(query, true);

        let mut results: *mut *mut ZVecDoc = ptr::null_mut();
        let mut result_count: usize = 0;
        let rc = zvec_collection_query(
            collection,
            query as *const _,
            &mut results,
            &mut result_count,
        );

        if rc != ZVecErrorCode_ZVEC_OK {
            let mut err_msg: *mut std::os::raw::c_char = ptr::null_mut();
            zvec_get_last_error(&mut err_msg);
            let msg = if !err_msg.is_null() {
                let s = CStr::from_ptr(err_msg).to_string_lossy().into_owned();
                zvec_free_ptr(err_msg as *mut _);
                s
            } else {
                "Unknown error".to_string()
            };
            println!("[ERROR] Query failed: {msg}");
            zvec_vector_query_destroy(query);
            zvec_collection_destroy(collection);
            println!("✓ Example completed");
            return;
        }

        zvec_vector_query_destroy(query);
        println!("✓ Query successful - Returned {result_count} results");

        // Process results
        for i in 0..result_count.min(5) {
            let doc = *results.add(i);
            let pk = zvec_doc_get_pk_copy(doc);
            let pk_str = if !pk.is_null() {
                let s = CStr::from_ptr(pk).to_string_lossy().into_owned();
                zvec_free_ptr(pk as *mut _);
                s
            } else {
                "NULL".to_string()
            };
            let doc_id = zvec_doc_get_doc_id(doc);
            let score = zvec_doc_get_score(doc);
            println!(
                "  Result {}: PK={pk_str}, DocID={doc_id}, Score={score:.4}",
                i + 1
            );
        }

        zvec_docs_free(results, result_count);

        // Cleanup
        zvec_collection_destroy(collection);
        println!("✓ Example completed");
    }
}
