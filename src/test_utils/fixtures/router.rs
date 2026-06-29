use axum::Router;

use crate::{
    adapters::{bigwig::BigwigClient, http::router::build_router},
    config::Config,
    state::AppState,
    test_utils::fixtures::global_assets::global_assets_repository,
};

pub(crate) fn transfers_router(config: Config) -> Router {
    build_router(AppState::with_asset_repository(
        config,
        global_assets_repository(),
    ))
}

pub(crate) fn transfers_router_without_repository(config: Config) -> Router {
    build_router(AppState::new(config))
}

pub(crate) fn transfers_router_with_bigwig_client(
    config: Config,
    bigwig_client: BigwigClient,
) -> Router {
    build_router(AppState::with_asset_repository_and_bigwig_client(
        config,
        global_assets_repository(),
        bigwig_client,
    ))
}
