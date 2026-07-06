// Retained as dormant internal-integration boilerplate after the public
// prediction/FIFA routes were removed in Mother API v0.2.0.
#![allow(dead_code)]

use std::time::Duration;

use reqwest::{StatusCode, Url};
use serde::{Deserialize, Serialize};
use tokio::time::sleep;
use tracing::warn;

use crate::config::Config;

const POLYMARKET_SNAPSHOT_PATH: &str = "/internal/v1/prediction-markets/polymarket/snapshot";
const RETRY_BACKOFF: Duration = Duration::from_millis(50);
const MAX_LOGGED_TOP_LEVEL_FIELDS: usize = 16;
const MAX_LOGGED_FIELD_NAME_CHARS: usize = 64;

#[derive(Clone)]
pub struct DisClient {
    client: reqwest::Client,
    base_url: Url,
    timeout: Duration,
    max_attempts: u64,
}

impl DisClient {
    pub fn new(
        base_url: &str,
        timeout_ms: u64,
        max_attempts: u64,
    ) -> Result<Self, DisClientInitError> {
        let base_url = Url::parse(base_url)
            .map_err(|error| DisClientInitError::InvalidBaseUrl(error.to_string()))?;

        if max_attempts == 0 {
            return Err(DisClientInitError::InvalidMaxAttempts);
        }

        Ok(Self {
            client: reqwest::Client::new(),
            base_url,
            timeout: Duration::from_millis(timeout_ms),
            max_attempts,
        })
    }

    #[allow(dead_code)]
    pub fn base_host(&self) -> Option<&str> {
        self.base_url.host_str()
    }

    #[allow(dead_code)]
    pub fn timeout_ms(&self) -> u128 {
        self.timeout.as_millis()
    }

    pub async fn get_polymarket_prediction_snapshot(
        &self,
        request: PolymarketSnapshotRequest,
    ) -> Result<PolymarketSnapshotResponse, DisClientError> {
        let mut attempt = 1;

        loop {
            let result = self.post_polymarket_prediction_snapshot(&request).await;

            match result {
                Ok(response) => return Ok(response),
                Err(error) if should_retry(&error, attempt, self.max_attempts) => {
                    attempt += 1;
                    sleep(RETRY_BACKOFF).await;
                }
                Err(error) => return Err(error),
            }
        }
    }

    async fn post_polymarket_prediction_snapshot(
        &self,
        request: &PolymarketSnapshotRequest,
    ) -> Result<PolymarketSnapshotResponse, DisClientError> {
        let response = self
            .client
            .post(self.polymarket_prediction_snapshot_url())
            .timeout(self.timeout)
            .json(request)
            .send()
            .await
            .map_err(map_reqwest_error)?;

        let status = response.status();
        let body = response.bytes().await.map_err(map_reqwest_error)?;

        if status.is_success() {
            return decode_success_response(request, status, &body);
        }

        let error = map_error_response(status, &body);
        match &error {
            DisClientError::MalformedErrorResponse => {
                let deserialization_error = serde_json::from_slice::<DisErrorEnvelope>(&body).err();
                log_response_issue(
                    request,
                    status,
                    &body,
                    "malformed_error_response",
                    deserialization_error.as_ref().map(serde_error_category),
                    None,
                );
            }
            DisClientError::UnknownResolverErrorCode(code) => {
                log_response_issue(
                    request,
                    status,
                    &body,
                    "unknown_resolver_error_code",
                    None,
                    Some(code),
                );
            }
            _ => {}
        }

        Err(error)
    }

    fn polymarket_prediction_snapshot_url(&self) -> Url {
        self.base_url
            .join(POLYMARKET_SNAPSHOT_PATH.trim_start_matches('/'))
            .expect("static DIS path should be a valid relative URL")
    }
}

impl std::fmt::Debug for DisClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DisClient")
            .field("base_url", &self.base_url)
            .field("timeout", &self.timeout)
            .field("max_attempts", &self.max_attempts)
            .finish()
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum DisClientInitError {
    InvalidBaseUrl(String),
    InvalidMaxAttempts,
}

impl std::fmt::Display for DisClientInitError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidBaseUrl(error) => write!(formatter, "invalid DIS_BASE_URL: {error}"),
            Self::InvalidMaxAttempts => write!(
                formatter,
                "DIS_RETRY_MAX_ATTEMPTS must be greater than zero"
            ),
        }
    }
}

impl std::error::Error for DisClientInitError {}

