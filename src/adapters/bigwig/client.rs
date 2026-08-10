use std::time::Duration;

use reqwest::{header::RETRY_AFTER, StatusCode, Url};
use serde_json::{json, Value};
use tracing::warn;

use crate::adapters::bigwig::{
    balances::{BigwigRequest, BigwigResponse},
    erc20_transfers::{BigwigErc20TransferRequest, BigwigErc20TransferResponse},
    error::{map_error_response, map_reqwest_error, BigwigClientInitError, BigwigError},
};
use crate::config::Config;

const CLIENT_SERVICE: &str = "mother-api";
const BALANCES_PATH: &str = "/internal/v1/primitives/evm/balances";
const ERC20_TRANSFERS_PATH: &str = "/internal/v1/extractions/erc20-transfers";
const ASYNC_REPORT_EXECUTE_PATH: &str = "/internal/v1/reports";
const DEFAULT_ARCHIVE_ROUTE: &str = "/v1/rpc/eth/mainnet/archive";

#[derive(Clone)]
pub(crate) struct BigwigClient {
    client: reqwest::Client,
    base_url: Url,
    token: String,
    timeout: Duration,
    archive_route: String,
}

impl BigwigClient {
    pub fn new(
        base_url: &str,
        token: &str,
        timeout_ms: u64,
    ) -> Result<Self, BigwigClientInitError> {
        let base_url = Url::parse(base_url)
            .map_err(|error| BigwigClientInitError::InvalidBaseUrl(error.to_string()))?;
        if !matches!(base_url.scheme(), "http" | "https") || base_url.host_str().is_none() {
            return Err(BigwigClientInitError::InvalidBaseUrl(
                "URL must use http or https and include a host".to_string(),
            ));
        }

        if token.trim().is_empty() {
            return Err(BigwigClientInitError::EmptyToken);
        }
        if timeout_ms == 0 {
            return Err(BigwigClientInitError::InvalidTimeout);
        }

        Ok(Self {
            client: reqwest::Client::new(),
            base_url,
            token: token.to_string(),
            timeout: Duration::from_millis(timeout_ms),
            archive_route: DEFAULT_ARCHIVE_ROUTE.to_string(),
        })
    }

    pub(crate) fn with_archive_route(mut self, route: &str) -> Self {
        self.archive_route = format!("/{}", route.trim().trim_matches('/'));
        self
    }

    #[cfg(test)]
    pub fn base_host(&self) -> Option<&str> {
        self.base_url.host_str()
    }

    #[cfg(test)]
    pub fn timeout_ms(&self) -> u128 {
        self.timeout.as_millis()
    }

    pub async fn balances(&self, request: &BigwigRequest) -> Result<BigwigResponse, BigwigError> {
        let response = self
            .client
            .post(self.balances_url())
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
                .map_err(|_| BigwigError::MalformedSuccessResponse);
        }

        if status.is_success() {
            return Err(BigwigError::UnexpectedSuccessStatus(status.as_u16()));
        }

