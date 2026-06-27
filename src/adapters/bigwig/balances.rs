use std::time::Duration;

use reqwest::{header::RETRY_AFTER, StatusCode, Url};
use serde::{de, Deserialize, Deserializer, Serialize};

const LATEST_BALANCES_PATH: &str = "/internal/v1/primitives/evm/latest-balances";
const CLIENT_SERVICE: &str = "mother-api";

#[derive(Clone)]
pub struct BigwigLatestBalancesClient {
    client: reqwest::Client,
    base_url: Url,
    token: String,
    timeout: Duration,
}

impl BigwigLatestBalancesClient {
    pub fn new(
        base_url: &str,
        token: &str,
        timeout_ms: u64,
    ) -> Result<Self, BigwigLatestBalancesClientInitError> {
        let base_url = Url::parse(base_url).map_err(|error| {
            BigwigLatestBalancesClientInitError::InvalidBaseUrl(error.to_string())
        })?;
        if !matches!(base_url.scheme(), "http" | "https") || base_url.host_str().is_none() {
            return Err(BigwigLatestBalancesClientInitError::InvalidBaseUrl(
                "URL must use http or https and include a host".to_string(),
            ));
        }

        if token.trim().is_empty() {
            return Err(BigwigLatestBalancesClientInitError::EmptyToken);
        }
        if timeout_ms == 0 {
            return Err(BigwigLatestBalancesClientInitError::InvalidTimeout);
        }

        Ok(Self {
            client: reqwest::Client::new(),
            base_url,
            token: token.to_string(),
            timeout: Duration::from_millis(timeout_ms),
        })
    }

    #[cfg(test)]
    pub fn base_host(&self) -> Option<&str> {
        self.base_url.host_str()
    }

    #[cfg(test)]
    pub fn timeout_ms(&self) -> u128 {
        self.timeout.as_millis()
    }

    pub async fn latest_balances(
        &self,
        request: &BigwigLatestBalancesRequest,
    ) -> Result<BigwigLatestBalancesResponse, BigwigLatestBalancesError> {
        let response = self
            .client
            .post(self.latest_balances_url())
            .bearer_auth(&self.token)
            .header("X-Client-Service", CLIENT_SERVICE)
            .timeout(self.timeout)
            .json(request)
            .send()
            .await
            .map_err(map_reqwest_error)?;

        let status = response.status();
        let retry_after_seconds = response
            .headers()
            .get(RETRY_AFTER)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.trim().parse::<u64>().ok());
        let body = response.bytes().await.map_err(map_reqwest_error)?;

        if status == StatusCode::OK {
            return serde_json::from_slice(&body)
                .map_err(|_| BigwigLatestBalancesError::MalformedSuccessResponse);
        }

        if status.is_success() {
            return Err(BigwigLatestBalancesError::UnexpectedSuccessStatus(
                status.as_u16(),
            ));
        }

        Err(map_error_response(status, &body, retry_after_seconds))
    }

    fn latest_balances_url(&self) -> Url {
        let mut url = self.base_url.clone();
        let base_path = url.path().trim_end_matches('/');
        url.set_path(&format!("{base_path}{LATEST_BALANCES_PATH}"));
        url.set_query(None);
        url.set_fragment(None);
        url
    }
}

impl std::fmt::Debug for BigwigLatestBalancesClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("BigwigLatestBalancesClient")
            .field("base_url", &self.base_url)
            .field("token", &"<redacted>")
            .field("timeout", &self.timeout)
            .finish()
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum BigwigLatestBalancesClientInitError {
    InvalidBaseUrl(String),
    EmptyToken,
    InvalidTimeout,
}

impl std::fmt::Display for BigwigLatestBalancesClientInitError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidBaseUrl(error) => {
                write!(formatter, "invalid INFRA_GATEWAY_URL: {error}")
            }
            Self::EmptyToken => write!(formatter, "INFRA_GATEWAY_TOKEN must not be empty"),
            Self::InvalidTimeout => {
                write!(
                    formatter,
                    "BIGWIG_REQUEST_TIMEOUT_MS must be greater than zero"
                )
            }
        }
    }
}

