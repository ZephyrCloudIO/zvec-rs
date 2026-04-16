//! Safe Rust bindings for zvec using the opaque C API from `zvec`.

pub mod builder;

// Re-export the raw FFI bindings as an internal `ffi` module alias.
use std::{
    collections::HashMap,
    ffi::{CStr, CString},
    os::raw::{c_char, c_void},
    slice,
};

// ============================================================================
// FFI aliases
// ============================================================================
use ffi::{
    zvec_error_code_t_ZVEC_ERROR_ALREADY_EXISTS as ZVEC_ERROR_ALREADY_EXISTS,
    zvec_error_code_t_ZVEC_ERROR_FAILED_PRECONDITION as ZVEC_ERROR_FAILED_PRECONDITION,
    zvec_error_code_t_ZVEC_ERROR_INTERNAL_ERROR as ZVEC_ERROR_INTERNAL_ERROR,
    zvec_error_code_t_ZVEC_OK as ZVEC_OK, ZVEC_DATA_TYPE_ARRAY_DOUBLE, ZVEC_DATA_TYPE_ARRAY_FLOAT,
    ZVEC_DATA_TYPE_ARRAY_INT32, ZVEC_DATA_TYPE_ARRAY_INT64, ZVEC_DATA_TYPE_ARRAY_STRING,
    ZVEC_DATA_TYPE_ARRAY_UINT32, ZVEC_DATA_TYPE_ARRAY_UINT64, ZVEC_DATA_TYPE_BINARY,
    ZVEC_DATA_TYPE_BOOL, ZVEC_DATA_TYPE_DOUBLE, ZVEC_DATA_TYPE_FLOAT, ZVEC_DATA_TYPE_INT32,
    ZVEC_DATA_TYPE_INT64, ZVEC_DATA_TYPE_SPARSE_VECTOR_FP16, ZVEC_DATA_TYPE_SPARSE_VECTOR_FP32,
    ZVEC_DATA_TYPE_STRING, ZVEC_DATA_TYPE_UINT32, ZVEC_DATA_TYPE_UINT64,
    ZVEC_DATA_TYPE_VECTOR_FP16, ZVEC_DATA_TYPE_VECTOR_FP32, ZVEC_DATA_TYPE_VECTOR_FP64,
    ZVEC_DATA_TYPE_VECTOR_INT16, ZVEC_DATA_TYPE_VECTOR_INT8, ZVEC_INDEX_TYPE_FLAT,
    ZVEC_INDEX_TYPE_HNSW, ZVEC_INDEX_TYPE_INVERT, ZVEC_INDEX_TYPE_IVF, ZVEC_METRIC_TYPE_COSINE,
    ZVEC_METRIC_TYPE_IP, ZVEC_METRIC_TYPE_L2, ZVEC_METRIC_TYPE_MIPSL2, ZVEC_METRIC_TYPE_UNDEFINED,
    ZVEC_QUANTIZE_TYPE_FP16, ZVEC_QUANTIZE_TYPE_INT4, ZVEC_QUANTIZE_TYPE_INT8,
    ZVEC_QUANTIZE_TYPE_UNDEFINED,
};

type ZVecCollection = ffi::zvec_collection_t;
type ZVecCollectionSchema = ffi::zvec_collection_schema_t;
type ZVecDataType = ffi::zvec_data_type_t;
type ZVecDoc = ffi::zvec_doc_t;
type ZVecErrorCode = ffi::zvec_error_code_t;
type ZVecFieldSchema = ffi::zvec_field_schema_t;
type ZVecIndexType = ffi::zvec_index_type_t;
type ZVecMetricType = ffi::zvec_metric_type_t;
type ZVecQuantizeType = ffi::zvec_quantize_type_t;
type ZVecString = ffi::zvec_string_t;
type ZVecWriteResult = ffi::zvec_write_result_t;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use thiserror::Error;
use zvec as ffi;

// ============================================================================
// Error type
// ============================================================================

#[derive(Debug, Error)]
pub enum ZvecError {
    #[error("zvec error (code {code}): {message}")]
    Api { code: i32, message: String },

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("string contains an interior nul byte")]
    Nul(#[from] std::ffi::NulError),

    #[error("unsupported operation: {0}")]
    Unsupported(String),

    #[error("invalid data: {0}")]
    InvalidData(String),
}

pub type Result<T> = std::result::Result<T, ZvecError>;

// ============================================================================
// Public result types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub pk: String,
    pub score: f32,
    #[serde(default)]
    pub fields: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteResult {
    pub pk: String,
    pub code: i32,
    #[serde(default)]
    pub message: String,
}

impl WriteResult {
    pub fn is_ok(&self) -> bool {
        self.code == 0
    }
}

/// Per-index statistics reported by the underlying zvec collection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexStats {
    pub name: String,
    pub completeness: f32,
}

/// Aggregate statistics reported by the underlying zvec collection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionStats {
    pub doc_count: u64,
    pub indexes: Vec<IndexStats>,
}

// ============================================================================
// Internal schema model
// ============================================================================

#[derive(Debug, Clone)]
struct CollectionSchemaModel {
    fields: HashMap<String, FieldType>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FieldType {
    Binary,
    String,
    Bool,
    Int32,
    Int64,
    UInt32,
    UInt64,
    Float,
    Double,
    VectorFp16,
    VectorFp32,
    VectorFp64,
    VectorInt8,
    VectorInt16,
    SparseVectorFp16,
    SparseVectorFp32,
    ArrayString,
    ArrayInt32,
    ArrayInt64,
    ArrayUInt32,
    ArrayUInt64,
    ArrayFloat,
    ArrayDouble,
}

impl FieldType {
    fn from_type_name(name: &str) -> Option<Self> {
        match name {
            "BINARY" => Some(Self::Binary),
            "STRING" => Some(Self::String),
            "BOOL" => Some(Self::Bool),
            "INT32" => Some(Self::Int32),
            "INT64" => Some(Self::Int64),
            "UINT32" => Some(Self::UInt32),
            "UINT64" => Some(Self::UInt64),
            "FLOAT" => Some(Self::Float),
            "DOUBLE" => Some(Self::Double),
            "VECTOR_FP16" => Some(Self::VectorFp16),
            "VECTOR_FP32" => Some(Self::VectorFp32),
            "VECTOR_FP64" => Some(Self::VectorFp64),
            "VECTOR_INT8" => Some(Self::VectorInt8),
            "VECTOR_INT16" => Some(Self::VectorInt16),
            "SPARSE_VECTOR_FP16" => Some(Self::SparseVectorFp16),
            "SPARSE_VECTOR_FP32" => Some(Self::SparseVectorFp32),
            "ARRAY_STRING" => Some(Self::ArrayString),
            "ARRAY_INT32" => Some(Self::ArrayInt32),
            "ARRAY_INT64" => Some(Self::ArrayInt64),
            "ARRAY_UINT32" => Some(Self::ArrayUInt32),
            "ARRAY_UINT64" => Some(Self::ArrayUInt64),
            "ARRAY_FLOAT" => Some(Self::ArrayFloat),
            "ARRAY_DOUBLE" => Some(Self::ArrayDouble),
            _ => None,
        }
    }

    fn from_ffi_data_type(data_type: ZVecDataType) -> Option<Self> {
        match data_type {
            ZVEC_DATA_TYPE_BINARY => Some(Self::Binary),
            ZVEC_DATA_TYPE_STRING => Some(Self::String),
            ZVEC_DATA_TYPE_BOOL => Some(Self::Bool),
            ZVEC_DATA_TYPE_INT32 => Some(Self::Int32),
            ZVEC_DATA_TYPE_INT64 => Some(Self::Int64),
            ZVEC_DATA_TYPE_UINT32 => Some(Self::UInt32),
            ZVEC_DATA_TYPE_UINT64 => Some(Self::UInt64),
            ZVEC_DATA_TYPE_FLOAT => Some(Self::Float),
            ZVEC_DATA_TYPE_DOUBLE => Some(Self::Double),
            ZVEC_DATA_TYPE_VECTOR_FP16 => Some(Self::VectorFp16),
            ZVEC_DATA_TYPE_VECTOR_FP32 => Some(Self::VectorFp32),
            ZVEC_DATA_TYPE_VECTOR_FP64 => Some(Self::VectorFp64),
            ZVEC_DATA_TYPE_VECTOR_INT8 => Some(Self::VectorInt8),
            ZVEC_DATA_TYPE_VECTOR_INT16 => Some(Self::VectorInt16),
            ZVEC_DATA_TYPE_SPARSE_VECTOR_FP16 => Some(Self::SparseVectorFp16),
            ZVEC_DATA_TYPE_SPARSE_VECTOR_FP32 => Some(Self::SparseVectorFp32),
            ZVEC_DATA_TYPE_ARRAY_STRING => Some(Self::ArrayString),
            ZVEC_DATA_TYPE_ARRAY_INT32 => Some(Self::ArrayInt32),
            ZVEC_DATA_TYPE_ARRAY_INT64 => Some(Self::ArrayInt64),
            ZVEC_DATA_TYPE_ARRAY_UINT32 => Some(Self::ArrayUInt32),
            ZVEC_DATA_TYPE_ARRAY_UINT64 => Some(Self::ArrayUInt64),
            ZVEC_DATA_TYPE_ARRAY_FLOAT => Some(Self::ArrayFloat),
            ZVEC_DATA_TYPE_ARRAY_DOUBLE => Some(Self::ArrayDouble),
            _ => None,
        }
    }

