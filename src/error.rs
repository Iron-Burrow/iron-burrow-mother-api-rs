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
