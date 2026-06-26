use std::{env, net::SocketAddr};

const DEFAULT_APP_ENV: &str = "development";
const DEFAULT_HTTP_HOST: &str = "0.0.0.0";
const DEFAULT_HTTP_PORT: u16 = 3000;
pub const DEFAULT_PRICE_INDEXER_TIMEOUT_MS: u64 = 2000;
pub const DEFAULT_DIS_REQUEST_TIMEOUT_MS: u64 = 5000;
pub const DEFAULT_DIS_RETRY_MAX_ATTEMPTS: u64 = 2;
pub const DEFAULT_BIGWIG_REQUEST_TIMEOUT_MS: u64 = 30000;
pub const DEFAULT_ERC20_TRANSFERS_ENABLED: bool = false;
pub const DEFAULT_ERC20_TRANSFERS_MAX_TOKEN_FILTERS: u64 = 20;
pub const DEFAULT_BIGWIG_MAX_CONTRACT_ADDRESSES: u64 = 20;

#[derive(Clone, Eq, PartialEq)]
pub struct Config {
    pub app_env: String,
    pub http_host: String,
    pub http_port: u16,
    pub database_url: Option<String>,
    pub price_indexer_url: Option<String>,
    pub price_ql_internal_token: Option<String>,
    pub price_indexer_timeout_ms: u64,
    pub dis_base_url: Option<String>,
    pub dis_request_timeout_ms: u64,
    pub dis_retry_max_attempts: u64,
    pub infra_gateway_url: Option<String>,
    pub infra_gateway_token: Option<String>,
    pub bigwig_request_timeout_ms: u64,
    pub erc20_transfers_enabled: bool,
    pub erc20_transfers_max_token_filters: u64,
    pub bigwig_max_contract_addresses: u64,
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        let config = Self {
            app_env: env::var("APP_ENV").unwrap_or_else(|_| DEFAULT_APP_ENV.to_string()),
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

    pub fn socket_addr(&self) -> Result<SocketAddr, ConfigError> {
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

fn optional_env(key: &str) -> Option<String> {
    env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn parse_optional_u64_env(key: &str, default: u64) -> Result<u64, String> {
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

fn parse_positive_optional_u64_env(key: &str, default: u64) -> Result<u64, String> {
    let value = parse_optional_u64_env(key, default)?;

    if value == 0 {
        return Err("0".to_string());
    }

    Ok(value)
}

fn parse_optional_bool_env(key: &str, default: bool) -> Result<bool, String> {
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

#[derive(Debug, Eq, PartialEq)]
pub enum ConfigError {
    InvalidHttpPort(String),
    InvalidPriceIndexerTimeout(String),
    InvalidDisRequestTimeout(String),
    InvalidDisRetryMaxAttempts(String),
    InvalidBigwigRequestTimeout(String),
    InvalidErc20TransfersEnabled(String),
    InvalidErc20TransfersMaxTokenFilters(String),
    InvalidBigwigMaxContractAddresses(String),
    Erc20TransfersPublicLimitExceedsBigwig {
        erc20_transfers_max_token_filters: u64,
        bigwig_max_contract_addresses: u64,
    },
    InvalidSocketAddress {
        host: String,
        port: u16,
    },
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidHttpPort(value) => {
                write!(formatter, "HTTP_PORT must be a valid u16, got {value:?}")
            }
            Self::InvalidPriceIndexerTimeout(value) => {
                write!(
                    formatter,
                    "PRICE_INDEXER_TIMEOUT_MS must be a valid u64, got {value:?}"
                )
            }
            Self::InvalidDisRequestTimeout(value) => {
                write!(
                    formatter,
                    "DIS_REQUEST_TIMEOUT_MS must be a valid u64, got {value:?}"
                )
            }
            Self::InvalidDisRetryMaxAttempts(value) => {
                write!(
                    formatter,
                    "DIS_RETRY_MAX_ATTEMPTS must be a positive u64, got {value:?}"
                )
            }
            Self::InvalidBigwigRequestTimeout(value) => {
                write!(
                    formatter,
                    "BIGWIG_REQUEST_TIMEOUT_MS must be a positive u64, got {value:?}"
                )
            }
            Self::InvalidErc20TransfersEnabled(value) => {
                write!(
                    formatter,
                    "ERC20_TRANSFERS_ENABLED must be a boolean, got {value:?}"
                )
            }
            Self::InvalidErc20TransfersMaxTokenFilters(value) => {
                write!(
                    formatter,
                    "ERC20_TRANSFERS_MAX_TOKEN_FILTERS must be a positive u64, got {value:?}"
                )
            }
            Self::InvalidBigwigMaxContractAddresses(value) => {
                write!(
                    formatter,
                    "BIGWIG_MAX_CONTRACT_ADDRESSES must be a positive u64, got {value:?}"
                )
            }
            Self::Erc20TransfersPublicLimitExceedsBigwig {
                erc20_transfers_max_token_filters,
                bigwig_max_contract_addresses,
            } => {
                write!(
                    formatter,
                    "ERC20_TRANSFERS_MAX_TOKEN_FILTERS ({erc20_transfers_max_token_filters}) must not exceed BIGWIG_MAX_CONTRACT_ADDRESSES ({bigwig_max_contract_addresses})"
                )
            }
            Self::InvalidSocketAddress { host, port } => {
                write!(
                    formatter,
                    "HTTP_HOST and HTTP_PORT must form a valid socket address, got {host}:{port}"
                )
            }
        }
    }
}

impl std::error::Error for ConfigError {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    struct EnvVarSnapshot {
        key: &'static str,
        value: Option<String>,
    }

    impl EnvVarSnapshot {
        fn capture(key: &'static str) -> Self {
            Self {
                key,
                value: std::env::var(key).ok(),
            }
        }
    }

    impl Drop for EnvVarSnapshot {
        fn drop(&mut self) {
            match &self.value {
                Some(value) => std::env::set_var(self.key, value),
                None => std::env::remove_var(self.key),
            }
        }
    }

    fn capture_env_vars(keys: &[&'static str]) -> Vec<EnvVarSnapshot> {
        keys.iter().copied().map(EnvVarSnapshot::capture).collect()
    }

    #[test]
    fn default_config_matches_public_contract() {
        let config = Config::default();

        assert_eq!(config.app_env, "development");
        assert_eq!(config.http_host, "0.0.0.0");
        assert_eq!(config.http_port, 3000);
        assert_eq!(config.database_url, None);
        assert_eq!(config.price_indexer_url, None);
        assert_eq!(config.price_ql_internal_token, None);
        assert_eq!(config.price_indexer_timeout_ms, 2000);
        assert_eq!(config.dis_base_url, None);
        assert_eq!(config.dis_request_timeout_ms, 5000);
        assert_eq!(config.dis_retry_max_attempts, 2);
        assert_eq!(config.infra_gateway_url, None);
        assert_eq!(config.infra_gateway_token, None);
        assert_eq!(config.bigwig_request_timeout_ms, 30000);
        assert!(!config.erc20_transfers_enabled);
        assert_eq!(config.erc20_transfers_max_token_filters, 20);
        assert_eq!(config.bigwig_max_contract_addresses, 20);
        assert_eq!(
            config.socket_addr().unwrap(),
            "0.0.0.0:3000".parse::<SocketAddr>().unwrap()
        );
    }

    #[test]
    fn price_indexer_timeout_defaults_when_env_is_missing_or_empty() {
        assert_eq!(
            parse_optional_u64_env("MISSING_PRICE_INDEXER_TIMEOUT", 2000).unwrap(),
            2000
        );

        std::env::set_var("EMPTY_PRICE_INDEXER_TIMEOUT", "   ");
        assert_eq!(
            parse_optional_u64_env("EMPTY_PRICE_INDEXER_TIMEOUT", 2000).unwrap(),
            2000
        );
        std::env::remove_var("EMPTY_PRICE_INDEXER_TIMEOUT");
    }

    #[test]
    fn price_indexer_timeout_rejects_invalid_values() {
        std::env::set_var("INVALID_PRICE_INDEXER_TIMEOUT", "soon");

        assert_eq!(
            parse_optional_u64_env("INVALID_PRICE_INDEXER_TIMEOUT", 2000),
            Err("soon".to_string())
        );

        std::env::remove_var("INVALID_PRICE_INDEXER_TIMEOUT");
    }

    #[test]
    fn dis_retry_max_attempts_defaults_when_env_is_missing_or_empty() {
        assert_eq!(
            parse_positive_optional_u64_env("MISSING_DIS_RETRY_MAX_ATTEMPTS", 2).unwrap(),
            2
        );

        std::env::set_var("EMPTY_DIS_RETRY_MAX_ATTEMPTS", "   ");
        assert_eq!(
            parse_positive_optional_u64_env("EMPTY_DIS_RETRY_MAX_ATTEMPTS", 2).unwrap(),
            2
        );
        std::env::remove_var("EMPTY_DIS_RETRY_MAX_ATTEMPTS");
    }

    #[test]
    fn boolean_config_defaults_trims_and_parses_common_values() {
        assert_eq!(
            parse_optional_bool_env("MISSING_ERC20_TRANSFERS_ENABLED", false).unwrap(),
            false
        );

        std::env::set_var("EMPTY_ERC20_TRANSFERS_ENABLED", "   ");
        std::env::set_var("TRUE_ERC20_TRANSFERS_ENABLED", " TRUE ");
        std::env::set_var("ONE_ERC20_TRANSFERS_ENABLED", "1");
        std::env::set_var("FALSE_ERC20_TRANSFERS_ENABLED", " false ");
        std::env::set_var("ZERO_ERC20_TRANSFERS_ENABLED", "0");

        assert_eq!(
            parse_optional_bool_env("EMPTY_ERC20_TRANSFERS_ENABLED", true).unwrap(),
            true
        );
        assert!(parse_optional_bool_env("TRUE_ERC20_TRANSFERS_ENABLED", false).unwrap());
        assert!(parse_optional_bool_env("ONE_ERC20_TRANSFERS_ENABLED", false).unwrap());
        assert!(!parse_optional_bool_env("FALSE_ERC20_TRANSFERS_ENABLED", true).unwrap());
        assert!(!parse_optional_bool_env("ZERO_ERC20_TRANSFERS_ENABLED", true).unwrap());

        std::env::remove_var("EMPTY_ERC20_TRANSFERS_ENABLED");
        std::env::remove_var("TRUE_ERC20_TRANSFERS_ENABLED");
        std::env::remove_var("ONE_ERC20_TRANSFERS_ENABLED");
        std::env::remove_var("FALSE_ERC20_TRANSFERS_ENABLED");
        std::env::remove_var("ZERO_ERC20_TRANSFERS_ENABLED");
    }

    #[test]
    fn boolean_config_rejects_invalid_values() {
        std::env::set_var("INVALID_ERC20_TRANSFERS_ENABLED", "sometimes");

        assert_eq!(
            parse_optional_bool_env("INVALID_ERC20_TRANSFERS_ENABLED", false),
            Err("sometimes".to_string())
        );

        std::env::remove_var("INVALID_ERC20_TRANSFERS_ENABLED");
    }

    #[test]
    fn dis_retry_max_attempts_rejects_zero_and_invalid_values() {
        std::env::set_var("ZERO_DIS_RETRY_MAX_ATTEMPTS", "0");
        std::env::set_var("INVALID_DIS_RETRY_MAX_ATTEMPTS", "soon");

        assert_eq!(
            parse_positive_optional_u64_env("ZERO_DIS_RETRY_MAX_ATTEMPTS", 2),
            Err("0".to_string())
        );
        assert_eq!(
            parse_positive_optional_u64_env("INVALID_DIS_RETRY_MAX_ATTEMPTS", 2),
            Err("soon".to_string())
        );

        std::env::remove_var("ZERO_DIS_RETRY_MAX_ATTEMPTS");
        std::env::remove_var("INVALID_DIS_RETRY_MAX_ATTEMPTS");
    }

    #[test]
    fn from_env_rejects_invalid_dis_timeout() {
        let _guard = ENV_LOCK.lock().unwrap();
        let _timeout_snapshot = EnvVarSnapshot::capture("DIS_REQUEST_TIMEOUT_MS");
        let _retry_snapshot = EnvVarSnapshot::capture("DIS_RETRY_MAX_ATTEMPTS");
        std::env::remove_var("DIS_RETRY_MAX_ATTEMPTS");
        std::env::set_var("DIS_REQUEST_TIMEOUT_MS", "eventually");

        assert_eq!(
            Config::from_env(),
            Err(ConfigError::InvalidDisRequestTimeout(
                "eventually".to_string()
            ))
        );
    }

    #[test]
    fn from_env_rejects_zero_dis_retry_max_attempts() {
        let _guard = ENV_LOCK.lock().unwrap();
        let _timeout_snapshot = EnvVarSnapshot::capture("DIS_REQUEST_TIMEOUT_MS");
        let _retry_snapshot = EnvVarSnapshot::capture("DIS_RETRY_MAX_ATTEMPTS");
        std::env::remove_var("DIS_REQUEST_TIMEOUT_MS");
        std::env::set_var("DIS_RETRY_MAX_ATTEMPTS", "0");

        assert_eq!(
            Config::from_env(),
            Err(ConfigError::InvalidDisRetryMaxAttempts("0".to_string()))
        );
    }

    #[test]
    fn bigwig_timeout_defaults_and_rejects_zero_or_invalid_values() {
        assert_eq!(
            parse_positive_optional_u64_env("MISSING_BIGWIG_REQUEST_TIMEOUT", 30000).unwrap(),
            30000
        );

        std::env::set_var("ZERO_BIGWIG_REQUEST_TIMEOUT", "0");
        std::env::set_var("INVALID_BIGWIG_REQUEST_TIMEOUT", "eventually");

        assert_eq!(
            parse_positive_optional_u64_env("ZERO_BIGWIG_REQUEST_TIMEOUT", 30000),
            Err("0".to_string())
        );
        assert_eq!(
            parse_positive_optional_u64_env("INVALID_BIGWIG_REQUEST_TIMEOUT", 30000),
            Err("eventually".to_string())
        );

        std::env::remove_var("ZERO_BIGWIG_REQUEST_TIMEOUT");
        std::env::remove_var("INVALID_BIGWIG_REQUEST_TIMEOUT");
    }

    #[test]
    fn from_env_rejects_invalid_bigwig_timeout() {
        let _guard = ENV_LOCK.lock().unwrap();
        let _timeout_snapshot = EnvVarSnapshot::capture("BIGWIG_REQUEST_TIMEOUT_MS");
        std::env::set_var("BIGWIG_REQUEST_TIMEOUT_MS", "eventually");

        assert_eq!(
            Config::from_env(),
            Err(ConfigError::InvalidBigwigRequestTimeout(
                "eventually".to_string()
            ))
        );
    }

    #[test]
    fn from_env_parses_erc20_transfer_config() {
        let _guard = ENV_LOCK.lock().unwrap();
        let _snapshots = capture_env_vars(&[
            "ERC20_TRANSFERS_ENABLED",
            "ERC20_TRANSFERS_MAX_TOKEN_FILTERS",
            "BIGWIG_MAX_CONTRACT_ADDRESSES",
        ]);
        std::env::set_var("ERC20_TRANSFERS_ENABLED", "true");
        std::env::set_var("ERC20_TRANSFERS_MAX_TOKEN_FILTERS", "12");
        std::env::set_var("BIGWIG_MAX_CONTRACT_ADDRESSES", "30");

        let config = Config::from_env().unwrap();

        assert!(config.erc20_transfers_enabled);
        assert_eq!(config.erc20_transfers_max_token_filters, 12);
        assert_eq!(config.bigwig_max_contract_addresses, 30);
    }

    #[test]
    fn from_env_rejects_invalid_erc20_transfer_enabled_flag() {
        let _guard = ENV_LOCK.lock().unwrap();
        let _snapshots = capture_env_vars(&[
            "ERC20_TRANSFERS_ENABLED",
            "ERC20_TRANSFERS_MAX_TOKEN_FILTERS",
            "BIGWIG_MAX_CONTRACT_ADDRESSES",
        ]);
        std::env::set_var("ERC20_TRANSFERS_ENABLED", "maybe");
        std::env::remove_var("ERC20_TRANSFERS_MAX_TOKEN_FILTERS");
        std::env::remove_var("BIGWIG_MAX_CONTRACT_ADDRESSES");

        assert_eq!(
            Config::from_env(),
            Err(ConfigError::InvalidErc20TransfersEnabled(
                "maybe".to_string()
            ))
        );
    }

    #[test]
    fn from_env_rejects_invalid_or_zero_erc20_transfer_limits() {
        let _guard = ENV_LOCK.lock().unwrap();
        let _snapshots = capture_env_vars(&[
            "ERC20_TRANSFERS_ENABLED",
            "ERC20_TRANSFERS_MAX_TOKEN_FILTERS",
            "BIGWIG_MAX_CONTRACT_ADDRESSES",
        ]);
        std::env::remove_var("ERC20_TRANSFERS_ENABLED");
        std::env::remove_var("BIGWIG_MAX_CONTRACT_ADDRESSES");

        std::env::set_var("ERC20_TRANSFERS_MAX_TOKEN_FILTERS", "many");
        assert_eq!(
            Config::from_env(),
            Err(ConfigError::InvalidErc20TransfersMaxTokenFilters(
                "many".to_string()
            ))
        );

        std::env::set_var("ERC20_TRANSFERS_MAX_TOKEN_FILTERS", "0");
        assert_eq!(
            Config::from_env(),
            Err(ConfigError::InvalidErc20TransfersMaxTokenFilters(
                "0".to_string()
            ))
        );

        std::env::remove_var("ERC20_TRANSFERS_MAX_TOKEN_FILTERS");
        std::env::set_var("BIGWIG_MAX_CONTRACT_ADDRESSES", "many");
        assert_eq!(
            Config::from_env(),
            Err(ConfigError::InvalidBigwigMaxContractAddresses(
                "many".to_string()
            ))
        );

        std::env::set_var("BIGWIG_MAX_CONTRACT_ADDRESSES", "0");
        assert_eq!(
            Config::from_env(),
            Err(ConfigError::InvalidBigwigMaxContractAddresses(
                "0".to_string()
            ))
        );
    }

    #[test]
    fn from_env_rejects_public_erc20_limit_above_bigwig_limit() {
        let _guard = ENV_LOCK.lock().unwrap();
        let _snapshots = capture_env_vars(&[
            "ERC20_TRANSFERS_ENABLED",
            "ERC20_TRANSFERS_MAX_TOKEN_FILTERS",
            "BIGWIG_MAX_CONTRACT_ADDRESSES",
        ]);
        std::env::remove_var("ERC20_TRANSFERS_ENABLED");
        std::env::set_var("ERC20_TRANSFERS_MAX_TOKEN_FILTERS", "21");
        std::env::set_var("BIGWIG_MAX_CONTRACT_ADDRESSES", "20");

        assert_eq!(
            Config::from_env(),
            Err(ConfigError::Erc20TransfersPublicLimitExceedsBigwig {
                erc20_transfers_max_token_filters: 21,
                bigwig_max_contract_addresses: 20,
            })
        );
    }

    #[test]
    fn config_debug_redacts_bigwig_token() {
        let config = Config {
            infra_gateway_url: Some("http://infra-gateway-hub:8080".to_string()),
            infra_gateway_token: Some("super-secret".to_string()),
            ..Config::default()
        };
        let debug = format!("{config:?}");

        assert!(debug.contains("<redacted>"));
        assert!(!debug.contains("super-secret"));
    }

    #[test]
    fn optional_env_trims_values_and_treats_empty_as_missing() {
        std::env::set_var("TRIMMED_PRICE_INDEXER_URL", "  http://price-indexer:3010  ");
        std::env::set_var("EMPTY_PRICE_QL_INTERNAL_TOKEN", "   ");

        assert_eq!(
            optional_env("TRIMMED_PRICE_INDEXER_URL"),
            Some("http://price-indexer:3010".to_string())
        );
        assert_eq!(optional_env("EMPTY_PRICE_QL_INTERNAL_TOKEN"), None);

        std::env::remove_var("TRIMMED_PRICE_INDEXER_URL");
        std::env::remove_var("EMPTY_PRICE_QL_INTERNAL_TOKEN");
    }
}
