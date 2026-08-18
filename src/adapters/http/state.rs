use std::sync::Arc;

use sqlx::PgPool;

use crate::adapters::bigwig::BigwigClient;
use crate::adapters::dis::DisClient;
use crate::adapters::http::rate_limit::ApiKeyMinuteLimiter;
use crate::adapters::postgres::{
    AccountRepository, ApiKeyRepository, PortfolioSimulationRepository, WorkspaceRepository,
};
use crate::adapters::price_indexer::PriceIndexerClient;
use crate::config::Config;
use crate::domain::canonical_registry::CanonicalRegistry;
use crate::domain::verified_protocol_registry::VerifiedProtocolRegistry;

#[derive(Clone, Debug)]
pub(crate) struct HttpState {
    pub(crate) config: Config,
    pub(crate) version: &'static str,
    pub(crate) canonical_registry: Arc<CanonicalRegistry>,
    pub(crate) verified_protocol_registry: Arc<VerifiedProtocolRegistry>,
    pub(crate) database_pool: Option<PgPool>,
    pub(crate) api_key_repository: Option<ApiKeyRepository>,
    pub(crate) account_repository: Option<AccountRepository>,
    pub(crate) workspace_repository: Option<WorkspaceRepository>,
    pub(crate) portfolio_simulation_repository: Option<PortfolioSimulationRepository>,
    pub(crate) api_key_minute_limiter: ApiKeyMinuteLimiter,
    pub(crate) price_indexer_client: Option<PriceIndexerClient>,
    pub(crate) dis_client: Option<DisClient>,
    #[allow(dead_code)]
    pub(crate) bigwig_client: Option<BigwigClient>,
}

#[cfg(test)]
pub(crate) fn embedded_canonical_registry() -> Arc<CanonicalRegistry> {
    Arc::new(
        CanonicalRegistry::from_embedded_catalog()
            .expect("embedded catalog should construct the canonical registry"),
    )
}

#[cfg(test)]
pub(crate) fn embedded_verified_protocol_registry() -> Arc<VerifiedProtocolRegistry> {
    let canonical = embedded_canonical_registry();
    Arc::new(
        VerifiedProtocolRegistry::from_embedded(&canonical)
            .expect("embedded protocol declarations should construct the registry"),
    )
}

#[cfg(test)]
impl HttpState {
    #[allow(dead_code)]
    pub(crate) fn new(config: Config) -> Self {
        crate::bootstrap::http::build_http_state(config)
            .expect("HTTP state should be built from config")
    }

    pub(crate) fn for_tests(config: Config) -> Self {
        Self::new(config)
    }

    pub(crate) fn for_tests_with_bigwig_client(
        config: Config,
        bigwig_client: BigwigClient,
    ) -> Self {
        let mut state = Self::new(config);
        state.bigwig_client = Some(bigwig_client);
        state
    }
}
