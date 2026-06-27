use std::collections::{HashMap, HashSet};

use axum::{
    extract::{rejection::JsonRejection, State},
    Json,
};
use serde::{de::IgnoredAny, Deserialize};
use tracing::warn;

use crate::{
    adapters::http::error::ApiError,
    application::balances::{
        catalog::{CatalogBalanceTargetResolver, CatalogResolverError},
        quote::PriceQuoteClient,
        response::{
            BalanceResponseAssembler, BalanceResponseAssemblerError, BulkBalanceResponse,
            SingleBalanceResponse,
        },
        service::{
            BalanceSnapshotAccount, BalanceSnapshotRequest, BalanceSnapshotService,
            BalanceSnapshotServiceError,
        },
    },
    state::AppState,
};

const MAX_ACCOUNTS: usize = 50;
const MAX_ASSETS: usize = 20;
const MAX_RESOLUTION_ITEMS: usize = 1_000;
const RESERVED_NETWORK_ALIAS_FIELDS: [&str; 3] = ["chain", "chain_id", "chain_slug"];

type ExtraFields = HashMap<String, IgnoredAny>;

#[derive(Debug, Deserialize)]
pub struct SingleBalanceRequest {
    as_of: BalanceAsOfRequest,
    account: BalanceAccountRequest,
    quote_currency: String,
    assets: Vec<BalanceAssetRequest>,
    #[serde(default, flatten)]
    extra: ExtraFields,
}

#[derive(Debug, Deserialize)]
pub struct BulkBalanceRequest {
    as_of: BalanceAsOfRequest,
    accounts: Vec<BalanceAccountRequest>,
    quote_currency: String,
    assets: Vec<BalanceAssetRequest>,
    #[serde(default, flatten)]
    extra: ExtraFields,
}

#[derive(Debug, Deserialize)]
struct BalanceAsOfRequest {
    kind: String,
}

#[derive(Debug, Deserialize)]
struct BalanceAccountRequest {
    network_slug: String,
    address: String,
    client_ref: Option<String>,
    #[serde(default, flatten)]
    extra: ExtraFields,
}

#[derive(Clone, Debug, Deserialize)]
struct BalanceAssetRequest {
    asset_slug: String,
}

pub async fn resolve_single_balance(
    State(state): State<AppState>,
    body: Result<Json<SingleBalanceRequest>, JsonRejection>,
) -> Result<Json<SingleBalanceResponse>, ApiError> {
    let Json(request) = body.map_err(|_| ApiError::invalid_request())?;
    reject_reserved_request_fields(&request.extra)?;
    reject_reserved_account_fields(&request.account)?;
    let request = validate_request(
        request.as_of,
        vec![request.account],
        request.quote_currency,
        request.assets,
    )?;
    let snapshot = resolve_snapshot(&state, request).await?;
    let response = BalanceResponseAssembler
        .single(snapshot)
        .map_err(balance_assembler_error_to_api_error)?;

    Ok(Json(response))
}

pub async fn resolve_bulk_balances(
    State(state): State<AppState>,
    body: Result<Json<BulkBalanceRequest>, JsonRejection>,
) -> Result<Json<BulkBalanceResponse>, ApiError> {
    let Json(request) = body.map_err(|_| ApiError::invalid_request())?;
    reject_reserved_request_fields(&request.extra)?;
    for account in &request.accounts {
        reject_reserved_account_fields(account)?;
    }
    let request = validate_request(
        request.as_of,
        request.accounts,
        request.quote_currency,
        request.assets,
    )?;
    let snapshot = resolve_snapshot(&state, request).await?;

    Ok(Json(BalanceResponseAssembler.bulk(snapshot)))
}

fn reject_reserved_request_fields(extra: &ExtraFields) -> Result<(), ApiError> {
    reject_reserved_fields(extra)
}

fn reject_reserved_account_fields(account: &BalanceAccountRequest) -> Result<(), ApiError> {
    reject_reserved_fields(&account.extra)
}

fn reject_reserved_fields(extra: &ExtraFields) -> Result<(), ApiError> {
    if RESERVED_NETWORK_ALIAS_FIELDS
        .iter()
        .any(|field| extra.contains_key(*field))
    {
        return Err(ApiError::invalid_request());
    }

    Ok(())
}

