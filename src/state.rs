use std::sync::Arc;

use sqlx::PgPool;

use crate::adapters::bigwig::{client::create_bigwig_client, BigwigClient};
use crate::adapters::dis::{client::create_dis_client, DisClient};
use crate::adapters::http::rate_limit::ApiKeyMinuteLimiter;
use crate::adapters::postgres::{
    AccountRepository, ApiKeyRepository, DefiProtocolRepository, GlobalAssetRepository,
    PortfolioSimulationRepository, WorkspaceRepository,
};
use crate::adapters::price_indexer::{client::create_price_indexer_client, PriceIndexerClient};
use crate::config::Config;
use crate::domain::canonical_registry::{CanonicalRegistry, CanonicalRegistryError};
use crate::infra::db;

#[derive(Debug, thiserror::Error)]
pub(crate) enum AppStateError {
    #[error("canonical registry initialization failed")]
    CanonicalRegistry(#[source] CanonicalRegistryError),
    #[error("database pool initialization failed")]
    Database(#[source] sqlx::Error),
}

#[derive(Clone, Debug)]
pub(crate) struct AppState {
    pub(crate) config: Config,
    pub(crate) version: &'static str,
    #[allow(dead_code)] // Runtime consumers arrive in later SPEC-033 PRs.
    pub(crate) canonical_registry: Arc<CanonicalRegistry>,
    pub(crate) database_pool: Option<PgPool>,
    pub(crate) api_key_repository: Option<ApiKeyRepository>,
    pub(crate) account_repository: Option<AccountRepository>,
    pub(crate) workspace_repository: Option<WorkspaceRepository>,
    pub(crate) portfolio_simulation_repository: Option<PortfolioSimulationRepository>,
    pub(crate) api_key_minute_limiter: ApiKeyMinuteLimiter,
    pub(crate) asset_repository: Option<GlobalAssetRepository>,
    pub(crate) defi_protocol_repository: Option<DefiProtocolRepository>,
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

    pub(crate) fn try_new(config: Config) -> Result<Self, AppStateError> {
        Self::try_new_with_registry(config, CanonicalRegistry::from_embedded_catalog)
    }

    pub(crate) fn try_new_with_registry<F>(
        config: Config,
        build_registry: F,
    ) -> Result<Self, AppStateError>
    where
        F: FnOnce() -> Result<CanonicalRegistry, CanonicalRegistryError>,
    {
        let canonical_registry =
            Arc::new(build_registry().map_err(AppStateError::CanonicalRegistry)?);
        let database_pool =
            db::create_pool(config.database_url.as_deref()).map_err(AppStateError::Database)?;
        let api_key_repository = database_pool.clone().map(ApiKeyRepository::database);
        let account_repository = database_pool.clone().map(AccountRepository::database);
        let workspace_repository = database_pool.clone().map(WorkspaceRepository::database);
        let portfolio_simulation_repository = database_pool
            .clone()
            .map(PortfolioSimulationRepository::database);
        let asset_repository = database_pool.clone().map(GlobalAssetRepository::database);
        let defi_protocol_repository = database_pool.clone().map(DefiProtocolRepository::database);
        let price_indexer_client = create_price_indexer_client(&config);
        let dis_client = create_dis_client(&config);
        let bigwig_client = create_bigwig_client(&config);

        Ok(Self {
            config,
            version: env!("CARGO_PKG_VERSION"),
            canonical_registry,
            database_pool,
            api_key_repository,
            account_repository,
            workspace_repository,
            portfolio_simulation_repository,
            api_key_minute_limiter: ApiKeyMinuteLimiter::default(),
            asset_repository,
            defi_protocol_repository,
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
            canonical_registry: embedded_canonical_registry(),
            database_pool: None,
            api_key_repository: None,
            account_repository: None,
            workspace_repository: None,
            portfolio_simulation_repository: None,
            api_key_minute_limiter: ApiKeyMinuteLimiter::default(),
            asset_repository: Some(asset_repository),
            defi_protocol_repository: None,
            price_indexer_client: None,
            dis_client: None,
            bigwig_client: None,
        }
    }

    #[cfg(test)]
    pub(crate) fn with_asset_repository_and_bigwig_client(
        config: Config,
        asset_repository: GlobalAssetRepository,
        bigwig_client: BigwigClient,
    ) -> Self {
        Self {
            config,
            version: env!("CARGO_PKG_VERSION"),
            canonical_registry: embedded_canonical_registry(),
            database_pool: None,
            api_key_repository: None,
            account_repository: None,
            workspace_repository: None,
            portfolio_simulation_repository: None,
            api_key_minute_limiter: ApiKeyMinuteLimiter::default(),
            asset_repository: Some(asset_repository),
            defi_protocol_repository: None,
            price_indexer_client: None,
            dis_client: None,
            bigwig_client: Some(bigwig_client),
        }
    }
}

#[cfg(test)]
pub(crate) fn embedded_canonical_registry() -> Arc<CanonicalRegistry> {
    Arc::new(
        CanonicalRegistry::from_embedded_catalog()
            .expect("embedded catalog should construct the canonical registry"),
    )
}

#[cfg(test)]
mod tests;
