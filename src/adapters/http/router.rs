use axum::{
    http::{header::USER_AGENT, HeaderMap, Method, StatusCode, Uri},
    middleware,
    routing::{get, post},
    Router,
};
use tower_http::trace::TraceLayer;
use tracing::{debug, warn};

use crate::adapters::http::routes::{
    assets::{get_asset, get_price_stats_signal, get_price_trend_signal, list_assets},
    balances::{resolve_bulk_balances, resolve_single_balance},
    erc20_transfers::search_erc20_transfers,
    health::health,
    predictions::{
        add_deprecation_header, get_world_cup_country_prediction, get_world_cup_winner_prediction,
    },
    resolve::assets_resolve,
    status::status,
};
use crate::state::AppState;

pub fn build_router(state: AppState) -> Router {
    let deprecated_prediction_routes = Router::new()
        .route(
            "/predictions/fifa-world-cup/winner",
            get(get_world_cup_winner_prediction),
        )
        .route(
            "/predictions/fifa-world-cup/{country}",
            get(get_world_cup_country_prediction),
        )
        .layer(middleware::map_response(add_deprecation_header));

    let mut v1_routes = Router::new()
        .route("/status", get(status))
        .route("/balances", post(resolve_single_balance))
        .route("/balances/bulk", post(resolve_bulk_balances))
        .route("/assets", get(list_assets))
        .route("/assets/resolve", get(assets_resolve))
        .route("/assets/{slug}", get(get_asset))
        .route(
            "/assets/{slug}/signal/price-stats",
            get(get_price_stats_signal),
        )
        .route(
            "/assets/{slug}/signal/price-trend",
            get(get_price_trend_signal),
        )
        .merge(deprecated_prediction_routes);

    if state.config.erc20_transfers_enabled {
        v1_routes = v1_routes.route("/erc20-transfers/search", post(search_erc20_transfers));
    }

    Router::new()
        .route("/health", get(health))
        .nest("/v1", v1_routes)
        .fallback(unmatched_route)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn unmatched_route(method: Method, uri: Uri, headers: HeaderMap) -> StatusCode {
    let user_agent = headers
        .get(USER_AGENT)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("-");
    let request_id = headers
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("-");

    if uri.path() == "/v1" || uri.path().starts_with("/v1/") {
        warn!(
            method = %method,
            path = uri.path(),
            user_agent,
            request_id,
            status = StatusCode::NOT_FOUND.as_u16(),
            "unmatched API route"
        );
    } else {
        debug!(
            method = %method,
            path = uri.path(),
            user_agent,
            request_id,
            status = StatusCode::NOT_FOUND.as_u16(),
            "unmatched non-API route"
        );
    }

    StatusCode::NOT_FOUND
}

#[cfg(test)]
mod tests;
