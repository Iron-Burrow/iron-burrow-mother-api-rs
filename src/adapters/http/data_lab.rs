use askama::Template;
use axum::{
    extract::{Extension, Form, Path, State},
    http::{header::CACHE_CONTROL, HeaderMap, HeaderValue, StatusCode},
    middleware,
    response::{IntoResponse, Json, Redirect, Response},
    routing::get,
    Router,
};
use fastnum::{decimal::Context, D512};
use serde::Deserialize;
use serde_json::Value;

use crate::{
    adapters::{
        http::{
            auth::{
                require_catalog_api_key, require_lab_api_key, require_prices_api_key,
                ApiKeyPrincipal,
            },
            web::{self, BrowserPrincipal},
        },
        postgres::{AccountRepository, PortfolioSimulationRun},
    },
    application::assets::service::{
        AssetEnrichmentParams, AssetEnrichmentQuery, AssetsService, AssetsServiceError,
        PriceEnrichmentInclude,
    },
    application::defi_realized_yield::{
        Command as RealizedYieldCommand, Error as RealizedYieldError,
        Service as RealizedYieldService,
    },
    application::portfolio_simulation::{
        Command as PortfolioSimulationCommand, Error as PortfolioSimulationError,
        Service as PortfolioSimulationService,
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
        .route("/lab/portfolio-simulation", get(portfolio_simulation_view))
        .route(
            "/lab/portfolio-simulation/runs",
            axum::routing::post(portfolio_simulation_submit),
        )
        .route(
            "/lab/portfolio-simulation/runs/{run_id}",
            get(portfolio_simulation_result_view),
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

fn portfolio_simulation_service(state: &AppState) -> PortfolioSimulationService {
    PortfolioSimulationService::new(
        state.price_indexer_client.clone(),
        state.defi_protocol_repository.clone(),
        state.bigwig_client.clone(),
    )
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
        .map(|repo| AssetsService::from_database(repo, state.price_indexer_client.clone()))
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
        detail: "Curated studies: asset overview, price signals, Workspace balances, ERC-20 transfers, DeFi protocol realized yield, and portfolio strategy simulation. Select an owned Workspace member before querying on-chain data.".to_string(),
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

async fn portfolio_simulation_view(
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
    let Some(csrf) = page_csrf_token(&headers) else {
        return StatusCode::FORBIDDEN.into_response();
    };
    web::private_html_response(PortfolioSimulationTemplate {
        csrf,
        error: None,
        run: None,
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PortfolioSimulationForm {
    csrf: String,
    initial_capital: String,
    quote_currency: String,
    start_date: String,
    end_date: String,
    strategy_slug: String,
}

async fn portfolio_simulation_submit(
    State(state): State<AppState>,
    headers: HeaderMap,
    Extension(principal): Extension<BrowserPrincipal>,
    Form(form): Form<PortfolioSimulationForm>,
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
    let BrowserPrincipal::Authenticated {
        account_id,
        csrf_hash,
        ..
    } = principal
    else {
        return Redirect::to("/login").into_response();
    };
    if !csrf_valid(&state, &headers, &csrf_hash, &form.csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Some(repository) = state.portfolio_simulation_repository.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let command = PortfolioSimulationCommand {
        initial_capital: form.initial_capital.trim().to_string(),
        quote_currency: form.quote_currency.trim().to_string(),
        start_date: form.start_date.trim().to_string(),
        end_date: form.end_date.trim().to_string(),
        strategy_slug: form.strategy_slug.trim().to_string(),
    };
    let simulation = portfolio_simulation_service(&state);
    let run = match simulation.run(command.clone()).await {
        Ok(run) => run,
        Err(error) => match simulation.failed_run(&command, &error) {
            Some(run) => run,
            None => {
                return portfolio_simulation_error(&headers, portfolio_simulation_failure(error))
            }
        },
    };
    match repository
        .create(
            account_id,
            &run.outcome,
            &run.strategy_slug,
            &run.strategy_version,
            run.engine_version,
            &run.evidence_digest,
            run.input,
            run.evidence,
            run.result,
        )
        .await
    {
        Ok(persisted) => Redirect::to(&format!(
            "/lab/portfolio-simulation/runs/{}",
            persisted.public_id
        ))
        .into_response(),
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}

async fn portfolio_simulation_result_view(
    State(state): State<AppState>,
    headers: HeaderMap,
    Extension(principal): Extension<BrowserPrincipal>,
    Path(run_id): Path<String>,
) -> Response {
    let account_id = match browser_account(
        principal,
        state.account_repository.as_ref(),
        Capability::LabRead,
    )
    .await
    {
        Ok(account_id) => account_id,
        Err(response) => return response,
    };
    let Some(repository) = state.portfolio_simulation_repository.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let Some(csrf) = page_csrf_token(&headers) else {
        return StatusCode::FORBIDDEN.into_response();
    };
    match repository.find_owned(account_id, &run_id).await {
        Ok(Some(run)) => web::private_html_response(PortfolioSimulationTemplate {
            csrf,
            error: None,
            run: Some(PortfolioSimulationView::from_run(run)),
        }),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}

fn portfolio_simulation_failure(error: PortfolioSimulationError) -> &'static str {
    match error {
        PortfolioSimulationError::InvalidInitialCapital => {
            "Initial capital must be a positive decimal."
        }
        PortfolioSimulationError::UnsupportedQuoteCurrency => {
            "Only USD is supported in this first study."
        }
        PortfolioSimulationError::InvalidDateRange => {
            "Use an ordered date range of at most 366 days."
        }
        PortfolioSimulationError::UnsupportedStrategy => "The selected strategy is not supported.",
        PortfolioSimulationError::HistoricalPricesUnavailable => {
            "Historical price evidence is temporarily unavailable."
        }
        PortfolioSimulationError::HistoricalPricesMalformed => {
            "Historical price evidence could not be verified."
        }
        PortfolioSimulationError::AaveEvidenceUnavailable => {
            "Aave historical evidence is temporarily unavailable."
        }
    }
}

fn portfolio_simulation_error(headers: &HeaderMap, error: &str) -> Response {
    web::private_html_response(PortfolioSimulationTemplate {
        csrf: page_csrf_token(headers).unwrap_or_default(),
        error: Some(error.to_string()),
        run: None,
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
    let mut response = Json(serde_json::json!({"ok":true,"studies":["asset_overview","price_signals","workspace_balances","workspace_erc20_transfers","defi_protocol_realized_yield","portfolio_strategy_simulation"]})).into_response();
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

#[derive(Clone)]
struct PortfolioSnapshotView {
    timestamp: String,
    value: String,
    price_status: String,
}

#[derive(Clone)]
struct PortfolioOperationView {
    timestamp: String,
    operation_type: String,
    from_position: String,
    to_position: String,
    amount: String,
    fees: String,
    reason: String,
}

#[derive(Clone)]
struct PortfolioSimulationView {
    public_id: String,
    created_at: String,
    outcome: String,
    strategy_slug: String,
    strategy_version: String,
    engine_version: String,
    request_schema_version: i32,
    evidence_digest: String,
    period: String,
    price_evidence_points: usize,
    aave_evidence_points: usize,
    initial_value: String,
    final_value: String,
    absolute_return: String,
    percentage_return: String,
    annualized_return: String,
    maximum_drawdown: String,
    limitations: Vec<String>,
    snapshots: Vec<PortfolioSnapshotView>,
    operations: Vec<PortfolioOperationView>,
    chart_points: String,
}

impl PortfolioSimulationView {
    fn from_run(run: PortfolioSimulationRun) -> Self {
        let result = &run.result;
        let metrics = result.get("metrics").unwrap_or(&Value::Null);
        let snapshots = result
            .get("snapshots")
            .and_then(Value::as_array)
            .map(|snapshots| {
                snapshots
                    .iter()
                    .map(|snapshot| PortfolioSnapshotView {
                        timestamp: value_string(snapshot, "timestamp"),
                        value: value_string(snapshot, "value"),
                        price_status: value_string(snapshot, "price_status"),
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let operations = result
            .get("operations")
            .and_then(Value::as_array)
            .map(|operations| {
                operations
                    .iter()
                    .map(|operation| PortfolioOperationView {
                        timestamp: value_string(operation, "timestamp"),
                        operation_type: value_string(operation, "operation_type"),
                        from_position: value_string(operation, "from_position"),
                        to_position: value_string(operation, "to_position"),
                        amount: value_string(operation, "amount"),
                        fees: value_string(operation, "fees"),
                        reason: value_string(operation, "reason"),
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let limitations = result
            .get("limitations")
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        let period = format!(
            "{} through {}",
            value_string(&run.input, "start_date"),
            value_string(&run.input, "end_date")
        );
        let price_evidence_points = run
            .evidence
            .pointer("/price_series/points")
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
        let aave_evidence_points = run
            .evidence
            .pointer("/aave_income_indexes")
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
        Self {
            public_id: run.public_id,
            created_at: run.created_at,
            outcome: run.outcome,
            strategy_slug: run.strategy_slug,
            strategy_version: run.strategy_version,
            engine_version: run.engine_version,
            request_schema_version: run.request_schema_version,
            evidence_digest: run.evidence_digest,
            period,
            price_evidence_points,
            aave_evidence_points,
            initial_value: value_string(metrics, "initial_value"),
            final_value: value_string(metrics, "final_value"),
            absolute_return: value_string(metrics, "absolute_return"),
            percentage_return: value_string(metrics, "percentage_return"),
            annualized_return: value_string(metrics, "annualized_return"),
            maximum_drawdown: value_string(metrics, "maximum_drawdown"),
            limitations,
            chart_points: chart_points(&snapshots),
            snapshots,
            operations,
        }
    }
}

fn value_string(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("unavailable")
        .to_string()
}

fn chart_points(snapshots: &[PortfolioSnapshotView]) -> String {
    let values = snapshots
        .iter()
        .filter_map(|snapshot| D512::from_str(&snapshot.value, Context::default()).ok())
        .collect::<Vec<_>>();
    if values.len() < 2 {
        return String::new();
    }
    let mut low = values[0].clone();
    let mut high = values[0].clone();
    for value in &values {
        if value < &low {
            low = value.clone();
        }
        if value > &high {
            high = value.clone();
        }
    }
    let zero = D512::from_str("0", Context::default()).expect("zero is valid");
    let one = D512::from_str("1", Context::default()).expect("one is valid");
    let hundred = D512::from_str("100", Context::default()).expect("constant is valid");
    let span = high.clone() - low.clone();
    let span = if span > zero { span } else { one };
    let divisor = D512::from_str(&(values.len() - 1).to_string(), Context::default())
        .expect("small length is valid");
    values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let index = D512::from_str(&index.to_string(), Context::default())
                .expect("small index is valid");
            let x = index * hundred.clone() / divisor.clone();
            let y =
                hundred.clone() - (value.clone() - low.clone()) / span.clone() * hundred.clone();
            format!("{},{}", chart_decimal(&x), chart_decimal(&y))
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn chart_decimal(value: &D512) -> String {
    let value = value.to_string();
    if value.contains('.') {
        value
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    } else {
        value
    }
}

#[derive(Template)]
#[template(path = "web/portfolio_simulation.html")]
struct PortfolioSimulationTemplate {
    csrf: String,
    error: Option<String>,
    run: Option<PortfolioSimulationView>,
}