impl std::error::Error for BigwigLatestBalancesClientInitError {}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BigwigLatestBalancesRequest {
    pub network_slug: String,
    pub accounts: Vec<BigwigBalanceAccount>,
    pub targets: Vec<BigwigBalanceTarget>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BigwigBalanceAccount {
    pub address: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum BigwigBalanceTarget {
    Native,
    Erc20 { contract_address: String },
}

impl<'de> Deserialize<'de> for BigwigBalanceTarget {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireTarget {
            kind: String,
            contract_address: Option<String>,
        }

        let target = WireTarget::deserialize(deserializer)?;
        match (target.kind.as_str(), target.contract_address) {
            ("native", None) => Ok(Self::Native),
            ("erc20", Some(contract_address)) => Ok(Self::Erc20 { contract_address }),
            ("native" | "erc20", _) => Err(de::Error::custom("invalid balance target shape")),
            _ => Err(de::Error::custom("unknown balance target kind")),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct BigwigLatestBalancesResponse {
    pub primitive: BigwigBalancePrimitive,
    pub status: BigwigBalanceEvidenceStatus,
    pub network: BigwigBalanceEvidenceNetwork,
    pub observed_at: String,
    pub block: BigwigBalanceEvidenceBlock,
    pub items: Vec<BigwigBalanceEvidenceItem>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
pub enum BigwigBalancePrimitive {
    #[serde(rename = "evm_latest_balances")]
    EvmLatestBalances,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum BigwigBalanceEvidenceStatus {
    Complete,
    Partial,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct BigwigBalanceEvidenceNetwork {
    pub network_slug: String,
    pub chain_id: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct BigwigBalanceEvidenceBlock {
    pub number: String,
    pub hash: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BigwigBalanceEvidenceItem {
    Resolved {
        account: BigwigBalanceAccount,
        target: BigwigBalanceTarget,
        raw_amount: String,
    },
    Failed {
        account: BigwigBalanceAccount,
        target: BigwigBalanceTarget,
        error: BigwigBalanceItemError,
    },
}

impl<'de> Deserialize<'de> for BigwigBalanceEvidenceItem {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "lowercase")]
        enum WireStatus {
            Resolved,
            Failed,
        }

        #[derive(Deserialize)]
        struct WireItem {
            status: WireStatus,
            account: BigwigBalanceAccount,
            target: BigwigBalanceTarget,
            raw_amount: Option<String>,
            error: Option<BigwigBalanceItemError>,
        }

        let item = WireItem::deserialize(deserializer)?;
        match (item.status, item.raw_amount, item.error) {
            (WireStatus::Resolved, Some(raw_amount), None) => Ok(Self::Resolved {
                account: item.account,
                target: item.target,
                raw_amount,
            }),
            (WireStatus::Failed, None, Some(error)) => Ok(Self::Failed {
                account: item.account,
                target: item.target,
                error,
            }),
            _ => Err(de::Error::custom("invalid balance evidence item shape")),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct BigwigBalanceItemError {
    pub code: BigwigBalanceItemErrorCode,
    pub message: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum BigwigBalanceItemErrorCode {
    NativeBalanceCallFailed,
    Erc20BalanceCallFailed,
    Erc20BadResponse,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BigwigRequestValidationCode {
    MalformedBody,
    EmptyAccounts,
    EmptyTargets,
    InvalidAccount,
    DuplicateAccount,
    InvalidTarget,
    DuplicateTarget,
    RequestTooLarge,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BigwigLatestBalancesError {
    Transport,
    Timeout,
    Unauthorized,
    UnsupportedNetwork,
    NetworkNotEnabledForOperation,
    NoRouteSatisfiesOperation,
    RateLimited { retry_after_seconds: Option<u64> },
    RpcError,
    ProviderUnavailable { retry_after_seconds: Option<u64> },
    ProviderTimeout,
    InternalError,
    RequestValidation(BigwigRequestValidationCode),
    MalformedSuccessResponse,
    MalformedErrorResponse,
    UnexpectedSuccessStatus(u16),
    UnexpectedErrorResponse { status: u16 },
}

fn map_reqwest_error(error: reqwest::Error) -> BigwigLatestBalancesError {
    if error.is_timeout() {
        BigwigLatestBalancesError::Timeout
    } else {
        BigwigLatestBalancesError::Transport
    }
}

fn map_error_response(
    status: StatusCode,
    body: &[u8],
    retry_after_seconds: Option<u64>,
) -> BigwigLatestBalancesError {
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
        Err(_) => return BigwigLatestBalancesError::MalformedErrorResponse,
    };

    match (status, envelope.error.code.as_str()) {
        (StatusCode::BAD_REQUEST, "malformed_body") => {
            BigwigLatestBalancesError::RequestValidation(BigwigRequestValidationCode::MalformedBody)
        }
        (StatusCode::BAD_REQUEST, "empty_accounts") => {
            BigwigLatestBalancesError::RequestValidation(BigwigRequestValidationCode::EmptyAccounts)
        }
        (StatusCode::BAD_REQUEST, "empty_targets") => {
            BigwigLatestBalancesError::RequestValidation(BigwigRequestValidationCode::EmptyTargets)
        }
        (StatusCode::BAD_REQUEST, "invalid_account") => {
            BigwigLatestBalancesError::RequestValidation(
                BigwigRequestValidationCode::InvalidAccount,
            )
        }
        (StatusCode::BAD_REQUEST, "duplicate_account") => {
            BigwigLatestBalancesError::RequestValidation(
                BigwigRequestValidationCode::DuplicateAccount,
            )
        }
        (StatusCode::BAD_REQUEST, "invalid_target") => {
            BigwigLatestBalancesError::RequestValidation(BigwigRequestValidationCode::InvalidTarget)
        }
        (StatusCode::BAD_REQUEST, "duplicate_target") => {
            BigwigLatestBalancesError::RequestValidation(
                BigwigRequestValidationCode::DuplicateTarget,
            )
        }
        (StatusCode::BAD_REQUEST, "request_too_large") => {
            BigwigLatestBalancesError::RequestValidation(
                BigwigRequestValidationCode::RequestTooLarge,
            )
        }
        (StatusCode::UNAUTHORIZED, "unauthorized") => BigwigLatestBalancesError::Unauthorized,
        (StatusCode::NOT_FOUND, "unsupported_network") => {
            BigwigLatestBalancesError::UnsupportedNetwork
        }
        (StatusCode::UNPROCESSABLE_ENTITY, "network_not_enabled_for_operation") => {
            BigwigLatestBalancesError::NetworkNotEnabledForOperation
        }
        (StatusCode::UNPROCESSABLE_ENTITY, "no_route_satisfies_operation") => {
            BigwigLatestBalancesError::NoRouteSatisfiesOperation
        }
        (StatusCode::TOO_MANY_REQUESTS, "gateway_rate_limited") => {
            BigwigLatestBalancesError::RateLimited {
                retry_after_seconds,
            }
        }
        (StatusCode::BAD_GATEWAY, "rpc_error") => BigwigLatestBalancesError::RpcError,
        (StatusCode::SERVICE_UNAVAILABLE, "provider_unavailable") => {
            BigwigLatestBalancesError::ProviderUnavailable {
                retry_after_seconds,
            }
        }
        (StatusCode::GATEWAY_TIMEOUT, "provider_timeout") => {
            BigwigLatestBalancesError::ProviderTimeout
        }
        (StatusCode::INTERNAL_SERVER_ERROR, "internal_error") => {
            BigwigLatestBalancesError::InternalError
        }
        _ => BigwigLatestBalancesError::UnexpectedErrorResponse {
            status: status.as_u16(),
        },
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
        time::{Duration, Instant},
    };

    use serde_json::{json, Value};

    use super::*;

    const ACCOUNT: &str = "0x1234567890abcdef1234567890abcdef1234BEEF";
    const CONTRACT: &str = "0xAf88D065e77C8cC2239327C5EDb3A432268e5831";

    #[test]
    fn request_serializes_only_bigwig_owned_fields() {
        let value = serde_json::to_value(sample_request()).unwrap();

        assert_eq!(
            value,
            json!({
                "network_slug": "arbitrum-mainnet",
                "accounts": [{ "address": ACCOUNT }],
                "targets": [
                    { "kind": "erc20", "contract_address": CONTRACT },
                    { "kind": "native" }
                ]
            })
        );

        let serialized = value.to_string();
        for forbidden in [
            "asset_slug",
            "decimals",
            "symbol",
            "quote_currency",
            "client_ref",
            "route_id",
        ] {
            assert!(!serialized.contains(forbidden));
        }
    }

    #[test]
    fn complete_partial_and_failed_responses_decode() {
        for (status, items, expected_status) in [
            (
                "complete",
                json!([resolved_item()]),
                BigwigBalanceEvidenceStatus::Complete,
            ),
            (
                "partial",
                json!([resolved_item(), failed_item("erc20_balance_call_failed")]),
                BigwigBalanceEvidenceStatus::Partial,
            ),
            (
                "failed",
                json!([failed_item("erc20_bad_response")]),
                BigwigBalanceEvidenceStatus::Failed,
            ),
        ] {
            let response: BigwigLatestBalancesResponse =
                serde_json::from_value(response_body(status, items)).unwrap();

            assert_eq!(
                response.primitive,
                BigwigBalancePrimitive::EvmLatestBalances
            );
            assert_eq!(response.status, expected_status);
            assert_eq!(response.network.network_slug, "arbitrum-mainnet");
            assert_eq!(response.network.chain_id, 42161);
            assert_eq!(response.observed_at, "2026-06-16T15:04:30Z");
            assert_eq!(response.block.number, "123456789");
            assert_eq!(response.items.len(), response_body_item_count(status));
        }
    }

    #[test]
    fn response_preserves_decimal_strings_and_address_casing() {
        let response: BigwigLatestBalancesResponse =
            serde_json::from_value(response_body("complete", json!([resolved_item()]))).unwrap();

        let BigwigBalanceEvidenceItem::Resolved {
            account,
            target,
            raw_amount,
        } = &response.items[0]
        else {
            panic!("expected resolved item");
        };

        assert_eq!(account.address, ACCOUNT);
        assert_eq!(raw_amount, "80001234560000000000000000000000000000");
        assert_eq!(
            target,
            &BigwigBalanceTarget::Erc20 {
                contract_address: CONTRACT.to_string()
            }
        );
    }

    #[test]
    fn additive_response_fields_are_ignored() {
        let mut body = response_body("complete", json!([resolved_item()]));
        body["future_top_level"] = json!({ "provider": "must-not-be-retained" });
        body["items"][0]["future_item_field"] = json!(true);
        body["items"][0]["target"]["future_target_field"] = json!("ignored");

        let response: BigwigLatestBalancesResponse = serde_json::from_value(body).unwrap();

        assert_eq!(response.items.len(), 1);
    }

    #[test]
    fn malformed_success_shapes_are_rejected() {
        let cases = [
            response_body("complete", json!([failed_item("future_item_code")])),
            response_body(
                "complete",
                json!([{
                    "status": "resolved",
                    "account": { "address": ACCOUNT },
                    "target": { "kind": "native" },
                    "raw_amount": "1",
                    "error": {
                        "code": "native_balance_call_failed",
                        "message": "must not coexist"
                    }
                }]),
            ),
            response_body(
                "complete",
                json!([{
                    "status": "resolved",
                    "account": { "address": ACCOUNT },
                    "target": { "kind": "native", "contract_address": CONTRACT },
                    "raw_amount": "1"
                }]),
            ),
            {
                let mut body = response_body("complete", json!([resolved_item()]));
                body["primitive"] = json!("future_primitive");
                body
            },
        ];

        for body in cases {
            assert!(serde_json::from_value::<BigwigLatestBalancesResponse>(body).is_err());
        }
    }

    #[test]
    fn maps_all_documented_error_pairs() {
        let cases = [
            (
                StatusCode::BAD_REQUEST,
                "malformed_body",
                BigwigLatestBalancesError::RequestValidation(
                    BigwigRequestValidationCode::MalformedBody,
                ),
            ),
            (
                StatusCode::BAD_REQUEST,
                "empty_accounts",
                BigwigLatestBalancesError::RequestValidation(
                    BigwigRequestValidationCode::EmptyAccounts,
                ),
            ),
            (
                StatusCode::BAD_REQUEST,
                "empty_targets",
                BigwigLatestBalancesError::RequestValidation(
                    BigwigRequestValidationCode::EmptyTargets,
                ),
            ),
            (
                StatusCode::BAD_REQUEST,
                "invalid_account",
                BigwigLatestBalancesError::RequestValidation(
                    BigwigRequestValidationCode::InvalidAccount,
                ),
            ),
            (
                StatusCode::BAD_REQUEST,
                "duplicate_account",
                BigwigLatestBalancesError::RequestValidation(
                    BigwigRequestValidationCode::DuplicateAccount,
                ),
            ),
            (
                StatusCode::BAD_REQUEST,
                "invalid_target",
                BigwigLatestBalancesError::RequestValidation(
                    BigwigRequestValidationCode::InvalidTarget,
                ),
            ),
            (
                StatusCode::BAD_REQUEST,
                "duplicate_target",
                BigwigLatestBalancesError::RequestValidation(
                    BigwigRequestValidationCode::DuplicateTarget,
                ),
            ),
            (
                StatusCode::BAD_REQUEST,
                "request_too_large",
                BigwigLatestBalancesError::RequestValidation(
                    BigwigRequestValidationCode::RequestTooLarge,
                ),
            ),
            (
                StatusCode::UNAUTHORIZED,
                "unauthorized",
                BigwigLatestBalancesError::Unauthorized,
            ),
            (
                StatusCode::NOT_FOUND,
                "unsupported_network",
                BigwigLatestBalancesError::UnsupportedNetwork,
            ),
            (
                StatusCode::UNPROCESSABLE_ENTITY,
                "network_not_enabled_for_operation",
                BigwigLatestBalancesError::NetworkNotEnabledForOperation,
            ),
            (
                StatusCode::UNPROCESSABLE_ENTITY,
                "no_route_satisfies_operation",
                BigwigLatestBalancesError::NoRouteSatisfiesOperation,
            ),
            (
                StatusCode::TOO_MANY_REQUESTS,
                "gateway_rate_limited",
                BigwigLatestBalancesError::RateLimited {
                    retry_after_seconds: Some(7),
                },
            ),
            (
                StatusCode::BAD_GATEWAY,
                "rpc_error",
                BigwigLatestBalancesError::RpcError,
            ),
            (
                StatusCode::SERVICE_UNAVAILABLE,
                "provider_unavailable",
                BigwigLatestBalancesError::ProviderUnavailable {
                    retry_after_seconds: Some(7),
                },
            ),
            (
                StatusCode::GATEWAY_TIMEOUT,
                "provider_timeout",
                BigwigLatestBalancesError::ProviderTimeout,
            ),
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                BigwigLatestBalancesError::InternalError,
            ),
        ];

        for (status, code, expected) in cases {
            let body = error_body(code);
            assert_eq!(map_error_response(status, &body, Some(7)), expected);
        }
    }

    #[test]
    fn malformed_and_unexpected_error_responses_are_distinct() {
        assert_eq!(
            map_error_response(StatusCode::BAD_GATEWAY, b"not-json", None),
            BigwigLatestBalancesError::MalformedErrorResponse
        );
        for body in [
            json!({ "error": { "code": "rpc_error", "details": {} } }),
            json!({ "error": { "code": "rpc_error", "message": "RPC failed." } }),
            json!({
                "error": {
                    "code": "rpc_error",
                    "message": null,
                    "details": null
                }
            }),
        ] {
            assert_eq!(
                map_error_response(
                    StatusCode::BAD_GATEWAY,
                    &serde_json::to_vec(&body).unwrap(),
                    None
                ),
                BigwigLatestBalancesError::MalformedErrorResponse
            );
        }
        assert_eq!(
            map_error_response(
                StatusCode::BAD_GATEWAY,
                &error_body("provider_timeout"),
                None
            ),
            BigwigLatestBalancesError::UnexpectedErrorResponse { status: 502 }
        );
        assert_eq!(
            map_error_response(StatusCode::IM_A_TEAPOT, &error_body("future_error"), None),
            BigwigLatestBalancesError::UnexpectedErrorResponse { status: 418 }
        );
    }

    #[test]
    fn client_debug_redacts_token() {
        let client =
            BigwigLatestBalancesClient::new("http://infra-gateway-hub:8080", "super-secret", 30000)
                .unwrap();
        let debug = format!("{client:?}");

        assert!(debug.contains("<redacted>"));
        assert!(!debug.contains("super-secret"));
    }

    #[test]
    fn client_rejects_invalid_initialization_values() {
        assert!(matches!(
            BigwigLatestBalancesClient::new("ftp://infra-gateway-hub", "test-token", 30000),
            Err(BigwigLatestBalancesClientInitError::InvalidBaseUrl(_))
        ));
        assert!(matches!(
            BigwigLatestBalancesClient::new("http://infra-gateway-hub:8080", " ", 30000),
            Err(BigwigLatestBalancesClientInitError::EmptyToken)
        ));
        assert!(matches!(
            BigwigLatestBalancesClient::new("http://infra-gateway-hub:8080", "test-token", 0),
            Err(BigwigLatestBalancesClientInitError::InvalidTimeout)
        ));
    }

    #[tokio::test]
    async fn sends_exact_authenticated_contract_request() {
        let Some((base_url, handle)) = spawn_server(
            StatusCode::OK,
            response_body("complete", json!([resolved_item()])),
            &[],
        ) else {
            return;
        };
        let client = BigwigLatestBalancesClient::new(&base_url, "test-token", 2000).unwrap();

        let response = client.latest_balances(&sample_request()).await.unwrap();
        let request = handle.join().unwrap();
        let (headers, body) = split_request(&request);

        assert!(
            headers.starts_with("POST /internal/v1/primitives/evm/latest-balances HTTP/1.1\r\n")
        );
        assert_header(headers, "authorization", "Bearer test-token");
        assert_header(headers, "x-client-service", "mother-api");
        assert_header(headers, "content-type", "application/json");
        assert_eq!(
            serde_json::from_str::<Value>(body).unwrap(),
            serde_json::to_value(sample_request()).unwrap()
        );
        assert_eq!(response.status, BigwigBalanceEvidenceStatus::Complete);
    }

    #[tokio::test]
    async fn retains_retry_after_without_retrying() {
        let Some((base_url, handle)) = spawn_counting_server(
            StatusCode::TOO_MANY_REQUESTS,
            json!({
                "error": {
                    "code": "gateway_rate_limited",
                    "message": "Try later.",
                    "details": { "retry_after_ms": 11000 }
                }
            }),
            &[("Retry-After", "11")],
        ) else {
            return;
        };
        let client = BigwigLatestBalancesClient::new(&base_url, "test-token", 2000).unwrap();

        let error = client
            .latest_balances(&sample_request())
            .await
            .expect_err("rate limit should fail");
        let attempts = handle.join().unwrap();

        assert_eq!(
            error,
            BigwigLatestBalancesError::RateLimited {
                retry_after_seconds: Some(11)
            }
        );
        assert_eq!(attempts, 1);
    }

    #[tokio::test]
    async fn non_ok_success_status_is_rejected() {
        let Some((base_url, handle)) =
            spawn_server(StatusCode::CREATED, json!({ "ignored": true }), &[])
        else {
            return;
        };
        let client = BigwigLatestBalancesClient::new(&base_url, "test-token", 2000).unwrap();

        let error = client
            .latest_balances(&sample_request())
            .await
            .expect_err("only 200 OK is evidence");

        assert_eq!(
            error,
            BigwigLatestBalancesError::UnexpectedSuccessStatus(201)
        );
        handle.join().unwrap();
    }

    #[tokio::test]
    async fn malformed_ok_body_maps_to_malformed_success_response() {
        let Some((base_url, handle)) =
            spawn_server(StatusCode::OK, json!({ "primitive": "wrong" }), &[])
        else {
            return;
        };
        let client = BigwigLatestBalancesClient::new(&base_url, "test-token", 2000).unwrap();

        let error = client
            .latest_balances(&sample_request())
            .await
            .expect_err("malformed evidence should fail");

        assert_eq!(error, BigwigLatestBalancesError::MalformedSuccessResponse);
        handle.join().unwrap();
    }

    #[tokio::test]
    async fn transport_and_timeout_failures_are_classified() {
        let Ok(listener) = TcpListener::bind("127.0.0.1:0") else {
            return;
        };
        let closed_url = format!("http://{}", listener.local_addr().unwrap());
        drop(listener);
        let client = BigwigLatestBalancesClient::new(&closed_url, "test-token", 2000).unwrap();

        assert_eq!(
            client.latest_balances(&sample_request()).await,
            Err(BigwigLatestBalancesError::Transport)
        );

        let Ok(listener) = TcpListener::bind("127.0.0.1:0") else {
            return;
        };
        let timeout_url = format!("http://{}", listener.local_addr().unwrap());
        let handle = thread::spawn(move || {
            let (_stream, _) = listener.accept().unwrap();
            thread::sleep(Duration::from_millis(100));
        });
        let client = BigwigLatestBalancesClient::new(&timeout_url, "test-token", 10).unwrap();

        assert_eq!(
            client.latest_balances(&sample_request()).await,
            Err(BigwigLatestBalancesError::Timeout)
        );
        handle.join().unwrap();
    }

    fn sample_request() -> BigwigLatestBalancesRequest {
        BigwigLatestBalancesRequest {
            network_slug: "arbitrum-mainnet".to_string(),
            accounts: vec![BigwigBalanceAccount {
                address: ACCOUNT.to_string(),
            }],
            targets: vec![
                BigwigBalanceTarget::Erc20 {
                    contract_address: CONTRACT.to_string(),
                },
                BigwigBalanceTarget::Native,
            ],
        }
    }

    fn resolved_item() -> Value {
        json!({
            "status": "resolved",
            "account": { "address": ACCOUNT },
            "target": {
                "kind": "erc20",
                "contract_address": CONTRACT
            },
            "raw_amount": "80001234560000000000000000000000000000"
        })
    }

    fn failed_item(code: &str) -> Value {
        json!({
            "status": "failed",
            "account": { "address": ACCOUNT },
            "target": { "kind": "native" },
            "error": {
                "code": code,
                "message": "Bigwig-owned message"
            }
        })
    }

    fn response_body(status: &str, items: Value) -> Value {
        json!({
            "primitive": "evm_latest_balances",
            "status": status,
            "network": {
                "network_slug": "arbitrum-mainnet",
                "chain_id": 42161
            },
            "observed_at": "2026-06-16T15:04:30Z",
            "block": {
                "number": "123456789",
                "hash": "0x0000000000000000000000000000000000000000000000000000000000000000"
            },
            "items": items
        })
    }

    fn response_body_item_count(status: &str) -> usize {
        if status == "partial" {
            2
        } else {
            1
        }
    }

    fn error_body(code: &str) -> Vec<u8> {
        serde_json::to_vec(&json!({
            "error": {
                "code": code,
                "message": "Bigwig-owned message",
                "details": {}
            }
        }))
        .unwrap()
    }

    fn spawn_server(
        status: StatusCode,
        body: Value,
        extra_headers: &[(&str, &str)],
    ) -> Option<(String, thread::JoinHandle<String>)> {
        let listener = match TcpListener::bind("127.0.0.1:0") {
            Ok(listener) => listener,
            Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return None,
            Err(error) => panic!("failed to bind test Bigwig server: {error}"),
        };
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        let extra_headers = extra_headers
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect::<Vec<_>>();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_http_request(&mut stream);
            write_response(&mut stream, status, body, &extra_headers);
            request
        });

        Some((base_url, handle))
    }

    fn spawn_counting_server(
        status: StatusCode,
        body: Value,
        extra_headers: &[(&str, &str)],
    ) -> Option<(String, thread::JoinHandle<usize>)> {
        let listener = match TcpListener::bind("127.0.0.1:0") {
            Ok(listener) => listener,
            Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return None,
            Err(error) => panic!("failed to bind test Bigwig server: {error}"),
        };
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        let extra_headers = extra_headers
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect::<Vec<_>>();
        let attempts = Arc::new(AtomicUsize::new(0));
        let server_attempts = Arc::clone(&attempts);
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            server_attempts.fetch_add(1, Ordering::SeqCst);
            let _request = read_http_request(&mut stream);
            write_response(&mut stream, status, body.clone(), &extra_headers);

            listener.set_nonblocking(true).unwrap();
            let deadline = Instant::now() + Duration::from_millis(150);
            while Instant::now() < deadline {
                match listener.accept() {
                    Ok((mut retry_stream, _)) => {
                        server_attempts.fetch_add(1, Ordering::SeqCst);
                        let _request = read_http_request(&mut retry_stream);
                        write_response(&mut retry_stream, status, body.clone(), &extra_headers);
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => panic!("failed to inspect retry attempts: {error}"),
                }
            }
            attempts.load(Ordering::SeqCst)
        });

        Some((base_url, handle))
    }

    fn read_http_request(stream: &mut impl Read) -> String {
        let mut request = Vec::new();
        let mut buffer = [0; 1024];

        loop {
            let bytes_read = stream.read(&mut buffer).unwrap();
            if bytes_read == 0 {
                break;
            }
            request.extend_from_slice(&buffer[..bytes_read]);

            let Some(headers_end) = request.windows(4).position(|window| window == b"\r\n\r\n")
            else {
                continue;
            };
            let headers = String::from_utf8_lossy(&request[..headers_end]);
            let content_length = headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())
                        .flatten()
                })
                .unwrap_or(0);
            if request.len() >= headers_end + 4 + content_length {
                break;
            }
        }

        String::from_utf8(request).unwrap()
    }

    fn write_response(
        stream: &mut std::net::TcpStream,
        status: StatusCode,
        body: Value,
        extra_headers: &[(String, String)],
    ) {
        let body = serde_json::to_string(&body).unwrap();
        let reason = status.canonical_reason().unwrap_or("Unknown");
        let extra_headers = extra_headers
            .iter()
            .map(|(name, value)| format!("{name}: {value}\r\n"))
            .collect::<String>();
        let response = format!(
            "HTTP/1.1 {} {}\r\ncontent-type: application/json\r\ncontent-length: {}\r\n{}connection: close\r\n\r\n{}",
            status.as_u16(),
            reason,
            body.len(),
            extra_headers,
            body
        );

        stream.write_all(response.as_bytes()).unwrap();
    }

    fn split_request(request: &str) -> (&str, &str) {
        request
            .split_once("\r\n\r\n")
            .expect("HTTP request should contain a header boundary")
    }

    fn assert_header(headers: &str, expected_name: &str, expected_value: &str) {
        let found = headers.lines().skip(1).any(|line| {
            let Some((name, value)) = line.split_once(':') else {
                return false;
            };
            name.eq_ignore_ascii_case(expected_name) && value.trim() == expected_value
        });

        assert!(
            found,
            "missing header {expected_name}: {expected_value} in {headers}"
        );
    }
}
