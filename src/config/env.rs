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
    pub(crate) public_api_base_url: String,
    pub(crate) public_web_base_url: String,
    pub(crate) account_email_lookup_pepper: Option<String>,
    pub(crate) database_url: Option<String>,
    pub(crate) price_indexer_url: Option<String>,
    pub(crate) price_ql_internal_token: Option<String>,
    pub(crate) price_indexer_timeout_ms: u64,
    pub(crate) infra_gateway_url: Option<String>,
    pub(crate) infra_gateway_token: Option<String>,
    pub(crate) bigwig_request_timeout_ms: u64,
    pub(crate) bigwig_archive_route: String,
    pub(crate) aave_v3_min_block_confirmations: u64,
    pub(crate) erc20_transfers_enabled: bool,
    pub(crate) async_reports_enabled: bool,
    pub(crate) bigwig_report_outcome_token: Option<String>,
    pub(crate) bigwig_report_start_timeout_ms: u64,
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
            public_api_base_url: optional_env("PUBLIC_API_BASE_URL")
                .unwrap_or_else(|| DEFAULT_PUBLIC_API_BASE_URL.to_string()),
            public_web_base_url: optional_env("PUBLIC_WEB_BASE_URL")
                .unwrap_or_else(|| DEFAULT_PUBLIC_WEB_BASE_URL.to_string()),
            account_email_lookup_pepper: optional_env("ACCOUNT_EMAIL_LOOKUP_PEPPER"),
            database_url: optional_env("DATABASE_URL"),
            price_indexer_url: optional_env("PRICE_INDEXER_URL"),
            price_ql_internal_token: optional_env("PRICE_QL_INTERNAL_TOKEN"),
            price_indexer_timeout_ms: parse_optional_u64_env(
                "PRICE_INDEXER_TIMEOUT_MS",
                DEFAULT_PRICE_INDEXER_TIMEOUT_MS,
            )
            .map_err(ConfigError::InvalidPriceIndexerTimeout)?,
            infra_gateway_url: optional_env("INFRA_GATEWAY_URL"),
            infra_gateway_token: optional_env("INFRA_GATEWAY_TOKEN"),
            bigwig_request_timeout_ms: parse_positive_optional_u64_env(
                "BIGWIG_REQUEST_TIMEOUT_MS",
                DEFAULT_BIGWIG_REQUEST_TIMEOUT_MS,
            )
            .map_err(ConfigError::InvalidBigwigRequestTimeout)?,
            bigwig_archive_route: optional_env("BIGWIG_ARCHIVE_ROUTE")
                .unwrap_or_else(|| DEFAULT_BIGWIG_ARCHIVE_ROUTE.to_string()),
            aave_v3_min_block_confirmations: parse_optional_u64_env(
                "AAVE_V3_MIN_BLOCK_CONFIRMATIONS",
                DEFAULT_AAVE_V3_MIN_BLOCK_CONFIRMATIONS,
            )
            .map_err(ConfigError::InvalidAaveV3MinBlockConfirmations)?,
            erc20_transfers_enabled: parse_optional_bool_env(
                "ERC20_TRANSFERS_ENABLED",
                DEFAULT_ERC20_TRANSFERS_ENABLED,
            )
            .map_err(ConfigError::InvalidErc20TransfersEnabled)?,
            async_reports_enabled: parse_optional_bool_env(
                "ASYNC_REPORTS_ENABLED",
                DEFAULT_ASYNC_REPORTS_ENABLED,
            )
            .map_err(ConfigError::InvalidAsyncReportsEnabled)?,
            bigwig_report_outcome_token: optional_env("BIGWIG_REPORT_OUTCOME_TOKEN"),
            bigwig_report_start_timeout_ms: parse_positive_optional_u64_env(
                "BIGWIG_REPORT_START_TIMEOUT_MS",
                DEFAULT_BIGWIG_REPORT_START_TIMEOUT_MS,
            )
            .map_err(ConfigError::InvalidBigwigReportStartTimeout)?,
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

        if self.async_reports_enabled && self.bigwig_report_outcome_token.is_none() {
            return Err(ConfigError::MissingBigwigReportOutcomeToken);
        }

        if self.app_env == "production"
            && (self.public_web_base_url.trim().is_empty()
                || self.public_web_base_url == DEFAULT_PUBLIC_WEB_BASE_URL
                || self.account_email_lookup_pepper.is_none())
        {
            return Err(ConfigError::MissingProductionAccountEntryConfig);
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
            public_api_base_url: DEFAULT_PUBLIC_API_BASE_URL.to_string(),
            public_web_base_url: DEFAULT_PUBLIC_WEB_BASE_URL.to_string(),
            account_email_lookup_pepper: None,
            database_url: None,
            price_indexer_url: None,
            price_ql_internal_token: None,
            price_indexer_timeout_ms: DEFAULT_PRICE_INDEXER_TIMEOUT_MS,
            infra_gateway_url: None,
            infra_gateway_token: None,
            bigwig_request_timeout_ms: DEFAULT_BIGWIG_REQUEST_TIMEOUT_MS,
            bigwig_archive_route: DEFAULT_BIGWIG_ARCHIVE_ROUTE.to_string(),
            aave_v3_min_block_confirmations: DEFAULT_AAVE_V3_MIN_BLOCK_CONFIRMATIONS,
            erc20_transfers_enabled: DEFAULT_ERC20_TRANSFERS_ENABLED,
            async_reports_enabled: DEFAULT_ASYNC_REPORTS_ENABLED,
            bigwig_report_outcome_token: None,
            bigwig_report_start_timeout_ms: DEFAULT_BIGWIG_REPORT_START_TIMEOUT_MS,
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
            .field("public_api_base_url", &self.public_api_base_url)
            .field("public_web_base_url", &self.public_web_base_url)
            .field(
                "account_email_lookup_pepper",
                &self
                    .account_email_lookup_pepper
                    .as_ref()
                    .map(|_| "<redacted>"),
            )
            .field("database_url", &self.database_url)
            .field("price_indexer_url", &self.price_indexer_url)
            .field(
                "price_ql_internal_token",
                &self.price_ql_internal_token.as_ref().map(|_| "<redacted>"),
            )
            .field("price_indexer_timeout_ms", &self.price_indexer_timeout_ms)
            .field("infra_gateway_url", &self.infra_gateway_url)
            .field(
                "infra_gateway_token",
                &self.infra_gateway_token.as_ref().map(|_| "<redacted>"),
            )
            .field("bigwig_request_timeout_ms", &self.bigwig_request_timeout_ms)
            .field("bigwig_archive_route", &self.bigwig_archive_route)
            .field(
                "aave_v3_min_block_confirmations",
                &self.aave_v3_min_block_confirmations,
            )
            .field("erc20_transfers_enabled", &self.erc20_transfers_enabled)
            .field("async_reports_enabled", &self.async_reports_enabled)
            .field(
                "bigwig_report_outcome_token",
                &self
                    .bigwig_report_outcome_token
                    .as_ref()
                    .map(|_| "<redacted>"),
            )
            .field(
                "bigwig_report_start_timeout_ms",
                &self.bigwig_report_start_timeout_ms,
            )
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
                _ => Err(trimmed.to_string()),
            }
        }
        Err(_) => Ok(default),
    }
}