    fn to_ffi_data_type(self) -> ZVecDataType {
        match self {
            Self::Binary => ZVEC_DATA_TYPE_BINARY,
            Self::String => ZVEC_DATA_TYPE_STRING,
            Self::Bool => ZVEC_DATA_TYPE_BOOL,
            Self::Int32 => ZVEC_DATA_TYPE_INT32,
            Self::Int64 => ZVEC_DATA_TYPE_INT64,
            Self::UInt32 => ZVEC_DATA_TYPE_UINT32,
            Self::UInt64 => ZVEC_DATA_TYPE_UINT64,
            Self::Float => ZVEC_DATA_TYPE_FLOAT,
            Self::Double => ZVEC_DATA_TYPE_DOUBLE,
            Self::VectorFp16 => ZVEC_DATA_TYPE_VECTOR_FP16,
            Self::VectorFp32 => ZVEC_DATA_TYPE_VECTOR_FP32,
            Self::VectorFp64 => ZVEC_DATA_TYPE_VECTOR_FP64,
            Self::VectorInt8 => ZVEC_DATA_TYPE_VECTOR_INT8,
            Self::VectorInt16 => ZVEC_DATA_TYPE_VECTOR_INT16,
            Self::SparseVectorFp16 => ZVEC_DATA_TYPE_SPARSE_VECTOR_FP16,
            Self::SparseVectorFp32 => ZVEC_DATA_TYPE_SPARSE_VECTOR_FP32,
            Self::ArrayString => ZVEC_DATA_TYPE_ARRAY_STRING,
            Self::ArrayInt32 => ZVEC_DATA_TYPE_ARRAY_INT32,
            Self::ArrayInt64 => ZVEC_DATA_TYPE_ARRAY_INT64,
            Self::ArrayUInt32 => ZVEC_DATA_TYPE_ARRAY_UINT32,
            Self::ArrayUInt64 => ZVEC_DATA_TYPE_ARRAY_UINT64,
            Self::ArrayFloat => ZVEC_DATA_TYPE_ARRAY_FLOAT,
            Self::ArrayDouble => ZVEC_DATA_TYPE_ARRAY_DOUBLE,
        }
    }
}

#[derive(Debug, Deserialize)]
struct SchemaJson {
    #[allow(dead_code)]
    name: String,
    fields: Vec<FieldSchemaJson>,
}

#[derive(Debug, Deserialize)]
struct FieldSchemaJson {
    name: String,
    data_type: String,
    #[serde(default)]
    nullable: bool,
    #[serde(default)]
    dimension: Option<u32>,
    #[serde(default)]
    index: Option<IndexSchemaJson>,
}

#[derive(Debug, Deserialize)]
struct IndexSchemaJson {
    #[serde(rename = "type")]
    index_type: String,
    #[serde(default)]
    metric: Option<String>,
    #[serde(default)]
    quantize: Option<String>,
    #[serde(default)]
    m: Option<i32>,
    #[serde(default)]
    ef_construction: Option<i32>,
    #[serde(default)]
    n_list: Option<i32>,
    #[serde(default)]
    n_iters: Option<i32>,
    #[serde(default)]
    use_soar: Option<bool>,
    #[serde(default)]
    enable_range_optimization: Option<bool>,
    #[serde(default)]
    enable_extended_wildcard: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct QueryJson {
    field_name: String,
    #[serde(default)]
    vector: Option<Vec<f32>>,
    #[serde(default)]
    indices: Option<Vec<u32>>,
    #[serde(default)]
    values: Option<Vec<f32>>,
    topk: u32,
    #[serde(default)]
    filter: Option<String>,
    #[serde(default)]
    include_vector: Option<bool>,
    #[serde(default)]
    output_fields: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IndexParamsJson {
    #[serde(rename = "type")]
    index_type: String,
    #[serde(default)]
    metric: Option<String>,
    #[serde(default)]
    quantize: Option<String>,
    #[serde(default)]
    m: Option<i32>,
    #[serde(default)]
    ef_construction: Option<i32>,
    #[serde(default)]
    n_list: Option<i32>,
    #[serde(default)]
    n_iters: Option<i32>,
    #[serde(default)]
    use_soar: Option<bool>,
    #[serde(default)]
    enable_range_optimization: Option<bool>,
    #[serde(default)]
    enable_extended_wildcard: Option<bool>,
}

// ============================================================================
// Helpers
// ============================================================================

const OPEN_READ_ONLY_MAX_RETRIES: usize = 20;
const OPEN_READ_ONLY_RETRY_DELAY_MS: u64 = 25;

fn ensure_initialized() -> Result<()> {
    if unsafe { ffi::zvec_is_initialized() } {
        return Ok(());
    }

    let code = unsafe { ffi::zvec_initialize(std::ptr::null()) };
    if code != ZVEC_OK && code != ZVEC_ERROR_ALREADY_EXISTS {
        return check_code(code);
    }

    Ok(())
}

fn check_code(code: ZVecErrorCode) -> Result<()> {
    if code == ZVEC_OK {
        return Ok(());
    }

    let mut message_ptr = std::ptr::null_mut();
    let _ = unsafe { ffi::zvec_get_last_error(&mut message_ptr) };
    let message = take_owned_c_string(message_ptr);

    Err(ZvecError::Api {
        code: code as i32,
        message: if message.is_empty() {
            "unknown zvec error".to_string()
        } else {
            message
        },
    })
}

fn take_owned_c_string(ptr: *mut c_char) -> String {
    if ptr.is_null() {
        return String::new();
    }

    let string = unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned();
    unsafe { ffi::zvec_free(ptr as *mut c_void) };
    string
}

fn c_ptr_to_string(ptr: *const c_char) -> String {
    if ptr.is_null() {
        return String::new();
    }
    unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned()
}

fn metric_from_name(name: Option<&str>) -> ZVecMetricType {
    match name.unwrap_or("L2") {
        "L2" => ZVEC_METRIC_TYPE_L2,
        "IP" => ZVEC_METRIC_TYPE_IP,
        "COSINE" => ZVEC_METRIC_TYPE_COSINE,
        "MIPSL2" => ZVEC_METRIC_TYPE_MIPSL2,
        _ => ZVEC_METRIC_TYPE_UNDEFINED,
    }
}

fn quantize_from_name(name: Option<&str>) -> ZVecQuantizeType {
    match name.unwrap_or("UNDEFINED") {
        "FP16" => ZVEC_QUANTIZE_TYPE_FP16,
        "INT8" => ZVEC_QUANTIZE_TYPE_INT8,
        "INT4" => ZVEC_QUANTIZE_TYPE_INT4,
        _ => ZVEC_QUANTIZE_TYPE_UNDEFINED,
    }
}

fn index_type_from_name(name: &str) -> std::result::Result<ZVecIndexType, ZvecError> {
    match name {
        "HNSW" => Ok(ZVEC_INDEX_TYPE_HNSW),
        "FLAT" => Ok(ZVEC_INDEX_TYPE_FLAT),
        "IVF" => Ok(ZVEC_INDEX_TYPE_IVF),
        "INVERT" => Ok(ZVEC_INDEX_TYPE_INVERT),
        unsupported => Err(ZvecError::Unsupported(format!(
            "unsupported index type: {unsupported}"
        ))),
    }
}

fn parse_schema_json(schema_json: &str) -> Result<SchemaJson> {
    Ok(serde_json::from_str(schema_json)?)
}

fn build_schema_model(schema: &SchemaJson) -> Result<CollectionSchemaModel> {
    let mut fields = HashMap::with_capacity(schema.fields.len());
    for field in &schema.fields {
        let field_type = FieldType::from_type_name(&field.data_type).ok_or_else(|| {
            ZvecError::Unsupported(format!("unsupported field data type: {}", field.data_type))
        })?;
        fields.insert(field.name.clone(), field_type);
    }

    Ok(CollectionSchemaModel { fields })
}

fn create_field_schema(field: &FieldSchemaJson) -> Result<*mut ZVecFieldSchema> {
    let field_type = FieldType::from_type_name(&field.data_type).ok_or_else(|| {
        ZvecError::Unsupported(format!("unsupported field data type: {}", field.data_type))
    })?;

    let name_c = CString::new(field.name.as_str())?;
    let field_ptr = unsafe {
        ffi::zvec_field_schema_create(
            name_c.as_ptr(),
            field_type.to_ffi_data_type(),
            field.nullable,
            field.dimension.unwrap_or(0),
        )
    };

    if field_ptr.is_null() {
        return Err(ZvecError::Api {
            code: ZVEC_ERROR_INTERNAL_ERROR as i32,
            message: "failed to create field schema".to_string(),
        });
    }

    if let Some(index) = &field.index {
        apply_field_index(field_ptr, index)?;
    }

    Ok(field_ptr)
}

/// Build opaque `ZVecIndexParams`, configure it for the given index type, and
/// set it on `field_ptr`. The params object is destroyed after being copied
/// into the field schema.
fn apply_field_index(field_ptr: *mut ZVecFieldSchema, index: &IndexSchemaJson) -> Result<()> {
    let idx_type = index_type_from_name(&index.index_type)?;
    let params = unsafe { ffi::zvec_index_params_create(idx_type) };
    if params.is_null() {
        return Err(ZvecError::Api {
            code: ZVEC_ERROR_INTERNAL_ERROR as i32,
            message: "failed to create index params".to_string(),
        });
    }

    // Common vector-index settings (metric + quantize).
    unsafe {
        ffi::zvec_index_params_set_metric_type(params, metric_from_name(index.metric.as_deref()))
    };
    unsafe {
        ffi::zvec_index_params_set_quantize_type(
            params,
            quantize_from_name(index.quantize.as_deref()),
        )
    };

    // Type-specific tuning knobs.
    match index.index_type.as_str() {
        "HNSW" => {
            unsafe {
                ffi::zvec_index_params_set_hnsw_params(
                    params,
                    index.m.unwrap_or(16),
                    index.ef_construction.unwrap_or(200),
                )
            };
        }
        "FLAT" => {
            // No extra params for FLAT beyond metric/quantize.
        }
        "IVF" => {
            unsafe {
                ffi::zvec_index_params_set_ivf_params(
                    params,
                    index.n_list.unwrap_or(1024),
                    index.n_iters.unwrap_or(20),
                    index.use_soar.unwrap_or(false),
                )
            };
        }
        "INVERT" => {
            unsafe {
                ffi::zvec_index_params_set_invert_params(
                    params,
                    index.enable_range_optimization.unwrap_or(false),
                    index.enable_extended_wildcard.unwrap_or(false),
                )
            };
        }
        _ => {
            // Already validated by index_type_from_name above.
        }
    }

    // Deep-copies into field schema; safe to destroy params afterwards.
    unsafe { ffi::zvec_field_schema_set_index_params(field_ptr, params) };
    unsafe { ffi::zvec_index_params_destroy(params) };

    Ok(())
}

fn create_collection_schema(schema: &SchemaJson) -> Result<*mut ZVecCollectionSchema> {
    let name_c = CString::new(schema.name.as_str())?;
    let schema_ptr = unsafe { ffi::zvec_collection_schema_create(name_c.as_ptr()) };
    if schema_ptr.is_null() {
        return Err(ZvecError::Api {
            code: ZVEC_ERROR_INTERNAL_ERROR as i32,
            message: "failed to create collection schema".to_string(),
        });
    }

    for field in &schema.fields {
        let field_ptr = create_field_schema(field)?;
        let add_code = unsafe { ffi::zvec_collection_schema_add_field(schema_ptr, field_ptr) };
        if add_code != ZVEC_OK {
            // ownership remains ambiguous on failure; free defensively before bailing.
            unsafe { ffi::zvec_field_schema_destroy(field_ptr) };
            let _ = unsafe { ffi::zvec_collection_schema_destroy(schema_ptr) };
            check_code(add_code)?;
        }
    }

    Ok(schema_ptr)
}

fn schema_model_from_collection(
    collection_ptr: *mut ZVecCollection,
) -> Result<CollectionSchemaModel> {
    let mut schema_ptr: *mut ZVecCollectionSchema = std::ptr::null_mut();
    check_code(unsafe { ffi::zvec_collection_get_schema(collection_ptr, &mut schema_ptr) })?;

    if schema_ptr.is_null() {
        return Err(ZvecError::Api {
            code: ZVEC_ERROR_INTERNAL_ERROR as i32,
            message: "collection schema pointer is null".to_string(),
        });
    }

    // Retrieve all field names, then look up each field by name.
    let mut names: *mut *const c_char = std::ptr::null_mut();
    let mut name_count = 0usize;
    check_code(unsafe {
        ffi::zvec_collection_schema_get_all_field_names(schema_ptr, &mut names, &mut name_count)
    })?;

    let mut fields = HashMap::with_capacity(name_count);

    for idx in 0..name_count {
        let name = c_ptr_to_string(unsafe { *names.add(idx) });
        if name.is_empty() {
            continue;
        }
        let name_c = CString::new(name.as_str())?;
        let field_ptr =
            unsafe { ffi::zvec_collection_schema_get_field(schema_ptr, name_c.as_ptr()) };
        if field_ptr.is_null() {
            continue;
        }
        let data_type = unsafe { ffi::zvec_field_schema_get_data_type(field_ptr) };
        if let Some(field_type) = FieldType::from_ffi_data_type(data_type) {
            fields.insert(name, field_type);
        }
        // field_ptr is owned by schema; do NOT destroy it.
    }

    // Free the names array allocated by the C library.
    unsafe { ffi::zvec_free_str_array(names as *mut *mut c_char, name_count) };

    unsafe { ffi::zvec_collection_schema_destroy(schema_ptr) };

    Ok(CollectionSchemaModel { fields })
}

fn infer_field_type(value: &Value) -> Option<FieldType> {
    match value {
        Value::String(_) => Some(FieldType::String),
        Value::Bool(_) => Some(FieldType::Bool),
        Value::Number(number) => {
            if number.is_i64() {
                Some(FieldType::Int64)
            } else if number.is_u64() {
                Some(FieldType::UInt64)
            } else if number.is_f64() {
                Some(FieldType::Float)
            } else {
                None
            }
        }
        Value::Array(items) => {
            if items.iter().all(|item| item.is_string()) {
                Some(FieldType::ArrayString)
            } else if items.iter().all(|item| item.is_number()) {
                Some(FieldType::VectorFp32)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn required_object_field<'a>(value: &'a Value, key: &str) -> Result<&'a Value> {
    value
        .get(key)
        .ok_or_else(|| ZvecError::InvalidData(format!("missing `{key}` field")))
}

fn number_to_i32(value: &Value, field_name: &str) -> Result<i32> {
    let num = value
        .as_i64()
        .ok_or_else(|| ZvecError::InvalidData(format!("`{field_name}` must be an integer")))?;
    i32::try_from(num).map_err(|_| {
        ZvecError::InvalidData(format!("`{field_name}` out of range for INT32: {num}"))
    })
}

fn number_to_i64(value: &Value, field_name: &str) -> Result<i64> {
    value
        .as_i64()
        .ok_or_else(|| ZvecError::InvalidData(format!("`{field_name}` must be an integer")))
}

fn number_to_u32(value: &Value, field_name: &str) -> Result<u32> {
    let num = value.as_u64().ok_or_else(|| {
        ZvecError::InvalidData(format!("`{field_name}` must be a non-negative integer"))
    })?;
    u32::try_from(num).map_err(|_| {
        ZvecError::InvalidData(format!("`{field_name}` out of range for UINT32: {num}"))
    })
}

fn number_to_u64(value: &Value, field_name: &str) -> Result<u64> {
    value.as_u64().ok_or_else(|| {
        ZvecError::InvalidData(format!("`{field_name}` must be a non-negative integer"))
    })
}

fn number_to_f32(value: &Value, field_name: &str) -> Result<f32> {
    let float = value
        .as_f64()
        .ok_or_else(|| ZvecError::InvalidData(format!("`{field_name}` must be numeric")))?;
    Ok(float as f32)
}

fn number_to_f64(value: &Value, field_name: &str) -> Result<f64> {
    value
        .as_f64()
        .ok_or_else(|| ZvecError::InvalidData(format!("`{field_name}` must be numeric")))
}

fn parse_float_array(value: &Value, field_name: &str) -> Result<Vec<f32>> {
    let array = value
        .as_array()
        .ok_or_else(|| ZvecError::InvalidData(format!("`{field_name}` must be an array")))?;

    array
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            item.as_f64().map(|n| n as f32).ok_or_else(|| {
                ZvecError::InvalidData(format!("`{field_name}[{idx}]` must be a number"))
            })
        })
        .collect()
}

fn parse_f64_array(value: &Value, field_name: &str) -> Result<Vec<f64>> {
    let array = value
        .as_array()
        .ok_or_else(|| ZvecError::InvalidData(format!("`{field_name}` must be an array")))?;

    array
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            item.as_f64().ok_or_else(|| {
                ZvecError::InvalidData(format!("`{field_name}[{idx}]` must be a number"))
            })
        })
        .collect()
}

fn parse_i8_array(value: &Value, field_name: &str) -> Result<Vec<i8>> {
    let array = value
        .as_array()
        .ok_or_else(|| ZvecError::InvalidData(format!("`{field_name}` must be an array")))?;

    array
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            let integer = item.as_i64().ok_or_else(|| {
                ZvecError::InvalidData(format!("`{field_name}[{idx}]` must be an integer"))
            })?;
            i8::try_from(integer).map_err(|_| {
                ZvecError::InvalidData(format!(
                    "`{field_name}[{idx}]` out of range for i8: {integer}"
                ))
            })
        })
        .collect()
}

