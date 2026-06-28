use reqwest::StatusCode;
use serde_json::Value;

use crate::adapters::http::error::ApiError;

pub(crate) fn assert_public_error(
    status: StatusCode,
    response: &Value,
    expected_status: StatusCode,
    expected_code: &str,
) {
    assert_eq!(status, expected_status);
    assert_eq!(response["ok"], false);
    assert_eq!(response["error"]["code"], expected_code);
    assert!(response["error"]["message"]
        .as_str()
        .is_some_and(|message| !message.is_empty()));
}
