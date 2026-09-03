use std::{collections::BTreeSet, sync::Arc};

use askama::Template;
use axum::{
    extract::{Extension, Form, Path, Query, State},
    http::{header::CACHE_CONTROL, HeaderMap, HeaderValue, StatusCode},
    middleware,
    response::{IntoResponse, Json, Redirect, Response},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::warn;

use crate::{
    adapters::{
        http::{
            auth::{require_treasury_api_key, require_workspace_activity_api_key, ApiKeyPrincipal},
            dto::onchain_time::as_of::AsOfRequest,
            error::ApiError,
            state::HttpState,
            web::{self, BrowserPrincipal},
        },
        postgres::workspaces::{
            Workspace, WorkspaceActivityEvent, WorkspaceMemberAddress, WorkspaceTreasurySnapshot,
        },
    },
    application::{
        balances::result::{BalanceItemOutcome, BalanceQuoteOutcome, GetBalancesResult},
        erc20_transfers::service::{
            build_search_plan, execute_search_plan, Erc20TransferSearchInput,
        },
        workspaces::{
            portfolio::{
                CurrentWorkspacePortfolio, WorkspaceBalanceResolutionPlanner,
                WorkspacePortfolioResolution,
            },
            WorkspaceService, WorkspaceServiceError,
        },
    },
    domain::{
        accounts::OnchainAccount,
        assets::token_selector::TokenSelector,
        capabilities::Capability,
        onchain_time::{
            as_of::AsOf,
            onchain_window::{LookbackWindow, OnchainWindow},
        },
        transfers::transfer_direction::TransferDirection,
    },
};

pub(crate) fn routes(state: HttpState) -> Router<HttpState> {
    Router::new()
        .route("/workspaces", get(list_workspaces).post(create_workspace))
        .route("/workspaces/{workspace_id}", get(workspace_detail))
        .route("/workspaces/{workspace_id}/activity", get(activity_view))
        .route("/workspaces/{workspace_id}/treasury", get(treasury_view))
        .route(
            "/workspaces/{workspace_id}/treasury/snapshots",
            post(capture_treasury_snapshot),
        )
        .route(
            "/workspaces/{workspace_id}/treasury.json",
            get(treasury_json).route_layer(middleware::from_fn_with_state(
                state.clone(),
                require_treasury_api_key,
            )),
        )
        .route(
            "/workspaces/{workspace_id}/activity.json",
            get(activity_json).route_layer(middleware::from_fn_with_state(
                state.clone(),
                require_workspace_activity_api_key,
            )),
        )
        .route("/workspaces/{workspace_id}/rename", post(rename_workspace))
        .route(
            "/workspaces/{workspace_id}/archive",
            post(archive_workspace),
        )
        .route(
            "/workspaces/{workspace_id}/restore",
            post(restore_workspace),
        )
        .route("/workspaces/{workspace_id}/addresses", post(add_address))
        .route(
            "/workspaces/{workspace_id}/addresses/{member_id}",
            get(member_detail),
        )
        .route(
            "/workspaces/{workspace_id}/addresses/{member_id}/labels",
            post(add_label),
        )
        .route(
            "/workspaces/{workspace_id}/addresses/{member_id}/labels/remove",
            post(remove_label),
        )
        .route(
            "/workspaces/{workspace_id}/addresses/{member_id}/balances",
            get(balance_view),
        )
        .route(
            "/workspaces/{workspace_id}/addresses/{member_id}/transfers",
            get(transfer_view),
        )
        .route_layer(axum::middleware::from_fn_with_state(
            state,
            web::attach_browser_context,
        ))
}

fn service(state: &HttpState) -> Option<WorkspaceService> {
    state
        .workspace_repository
        .clone()
        .map(WorkspaceService::new)
}
fn authenticated(principal: BrowserPrincipal) -> Option<(uuid::Uuid, Vec<u8>)> {
    match principal {
        BrowserPrincipal::Authenticated {
            account_id,
            csrf_hash,
            ..
        } => Some((account_id, csrf_hash)),
        BrowserPrincipal::Anonymous => None,
    }
}
fn csrf_valid(
    state: &HttpState,
    headers: &HeaderMap,
    expected_hash: &[u8],
    submitted: &str,
) -> bool {
    web::same_origin(headers, &state.config.public_web_base_url)
        && web::cookie_value(headers, "__Host-ib_csrf") == Some(submitted)
        && web::hash(submitted).as_slice() == expected_hash
}
fn csrf_token(headers: &HeaderMap) -> Option<String> {
    web::cookie_value(headers, "__Host-ib_csrf").map(str::to_string)
}
#[allow(clippy::result_large_err)]
fn page_csrf_token(headers: &HeaderMap) -> Result<String, Response> {
    csrf_token(headers).ok_or_else(|| StatusCode::FORBIDDEN.into_response())
}
fn unavailable() -> Response {
    StatusCode::SERVICE_UNAVAILABLE.into_response()
}
fn not_found() -> Response {
    StatusCode::NOT_FOUND.into_response()
}
fn invalid() -> Response {
    StatusCode::BAD_REQUEST.into_response()
}
fn workspace_error(error: WorkspaceServiceError) -> Response {
    match error {
        WorkspaceServiceError::Input(_) => invalid(),
        WorkspaceServiceError::Repository(_) => unavailable(),
    }
}

#[allow(dead_code)] // PR 5 exposes this private browser-flow seam through its portfolio route.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WorkspacePortfolioAccessError {
    NotFound,
    Forbidden,
    Unavailable,
}

/// Resolves an owned Workspace portfolio only after every network that the
/// deterministic planner would query is authorized. PR 5 will expose this
/// helper through the private portfolio page; keeping it route-free here
/// preserves the PR 4 boundary.
#[allow(dead_code)] // PR 5 invokes this after browser-session authentication.
async fn resolve_workspace_portfolio<R>(
    service: &WorkspaceService,
    accounts: &crate::adapters::postgres::AccountRepository,
    registry: Arc<crate::domain::canonical_registry::CanonicalRegistry>,
    resolver: &R,
    account_id: uuid::Uuid,
    workspace_id: &str,
) -> Result<CurrentWorkspacePortfolio, WorkspacePortfolioAccessError>
where
    R: WorkspacePortfolioResolution,
{
    let workspace = service
        .find(account_id, workspace_id)
        .await
        .map_err(|_| WorkspacePortfolioAccessError::Unavailable)?
        .ok_or(WorkspacePortfolioAccessError::NotFound)?;
    let members = service
        .members(workspace.id)
        .await
        .map_err(|_| WorkspacePortfolioAccessError::Unavailable)?;
    let required_networks = portfolio_resolution_networks(registry, &members)
        .map_err(|_| WorkspacePortfolioAccessError::Unavailable)?;

    for network_slug in required_networks {
        match allowed(
            accounts,
            account_id,
            Capability::BalancesRead,
            &network_slug,
        )
        .await
        {
            Ok(true) => {}
            Ok(false) => return Err(WorkspacePortfolioAccessError::Forbidden),
            Err(()) => return Err(WorkspacePortfolioAccessError::Unavailable),
        }
    }

    resolver
        .resolve_workspace_portfolio(workspace, members)
        .await
        .map_err(|_| WorkspacePortfolioAccessError::Unavailable)
}

