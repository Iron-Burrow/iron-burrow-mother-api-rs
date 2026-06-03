use std::{env, net::SocketAddr};

const DEFAULT_APP_ENV: &str = "development";
const DEFAULT_HTTP_HOST: &str = "0.0.0.0";
const DEFAULT_HTTP_PORT: u16 = 3000;
pub const DEFAULT_PRICE_INDEXER_TIMEOUT_MS: u64 = 2000;
pub const DEFAULT_DIS_REQUEST_TIMEOUT_MS: u64 = 5000;
pub const DEFAULT_DIS_RETRY_MAX_ATTEMPTS: u64 = 2;

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
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        Ok(Self {
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
        })
    }

    pub fn socket_addr(&self) -> Result<SocketAddr, ConfigError> {
        format!("{}:{}", self.http_host, self.http_port)
            .parse()
            .map_err(|_| ConfigError::InvalidSocketAddress {
                host: self.http_host.clone(),
                port: self.http_port,
            })
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

#[derive(Debug, Eq, PartialEq)]
pub enum ConfigError {
    InvalidHttpPort(String),
    InvalidPriceIndexerTimeout(String),
    InvalidDisRequestTimeout(String),
    InvalidDisRetryMaxAttempts(String),
    InvalidSocketAddress { host: String, port: u16 },
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