fn validate_request(
    as_of: BalanceAsOfRequest,
    accounts: Vec<BalanceAccountRequest>,
    quote_currency: String,
    assets: Vec<BalanceAssetRequest>,
) -> Result<BalanceSnapshotRequest, ApiError> {
    if as_of.kind != "latest" {
        return Err(ApiError::unsupported_as_of());
    }
    if accounts.is_empty() {
        return Err(ApiError::empty_accounts());
    }
    if assets.is_empty() {
        return Err(ApiError::empty_assets());
    }

    let resolution_items = accounts
        .len()
        .checked_mul(assets.len())
        .ok_or_else(ApiError::request_too_large)?;
    if accounts.len() > MAX_ACCOUNTS
        || assets.len() > MAX_ASSETS
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

    let mut seen_assets = HashSet::<String>::with_capacity(assets.len());
    for asset in &assets {
        if !seen_assets.insert(asset.asset_slug.clone()) {
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
        asset_slugs: assets.into_iter().map(|asset| asset.asset_slug).collect(),
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
        state.bigwig_latest_balances_client.clone(),
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

    use axum::{
        body::Body,
        http::{Request, StatusCode},
        response::IntoResponse,
        Router,
    };
    use serde_json::{json, Value};
    use tower::ServiceExt;

    use crate::adapters::postgres::errors::RepositoryError;
    use crate::test_utils::global_assets::asset_fixtures;
    use crate::{
        adapters::bigwig::balances::BigwigLatestBalancesClient,
        adapters::postgres::global_assets::GlobalAssetRepository,
        adapters::price_indexer::PriceIndexerClient, app::create_app,
        application::balances::service::BalancePlanIssue, config::Config,
    };

    use super::*;

    const ACCOUNT_A: &str = "0x1111111111111111111111111111111111111111";
    const ACCOUNT_B: &str = "0x2222222222222222222222222222222222222222";

    #[test]
    fn validation_normalizes_quote_and_preserves_public_identifiers() {
        let request = validate_request(
            BalanceAsOfRequest {
                kind: "latest".to_string(),
            },
            vec![account("eth-mainnet", ACCOUNT_A)],
            " mxn ".to_string(),
            vec![asset("ethereum")],
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
            BalanceAsOfRequest {
                kind: "latest".to_string(),
            },
            vec![
                account("eth-mainnet", ACCOUNT_A),
                account("base-mainnet", ACCOUNT_A),
            ],
            "USD".to_string(),
            vec![asset("usdc")],
        )
        .unwrap();

        assert_eq!(request.accounts.len(), 2);
    }

    #[test]
    fn validation_rejects_duplicate_accounts_case_insensitively() {
        let error = validate_request(
            BalanceAsOfRequest {
                kind: "latest".to_string(),
            },
            vec![
                account("eth-mainnet", "0xabcdefabcdefabcdefabcdefabcdefabcdefabcd"),
                account("eth-mainnet", "0xABCDEFABCDEFABCDEFABCDEFABCDEFABCDEFABCD"),
            ],
            "USD".to_string(),
            vec![asset("usdc")],
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
        let assets = (0..MAX_ASSETS)
            .map(|index| asset(&format!("asset-{index}")))
            .collect();

        let request = validate_request(
            BalanceAsOfRequest {
                kind: "latest".to_string(),
            },
            accounts,
            "USD".to_string(),
            assets,
        )
        .unwrap();

        assert_eq!(request.accounts.len(), MAX_ACCOUNTS);
        assert_eq!(request.asset_slugs.len(), MAX_ASSETS);
    }

    #[tokio::test]
    async fn single_route_returns_complete_snapshot_and_ignores_unknown_fields() {
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
            "as_of": {"kind": "latest", "future": true},
            "account": {
                "network_slug": "eth-mainnet",
                "address": "0xABCDEFabcdefABCDEFabcdefABCDEFabcdefABCD",
                "client_ref": "primary",
                "label": "ignored"
            },
            "quote_currency": " mxn ",
            "assets": [{"asset_slug": "ethereum", "symbol": "ETH"}],
            "future": {"ignored": true}
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
                "assets": [{"asset_slug": "usdc"}]
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
                br#"{"as_of":{"kind":"latest"},"account":{},"quote_currency":"USD","assets":[]}"#
                    .as_slice(),
            ),
            (
                Some("application/json"),
                br#"{"as_of":{"kind":"latest"},"account":[],"quote_currency":"USD","assets":[]}"#
                    .as_slice(),
            ),
            (
                None,
                br#"{"as_of":{"kind":"latest"},"account":{"network_slug":"eth-mainnet","address":"0x1111111111111111111111111111111111111111"},"quote_currency":"USD","assets":[{"asset_slug":"ethereum"}]}"#
                    .as_slice(),
            ),
        ];

        for (content_type, body) in requests {
            let (status, response) =
                post_raw(app.clone(), "/v1/balances", content_type, body.to_vec()).await;
            assert_public_error(status, &response, "invalid_request");
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
            assert_public_error(status, &response, "invalid_request");
        }

        for field in ["chain", "chain_id", "chain_slug"] {
            let mut body = single_body("eth-mainnet", ACCOUNT_A, "ethereum");
            body[field] = json!("eth-mainnet");

            let (status, response) = post_json(app.clone(), "/v1/balances", body).await;
            assert_public_error(status, &response, "invalid_request");
        }

        let mut both_names = single_body("eth-mainnet", ACCOUNT_A, "ethereum");
        both_names["account"]["chain"] = json!("eth-mainnet");
        let (status, response) = post_json(app.clone(), "/v1/balances", both_names).await;
        assert_public_error(status, &response, "invalid_request");

        for field in ["chain", "chain_id", "chain_slug"] {
            let mut body = json!({
                "as_of": {"kind": "latest"},
                "accounts": [
                    {"network_slug": "eth-mainnet", "address": ACCOUNT_A}
                ],
                "quote_currency": "USD",
                "assets": [{"asset_slug": "ethereum"}]
            });
            body["accounts"][0][field] = json!("eth-mainnet");

            let (status, response) = post_json(app.clone(), "/v1/balances/bulk", body).await;
            assert_public_error(status, &response, "invalid_request");
        }
    }

    #[tokio::test]
    async fn semantic_validation_codes_are_stable_and_ordered() {
        let app = balance_app(None, None);
        let valid_account = json!({"network_slug": "eth-mainnet", "address": ACCOUNT_A});
        let valid_asset = json!({"asset_slug": "ethereum"});
        let cases = [
            (
                json!({
                    "as_of": {"kind": "historical"},
                    "accounts": [],
                    "quote_currency": "NOPE",
                    "assets": []
                }),
                "unsupported_as_of",
            ),
            (
                json!({
                    "as_of": {"kind": "latest"},
                    "accounts": [],
                    "quote_currency": "USD",
                    "assets": [valid_asset.clone()]
                }),
                "empty_accounts",
            ),
            (
                json!({
                    "as_of": {"kind": "latest"},
                    "accounts": [valid_account.clone()],
                    "quote_currency": "USD",
                    "assets": []
                }),
                "empty_assets",
            ),
            (
                json!({
                    "as_of": {"kind": "latest"},
                    "accounts": [valid_account.clone()],
                    "quote_currency": "EUR",
                    "assets": [valid_asset.clone()]
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
                    "assets": [valid_asset.clone()]
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
                    "assets": [valid_asset.clone()]
                }),
                "duplicate_account",
            ),
            (
                json!({
                    "as_of": {"kind": "latest"},
                    "accounts": [valid_account],
                    "quote_currency": "USD",
                    "assets": [valid_asset.clone(), valid_asset]
                }),
                "duplicate_asset",
            ),
        ];

        for (body, expected_code) in cases {
            let (status, response) = post_json(app.clone(), "/v1/balances/bulk", body).await;
            assert_public_error(status, &response, expected_code);
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
            .map(|index| json!({"asset_slug": format!("asset-{index}")}))
            .collect::<Vec<_>>();

        for body in [
            json!({
                "as_of": {"kind": "latest"},
                "accounts": too_many_accounts,
                "quote_currency": "USD",
                "assets": [{"asset_slug": "ethereum"}]
            }),
            json!({
                "as_of": {"kind": "latest"},
                "accounts": [{
                    "network_slug": "eth-mainnet",
                    "address": ACCOUNT_A
                }],
                "quote_currency": "USD",
                "assets": too_many_assets
            }),
        ] {
            let (status, response) = post_json(app.clone(), "/v1/balances/bulk", body).await;
            assert_public_error(status, &response, "request_too_large");
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
            assert_public_error(status, &response, "unsupported_network");
        }

        for asset_slug in ["ETHEREUM", " ethereum ", "missing-asset"] {
            let (status, response) = post_json(
                app.clone(),
                "/v1/balances",
                single_body("eth-mainnet", ACCOUNT_A, asset_slug),
            )
            .await;
            assert_public_error(status, &response, "unsupported_asset");
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
                "assets": [{"asset_slug": "usdc"}]
            }),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(response["accounts"].as_array().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn missing_catalog_configuration_returns_service_unavailable() {
        let app = create_app(AppState::new(Config::default()));
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
            extra: ExtraFields::default(),
        }
    }

    fn asset(asset_slug: &str) -> BalanceAssetRequest {
        BalanceAssetRequest {
            asset_slug: asset_slug.to_string(),
        }
    }

    fn balance_app(bigwig_url: Option<&str>, price_url: Option<&str>) -> Router {
        let bigwig_latest_balances_client = bigwig_url
            .map(|url| BigwigLatestBalancesClient::new(url, "test-bigwig-token", 2_000).unwrap());
        let price_indexer_client =
            price_url.map(|url| PriceIndexerClient::new(url, "test-price-token", 2_000).unwrap());

        create_app(AppState {
            config: Config::default(),
            version: env!("CARGO_PKG_VERSION"),
            database_pool: None,
            asset_repository: Some(GlobalAssetRepository::in_memory(asset_fixtures())),
            price_indexer_client,
            dis_client: None,
            bigwig_latest_balances_client,
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
            "assets": [{"asset_slug": asset_slug}]
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

    async fn post_raw(
        app: Router,
        uri: &str,
        content_type: Option<&str>,
        body: Vec<u8>,
    ) -> (StatusCode, Value) {
        let mut request = Request::builder().method("POST").uri(uri);
        if let Some(content_type) = content_type {
            request = request.header("content-type", content_type);
        }
        let response = app
            .oneshot(request.body(Body::from(body)).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json = serde_json::from_slice(&body).unwrap();

        (status, json)
    }

    fn assert_public_error(status: StatusCode, response: &Value, expected_code: &str) {
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(response["ok"], false);
        assert_eq!(response["error"]["code"], expected_code);
        assert!(response["error"]["message"]
            .as_str()
            .is_some_and(|message| !message.is_empty()));
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
