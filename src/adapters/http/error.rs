use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use utoipa::ToSchema;

use crate::application::filters::onchain_window::InvalidOnchainWindowError;

#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: String,
}

impl ApiError {
    pub fn invalid_json() -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "invalid_json",
            message: "Request body must be valid JSON.".to_string(),
        }
    }

    pub fn unknown_field() -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "unknown_field",
            message: "Request contains an unknown field.".to_string(),
        }
    }

    pub fn missing_query(message: &'static str) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "missing_query",
            message: message.to_string(),
        }
    }

    pub fn query_too_long() -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "query_too_long",
            message: "Query parameter `q` must be 128 characters or fewer.".to_string(),
        }
    }

    pub fn invalid_limit() -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "invalid_limit",
            message: "Query parameter `limit` must be a positive integer.".to_string(),
        }
    }

    pub fn invalid_request() -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "invalid_request",
            message: "Request parameters are invalid.".to_string(),
        }
    }

    pub fn missing_network_slug() -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "missing_network_slug",
            message: "Network slug is required.".to_string(),
        }
    }

    pub fn invalid_account() -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "invalid_account",
            message: "Account address is invalid.".to_string(),
        }
    }

    pub fn invalid_address() -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "invalid_address",
            message: "Wallet address is invalid.".to_string(),
        }
    }

    pub fn unsupported_network() -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "unsupported_network",
            message: "Network is not supported for balance resolution.".to_string(),
        }
    }

    pub fn transfer_unsupported_network() -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            code: "unsupported_network",
            message: "Network is not supported for ERC-20 transfer search.".to_string(),
        }
    }

    pub fn unsupported_asset() -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "unsupported_asset",
            message: "Asset is not supported.".to_string(),
        }
    }

    pub fn unsupported_quote_currency() -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "unsupported_quote_currency",
            message: "Quote currency is not supported.".to_string(),
        }
    }

    pub fn unsupported_as_of() -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "unsupported_as_of",
            message: "Only latest balance snapshots are supported.".to_string(),
        }
    }

    pub fn invalid_direction() -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "invalid_direction",
            message: "Direction is invalid.".to_string(),
        }
    }

    pub fn invalid_window() -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "invalid_window",
            message: "Window is invalid.".to_string(),
        }
    }

    pub fn invalid_window_with_message(message: String) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "invalid_window",
            message,
        }
    }

    pub fn invalid_asset_slug() -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "invalid_asset_slug",
            message: "Asset slug is invalid.".to_string(),
        }
    }

    pub fn invalid_contract_address() -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "invalid_contract_address",
            message: "Contract address is invalid.".to_string(),
        }
    }

    pub fn too_many_token_filters() -> Self {
        Self {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            code: "too_many_token_filters",
            message: "Too many token filters were requested.".to_string(),
        }
    }

    pub fn empty_accounts() -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "empty_accounts",
            message: "At least one account is required.".to_string(),
        }
    }

    pub fn empty_assets() -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "empty_assets",
            message: "At least one asset is required.".to_string(),
        }
    }

    pub fn duplicate_account() -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "duplicate_account",
            message: "Each network-scoped account must be unique.".to_string(),
        }
    }

    pub fn duplicate_asset() -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "duplicate_asset",
            message: "Each asset slug must be unique.".to_string(),
        }
    }

    pub fn request_too_large() -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "request_too_large",
            message: "Balance request exceeds the public limits.".to_string(),
        }
    }

    pub fn asset_network_map_unavailable() -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            code: "asset_network_map_unavailable",
            message: "Balance catalog is temporarily unavailable.".to_string(),
        }
    }

    pub fn asset_not_found() -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            code: "asset_not_found",
            message: "Asset was not found.".to_string(),
        }
    }

    pub fn asset_not_available_on_network() -> Self {
        Self {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            code: "asset_not_available_on_network",
            message: "Asset is not available on the requested network.".to_string(),
        }
    }

    pub fn asset_not_erc20_on_network() -> Self {
        Self {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            code: "asset_not_erc20_on_network",
            message: "Asset is not an ERC-20 token on the requested network.".to_string(),
        }
    }

    pub fn asset_contract_mapping_unavailable() -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            code: "asset_contract_mapping_unavailable",
            message: "Asset contract mapping is temporarily unavailable.".to_string(),
        }
    }

    pub fn database_unavailable() -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            code: "database_unavailable",
            message: "Asset resolution is temporarily unavailable.".to_string(),
        }
    }

    pub fn price_indexer_unavailable() -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            code: "price_indexer_unavailable",
            message: "Price signals are temporarily unavailable.".to_string(),
        }
    }

    pub fn extraction_unavailable() -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            code: "extraction_unavailable",
            message: "ERC-20 transfer extraction is temporarily unavailable.".to_string(),
        }
    }

    pub fn upstream_auth_failed() -> Self {
        Self {
            status: StatusCode::BAD_GATEWAY,
            code: "upstream_auth_failed",
            message: "Price signals are temporarily unavailable.".to_string(),
        }
    }

    pub fn price_indexer_error() -> Self {
        Self {
            status: StatusCode::BAD_GATEWAY,
            code: "price_indexer_error",
            message: "Price signals are temporarily unavailable.".to_string(),
        }
    }

    pub fn upstream_invalid_response() -> Self {
        Self {
            status: StatusCode::BAD_GATEWAY,
            code: "upstream_invalid_response",
            message: "Price signals are temporarily unavailable.".to_string(),
        }
    }

    pub fn internal_error() -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "internal_error",
            message: "Mother API encountered an unexpected error.".to_string(),
        }
    }

    pub fn unsupported_prediction_subject() -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "unsupported_prediction_subject",
            message: "Prediction subject is not supported for this event.".to_string(),
        }
    }

    pub fn prediction_provider_unavailable() -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            code: "prediction_provider_unavailable",
            message: "Prediction provider is temporarily unavailable.".to_string(),
        }
    }

    pub fn prediction_provider_timeout() -> Self {
        Self {
            status: StatusCode::GATEWAY_TIMEOUT,
            code: "prediction_provider_timeout",
            message: "Prediction provider timed out.".to_string(),
        }
    }

    pub fn prediction_resolver_unavailable() -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            code: "prediction_resolver_unavailable",
            message: "Prediction resolver is temporarily unavailable.".to_string(),
        }
    }

    pub fn prediction_resolver_schema_mismatch() -> Self {
        Self {
            status: StatusCode::BAD_GATEWAY,
            code: "prediction_resolver_schema_mismatch",
            message: "Prediction resolver returned an unsupported response.".to_string(),
        }
    }

    pub fn prediction_resolver_timeout() -> Self {
        Self {
            status: StatusCode::GATEWAY_TIMEOUT,
            code: "prediction_resolver_timeout",
            message: "Prediction resolver timed out.".to_string(),
        }
    }

    pub fn prediction_resolver_malformed_response() -> Self {
        Self {
            status: StatusCode::BAD_GATEWAY,
            code: "prediction_resolver_malformed_response",
            message: "Prediction resolver returned a malformed error response.".to_string(),
        }
    }

    pub fn prediction_resolver_error() -> Self {
        Self {
            status: StatusCode::BAD_GATEWAY,
            code: "prediction_resolver_error",
            message: "Prediction resolver returned an unclassified error.".to_string(),
        }
    }
}

