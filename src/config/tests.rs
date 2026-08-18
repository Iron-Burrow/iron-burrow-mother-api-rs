use std::net::SocketAddr;
use std::sync::Mutex;

use crate::config::env::{
    optional_env, parse_optional_bool_env, parse_optional_public_api_surface_env,
    parse_optional_u64_env, parse_positive_optional_u64_env, Config, PublicApiSurface,
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
    assert_eq!(config.public_api_surface, PublicApiSurface::Alpha);
    assert_eq!(config.http_host, "0.0.0.0");
    assert_eq!(config.http_port, 3000);
    assert_eq!(config.public_api_base_url, "http://localhost:3000");
    assert_eq!(config.database_url, None);
    assert_eq!(config.price_indexer_url, None);
    assert_eq!(config.price_ql_internal_token, None);
    assert_eq!(config.price_indexer_timeout_ms, 2000);
    assert_eq!(config.infra_gateway_url, None);
    assert_eq!(config.infra_gateway_token, None);
    assert_eq!(config.bigwig_report_outcome_token, None);
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
fn public_api_surface_config_defaults_trims_and_parses_known_values() {
    assert_eq!(
        parse_optional_public_api_surface_env(
            "MISSING_PUBLIC_API_SURFACE",
            PublicApiSurface::Alpha
        )
        .unwrap(),
        PublicApiSurface::Alpha
    );

    std::env::set_var("EMPTY_PUBLIC_API_SURFACE", "   ");
    std::env::set_var("ALPHA_PUBLIC_API_SURFACE", " ALPHA ");
    std::env::set_var("BETA_PUBLIC_API_SURFACE", " beta ");

    assert_eq!(
        parse_optional_public_api_surface_env("EMPTY_PUBLIC_API_SURFACE", PublicApiSurface::Beta)
            .unwrap(),
        PublicApiSurface::Beta
    );
    assert_eq!(
        parse_optional_public_api_surface_env("ALPHA_PUBLIC_API_SURFACE", PublicApiSurface::Beta)
            .unwrap(),
        PublicApiSurface::Alpha
    );
    assert_eq!(
        parse_optional_public_api_surface_env("BETA_PUBLIC_API_SURFACE", PublicApiSurface::Alpha)
            .unwrap(),
        PublicApiSurface::Beta
    );

    std::env::remove_var("EMPTY_PUBLIC_API_SURFACE");
    std::env::remove_var("ALPHA_PUBLIC_API_SURFACE");
    std::env::remove_var("BETA_PUBLIC_API_SURFACE");
}

#[test]
fn public_api_surface_config_rejects_invalid_values() {
    std::env::set_var("INVALID_PUBLIC_API_SURFACE", " gamma ");

    assert_eq!(
        parse_optional_public_api_surface_env(
            "INVALID_PUBLIC_API_SURFACE",
            PublicApiSurface::Alpha
        ),
        Err("gamma".to_string())
    );

    std::env::remove_var("INVALID_PUBLIC_API_SURFACE");
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
fn from_env_parses_public_api_surface_config() {
    let _guard = ENV_LOCK.lock().unwrap();
    let _snapshot = EnvVarSnapshot::capture("PUBLIC_API_SURFACE");

    std::env::set_var("PUBLIC_API_SURFACE", "beta");
    assert_eq!(
        Config::from_env().unwrap().public_api_surface,
        PublicApiSurface::Beta
    );

    std::env::set_var("PUBLIC_API_SURFACE", "alpha");
    assert_eq!(
        Config::from_env().unwrap().public_api_surface,
        PublicApiSurface::Alpha
    );
}

#[test]
fn from_env_uses_trimmed_public_api_base_url_or_local_default() {
    let _guard = ENV_LOCK.lock().unwrap();
    let _snapshot = EnvVarSnapshot::capture("PUBLIC_API_BASE_URL");

    std::env::remove_var("PUBLIC_API_BASE_URL");
    assert_eq!(
        Config::from_env().unwrap().public_api_base_url,
        "http://localhost:3000"
    );

    std::env::set_var("PUBLIC_API_BASE_URL", " https://api.example.test/ ");
    assert_eq!(
        Config::from_env().unwrap().public_api_base_url,
        "https://api.example.test/"
    );
}

#[test]
fn production_account_entry_requires_a_non_default_public_web_url() {
    let _guard = ENV_LOCK.lock().unwrap();
    let _snapshots = capture_env_vars(&[
        "APP_ENV",
        "PUBLIC_WEB_BASE_URL",
        "ACCOUNT_EMAIL_LOOKUP_PEPPER",
    ]);
    std::env::set_var("APP_ENV", "production");
    std::env::set_var("ACCOUNT_EMAIL_LOOKUP_PEPPER", "test-pepper");

    std::env::remove_var("PUBLIC_WEB_BASE_URL");
    assert_eq!(
        Config::from_env(),
        Err(ConfigError::MissingProductionAccountEntryConfig)
    );

    std::env::set_var("PUBLIC_WEB_BASE_URL", "http://localhost:3000");
    assert_eq!(
        Config::from_env(),
        Err(ConfigError::MissingProductionAccountEntryConfig)
    );

    std::env::set_var("PUBLIC_WEB_BASE_URL", "https://www.example.test");
    assert!(Config::from_env().is_ok());
}

#[test]
fn from_env_rejects_invalid_public_api_surface() {
    let _guard = ENV_LOCK.lock().unwrap();
    let _snapshot = EnvVarSnapshot::capture("PUBLIC_API_SURFACE");

    std::env::set_var("PUBLIC_API_SURFACE", "staging");

    assert_eq!(
        Config::from_env(),
        Err(ConfigError::InvalidPublicApiSurface("staging".to_string()))
    );
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
        bigwig_report_outcome_token: Some("outcome-secret".to_string()),
        ..Config::default()
    };
    let debug = format!("{config:?}");

    assert!(debug.contains("<redacted>"));
    assert!(!debug.contains("super-secret"));
    assert!(!debug.contains("outcome-secret"));
}

#[test]
fn async_reports_require_a_dedicated_outcome_token() {
    let _guard = ENV_LOCK.lock().unwrap();
    let _snapshots = capture_env_vars(&["ASYNC_REPORTS_ENABLED", "BIGWIG_REPORT_OUTCOME_TOKEN"]);
    std::env::set_var("ASYNC_REPORTS_ENABLED", "true");
    std::env::remove_var("BIGWIG_REPORT_OUTCOME_TOKEN");

    assert_eq!(
        Config::from_env(),
        Err(ConfigError::MissingBigwigReportOutcomeToken)
    );

    std::env::set_var("BIGWIG_REPORT_OUTCOME_TOKEN", "   ");
    assert_eq!(
        Config::from_env(),
        Err(ConfigError::MissingBigwigReportOutcomeToken)
    );

    std::env::set_var("BIGWIG_REPORT_OUTCOME_TOKEN", "outcome-token");
    assert_eq!(
        Config::from_env().unwrap().bigwig_report_outcome_token,
        Some("outcome-token".to_string())
    );
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