fn parse_i16_array(value: &Value, field_name: &str) -> Result<Vec<i16>> {
    let array = value
        .as_array()
        .ok_or_else(|| ZvecError::InvalidData(format!("`{field_name}` must be an array")))?;

    array
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            let integer = item.as_i64().ok_or_else(|| {
                ZvecError::InvalidData(format!("`{field_name}[{idx}]` must be an integer"))
            })?;
            i16::try_from(integer).map_err(|_| {
                ZvecError::InvalidData(format!(
                    "`{field_name}[{idx}]` out of range for i16: {integer}"
                ))
            })
        })
        .collect()
}

fn parse_string_array(value: &Value, field_name: &str) -> Result<Vec<String>> {
    let array = value
        .as_array()
        .ok_or_else(|| ZvecError::InvalidData(format!("`{field_name}` must be an array")))?;

    array
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            item.as_str().map(ToString::to_string).ok_or_else(|| {
                ZvecError::InvalidData(format!("`{field_name}[{idx}]` must be a string"))
            })
        })
        .collect()
}

fn build_sparse_buffer(value: &Value, field_name: &str) -> Result<Vec<u8>> {
    let object = value.as_object().ok_or_else(|| {
        ZvecError::InvalidData(format!(
            "`{field_name}` must be an object with `indices` and `values`"
        ))
    })?;

    let indices_value = object
        .get("indices")
        .ok_or_else(|| ZvecError::InvalidData(format!("`{field_name}.indices` is required")))?;
    let values_value = object
        .get("values")
        .ok_or_else(|| ZvecError::InvalidData(format!("`{field_name}.values` is required")))?;

    let indices = indices_value
        .as_array()
        .ok_or_else(|| ZvecError::InvalidData(format!("`{field_name}.indices` must be an array")))?
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            let index = item.as_u64().ok_or_else(|| {
                ZvecError::InvalidData(format!(
                    "`{field_name}.indices[{idx}]` must be an unsigned integer"
                ))
            })?;
            u32::try_from(index).map_err(|_| {
                ZvecError::InvalidData(format!(
                    "`{field_name}.indices[{idx}]` out of range for u32: {index}"
                ))
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let values = values_value
        .as_array()
        .ok_or_else(|| ZvecError::InvalidData(format!("`{field_name}.values` must be an array")))?
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            item.as_f64().map(|v| v as f32).ok_or_else(|| {
                ZvecError::InvalidData(format!("`{field_name}.values[{idx}]` must be a number"))
            })
        })
        .collect::<Result<Vec<_>>>()?;

    if indices.len() != values.len() {
        return Err(ZvecError::InvalidData(format!(
            "`{field_name}` sparse indices/values length mismatch"
        )));
    }

    let nnz = u32::try_from(indices.len())
        .map_err(|_| ZvecError::InvalidData(format!("`{field_name}` sparse vector too large")))?;

    let mut buffer = Vec::with_capacity(
        std::mem::size_of::<u32>()
            + indices.len() * std::mem::size_of::<u32>()
            + values.len() * std::mem::size_of::<f32>(),
    );

    buffer.extend_from_slice(&nnz.to_ne_bytes());
    for index in indices {
        buffer.extend_from_slice(&index.to_ne_bytes());
    }
    for value in values {
        buffer.extend_from_slice(&value.to_ne_bytes());
    }

    Ok(buffer)
}

