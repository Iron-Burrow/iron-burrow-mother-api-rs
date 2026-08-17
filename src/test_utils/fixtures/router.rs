use axum::Router;

use crate::{
    adapters::{bigwig::BigwigClient, http::router::build_router},
    config::Config,
    state::AppState,
};

pub(crate) fn transfers_router(config: Config) -> Router {
    build_router(AppState::for_tests(config))
}

pub(crate) fn transfers_router_without_repository(config: Config) -> Router {
    build_router(AppState::new(config))
}

pub(crate) fn transfers_router_with_bigwig_client(
    config: Config,
    bigwig_client: BigwigClient,
) -> Router {
    build_router(AppState::for_tests_with_bigwig_client(
        config,
        bigwig_client,
    ))
}
