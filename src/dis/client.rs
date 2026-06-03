use std::time::Duration;

use reqwest::{StatusCode, Url};
use serde::{Deserialize, Serialize};
use tokio::time::sleep;

const POLYMARKET_SNAPSHOT_PATH: &str = "/internal/v1/prediction-markets/polymarket/snapshot";
const RETRY_BACKOFF: Duration = Duration::from_millis(50);

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
            return serde_json::from_slice::<PolymarketSnapshotResponse>(&body)
                .map_err(|_| DisClientError::MalformedResponse);
        }

        Err(map_error_response(status, &body))
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
    ProviderUnavailable,
    ProviderTimeout,
    UnsupportedSubject,
    MalformedResponse,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PolymarketSnapshotRequest {
    pub event_slug: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct PolymarketSnapshotResponse {
    pub ok: bool,
    pub event: Option<String>,
    pub event_slug: Option<String>,
    pub odds: Option<Vec<PolymarketSnapshotOdd>>,
    pub market: Option<String>,
    pub country: Option<PolymarketCountrySummary>,
    pub probability: Option<String>,
    pub price: Option<String>,
    pub currency: Option<String>,
    pub source: String,
    pub deterministic: bool,
    pub captured_at: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct PolymarketSnapshotOdd {
    pub team: String,
    pub probability: String,
    pub price: String,
    pub currency: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct PolymarketCountrySummary {
    pub slug: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
struct DisErrorEnvelope {
    error: DisErrorBody,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct DisErrorBody {
    code: String,
    message: String,
    details: Option<serde_json::Value>,
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
        )
}

fn map_reqwest_error(error: reqwest::Error) -> DisClientError {
    if error.is_timeout() {
        DisClientError::Timeout
    } else {
        DisClientError::Transport
    }
}

fn map_error_response(status: StatusCode, body: &[u8]) -> DisClientError {
    let envelope = match serde_json::from_slice::<DisErrorEnvelope>(body) {
        Ok(envelope) => envelope,
        Err(_)
            if matches!(
                status,
                StatusCode::SERVICE_UNAVAILABLE | StatusCode::GATEWAY_TIMEOUT
            ) =>
        {
            return DisClientError::ResolverUnavailable;
        }
        Err(_) => return DisClientError::MalformedResponse,
    };

    match envelope.error.code.as_str() {
        "unsupported_prediction_subject" | "unsupported_country" => {
            DisClientError::UnsupportedSubject
        }
        "prediction_provider_unavailable" => DisClientError::ProviderUnavailable,
        "prediction_provider_timeout" => DisClientError::ProviderTimeout,
        "prediction_resolver_unavailable" | "internal_error" => DisClientError::ResolverUnavailable,
        _ if status == StatusCode::SERVICE_UNAVAILABLE => DisClientError::ResolverUnavailable,
        _ if status == StatusCode::GATEWAY_TIMEOUT => DisClientError::ResolverUnavailable,
        _ => DisClientError::ResolverUnavailable,
    }
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        },
        thread,
        time::Duration,
    };

    use serde_json::{json, Value};

    use super::*;

    #[test]
    fn winner_request_serializes_event_slug_only() {
        let request = PolymarketSnapshotRequest {
            event_slug: "fifa-world-cup-2026-winner".to_string(),
            country: None,
        };

        assert_eq!(
            serde_json::to_value(&request).unwrap(),
            json!({ "event_slug": "fifa-world-cup-2026-winner" })
        );
    }

    #[test]
    fn country_request_serializes_event_slug_and_country() {
        let request = PolymarketSnapshotRequest {
            event_slug: "fifa-world-cup-2026-country-probability".to_string(),
            country: Some("mexico".to_string()),
        };

        assert_eq!(
            serde_json::to_value(&request).unwrap(),
            json!({
                "event_slug": "fifa-world-cup-2026-country-probability",
                "country": "mexico"
            })
        );
    }

    #[test]
    fn winner_response_preserves_decimal_strings_and_ignores_future_fields() {
        let response = serde_json::from_value::<PolymarketSnapshotResponse>(json!({
            "ok": true,
            "event": "2026 FIFA World Cup Winner",
            "event_slug": "fifa-world-cup-2026-winner",
            "odds": [
                {
                    "team": "France",
                    "probability": "0.180000000000000001",
                    "price": "0.18",
                    "currency": "USDC",
                    "futureField": "ignored"
                }
            ],
            "source": "polymarket",
            "deterministic": true,
            "captured_at": "2026-06-03T18:20:00Z",
            "futureField": {"ignored": true}
        }))
        .unwrap();

        let odds = response.odds.unwrap();
        assert_eq!(odds[0].probability, "0.180000000000000001");
        assert_eq!(odds[0].price, "0.18");
    }

    #[test]
    fn country_response_preserves_decimal_strings_and_ignores_future_fields() {
        let response = serde_json::from_value::<PolymarketSnapshotResponse>(json!({
            "ok": true,
            "market": "Mexico to reach Round of 16",
            "country": { "slug": "mexico", "name": "Mexico", "futureField": "ignored" },
            "probability": "0.630000000000000001",
            "price": "0.63",
            "currency": "USDC",
            "source": "polymarket",
            "deterministic": true,
            "captured_at": "2026-06-03T18:20:00Z",
            "futureField": {"ignored": true}
        }))
        .unwrap();

        assert_eq!(
            response.probability.as_deref(),
            Some("0.630000000000000001")
        );
        assert_eq!(response.price.as_deref(), Some("0.63"));
    }

    #[test]
    fn maps_dis_error_envelopes() {
        for (status, code, expected) in [
            (
                StatusCode::BAD_REQUEST,
                "unsupported_prediction_subject",
                DisClientError::UnsupportedSubject,
            ),
            (
                StatusCode::BAD_REQUEST,
                "unsupported_country",
                DisClientError::UnsupportedSubject,
            ),
            (
                StatusCode::SERVICE_UNAVAILABLE,
                "prediction_provider_unavailable",
                DisClientError::ProviderUnavailable,
            ),
            (
                StatusCode::GATEWAY_TIMEOUT,
                "prediction_provider_timeout",
                DisClientError::ProviderTimeout,
            ),
            (
                StatusCode::SERVICE_UNAVAILABLE,
                "prediction_resolver_unavailable",
                DisClientError::ResolverUnavailable,
            ),
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                DisClientError::ResolverUnavailable,
            ),
        ] {
            let body = serde_json::to_vec(&json!({
                "error": {
                    "code": code,
                    "message": "DIS-owned message.",
                    "details": { "ignored": true }
                }
            }))
            .unwrap();

            assert_eq!(map_error_response(status, &body), expected);
        }
    }

    #[test]
    fn malformed_dis_availability_error_body_maps_to_resolver_unavailable() {
        assert_eq!(
            map_error_response(StatusCode::SERVICE_UNAVAILABLE, b"not-json"),
            DisClientError::ResolverUnavailable
        );
        assert_eq!(
            map_error_response(StatusCode::GATEWAY_TIMEOUT, b"not-json"),
            DisClientError::ResolverUnavailable
        );
    }

    #[test]
    fn malformed_non_availability_error_body_maps_to_malformed_response() {
        assert_eq!(
            map_error_response(StatusCode::INTERNAL_SERVER_ERROR, b"not-json"),
            DisClientError::MalformedResponse
        );
    }

    #[tokio::test]
    async fn prediction_snapshot_request_maps_transport_failure() {
        let Ok(listener) = TcpListener::bind("127.0.0.1:0") else {
            return;
        };
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        drop(listener);
        let client = DisClient::new(&base_url, 2000, 1).unwrap();

        let error = client
            .get_polymarket_prediction_snapshot(winner_request())
            .await
            .expect_err("closed listener should cause transport failure");

        assert_eq!(error, DisClientError::Transport);
    }

    #[tokio::test]
    async fn prediction_snapshot_request_maps_timeout() {
        let Ok(listener) = TcpListener::bind("127.0.0.1:0") else {
            return;
        };
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        let handle = thread::spawn(move || {
            let (_stream, _) = listener.accept().expect("test request should connect");
            thread::sleep(Duration::from_millis(1000));
        });
        let client = DisClient::new(&base_url, 100, 1).unwrap();

        let error = client
            .get_polymarket_prediction_snapshot(winner_request())
            .await
            .expect_err("held connection should time out");

        assert_eq!(error, DisClientError::Timeout);
        handle.join().expect("test listener thread should finish");
    }

    #[tokio::test]
    async fn retry_budget_caps_retryable_failures() {
        let Ok(listener) = TcpListener::bind("127.0.0.1:0") else {
            return;
        };
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        let attempts = Arc::new(AtomicUsize::new(0));
        let server_attempts = Arc::clone(&attempts);
        let handle = thread::spawn(move || {
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().expect("test request should connect");
                server_attempts.fetch_add(1, Ordering::SeqCst);
                let mut buffer = [0; 1024];
                let _ = stream.read(&mut buffer);
                write_response(
                    &mut stream,
                    StatusCode::SERVICE_UNAVAILABLE,
                    json!({
                        "error": {
                            "code": "prediction_provider_unavailable",
                            "message": "Provider unavailable."
                        }
                    }),
                );
            }
        });
        let client = DisClient::new(&base_url, 2000, 2).unwrap();

        let error = client
            .get_polymarket_prediction_snapshot(winner_request())
            .await
            .expect_err("retryable response should exhaust attempts");

        assert_eq!(error, DisClientError::ProviderUnavailable);
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
        handle.join().expect("test listener thread should finish");
    }

    #[tokio::test]
    async fn unsupported_subject_is_not_retried() {
        let Ok(listener) = TcpListener::bind("127.0.0.1:0") else {
            return;
        };
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        let attempts = Arc::new(AtomicUsize::new(0));
        let server_attempts = Arc::clone(&attempts);
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("test request should connect");
            server_attempts.fetch_add(1, Ordering::SeqCst);
            let mut buffer = [0; 1024];
            let _ = stream.read(&mut buffer);
            write_response(
                &mut stream,
                StatusCode::BAD_REQUEST,
                json!({
                    "error": {
                        "code": "unsupported_prediction_subject",
                        "message": "Unsupported subject."
                    }
                }),
            );
        });
        let client = DisClient::new(&base_url, 2000, 3).unwrap();

        let error = client
            .get_polymarket_prediction_snapshot(winner_request())
            .await
            .expect_err("unsupported subject should fail");

        assert_eq!(error, DisClientError::UnsupportedSubject);
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
        handle.join().expect("test listener thread should finish");
    }

    fn winner_request() -> PolymarketSnapshotRequest {
        PolymarketSnapshotRequest {
            event_slug: "fifa-world-cup-2026-winner".to_string(),
            country: None,
        }
    }

    fn write_response(stream: &mut std::net::TcpStream, status: StatusCode, body: Value) {
        let body = serde_json::to_string(&body).unwrap();
        let reason = status.canonical_reason().unwrap_or("Unknown");
        let response = format!(
            "HTTP/1.1 {} {}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            status.as_u16(),
            reason,
            body.len(),
            body
        );

        stream.write_all(response.as_bytes()).unwrap();
    }
}
