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

    pub fn database_unavailable() -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            code: "database_unavailable",
            message: "Asset resolution is temporarily unavailable.",
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
