use std::{env, net::SocketAddr};

use super::constants::*;
use crate::config::error::ConfigError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PublicApiSurface {
    Alpha,
    Beta,
}

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct Config {
    pub(crate) app_env: String,
    pub(crate) public_api_surface: PublicApiSurface,
    pub(crate) http_host: String,
    pub(crate) http_port: u16,
    pub(crate) database_url: Option<String>,
    pub(crate) price_indexer_url: Option<String>,
    pub(crate) price_ql_internal_token: Option<String>,
    pub(crate) price_indexer_timeout_ms: u64,
    pub(crate) dis_base_url: Option<String>,
    pub(crate) dis_request_timeout_ms: u64,
    pub(crate) dis_retry_max_attempts: u64,
    pub(crate) infra_gateway_url: Option<String>,
    pub(crate) infra_gateway_token: Option<String>,
    pub(crate) bigwig_request_timeout_ms: u64,
    pub(crate) erc20_transfers_enabled: bool,
    pub(crate) erc20_transfers_max_token_filters: u64,
    pub(crate) bigwig_max_contract_addresses: u64,
}

impl Config {
    pub(crate) fn from_env() -> Result<Self, ConfigError> {
        let config = Self {
            app_env: env::var("APP_ENV").unwrap_or_else(|_| DEFAULT_APP_ENV.to_string()),
            public_api_surface: parse_optional_public_api_surface_env(
                "PUBLIC_API_SURFACE",
                PublicApiSurface::Alpha,
            )
            .map_err(ConfigError::InvalidPublicApiSurface)?,
            http_host: env::var("HTTP_HOST").unwrap_or_else(|_| DEFAULT_HTTP_HOST.to_string()),
            http_port: match env::var("HTTP_PORT") {
                Ok(value) => value
                    .parse()
                    .map_err(|_| ConfigError::InvalidHttpPort(value))?,
                Err(_) => DEFAULT_HTTP_PORT,
            },
            database_url: optional_env("DATABASE_URL"),
            price_indexer_url: optional_env("PRICE_INDEXER_URL"),
            price_ql_internal_token: optional_env("PRICE_QL_INTERNAL_TOKEN"),
            price_indexer_timeout_ms: parse_optional_u64_env(
                "PRICE_INDEXER_TIMEOUT_MS",
                DEFAULT_PRICE_INDEXER_TIMEOUT_MS,
            )
            .map_err(ConfigError::InvalidPriceIndexerTimeout)?,
            dis_base_url: optional_env("DIS_BASE_URL"),
            dis_request_timeout_ms: parse_optional_u64_env(
                "DIS_REQUEST_TIMEOUT_MS",
                DEFAULT_DIS_REQUEST_TIMEOUT_MS,
            )
            .map_err(ConfigError::InvalidDisRequestTimeout)?,
            dis_retry_max_attempts: parse_positive_optional_u64_env(
                "DIS_RETRY_MAX_ATTEMPTS",
                DEFAULT_DIS_RETRY_MAX_ATTEMPTS,
            )
            .map_err(ConfigError::InvalidDisRetryMaxAttempts)?,
            infra_gateway_url: optional_env("INFRA_GATEWAY_URL"),
            infra_gateway_token: optional_env("INFRA_GATEWAY_TOKEN"),
            bigwig_request_timeout_ms: parse_positive_optional_u64_env(
                "BIGWIG_REQUEST_TIMEOUT_MS",
                DEFAULT_BIGWIG_REQUEST_TIMEOUT_MS,
            )
            .map_err(ConfigError::InvalidBigwigRequestTimeout)?,
            erc20_transfers_enabled: parse_optional_bool_env(
                "ERC20_TRANSFERS_ENABLED",
                DEFAULT_ERC20_TRANSFERS_ENABLED,
            )
            .map_err(ConfigError::InvalidErc20TransfersEnabled)?,
            erc20_transfers_max_token_filters: parse_positive_optional_u64_env(
                "ERC20_TRANSFERS_MAX_TOKEN_FILTERS",
                DEFAULT_ERC20_TRANSFERS_MAX_TOKEN_FILTERS,
            )
            .map_err(ConfigError::InvalidErc20TransfersMaxTokenFilters)?,
            bigwig_max_contract_addresses: parse_positive_optional_u64_env(
                "BIGWIG_MAX_CONTRACT_ADDRESSES",
                DEFAULT_BIGWIG_MAX_CONTRACT_ADDRESSES,
            )
            .map_err(ConfigError::InvalidBigwigMaxContractAddresses)?,
        };

        config.validate_startup()?;

        Ok(config)
    }

    pub(crate) fn socket_addr(&self) -> Result<SocketAddr, ConfigError> {
        format!("{}:{}", self.http_host, self.http_port)
            .parse()
            .map_err(|_| ConfigError::InvalidSocketAddress {
                host: self.http_host.clone(),
                port: self.http_port,
            })
    }

