use std::collections::HashSet;

use axum::{body::Bytes, extract::State, http::HeaderMap, Json};
use tracing::warn;

use crate::{
    adapters::http::{
        dto::balances::{
            BalanceAccountRequest, BalanceAsOfRequest, BalanceResponseAssembler,
            BalanceResponseAssemblerError, BalanceTokenSelectorRequest, BulkBalanceRequest,
            BulkBalanceResponse, SingleBalanceRequest, SingleBalanceResponse,
        },
        error::ApiError,
        json_body::parse_json_object_body,
        validation::ensure_json_content_type,
    },
    application::balances::{
        catalog::CatalogBalanceTargetResolver,
        quote::PriceQuoteClient,
        service::{
            BalanceSnapshotAccount, BalanceSnapshotRequest, BalanceSnapshotService,
            BalanceSnapshotServiceError,
        },
    },
    domain::balance_catalog::CatalogResolverError,
    state::AppState,
};

const MAX_ACCOUNTS: usize = 50;
const MAX_ASSETS: usize = 20;
const MAX_RESOLUTION_ITEMS: usize = 1_000;

pub async fn resolve_single_balance(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<SingleBalanceResponse>, ApiError> {
    let request = parse_single_balance_request(&headers, &body)?;
    let request = validate_request(
        request.as_of,
        vec![request.account],
        request.quote_currency,
        request.tokens,
    )?;
    let snapshot = resolve_snapshot(&state, request).await?;
    let response = BalanceResponseAssembler
        .single(snapshot)
        .map_err(balance_assembler_error_to_api_error)?;

    Ok(Json(response))
}

pub async fn resolve_bulk_balances(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<BulkBalanceResponse>, ApiError> {
    let request = parse_bulk_balance_request(&headers, &body)?;
    let request = validate_request(
        request.as_of,
        request.accounts,
        request.quote_currency,
        request.tokens,
    )?;
    let snapshot = resolve_snapshot(&state, request).await?;

    Ok(Json(BalanceResponseAssembler.bulk(snapshot)))
}

fn parse_single_balance_request(
    headers: &HeaderMap,
    body: &[u8],
) -> Result<SingleBalanceRequest, ApiError> {
    ensure_json_content_type(headers).map_err(|_| ApiError::invalid_request())?;
    let request = parse_json_object_body(body).map_err(|_| ApiError::invalid_request())?;

    SingleBalanceRequest::try_from(request)
}

fn parse_bulk_balance_request(
    headers: &HeaderMap,
    body: &[u8],
) -> Result<BulkBalanceRequest, ApiError> {
    ensure_json_content_type(headers).map_err(|_| ApiError::invalid_request())?;
    let request = parse_json_object_body(body).map_err(|_| ApiError::invalid_request())?;

    BulkBalanceRequest::try_from(request)
}

fn validate_request(
    as_of: BalanceAsOfRequest,
    accounts: Vec<BalanceAccountRequest>,
    quote_currency: String,
    tokens: BalanceTokenSelectorRequest,
) -> Result<BalanceSnapshotRequest, ApiError> {
    if as_of.kind != "latest" || as_of.timestamp.is_some() || as_of.block_number.is_some() {
        return Err(ApiError::unsupported_as_of());
    }
    if accounts.is_empty() {
        return Err(ApiError::empty_accounts());
    }
    if tokens.asset_slugs.is_empty() && tokens.contract_addresses.is_empty() {
        return Err(ApiError::empty_tokens());
    }
    if !tokens.contract_addresses.is_empty() {
        return Err(ApiError::unsupported_token_selector());
    }

    let resolution_items = accounts
        .len()
        .checked_mul(tokens.asset_slugs.len())
        .ok_or_else(ApiError::request_too_large)?;
    if accounts.len() > MAX_ACCOUNTS
        || tokens.asset_slugs.len() > MAX_ASSETS
        || resolution_items > MAX_RESOLUTION_ITEMS
    {
        return Err(ApiError::request_too_large());
    }

    let quote_currency = quote_currency.trim().to_ascii_uppercase();
    if !matches!(quote_currency.as_str(), "USD" | "MXN" | "USDC" | "BTC") {
        return Err(ApiError::unsupported_quote_currency());
    }

    if accounts
        .iter()
        .any(|account| !is_evm_address(&account.address))
    {
        return Err(ApiError::invalid_account());
    }

    let mut seen_accounts = HashSet::<(String, String)>::with_capacity(accounts.len());
    for account in &accounts {
        if !seen_accounts.insert((
            account.network_slug.clone(),
            account.address.to_ascii_lowercase(),
        )) {
            return Err(ApiError::duplicate_account());
        }
    }

    let mut seen_assets = HashSet::<String>::with_capacity(tokens.asset_slugs.len());
    for asset_slug in &tokens.asset_slugs {
        if !seen_assets.insert(asset_slug.clone()) {
            return Err(ApiError::duplicate_asset());
        }
    }

    Ok(BalanceSnapshotRequest {
        accounts: accounts
            .into_iter()
            .map(|account| BalanceSnapshotAccount {
                network_slug: account.network_slug,
                address: account.address,
                client_ref: account.client_ref,
            })
            .collect(),
        asset_slugs: tokens.asset_slugs,
        quote_currency,
    })
}

fn is_evm_address(address: &str) -> bool {
    address.len() == 42
        && address.starts_with("0x")
        && address.as_bytes()[2..]
            .iter()
            .all(|character| character.is_ascii_hexdigit())
}

async fn resolve_snapshot(
    state: &AppState,
    request: BalanceSnapshotRequest,
) -> Result<crate::application::balances::service::BalanceSnapshotResult, ApiError> {
    let repository = state
        .asset_repository
        .clone()
        .ok_or_else(ApiError::asset_network_map_unavailable)?;
    let service = BalanceSnapshotService::new(
        CatalogBalanceTargetResolver::new(repository),
        state.bigwig_client.clone(),
        state
            .price_indexer_client
            .clone()
            .map(PriceQuoteClient::new),
    );

    service
        .resolve_latest(request)
        .await
        .map_err(balance_service_error_to_api_error)
}

fn balance_service_error_to_api_error(error: BalanceSnapshotServiceError) -> ApiError {
    match error {
        BalanceSnapshotServiceError::UnsupportedNetwork { .. } => ApiError::unsupported_network(),
        BalanceSnapshotServiceError::UnsupportedAsset { .. } => ApiError::unsupported_asset(),
        BalanceSnapshotServiceError::RequestTooLarge { .. } => ApiError::request_too_large(),
        BalanceSnapshotServiceError::Catalog(CatalogResolverError::Repository(error)) => {
            warn!(%error, "Balance catalog lookup failed");
            ApiError::asset_network_map_unavailable()
        }
        BalanceSnapshotServiceError::Catalog(error) => {
            warn!(%error, "Balance catalog is internally inconsistent");
            ApiError::internal_error()
        }
        BalanceSnapshotServiceError::InvalidPlan {
            network_slug,
            issue,
        } => {
            warn!(
                network_slug,
                ?issue,
                "Balance orchestration plan is invalid"
            );
            ApiError::internal_error()
        }
        BalanceSnapshotServiceError::ExecutionTaskFailed => {
            warn!("Balance orchestration task failed");
            ApiError::internal_error()
        }
    }
}

fn balance_assembler_error_to_api_error(error: BalanceResponseAssemblerError) -> ApiError {
    warn!(?error, "Balance response assembly failed");
    ApiError::internal_error()
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        net::{TcpListener, TcpStream},
    };

    use axum::{http::StatusCode, response::IntoResponse, Router};
    use serde_json::{json, Value};

    use crate::application::balances::service::BalancePlanIssue;
    use crate::test_utils::{
        errors::assert_public_error, fixtures::global_assets::sample_assets, http::post_raw,
    };
    use crate::{
        adapters::{
            bigwig::client::BigwigClient,
            http::router::build_router,
            postgres::{errors::RepositoryError, global_assets::GlobalAssetRepository},
            price_indexer::PriceIndexerClient,
        },
        config::Config,
    };

    use super::*;

    const ACCOUNT_A: &str = "0x1111111111111111111111111111111111111111";
    const ACCOUNT_B: &str = "0x2222222222222222222222222222222222222222";

    #[test]
    fn validation_normalizes_quote_and_preserves_public_identifiers() {
        let request = validate_request(
            latest_as_of(),
            vec![account("eth-mainnet", ACCOUNT_A)],
            " mxn ".to_string(),
            tokens(["ethereum"]),
        )
        .unwrap();

        assert_eq!(request.quote_currency, "MXN");
        assert_eq!(request.accounts[0].network_slug, "eth-mainnet");
        assert_eq!(request.accounts[0].address, ACCOUNT_A);
        assert_eq!(request.asset_slugs, ["ethereum"]);
    }

    #[test]
    fn validation_allows_same_address_on_different_networks() {
        let request = validate_request(
            latest_as_of(),
            vec![
                account("eth-mainnet", ACCOUNT_A),
                account("base-mainnet", ACCOUNT_A),
            ],
            "USD".to_string(),
            tokens(["usdc"]),
        )
        .unwrap();

        assert_eq!(request.accounts.len(), 2);
    }

    #[test]
    fn validation_rejects_duplicate_accounts_case_insensitively() {
        let error = validate_request(
            latest_as_of(),
            vec![
                account("eth-mainnet", "0xabcdefabcdefabcdefabcdefabcdefabcdefabcd"),
                account("eth-mainnet", "0xABCDEFABCDEFABCDEFABCDEFABCDEFABCDEFABCD"),
            ],
            "USD".to_string(),
            tokens(["usdc"]),
        )
        .unwrap_err();

        assert_eq!(
            error.into_response().status(),
            axum::http::StatusCode::BAD_REQUEST
        );
    }

    #[test]
    fn validation_accepts_exact_public_limit_boundaries() {
        let accounts = (0..MAX_ACCOUNTS)
            .map(|index| account("eth-mainnet", &format!("0x{index:040x}")))
            .collect();
        let asset_slugs = (0..MAX_ASSETS)
            .map(|index| format!("asset-{index}"))
            .collect::<Vec<_>>();

        let request = validate_request(
            latest_as_of(),
            accounts,
            "USD".to_string(),
            token_vec(asset_slugs),
        )
        .unwrap();

        assert_eq!(request.accounts.len(), MAX_ACCOUNTS);
        assert_eq!(request.asset_slugs.len(), MAX_ASSETS);
    }

    #[tokio::test]
    async fn single_route_returns_complete_snapshot() {
        let Some((bigwig_url, bigwig_handle)) = spawn_server(bigwig_success(
            "eth-mainnet",
            1,
            json!({"kind": "native"}),
            &[(
                "0xABCDEFabcdefABCDEFabcdefABCDEFabcdefABCD",
                "1000000000000000000",
            )],
        )) else {
            return;
        };
        let Some((price_url, price_handle)) =
            spawn_server(price_success("ethereum", "MXN", "35000.50"))
        else {
            return;
        };
        let app = balance_app(Some(&bigwig_url), Some(&price_url));
        let body = json!({
            "as_of": {"kind": "latest"},
            "account": {
                "network_slug": "eth-mainnet",
                "address": "0xABCDEFabcdefABCDEFabcdefABCDEFabcdefABCD",
                "client_ref": "primary"
            },
            "quote_currency": " mxn ",
            "tokens": {"asset_slugs": ["ethereum"], "contract_addresses": []}
        });

        let (status, response) = post_json(app, "/v1/balances", body).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(response["type"], "balances");
        assert_eq!(response["status"], "complete");
        assert_eq!(response["quote_currency"], "MXN");
        assert_eq!(
            response["account"]["address"],
            "0xABCDEFabcdefABCDEFabcdefABCDEFabcdefABCD"
        );
        assert_eq!(response["account"]["client_ref"], "primary");
        assert_eq!(response["evidence"]["network_slug"], "eth-mainnet");
        assert_eq!(response["positions"][0]["asset_slug"], "ethereum");
        assert_eq!(
            response["positions"][0]["balance"]["amount"],
            "1.000000000000000000"
        );
        assert_eq!(
            response["positions"][0]["quote"]["value"],
            "35000.500000000000000000"
        );
        assert!(response["account"].get("chain").is_none());
        assert!(response["account"].get("chain_id").is_none());
        assert!(response["account"].get("chain_slug").is_none());
        assert!(response["evidence"].get("chain_id").is_none());

        let bigwig_request = bigwig_handle.await.unwrap();
        assert!(
            bigwig_request.starts_with("POST /internal/v1/primitives/evm/latest-balances HTTP/1.1")
        );
        let bigwig_json = request_body_json(&bigwig_request);
        assert_eq!(
            bigwig_json,
            json!({
                "network_slug": "eth-mainnet",
                "accounts": [{
                    "address": "0xABCDEFabcdefABCDEFabcdefABCDEFabcdefABCD"
                }],
                "targets": [{"kind": "native"}]
            })
        );
        assert!(!bigwig_request.contains("client_ref"));
        assert!(!bigwig_request.contains("quote_currency"));

        let price_request = price_handle.await.unwrap();
        assert!(price_request.starts_with("POST /prices/latest/batch HTTP/1.1"));
        assert_eq!(
            request_body_json(&price_request),
            json!({"slugs": ["ethereum"], "quoteCurrency": "MXN"})
        );
    }

    #[tokio::test]
    async fn bulk_route_returns_complete_ordered_snapshot() {
        let Some((bigwig_url, bigwig_handle)) = spawn_server(bigwig_success(
            "eth-mainnet",
            1,
            json!({
                "kind": "erc20",
                "contract_address": "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
            }),
            &[(ACCOUNT_A, "1000000"), (ACCOUNT_B, "2000000")],
        )) else {
            return;
        };
        let Some((price_url, price_handle)) = spawn_server(price_success("usdc", "USD", "1.00"))
        else {
            return;
        };
        let app = balance_app(Some(&bigwig_url), Some(&price_url));

        let (status, response) = post_json(
            app,
            "/v1/balances/bulk",
            json!({
                "as_of": {"kind": "latest"},
                "accounts": [
                    {"network_slug": "eth-mainnet", "address": ACCOUNT_A},
                    {"network_slug": "eth-mainnet", "address": ACCOUNT_B}
                ],
                "quote_currency": "USD",
                "tokens": {"asset_slugs": ["usdc"], "contract_addresses": []}
            }),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(response["type"], "balances_bulk");
        assert_eq!(response["status"], "complete");
        assert_eq!(response["summary"]["requested_accounts"], 2);
        assert_eq!(response["summary"]["requested_resolution_items"], 2);
        assert_eq!(response["summary"]["positions_returned"], 2);
        assert_eq!(response["accounts"][0]["account"]["address"], ACCOUNT_A);
        assert_eq!(response["accounts"][1]["account"]["address"], ACCOUNT_B);
        assert_eq!(
            response["accounts"][0]["account"]["network_slug"],
            "eth-mainnet"
        );
        assert!(response["accounts"][0]["account"].get("chain").is_none());
        assert!(response["accounts"][0]["evidence"]
            .get("chain_id")
            .is_none());
        assert_eq!(
            response["accounts"][0]["positions"][0]["balance"]["amount"],
            "1.000000"
        );
        assert_eq!(
            response["accounts"][1]["positions"][0]["balance"]["amount"],
            "2.000000"
        );

        bigwig_handle.await.unwrap();
        price_handle.await.unwrap();
    }

    #[tokio::test]
    async fn body_extraction_failures_use_invalid_request_envelope() {
        let app = balance_app(None, None);
        let requests = [
            (
                Some("application/json"),
                br#"{"as_of":{"kind":"latest"}"#.as_slice(),
            ),
            (
                Some("application/json"),
                br#"{"as_of":{"kind":"latest"},"account":{},"quote_currency":"USD","tokens":{"asset_slugs":["ethereum"],"contract_addresses":[]}}"#
                    .as_slice(),
            ),
            (
                Some("application/json"),
                br#"{"as_of":{"kind":"latest"},"account":[],"quote_currency":"USD","tokens":{"asset_slugs":["ethereum"],"contract_addresses":[]}}"#
                    .as_slice(),
            ),
            (Some("application/json"), br#"[]"#.as_slice()),
            (
                None,
                br#"{"as_of":{"kind":"latest"},"account":{"network_slug":"eth-mainnet","address":"0x1111111111111111111111111111111111111111"},"quote_currency":"USD","tokens":{"asset_slugs":["ethereum"],"contract_addresses":[]}}"#
                    .as_slice(),
            ),
            (
                Some("text/plain"),
                br#"{"as_of":{"kind":"latest"},"account":{"network_slug":"eth-mainnet","address":"0x1111111111111111111111111111111111111111"},"quote_currency":"USD","tokens":{"asset_slugs":["ethereum"],"contract_addresses":[]}}"#
                    .as_slice(),
            ),
        ];

        for (content_type, body) in requests {
            let (status, response) =
                post_raw(app.clone(), "/v1/balances", content_type, body.to_vec()).await;
            assert_public_error(
                status,
                &response,
                StatusCode::BAD_REQUEST,
                "invalid_request",
            );
        }
    }

    #[tokio::test]
    async fn balance_routes_reject_unknown_fields_with_unknown_field() {
        let app = balance_app(None, None);
        let single_cases = [
            {
                let mut body = single_body("eth-mainnet", ACCOUNT_A, "ethereum");
                body["future"] = json!(true);
                body
            },
            {
                let mut body = single_body("eth-mainnet", ACCOUNT_A, "ethereum");
                body["as_of"]["observed_at"] = json!("2026-06-18T12:00:00Z");
                body
            },
            {
                let mut body = single_body("eth-mainnet", ACCOUNT_A, "ethereum");
                body["account"]["label"] = json!("primary");
                body
            },
            {
                let mut body = single_body("eth-mainnet", ACCOUNT_A, "ethereum");
                body["tokens"]["symbol"] = json!("ETH");
                body
            },
            {
                let mut body = single_body("eth-mainnet", ACCOUNT_A, "ethereum");
                body["assets"] = json!([{"asset_slug": "ethereum"}]);
                body
            },
        ];

        for body in single_cases {
            let (status, response) = post_json(app.clone(), "/v1/balances", body).await;
            assert_public_error(status, &response, StatusCode::BAD_REQUEST, "unknown_field");
        }

        let bulk_cases = [
            {
                let mut body = bulk_body("eth-mainnet", ACCOUNT_A, "ethereum");
                body["future"] = json!(true);
                body
            },
            {
                let mut body = bulk_body("eth-mainnet", ACCOUNT_A, "ethereum");
                body["as_of"]["observed_at"] = json!("2026-06-18T12:00:00Z");
                body
            },
            {
                let mut body = bulk_body("eth-mainnet", ACCOUNT_A, "ethereum");
                body["accounts"][0]["label"] = json!("primary");
                body
            },
            {
                let mut body = bulk_body("eth-mainnet", ACCOUNT_A, "ethereum");
                body["tokens"]["symbol"] = json!("ETH");
                body
            },
            {
                let mut body = bulk_body("eth-mainnet", ACCOUNT_A, "ethereum");
                body["assets"] = json!([{"asset_slug": "ethereum"}]);
                body
            },
        ];

        for body in bulk_cases {
            let (status, response) = post_json(app.clone(), "/v1/balances/bulk", body).await;
            assert_public_error(status, &response, StatusCode::BAD_REQUEST, "unknown_field");
        }
    }

    #[tokio::test]
    async fn balance_routes_reject_reserved_network_alias_fields() {
        let app = balance_app(None, None);
        let forbidden_single_account_fields = [
            ("chain", json!("eth-mainnet")),
            ("chain_id", json!(1)),
            ("chain_slug", json!("eth-mainnet")),
        ];

        for (field, value) in forbidden_single_account_fields {
            let mut body = single_body("eth-mainnet", ACCOUNT_A, "ethereum");
            body["account"][field] = value;

            let (status, response) = post_json(app.clone(), "/v1/balances", body).await;
            assert_public_error(
                status,
                &response,
                StatusCode::BAD_REQUEST,
                "invalid_request",
            );
        }

        for field in ["chain", "chain_id", "chain_slug"] {
            let mut body = single_body("eth-mainnet", ACCOUNT_A, "ethereum");
            body[field] = json!("eth-mainnet");

            let (status, response) = post_json(app.clone(), "/v1/balances", body).await;
            assert_public_error(
                status,
                &response,
                StatusCode::BAD_REQUEST,
                "invalid_request",
            );
        }

        for field in ["chain", "chain_id", "chain_slug"] {
            let mut body = single_body("eth-mainnet", ACCOUNT_A, "ethereum");
            body["as_of"][field] = json!("eth-mainnet");

            let (status, response) = post_json(app.clone(), "/v1/balances", body).await;
            assert_public_error(
                status,
                &response,
                StatusCode::BAD_REQUEST,
                "invalid_request",
            );
        }

        for field in ["chain", "chain_id", "chain_slug"] {
            let mut body = single_body("eth-mainnet", ACCOUNT_A, "ethereum");
            body["tokens"][field] = json!("eth-mainnet");

            let (status, response) = post_json(app.clone(), "/v1/balances", body).await;
            assert_public_error(
                status,
                &response,
                StatusCode::BAD_REQUEST,
                "invalid_request",
            );
        }

        let mut both_names = single_body("eth-mainnet", ACCOUNT_A, "ethereum");
        both_names["account"]["chain"] = json!("eth-mainnet");
        both_names["future"] = json!(true);
        let (status, response) = post_json(app.clone(), "/v1/balances", both_names).await;
        assert_public_error(
            status,
            &response,
            StatusCode::BAD_REQUEST,
            "invalid_request",
        );

        for field in ["chain", "chain_id", "chain_slug"] {
            let mut body = json!({
                "as_of": {"kind": "latest"},
                "accounts": [
                    {"network_slug": "eth-mainnet", "address": ACCOUNT_A}
                ],
                "quote_currency": "USD",
                "tokens": {"asset_slugs": ["ethereum"], "contract_addresses": []}
            });
            body["accounts"][0][field] = json!("eth-mainnet");

            let (status, response) = post_json(app.clone(), "/v1/balances/bulk", body).await;
            assert_public_error(
                status,
                &response,
                StatusCode::BAD_REQUEST,
                "invalid_request",
            );
        }
    }

    #[tokio::test]
    async fn semantic_validation_codes_are_stable_and_ordered() {
        let app = balance_app(None, None);
        let valid_account = json!({"network_slug": "eth-mainnet", "address": ACCOUNT_A});
        let valid_tokens = json!({"asset_slugs": ["ethereum"], "contract_addresses": []});
        let cases = [
            (
                json!({
                    "as_of": {"kind": "historical"},
                    "accounts": [],
                    "quote_currency": "NOPE",
                    "tokens": valid_tokens.clone()
                }),
                "unsupported_as_of",
            ),
            (
                json!({
                    "as_of": {
                        "kind": "timestamp",
                        "timestamp": "2026-07-03T00:00:00Z"
                    },
                    "accounts": [valid_account.clone()],
                    "quote_currency": "USD",
                    "tokens": valid_tokens.clone()
                }),
                "unsupported_as_of",
            ),
            (
                json!({
                    "as_of": {
                        "kind": "block_number",
                        "block_number": "19000000"
                    },
                    "accounts": [valid_account.clone()],
                    "quote_currency": "USD",
                    "tokens": valid_tokens.clone()
                }),
                "unsupported_as_of",
            ),
            (
                json!({
                    "as_of": {"kind": "latest"},
                    "accounts": [],
                    "quote_currency": "USD",
                    "tokens": valid_tokens.clone()
                }),
                "empty_accounts",
            ),
            (
                json!({
                    "as_of": {"kind": "latest"},
                    "accounts": [valid_account.clone()],
                    "quote_currency": "USD",
                    "tokens": {"asset_slugs": [], "contract_addresses": []}
                }),
                "empty_tokens",
            ),
            (
                json!({
                    "as_of": {"kind": "latest"},
                    "accounts": [valid_account.clone()],
                    "quote_currency": "USD"
                }),
                "empty_tokens",
            ),
            (
                json!({
                    "as_of": {"kind": "latest"},
                    "accounts": [valid_account.clone()],
                    "quote_currency": "USD",
                    "tokens": {
                        "asset_slugs": [],
                        "contract_addresses": ["0x1234"]
                    }
                }),
                "invalid_contract_address",
            ),
            (
                json!({
                    "as_of": {"kind": "latest"},
                    "accounts": [valid_account.clone()],
                    "quote_currency": "USD",
                    "tokens": {
                        "asset_slugs": [],
                        "contract_addresses": ["0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"]
                    }
                }),
                "unsupported_token_selector",
            ),
            (
                json!({
                    "as_of": {"kind": "latest"},
                    "accounts": [valid_account.clone()],
                    "quote_currency": "EUR",
                    "tokens": valid_tokens.clone()
                }),
                "unsupported_quote_currency",
            ),
            (
                json!({
                    "as_of": {"kind": "latest"},
                    "accounts": [{
                        "network_slug": "eth-mainnet",
                        "address": "0x1234"
                    }],
                    "quote_currency": "USD",
                    "tokens": valid_tokens.clone()
                }),
                "invalid_account",
            ),
            (
                json!({
                    "as_of": {"kind": "latest"},
                    "accounts": [
                        {
                            "network_slug": "eth-mainnet",
                            "address": "0xabcdefabcdefabcdefabcdefabcdefabcdefabcd"
                        },
                        {
                            "network_slug": "eth-mainnet",
                            "address": "0xABCDEFABCDEFABCDEFABCDEFABCDEFABCDEFABCD"
                        }
                    ],
                    "quote_currency": "USD",
                    "tokens": valid_tokens.clone()
                }),
                "duplicate_account",
            ),
            (
                json!({
                    "as_of": {"kind": "latest"},
                    "accounts": [valid_account],
                    "quote_currency": "USD",
                    "tokens": {
                        "asset_slugs": ["ethereum", "ethereum"],
                        "contract_addresses": []
                    }
                }),
                "duplicate_asset",
            ),
        ];

        for (body, expected_code) in cases {
            let (status, response) = post_json(app.clone(), "/v1/balances/bulk", body).await;
            assert_public_error(status, &response, StatusCode::BAD_REQUEST, expected_code);
        }
    }

    #[tokio::test]
    async fn public_limits_reject_only_values_above_the_boundary() {
        let app = balance_app(None, None);
        let too_many_accounts = (0..=MAX_ACCOUNTS)
            .map(|index| {
                json!({
                    "network_slug": "eth-mainnet",
                    "address": format!("0x{index:040x}")
                })
            })
            .collect::<Vec<_>>();
        let too_many_assets = (0..=MAX_ASSETS)
            .map(|index| format!("asset-{index}"))
            .collect::<Vec<_>>();

        for body in [
            json!({
                "as_of": {"kind": "latest"},
                "accounts": too_many_accounts,
                "quote_currency": "USD",
                "tokens": {"asset_slugs": ["ethereum"], "contract_addresses": []}
            }),
            json!({
                "as_of": {"kind": "latest"},
                "accounts": [{
                    "network_slug": "eth-mainnet",
                    "address": ACCOUNT_A
                }],
                "quote_currency": "USD",
                "tokens": {"asset_slugs": too_many_assets, "contract_addresses": []}
            }),
        ] {
            let (status, response) = post_json(app.clone(), "/v1/balances/bulk", body).await;
            assert_public_error(
                status,
                &response,
                StatusCode::BAD_REQUEST,
                "request_too_large",
            );
        }
    }

    #[tokio::test]
    async fn canonical_identifiers_are_strict_and_catalog_admitted() {
        let app = balance_app(None, None);

        for network_slug in [
            "base",
            "mantle",
            "arbitrum-one",
            "bitcoin-mainnet",
            "unknown-mainnet",
            "eth-mainnet ",
            "ETH-MAINNET",
        ] {
            let (status, response) = post_json(
                app.clone(),
                "/v1/balances",
                single_body(network_slug, ACCOUNT_A, "ethereum"),
            )
            .await;
            assert_public_error(
                status,
                &response,
                StatusCode::BAD_REQUEST,
                "unsupported_network",
            );
        }

        for asset_slug in ["ETHEREUM", " ethereum "] {
            let (status, response) = post_json(
                app.clone(),
                "/v1/balances",
                single_body("eth-mainnet", ACCOUNT_A, asset_slug),
            )
            .await;
            assert_public_error(
                status,
                &response,
                StatusCode::BAD_REQUEST,
                "invalid_asset_slug",
            );
        }

        for asset_slug in ["missing-asset"] {
            let (status, response) = post_json(
                app.clone(),
                "/v1/balances",
                single_body("eth-mainnet", ACCOUNT_A, asset_slug),
            )
            .await;
            assert_public_error(
                status,
                &response,
                StatusCode::BAD_REQUEST,
                "unsupported_asset",
            );
        }
    }

    #[tokio::test]
    async fn unsupported_pairs_are_skipped_without_provider_clients() {
        let (status, response) = post_json(
            balance_app(None, None),
            "/v1/balances",
            single_body("base-mainnet", ACCOUNT_A, "mantle"),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(response["status"], "complete");
        assert_eq!(response["evidence"], Value::Null);
        assert_eq!(response["positions"], json!([]));
        assert_eq!(response["errors"], json!([]));
        assert_eq!(
            response["skipped"],
            json!([{
                "network_slug": "base-mainnet",
                "asset_slug": "mantle",
                "reason": "asset_not_supported_on_network"
            }])
        );
    }

    #[tokio::test]
    async fn provider_unavailability_remains_a_sanitized_item_level_200() {
        let address = "0xABCDEFabcdefABCDEFabcdefABCDEFabcdefABCD";
        let (status, response) = post_json(
            balance_app(None, None),
            "/v1/balances",
            single_body("eth-mainnet", address, "ethereum"),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(response["status"], "failed");
        assert_eq!(response["account"]["address"], address);
        assert_eq!(response["evidence"], Value::Null);
        assert_eq!(
            response["errors"][0]["code"],
            "balance_provider_unavailable"
        );
        assert!(response.to_string().find("Bigwig").is_none());
    }

    #[tokio::test]
    async fn same_address_on_different_networks_is_not_a_duplicate() {
        let (status, response) = post_json(
            balance_app(None, None),
            "/v1/balances/bulk",
            json!({
                "as_of": {"kind": "latest"},
                "accounts": [
                    {"network_slug": "eth-mainnet", "address": ACCOUNT_A},
                    {"network_slug": "base-mainnet", "address": ACCOUNT_A}
                ],
                "quote_currency": "USD",
                "tokens": {"asset_slugs": ["usdc"], "contract_addresses": []}
            }),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(response["accounts"].as_array().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn missing_catalog_configuration_returns_service_unavailable() {
        let app = build_router(AppState::new(Config::default()));
        let (status, response) = post_json(
            app,
            "/v1/balances",
            single_body("eth-mainnet", ACCOUNT_A, "ethereum"),
        )
        .await;

        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(response["error"]["code"], "asset_network_map_unavailable");
    }

    #[tokio::test]
    async fn repository_and_internal_failures_have_request_wide_mappings() {
        let repository_error =
            balance_service_error_to_api_error(BalanceSnapshotServiceError::Catalog(
                CatalogResolverError::Repository(RepositoryError::test()),
            ))
            .into_response();
        assert_eq!(repository_error.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            response_error_code(repository_error).await,
            "asset_network_map_unavailable"
        );

        for error in [
            BalanceSnapshotServiceError::InvalidPlan {
                network_slug: "eth-mainnet".to_string(),
                issue: BalancePlanIssue::ResolutionCountMismatch,
            },
            BalanceSnapshotServiceError::ExecutionTaskFailed,
        ] {
            let response = balance_service_error_to_api_error(error).into_response();
            assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
            assert_eq!(response_error_code(response).await, "internal_error");
        }

        let response = balance_assembler_error_to_api_error(
            BalanceResponseAssemblerError::ExpectedSingleAccount,
        )
        .into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(response_error_code(response).await, "internal_error");
    }

    fn account(network_slug: &str, address: &str) -> BalanceAccountRequest {
        BalanceAccountRequest {
            network_slug: network_slug.to_string(),
            address: address.to_string(),
            client_ref: None,
        }
    }

    fn latest_as_of() -> BalanceAsOfRequest {
        BalanceAsOfRequest {
            kind: "latest".to_string(),
            timestamp: None,
            block_number: None,
        }
    }

    fn tokens<const N: usize>(asset_slugs: [&str; N]) -> BalanceTokenSelectorRequest {
        token_vec(asset_slugs.into_iter().map(str::to_string).collect())
    }

    fn token_vec(asset_slugs: Vec<String>) -> BalanceTokenSelectorRequest {
        BalanceTokenSelectorRequest {
            asset_slugs,
            contract_addresses: Vec::new(),
        }
    }

    fn balance_app(bigwig_url: Option<&str>, price_url: Option<&str>) -> Router {
        let bigwig_client =
            bigwig_url.map(|url| BigwigClient::new(url, "test-bigwig-token", 2_000).unwrap());
        let price_indexer_client =
            price_url.map(|url| PriceIndexerClient::new(url, "test-price-token", 2_000).unwrap());

        build_router(AppState {
            config: Config::default(),
            version: env!("CARGO_PKG_VERSION"),
            database_pool: None,
            api_key_repository: None,
            api_key_minute_limiter: crate::adapters::http::rate_limit::ApiKeyMinuteLimiter::default(
            ),
            asset_repository: Some(GlobalAssetRepository::in_memory(sample_assets())),
            price_indexer_client,
            dis_client: None,
            bigwig_client,
        })
    }

    fn single_body(network_slug: &str, address: &str, asset_slug: &str) -> Value {
        json!({
            "as_of": {"kind": "latest"},
            "account": {
                "network_slug": network_slug,
                "address": address
            },
            "quote_currency": "USD",
            "tokens": {
                "asset_slugs": [asset_slug],
                "contract_addresses": []
            }
        })
    }

    fn bulk_body(network_slug: &str, address: &str, asset_slug: &str) -> Value {
        json!({
            "as_of": {"kind": "latest"},
            "accounts": [{
                "network_slug": network_slug,
                "address": address
            }],
            "quote_currency": "USD",
            "tokens": {
                "asset_slugs": [asset_slug],
                "contract_addresses": []
            }
        })
    }

    async fn post_json(app: Router, uri: &str, body: Value) -> (StatusCode, Value) {
        post_raw(
            app,
            uri,
            Some("application/json"),
            serde_json::to_vec(&body).unwrap(),
        )
        .await
    }

    async fn response_error_code(response: axum::response::Response) -> String {
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();
        json["error"]["code"].as_str().unwrap().to_string()
    }

    fn bigwig_success(
        network_slug: &str,
        chain_id: i64,
        target: Value,
        accounts: &[(&str, &str)],
    ) -> Value {
        let items = accounts
            .iter()
            .map(|(address, raw_amount)| {
                json!({
                    "status": "resolved",
                    "account": {"address": address},
                    "target": target.clone(),
                    "raw_amount": raw_amount
                })
            })
            .collect::<Vec<_>>();

        json!({
            "primitive": "evm_latest_balances",
            "status": "complete",
            "network": {
                "network_slug": network_slug,
                "chain_id": chain_id
            },
            "observed_at": "2026-06-18T12:00:00Z",
            "block": {
                "number": "123456",
                "hash": format!("0x{}", "a".repeat(64))
            },
            "items": items
        })
    }

    fn price_success(asset_slug: &str, quote_currency: &str, price: &str) -> Value {
        json!({
            "quoteCurrency": quote_currency,
            "requestedCount": 1,
            "uniqueCount": 1,
            "results": [{
                "requestedSlug": asset_slug,
                "normalizedSlug": asset_slug,
                "assetId": asset_slug,
                "slug": asset_slug,
                "name": asset_slug,
                "status": "found",
                "freshnessStatus": "fresh",
                "price": {
                    "assetId": asset_slug,
                    "slug": asset_slug,
                    "quoteCurrency": quote_currency,
                    "price": price,
                    "sourceType": "test",
                    "publishedAt": "2026-06-18T11:59:59Z",
                    "recordedAt": "2026-06-18T11:59:59Z",
                    "freshnessStatus": "fresh",
                    "confidenceLabel": "high",
                    "isFallback": false,
                    "isDerived": false,
                    "staleness": {
                        "ageSeconds": 1,
                        "isStale": false,
                        "warningThresholdSeconds": 300
                    }
                },
                "error": null
            }]
        })
    }

    fn spawn_server(body: Value) -> Option<(String, tokio::task::JoinHandle<String>)> {
        let listener = match TcpListener::bind("127.0.0.1:0") {
            Ok(listener) => listener,
            Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return None,
            Err(error) => panic!("failed to bind balance test server: {error}"),
        };
        let url = format!("http://{}", listener.local_addr().unwrap());
        let handle = tokio::task::spawn_blocking(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_http_request(&mut stream);
            let body = body.to_string();
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
            request
        });

        Some((url, handle))
    }

    fn read_http_request(stream: &mut TcpStream) -> String {
        let mut bytes = Vec::new();
        let mut buffer = [0u8; 4096];
        let header_end = loop {
            let read = stream.read(&mut buffer).unwrap();
            if read == 0 {
                panic!("connection closed before request was complete");
            }
            bytes.extend_from_slice(&buffer[..read]);
            if let Some(index) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
                break index + 4;
            }
        };
        let headers = String::from_utf8_lossy(&bytes[..header_end]);
        let content_length = headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().unwrap())
            })
            .unwrap_or(0);
        while bytes.len() < header_end + content_length {
            let read = stream.read(&mut buffer).unwrap();
            if read == 0 {
                break;
            }
            bytes.extend_from_slice(&buffer[..read]);
        }

        String::from_utf8(bytes).unwrap()
    }

    fn request_body_json(request: &str) -> Value {
        let (_, body) = request.split_once("\r\n\r\n").unwrap();
        serde_json::from_str(body).unwrap()
    }
}
