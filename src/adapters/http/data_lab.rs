use askama::Template;
use axum::{
    extract::{Extension, Form, Path, State},
    http::{header::CACHE_CONTROL, HeaderMap, HeaderValue, StatusCode},
    middleware,
    response::{IntoResponse, Json, Redirect, Response},
    routing::get,
    Router,
};
use serde::Deserialize;

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
        AssetEnrichmentParams, AssetEnrichmentQuery, AssetsService, AssetsServiceError,
        PriceEnrichmentInclude,
    },
    application::defi_realized_yield::{
        Command as RealizedYieldCommand, Error as RealizedYieldError,
        Service as RealizedYieldService,
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
            "/lab/defi-protocols/realized-yield",
            get(realized_yield_view).post(realized_yield_submit),
        )
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

fn realized_yield_service(state: &AppState) -> Option<RealizedYieldService> {
    Some(RealizedYieldService::new(
        state.defi_protocol_repository.clone()?,
        state.bigwig_client.clone()?,
        state.config.aave_v3_min_block_confirmations,
    ))
}

fn csrf_valid(
    state: &AppState,
    headers: &HeaderMap,
    expected_hash: &[u8],
    submitted: &str,
) -> bool {
    web::same_origin(headers, &state.config.public_web_base_url)
        && web::cookie_value(headers, "__Host-ib_csrf") == Some(submitted)
        && web::hash(submitted).as_slice() == expected_hash
}

fn page_csrf_token(headers: &HeaderMap) -> Option<String> {
    web::cookie_value(headers, "__Host-ib_csrf").map(str::to_string)
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
    web::private_html_response(DataLabTemplate {
        title: "Data Lab",
        detail: "Curated studies: asset overview, price signals, Workspace balances, ERC-20 transfers, and DeFi protocol realized yield. Select an owned Workspace member before querying on-chain data.".to_string(),
    })
}

