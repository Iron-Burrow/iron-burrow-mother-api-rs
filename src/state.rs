use sqlx::PgPool;
use tracing::warn;

use crate::{
    config::Config, db, price_indexer::PriceIndexerClient,
    repositories::global_assets::GlobalAssetRepository,
};

#[derive(Clone, Debug)]
pub struct AppState {
    pub config: Config,
    pub version: &'static str,
    pub database_pool: Option<PgPool>,
    pub asset_repository: Option<GlobalAssetRepository>,
    pub price_indexer_client: Option<PriceIndexerClient>,
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

        Ok(Self {
            config,
            version: env!("CARGO_PKG_VERSION"),
            database_pool,
            asset_repository,
            price_indexer_client,
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
