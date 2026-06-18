use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;

#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: &'static str,
}

impl ApiError {
    pub fn missing_query(message: &'static str) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "missing_query",
            message,
        }
    }

    pub fn query_too_long() -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "query_too_long",
            message: "Query parameter `q` must be 128 characters or fewer.",
        }
    }

    pub fn invalid_limit() -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "invalid_limit",
            message: "Query parameter `limit` must be a positive integer.",
        }
    }

    pub fn invalid_request() -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "invalid_request",
            message: "Request parameters are invalid.",
        }
    }

    pub fn invalid_account() -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "invalid_account",
            message: "Account address is invalid.",
        }
    }

    pub fn unsupported_network() -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "unsupported_network",
            message: "Network is not supported for balance resolution.",
        }
    }

    pub fn unsupported_asset() -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "unsupported_asset",
            message: "Asset is not supported.",
        }
    }

    pub fn unsupported_quote_currency() -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "unsupported_quote_currency",
            message: "Quote currency is not supported.",
        }
    }

    pub fn unsupported_as_of() -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "unsupported_as_of",
            message: "Only latest balance snapshots are supported.",
        }
    }

    pub fn empty_accounts() -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "empty_accounts",
            message: "At least one account is required.",
        }
    }

    pub fn empty_assets() -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "empty_assets",
            message: "At least one asset is required.",
        }
    }

    pub fn duplicate_account() -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "duplicate_account",
            message: "Each network-scoped account must be unique.",
        }
    }

    pub fn duplicate_asset() -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "duplicate_asset",
            message: "Each asset slug must be unique.",
        }
    }

    pub fn request_too_large() -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "request_too_large",
            message: "Balance request exceeds the public limits.",
        }
    }

    pub fn asset_network_map_unavailable() -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            code: "asset_network_map_unavailable",
            message: "Balance catalog is temporarily unavailable.",
        }
    }

    pub fn asset_not_found() -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            code: "asset_not_found",
            message: "Asset was not found.",
        }
    }

    pub fn database_unavailable() -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            code: "database_unavailable",
            message: "Asset resolution is temporarily unavailable.",
        }
    }

    pub fn price_indexer_unavailable() -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            code: "price_indexer_unavailable",
            message: "Price signals are temporarily unavailable.",
        }
    }

    pub fn upstream_auth_failed() -> Self {
        Self {
            status: StatusCode::BAD_GATEWAY,
            code: "upstream_auth_failed",
            message: "Price signals are temporarily unavailable.",
        }
    }

    pub fn price_indexer_error() -> Self {
        Self {
            status: StatusCode::BAD_GATEWAY,
            code: "price_indexer_error",
            message: "Price signals are temporarily unavailable.",
        }
    }

    pub fn upstream_invalid_response() -> Self {
        Self {
            status: StatusCode::BAD_GATEWAY,
            code: "upstream_invalid_response",
            message: "Price signals are temporarily unavailable.",
        }
    }

    pub fn internal_error() -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "internal_error",
            message: "Mother API encountered an unexpected error.",
        }
    }

    pub fn unsupported_prediction_subject() -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "unsupported_prediction_subject",
            message: "Prediction subject is not supported for this event.",
        }
    }

    pub fn prediction_provider_unavailable() -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            code: "prediction_provider_unavailable",
            message: "Prediction provider is temporarily unavailable.",
        }
    }

    pub fn prediction_provider_timeout() -> Self {
        Self {
            status: StatusCode::GATEWAY_TIMEOUT,
            code: "prediction_provider_timeout",
            message: "Prediction provider timed out.",
        }
    }

    pub fn prediction_resolver_unavailable() -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            code: "prediction_resolver_unavailable",
            message: "Prediction resolver is temporarily unavailable.",
        }
    }

    pub fn prediction_resolver_schema_mismatch() -> Self {
        Self {
            status: StatusCode::BAD_GATEWAY,
            code: "prediction_resolver_schema_mismatch",
            message: "Prediction resolver returned an unsupported response.",
        }
    }

    pub fn prediction_resolver_timeout() -> Self {
        Self {
            status: StatusCode::GATEWAY_TIMEOUT,
            code: "prediction_resolver_timeout",
            message: "Prediction resolver timed out.",
        }
    }

    pub fn prediction_resolver_malformed_response() -> Self {
        Self {
            status: StatusCode::BAD_GATEWAY,
            code: "prediction_resolver_malformed_response",
            message: "Prediction resolver returned a malformed error response.",
        }
    }

    pub fn prediction_resolver_error() -> Self {
        Self {
            status: StatusCode::BAD_GATEWAY,
            code: "prediction_resolver_error",
            message: "Prediction resolver returned an unclassified error.",
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

#[derive(Serialize)]
struct ErrorResponse {
    ok: bool,
    error: ErrorBody,
}

#[derive(Serialize)]
struct ErrorBody {
    code: &'static str,
    message: &'static str,
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
}