impl From<InvalidOnchainWindowError> for ApiError {
    fn from(error: InvalidOnchainWindowError) -> Self {
        match error {
            InvalidOnchainWindowError::BlockRange {
                from_block,
                to_block,
            } => ApiError::invalid_window_with_message(format!(
                "from_block must be less than or equal to to_block: from_block={from_block}, to_block={to_block}"
            )),

            InvalidOnchainWindowError::TimestampRange {
                from_timestamp,
                to_timestamp,
            } => ApiError::invalid_window_with_message(format!(
                "from_timestamp must be less than or equal to to_timestamp: from_timestamp={from_timestamp}, to_timestamp={to_timestamp}"
            )),

            InvalidOnchainWindowError::LookbackSeconds {
                lookback_seconds,
            } => ApiError::invalid_window_with_message(format!(
                "lookback_seconds must be greater than zero: lookback_seconds={lookback_seconds}"
            )),
            InvalidOnchainWindowError::Timestamp { field, value } => {
                ApiError::invalid_window_with_message(format!(
                    "{field} must be a valid RFC3339 timestamp: {value}"
                ))
            }
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorResponse {
                ok: false,
                error: ErrorBody {
                    code: self.code,
                    message: self.message,
                },
            }),
        )
            .into_response()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, ToSchema)]
pub struct ErrorResponse {
    pub ok: bool,
    pub error: ErrorBody,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, ToSchema)]
pub struct ErrorBody {
    pub code: &'static str,
    pub message: String,
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::*;

    #[tokio::test]
    async fn internal_error_uses_public_error_envelope() {
        let response = ApiError::internal_error().into_response();
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(json["ok"], false);
        assert_eq!(json["error"]["code"], "internal_error");
        assert_eq!(
            json["error"]["message"],
            "Mother API encountered an unexpected error."
        );
        assert_error_shape(&json);
    }

    fn assert_error_shape(json: &Value) {
        let top_level = json
            .as_object()
            .expect("error response should be an object");
        assert_eq!(top_level.len(), 2);
        assert!(top_level.contains_key("ok"));
        assert!(top_level.contains_key("error"));

        let error = json["error"]
            .as_object()
            .expect("error body should be an object");
        assert_eq!(error.len(), 2);
        assert!(error.contains_key("code"));
        assert!(error.contains_key("message"));
    }

    #[tokio::test]
    async fn transfer_unsupported_network_uses_not_found_status() {
        let response = ApiError::transfer_unsupported_network().into_response();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
