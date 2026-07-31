use askama::Template;
use axum::{
    extract::{Extension, Form, Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
    Router,
};
use serde::Deserialize;

use crate::{
    adapters::{
        http::{
            dto::onchain_time::as_of::AsOfRequest,
            web::{self, BrowserPrincipal},
        },
        postgres::workspaces::{Workspace, WorkspaceMemberAddress},
    },
    application::{
        balances::{
            catalog::CatalogBalanceTargetResolver, quote::PriceQuoteClient,
            service::BalanceSnapshotService,
        },
        erc20_transfers::service::{
            build_search_plan, execute_search_plan, Erc20TransferSearchInput,
        },
        workspaces::{WorkspaceService, WorkspaceServiceError},
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
    state::AppState,
};

pub(crate) fn routes(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/workspaces", get(list_workspaces).post(create_workspace))
        .route("/workspaces/{workspace_id}", get(workspace_detail))
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

fn service(state: &AppState) -> Option<WorkspaceService> {
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
    state: &AppState,
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

async fn list_workspaces(
    State(state): State<AppState>,
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
    State(state): State<AppState>,
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
    State(state): State<AppState>,
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

#[derive(Deserialize)]
struct NameForm {
    name: String,
    csrf: String,
}
async fn rename_workspace(
    State(state): State<AppState>,
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
    State(state): State<AppState>,
    headers: HeaderMap,
    Extension(principal): Extension<BrowserPrincipal>,
    Path(workspace_id): Path<String>,
    Form(form): Form<CsrfForm>,
) -> Response {
    set_archive(state, headers, principal, workspace_id, form.csrf, true).await
}
async fn restore_workspace(
    State(state): State<AppState>,
    headers: HeaderMap,
    Extension(principal): Extension<BrowserPrincipal>,
    Path(workspace_id): Path<String>,
    Form(form): Form<CsrfForm>,
) -> Response {
    set_archive(state, headers, principal, workspace_id, form.csrf, false).await
}
async fn set_archive(
    state: AppState,
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
    State(state): State<AppState>,
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
    State(state): State<AppState>,
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
    State(state): State<AppState>,
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
    State(state): State<AppState>,
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
    state: AppState,
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
    State(state): State<AppState>,
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
    let Some(repository) = state.asset_repository.clone() else {
        return unavailable();
    };
    let result = BalanceSnapshotService::new(
        CatalogBalanceTargetResolver::new(repository),
        state.bigwig_client.clone(),
        state
            .price_indexer_client
            .clone()
            .map(PriceQuoteClient::new),
    )
    .resolve(command)
    .await;
    let detail = match result {
        Ok(value) => format!("{value:#?}"),
        Err(error) => format!("Balance data is unavailable: {error}"),
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
    State(state): State<AppState>,
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
        state.asset_repository.clone(),
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
    let detail = match execute_search_plan(plan, state.asset_repository.clone(), client).await {
        Ok(value) => format!("{value:#?}"),
        Err(error) => format!("Transfer data is unavailable: {error}"),
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

#[cfg(test)]
mod tests {
    use axum::http::{HeaderMap, StatusCode};

    use crate::{
        adapters::postgres::errors::RepositoryError, application::workspaces::WorkspaceInputError,
    };

    use super::{non_empty, page_csrf_token, split_values, workspace_error, WorkspaceServiceError};

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
}
