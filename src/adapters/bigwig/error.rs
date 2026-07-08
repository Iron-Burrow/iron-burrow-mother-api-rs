use reqwest::StatusCode;
use serde::Deserialize;

use crate::adapters::bigwig::balances::BigwigRequestValidationCode;

pub(super) fn map_reqwest_error(error: reqwest::Error) -> BigwigError {
    if error.is_timeout() {
        BigwigError::Timeout
    } else {
        BigwigError::Transport
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BigwigError {
    Transport,
    Timeout,
    InvalidExtractionRequest,
    Unauthorized,
    UnsupportedNetwork,
    NetworkNotEnabledForOperation,
    NoRouteSatisfiesOperation,
    RateLimited { retry_after_seconds: Option<u64> },
    InvalidAddress,
    InvalidContractAddress,
    InvalidDirection,
    InvalidWindowShape,
    InvalidAsOf,
    ReversedBlockRange,
    BlockOutOfRange,
    ReversedTimestampRange,
    TimestampAnchorNotConfigured,
    TimestampOutOfRange,
    LookbackTooLarge,
    RangeTooLarge,
    TooManyContractAddresses,
    RpcError,
    ProviderUnavailable { retry_after_seconds: Option<u64> },
    ProviderTimeout,
    ExtractionTimeout,
    InternalError,
    RequestValidation(BigwigRequestValidationCode),
    MalformedSuccessResponse,
    MalformedErrorResponse,
    UnexpectedSuccessStatus(u16),
    UnexpectedErrorResponse { status: u16 },
}

#[derive(Debug, thiserror::Error, Eq, PartialEq)]
pub enum BigwigClientInitError {
    #[error("INFRA_GATEWAY_URL is required")]
    MissingBaseUrl,

    #[error("invalid INFRA_GATEWAY_URL: {0}")]
    InvalidBaseUrl(String),

    #[error("INFRA_GATEWAY_TOKEN is required")]
    MissingToken,

    #[error("INFRA_GATEWAY_TOKEN must not be empty")]
    EmptyToken,

    #[error("BIGWIG_REQUEST_TIMEOUT_MS must be greater than zero")]
    InvalidTimeout,
}

pub(super) fn map_error_response(
    status: StatusCode,
    body: &[u8],
    retry_after_seconds: Option<u64>,
) -> BigwigError {
    #[derive(Deserialize)]
    struct ErrorEnvelope {
        error: ErrorBody,
    }

    #[derive(Deserialize)]
    struct ErrorBody {
        code: String,
        // Bigwig's binding error contract requires both fields. Decode and
        // discard them so contract drift is classified as malformed without
        // retaining or exposing upstream messages or details.
        #[serde(rename = "message")]
        _message: String,
        #[serde(rename = "details")]
        _details: serde_json::Map<String, serde_json::Value>,
    }

    let envelope = match serde_json::from_slice::<ErrorEnvelope>(body) {
        Ok(envelope) => envelope,
        Err(_) => return BigwigError::MalformedErrorResponse,
    };

    match (status, envelope.error.code.as_str()) {
        (StatusCode::BAD_REQUEST, "malformed_body") => {
            BigwigError::RequestValidation(BigwigRequestValidationCode::MalformedBody)
        }
        (StatusCode::BAD_REQUEST, "empty_accounts") => {
            BigwigError::RequestValidation(BigwigRequestValidationCode::EmptyAccounts)
        }
        (StatusCode::BAD_REQUEST, "empty_targets") => {
            BigwigError::RequestValidation(BigwigRequestValidationCode::EmptyTargets)
        }
        (StatusCode::BAD_REQUEST, "invalid_account") => {
            BigwigError::RequestValidation(BigwigRequestValidationCode::InvalidAccount)
        }
        (StatusCode::BAD_REQUEST, "duplicate_account") => {
            BigwigError::RequestValidation(BigwigRequestValidationCode::DuplicateAccount)
        }
        (StatusCode::BAD_REQUEST, "invalid_target") => {
            BigwigError::RequestValidation(BigwigRequestValidationCode::InvalidTarget)
        }
        (StatusCode::BAD_REQUEST, "duplicate_target") => {
            BigwigError::RequestValidation(BigwigRequestValidationCode::DuplicateTarget)
        }
        (StatusCode::BAD_REQUEST, "request_too_large") => {
            BigwigError::RequestValidation(BigwigRequestValidationCode::RequestTooLarge)
        }
        (StatusCode::BAD_REQUEST, "invalid_extraction_request") => {
            BigwigError::InvalidExtractionRequest
        }
        (StatusCode::BAD_REQUEST, "invalid_address") => BigwigError::InvalidAddress,
        (StatusCode::BAD_REQUEST, "invalid_contract_address") => {
            BigwigError::InvalidContractAddress
        }
        (StatusCode::BAD_REQUEST, "invalid_direction") => BigwigError::InvalidDirection,
        (StatusCode::BAD_REQUEST, "invalid_window_shape") => BigwigError::InvalidWindowShape,
        (StatusCode::BAD_REQUEST, "invalid_as_of") => BigwigError::InvalidAsOf,
        (StatusCode::UNAUTHORIZED, "unauthorized") => BigwigError::Unauthorized,
        (StatusCode::NOT_FOUND, "unsupported_network") => BigwigError::UnsupportedNetwork,
        (StatusCode::UNPROCESSABLE_ENTITY, "network_not_enabled_for_operation") => {
            BigwigError::NetworkNotEnabledForOperation
        }
        (StatusCode::UNPROCESSABLE_ENTITY, "no_route_satisfies_operation") => {
            BigwigError::NoRouteSatisfiesOperation
        }
        (StatusCode::UNPROCESSABLE_ENTITY, "reversed_block_range") => {
            BigwigError::ReversedBlockRange
        }
        (StatusCode::UNPROCESSABLE_ENTITY, "block_out_of_range") => BigwigError::BlockOutOfRange,
        (StatusCode::UNPROCESSABLE_ENTITY, "reversed_timestamp_range") => {
            BigwigError::ReversedTimestampRange
        }
        (StatusCode::UNPROCESSABLE_ENTITY, "timestamp_anchor_not_configured") => {
            BigwigError::TimestampAnchorNotConfigured
        }
        (StatusCode::UNPROCESSABLE_ENTITY, "timestamp_out_of_range") => {
            BigwigError::TimestampOutOfRange
        }
        (StatusCode::UNPROCESSABLE_ENTITY, "lookback_too_large") => BigwigError::LookbackTooLarge,
        (StatusCode::UNPROCESSABLE_ENTITY, "range_too_large") => BigwigError::RangeTooLarge,
        (StatusCode::UNPROCESSABLE_ENTITY, "too_many_contract_addresses") => {
            BigwigError::TooManyContractAddresses
        }
        (StatusCode::TOO_MANY_REQUESTS, "gateway_rate_limited") => BigwigError::RateLimited {
            retry_after_seconds,
        },
        (StatusCode::BAD_GATEWAY, "rpc_error") => BigwigError::RpcError,
        (StatusCode::SERVICE_UNAVAILABLE, "provider_unavailable") => {
            BigwigError::ProviderUnavailable {
                retry_after_seconds,
            }
        }
        (StatusCode::GATEWAY_TIMEOUT, "provider_timeout") => BigwigError::ProviderTimeout,
        (StatusCode::GATEWAY_TIMEOUT, "extraction_timeout") => BigwigError::ExtractionTimeout,
        (StatusCode::INTERNAL_SERVER_ERROR, "internal_error") => BigwigError::InternalError,
        _ => BigwigError::UnexpectedErrorResponse {
            status: status.as_u16(),
        },
    }
}