        Err(map_error_response(status, &body, retry_after_seconds))
    }

    pub(crate) async fn search_erc20_transfers(
        &self,
        request: &BigwigErc20TransferRequest,
    ) -> Result<BigwigErc20TransferResponse, BigwigError> {
        let response = self
            .client
            .post(self.erc20_transfers_url())
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
                .map_err(|_| BigwigError::MalformedSuccessResponse);
        }

        if status.is_success() {
            return Err(BigwigError::UnexpectedSuccessStatus(status.as_u16()));
        }

        Err(map_error_response(status, &body, retry_after_seconds))
    }

    pub(crate) async fn execute_async_report(
        &self,
        report_id: &str,
        report_type: &str,
        report_version: i32,
        input: &Value,
        timeout_ms: u64,
    ) -> Result<(), BigwigError> {
        let response = self.client.post(self.async_report_execute_url(report_id))
            .bearer_auth(&self.token)
            .header("X-Client-Service", CLIENT_SERVICE)
            .timeout(Duration::from_millis(timeout_ms))
            .json(&json!({"report_type":report_type,"report_version":report_version,"input":input}))
            .send().await.map_err(map_reqwest_error)?;
        let status = response.status();
        if status == StatusCode::ACCEPTED || status == StatusCode::OK { return Ok(()); }
        let body = response.bytes().await.map_err(map_reqwest_error)?;
        Err(map_error_response(status, &body, None))
    }

    pub(crate) async fn archive_rpc(
        &self,
        method: &str,
        params: Value,
    ) -> Result<Value, BigwigError> {
        if !matches!(
            method,
            "eth_blockNumber" | "eth_call" | "eth_getBlockByNumber"
        ) {
            return Err(BigwigError::InvalidExtractionRequest);
        }
        let response = self
            .client
            .post(self.archive_url())
            .bearer_auth(&self.token)
            .header("X-Client-Service", CLIENT_SERVICE)
            .timeout(self.timeout)
            .json(&json!({"jsonrpc":"2.0", "id": 1, "method": method, "params": params}))
            .send()
            .await
            .map_err(map_reqwest_error)?;
        let status = response.status();
        let body = response.bytes().await.map_err(map_reqwest_error)?;
        if !status.is_success() {
            return Err(map_error_response(status, &body, None));
        }
        let value: Value =
            serde_json::from_slice(&body).map_err(|_| BigwigError::MalformedSuccessResponse)?;
        if value.get("error").is_some() {
            return Err(BigwigError::RpcError);
        }
        value
            .get("result")
            .cloned()
            .ok_or(BigwigError::MalformedSuccessResponse)
    }

    fn balances_url(&self) -> Url {
        self.url_for_path(BALANCES_PATH)
    }

    fn erc20_transfers_url(&self) -> Url {
        self.url_for_path(ERC20_TRANSFERS_PATH)
    }

    fn archive_url(&self) -> Url {
        self.url_for_path(&self.archive_route)
    }
    fn async_report_execute_url(&self, report_id: &str) -> Url {
        self.url_for_path(&format!("{ASYNC_REPORT_EXECUTE_PATH}/{report_id}/execute"))
    }

    fn url_for_path(&self, path: &str) -> Url {
        let mut url = self.base_url.clone();
        let base_path = url.path().trim_end_matches('/');
        url.set_path(&format!("{base_path}{path}"));
        url.set_query(None);
        url.set_fragment(None);
        url
    }
}

impl std::fmt::Debug for BigwigClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("BigwigClient")
            .field("base_url", &self.base_url)
            .field("token", &"<redacted>")
            .field("timeout", &self.timeout)
            .finish()
    }
}

impl TryFrom<&Config> for BigwigClient {
    type Error = BigwigClientInitError;

    fn try_from(config: &Config) -> Result<Self, Self::Error> {
        let base_url = config
            .infra_gateway_url
            .as_deref()
            .ok_or(BigwigClientInitError::MissingBaseUrl)?;

        let token = config
            .infra_gateway_token
            .as_deref()
            .ok_or(BigwigClientInitError::MissingToken)?;

        Self::new(base_url, token, config.bigwig_request_timeout_ms)
            .map(|client| client.with_archive_route(&config.bigwig_archive_route))
    }
}

pub(crate) fn create_bigwig_client(config: &Config) -> Option<BigwigClient> {
    match (
        config.infra_gateway_url.as_deref(),
        config.infra_gateway_token.as_deref(),
    ) {
        (Some(_), Some(_)) => match BigwigClient::try_from(config) {
            Ok(client) => Some(client),
            Err(error) => {
                warn!(%error, "Bigwig config is invalid; latest-balance integration disabled");
                None
            }
        },
        (None, None) => None,
        (url, token) => {
            warn!(
                infra_gateway_url_configured = url.is_some(),
                infra_gateway_token_configured = token.is_some(),
                "Bigwig config is incomplete; latest-balance integration disabled"
            );
            None
        }
    }
}

#[cfg(test)]
mod tests;
