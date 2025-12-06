//! JSON Schema helper functions for schemars
//!
//! Provides custom schema functions to avoid invalid format fields that schemars
//! generates for Rust integer types. JSON Schema only defines "int32" and "int64"
//! as valid integer formats. For unsigned integers and other sizes, we use
//! type: "integer" with min/max constraints instead of invalid format fields.

use schemars;

/// Schema for unsigned 64-bit integers (u64)
pub fn u64_schema(_gen: &mut schemars::SchemaGenerator) -> schemars::Schema {
    serde_json::from_value(serde_json::json!({
        "type": "integer",
        "minimum": 0
    })).unwrap()
}

/// Schema for unsigned 32-bit integers (u32)
pub fn u32_schema(_gen: &mut schemars::SchemaGenerator) -> schemars::Schema {
    serde_json::from_value(serde_json::json!({
        "type": "integer",
        "minimum": 0
    })).unwrap()
}

/// Schema for unsigned 16-bit integers (u16)
pub fn u16_schema(_gen: &mut schemars::SchemaGenerator) -> schemars::Schema {
    serde_json::from_value(serde_json::json!({
        "type": "integer",
        "minimum": 0,
        "maximum": 65535
    })).unwrap()
}

/// Schema for unsigned 8-bit integers (u8)
pub fn u8_schema(_gen: &mut schemars::SchemaGenerator) -> schemars::Schema {
    serde_json::from_value(serde_json::json!({
        "type": "integer",
        "minimum": 0,
        "maximum": 255
    })).unwrap()
}

/// Schema for signed 8-bit integers (i8)
pub fn i8_schema(_gen: &mut schemars::SchemaGenerator) -> schemars::Schema {
    serde_json::from_value(serde_json::json!({
        "type": "integer",
        "minimum": -128,
        "maximum": 127
    })).unwrap()
}

/// Schema for usize (platform-dependent unsigned integer)
pub fn usize_schema(_gen: &mut schemars::SchemaGenerator) -> schemars::Schema {
    serde_json::from_value(serde_json::json!({
        "type": "integer",
        "minimum": 0
    })).unwrap()
}

/// Schema for Vec<u8> (byte arrays)
pub fn vec_u8_schema(_gen: &mut schemars::SchemaGenerator) -> schemars::Schema {
    serde_json::from_value(serde_json::json!({
        "type": "array",
        "items": {
            "type": "integer",
            "minimum": 0,
            "maximum": 255
        }
    })).unwrap()
}

/// Schema for optional (u8, u8) tuples
pub fn optional_u8_tuple_schema(_gen: &mut schemars::SchemaGenerator) -> schemars::Schema {
    serde_json::from_value(serde_json::json!({
        "type": ["array", "null"],
        "items": {
            "type": "integer",
            "minimum": 0,
            "maximum": 255
        },
        "minItems": 2,
        "maxItems": 2
    })).unwrap()
}

/// Schema for HashMap<String, usize>
pub fn hashmap_string_usize_schema(_gen: &mut schemars::SchemaGenerator) -> schemars::Schema {
    serde_json::from_value(serde_json::json!({
        "type": "object",
        "additionalProperties": {
            "type": "integer",
            "minimum": 0
        }
    })).unwrap()
}

/// Schema for Option<usize>
pub fn optional_usize_schema(_gen: &mut schemars::SchemaGenerator) -> schemars::Schema {
    serde_json::from_value(serde_json::json!({
        "type": ["integer", "null"],
        "minimum": 0
    })).unwrap()
}

/// Schema for Option<u32>
pub fn optional_u32_schema(_gen: &mut schemars::SchemaGenerator) -> schemars::Schema {
    serde_json::from_value(serde_json::json!({
        "type": ["integer", "null"],
        "minimum": 0
    })).unwrap()
}

/// Schema for Option<u16>
pub fn optional_u16_schema(_gen: &mut schemars::SchemaGenerator) -> schemars::Schema {
    serde_json::from_value(serde_json::json!({
        "type": ["integer", "null"],
        "minimum": 0,
        "maximum": 65535
    })).unwrap()
}

/// Schema for Option<u8>
pub fn optional_u8_schema(_gen: &mut schemars::SchemaGenerator) -> schemars::Schema {
    serde_json::from_value(serde_json::json!({
        "type": ["integer", "null"],
        "minimum": 0,
        "maximum": 255
    })).unwrap()
}

/// Schema for Option<i8>
pub fn optional_i8_schema(_gen: &mut schemars::SchemaGenerator) -> schemars::Schema {
    serde_json::from_value(serde_json::json!({
        "type": ["integer", "null"],
        "minimum": -128,
        "maximum": 127
    })).unwrap()
}
