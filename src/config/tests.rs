use std::net::SocketAddr;
use std::sync::Mutex;

use crate::config::env::{
    optional_env, parse_optional_bool_env, parse_optional_u64_env, parse_positive_optional_u64_env,
    Config,
};
use crate::config::error::ConfigError;
use crate::test_utils::constants::{INFRA_GATEWAY_URL, PRICE_INDEXER_URL};

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
    assert!(!parse_optional_bool_env("MISSING_ERC20_TRANSFERS_ENABLED", false).unwrap());

    std::env::set_var("EMPTY_ERC20_TRANSFERS_ENABLED", "   ");
    std::env::set_var("TRUE_ERC20_TRANSFERS_ENABLED", " TRUE ");
    std::env::set_var("ONE_ERC20_TRANSFERS_ENABLED", "1");
    std::env::set_var("FALSE_ERC20_TRANSFERS_ENABLED", " false ");
    std::env::set_var("ZERO_ERC20_TRANSFERS_ENABLED", "0");

    assert!(parse_optional_bool_env("EMPTY_ERC20_TRANSFERS_ENABLED", true).unwrap());
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
        infra_gateway_url: Some(INFRA_GATEWAY_URL.to_string()),
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
        Some(PRICE_INDEXER_URL.to_string())
    );
    assert_eq!(optional_env("EMPTY_PRICE_QL_INTERNAL_TOKEN"), None);

    std::env::remove_var("TRIMMED_PRICE_INDEXER_URL");
    std::env::remove_var("EMPTY_PRICE_QL_INTERNAL_TOKEN");
}