fn build_sparse_buffer_from_raw(indices: &[u32], values: &[f32]) -> Result<Vec<u8>> {
    let nnz = u32::try_from(indices.len())
        .map_err(|_| ZvecError::InvalidData("sparse vector too large".to_string()))?;
    let mut buffer = Vec::with_capacity(
        std::mem::size_of::<u32>()
            + indices.len() * std::mem::size_of::<u32>()
            + values.len() * std::mem::size_of::<f32>(),
    );
    buffer.extend_from_slice(&nnz.to_ne_bytes());
    for index in indices {
        buffer.extend_from_slice(&index.to_ne_bytes());
    }
    for value in values {
        buffer.extend_from_slice(&value.to_ne_bytes());
    }
    Ok(buffer)
}

fn parse_null_terminated_string_array(bytes: &[u8]) -> Vec<Value> {
    let mut output = Vec::new();
    let mut start = 0usize;

    for (idx, byte) in bytes.iter().enumerate() {
        if *byte == 0 {
            let chunk = &bytes[start..idx];
            output.push(Value::String(String::from_utf8_lossy(chunk).into_owned()));
            start = idx + 1;
        }
    }

    if start < bytes.len() {
        output.push(Value::String(
            String::from_utf8_lossy(&bytes[start..]).into_owned(),
        ));
    }

    output
}

fn dangling_non_null_ptr() -> *const c_void {
    std::ptr::NonNull::<u8>::dangling()
        .as_ptr()
        .cast::<c_void>()
}

// ============================================================================
// Collection
// ============================================================================

pub struct Collection {
    ptr: *mut ZVecCollection,
    schema: CollectionSchemaModel,
    read_only: bool,
}

unsafe impl Send for Collection {}
unsafe impl Sync for Collection {}

impl Collection {
    pub fn create_and_open(path: &str, schema_json: &str) -> Result<Self> {
        ensure_initialized()?;

        let parsed_schema = parse_schema_json(schema_json)?;
        let schema_model = build_schema_model(&parsed_schema)?;
        let schema_ptr = create_collection_schema(&parsed_schema)?;

        let path_c = CString::new(path)?;
        let mut collection_ptr = std::ptr::null_mut();
        let code = unsafe {
            ffi::zvec_collection_create_and_open(
                path_c.as_ptr(),
                schema_ptr,
                std::ptr::null(),
                &mut collection_ptr,
            )
        };

        unsafe { ffi::zvec_collection_schema_destroy(schema_ptr) };
        check_code(code)?;

        Ok(Self {
            ptr: collection_ptr,
            schema: schema_model,
            read_only: false,
        })
    }

    pub fn open(path: &str) -> Result<Self> {
        ensure_initialized()?;

        let path_c = CString::new(path)?;
        let mut collection_ptr = std::ptr::null_mut();
        check_code(unsafe {
            ffi::zvec_collection_open(path_c.as_ptr(), std::ptr::null(), &mut collection_ptr)
        })?;

        let schema = schema_model_from_collection(collection_ptr)?;
        Ok(Self {
            ptr: collection_ptr,
            schema,
            read_only: false,
        })
    }

    pub fn open_read_only(path: &str) -> Result<Self> {
        ensure_initialized()?;

        let path_c = CString::new(path)?;

        // Create opaque options handle and enable read-only mode.
        let options = unsafe { ffi::zvec_collection_options_create() };
        unsafe { ffi::zvec_collection_options_set_read_only(options, true) };

        let mut collection_ptr = std::ptr::null_mut();
        for attempt in 0..OPEN_READ_ONLY_MAX_RETRIES {
            collection_ptr = std::ptr::null_mut();
            let open_code =
                unsafe { ffi::zvec_collection_open(path_c.as_ptr(), options, &mut collection_ptr) };

            match check_code(open_code) {
                Ok(()) => break,
                Err(error) => {
                    let should_retry = matches!(
                        &error,
                        ZvecError::Api { message, .. }
                            if message.contains("Can't lock read-only collection")
                                || message.contains("Can't lock read-write collection")
                    );
                    if should_retry && attempt + 1 < OPEN_READ_ONLY_MAX_RETRIES {
                        std::thread::sleep(std::time::Duration::from_millis(
                            OPEN_READ_ONLY_RETRY_DELAY_MS,
                        ));
                        continue;
                    }
                    unsafe { ffi::zvec_collection_options_destroy(options) };
                    return Err(error);
                }
            }
        }

        unsafe { ffi::zvec_collection_options_destroy(options) };

        let schema = schema_model_from_collection(collection_ptr)?;
        Ok(Self {
            ptr: collection_ptr,
            schema,
            read_only: true,
        })
    }

    /// Returns collection statistics copied out of the underlying zvec handle.
    pub fn stats(&self) -> Result<CollectionStats> {
        let mut stats_ptr = std::ptr::null_mut();
        check_code(unsafe { ffi::zvec_collection_get_stats(self.ptr, &mut stats_ptr) })?;

        if stats_ptr.is_null() {
            return Err(ZvecError::Api {
                code: ZVEC_ERROR_INTERNAL_ERROR as i32,
                message: "collection stats pointer is null".to_string(),
            });
        }

        let doc_count = unsafe { ffi::zvec_collection_stats_get_doc_count(stats_ptr) };
        let index_count = unsafe { ffi::zvec_collection_stats_get_index_count(stats_ptr) };
        let mut indexes = Vec::with_capacity(index_count);

        for index in 0..index_count {
            indexes.push(IndexStats {
                name: c_ptr_to_string(unsafe {
                    ffi::zvec_collection_stats_get_index_name(stats_ptr, index)
                }),
                completeness: unsafe {
                    ffi::zvec_collection_stats_get_index_completeness(stats_ptr, index)
                },
            });
        }

        unsafe { ffi::zvec_collection_stats_destroy(stats_ptr) };

        Ok(CollectionStats { doc_count, indexes })
    }

    /// Returns the number of documents currently tracked by the collection.
    pub fn doc_count(&self) -> Result<u64> {
        self.stats().map(|stats| stats.doc_count)
    }

    pub fn flush(&self) -> Result<()> {
        self.ensure_writable()?;
        check_code(unsafe { ffi::zvec_collection_flush(self.ptr) })
    }

    pub fn destroy(self) -> Result<()> {
        self.ensure_writable()?;

        let ptr = self.ptr;
        std::mem::forget(self);

        check_code(unsafe { ffi::zvec_collection_destroy(ptr) })?;
        check_code(unsafe { ffi::zvec_collection_close(ptr) })
    }

    pub fn insert(&self, docs: &[Value]) -> Result<Vec<WriteResult>> {
        self.ensure_writable()?;
        self.write_op(
            docs,
            |ptr, docs_ptr, count, results_ptr, result_count| unsafe {
                ffi::zvec_collection_insert_with_results(
                    ptr,
                    docs_ptr,
                    count,
                    results_ptr,
                    result_count,
                )
            },
        )
    }

    pub fn upsert(&self, docs: &[Value]) -> Result<Vec<WriteResult>> {
        self.ensure_writable()?;
        self.write_op(
            docs,
            |ptr, docs_ptr, count, results_ptr, result_count| unsafe {
                ffi::zvec_collection_upsert_with_results(
                    ptr,
                    docs_ptr,
                    count,
                    results_ptr,
                    result_count,
                )
            },
        )
    }

    pub fn update(&self, docs: &[Value]) -> Result<Vec<WriteResult>> {
        self.ensure_writable()?;
        self.write_op(
            docs,
            |ptr, docs_ptr, count, results_ptr, result_count| unsafe {
                ffi::zvec_collection_update_with_results(
                    ptr,
                    docs_ptr,
                    count,
                    results_ptr,
                    result_count,
                )
            },
        )
    }

