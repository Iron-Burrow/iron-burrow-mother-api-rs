use axum::Router;

use crate::{
    adapters::http::router::build_router, config::Config, state::AppState,
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
