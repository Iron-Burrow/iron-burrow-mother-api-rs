use std::time::Duration;

use reqwest::{header::RETRY_AFTER, StatusCode, Url};
use tracing::warn;

use crate::adapters::bigwig::error::{map_error_response, BigwigClientInitError, BigwigError};
use crate::adapters::bigwig::{
    balances::{BigwigRequest, BigwigResponse},
    error::map_reqwest_error,
};
use crate::config::Config;

const CLIENT_SERVICE: &str = "mother-api";
const LATEST_BALANCES_PATH: &str = "/internal/v1/primitives/evm/latest-balances";

#[derive(Clone)]
pub struct BigwigClient {
    client: reqwest::Client,
    base_url: Url,
    token: String,
    timeout: Duration,
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
        request: &BigwigRequest,
    ) -> Result<BigwigResponse, BigwigError> {
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
                .map_err(|_| BigwigError::MalformedSuccessResponse);
        }

        if status.is_success() {
            return Err(BigwigError::UnexpectedSuccessStatus(status.as_u16()));
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
