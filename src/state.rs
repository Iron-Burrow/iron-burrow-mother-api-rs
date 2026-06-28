use sqlx::PgPool;

use crate::adapters::bigwig::{client::create_bigwig_client, BigwigClient};
use crate::adapters::dis::{client::create_dis_client, DisClient};
use crate::adapters::postgres::GlobalAssetRepository;
use crate::adapters::price_indexer::{client::create_price_indexer_client, PriceIndexerClient};
use crate::config::Config;
use crate::infra::db;

#[derive(Clone, Debug)]
pub(crate) struct AppState {
    pub(crate) config: Config,
    pub(crate) version: &'static str,
    pub(crate) database_pool: Option<PgPool>,
    pub(crate) asset_repository: Option<GlobalAssetRepository>,
    pub(crate) price_indexer_client: Option<PriceIndexerClient>,
    pub(crate) dis_client: Option<DisClient>,
    #[allow(dead_code)]
    pub(crate) bigwig_client: Option<BigwigClient>,
}

impl AppState {
    #[allow(dead_code)]
    pub(crate) fn new(config: Config) -> Self {
        Self::try_new(config).expect("app state should be created from config")
    }

    pub(crate) fn try_new(config: Config) -> Result<Self, sqlx::Error> {
        let database_pool = db::create_pool(config.database_url.as_deref())?;
        let asset_repository = database_pool.clone().map(GlobalAssetRepository::database);
        let price_indexer_client = create_price_indexer_client(&config);
        let dis_client = create_dis_client(&config);
        let bigwig_client = create_bigwig_client(&config);

        Ok(Self {
            config,
            version: env!("CARGO_PKG_VERSION"),
            database_pool,
            asset_repository,
            price_indexer_client,
            dis_client,
            bigwig_client,
        })
    }

    #[cfg(test)]
    pub(crate) fn with_asset_repository(
        config: Config,
        asset_repository: GlobalAssetRepository,
    ) -> Self {
        Self {
            config,
            version: env!("CARGO_PKG_VERSION"),
            database_pool: None,
            asset_repository: Some(asset_repository),
            price_indexer_client: None,
            dis_client: None,
            bigwig_client: None,
        }
    }
}

#[cfg(test)]
mod tests;