    fn validate_startup(&self) -> Result<(), ConfigError> {
        if self.erc20_transfers_max_token_filters > self.bigwig_max_contract_addresses {
            return Err(ConfigError::Erc20TransfersPublicLimitExceedsBigwig {
                erc20_transfers_max_token_filters: self.erc20_transfers_max_token_filters,
                bigwig_max_contract_addresses: self.bigwig_max_contract_addresses,
            });
        }

        Ok(())
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            app_env: DEFAULT_APP_ENV.to_string(),
            public_api_surface: PublicApiSurface::Alpha,
            http_host: DEFAULT_HTTP_HOST.to_string(),
            http_port: DEFAULT_HTTP_PORT,
            database_url: None,
            price_indexer_url: None,
            price_ql_internal_token: None,
            price_indexer_timeout_ms: DEFAULT_PRICE_INDEXER_TIMEOUT_MS,
            dis_base_url: None,
            dis_request_timeout_ms: DEFAULT_DIS_REQUEST_TIMEOUT_MS,
            dis_retry_max_attempts: DEFAULT_DIS_RETRY_MAX_ATTEMPTS,
            infra_gateway_url: None,
            infra_gateway_token: None,
            bigwig_request_timeout_ms: DEFAULT_BIGWIG_REQUEST_TIMEOUT_MS,
            erc20_transfers_enabled: DEFAULT_ERC20_TRANSFERS_ENABLED,
            erc20_transfers_max_token_filters: DEFAULT_ERC20_TRANSFERS_MAX_TOKEN_FILTERS,
            bigwig_max_contract_addresses: DEFAULT_BIGWIG_MAX_CONTRACT_ADDRESSES,
        }
    }
}

impl std::fmt::Debug for Config {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Config")
            .field("app_env", &self.app_env)
            .field("public_api_surface", &self.public_api_surface)
            .field("http_host", &self.http_host)
            .field("http_port", &self.http_port)
            .field("database_url", &self.database_url)
            .field("price_indexer_url", &self.price_indexer_url)
            .field(
                "price_ql_internal_token",
                &self.price_ql_internal_token.as_ref().map(|_| "<redacted>"),
            )
            .field("price_indexer_timeout_ms", &self.price_indexer_timeout_ms)
            .field("dis_base_url", &self.dis_base_url)
            .field("dis_request_timeout_ms", &self.dis_request_timeout_ms)
            .field("dis_retry_max_attempts", &self.dis_retry_max_attempts)
            .field("infra_gateway_url", &self.infra_gateway_url)
            .field(
                "infra_gateway_token",
                &self.infra_gateway_token.as_ref().map(|_| "<redacted>"),
            )
            .field("bigwig_request_timeout_ms", &self.bigwig_request_timeout_ms)
            .field("erc20_transfers_enabled", &self.erc20_transfers_enabled)
            .field(
                "erc20_transfers_max_token_filters",
                &self.erc20_transfers_max_token_filters,
            )
            .field(
                "bigwig_max_contract_addresses",
                &self.bigwig_max_contract_addresses,
            )
            .finish()
    }
}

pub(super) fn optional_env(key: &str) -> Option<String> {
    env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub(super) fn parse_optional_u64_env(key: &str, default: u64) -> Result<u64, String> {
    match env::var(key) {
        Ok(value) => {
            let trimmed = value.trim();

            if trimmed.is_empty() {
                return Ok(default);
            }

            trimmed.parse().map_err(|_| value)
        }
        Err(_) => Ok(default),
    }
}

pub(super) fn parse_positive_optional_u64_env(key: &str, default: u64) -> Result<u64, String> {
    let value = parse_optional_u64_env(key, default)?;

    if value == 0 {
        return Err("0".to_string());
    }

    Ok(value)
}

pub(super) fn parse_optional_bool_env(key: &str, default: bool) -> Result<bool, String> {
    match env::var(key) {
        Ok(value) => {
            let trimmed = value.trim();

            if trimmed.is_empty() {
                return Ok(default);
            }

            match trimmed.to_ascii_lowercase().as_str() {
                "true" | "1" => Ok(true),
                "false" | "0" => Ok(false),
                _ => Err(value),
            }
        }
        Err(_) => Ok(default),
    }
}

pub(super) fn parse_optional_public_api_surface_env(
    key: &str,
    default: PublicApiSurface,
) -> Result<PublicApiSurface, String> {
    match env::var(key) {
        Ok(value) => {
            let trimmed = value.trim();

            if trimmed.is_empty() {
                return Ok(default);
            }

            match trimmed.to_ascii_lowercase().as_str() {
                "alpha" => Ok(PublicApiSurface::Alpha),
                "beta" => Ok(PublicApiSurface::Beta),
                _ => Err(value),
            }
        }
        Err(_) => Ok(default),
    }
}