#[allow(dead_code)] // Called by the deferred PR 5 portfolio route through its coordinator.
fn portfolio_resolution_networks(
    registry: Arc<crate::domain::canonical_registry::CanonicalRegistry>,
    members: &[WorkspaceMemberAddress],
) -> Result<Vec<String>, crate::application::balances::error::GetBalancesCommandError> {
    let planner = WorkspaceBalanceResolutionPlanner::new(registry);
    let commands = planner.plan(members)?;
    Ok(commands
        .iter()
        .flat_map(|command| command.accounts().iter())
        .map(|account| account.network_slug.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect())
}

async fn list_workspaces(
    State(state): State<HttpState>,
    headers: HeaderMap,
    Extension(principal): Extension<BrowserPrincipal>,
) -> Response {
    let Some((account_id, _)) = authenticated(principal) else {
        return Redirect::to("/login").into_response();
    };
    let Some(service) = service(&state) else {
        return unavailable();
    };
    let csrf = match page_csrf_token(&headers) {
        Ok(csrf) => csrf,
        Err(response) => return response,
    };
    match service.list(account_id).await {
        Ok(workspaces) => web::private_html_response(WorkspaceListTemplate { workspaces, csrf }),
        Err(_) => unavailable(),
    }
}

#[derive(Deserialize)]
struct WorkspaceForm {
    name: String,
    description: Option<String>,
    csrf: String,
}
async fn create_workspace(
    State(state): State<HttpState>,
    headers: HeaderMap,
    Extension(principal): Extension<BrowserPrincipal>,
    Form(form): Form<WorkspaceForm>,
) -> Response {
    let Some((account_id, csrf_hash)) = authenticated(principal) else {
        return Redirect::to("/login").into_response();
    };
    if !csrf_valid(&state, &headers, &csrf_hash, &form.csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Some(service) = service(&state) else {
        return unavailable();
    };
    match service
        .create(account_id, &form.name, form.description.as_deref())
        .await
    {
        Ok(workspace) => {
            Redirect::to(&format!("/workspaces/{}", workspace.public_id)).into_response()
        }
        Err(error) => workspace_error(error),
    }
}

async fn workspace_detail(
    State(state): State<HttpState>,
    headers: HeaderMap,
    Extension(principal): Extension<BrowserPrincipal>,
    Path(workspace_id): Path<String>,
) -> Response {
    let Some((account_id, _)) = authenticated(principal) else {
        return Redirect::to("/login").into_response();
    };
    let Some(service) = service(&state) else {
        return unavailable();
    };
    let csrf = match page_csrf_token(&headers) {
        Ok(csrf) => csrf,
        Err(response) => return response,
    };
    let workspace = match service.find(account_id, &workspace_id).await {
        Ok(Some(value)) => value,
        Ok(None) => return not_found(),
        Err(_) => return unavailable(),
    };
    match service.members(workspace.id).await {
        Ok(members) => web::private_html_response(WorkspaceDetailTemplate {
            workspace,
            members,
            csrf,
        }),
        Err(_) => unavailable(),
    }
}

#[derive(Deserialize, Default)]
struct ActivityQuery {
    limit: Option<u16>,
    before: Option<String>,
}
#[allow(clippy::result_large_err)]
fn activity_page(query: ActivityQuery) -> Result<(i64, Option<String>), Response> {
    let limit = query.limit.unwrap_or(50);
    if limit == 0
        || limit > 100
        || query
            .before
            .as_deref()
            .is_some_and(|value| !is_event_id(value))
    {
        return Err(invalid());
    }
    Ok((i64::from(limit), query.before))
}
fn is_event_id(value: &str) -> bool {
    value.len() == 36
        && value.starts_with("wae_")
        && value[4..]
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}
async fn activity_events(
    service: &WorkspaceService,
    workspace: &Workspace,
    limit: i64,
    before: Option<&str>,
) -> Result<(Vec<WorkspaceActivityEvent>, Option<String>), Response> {
    let mut events = service
        .activity(workspace.id, before, limit + 1)
        .await
        .map_err(|_| unavailable())?;
    let next_before = if events.len() > limit as usize {
        events.pop();
        events.last().map(|event| event.public_id.clone())
    } else {
        None
    };
    Ok((events, next_before))
}
async fn activity_view(
    State(state): State<HttpState>,
    Extension(principal): Extension<BrowserPrincipal>,
    Path(workspace_id): Path<String>,
    Query(query): Query<ActivityQuery>,
) -> Response {
    let Some((account_id, _)) = authenticated(principal) else {
        return Redirect::to("/login").into_response();
    };
    let Some(service) = service(&state) else {
        return unavailable();
    };
    let (limit, before) = match activity_page(query) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let workspace = match service.find(account_id, &workspace_id).await {
        Ok(Some(value)) => value,
        Ok(None) => return not_found(),
        Err(_) => return unavailable(),
    };
    let (events, next_before) =
        match activity_events(&service, &workspace, limit, before.as_deref()).await {
            Ok(value) => value,
            Err(response) => return response,
        };
    web::private_html_response(ActivityTemplate {
        workspace,
        events,
        next_before,
    })
}
#[derive(Serialize)]
struct ActivityWorkspace {
    id: String,
    name: String,
    status: String,
}
#[derive(Serialize)]
struct ActivityJsonResponse {
    ok: bool,
    workspace: ActivityWorkspace,
    events: Vec<WorkspaceActivityEvent>,
    next_before: Option<String>,
}
async fn activity_json(
    State(state): State<HttpState>,
    Extension(principal): Extension<ApiKeyPrincipal>,
    Path(workspace_id): Path<String>,
    Query(query): Query<ActivityQuery>,
) -> Response {
    let Some(account_id) = principal.ib_account_id else {
        return ApiError::capability_not_granted().into_response();
    };
    if !matches!(principal.key_kind.as_str(), "account" | "agent") {
        return ApiError::capability_not_granted().into_response();
    }
    let Some(service) = service(&state) else {
        return ApiError::database_unavailable_for_auth().into_response();
    };
    let (limit, before) = match activity_page(query) {
        Ok(value) => value,
        Err(_) => return ApiError::invalid_request().into_response(),
    };
    let workspace = match service.find(account_id, &workspace_id).await {
        Ok(Some(value)) => value,
        Ok(None) => return not_found(),
        Err(_) => return ApiError::database_unavailable_for_auth().into_response(),
    };
    let (events, next_before) =
        match activity_events(&service, &workspace, limit, before.as_deref()).await {
            Ok(value) => value,
            Err(_) => return ApiError::database_unavailable_for_auth().into_response(),
        };
    let workspace = ActivityWorkspace {
        id: workspace.public_id,
        name: workspace.name,
        status: workspace.status,
    };
    let mut response = Json(ActivityJsonResponse {
        ok: true,
        workspace,
        events,
        next_before,
    })
    .into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    response
}

#[derive(Deserialize)]
struct TreasuryForm {
    csrf: String,
    asset_slugs: String,
    quote_currency: Option<String>,
    as_of_kind: Option<String>,
    as_of_timestamp: Option<String>,
    as_of_block_number: Option<String>,
}

async fn treasury_view(
    State(state): State<HttpState>,
    Extension(principal): Extension<BrowserPrincipal>,
    Path(workspace_id): Path<String>,
) -> Response {
    let Some((account_id, _)) = authenticated(principal) else {
        return Redirect::to("/login").into_response();
    };
    let (Some(service), Some(accounts)) = (service(&state), state.account_repository.as_ref())
    else {
        return unavailable();
    };
    let workspace = match service.find(account_id, &workspace_id).await {
        Ok(Some(value)) => value,
        Ok(None) => return not_found(),
        Err(_) => return unavailable(),
    };
    match allowed(accounts, account_id, Capability::TreasuryRead, "*").await {
        Ok(true) => {}
        Ok(false) => return StatusCode::FORBIDDEN.into_response(),
        Err(()) => return unavailable(),
    }
    let snapshots = match service.treasury_snapshots(workspace.id).await {
        Ok(value) => value,
        Err(_) => return unavailable(),
    };
    web::private_html_response(TreasuryTemplate {
        workspace,
        snapshots,
    })
}

async fn capture_treasury_snapshot(
    State(state): State<HttpState>,
    headers: HeaderMap,
    Extension(principal): Extension<BrowserPrincipal>,
    Path(workspace_id): Path<String>,
    Form(form): Form<TreasuryForm>,
) -> Response {
    let Some((account_id, csrf_hash)) = authenticated(principal) else {
        return Redirect::to("/login").into_response();
    };
    if !csrf_valid(&state, &headers, &csrf_hash, &form.csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let (Some(service), Some(accounts)) = (service(&state), state.account_repository.as_ref())
    else {
        return unavailable();
    };
    let workspace = match service.find(account_id, &workspace_id).await {
        Ok(Some(value)) => value,
        Ok(None) => return not_found(),
        Err(_) => return unavailable(),
    };
    match allowed(accounts, account_id, Capability::TreasurySnapshotWrite, "*").await {
        Ok(true) => {}
        Ok(false) => return StatusCode::FORBIDDEN.into_response(),
        Err(()) => return unavailable(),
    }
    let asset_slugs = split_values(Some(form.asset_slugs));
    if asset_slugs.is_empty() || asset_slugs.len() > 10 {
        return invalid();
    }
    let members = match service.members(workspace.id).await {
        Ok(value) if !value.is_empty() => value,
        Ok(_) => return invalid(),
        Err(_) => return unavailable(),
    };
    for member in &members {
        match allowed(
            accounts,
            account_id,
            Capability::BalancesRead,
            &member.network_slug,
        )
        .await
        {
            Ok(true) => {}
            Ok(false) => return StatusCode::FORBIDDEN.into_response(),
            Err(()) => return unavailable(),
        }
    }
    let as_of = match AsOf::try_from(AsOfRequest {
        kind: form.as_of_kind.unwrap_or_else(|| "latest".to_string()),
        timestamp: non_empty(form.as_of_timestamp),
        block_number: non_empty(form.as_of_block_number),
    }) {
        Ok(value) => value,
        Err(_) => return invalid(),
    };
    let quote_currency = form.quote_currency.unwrap_or_else(|| "USD".to_string());
    let command = match crate::application::balances::command::GetBalancesCommand::try_new(
        as_of.clone(),
        members
            .iter()
            .map(|member| OnchainAccount {
                network_slug: member.network_slug.clone(),
                address: member.address.clone(),
                client_ref: member.client_ref.clone(),
            })
            .collect(),
        quote_currency.clone(),
        TokenSelector {
            asset_slugs: asset_slugs.clone(),
            contract_addresses: vec![],
        },
    ) {
        Ok(value) => value,
        Err(_) => return invalid(),
    };
    let result = match state.balance_service.resolve(command).await {
        Ok(value) => value,
        Err(_) => return unavailable(),
    };
    let payload = json!({
        "operation": "treasury.snapshot",
        "valuation": if matches!(as_of, AsOf::Latest) { "latest_quote_when_available" } else { "unavailable_historical_quote" },
        "balances": treasury_balance_payload(&result),
    });
    if service
        .create_treasury_snapshot(
            workspace.id,
            requested_as_of_json(&as_of),
            &quote_currency,
            json!(asset_slugs),
            payload,
        )
        .await
        .is_err()
    {
        return unavailable();
    }
    Redirect::to(&format!("/workspaces/{workspace_id}/treasury")).into_response()
}

async fn treasury_json(
    State(state): State<HttpState>,
    Extension(principal): Extension<ApiKeyPrincipal>,
    Path(workspace_id): Path<String>,
) -> Response {
    let Some(account_id) = principal.ib_account_id else {
        return ApiError::capability_not_granted().into_response();
    };
    let Some(service) = service(&state) else {
        return ApiError::database_unavailable_for_auth().into_response();
    };
    let workspace = match service.find(account_id, &workspace_id).await {
        Ok(Some(value)) => value,
        Ok(None) => return not_found(),
        Err(_) => return ApiError::database_unavailable_for_auth().into_response(),
    };
    let snapshots = match service.treasury_snapshots(workspace.id).await {
        Ok(value) => value,
        Err(_) => return ApiError::database_unavailable_for_auth().into_response(),
    };
    let mut response =
        Json(json!({"ok":true,"workspace_id":workspace.public_id,"snapshots":snapshots}))
            .into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    response
}

fn treasury_balance_payload(result: &GetBalancesResult) -> Value {
    json!({
        "as_of": format!("{:?}", result.as_of),
        "quote_currency": result.quote_currency,
        "accounts": result.accounts.iter().map(|account| json!({
            "network_slug": account.account.network_slug,
            "address": account.account.address,
            "evidence": account.evidence.as_ref().map(|evidence| json!({"source":"bigwig","block_number":evidence.block_number,"block_hash":evidence.block_hash,"block_timestamp":evidence.block_timestamp,"observed_at":evidence.observed_at})),
            "items": account.items.iter().map(balance_item).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    })
}

#[derive(Deserialize)]
struct NameForm {
    name: String,
    csrf: String,
}
async fn rename_workspace(
    State(state): State<HttpState>,
    headers: HeaderMap,
    Extension(principal): Extension<BrowserPrincipal>,
    Path(workspace_id): Path<String>,
    Form(form): Form<NameForm>,
) -> Response {
    let Some((account_id, csrf_hash)) = authenticated(principal) else {
        return Redirect::to("/login").into_response();
    };
    if !csrf_valid(&state, &headers, &csrf_hash, &form.csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Some(service) = service(&state) else {
        return unavailable();
    };
    match service.rename(account_id, &workspace_id, &form.name).await {
        Ok(true) => Redirect::to(&format!("/workspaces/{workspace_id}")).into_response(),
        Ok(false) => not_found(),
        Err(error) => workspace_error(error),
    }
}

#[derive(Deserialize)]
struct CsrfForm {
    csrf: String,
}
async fn archive_workspace(
    State(state): State<HttpState>,
    headers: HeaderMap,
    Extension(principal): Extension<BrowserPrincipal>,
    Path(workspace_id): Path<String>,
    Form(form): Form<CsrfForm>,
) -> Response {
    set_archive(state, headers, principal, workspace_id, form.csrf, true).await
}
async fn restore_workspace(
    State(state): State<HttpState>,
    headers: HeaderMap,
    Extension(principal): Extension<BrowserPrincipal>,
    Path(workspace_id): Path<String>,
    Form(form): Form<CsrfForm>,
) -> Response {
    set_archive(state, headers, principal, workspace_id, form.csrf, false).await
}
async fn set_archive(
    state: HttpState,
    headers: HeaderMap,
    principal: BrowserPrincipal,
    workspace_id: String,
    csrf: String,
    archived: bool,
) -> Response {
    let Some((account_id, csrf_hash)) = authenticated(principal) else {
        return Redirect::to("/login").into_response();
    };
    if !csrf_valid(&state, &headers, &csrf_hash, &csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Some(service) = service(&state) else {
        return unavailable();
    };
    match service.archive(account_id, &workspace_id, archived).await {
        Ok(true) => Redirect::to(&format!("/workspaces/{workspace_id}")).into_response(),
        Ok(false) => not_found(),
        Err(_) => unavailable(),
    }
}

#[derive(Deserialize)]
struct AddressForm {
    network_slug: String,
    address: String,
    client_ref: Option<String>,
    csrf: String,
}
async fn add_address(
    State(state): State<HttpState>,
    headers: HeaderMap,
    Extension(principal): Extension<BrowserPrincipal>,
    Path(workspace_id): Path<String>,
    Form(form): Form<AddressForm>,
) -> Response {
    let Some((account_id, csrf_hash)) = authenticated(principal) else {
        return Redirect::to("/login").into_response();
    };
    if !csrf_valid(&state, &headers, &csrf_hash, &form.csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Some(service) = service(&state) else {
        return unavailable();
    };
    let workspace = match service.find(account_id, &workspace_id).await {
        Ok(Some(value)) => value,
        Ok(None) => return not_found(),
        Err(_) => return unavailable(),
    };
    match service
        .add_member(
            &workspace,
            &form.network_slug,
            &form.address,
            form.client_ref.as_deref(),
        )
        .await
    {
        Ok(_) => Redirect::to(&format!("/workspaces/{workspace_id}")).into_response(),
        Err(error) => workspace_error(error),
    }
}

async fn member_detail(
    State(state): State<HttpState>,
    headers: HeaderMap,
    Extension(principal): Extension<BrowserPrincipal>,
    Path((workspace_id, member_id)): Path<(String, String)>,
) -> Response {
    let Some((account_id, _)) = authenticated(principal) else {
        return Redirect::to("/login").into_response();
    };
    let Some(service) = service(&state) else {
        return unavailable();
    };
    let csrf = match page_csrf_token(&headers) {
        Ok(csrf) => csrf,
        Err(response) => return response,
    };
    match service
        .find_member(account_id, &workspace_id, &member_id)
        .await
    {
        Ok(Some((workspace, member))) => web::private_html_response(MemberTemplate {
            workspace,
            member,
            csrf,
        }),
        Ok(None) => not_found(),
        Err(_) => unavailable(),
    }
}

#[derive(Deserialize)]
struct LabelForm {
    label: String,
    csrf: String,
}
async fn add_label(
    State(state): State<HttpState>,
    headers: HeaderMap,
    Extension(principal): Extension<BrowserPrincipal>,
    Path((workspace_id, member_id)): Path<(String, String)>,
    Form(form): Form<LabelForm>,
) -> Response {
    mutate_label(
        state,
        headers,
        principal,
        workspace_id,
        member_id,
        form,
        true,
    )
    .await
}
async fn remove_label(
    State(state): State<HttpState>,
    headers: HeaderMap,
    Extension(principal): Extension<BrowserPrincipal>,
    Path((workspace_id, member_id)): Path<(String, String)>,
    Form(form): Form<LabelForm>,
) -> Response {
    mutate_label(
        state,
        headers,
        principal,
        workspace_id,
        member_id,
        form,
        false,
    )
    .await
}
async fn mutate_label(
    state: HttpState,
    headers: HeaderMap,
    principal: BrowserPrincipal,
    workspace_id: String,
    member_id: String,
    form: LabelForm,
    add: bool,
) -> Response {
    let Some((account_id, csrf_hash)) = authenticated(principal) else {
        return Redirect::to("/login").into_response();
    };
    if !csrf_valid(&state, &headers, &csrf_hash, &form.csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Some(service) = service(&state) else {
        return unavailable();
    };
    let Some((workspace, member)) = (match service
        .find_member(account_id, &workspace_id, &member_id)
        .await
    {
        Ok(value) => value,
        Err(_) => return unavailable(),
    }) else {
        return not_found();
    };
    let result = if add {
        service.add_label(&workspace, &member, &form.label).await
    } else {
        service.remove_label(&workspace, &member, &form.label).await
    };
    match result {
        Ok(()) => Redirect::to(&format!("/workspaces/{workspace_id}/addresses/{member_id}"))
            .into_response(),
        Err(error) => workspace_error(error),
    }
}

#[derive(Deserialize, Default)]
struct BalanceQuery {
    as_of_kind: Option<String>,
    as_of_timestamp: Option<String>,
    as_of_block_number: Option<String>,
    asset_slugs: Option<String>,
    contract_addresses: Option<String>,
    quote_currency: Option<String>,
}
async fn balance_view(
    State(state): State<HttpState>,
    Extension(principal): Extension<BrowserPrincipal>,
    Path((workspace_id, member_id)): Path<(String, String)>,
    Query(query): Query<BalanceQuery>,
) -> Response {
    let Some((account_id, _)) = authenticated(principal) else {
        return Redirect::to("/login").into_response();
    };
    let (Some(service), Some(accounts)) = (service(&state), state.account_repository.as_ref())
    else {
        return unavailable();
    };
    let Some((workspace, member)) = (match service
        .find_member(account_id, &workspace_id, &member_id)
        .await
    {
        Ok(value) => value,
        Err(_) => return unavailable(),
    }) else {
        return not_found();
    };
    match allowed(
        accounts,
        account_id,
        Capability::BalancesRead,
        &member.network_slug,
    )
    .await
    {
        Ok(true) => {}
        Ok(false) => return StatusCode::FORBIDDEN.into_response(),
        Err(()) => return unavailable(),
    }
    let tokens = token_selector(query.asset_slugs, query.contract_addresses);
    if tokens.asset_slugs.is_empty() && tokens.contract_addresses.is_empty() {
        return web::private_html_response(DataViewTemplate {
            title: "Balance view",
            workspace,
            member,
            detail: "Choose one or more token selectors to resolve balances.".to_string(),
        });
    }
    let as_of = match AsOf::try_from(AsOfRequest {
        kind: query.as_of_kind.unwrap_or_else(|| "latest".to_string()),
        timestamp: non_empty(query.as_of_timestamp),
        block_number: non_empty(query.as_of_block_number),
    }) {
        Ok(value) => value,
        Err(_) => return invalid(),
    };
    let command = match crate::application::balances::command::GetBalancesCommand::try_new(
        as_of,
        vec![OnchainAccount {
            network_slug: member.network_slug.clone(),
            address: member.address.clone(),
            client_ref: member.client_ref.clone(),
        }],
        query.quote_currency.unwrap_or_else(|| "USD".to_string()),
        tokens,
    ) {
        Ok(command) => command,
        Err(_) => return invalid(),
    };
    let result = state.balance_service.resolve(command).await;
    let detail = match result {
        Ok(value) => {
            let Some(repository) = state.workspace_repository.as_ref() else {
                return unavailable();
            };
            if repository
                .append_observation(
                    workspace.id,
                    "balance.observed",
                    balance_payload(&member, &value),
                )
                .await
                .is_err()
            {
                warn!(workspace_id = %workspace.id, "failed to append balance observation event");
            }
            format!("{value:#?}")
        }
        Err(error) => {
            let payload = json!({"operation":"balances.read","outcome":"unavailable","request":{"network_slug":member.network_slug,"address":member.address},"source":"bigwig","error":error.to_string()});
            let Some(repository) = state.workspace_repository.as_ref() else {
                return unavailable();
            };
            if repository
                .append_observation(workspace.id, "balance.observed", payload)
                .await
                .is_err()
            {
                warn!(workspace_id = %workspace.id, "failed to append balance observation event");
            }
            format!("Balance data is unavailable: {error}")
        }
    };
    web::private_html_response(DataViewTemplate {
        title: "Balance view",
        workspace,
        member,
        detail,
    })
}

#[derive(Deserialize, Default)]
struct TransferQuery {
    lookback_seconds: Option<u64>,
    direction: Option<String>,
    asset_slugs: Option<String>,
    contract_addresses: Option<String>,
}
async fn transfer_view(
    State(state): State<HttpState>,
    Extension(principal): Extension<BrowserPrincipal>,
    Path((workspace_id, member_id)): Path<(String, String)>,
    Query(query): Query<TransferQuery>,
) -> Response {
    let Some((account_id, _)) = authenticated(principal) else {
        return Redirect::to("/login").into_response();
    };
    let (Some(service), Some(accounts)) = (service(&state), state.account_repository.as_ref())
    else {
        return unavailable();
    };
    let Some((workspace, member)) = (match service
        .find_member(account_id, &workspace_id, &member_id)
        .await
    {
        Ok(value) => value,
        Err(_) => return unavailable(),
    }) else {
        return not_found();
    };
    if member.network_slug != "eth-mainnet" {
        return web::private_html_response(DataViewTemplate {
            title: "Transfer view",
            workspace,
            member,
            detail: "Transfer search is currently available only for eth-mainnet.".to_string(),
        });
    }
    match allowed(
        accounts,
        account_id,
        Capability::Erc20TransfersRead,
        &member.network_slug,
    )
    .await
    {
        Ok(true) => {}
        Ok(false) => return StatusCode::FORBIDDEN.into_response(),
        Err(()) => return unavailable(),
    }
    let direction = match query.direction.as_deref().unwrap_or("any") {
        "any" => TransferDirection::Any,
        "from" => TransferDirection::From,
        "to" => TransferDirection::To,
        _ => return invalid(),
    };
    let lookback_seconds = query.lookback_seconds.unwrap_or(86_400);
    let Ok(window) = LookbackWindow::latest(lookback_seconds) else {
        return invalid();
    };
    let input = Erc20TransferSearchInput {
        network_slug: member.network_slug.clone(),
        address: member.address.clone(),
        direction,
        window: OnchainWindow::Lookback(window),
        asset_slugs: split_values(query.asset_slugs),
        contract_addresses: split_values(query.contract_addresses),
    };
    let plan = match build_search_plan(
        input,
        state.canonical_registry.clone(),
        state.config.erc20_transfers_max_token_filters,
    )
    .await
    {
        Ok(value) => value,
        Err(error) => {
            return web::private_html_response(DataViewTemplate {
                title: "Transfer view",
                workspace,
                member,
                detail: format!("Transfer request is invalid: {error}"),
            })
        }
    };
    let Some(client) = state.bigwig_client.as_ref() else {
        return unavailable();
    };
    let detail = match execute_search_plan(plan, state.canonical_registry.clone(), client).await {
        Ok(value) => {
            let Some(repository) = state.workspace_repository.as_ref() else {
                return unavailable();
            };
            if repository
                .append_observation(
                    workspace.id,
                    "transfer.observed",
                    transfer_payload(&member, &value),
                )
                .await
                .is_err()
            {
                warn!(workspace_id = %workspace.id, "failed to append transfer observation event");
            }
            format!("{value:#?}")
        }
        Err(error) => {
            let payload = json!({"operation":"transfers.read","outcome":"unavailable","request":{"network_slug":member.network_slug,"address":member.address},"source":"bigwig","error":error.to_string()});
            let Some(repository) = state.workspace_repository.as_ref() else {
                return unavailable();
            };
            if repository
                .append_observation(workspace.id, "transfer.observed", payload)
                .await
                .is_err()
            {
                warn!(workspace_id = %workspace.id, "failed to append transfer observation event");
            }
            format!("Transfer data is unavailable: {error}")
        }
    };
    web::private_html_response(DataViewTemplate {
        title: "Transfer view",
        workspace,
        member,
        detail,
    })
}

async fn allowed(
    repository: &crate::adapters::postgres::AccountRepository,
    account_id: uuid::Uuid,
    capability: Capability,
    network_slug: &str,
) -> Result<bool, ()> {
    repository
        .has_active_capability(account_id, capability, network_slug)
        .await
        .map_err(|_| ())
}
fn split_values(value: Option<String>) -> Vec<String> {
    value
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect()
}
fn non_empty(value: Option<String>) -> Option<String> {
    value.and_then(|value| (!value.trim().is_empty()).then(|| value.trim().to_string()))
}
fn token_selector(
    asset_slugs: Option<String>,
    contract_addresses: Option<String>,
) -> TokenSelector {
    TokenSelector {
        asset_slugs: split_values(asset_slugs),
        contract_addresses: split_values(contract_addresses),
    }
}

fn balance_payload(member: &WorkspaceMemberAddress, result: &GetBalancesResult) -> Value {
    let accounts = result.accounts.iter().map(|account| json!({
        "account": {"network_slug": account.account.network_slug, "address": account.account.address},
        "evidence": account.evidence.as_ref().map(|evidence| json!({"network_slug": evidence.network_slug, "observed_at": evidence.observed_at, "block_number": evidence.block_number, "block_hash": evidence.block_hash, "block_timestamp": evidence.block_timestamp})),
        "items": account.items.iter().map(balance_item).collect::<Vec<_>>(),
    })).collect::<Vec<_>>();
    json!({"operation":"balances.read","outcome":"complete","request":{"network_slug":member.network_slug,"address":member.address,"as_of":format!("{:?}", result.as_of),"quote_currency":result.quote_currency},"source":"bigwig","accounts":accounts})
}
fn balance_item(item: &BalanceItemOutcome) -> Value {
    match item {
        BalanceItemOutcome::Resolved {
            target,
            raw_amount,
            amount,
            quote,
        } => {
            json!({"status":"resolved","network_slug":target.network_slug,"asset_slug":target.asset_slug,"contract_address":target.contract_address(),"raw_amount":raw_amount,"amount":amount,"quote":match quote { BalanceQuoteOutcome::Available { currency, unit_price, value, price_as_of } => json!({"status":"available","currency":currency,"unit_price":unit_price,"value":value,"price_as_of":price_as_of}), BalanceQuoteOutcome::Unavailable { code } => json!({"status":"unavailable","code":format!("{:?}",code)}), BalanceQuoteOutcome::Unsupported => json!({"status":"unsupported"}) }})
        }
        BalanceItemOutcome::Skipped {
            network_slug,
            asset_slug,
        } => json!({"status":"skipped","network_slug":network_slug,"asset_slug":asset_slug}),
        BalanceItemOutcome::Failed { target, code } => {
            json!({"status":"failed","network_slug":target.network_slug,"asset_slug":target.asset_slug,"code":format!("{:?}",code)})
        }
    }
}
fn transfer_payload(
    member: &WorkspaceMemberAddress,
    result: &crate::application::erc20_transfers::service::Erc20TransferSearchResult,
) -> Value {
    json!({"operation":"transfers.read","outcome":if result.extraction.truncated {"partial"} else {"complete"},"request":{"network_slug":member.network_slug,"address":member.address,"direction":format!("{:?}",result.plan.extraction_request.direction),"window":format!("{:?}",result.plan.extraction_request.window)},"source":"bigwig","truncated":result.extraction.truncated,"transfers":result.extraction.rows.iter().map(|row| json!({"block_number":row.block_number,"tx_hash":row.tx_hash,"log_index":row.log_index,"token":row.token,"from":row.from,"to":row.to,"value":row.value})).collect::<Vec<_>>()})
}

fn requested_as_of_json(as_of: &AsOf) -> Value {
    match as_of {
        AsOf::Latest => json!({"kind": "latest"}),
        AsOf::Timestamp { timestamp } => {
            json!({"kind": "timestamp", "timestamp": timestamp})
        }
        AsOf::BlockNumber { block_number } => {
            json!({"kind": "block_number", "block_number": block_number})
        }
    }
}

#[derive(Template)]
#[template(path = "web/workspaces.html")]
struct WorkspaceListTemplate {
    workspaces: Vec<Workspace>,
    csrf: String,
}
#[derive(Template)]
#[template(path = "web/workspace.html")]
struct WorkspaceDetailTemplate {
    workspace: Workspace,
    members: Vec<WorkspaceMemberAddress>,
    csrf: String,
}
#[derive(Template)]
#[template(path = "web/workspace_activity.html")]
struct ActivityTemplate {
    workspace: Workspace,
    events: Vec<WorkspaceActivityEvent>,
    next_before: Option<String>,
}
#[derive(Template)]
#[template(path = "web/workspace_member.html")]
struct MemberTemplate {
    workspace: Workspace,
    member: WorkspaceMemberAddress,
    csrf: String,
}
#[derive(Template)]
#[template(path = "web/workspace_data.html")]
struct DataViewTemplate {
    title: &'static str,
    workspace: Workspace,
    member: WorkspaceMemberAddress,
    detail: String,
}
#[derive(Template)]
#[template(path = "web/workspace_treasury.html")]
struct TreasuryTemplate {
    workspace: Workspace,
    snapshots: Vec<WorkspaceTreasurySnapshot>,
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{Arc, Mutex},
        time::SystemTime,
    };

    use axum::http::{HeaderMap, StatusCode};
    use sqlx::PgPool;
    use uuid::Uuid;

    use crate::{
        adapters::postgres::{
            errors::RepositoryError,
            workspaces::{Workspace, WorkspaceMemberAddress},
            AccountRepository, WorkspaceRepository,
        },
        application::workspaces::{
            portfolio::{
                CurrentWorkspacePortfolio, PortfolioObservationStatus,
                WorkspacePortfolioResolution, WorkspacePortfolioResolverError,
            },
            WorkspaceInputError, WorkspaceService,
        },
        test_utils::{fixtures::registry::embedded_canonical_registry, postgres::migrated_pool},
    };

    use super::{
        activity_page, is_event_id, non_empty, page_csrf_token, portfolio_resolution_networks,
        resolve_workspace_portfolio, split_values, workspace_error, ActivityQuery,
        WorkspacePortfolioAccessError, WorkspaceServiceError,
    };

    type PortfolioResolutionCall = (Workspace, Vec<WorkspaceMemberAddress>);

    #[derive(Clone, Default)]
    struct RecordingPortfolioResolver {
        calls: Arc<Mutex<Vec<PortfolioResolutionCall>>>,
        fail: bool,
    }

    impl RecordingPortfolioResolver {
        fn call_count(&self) -> usize {
            self.calls.lock().unwrap().len()
        }

        fn members(&self) -> Vec<WorkspaceMemberAddress> {
            self.calls.lock().unwrap()[0].1.clone()
        }
    }

    impl WorkspacePortfolioResolution for RecordingPortfolioResolver {
        async fn resolve_workspace_portfolio(
            &self,
            workspace: Workspace,
            members: Vec<WorkspaceMemberAddress>,
        ) -> Result<CurrentWorkspacePortfolio, WorkspacePortfolioResolverError> {
            self.calls
                .lock()
                .unwrap()
                .push((workspace.clone(), members.clone()));
            if self.fail {
                return Err(WorkspacePortfolioResolverError::InvalidDecimal);
            }
            Ok(CurrentWorkspacePortfolio {
                workspace,
                resolved_at: SystemTime::UNIX_EPOCH,
                quote_currency: "USD".to_string(),
                members: Vec::new(),
                assets: Vec::new(),
                known_value: "0".to_string(),
                valuation_status: PortfolioObservationStatus::Complete,
            })
        }
    }

    async fn create_account(pool: &PgPool) -> Uuid {
        let account_id = Uuid::new_v4();
        sqlx::query("insert into mother_api.ib_account (id, public_id) values ($1, $2)")
            .bind(account_id)
            .bind(format!("iba_{}", account_id.simple()))
            .execute(pool)
            .await
            .unwrap();
        account_id
    }

    async fn create_workspace(pool: &PgPool, account_id: Uuid) -> (WorkspaceService, Workspace) {
        let service = WorkspaceService::new(WorkspaceRepository::database(pool.clone()));
        let workspace = service
            .create(account_id, "Portfolio authorization", None)
            .await
            .unwrap();
        (service, workspace)
    }

    async fn grant_balances_read(pool: &PgPool, account_id: Uuid, network_slug: &str) {
        sqlx::query("insert into mother_api.ib_account_capability_grant (ib_account_id, capability_id, network_scope) values ($1, 'balances.read', $2)")
            .bind(account_id)
            .bind(network_slug)
            .execute(pool)
            .await
            .unwrap();
    }

    async fn remove_account(pool: &PgPool, account_id: Uuid) {
        let workspace_ids = sqlx::query_scalar::<_, Uuid>(
            "select id from mother_api.workspace where owner_ib_account_id = $1",
        )
        .bind(account_id)
        .fetch_all(pool)
        .await
        .unwrap();
        for workspace_id in workspace_ids {
            sqlx::query("delete from mother_api.workspace_member_address_label where member_address_id in (select id from mother_api.workspace_member_address where workspace_id = $1)")
                .bind(workspace_id)
                .execute(pool)
                .await
                .unwrap();
            sqlx::query("delete from mother_api.workspace_member_address where workspace_id = $1")
                .bind(workspace_id)
                .execute(pool)
                .await
                .unwrap();
            sqlx::query("delete from mother_api.workspace where id = $1")
                .bind(workspace_id)
                .execute(pool)
                .await
                .unwrap();
        }
        sqlx::query("delete from mother_api.ib_account where id = $1")
            .bind(account_id)
            .execute(pool)
            .await
            .unwrap();
    }

    async fn activity_count(pool: &PgPool, workspace_id: Uuid) -> i64 {
        sqlx::query_scalar(
            "select count(*) from mother_api.workspace_activity_event where workspace_id = $1",
        )
        .bind(workspace_id)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    async fn snapshot_count(pool: &PgPool, workspace_id: Uuid) -> i64 {
        sqlx::query_scalar(
            "select count(*) from mother_api.workspace_treasury_snapshot where workspace_id = $1",
        )
        .bind(workspace_id)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    #[test]
    fn optional_form_fields_drop_blank_values() {
        assert_eq!(non_empty(Some("  ".to_string())), None);
        assert_eq!(non_empty(Some("  42 ".to_string())), Some("42".to_string()));
    }

    #[test]
    fn comma_separated_selectors_are_trimmed_and_skip_blanks() {
        assert_eq!(
            split_values(Some(" ethereum, ,usdc ".to_string())),
            vec!["ethereum", "usdc"]
        );
    }

    #[test]
    fn missing_page_csrf_cookie_is_forbidden() {
        let response = page_csrf_token(&HeaderMap::new()).unwrap_err();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn workspace_input_errors_are_bad_requests() {
        let response = workspace_error(WorkspaceInputError::InvalidName.into());

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn workspace_repository_errors_are_unavailable() {
        let response = workspace_error(WorkspaceServiceError::Repository(RepositoryError::test()));

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[test]
    fn activity_cursor_and_limits_are_bounded() {
        assert!(is_event_id("wae_0123456789abcdef0123456789abcdef"));
        assert!(!is_event_id("wae_0123456789abcdef0123456789abcdeF"));
        assert!(activity_page(ActivityQuery {
            limit: Some(100),
            before: Some("wae_0123456789abcdef0123456789abcdef".to_string()),
        })
        .is_ok());
        assert!(activity_page(ActivityQuery {
            limit: Some(101),
            before: None,
        })
        .is_err());
    }

    #[test]
    fn portfolio_resolution_networks_are_sorted_and_deduplicated() {
        let members = vec![
            WorkspaceMemberAddress {
                id: Uuid::new_v4(),
                public_id: "wma_b".to_string(),
                network_slug: "base-mainnet".to_string(),
                address: "0x2222222222222222222222222222222222222222".to_string(),
                client_ref: None,
                labels: Vec::new(),
            },
            WorkspaceMemberAddress {
                id: Uuid::new_v4(),
                public_id: "wma_e1".to_string(),
                network_slug: "eth-mainnet".to_string(),
                address: "0x1111111111111111111111111111111111111111".to_string(),
                client_ref: None,
                labels: Vec::new(),
            },
            WorkspaceMemberAddress {
                id: Uuid::new_v4(),
                public_id: "wma_e2".to_string(),
                network_slug: "eth-mainnet".to_string(),
                address: "0x3333333333333333333333333333333333333333".to_string(),
                client_ref: None,
                labels: Vec::new(),
            },
        ];

        assert_eq!(
            portfolio_resolution_networks(embedded_canonical_registry(), &members).unwrap(),
            vec!["base-mainnet", "eth-mainnet"]
        );
    }

    #[tokio::test]
    async fn portfolio_resolution_authorizes_all_networks_before_invoking_the_resolver() {
        let Some(pool) = migrated_pool().await else {
            return;
        };
        let account_id = create_account(&pool).await;
        let (service, workspace) = create_workspace(&pool, account_id).await;
        service
            .add_member(
                &workspace,
                "eth-mainnet",
                "0x1111111111111111111111111111111111111111",
                None,
            )
            .await
            .unwrap();
        service
            .add_member(
                &workspace,
                "base-mainnet",
                "0x2222222222222222222222222222222222222222",
                None,
            )
            .await
            .unwrap();
        grant_balances_read(&pool, account_id, "eth-mainnet").await;
        let resolver = RecordingPortfolioResolver::default();
        let accounts = AccountRepository::database(pool.clone());

        let result = resolve_workspace_portfolio(
            &service,
            &accounts,
            embedded_canonical_registry(),
            &resolver,
            account_id,
            &workspace.public_id,
        )
        .await;

        assert_eq!(result, Err(WorkspacePortfolioAccessError::Forbidden));
        assert_eq!(resolver.call_count(), 0);
        remove_account(&pool, account_id).await;
    }

    #[tokio::test]
    async fn portfolio_resolution_allows_an_owner_and_preserves_duplicate_network_members() {
        let Some(pool) = migrated_pool().await else {
            return;
        };
        let account_id = create_account(&pool).await;
        let (service, workspace) = create_workspace(&pool, account_id).await;
        service
            .add_member(
                &workspace,
                "eth-mainnet",
                "0x1111111111111111111111111111111111111111",
                None,
            )
            .await
            .unwrap();
        service
            .add_member(
                &workspace,
                "base-mainnet",
                "0x3333333333333333333333333333333333333333",
                None,
            )
            .await
            .unwrap();
        grant_balances_read(&pool, account_id, "base-mainnet").await;
        service
            .add_member(
                &workspace,
                "eth-mainnet",
                "0x2222222222222222222222222222222222222222",
                None,
            )
            .await
            .unwrap();
        grant_balances_read(&pool, account_id, "eth-mainnet").await;
        let resolver = RecordingPortfolioResolver::default();
        let accounts = AccountRepository::database(pool.clone());

        let result = resolve_workspace_portfolio(
            &service,
            &accounts,
            embedded_canonical_registry(),
            &resolver,
            account_id,
            &workspace.public_id,
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(resolver.call_count(), 1);
        assert_eq!(resolver.members().len(), 3);
        remove_account(&pool, account_id).await;
    }

    #[tokio::test]
    async fn portfolio_resolution_hides_cross_account_workspaces_before_member_access() {
        let Some(pool) = migrated_pool().await else {
            return;
        };
        let owner_id = create_account(&pool).await;
        let requester_id = create_account(&pool).await;
        let (service, workspace) = create_workspace(&pool, owner_id).await;
        service
            .add_member(
                &workspace,
                "eth-mainnet",
                "0x1111111111111111111111111111111111111111",
                None,
            )
            .await
            .unwrap();
        let resolver = RecordingPortfolioResolver::default();
        let accounts = AccountRepository::database(pool.clone());

        let result = resolve_workspace_portfolio(
            &service,
            &accounts,
            embedded_canonical_registry(),
            &resolver,
            requester_id,
            &workspace.public_id,
        )
        .await;

        assert_eq!(result, Err(WorkspacePortfolioAccessError::NotFound));
        assert_eq!(resolver.call_count(), 0);
        remove_account(&pool, owner_id).await;
        remove_account(&pool, requester_id).await;
    }

    #[tokio::test]
    async fn an_empty_workspace_resolves_without_a_balance_grant() {
        let Some(pool) = migrated_pool().await else {
            return;
        };
        let account_id = create_account(&pool).await;
        let (service, workspace) = create_workspace(&pool, account_id).await;
        let resolver = RecordingPortfolioResolver::default();
        let accounts = AccountRepository::database(pool.clone());

        let result = resolve_workspace_portfolio(
            &service,
            &accounts,
            embedded_canonical_registry(),
            &resolver,
            account_id,
            &workspace.public_id,
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(resolver.call_count(), 1);
        assert!(resolver.members().is_empty());
        remove_account(&pool, account_id).await;
    }

    #[tokio::test]
    async fn portfolio_resolution_does_not_persist_snapshots_or_activity() {
        let Some(pool) = migrated_pool().await else {
            return;
        };
        let account_id = create_account(&pool).await;
        let (service, workspace) = create_workspace(&pool, account_id).await;
        service
            .add_member(
                &workspace,
                "eth-mainnet",
                "0x1111111111111111111111111111111111111111",
                None,
            )
            .await
            .unwrap();
        grant_balances_read(&pool, account_id, "eth-mainnet").await;
        let activities_before = activity_count(&pool, workspace.id).await;
        let snapshots_before = snapshot_count(&pool, workspace.id).await;
        let resolver = RecordingPortfolioResolver::default();
        let accounts = AccountRepository::database(pool.clone());

        let result = resolve_workspace_portfolio(
            &service,
            &accounts,
            embedded_canonical_registry(),
            &resolver,
            account_id,
            &workspace.public_id,
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(activity_count(&pool, workspace.id).await, activities_before);
        assert_eq!(snapshot_count(&pool, workspace.id).await, snapshots_before);
        remove_account(&pool, account_id).await;
    }

    #[tokio::test]
    async fn portfolio_resolver_failures_are_service_unavailable() {
        let Some(pool) = migrated_pool().await else {
            return;
        };
        let account_id = create_account(&pool).await;
        let (service, workspace) = create_workspace(&pool, account_id).await;
        let resolver = RecordingPortfolioResolver {
            fail: true,
            ..Default::default()
        };
        let accounts = AccountRepository::database(pool.clone());

        let result = resolve_workspace_portfolio(
            &service,
            &accounts,
            embedded_canonical_registry(),
            &resolver,
            account_id,
            &workspace.public_id,
        )
        .await;

        assert_eq!(result, Err(WorkspacePortfolioAccessError::Unavailable));
        assert_eq!(resolver.call_count(), 1);
        remove_account(&pool, account_id).await;
    }
}