async fn realized_yield_view(
    State(state): State<AppState>,
    headers: HeaderMap,
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
    let csrf = match page_csrf_token(&headers) {
        Some(csrf) => csrf,
        None => return StatusCode::FORBIDDEN.into_response(),
    };
    web::private_html_response(RealizedYieldTemplate {
        csrf,
        error: None,
        result: None,
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RealizedYieldForm {
    csrf: String,
    protocol_slug: String,
    asset_slug: String,
    from_block: String,
    to_block: String,
    include_annualized_apy_estimate: Option<String>,
}

async fn realized_yield_submit(
    State(state): State<AppState>,
    headers: HeaderMap,
    Extension(principal): Extension<BrowserPrincipal>,
    Form(form): Form<RealizedYieldForm>,
) -> Response {
    if let Err(response) = browser_account(
        principal.clone(),
        state.account_repository.as_ref(),
        Capability::LabRead,
    )
    .await
    {
        return response;
    }
    let BrowserPrincipal::Authenticated { csrf_hash, .. } = principal else {
        return Redirect::to("/login").into_response();
    };
    if !csrf_valid(&state, &headers, &csrf_hash, &form.csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let (from_block, to_block) =
        match (form.from_block.parse::<u64>(), form.to_block.parse::<u64>()) {
            (Ok(from_block), Ok(to_block)) => (from_block, to_block),
            _ => {
                return realized_yield_error(
                    &headers,
                    "Block numbers must be positive integers.",
                    StatusCode::BAD_REQUEST,
                )
            }
        };
    let Some(service) = realized_yield_service(&state) else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match service
        .resolve(RealizedYieldCommand {
            protocol_slug: form.protocol_slug.trim().to_ascii_lowercase(),
            asset_slug: form.asset_slug.trim().to_ascii_lowercase(),
            from_block,
            to_block,
            include_annualized_apy_estimate: form.include_annualized_apy_estimate.is_some(),
        })
        .await
    {
        Ok(value) => web::private_html_response(RealizedYieldTemplate {
            csrf: form.csrf,
            error: None,
            result: Some(RealizedYieldView {
                protocol_slug: value.protocol.slug,
                network_slug: value.protocol.network_slug,
                asset_symbol: value.resolved.asset_symbol,
                underlying_asset_address: value.resolved.underlying_asset_address,
                from_index: value.resolved.from_index,
                to_index: value.resolved.to_index,
                realized_yield: value.resolved.realized_yield,
                annualized_apy_estimate: value
                    .resolved
                    .annualized_apy_estimate
                    .unwrap_or_else(|| "unavailable".to_string()),
                from_timestamp: value
                    .resolved
                    .from_timestamp
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "unavailable".to_string()),
                to_timestamp: value
                    .resolved
                    .to_timestamp
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "unavailable".to_string()),
                warnings: value.resolved.warnings.join(", "),
            }),
        }),
        Err(error) => {
            let (message, status) = realized_yield_failure(error);
            realized_yield_error(&headers, message, status)
        }
    }
}

fn realized_yield_failure(error: RealizedYieldError) -> (&'static str, StatusCode) {
    match error {
        RealizedYieldError::ProtocolUnavailable => (
            "The selected protocol is not available.",
            StatusCode::NOT_FOUND,
        ),
        RealizedYieldError::OperationUnsupported => (
            "The selected protocol does not support realized yield.",
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        RealizedYieldError::Aave(crate::adapters::aave_v3::Error::UnsupportedAsset) => (
            "The selected asset is not supported by this protocol.",
            StatusCode::BAD_REQUEST,
        ),
        RealizedYieldError::Aave(crate::adapters::aave_v3::Error::InvalidBlockRange) => {
            ("Block range is invalid.", StatusCode::BAD_REQUEST)
        }
        RealizedYieldError::Aave(crate::adapters::aave_v3::Error::BlockNotFinal) => (
            "The end block has not reached finality.",
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        _ => (
            "Realized yield is temporarily unavailable.",
            StatusCode::BAD_GATEWAY,
        ),
    }
}

fn realized_yield_error(headers: &HeaderMap, error: &str, status: StatusCode) -> Response {
    let csrf = web::cookie_value(headers, "__Host-ib_csrf")
        .unwrap_or_default()
        .to_string();
    let mut response = web::private_html_response(RealizedYieldTemplate {
        csrf,
        error: Some(error.to_string()),
        result: None,
    });
    *response.status_mut() = status;
    response
}
async fn lab_json(Extension(_principal): Extension<ApiKeyPrincipal>) -> Response {
    let mut response = Json(serde_json::json!({"ok":true,"studies":["asset_overview","price_signals","workspace_balances","workspace_erc20_transfers","defi_protocol_realized_yield"]})).into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    response
}
fn private_json<T: serde::Serialize>(result: Result<T, AssetsServiceError>) -> Response {
    match result {
        Ok(value) => {
            let mut response = Json(value).into_response();
            response
                .headers_mut()
                .insert(CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
            response
        }
        Err(AssetsServiceError::AssetNotFound) => StatusCode::NOT_FOUND.into_response(),
        Err(AssetsServiceError::InvalidLimit) => StatusCode::BAD_REQUEST.into_response(),
        Err(AssetsServiceError::Repository(_)) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}
#[derive(Template)]
#[template(path = "web/data_lab.html")]
struct DataLabTemplate {
    title: &'static str,
    detail: String,
}

#[derive(Clone)]
struct RealizedYieldView {
    protocol_slug: String,
    network_slug: String,
    asset_symbol: String,
    underlying_asset_address: String,
    from_index: String,
    to_index: String,
    realized_yield: String,
    annualized_apy_estimate: String,
    from_timestamp: String,
    to_timestamp: String,
    warnings: String,
}

#[derive(Template)]
#[template(path = "web/realized_yield.html")]
struct RealizedYieldTemplate {
    csrf: String,
    error: Option<String>,
    result: Option<RealizedYieldView>,
}
