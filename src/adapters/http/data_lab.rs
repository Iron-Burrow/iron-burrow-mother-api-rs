use askama::Template;
use axum::{
    extract::{Extension, Path, State},
    http::{header::CACHE_CONTROL, HeaderValue, StatusCode},
    middleware,
    response::{IntoResponse, Json, Redirect, Response},
    routing::get,
    Router,
};

use crate::{
    adapters::{
        http::{
            auth::{
                require_catalog_api_key, require_lab_api_key, require_prices_api_key,
                ApiKeyPrincipal,
            },
            web::{self, BrowserPrincipal},
        },
        postgres::AccountRepository,
    },
    application::assets::service::{
        AssetEnrichmentParams, AssetEnrichmentQuery, AssetsService, PriceEnrichmentInclude,
    },
    domain::capabilities::Capability,
    state::AppState,
};

pub(crate) fn routes(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/catalog/assets", get(assets_view))
        .route("/catalog/assets/{slug}", get(asset_view))
        .route(
            "/catalog/assets.json",
            get(assets_json).route_layer(middleware::from_fn_with_state(
                state.clone(),
                require_catalog_api_key,
            )),
        )
        .route(
            "/catalog/assets/{slug}/export.json",
            get(asset_json).route_layer(middleware::from_fn_with_state(
                state.clone(),
                require_catalog_api_key,
            )),
        )
        .route("/prices/{slug}", get(price_view))
        .route(
            "/prices/{slug}/export.json",
            get(price_json).route_layer(middleware::from_fn_with_state(
                state.clone(),
                require_prices_api_key,
            )),
        )
        .route("/lab", get(lab_view))
        .route(
            "/lab.json",
            get(lab_json).route_layer(middleware::from_fn_with_state(
                state.clone(),
                require_lab_api_key,
            )),
        )
        .route_layer(middleware::from_fn_with_state(
            state,
            web::attach_browser_context,
        ))
}

async fn browser_account(
    principal: BrowserPrincipal,
    accounts: Option<&AccountRepository>,
    capability: Capability,
) -> Result<uuid::Uuid, Response> {
    let BrowserPrincipal::Authenticated { account_id, .. } = principal else {
        return Err(Redirect::to("/login").into_response());
    };
    let Some(accounts) = accounts else {
        return Err(StatusCode::SERVICE_UNAVAILABLE.into_response());
    };
    match accounts
        .has_active_capability(account_id, capability, "*")
        .await
    {
        Ok(true) => Ok(account_id),
        Ok(false) => Err(StatusCode::FORBIDDEN.into_response()),
        Err(_) => Err(StatusCode::SERVICE_UNAVAILABLE.into_response()),
    }
}
fn assets(state: &AppState) -> Option<AssetsService> {
    state
        .asset_repository
        .clone()
        .map(|repo| AssetsService::new(repo, state.price_indexer_client.clone()))
}
async fn assets_view(
    State(state): State<AppState>,
    Extension(principal): Extension<BrowserPrincipal>,
) -> Response {
    if let Err(response) = browser_account(
        principal,
        state.account_repository.as_ref(),
        Capability::CatalogRead,
    )
    .await
    {
        return response;
    }
    let Some(service) = assets(&state) else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match service.list_assets(Some("100")).await {
        Ok(value) => web::private_html_response(DataLabTemplate {
            title: "Data Lab assets",
            detail: format!("{value:#?}"),
        }),
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}
async fn asset_view(
    State(state): State<AppState>,
    Extension(principal): Extension<BrowserPrincipal>,
    Path(slug): Path<String>,
) -> Response {
    if let Err(response) = browser_account(
        principal,
        state.account_repository.as_ref(),
        Capability::CatalogRead,
    )
    .await
    {
        return response;
    }
    let Some(service) = assets(&state) else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match service.get_asset(&slug, "USD", None).await {
        Ok(value) => web::private_html_response(DataLabTemplate {
            title: "Data Lab asset",
            detail: format!("{value:#?}"),
        }),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}
async fn assets_json(
    State(state): State<AppState>,
    Extension(_principal): Extension<ApiKeyPrincipal>,
) -> Response {
    let Some(service) = assets(&state) else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    private_json(service.list_assets(Some("100")).await)
}
async fn asset_json(
    State(state): State<AppState>,
    Extension(_principal): Extension<ApiKeyPrincipal>,
    Path(slug): Path<String>,
) -> Response {
    let Some(service) = assets(&state) else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    private_json(service.get_asset(&slug, "USD", None).await)
}
fn enrichment(slug: String) -> AssetEnrichmentQuery {
    AssetEnrichmentQuery {
        include: vec![
            PriceEnrichmentInclude::Stats,
            PriceEnrichmentInclude::Trend,
            PriceEnrichmentInclude::Series,
        ],
        params: Some(AssetEnrichmentParams {
            slug,
            quote_currency: "USD".to_string(),
            window: "24h".to_string(),
            granularity: Some("1h".to_string()),
        }),
    }
}
async fn price_view(
    State(state): State<AppState>,
    Extension(principal): Extension<BrowserPrincipal>,
    Path(slug): Path<String>,
) -> Response {
    if let Err(response) = browser_account(
        principal,
        state.account_repository.as_ref(),
        Capability::PricesRead,
    )
    .await
    {
        return response;
    }
    let Some(service) = assets(&state) else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match service
        .get_asset(&slug, "USD", Some(enrichment(slug.clone())))
        .await
    {
        Ok(value) => web::private_html_response(DataLabTemplate {
            title: "Data Lab price",
            detail: format!("{value:#?}"),
        }),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}
async fn price_json(
    State(state): State<AppState>,
    Extension(_principal): Extension<ApiKeyPrincipal>,
    Path(slug): Path<String>,
) -> Response {
    let Some(service) = assets(&state) else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    private_json(
        service
            .get_asset(&slug, "USD", Some(enrichment(slug.clone())))
            .await,
    )
}
async fn lab_view(
    State(state): State<AppState>,
    Extension(principal): Extension<BrowserPrincipal>,
) -> Response {
    if let Err(response) = browser_account(
        principal,
        state.account_repository.as_ref(),
        Capability::LabRead,
    )
    .await
    {
        return response;
    }
    web::private_html_response(DataLabTemplate { title: "Data Lab", detail: "Curated studies: asset overview, price signals, Workspace balances, and ERC-20 transfers. Select an owned Workspace member before querying on-chain data.".to_string() })
}
async fn lab_json(Extension(_principal): Extension<ApiKeyPrincipal>) -> Response {
    let mut response = Json(serde_json::json!({"ok":true,"studies":["asset_overview","price_signals","workspace_balances","workspace_erc20_transfers"]})).into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    response
}
fn private_json<T: serde::Serialize, E>(result: Result<T, E>) -> Response {
    match result {
        Ok(value) => {
            let mut response = Json(value).into_response();
            response
                .headers_mut()
                .insert(CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
            response
        }
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}
#[derive(Template)]
#[template(path = "web/data_lab.html")]
struct DataLabTemplate {
    title: &'static str,
    detail: String,
}
