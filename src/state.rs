use sqlx::PgPool;
use tracing::warn;

use crate::{
    adapters::global_assets::GlobalAssetRepository, balances::bigwig::BigwigLatestBalancesClient,
    config::Config, dis::DisClient, infra::db, price_indexer::PriceIndexerClient,
};

#[derive(Clone, Debug)]
pub struct AppState {
    pub config: Config,
    pub version: &'static str,
    pub database_pool: Option<PgPool>,
    pub asset_repository: Option<GlobalAssetRepository>,
    pub price_indexer_client: Option<PriceIndexerClient>,
    pub dis_client: Option<DisClient>,
    #[allow(dead_code)]
    pub bigwig_latest_balances_client: Option<BigwigLatestBalancesClient>,
}

impl AppState {
    #[allow(dead_code)]
    pub fn new(config: Config) -> Self {
        Self::try_new(config).expect("app state should be created from config")
    }

    pub fn try_new(config: Config) -> Result<Self, sqlx::Error> {
        let database_pool = db::create_pool(config.database_url.as_deref())?;
        let asset_repository = database_pool.clone().map(GlobalAssetRepository::database);
        let price_indexer_client = create_price_indexer_client(&config);
        let dis_client = create_dis_client(&config);
        let bigwig_latest_balances_client = create_bigwig_latest_balances_client(&config);

        Ok(Self {
            config,
            version: env!("CARGO_PKG_VERSION"),
            database_pool,
            asset_repository,
            price_indexer_client,
            dis_client,
            bigwig_latest_balances_client,
        })
    }

    #[cfg(test)]
    pub fn with_asset_repository(config: Config, asset_repository: GlobalAssetRepository) -> Self {
        Self {
            config,
            version: env!("CARGO_PKG_VERSION"),
            database_pool: None,
            asset_repository: Some(asset_repository),
            price_indexer_client: None,
            dis_client: None,
            bigwig_latest_balances_client: None,
        }
    }
}

fn create_price_indexer_client(config: &Config) -> Option<PriceIndexerClient> {
    match (
        config.price_indexer_url.as_deref(),
        config.price_ql_internal_token.as_deref(),
    ) {
        (Some(url), Some(token)) => {
            match PriceIndexerClient::new(url, token, config.price_indexer_timeout_ms) {
                Ok(client) => Some(client),
                Err(error) => {
                    warn!(
                        %error,
                        "Price indexer config is invalid; price enrichment disabled"
                    );
                    None
                }
            }
        }
        (None, None) => None,
        (url, token) => {
            warn!(
                price_indexer_url_configured = url.is_some(),
                price_ql_internal_token_configured = token.is_some(),
                "Price indexer config is incomplete; price enrichment disabled"
            );
            None
        }
    }
}

fn create_dis_client(config: &Config) -> Option<DisClient> {
    match config.dis_base_url.as_deref() {
        Some(url) => match DisClient::new(
            url,
            config.dis_request_timeout_ms,
            config.dis_retry_max_attempts,
        ) {
            Ok(client) => Some(client),
            Err(error) => {
                warn!(%error, "DIS config is invalid; DIS integration disabled");
                None
            }
        },
        None => None,
    }
}

fn create_bigwig_latest_balances_client(config: &Config) -> Option<BigwigLatestBalancesClient> {
    match (
        config.infra_gateway_url.as_deref(),
        config.infra_gateway_token.as_deref(),
    ) {
        (Some(url), Some(token)) => {
            match BigwigLatestBalancesClient::new(url, token, config.bigwig_request_timeout_ms) {
                Ok(client) => Some(client),
                Err(error) => {
                    warn!(%error, "Bigwig config is invalid; latest-balance integration disabled");
                    None
                }
            }
        }
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
mod tests {
    use super::*;

    #[test]
    fn missing_dis_base_url_disables_client() {
        let state = AppState::new(Config::default());

        assert!(state.dis_client.is_none());
    }

    #[test]
    fn valid_dis_base_url_creates_client() {
        let state = AppState::new(Config {
            dis_base_url: Some("http://dis:8000".to_string()),
            ..Config::default()
        });

        assert!(state.dis_client.is_some());
    }

    #[test]
    fn invalid_dis_base_url_disables_client_without_failing_startup() {
        let state = AppState::new(Config {
            dis_base_url: Some("not a url".to_string()),
            ..Config::default()
        });

        assert!(state.dis_client.is_none());
    }

    #[test]
    fn missing_bigwig_config_disables_client() {
        let state = AppState::new(Config::default());

        assert!(state.bigwig_latest_balances_client.is_none());
    }

    #[test]
    fn valid_bigwig_config_creates_client() {
        let state = AppState::new(Config {
            infra_gateway_url: Some("http://infra-gateway-hub:8080".to_string()),
            infra_gateway_token: Some("test-token".to_string()),
            ..Config::default()
        });

        let client = state
            .bigwig_latest_balances_client
            .expect("valid Bigwig config should create a client");
        assert_eq!(client.base_host(), Some("infra-gateway-hub"));
        assert_eq!(client.timeout_ms(), 30000);
    }

    #[test]
    fn partial_bigwig_config_disables_client_without_failing_startup() {
        for config in [
            Config {
                infra_gateway_url: Some("http://infra-gateway-hub:8080".to_string()),
                ..Config::default()
            },
            Config {
                infra_gateway_token: Some("test-token".to_string()),
                ..Config::default()
            },
        ] {
            let state = AppState::new(config);
            assert!(state.bigwig_latest_balances_client.is_none());
        }
    }

    #[test]
    fn invalid_bigwig_url_disables_client_without_failing_startup() {
        let state = AppState::new(Config {
            infra_gateway_url: Some("not a url".to_string()),
            infra_gateway_token: Some("test-token".to_string()),
            ..Config::default()
        });

        assert!(state.bigwig_latest_balances_client.is_none());
    }
}
