use serde_json::Value;

use crate::adapters::http::types::JsonObject;

pub(crate) fn json_object(value: Value) -> JsonObject {
    match value {
        Value::Object(object) => object,
        other => panic!("expected JSON object, got {other:?}"),
    }
}
