use std::sync::Arc;

use sqlx::PgPool;

use crate::adapters::bigwig::BigwigClient;
use crate::adapters::http::rate_limit::ApiKeyMinuteLimiter;
use crate::adapters::postgres::{
    AccountRepository, ApiKeyRepository, PortfolioSimulationRepository, WorkspaceRepository,
};
use crate::config::Config;
use crate::domain::canonical_registry::CanonicalRegistry;
use crate::domain::verified_protocol_registry::VerifiedProtocolRegistry;
use crate::{
    adapters::price_indexer::{PriceIndexerBalanceQuoteReader, PriceIndexerClient},
    application::balances::service::BalanceSnapshotService,
};

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
    #[allow(dead_code)]
    pub(crate) bigwig_client: Option<BigwigClient>,
    pub(crate) balance_service: BalanceSnapshotService<PriceIndexerBalanceQuoteReader>,
}

#[cfg(test)]
pub(crate) struct HttpStateTestBuilder {
    state: HttpState,
}

#[cfg(test)]
impl HttpStateTestBuilder {
    pub(crate) fn new(config: Config) -> Self {
        Self {
            state: crate::bootstrap::http::build_http_state(config)
                .expect("HTTP state should be built from config"),
        }
    }

    pub(crate) fn with_api_key_repository(mut self, api_key_repository: ApiKeyRepository) -> Self {
        self.state.api_key_repository = Some(api_key_repository);
        self
    }

    pub(crate) fn with_price_indexer_client(
        mut self,
        price_indexer_client: PriceIndexerClient,
    ) -> Self {
        self.state.price_indexer_client = Some(price_indexer_client);
        self
    }

    pub(crate) fn with_bigwig_client(mut self, bigwig_client: BigwigClient) -> Self {
        self.state.bigwig_client = Some(bigwig_client);
        self
    }

    pub(crate) fn build(self) -> HttpState {
        let mut state = self.state;
        state.rebuild_balance_service();
        state
    }
}

#[cfg(test)]
impl HttpState {
    fn rebuild_balance_service(&mut self) {
        self.balance_service = BalanceSnapshotService::new(
            crate::application::balances::catalog::CatalogBalanceTargetResolver::new(
                self.canonical_registry.clone(),
            ),
            self.bigwig_client.clone(),
            self.price_indexer_client
                .clone()
                .map(PriceIndexerBalanceQuoteReader::new),
        );
    }
}
