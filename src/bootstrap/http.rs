use std::sync::Arc;

use crate::{
    adapters::{
        bigwig::client::create_bigwig_client,
        dis::client::create_dis_client,
        http::{rate_limit::ApiKeyMinuteLimiter, state::HttpState},
        postgres::{
            AccountRepository, ApiKeyRepository, PortfolioSimulationRepository, WorkspaceRepository,
        },
        price_indexer::client::create_price_indexer_client,
    },
    config::Config,
    domain::{
        canonical_registry::{CanonicalRegistry, CanonicalRegistryError},
        verified_protocol_registry::{VerifiedProtocolRegistry, VerifiedProtocolRegistryError},
    },
    infra::db,
};

#[derive(Debug, thiserror::Error)]
pub(crate) enum BootstrapError {
    #[error("canonical registry initialization failed")]
    CanonicalRegistry(#[source] CanonicalRegistryError),
    #[error("verified protocol registry initialization failed")]
    VerifiedProtocolRegistry(#[source] VerifiedProtocolRegistryError),
    #[error("database pool initialization failed")]
    Database(#[source] sqlx::Error),
}

pub(crate) fn build_http_state(config: Config) -> Result<HttpState, BootstrapError> {
    build_http_state_with_registry(config, CanonicalRegistry::from_embedded_catalog)
}

fn build_http_state_with_registry<F>(
    config: Config,
    build_registry: F,
) -> Result<HttpState, BootstrapError>
where
    F: FnOnce() -> Result<CanonicalRegistry, CanonicalRegistryError>,
{
    let canonical_registry = Arc::new(build_registry().map_err(BootstrapError::CanonicalRegistry)?);
    let verified_protocol_registry = Arc::new(
        VerifiedProtocolRegistry::from_embedded(&canonical_registry)
            .map_err(BootstrapError::VerifiedProtocolRegistry)?,
    );
    let database_pool =
        db::create_pool(config.database_url.as_deref()).map_err(BootstrapError::Database)?;
    let api_key_repository = database_pool.clone().map(ApiKeyRepository::database);
    let account_repository = database_pool.clone().map(AccountRepository::database);
    let workspace_repository = database_pool.clone().map(WorkspaceRepository::database);
    let portfolio_simulation_repository = database_pool
        .clone()
        .map(PortfolioSimulationRepository::database);
    let price_indexer_client = create_price_indexer_client(&config);
    let dis_client = create_dis_client(&config);
    let bigwig_client = create_bigwig_client(&config);

    Ok(HttpState {
        config,
        version: env!("CARGO_PKG_VERSION"),
        canonical_registry,
        verified_protocol_registry,
        database_pool,
        api_key_repository,
        account_repository,
        workspace_repository,
        portfolio_simulation_repository,
        api_key_minute_limiter: ApiKeyMinuteLimiter::default(),
        price_indexer_client,
        dis_client,
        bigwig_client,
    })
}

#[cfg(test)]
pub(crate) fn build_http_state_with_registry_for_test<F>(
    config: Config,
    build_registry: F,
) -> Result<HttpState, BootstrapError>
where
    F: FnOnce() -> Result<CanonicalRegistry, CanonicalRegistryError>,
{
    build_http_state_with_registry(config, build_registry)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::constants::{DIS_BASE_URL, INFRA_GATEWAY_URL};

    #[test]
    fn registry_constructs_without_a_database_pool() {
        let state = build_http_state(Config::default()).unwrap();

        assert!(state.database_pool.is_none());
        assert!(state.canonical_registry.asset_by_slug("usdc").is_some());
    }

    #[test]
    fn registry_startup_error_is_sanitized_and_retains_its_source() {
        let error = build_http_state_with_registry_for_test(Config::default(), || {
            Err(CanonicalRegistryError::Invalid(
                "fixture must not reach startup output".to_string(),
            ))
        })
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            "canonical registry initialization failed"
        );
        assert!(!error.to_string().contains("fixture"));
        assert!(std::error::Error::source(&error).is_some());
    }

    #[test]
    fn missing_dis_base_url_disables_client() {
        let state = build_http_state(Config::default()).unwrap();

        assert!(state.dis_client.is_none());
    }

    #[test]
    fn valid_dis_base_url_creates_client() {
        let state = build_http_state(Config {
            dis_base_url: Some(DIS_BASE_URL.to_string()),
            ..Config::default()
        })
        .unwrap();

        assert!(state.dis_client.is_some());
    }

    #[test]
    fn invalid_dis_base_url_disables_client_without_failing_startup() {
        let state = build_http_state(Config {
            dis_base_url: Some("not a url".to_string()),
            ..Config::default()
        })
        .unwrap();

        assert!(state.dis_client.is_none());
    }

    #[test]
    fn missing_bigwig_config_disables_client() {
        let state = build_http_state(Config::default()).unwrap();

        assert!(state.bigwig_client.is_none());
    }

    #[test]
    fn valid_bigwig_config_creates_client() {
        let state = build_http_state(Config {
            infra_gateway_url: Some(INFRA_GATEWAY_URL.to_string()),
            infra_gateway_token: Some("test-token".to_string()),
            ..Config::default()
        })
        .unwrap();

        let client = state
            .bigwig_client
            .expect("valid Bigwig config should create a client");
        assert_eq!(client.base_host(), Some("infra-gateway-hub"));
        assert_eq!(client.timeout_ms(), 30000);
    }

    #[test]
    fn partial_bigwig_config_disables_client_without_failing_startup() {
        for config in [
            Config {
                infra_gateway_url: Some(INFRA_GATEWAY_URL.to_string()),
                ..Config::default()
            },
            Config {
                infra_gateway_token: Some("test-token".to_string()),
                ..Config::default()
            },
        ] {
            let state = build_http_state(config).unwrap();
            assert!(state.bigwig_client.is_none());
        }
    }

    #[test]
    fn invalid_bigwig_url_disables_client_without_failing_startup() {
        let state = build_http_state(Config {
            infra_gateway_url: Some("not a url".to_string()),
            infra_gateway_token: Some("test-token".to_string()),
            ..Config::default()
        })
        .unwrap();

        assert!(state.bigwig_client.is_none());
    }
}
