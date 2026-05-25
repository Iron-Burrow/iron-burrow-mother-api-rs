use sqlx::PgPool;

use crate::{config::Config, db, repositories::global_assets::GlobalAssetRepository};

#[derive(Clone, Debug)]
pub struct AppState {
    pub config: Config,
    pub version: &'static str,
    pub database_pool: Option<PgPool>,
    pub asset_repository: Option<GlobalAssetRepository>,
}

impl AppState {
    #[allow(dead_code)]
    pub fn new(config: Config) -> Self {
        Self::try_new(config).expect("app state should be created from config")
    }

    pub fn try_new(config: Config) -> Result<Self, sqlx::Error> {
        let database_pool = db::create_pool(config.database_url.as_deref())?;
        let asset_repository = database_pool.clone().map(GlobalAssetRepository::database);

        Ok(Self {
            config,
            version: env!("CARGO_PKG_VERSION"),
            database_pool,
            asset_repository,
        })
    }

    #[cfg(test)]
    pub fn with_asset_repository(config: Config, asset_repository: GlobalAssetRepository) -> Self {
        Self {
            config,
            version: env!("CARGO_PKG_VERSION"),
            database_pool: None,
            asset_repository: Some(asset_repository),
        }
    }
}