#[derive(Debug, Eq, PartialEq)]
pub enum DisClientError {
    Transport,
    Timeout,
    ResolverUnavailable,
    ResolverError,
    ProviderUnavailable,
    ProviderTimeout,
    UnsupportedSubject,
    UnsupportedResponseSchema,
    MalformedErrorResponse,
    UnknownResolverErrorCode(String),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PolymarketSnapshotRequest {
    pub event_slug: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PolymarketSnapshotResponse {
    Winner(PolymarketWinnerSnapshot),
    Country(PolymarketCountrySnapshot),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct PolymarketWinnerSnapshot {
    pub event_slug: String,
    pub event_title: String,
    pub outcomes: Vec<PolymarketWinnerOutcome>,
    pub source: String,
    pub deterministic: bool,
    pub captured_at: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct PolymarketWinnerOutcome {
    pub name: String,
    pub probability: String,
    pub price: String,
    pub currency: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct PolymarketCountrySnapshot {
    pub event_slug: String,
    pub event_title: String,
    pub subject: PolymarketCountrySubject,
    pub market: String,
    pub probability: String,
    pub price: String,
    pub currency: String,
    pub source: String,
    pub deterministic: bool,
    pub captured_at: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct PolymarketCountrySubject {
    pub slug: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
struct DisErrorEnvelope {
    error: DisErrorBody,
}

#[derive(Debug, Deserialize)]
struct DisErrorBody {
    code: String,
}

fn should_retry(error: &DisClientError, attempt: u64, max_attempts: u64) -> bool {
    attempt < max_attempts
        && matches!(
            error,
            DisClientError::Transport
                | DisClientError::Timeout
                | DisClientError::ProviderUnavailable
                | DisClientError::ProviderTimeout
                | DisClientError::ResolverUnavailable
                | DisClientError::ResolverError
        )
}

fn map_reqwest_error(error: reqwest::Error) -> DisClientError {
    if error.is_timeout() {
        DisClientError::Timeout
    } else {
        DisClientError::Transport
    }
}

fn decode_success_response(
    request: &PolymarketSnapshotRequest,
    status: StatusCode,
    body: &[u8],
) -> Result<PolymarketSnapshotResponse, DisClientError> {
    let decoded = match expected_response_variant(request) {
        ExpectedResponseVariant::Winner => serde_json::from_slice::<PolymarketWinnerSnapshot>(body)
            .map(PolymarketSnapshotResponse::Winner),
        ExpectedResponseVariant::Country => {
            serde_json::from_slice::<PolymarketCountrySnapshot>(body)
                .map(PolymarketSnapshotResponse::Country)
        }
    };

    decoded.map_err(|error| {
        log_schema_mismatch(request, status, body, Some(&error));
        DisClientError::UnsupportedResponseSchema
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ExpectedResponseVariant {
    Winner,
    Country,
}

impl ExpectedResponseVariant {
    fn as_str(self) -> &'static str {
        match self {
            Self::Winner => "winner",
            Self::Country => "country",
        }
    }
}

fn expected_response_variant(request: &PolymarketSnapshotRequest) -> ExpectedResponseVariant {
    if request.country.is_some() {
        ExpectedResponseVariant::Country
    } else {
        ExpectedResponseVariant::Winner
    }
}

fn log_schema_mismatch(
    request: &PolymarketSnapshotRequest,
    status: StatusCode,
    body: &[u8],
    error: Option<&serde_json::Error>,
) {
    let top_level_fields = top_level_json_field_names(body);
    let error_category = error.map(serde_error_category).unwrap_or("data");

    warn!(
        dis_path = POLYMARKET_SNAPSHOT_PATH,
        status_code = status.as_u16(),
        event_slug = %request.event_slug,
        expected_response_variant = expected_response_variant(request).as_str(),
        deserialization_error_category = error_category,
        body_length = body.len(),
        top_level_json_fields = ?top_level_fields,
        "DIS prediction response schema mismatch"
    );
}

fn log_response_issue(
    request: &PolymarketSnapshotRequest,
    status: StatusCode,
    body: &[u8],
    response_issue: &'static str,
    deserialization_error_category: Option<&'static str>,
    dis_error_code: Option<&str>,
) {
    let top_level_fields = top_level_json_field_names(body);
    let dis_error_code = dis_error_code.map(|code| {
        code.chars()
            .take(MAX_LOGGED_FIELD_NAME_CHARS)
            .collect::<String>()
    });

    warn!(
        dis_path = POLYMARKET_SNAPSHOT_PATH,
        status_code = status.as_u16(),
        event_slug = %request.event_slug,
        expected_response_variant = expected_response_variant(request).as_str(),
        response_issue,
        deserialization_error_category,
        dis_error_code = ?dis_error_code,
        body_length = body.len(),
        top_level_json_fields = ?top_level_fields,
        "DIS prediction error response could not be classified"
    );
}

fn serde_error_category(error: &serde_json::Error) -> &'static str {
    match error.classify() {
        serde_json::error::Category::Io => "io",
        serde_json::error::Category::Syntax => "syntax",
        serde_json::error::Category::Data => "data",
        serde_json::error::Category::Eof => "eof",
    }
}

fn top_level_json_field_names(body: &[u8]) -> Vec<String> {
    let Ok(serde_json::Value::Object(fields)) = serde_json::from_slice::<serde_json::Value>(body)
    else {
        return Vec::new();
    };

    let mut names = fields
        .keys()
        .map(|name| {
            name.chars()
                .take(MAX_LOGGED_FIELD_NAME_CHARS)
                .collect::<String>()
        })
        .collect::<Vec<_>>();
    names.sort();
    names.truncate(MAX_LOGGED_TOP_LEVEL_FIELDS);
    names
}

fn map_error_response(_status: StatusCode, body: &[u8]) -> DisClientError {
    let envelope = match serde_json::from_slice::<DisErrorEnvelope>(body) {
        Ok(envelope) => envelope,
        Err(_) => return DisClientError::MalformedErrorResponse,
    };

    match envelope.error.code.as_str() {
        "unsupported_prediction_subject" | "unsupported_country" => {
            DisClientError::UnsupportedSubject
        }
        "prediction_provider_unavailable" => DisClientError::ProviderUnavailable,
        "prediction_provider_timeout" => DisClientError::ProviderTimeout,
        "prediction_resolver_unavailable" => DisClientError::ResolverUnavailable,
        "internal_error" => DisClientError::ResolverError,
        code => DisClientError::UnknownResolverErrorCode(code.to_string()),
    }
}

pub(crate) fn create_dis_client(config: &Config) -> Option<DisClient> {
    match config.dis_base_url.as_deref() {
        Some(url) => match DisClient::new(
            url,
            config.dis_request_timeout_ms,
            config.dis_retry_max_attempts,
        ) {
            Ok(client) => Some(client),
            Err(error) => {
                warn!(%error, "DIS config is invalid; DIS integration disabled");
                None
            }
        },
        None => None,
    }
}

#[cfg(test)]
mod tests;
