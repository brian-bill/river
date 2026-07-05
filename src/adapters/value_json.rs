use serde_json::Value as JsonValue;

use crate::adapters::Value;

/// Convert a River adapter [`Value`] into a `serde_json::Value`.
///
/// Shared by the MCP server and the CLI file processor so that JSON output is
/// consistent across every entry point. Booleans map to JSON booleans, `Null`
/// maps to JSON `null`, and floats that cannot be represented as a JSON number
/// (NaN/Inf) fall back to `null`.
pub fn val_to_json(val: &Value) -> JsonValue {
    match val {
        Value::Null => JsonValue::Null,
        Value::String(s) => JsonValue::String(s.clone()),
        Value::Int(n) => JsonValue::Number((*n).into()),
        Value::Float(f) => serde_json::Number::from_f64(*f)
            .map(JsonValue::Number)
            .unwrap_or(JsonValue::Null),
        Value::Bool(b) => JsonValue::Bool(*b),
    }
}