    fn write_op(
        &self,
        docs: &[Value],
        call: impl FnOnce(
            *mut ZVecCollection,
            *mut *const ZVecDoc,
            usize,
            *mut *mut ZVecWriteResult,
            *mut usize,
        ) -> ZVecErrorCode,
    ) -> Result<Vec<WriteResult>> {
        let doc_ptrs = self.build_docs(docs)?;
        let mut doc_ptrs_const: Vec<*const ZVecDoc> =
            doc_ptrs.iter().map(|ptr| *ptr as *const ZVecDoc).collect();

        let mut results_ptr = std::ptr::null_mut();
        let mut result_count = 0usize;
        let code = call(
            self.ptr,
            doc_ptrs_const.as_mut_ptr(),
            doc_ptrs_const.len(),
            &mut results_ptr,
            &mut result_count,
        );

        for doc_ptr in &doc_ptrs {
            unsafe { ffi::zvec_doc_destroy(*doc_ptr) };
        }

        check_code(code)?;

        // ZVecWriteResult no longer carries pk; pair results with input doc pks
        // by index.
        let mut output = Vec::with_capacity(result_count);
        if !results_ptr.is_null() {
            let results = unsafe { slice::from_raw_parts(results_ptr, result_count) };
            for (idx, item) in results.iter().enumerate() {
                let pk = docs
                    .get(idx)
                    .and_then(|d| d.get("pk"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                output.push(WriteResult {
                    pk,
                    code: item.code as i32,
                    message: c_ptr_to_string(item.message),
                });
            }
            unsafe { ffi::zvec_write_results_free(results_ptr, result_count) };
        }

        Ok(output)
    }

    pub fn delete_by_pks(&self, pks: &[&str]) -> Result<Vec<WriteResult>> {
        self.ensure_writable()?;

        let pk_cstrings = pks
            .iter()
            .map(|pk| CString::new(*pk))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let pk_ptrs = pk_cstrings.iter().map(|pk| pk.as_ptr()).collect::<Vec<_>>();

        let mut results_ptr = std::ptr::null_mut();
        let mut result_count = 0usize;
        check_code(unsafe {
            ffi::zvec_collection_delete_with_results(
                self.ptr,
                pk_ptrs.as_ptr(),
                pk_ptrs.len(),
                &mut results_ptr,
                &mut result_count,
            )
        })?;

        // ZVecWriteResult no longer carries pk; pair results with input pks
        // by index.
        let mut output = Vec::with_capacity(result_count);
        if !results_ptr.is_null() {
            let results = unsafe { slice::from_raw_parts(results_ptr, result_count) };
            for (idx, item) in results.iter().enumerate() {
                output.push(WriteResult {
                    pk: pks.get(idx).unwrap_or(&"").to_string(),
                    code: item.code as i32,
                    message: c_ptr_to_string(item.message),
                });
            }
            unsafe { ffi::zvec_write_results_free(results_ptr, result_count) };
        }

        Ok(output)
    }

    pub fn delete_by_filter(&self, filter: &str) -> Result<()> {
        self.ensure_writable()?;

        let filter_c = CString::new(filter)?;
        check_code(unsafe { ffi::zvec_collection_delete_by_filter(self.ptr, filter_c.as_ptr()) })
    }

    pub fn query(&self, query: &Value) -> Result<Vec<SearchResult>> {
        self.query_like(query)
    }

    pub fn sparse_query(&self, query: &Value) -> Result<Vec<SearchResult>> {
        self.query_like(query)
    }

    fn query_like(&self, query_value: &Value) -> Result<Vec<SearchResult>> {
        let query = serde_json::from_value::<QueryJson>(query_value.clone())?;

        let field_name_c = CString::new(query.field_name.as_str())?;
        let filter_str = query.filter.unwrap_or_default();
        let include_vector = query.include_vector.unwrap_or(false);

        let dense_values = query.vector.unwrap_or_default();
        let sparse_indices = query.indices.unwrap_or_default();
        let sparse_values = query.values.unwrap_or_default();

        if !sparse_indices.is_empty() && sparse_indices.len() != sparse_values.len() {
            return Err(ZvecError::InvalidData(
                "sparse query requires indices/values length match".to_string(),
            ));
        }

        let output_fields = query.output_fields.unwrap_or_default();

        // Build opaque ZVecVectorQuery.
        let query_ptr = unsafe { ffi::zvec_vector_query_create() };
        if query_ptr.is_null() {
            return Err(ZvecError::Api {
                code: ZVEC_ERROR_INTERNAL_ERROR as i32,
                message: "failed to create vector query".to_string(),
            });
        }

        unsafe {
            ffi::zvec_vector_query_set_topk(
                query_ptr,
                i32::try_from(query.topk).unwrap_or(i32::MAX),
            )
        };
        unsafe { ffi::zvec_vector_query_set_field_name(query_ptr, field_name_c.as_ptr()) };
        unsafe { ffi::zvec_vector_query_set_include_vector(query_ptr, include_vector) };

        // Dense vector.
        if !dense_values.is_empty() {
            unsafe {
                ffi::zvec_vector_query_set_query_vector(
                    query_ptr,
                    dense_values.as_ptr().cast::<c_void>(),
                    dense_values.len() * std::mem::size_of::<f32>(),
                )
            };
        }

        // Sparse vector: encode as nnz + indices + values, pass through set_query_vector.
        if !sparse_indices.is_empty() {
            let sparse_buf = build_sparse_buffer_from_raw(&sparse_indices, &sparse_values)?;
            unsafe {
                ffi::zvec_vector_query_set_query_vector(
                    query_ptr,
                    sparse_buf.as_ptr().cast::<c_void>(),
                    sparse_buf.len(),
                )
            };
        }

        // Filter.
        if !filter_str.is_empty() {
            let filter_c = CString::new(filter_str)?;
            unsafe { ffi::zvec_vector_query_set_filter(query_ptr, filter_c.as_ptr()) };
        }

        // Output fields — API takes (*mut *const c_char, count).
        let output_cstrings = output_fields
            .iter()
            .map(|f| CString::new(f.as_str()))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        if !output_cstrings.is_empty() {
            let mut ptrs: Vec<*const c_char> = output_cstrings.iter().map(|c| c.as_ptr()).collect();
            unsafe {
                ffi::zvec_vector_query_set_output_fields(query_ptr, ptrs.as_mut_ptr(), ptrs.len())
            };
        }

        // Execute query.
        let mut docs_ptr = std::ptr::null_mut();
        let mut doc_count = 0usize;
        let code = unsafe {
            ffi::zvec_collection_query(self.ptr, query_ptr, &mut docs_ptr, &mut doc_count)
        };
        unsafe { ffi::zvec_vector_query_destroy(query_ptr) };
        check_code(code)?;

        let docs = self.convert_search_results(docs_ptr, doc_count)?;
        unsafe { ffi::zvec_docs_free(docs_ptr, doc_count) };
        Ok(docs)
    }

    pub fn fetch(&self, pks: &[&str]) -> Result<Vec<SearchResult>> {
        let pk_cstrings = pks
            .iter()
            .map(|pk| CString::new(*pk))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let pk_ptrs = pk_cstrings.iter().map(|pk| pk.as_ptr()).collect::<Vec<_>>();

        let mut docs_ptr = std::ptr::null_mut();
        let mut doc_count = 0usize;
        check_code(unsafe {
            ffi::zvec_collection_fetch(
                self.ptr,
                pk_ptrs.as_ptr(),
                pk_ptrs.len(),
                &mut docs_ptr,
                &mut doc_count,
            )
        })?;

        let docs = self.convert_search_results(docs_ptr, doc_count)?;
        unsafe { ffi::zvec_docs_free(docs_ptr, doc_count) };
        Ok(docs)
    }

    pub fn create_index(&self, column_name: &str, index_params_json: &str) -> Result<()> {
        self.ensure_writable()?;

        let index_params = serde_json::from_str::<IndexParamsJson>(index_params_json)?;
        let column_c = CString::new(column_name)?;

        let idx_type = index_type_from_name(&index_params.index_type)?;
        let params = unsafe { ffi::zvec_index_params_create(idx_type) };
        if params.is_null() {
            return Err(ZvecError::Api {
                code: ZVEC_ERROR_INTERNAL_ERROR as i32,
                message: "failed to create index params".to_string(),
            });
        }

        unsafe {
            ffi::zvec_index_params_set_metric_type(
                params,
                metric_from_name(index_params.metric.as_deref()),
            )
        };
        unsafe {
            ffi::zvec_index_params_set_quantize_type(
                params,
                quantize_from_name(index_params.quantize.as_deref()),
            )
        };

        match index_params.index_type.as_str() {
            "HNSW" => {
                unsafe {
                    ffi::zvec_index_params_set_hnsw_params(
                        params,
                        index_params.m.unwrap_or(16),
                        index_params.ef_construction.unwrap_or(200),
                    )
                };
            }
            "FLAT" => {}
            "IVF" => {
                unsafe {
                    ffi::zvec_index_params_set_ivf_params(
                        params,
                        index_params.n_list.unwrap_or(1024),
                        index_params.n_iters.unwrap_or(20),
                        index_params.use_soar.unwrap_or(false),
                    )
                };
            }
            "INVERT" => {
                unsafe {
                    ffi::zvec_index_params_set_invert_params(
                        params,
                        index_params.enable_range_optimization.unwrap_or(false),
                        index_params.enable_extended_wildcard.unwrap_or(false),
                    )
                };
            }
            _ => {
                // Already validated by index_type_from_name above.
            }
        }

        let code =
            unsafe { ffi::zvec_collection_create_index(self.ptr, column_c.as_ptr(), params) };
        unsafe { ffi::zvec_index_params_destroy(params) };
        check_code(code)
    }

    pub fn drop_index(&self, column_name: &str) -> Result<()> {
        self.ensure_writable()?;

        let column_c = CString::new(column_name)?;
        check_code(unsafe { ffi::zvec_collection_drop_index(self.ptr, column_c.as_ptr()) })
    }

    pub fn add_column(&self, field_schema_json: &str, expression: &str) -> Result<()> {
        self.ensure_writable()?;

        let field_schema = serde_json::from_str::<FieldSchemaJson>(field_schema_json)?;
        let field_ptr = create_field_schema(&field_schema)?;

        let expression_c = CString::new(expression)?;
        let code =
            unsafe { ffi::zvec_collection_add_column(self.ptr, field_ptr, expression_c.as_ptr()) };
        unsafe { ffi::zvec_field_schema_destroy(field_ptr) };
        check_code(code)
    }

    pub fn drop_column(&self, column_name: &str) -> Result<()> {
        self.ensure_writable()?;

        let column_c = CString::new(column_name)?;
        check_code(unsafe { ffi::zvec_collection_drop_column(self.ptr, column_c.as_ptr()) })
    }

    pub fn alter_column(
        &self,
        column_name: &str,
        new_name: &str,
        field_schema_json: &str,
    ) -> Result<()> {
        self.ensure_writable()?;

        let column_c = CString::new(column_name)?;
        let new_name_c = if new_name.is_empty() {
            None
        } else {
            Some(CString::new(new_name)?)
        };

        let schema_ptr = if field_schema_json.trim().is_empty() {
            None
        } else {
            Some(create_field_schema(&serde_json::from_str::<
                FieldSchemaJson,
            >(field_schema_json)?)?)
        };

        let code = unsafe {
            ffi::zvec_collection_alter_column(
                self.ptr,
                column_c.as_ptr(),
                new_name_c
                    .as_ref()
                    .map(|value| value.as_ptr())
                    .unwrap_or(std::ptr::null()),
                schema_ptr.unwrap_or(std::ptr::null_mut()),
            )
        };

        if let Some(schema_ptr) = schema_ptr {
            unsafe { ffi::zvec_field_schema_destroy(schema_ptr) };
        }

        check_code(code)
    }

    fn ensure_writable(&self) -> Result<()> {
        if self.read_only {
            return Err(ZvecError::Api {
                code: ZVEC_ERROR_FAILED_PRECONDITION as i32,
                message: "operation is not allowed on read-only collection handle".to_string(),
            });
        }
        Ok(())
    }

    fn build_docs(&self, docs: &[Value]) -> Result<Vec<*mut ZVecDoc>> {
        let mut output = Vec::with_capacity(docs.len());

        for doc_json in docs {
            let doc_ptr = unsafe { ffi::zvec_doc_create() };
            if doc_ptr.is_null() {
                for created in &output {
                    unsafe { ffi::zvec_doc_destroy(*created) };
                }
                return Err(ZvecError::Api {
                    code: ZVEC_ERROR_INTERNAL_ERROR as i32,
                    message: "failed to allocate zvec doc".to_string(),
                });
            }

            let result = self.fill_doc_from_json(doc_ptr, doc_json);
            if let Err(error) = result {
                unsafe { ffi::zvec_doc_destroy(doc_ptr) };
                for created in &output {
                    unsafe { ffi::zvec_doc_destroy(*created) };
                }
                return Err(error);
            }

            output.push(doc_ptr);
        }

        Ok(output)
    }

    fn fill_doc_from_json(&self, doc_ptr: *mut ZVecDoc, doc_json: &Value) -> Result<()> {
        let pk_value = required_object_field(doc_json, "pk")?;
        let pk = pk_value
            .as_str()
            .ok_or_else(|| ZvecError::InvalidData("`pk` must be a string".to_string()))?;
        let pk_c = CString::new(pk)?;
        unsafe { ffi::zvec_doc_set_pk(doc_ptr, pk_c.as_ptr()) };

        let fields_value = required_object_field(doc_json, "fields")?;
        let fields = fields_value
            .as_object()
            .ok_or_else(|| ZvecError::InvalidData("`fields` must be a JSON object".to_string()))?;

        for (field_name, field_value) in fields {
            let field_type = self
                .schema
                .fields
                .get(field_name)
                .copied()
                .or_else(|| infer_field_type(field_value))
                .ok_or_else(|| {
                    ZvecError::InvalidData(format!(
                        "failed to determine field type for `{field_name}`"
                    ))
                })?;

            let field_name_c = CString::new(field_name.as_str())?;
            if field_value.is_null() {
                check_code(unsafe {
                    ffi::zvec_doc_set_field_null(doc_ptr, field_name_c.as_ptr())
                })?;
                continue;
            }

            self.add_non_null_field(
                doc_ptr,
                field_name,
                field_name_c.as_c_str(),
                field_type,
                field_value,
            )?;
        }

        Ok(())
    }

    fn add_non_null_field(
        &self,
        doc_ptr: *mut ZVecDoc,
        field_name: &str,
        field_name_c: &CStr,
        field_type: FieldType,
        field_value: &Value,
    ) -> Result<()> {
        match field_type {
            FieldType::Binary | FieldType::String => {
                let string_value = field_value.as_str().ok_or_else(|| {
                    ZvecError::InvalidData(format!("`{field_name}` must be a string"))
                })?;
                let bytes = string_value.as_bytes();
                check_code(unsafe {
                    ffi::zvec_doc_add_field_by_value(
                        doc_ptr,
                        field_name_c.as_ptr(),
                        field_type.to_ffi_data_type(),
                        if bytes.is_empty() {
                            dangling_non_null_ptr()
                        } else {
                            bytes.as_ptr().cast::<c_void>()
                        },
                        bytes.len(),
                    )
                })
            }
            FieldType::Bool => {
                let boolean = field_value.as_bool().ok_or_else(|| {
                    ZvecError::InvalidData(format!("`{field_name}` must be a boolean"))
                })?;
                check_code(unsafe {
                    ffi::zvec_doc_add_field_by_value(
                        doc_ptr,
                        field_name_c.as_ptr(),
                        field_type.to_ffi_data_type(),
                        (&boolean as *const bool).cast::<c_void>(),
                        std::mem::size_of::<bool>(),
                    )
                })
            }
            FieldType::Int32 => {
                let value = number_to_i32(field_value, field_name)?;
                check_code(unsafe {
                    ffi::zvec_doc_add_field_by_value(
                        doc_ptr,
                        field_name_c.as_ptr(),
                        field_type.to_ffi_data_type(),
                        (&value as *const i32).cast::<c_void>(),
                        std::mem::size_of::<i32>(),
                    )
                })
            }
            FieldType::Int64 => {
                let value = number_to_i64(field_value, field_name)?;
                check_code(unsafe {
                    ffi::zvec_doc_add_field_by_value(
                        doc_ptr,
                        field_name_c.as_ptr(),
                        field_type.to_ffi_data_type(),
                        (&value as *const i64).cast::<c_void>(),
                        std::mem::size_of::<i64>(),
                    )
                })
            }
            FieldType::UInt32 => {
                let value = number_to_u32(field_value, field_name)?;
                check_code(unsafe {
                    ffi::zvec_doc_add_field_by_value(
                        doc_ptr,
                        field_name_c.as_ptr(),
                        field_type.to_ffi_data_type(),
                        (&value as *const u32).cast::<c_void>(),
                        std::mem::size_of::<u32>(),
                    )
                })
            }
            FieldType::UInt64 => {
                let value = number_to_u64(field_value, field_name)?;
                check_code(unsafe {
                    ffi::zvec_doc_add_field_by_value(
                        doc_ptr,
                        field_name_c.as_ptr(),
                        field_type.to_ffi_data_type(),
                        (&value as *const u64).cast::<c_void>(),
                        std::mem::size_of::<u64>(),
                    )
                })
            }
            FieldType::Float => {
                let value = number_to_f32(field_value, field_name)?;
                check_code(unsafe {
                    ffi::zvec_doc_add_field_by_value(
                        doc_ptr,
                        field_name_c.as_ptr(),
                        field_type.to_ffi_data_type(),
                        (&value as *const f32).cast::<c_void>(),
                        std::mem::size_of::<f32>(),
                    )
                })
            }
            FieldType::Double => {
                let value = number_to_f64(field_value, field_name)?;
                check_code(unsafe {
                    ffi::zvec_doc_add_field_by_value(
                        doc_ptr,
                        field_name_c.as_ptr(),
                        field_type.to_ffi_data_type(),
                        (&value as *const f64).cast::<c_void>(),
                        std::mem::size_of::<f64>(),
                    )
                })
            }
            FieldType::VectorFp32 => {
                let values = parse_float_array(field_value, field_name)?;
                check_code(unsafe {
                    ffi::zvec_doc_add_field_by_value(
                        doc_ptr,
                        field_name_c.as_ptr(),
                        field_type.to_ffi_data_type(),
                        if values.is_empty() {
                            dangling_non_null_ptr()
                        } else {
                            values.as_ptr().cast::<c_void>()
                        },
                        values.len() * std::mem::size_of::<f32>(),
                    )
                })
            }
            FieldType::VectorFp64 => {
                let values = parse_f64_array(field_value, field_name)?;
                check_code(unsafe {
                    ffi::zvec_doc_add_field_by_value(
                        doc_ptr,
                        field_name_c.as_ptr(),
                        field_type.to_ffi_data_type(),
                        if values.is_empty() {
                            dangling_non_null_ptr()
                        } else {
                            values.as_ptr().cast::<c_void>()
                        },
                        values.len() * std::mem::size_of::<f64>(),
                    )
                })
            }
            FieldType::VectorInt8 => {
                let values = parse_i8_array(field_value, field_name)?;
                check_code(unsafe {
                    ffi::zvec_doc_add_field_by_value(
                        doc_ptr,
                        field_name_c.as_ptr(),
                        field_type.to_ffi_data_type(),
                        if values.is_empty() {
                            dangling_non_null_ptr()
                        } else {
                            values.as_ptr().cast::<c_void>()
                        },
                        values.len() * std::mem::size_of::<i8>(),
                    )
                })
            }
            FieldType::VectorInt16 => {
                let values = parse_i16_array(field_value, field_name)?;
                check_code(unsafe {
                    ffi::zvec_doc_add_field_by_value(
                        doc_ptr,
                        field_name_c.as_ptr(),
                        field_type.to_ffi_data_type(),
                        if values.is_empty() {
                            dangling_non_null_ptr()
                        } else {
                            values.as_ptr().cast::<c_void>()
                        },
                        values.len() * std::mem::size_of::<i16>(),
                    )
                })
            }
            FieldType::SparseVectorFp16 | FieldType::SparseVectorFp32 => {
                let buffer = build_sparse_buffer(field_value, field_name)?;
                check_code(unsafe {
                    ffi::zvec_doc_add_field_by_value(
                        doc_ptr,
                        field_name_c.as_ptr(),
                        field_type.to_ffi_data_type(),
                        if buffer.is_empty() {
                            dangling_non_null_ptr()
                        } else {
                            buffer.as_ptr().cast::<c_void>()
                        },
                        buffer.len(),
                    )
                })
            }
            FieldType::ArrayString => {
                let string_values = parse_string_array(field_value, field_name)?;
                let cstrings = string_values
                    .iter()
                    .map(|item| CString::new(item.as_str()))
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                let mut zstrings = cstrings
                    .iter()
                    .map(|item| ZVecString {
                        data: item.as_ptr() as *mut _,
                        length: item.as_bytes().len(),
                        capacity: item.as_bytes().len() + 1,
                    })
                    .collect::<Vec<_>>();
                let mut pointers = zstrings
                    .iter_mut()
                    .map(|item| item as *mut ZVecString)
                    .collect::<Vec<_>>();

                let mut null_sentinel: *mut ZVecString = std::ptr::null_mut();
                let ptr = if pointers.is_empty() {
                    (&mut null_sentinel as *mut *mut ZVecString).cast::<c_void>()
                } else {
                    pointers.as_mut_ptr().cast::<c_void>()
                };

                check_code(unsafe {
                    ffi::zvec_doc_add_field_by_value(
                        doc_ptr,
                        field_name_c.as_ptr(),
                        field_type.to_ffi_data_type(),
                        ptr,
                        pointers.len() * std::mem::size_of::<*mut ZVecString>(),
                    )
                })
            }
            FieldType::ArrayInt32
            | FieldType::ArrayInt64
            | FieldType::ArrayUInt32
            | FieldType::ArrayUInt64
            | FieldType::ArrayFloat
            | FieldType::ArrayDouble
            | FieldType::VectorFp16 => Err(ZvecError::Unsupported(format!(
                "field type currently unsupported in write conversion: {:?}",
                field_type
            ))),
        }
    }

    fn convert_search_results(
        &self,
        docs_ptr: *mut *mut ZVecDoc,
        doc_count: usize,
    ) -> Result<Vec<SearchResult>> {
        if docs_ptr.is_null() || doc_count == 0 {
            return Ok(Vec::new());
        }

        let docs = unsafe { slice::from_raw_parts(docs_ptr, doc_count) };
        let mut output = Vec::with_capacity(doc_count);

        for doc_ptr in docs {
            if doc_ptr.is_null() {
                continue;
            }

            let pk = c_ptr_to_string(unsafe { ffi::zvec_doc_get_pk_pointer(*doc_ptr) });
            let score = unsafe { ffi::zvec_doc_get_score(*doc_ptr) };
            let fields = self.extract_fields_from_doc(*doc_ptr)?;

            output.push(SearchResult { pk, score, fields });
        }

        Ok(output)
    }

    fn extract_fields_from_doc(&self, doc_ptr: *mut ZVecDoc) -> Result<Map<String, Value>> {
        let mut fields = Map::new();

        for (field_name, field_type) in &self.schema.fields {
            let field_c = CString::new(field_name.as_str())?;
            let has_field = unsafe { ffi::zvec_doc_has_field(doc_ptr, field_c.as_ptr()) };
            if !has_field {
                continue;
            }

            let is_null = unsafe { ffi::zvec_doc_is_field_null(doc_ptr, field_c.as_ptr()) };
            if is_null {
                fields.insert(field_name.clone(), Value::Null);
                continue;
            }

            if !unsafe { ffi::zvec_doc_has_field_value(doc_ptr, field_c.as_ptr()) } {
                continue;
            }

            let value =
                self.extract_field_value(doc_ptr, field_name, field_c.as_c_str(), *field_type)?;
            fields.insert(field_name.clone(), value);
        }

        Ok(fields)
    }

    fn extract_field_value(
        &self,
        doc_ptr: *mut ZVecDoc,
        field_name: &str,
        field_c: &CStr,
        field_type: FieldType,
    ) -> Result<Value> {
        match field_type {
            FieldType::Bool => {
                let mut value = false;
                check_code(unsafe {
                    ffi::zvec_doc_get_field_value_basic(
                        doc_ptr,
                        field_c.as_ptr(),
                        field_type.to_ffi_data_type(),
                        (&mut value as *mut bool).cast::<c_void>(),
                        std::mem::size_of::<bool>(),
                    )
                })?;
                Ok(Value::Bool(value))
            }
            FieldType::Int32 => {
                let mut value: i32 = 0;
                check_code(unsafe {
                    ffi::zvec_doc_get_field_value_basic(
                        doc_ptr,
                        field_c.as_ptr(),
                        field_type.to_ffi_data_type(),
                        (&mut value as *mut i32).cast::<c_void>(),
                        std::mem::size_of::<i32>(),
                    )
                })?;
                Ok(Value::Number(value.into()))
            }
            FieldType::Int64 => {
                let mut value: i64 = 0;
                check_code(unsafe {
                    ffi::zvec_doc_get_field_value_basic(
                        doc_ptr,
                        field_c.as_ptr(),
                        field_type.to_ffi_data_type(),
                        (&mut value as *mut i64).cast::<c_void>(),
                        std::mem::size_of::<i64>(),
                    )
                })?;
                Ok(Value::Number(value.into()))
            }
            FieldType::UInt32 => {
                let mut value: u32 = 0;
                check_code(unsafe {
                    ffi::zvec_doc_get_field_value_basic(
                        doc_ptr,
                        field_c.as_ptr(),
                        field_type.to_ffi_data_type(),
                        (&mut value as *mut u32).cast::<c_void>(),
                        std::mem::size_of::<u32>(),
                    )
                })?;
                Ok(Value::Number(value.into()))
            }
            FieldType::UInt64 => {
                let mut value: u64 = 0;
                check_code(unsafe {
                    ffi::zvec_doc_get_field_value_basic(
                        doc_ptr,
                        field_c.as_ptr(),
                        field_type.to_ffi_data_type(),
                        (&mut value as *mut u64).cast::<c_void>(),
                        std::mem::size_of::<u64>(),
                    )
                })?;
                Ok(Value::Number(value.into()))
            }
            FieldType::Float => {
                let mut value: f32 = 0.0;
                check_code(unsafe {
                    ffi::zvec_doc_get_field_value_basic(
                        doc_ptr,
                        field_c.as_ptr(),
                        field_type.to_ffi_data_type(),
                        (&mut value as *mut f32).cast::<c_void>(),
                        std::mem::size_of::<f32>(),
                    )
                })?;
                Ok(json!(value))
            }
            FieldType::Double => {
                let mut value: f64 = 0.0;
                check_code(unsafe {
                    ffi::zvec_doc_get_field_value_basic(
                        doc_ptr,
                        field_c.as_ptr(),
                        field_type.to_ffi_data_type(),
                        (&mut value as *mut f64).cast::<c_void>(),
                        std::mem::size_of::<f64>(),
                    )
                })?;
                Ok(json!(value))
            }
            FieldType::String | FieldType::Binary => {
                let bytes =
                    self.copy_field_bytes(doc_ptr, field_c, field_type.to_ffi_data_type())?;
                Ok(Value::String(String::from_utf8_lossy(&bytes).into_owned()))
            }
            FieldType::ArrayString => {
                let bytes =
                    self.copy_field_bytes(doc_ptr, field_c, field_type.to_ffi_data_type())?;
                Ok(Value::Array(parse_null_terminated_string_array(&bytes)))
            }
            FieldType::VectorFp32 => {
                let bytes =
                    self.copy_field_bytes(doc_ptr, field_c, field_type.to_ffi_data_type())?;
                if bytes.len() % std::mem::size_of::<f32>() != 0 {
                    return Err(ZvecError::InvalidData(format!(
                        "field `{field_name}` returned invalid VECTOR_FP32 payload"
                    )));
                }
                let values = bytes
                    .chunks_exact(std::mem::size_of::<f32>())
                    .map(|chunk| {
                        let mut buffer = [0u8; std::mem::size_of::<f32>()];
                        buffer.copy_from_slice(chunk);
                        Value::from(f32::from_ne_bytes(buffer) as f64)
                    })
                    .collect::<Vec<_>>();
                Ok(Value::Array(values))
            }
            FieldType::VectorFp64 => {
                let bytes =
                    self.copy_field_bytes(doc_ptr, field_c, field_type.to_ffi_data_type())?;
                if bytes.len() % std::mem::size_of::<f64>() != 0 {
                    return Err(ZvecError::InvalidData(format!(
                        "field `{field_name}` returned invalid VECTOR_FP64 payload"
                    )));
                }
                let values = bytes
                    .chunks_exact(std::mem::size_of::<f64>())
                    .map(|chunk| {
                        let mut buffer = [0u8; std::mem::size_of::<f64>()];
                        buffer.copy_from_slice(chunk);
                        Value::from(f64::from_ne_bytes(buffer))
                    })
                    .collect::<Vec<_>>();
                Ok(Value::Array(values))
            }
            FieldType::VectorInt8 => {
                let bytes =
                    self.copy_field_bytes(doc_ptr, field_c, field_type.to_ffi_data_type())?;
                Ok(Value::Array(
                    bytes
                        .iter()
                        .map(|value| Value::from(i64::from(*value as i8)))
                        .collect(),
                ))
            }
            FieldType::VectorInt16 => {
                let bytes =
                    self.copy_field_bytes(doc_ptr, field_c, field_type.to_ffi_data_type())?;
                if bytes.len() % std::mem::size_of::<i16>() != 0 {
                    return Err(ZvecError::InvalidData(format!(
                        "field `{field_name}` returned invalid VECTOR_INT16 payload"
                    )));
                }
                let values = bytes
                    .chunks_exact(std::mem::size_of::<i16>())
                    .map(|chunk| {
                        let mut buffer = [0u8; std::mem::size_of::<i16>()];
                        buffer.copy_from_slice(chunk);
                        Value::from(i64::from(i16::from_ne_bytes(buffer)))
                    })
                    .collect::<Vec<_>>();
                Ok(Value::Array(values))
            }
            FieldType::ArrayInt32
            | FieldType::ArrayInt64
            | FieldType::ArrayUInt32
            | FieldType::ArrayUInt64
            | FieldType::ArrayFloat
            | FieldType::ArrayDouble
            | FieldType::SparseVectorFp16
            | FieldType::SparseVectorFp32
            | FieldType::VectorFp16 => Err(ZvecError::Unsupported(format!(
                "field extraction currently unsupported for `{field_name}` ({field_type:?})"
            ))),
        }
    }

    fn copy_field_bytes(
        &self,
        doc_ptr: *mut ZVecDoc,
        field_c: &CStr,
        data_type: ZVecDataType,
    ) -> Result<Vec<u8>> {
        let mut value_ptr = std::ptr::null_mut();
        let mut value_size = 0usize;
        check_code(unsafe {
            ffi::zvec_doc_get_field_value_copy(
                doc_ptr,
                field_c.as_ptr(),
                data_type,
                &mut value_ptr,
                &mut value_size,
            )
        })?;

        if value_ptr.is_null() || value_size == 0 {
            if !value_ptr.is_null() {
                unsafe { ffi::zvec_free(value_ptr) };
            }
            return Ok(Vec::new());
        }

        let bytes = unsafe { slice::from_raw_parts(value_ptr as *const u8, value_size) }.to_vec();
        unsafe { ffi::zvec_free(value_ptr) };
        Ok(bytes)
    }
}

impl Drop for Collection {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            let _ = unsafe { ffi::zvec_collection_close(self.ptr) };
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    use serde_json::{json, Value};

    use super::{builder, Collection};

    struct TestCollectionPath {
        cleanup_path: PathBuf,
        zvec_path: String,
    }

    impl TestCollectionPath {
        fn as_str(&self) -> &str {
            &self.zvec_path
        }

        fn remove_dir_all(&self) {
            let _ = fs::remove_dir_all(&self.cleanup_path);
        }
    }

    fn forward_slash_path(path: &Path) -> String {
        path.to_string_lossy().replace('\\', "/")
    }

    fn temp_collection_path(name: &str) -> TestCollectionPath {
        let now_nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after UNIX_EPOCH")
            .as_nanos();
        let cleanup_path = PathBuf::from("target/zvec-rs-tests")
            .join(format!("{name}-{}-{now_nanos}", std::process::id()));
        if let Some(parent) = cleanup_path.parent() {
            fs::create_dir_all(parent).expect("test data directory should be created");
        }

        TestCollectionPath {
            zvec_path: forward_slash_path(&cleanup_path),
            cleanup_path,
        }
    }

    #[test]
    fn open_read_only_smoke() {
        let collection_path = temp_collection_path("read-only");
        let schema = builder::collection_schema(
            "read_only_smoke",
            vec![
                builder::vector_field("embedding", "VECTOR_FP32", 4, "L2", "FLAT"),
                builder::scalar_field("title", "STRING", false),
            ],
        );

        let writer = Collection::create_and_open(collection_path.as_str(), &schema)
            .expect("collection should be created");
        let write_results = writer
            .insert(&[builder::doc(
                "doc-1",
                json!({
                    "embedding": [0.1_f32, 0.2_f32, 0.3_f32, 0.4_f32],
                    "title": "read-only smoke",
                }),
            )])
            .expect("document should be inserted");
        assert_eq!(write_results.len(), 1);
        assert!(write_results[0].is_ok());
        writer
            .flush()
            .expect("flush should succeed for writable handle");
        drop(writer);

        let reader = Collection::open_read_only(collection_path.as_str())
            .expect("read-only collection should be opened");
        let results = reader
            .query(&builder::vector_query(
                "embedding",
                &[0.1_f32, 0.2_f32, 0.3_f32, 0.4_f32],
                1,
            ))
            .expect("read-only collection should allow query operations");
        assert_eq!(results.len(), 1);
        assert!(
            reader.flush().is_err(),
            "read-only handles should reject flush operations"
        );
        drop(reader);

        collection_path.remove_dir_all();
    }

    #[test]
    fn upsert_fetch_roundtrip_handles_nullable_and_arrays() {
        let collection_path = temp_collection_path("roundtrip");

        let schema = builder::collection_schema(
            "roundtrip",
            vec![
                builder::vector_field("embedding", "VECTOR_FP32", 4, "COSINE", "FLAT"),
                builder::scalar_field("title", "STRING", false),
                builder::scalar_field("tags", "ARRAY_STRING", true),
                json!({ "name": "is_active", "data_type": "BOOL", "nullable": false }),
                builder::scalar_field("note", "STRING", true),
            ],
        );

        let collection = Collection::create_and_open(collection_path.as_str(), &schema)
            .expect("collection should be created");

        let docs = vec![builder::doc(
            "doc-1",
            json!({
                "embedding": [0.2_f32, 0.3_f32, 0.4_f32, 0.5_f32],
                "title": "hello",
                "tags": ["a", "b"],
                "is_active": true,
                "note": serde_json::Value::Null,
            }),
        )];

        let write_results = collection.upsert(&docs).expect("upsert should succeed");
        assert_eq!(write_results.len(), 1);
        assert!(write_results[0].is_ok());

        collection.flush().expect("flush should succeed");

        let fetched = collection.fetch(&["doc-1"]).expect("fetch should succeed");
        assert_eq!(fetched.len(), 1);
        let fields = &fetched[0].fields;

        assert_eq!(fields.get("title").and_then(Value::as_str), Some("hello"));
        assert_eq!(fields.get("is_active").and_then(Value::as_bool), Some(true));
        assert!(fields.get("note").is_some_and(Value::is_null));

        let tags = fields
            .get("tags")
            .and_then(Value::as_array)
            .expect("tags should be present as array");
        assert_eq!(tags.len(), 2);
        assert_eq!(tags[0].as_str(), Some("a"));
        assert_eq!(tags[1].as_str(), Some("b"));

        drop(collection);
        collection_path.remove_dir_all();
    }

    #[test]
    fn query_with_filter_and_output_fields() {
        let collection_path = temp_collection_path("query-filter");

        let schema = builder::collection_schema(
            "query_filter",
            vec![
                builder::vector_field("embedding", "VECTOR_FP32", 4, "COSINE", "FLAT"),
                builder::scalar_field_indexed("title", "STRING", false),
                builder::scalar_field("body", "STRING", false),
            ],
        );

        let collection = Collection::create_and_open(collection_path.as_str(), &schema)
            .expect("collection should be created");

        collection
            .insert(&[
                builder::doc(
                    "doc-1",
                    json!({
                        "embedding": [0.9_f32, 0.1_f32, 0.1_f32, 0.1_f32],
                        "title": "match",
                        "body": "first",
                    }),
                ),
                builder::doc(
                    "doc-2",
                    json!({
                        "embedding": [0.1_f32, 0.9_f32, 0.1_f32, 0.1_f32],
                        "title": "other",
                        "body": "second",
                    }),
                ),
            ])
            .expect("insert should succeed");

        collection.flush().expect("flush should succeed");

        let query = builder::vector_query_select_with_filter(
            "embedding",
            &[0.8_f32, 0.1_f32, 0.1_f32, 0.1_f32],
            5,
            "title = 'match'",
            &["title"],
        );

        let results = collection.query(&query).expect("query should succeed");
        assert!(!results.is_empty());
        assert_eq!(
            results[0].fields.get("title").and_then(Value::as_str),
            Some("match")
        );

        drop(collection);
        collection_path.remove_dir_all();
    }

    #[test]
    fn stats_report_doc_count_and_indexes_for_read_only_handles() {
        let collection_path = temp_collection_path("stats");

        let schema = builder::collection_schema(
            "stats",
            vec![
                builder::vector_field("embedding", "VECTOR_FP32", 4, "COSINE", "FLAT"),
                builder::scalar_field_indexed("title", "STRING", false),
            ],
        );

        let collection = Collection::create_and_open(collection_path.as_str(), &schema)
            .expect("collection should be created");

        let insert_results = collection
            .insert(&[
                builder::doc(
                    "doc-1",
                    json!({
                        "embedding": [0.1_f32, 0.2_f32, 0.3_f32, 0.4_f32],
                        "title": "first",
                    }),
                ),
                builder::doc(
                    "doc-2",
                    json!({
                        "embedding": [0.4_f32, 0.3_f32, 0.2_f32, 0.1_f32],
                        "title": "second",
                    }),
                ),
            ])
            .expect("insert should succeed");
        assert_eq!(insert_results.len(), 2);
        assert!(insert_results.iter().all(|result| result.is_ok()));

        let upsert_results = collection
            .upsert(&[
                builder::doc(
                    "doc-2",
                    json!({
                        "embedding": [0.5_f32, 0.3_f32, 0.2_f32, 0.1_f32],
                        "title": "second-updated",
                    }),
                ),
                builder::doc(
                    "doc-3",
                    json!({
                        "embedding": [0.3_f32, 0.4_f32, 0.5_f32, 0.6_f32],
                        "title": "third",
                    }),
                ),
            ])
            .expect("upsert should succeed");
        assert_eq!(upsert_results.len(), 2);
        assert!(upsert_results.iter().all(|result| result.is_ok()));

        collection.flush().expect("flush should succeed");

        let stats = collection.stats().expect("stats should succeed");
        assert_eq!(stats.doc_count, 3);
        assert_eq!(
            collection.doc_count().expect("doc_count should succeed"),
            stats.doc_count
        );
        assert!(
            !stats.indexes.is_empty(),
            "stats should report at least one index"
        );
        assert!(stats.indexes.iter().any(|index| index.name == "embedding"));
        assert!(stats.indexes.iter().all(|index| {
            index.completeness.is_finite() && (0.0..=1.0).contains(&index.completeness)
        }));

        drop(collection);

        let reader = Collection::open_read_only(collection_path.as_str())
            .expect("read-only collection should be opened");
        let read_only_stats = reader.stats().expect("read-only stats should succeed");
        assert_eq!(read_only_stats.doc_count, 3);
        assert_eq!(
            reader
                .doc_count()
                .expect("read-only doc_count should succeed"),
            read_only_stats.doc_count
        );
        assert!(
            !read_only_stats.indexes.is_empty(),
            "read-only stats should report at least one index"
        );
        assert!(read_only_stats
            .indexes
            .iter()
            .any(|index| index.name == "embedding"));
        assert!(read_only_stats.indexes.iter().all(|index| {
            index.completeness.is_finite() && (0.0..=1.0).contains(&index.completeness)
        }));

        drop(reader);
        collection_path.remove_dir_all();
    }
}
