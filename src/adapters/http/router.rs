use axum::{
    extract::State,
    http::{header::USER_AGENT, HeaderMap, Method, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use tower_http::trace::TraceLayer;
use tracing::{debug, warn};

use crate::{
    adapters::http::{
        error::ApiError,
        routes::{
            assets::{get_asset, get_price_stats_signal, get_price_trend_signal, list_assets},
            balances::{resolve_bulk_balances, resolve_single_balance},
            erc20_transfers::search_erc20_transfers,
            health::health,
            resolve::assets_resolve,
            status::status,
        },
    },
    config::PublicApiSurface,
    state::AppState,
};

pub fn build_router(state: AppState) -> Router {
    let mut v1_routes = Router::new()
        .route("/balances", post(resolve_single_balance))
        .route("/balances/bulk", post(resolve_bulk_balances));

    if state.config.erc20_transfers_enabled {
        v1_routes = v1_routes.route("/erc20-transfers/search", post(search_erc20_transfers));
    }

    if state.config.public_api_surface == PublicApiSurface::Alpha {
        v1_routes = v1_routes.merge(alpha_v1_routes());
    }

    Router::new()
        .route("/health", get(health))
        .nest("/v1", v1_routes)
        .fallback(unmatched_route)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

fn alpha_v1_routes() -> Router<AppState> {
    Router::new()
        .route("/status", get(status))
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
}

async fn unmatched_route(
    State(state): State<AppState>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
) -> Response {
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
            status = unmatched_status(&state, &method, uri.path()).as_u16(),
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

    if state.config.public_api_surface == PublicApiSurface::Beta
        && is_known_disabled_beta_route(&method, uri.path())
    {
        return ApiError::endpoint_disabled().into_response();
    }

    StatusCode::NOT_FOUND.into_response()
}

fn unmatched_status(state: &AppState, method: &Method, path: &str) -> StatusCode {
    if state.config.public_api_surface == PublicApiSurface::Beta
        && is_known_disabled_beta_route(method, path)
    {
        StatusCode::FORBIDDEN
    } else {
        StatusCode::NOT_FOUND
    }
}

fn is_known_disabled_beta_route(method: &Method, path: &str) -> bool {
    if !matches!(*method, Method::GET | Method::HEAD) {
        return false;
    }

    matches!(
        path,
        "/v1/status" | "/v1/assets" | "/v1/assets/resolve" | "/v1/search-engine"
    ) || is_disabled_beta_asset_route(path)
}

fn is_disabled_beta_asset_route(path: &str) -> bool {
    let Some(rest) = path.strip_prefix("/v1/assets/") else {
        return false;
    };

    let mut segments = rest.split('/');
    let Some(slug) = segments.next() else {
        return false;
    };

    if slug.is_empty() {
        return false;
    }

    match (
        segments.next(),
        segments.next(),
        segments.next(),
        segments.next(),
    ) {
        (None, None, None, None) => slug != "resolve",
        (Some("signal"), Some("price-stats" | "price-trend"), None, None) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests;
