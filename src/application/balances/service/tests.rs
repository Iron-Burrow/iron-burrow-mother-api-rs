use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
};

use reqwest::StatusCode;
use serde_json::{json, Value};

use super::*;
use crate::{
    adapters::bigwig::balances::BigwigRequestValidationCode,
    test_utils::fixtures::global_assets::sample_assets,
};
use crate::{
    adapters::bigwig::balances::{
        BigwigEvidenceNetwork, BigwigItemError, BigwigItemErrorCode, BigwigResolvedEvidence,
        BigwigResolvedEvidenceKind,
    },
    adapters::postgres::global_assets::GlobalAssetRepository,
    adapters::price_indexer::PriceIndexerClient,
};

const ACCOUNT_A: &str = "0x1111111111111111111111111111111111111111";
const ACCOUNT_B: &str = "0x2222222222222222222222222222222222222222";
const ACCOUNT_C: &str = "0x3333333333333333333333333333333333333333";

#[tokio::test]
async fn groups_networks_concurrently_and_restores_caller_order() {
    let Some((base_url, server)) = spawn_dynamic_server(2) else {
        return;
    };
    let service = service(Some(bigwig_client(&base_url)));
    let request = GetBalancesCommand {
        as_of: AsOf::Latest,
        accounts: vec![
            account("base-mainnet", ACCOUNT_A, Some("base-a")),
            account("eth-mainnet", ACCOUNT_B, Some("eth-b")),
            account("base-mainnet", ACCOUNT_C, Some("base-c")),
        ],
        tokens: token_slugs(["usdc", "ethereum"]),
        quote_currency: "MXN".to_string(),
    };

    let result = service.resolve(request.clone()).await.unwrap();
    let requests = server.join().unwrap();
    let requests_by_network = requests
        .into_iter()
        .map(|request| {
            (
                request["network_slug"].as_str().unwrap().to_string(),
                request,
            )
        })
        .collect::<HashMap<_, _>>();

    assert_eq!(requests_by_network.len(), 2);
    assert_eq!(
        requests_by_network["base-mainnet"]["accounts"],
        json!([ACCOUNT_A, ACCOUNT_C])
    );
    assert_eq!(
        requests_by_network["eth-mainnet"]["accounts"],
        json!([ACCOUNT_B])
    );
    for network_slug in ["base-mainnet", "eth-mainnet"] {
        assert_eq!(
            requests_by_network[network_slug]["tokens"][0]["kind"],
            "erc20"
        );
        assert_eq!(
            requests_by_network[network_slug]["tokens"][1],
            json!({ "kind": "native" })
        );
        let serialized = requests_by_network[network_slug].to_string();
        for forbidden in [
            "asset_slug",
            "decimals",
            "symbol",
            "quote_currency",
            "client_ref",
            "route_id",
        ] {
            assert!(!serialized.contains(forbidden));
        }
    }

    assert_eq!(result.quote_currency, "MXN");
    assert_eq!(result.requested_token_count, request.tokens.len());
    assert_eq!(
        result
            .accounts
            .iter()
            .map(|result| result.account.clone())
            .collect::<Vec<_>>(),
        request.accounts
    );
    assert_eq!(
        result
            .accounts
            .iter()
            .map(|result| result.evidence.as_ref().unwrap().network_slug.as_str())
            .collect::<Vec<_>>(),
        vec!["base-mainnet", "eth-mainnet", "base-mainnet"]
    );
    assert!(result
        .accounts
        .iter()
        .all(|account| account.items.len() == 2
            && account
                .items
                .iter()
                .all(|item| matches!(item, BalanceItemOutcome::Resolved { .. }))));
}

#[tokio::test]
async fn batches_deduplicated_quotes_once_and_fans_them_out_in_caller_order() {
    let Some((bigwig_url, bigwig_server)) = spawn_dynamic_server(2) else {
        return;
    };
    let Some((price_url, price_server)) = spawn_price_server(&["usdc", "ethereum"], "MXN", "2.50")
    else {
        return;
    };
    let request = GetBalancesCommand {
        as_of: AsOf::Latest,
        accounts: vec![
            account("base-mainnet", ACCOUNT_A, Some("base")),
            account("eth-mainnet", ACCOUNT_B, Some("eth")),
        ],
        tokens: token_slugs(["usdc", "ethereum"]),
        quote_currency: "MXN".to_string(),
    };

    let result = service_with_quote(
        Some(bigwig_client(&bigwig_url)),
        Some(price_quote_client(&price_url)),
    )
    .resolve(request.clone())
    .await
    .unwrap();
    bigwig_server.join().unwrap();
    let price_request = price_server.join().unwrap();
    let price_body = price_request
        .split_once("\r\n\r\n")
        .map(|(_, body)| serde_json::from_str::<Value>(body).unwrap())
        .unwrap();

    assert!(price_request.starts_with("POST /prices/latest/batch "));
    assert_eq!(price_body["slugs"], json!(["usdc", "ethereum"]));
    assert_eq!(price_body["quoteCurrency"], "MXN");
    assert_eq!(
        result
            .accounts
            .iter()
            .map(|account| account.account.clone())
            .collect::<Vec<_>>(),
        request.accounts
    );

    let base_usdc = &result.accounts[0].items[0];
    assert!(matches!(
        base_usdc,
        BalanceItemOutcome::Resolved {
            amount,
            quote: BalanceQuoteOutcome::Available {
                currency,
                unit_price,
                value,
                ..
            },
            ..
        } if amount.as_deref() == Some("0.001000")
            && currency == "MXN"
            && unit_price == "2.50"
            && value == "0.002500"
    ));
    let eth_native = &result.accounts[1].items[1];
    assert!(matches!(
        eth_native,
        BalanceItemOutcome::Resolved {
            amount,
            quote: BalanceQuoteOutcome::Available { value, .. },
            ..
        } if amount.as_deref() == Some("0.000000000000001000")
            && value == "0.000000000000002500"
    ));
}

#[tokio::test]
async fn historical_requests_do_not_use_latest_quote_lookup() {
    let Some((bigwig_url, bigwig_server)) = spawn_dynamic_server(1) else {
        return;
    };
    let Ok(price_listener) = TcpListener::bind("127.0.0.1:0") else {
        return;
    };
    let price_url = format!("http://{}", price_listener.local_addr().unwrap());

    let result = service_with_quote(
        Some(bigwig_client(&bigwig_url)),
        Some(price_quote_client(&price_url)),
    )
    .resolve(GetBalancesCommand {
        as_of: AsOf::BlockNumber {
            block_number: "19000000".to_string(),
        },
        accounts: vec![account("eth-mainnet", ACCOUNT_A, None)],
        tokens: token_slugs(["ethereum"]),
        quote_currency: "USD".to_string(),
    })
    .await
    .unwrap();

    let requests = bigwig_server.join().unwrap();
    assert_eq!(
        requests[0]["as_of"],
        json!({"kind": "block_number", "block_number": "19000000"})
    );
    price_listener.set_nonblocking(true).unwrap();
    assert_eq!(
        price_listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    assert!(matches!(
        &result.accounts[0].items[0],
        BalanceItemOutcome::Resolved {
            quote: BalanceQuoteOutcome::Unavailable {
                code: BalanceItemErrorCode::PriceResolutionFailed,
            },
            ..
        }
    ));
}

#[test]
fn matches_quotes_with_the_same_normalized_pricing_slug_used_for_collection() {
    let mut pricing_target = resolved_target_from_asset_selector(
        "ethereum",
        target("eth-mainnet", 1, "ethereum", BalanceTargetKind::Native),
    );
    pricing_target.pricing_asset_slug = Some(" Ethereum ".to_string());
    let accounts = vec![RawBalancesAccountResult {
        account: account("eth-mainnet", ACCOUNT_A, None),
        evidence: None,
        items: vec![RawBalanceItemOutcome::Resolved {
            target: pricing_target,
            raw_amount: "1000000000000000000".to_string(),
        }],
    }];

    assert_eq!(
        collect_pricing_asset_slugs(&accounts),
        vec!["ethereum".to_string()]
    );

    let quotes = Ok(HashMap::from([(
        "ethereum".to_string(),
        PriceQuoteResolution::Available {
            unit_price: "2.50".to_string(),
            quote_currency: "USD".to_string(),
            price_as_of: "2026-06-17T11:59:59Z".to_string(),
        },
    )]));
    let results = enrich_account_results(accounts, quotes);

    assert!(matches!(
        &results[0].items[0],
        BalanceItemOutcome::Resolved {
            target,
            amount,
            quote: BalanceQuoteOutcome::Available {
                currency,
                unit_price,
                value,
                ..
            },
            ..
        } if target.pricing_asset_slug.as_deref() == Some(" Ethereum ")
            && amount.as_deref() == Some("1.000000000000000000")
            && currency == "USD"
            && unit_price == "2.50"
            && value == "2.500000000000000000"
    ));
}

#[test]
fn unavailable_quote_resolution_preserves_raw_balance() {
    let accounts = vec![RawBalancesAccountResult {
        account: account("eth-mainnet", ACCOUNT_A, None),
        evidence: None,
        items: vec![RawBalanceItemOutcome::Resolved {
            target: resolved_target_from_asset_selector(
                "usdc",
                target(
                    "eth-mainnet",
                    1,
                    "usdc",
                    BalanceTargetKind::Erc20 {
                        contract_address: "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48".to_string(),
                    },
                ),
            ),
            raw_amount: "1000".to_string(),
        }],
    }];
    let quotes = Ok(HashMap::from([(
        "usdc".to_string(),
        PriceQuoteResolution::Unavailable,
    )]));
    let results = enrich_account_results(accounts, quotes);

    assert!(matches!(
        &results[0].items[0],
        BalanceItemOutcome::Resolved {
            raw_amount,
            quote: BalanceQuoteOutcome::Unavailable {
                code: BalanceItemErrorCode::PriceResolutionFailed,
            },
            ..
        } if raw_amount == "1000"
    ));
}

#[test]
fn missing_quote_entries_are_quote_resolution_failures_not_internal_errors() {
    let accounts = vec![RawBalancesAccountResult {
        account: account("eth-mainnet", ACCOUNT_A, None),
        evidence: None,
        items: vec![RawBalanceItemOutcome::Resolved {
            target: resolved_target_from_asset_selector(
                "ethereum",
                target("eth-mainnet", 1, "ethereum", BalanceTargetKind::Native),
            ),
            raw_amount: "1000000000000000000".to_string(),
        }],
    }];
    let results = enrich_account_results(accounts, Ok(HashMap::new()));

    assert!(matches!(
        &results[0].items[0],
        BalanceItemOutcome::Resolved {
            raw_amount,
            amount,
            quote: BalanceQuoteOutcome::Unavailable {
                code: BalanceItemErrorCode::PriceResolutionFailed,
            },
            ..
        } if raw_amount == "1000000000000000000"
            && amount.as_deref() == Some("1.000000000000000000")
    ));
}

#[test]
fn quote_provider_failures_preserve_all_resolved_raw_balances() {
    let accounts = vec![RawBalancesAccountResult {
        account: account("eth-mainnet", ACCOUNT_A, None),
        evidence: None,
        items: vec![
            RawBalanceItemOutcome::Resolved {
                target: resolved_target_from_asset_selector(
                    "usdc",
                    target(
                        "eth-mainnet",
                        1,
                        "usdc",
                        BalanceTargetKind::Erc20 {
                            contract_address: "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
                                .to_string(),
                        },
                    ),
                ),
                raw_amount: "1000".to_string(),
            },
            RawBalanceItemOutcome::Resolved {
                target: resolved_target_from_asset_selector(
                    "ethereum",
                    target("eth-mainnet", 1, "ethereum", BalanceTargetKind::Native),
                ),
                raw_amount: "2000".to_string(),
            },
        ],
    }];
    let results = enrich_account_results(accounts, Err(PriceQuoteClientError::ProviderUnavailable));
    let raw_amounts = results[0]
        .items
        .iter()
        .map(|item| match item {
            BalanceItemOutcome::Resolved {
                raw_amount,
                quote:
                    BalanceQuoteOutcome::Unavailable {
                        code: BalanceItemErrorCode::PriceProviderUnavailable,
                    },
                ..
            } => raw_amount.as_str(),
            other => panic!("expected resolved balance with provider-unavailable quote: {other:?}"),
        })
        .collect::<Vec<_>>();

    assert_eq!(raw_amounts, vec!["1000", "2000"]);
}

#[tokio::test]
async fn unresolved_explicit_contracts_skip_quote_lookup_and_return_unsupported() {
    let contract = "0x9999999999999999999999999999999999999999";
    let Some((bigwig_url, bigwig_server)) = spawn_dynamic_server(1) else {
        return;
    };
    let Ok(price_listener) = TcpListener::bind("127.0.0.1:0") else {
        return;
    };
    let price_url = format!("http://{}", price_listener.local_addr().unwrap());

    let result = service_with_quote(
        Some(bigwig_client(&bigwig_url)),
        Some(price_quote_client(&price_url)),
    )
    .resolve(GetBalancesCommand {
        as_of: AsOf::Latest,
        accounts: vec![account("eth-mainnet", ACCOUNT_A, None)],
        tokens: mixed_tokens(&[], &[contract]),
        quote_currency: "USD".to_string(),
    })
    .await
    .unwrap();

    bigwig_server.join().unwrap();
    price_listener.set_nonblocking(true).unwrap();
    assert_eq!(
        price_listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    assert!(matches!(
        &result.accounts[0].items[0],
        BalanceItemOutcome::Resolved {
            target,
            raw_amount,
            amount: None,
            quote: BalanceQuoteOutcome::Unsupported,
        } if target.selector == BalanceTokenSelector::ContractAddress(contract.to_string())
            && target.asset_slug.is_none()
            && target.decimals.is_none()
            && raw_amount == "1000"
    ));
}

fn spawn_dynamic_server(request_count: usize) -> Option<(String, thread::JoinHandle<Vec<Value>>)> {
    let listener = match TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => listener,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return None,
        Err(error) => panic!("failed to bind orchestration test server: {error}"),
    };
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    let handle = thread::spawn(move || {
        let mut connections = Vec::with_capacity(request_count);

        // No response is written until every expected connection arrives.
        // A sequential orchestrator therefore times out this test, while
        // concurrent per-network calls make progress.
        for _ in 0..request_count {
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_http_request(&mut stream);
            let body = request
                .split_once("\r\n\r\n")
                .map(|(_, body)| body)
                .unwrap();
            let request_body = serde_json::from_str::<Value>(body).unwrap();
            connections.push((stream, request_body));
        }

        let requests = connections
            .iter()
            .map(|(_, request)| request.clone())
            .collect::<Vec<_>>();
        for (mut stream, request) in connections {
            write_json_response(&mut stream, StatusCode::OK, dynamic_response(&request));
        }

        requests
    });

    Some((base_url, handle))
}

fn service(client: Option<BigwigClient>) -> BalanceSnapshotService {
    service_with_quote(client, None)
}

fn service_with_quote(
    client: Option<BigwigClient>,
    price_quote_client: Option<PriceQuoteClient>,
) -> BalanceSnapshotService {
    BalanceSnapshotService::new(
        CatalogBalanceTargetResolver::new(GlobalAssetRepository::in_memory(sample_assets())),
        client,
        price_quote_client,
    )
}

fn bigwig_client(base_url: &str) -> BigwigClient {
    BigwigClient::new(base_url, "test-token", 2_000).unwrap()
}

fn price_quote_client(base_url: &str) -> PriceQuoteClient {
    PriceQuoteClient::new(PriceIndexerClient::new(base_url, "test-token", 2_000).unwrap())
}

fn account(network_slug: &str, address: &str, client_ref: Option<&str>) -> OnchainAccount {
    OnchainAccount {
        network_slug: network_slug.to_string(),
        address: address.to_string(),
        client_ref: client_ref.map(str::to_string),
    }
}

fn token_slugs<const N: usize>(asset_slugs: [&str; N]) -> TokenSelector {
    TokenSelector {
        asset_slugs: asset_slugs.into_iter().map(str::to_string).collect(),
        contract_addresses: Vec::new(),
    }
}

fn mixed_tokens(asset_slugs: &[&str], contract_addresses: &[&str]) -> TokenSelector {
    TokenSelector {
        asset_slugs: asset_slugs
            .iter()
            .map(|asset_slug| (*asset_slug).to_string())
            .collect(),
        contract_addresses: contract_addresses
            .iter()
            .map(|contract_address| (*contract_address).to_string())
            .collect(),
    }
}

fn write_json_response(stream: &mut impl Write, status: StatusCode, body: Value) {
    let body = serde_json::to_string(&body).unwrap();
    let reason = status.canonical_reason().unwrap_or("Unknown");
    let response = format!(
            "HTTP/1.1 {} {}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            status.as_u16(),
            reason,
            body.len(),
            body
        );
    stream.write_all(response.as_bytes()).unwrap();
}

fn spawn_price_server(
    slugs: &[&str],
    quote_currency: &str,
    unit_price: &str,
) -> Option<(String, thread::JoinHandle<String>)> {
    let listener = match TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => listener,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return None,
        Err(error) => panic!("failed to bind price test server: {error}"),
    };
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    let slugs = slugs
        .iter()
        .map(|slug| slug.to_string())
        .collect::<Vec<_>>();
    let quote_currency = quote_currency.to_string();
    let unit_price = unit_price.to_string();
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let request = read_http_request(&mut stream);
        let results = slugs
            .iter()
            .map(|slug| {
                json!({
                    "requestedSlug": slug,
                    "normalizedSlug": slug,
                    "assetId": slug,
                    "slug": slug,
                    "name": slug,
                    "status": "found",
                    "freshnessStatus": "fresh",
                    "price": {
                        "assetId": slug,
                        "slug": slug,
                        "quoteCurrency": quote_currency.clone(),
                        "price": unit_price.clone(),
                        "sourceType": "test",
                        "recordedAt": "2026-06-17T11:59:59Z",
                        "freshnessStatus": "fresh"
                    },
                    "error": null
                })
            })
            .collect::<Vec<_>>();
        write_json_response(
            &mut stream,
            StatusCode::OK,
            json!({
                "quoteCurrency": quote_currency.clone(),
                "requestedCount": slugs.len(),
                "uniqueCount": slugs.len(),
                "results": results
            }),
        );
        request
    });

    Some((base_url, handle))
}

fn dynamic_response(request: &Value) -> Value {
    let network_slug = request["network_slug"].as_str().unwrap();
    let chain_id = match network_slug {
        "eth-mainnet" => 1,
        "base-mainnet" => 8453,
        "arbitrum-mainnet" => 42161,
        "mantle-mainnet" => 5000,
        other => panic!("unexpected test network: {other}"),
    };
    let accounts = request["accounts"].as_array().unwrap();
    let targets = request["tokens"].as_array().unwrap();
    let items = accounts
        .iter()
        .flat_map(|account| {
            targets.iter().map(move |target| {
                json!({
                    "account_address": account,
                    "requested_token": target,
                    "raw_balance": {
                        "status": "resolved",
                        "value": "1000"
                    }
                })
            })
        })
        .collect::<Vec<_>>();

    json!({
        "primitive": "evm_balances",
        "status": "complete",
        "network": {
            "network_slug": network_slug,
            "chain_id": chain_id
        },
        "requested_as_of": request["as_of"].clone(),
        "resolved_evidence": {
            "kind": "observed_head",
            "block_number": "123",
            "block_hash": format!("0x{}", "a".repeat(64)),
            "block_timestamp": "2026-06-17T12:00:00Z"
        },
        "items": items
    })
}

fn read_http_request(stream: &mut impl Read) -> String {
    let mut request = Vec::new();
    let mut buffer = [0; 1024];

    loop {
        let bytes_read = stream.read(&mut buffer).unwrap();
        if bytes_read == 0 {
            break;
        }
        request.extend_from_slice(&buffer[..bytes_read]);

        let Some(headers_end) = request.windows(4).position(|window| window == b"\r\n\r\n") else {
            continue;
        };
        let headers = String::from_utf8_lossy(&request[..headers_end]);
        let content_length = headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().ok())
                    .flatten()
            })
            .unwrap_or(0);
        if request.len() >= headers_end + 4 + content_length {
            break;
        }
    }

    String::from_utf8(request).unwrap()
}

fn request_body_json(request: &str) -> Value {
    request
        .split_once("\r\n\r\n")
        .map(|(_, body)| serde_json::from_str::<Value>(body).unwrap())
        .unwrap()
}

fn grouped_accounts(network_slug: &str, count: usize) -> GroupedAccounts {
    GroupedAccounts {
        network_slug: network_slug.to_string(),
        accounts: (0..count)
            .map(|index| GroupAccount {
                original_index: index,
                account: account(
                    network_slug,
                    &format!("0x{index:040x}"),
                    Some(&format!("account-{index}")),
                ),
            })
            .collect(),
    }
}

fn target(
    network_slug: &str,
    chain_id: i64,
    asset_slug: &str,
    kind: BalanceTargetKind,
) -> BalanceTarget {
    BalanceTarget {
        network_slug: network_slug.to_string(),
        chain_id,
        asset_slug: asset_slug.to_string(),
        symbol: asset_slug.to_ascii_uppercase(),
        name: asset_slug.to_string(),
        decimals: 18,
        pricing_asset_slug: asset_slug.to_string(),
        kind,
    }
}

fn validation_plan() -> NetworkGroupPlan {
    plan_asset_group(
        grouped_accounts("base-mainnet", 2),
        vec!["usdc".to_string(), "ethereum".to_string()],
        vec![
            BalanceTargetResolution::Resolved(target(
                "base-mainnet",
                8453,
                "usdc",
                BalanceTargetKind::Erc20 {
                    contract_address: "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
                },
            )),
            BalanceTargetResolution::Resolved(target(
                "base-mainnet",
                8453,
                "ethereum",
                BalanceTargetKind::Native,
            )),
        ],
    )
    .unwrap()
}

fn plan_asset_group(
    group: GroupedAccounts,
    asset_slugs: Vec<String>,
    resolutions: Vec<BalanceTargetResolution>,
) -> Result<NetworkGroupPlan, BalanceSnapshotServiceError> {
    let tokens = TokenSelector {
        asset_slugs,
        contract_addresses: Vec::new(),
    };
    plan_network_group(group, &AsOf::Latest, &tokens, None, resolutions, Vec::new())
}

fn response_for_plan(
    plan: &NetworkGroupPlan,
    resolved: Vec<bool>,
    status: BigwigEvidenceStatus,
) -> BigwigResponse {
    let items = plan
        .accounts
        .iter()
        .flat_map(|account| {
            plan.targets.iter().map(move |target| {
                (
                    account.account.address.to_ascii_uppercase(),
                    target.wire_target.clone(),
                )
            })
        })
        .zip(resolved)
        .enumerate()
        .map(|(index, ((account, target), resolved))| {
            if resolved {
                BigwigEvidenceItem::Resolved {
                    account_address: account,
                    target,
                    raw_amount: format!("{}000", index + 1),
                }
            } else {
                BigwigEvidenceItem::Failed {
                    account_address: account,
                    target,
                    error: BigwigItemError {
                        code: BigwigItemErrorCode::Erc20BalanceCallFailed,
                        message: "Bigwig-owned message".to_string(),
                    },
                }
            }
        })
        .collect();

    BigwigResponse {
        primitive: BigwigPrimitive::EvmBalances,
        status,
        network: BigwigEvidenceNetwork {
            network_slug: plan.network_slug.clone(),
            chain_id: u64::try_from(plan.chain_id.unwrap()).unwrap(),
        },
        requested_as_of: BigwigAsOfDTO::from(&plan.as_of),
        resolved_evidence: BigwigResolvedEvidence {
            kind: BigwigResolvedEvidenceKind::ObservedHead,
            block_number: "123".to_string(),
            block_hash: format!("0x{}", "a".repeat(64)),
            block_timestamp: "2026-06-17T12:00:00Z".to_string(),
        },
        items,
    }
}

#[tokio::test]
async fn deduplicates_targets_and_fans_out_duplicate_assets() {
    let Some((base_url, server)) = spawn_dynamic_server(1) else {
        return;
    };
    let result = service(Some(bigwig_client(&base_url)))
        .resolve(GetBalancesCommand {
            as_of: AsOf::Latest,
            accounts: vec![account("base-mainnet", ACCOUNT_A, None)],
            tokens: token_slugs(["usdc", "usdc"]),
            quote_currency: "USD".to_string(),
        })
        .await
        .unwrap();
    let requests = server.join().unwrap();

    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0]["tokens"].as_array().unwrap().len(), 1);
    assert_eq!(result.accounts[0].items.len(), 2);
    let raw_amounts = result.accounts[0]
        .items
        .iter()
        .map(|item| match item {
            BalanceItemOutcome::Resolved {
                target,
                raw_amount,
                quote,
                ..
            } => {
                assert_eq!(target.asset_slug.as_deref(), Some("usdc"));
                assert_eq!(
                    quote,
                    &BalanceQuoteOutcome::Unavailable {
                        code: BalanceItemErrorCode::PriceProviderUnavailable,
                    }
                );
                raw_amount.as_str()
            }
            _ => panic!("duplicate supported assets should both resolve"),
        })
        .collect::<Vec<_>>();
    assert_eq!(raw_amounts, vec!["1000", "1000"]);
}

#[tokio::test]
async fn mixed_asset_and_explicit_contract_deduplicates_upstream_and_keeps_attribution() {
    let contract = "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48";
    let Some((bigwig_url, bigwig_server)) = spawn_dynamic_server(1) else {
        return;
    };
    let Some((price_url, price_server)) = spawn_price_server(&["usdc"], "USD", "1.00") else {
        return;
    };

    let result = service_with_quote(
        Some(bigwig_client(&bigwig_url)),
        Some(price_quote_client(&price_url)),
    )
    .resolve(GetBalancesCommand {
        as_of: AsOf::Latest,
        accounts: vec![account("eth-mainnet", ACCOUNT_A, None)],
        tokens: mixed_tokens(&["usdc"], &[contract]),
        quote_currency: "USD".to_string(),
    })
    .await
    .unwrap();

    let requests = bigwig_server.join().unwrap();
    assert_eq!(requests[0]["tokens"].as_array().unwrap().len(), 1);
    assert_eq!(
        requests[0]["tokens"],
        json!([{"kind": "erc20", "contract_address": contract}])
    );
    let price_request = price_server.join().unwrap();
    assert_eq!(request_body_json(&price_request)["slugs"], json!(["usdc"]));
    assert_eq!(result.accounts[0].items.len(), 2);
    assert!(matches!(
        &result.accounts[0].items[0],
        BalanceItemOutcome::Resolved {
            target,
            quote: BalanceQuoteOutcome::Available { .. },
            ..
        } if target.selector == BalanceTokenSelector::AssetSlug("usdc".to_string())
            && target.asset_slug.as_deref() == Some("usdc")
    ));
    assert!(matches!(
        &result.accounts[0].items[1],
        BalanceItemOutcome::Resolved {
            target,
            quote: BalanceQuoteOutcome::Available { .. },
            ..
        } if target.selector == BalanceTokenSelector::ContractAddress(contract.to_string())
            && target.asset_slug.as_deref() == Some("usdc")
    ));
}

#[tokio::test]
async fn skips_unsupported_pairs_without_calling_bigwig() {
    let Ok(listener) = TcpListener::bind("127.0.0.1:0") else {
        return;
    };
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    let result = service(Some(bigwig_client(&base_url)))
        .resolve(GetBalancesCommand {
            as_of: AsOf::Latest,
            accounts: vec![account("mantle-mainnet", ACCOUNT_A, None)],
            tokens: token_slugs(["wrapped-bitcoin"]),
            quote_currency: "USD".to_string(),
        })
        .await
        .unwrap();

    listener.set_nonblocking(true).unwrap();
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    assert_eq!(result.accounts[0].evidence, None);
    assert_eq!(
        result.accounts[0].items,
        vec![BalanceItemOutcome::Skipped {
            network_slug: "mantle-mainnet".to_string(),
            asset_slug: "wrapped-bitcoin".to_string(),
        }]
    );
}

#[tokio::test]
async fn skipped_only_results_do_not_call_price_indexer() {
    let Ok(listener) = TcpListener::bind("127.0.0.1:0") else {
        return;
    };
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    let result = service_with_quote(None, Some(price_quote_client(&base_url)))
        .resolve(GetBalancesCommand {
            as_of: AsOf::Latest,
            accounts: vec![account("mantle-mainnet", ACCOUNT_A, None)],
            tokens: token_slugs(["wrapped-bitcoin"]),
            quote_currency: "USD".to_string(),
        })
        .await
        .unwrap();

    listener.set_nonblocking(true).unwrap();
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    assert!(matches!(
        &result.accounts[0].items[0],
        BalanceItemOutcome::Skipped { .. }
    ));
}

#[tokio::test]
async fn missing_bigwig_client_marks_supported_items_provider_unavailable() {
    let result = service(None)
        .resolve(GetBalancesCommand {
            as_of: AsOf::Latest,
            accounts: vec![account("base-mainnet", ACCOUNT_A, None)],
            tokens: token_slugs(["usdc", "wrapped-bitcoin"]),
            quote_currency: "USD".to_string(),
        })
        .await
        .unwrap();

    assert_eq!(result.accounts[0].evidence, None);
    assert!(matches!(
        &result.accounts[0].items[0],
        BalanceItemOutcome::Failed {
            code: BalanceItemErrorCode::BalanceProviderUnavailable,
            ..
        }
    ));
    assert!(matches!(
        &result.accounts[0].items[1],
        BalanceItemOutcome::Skipped { .. }
    ));
}

#[tokio::test]
async fn planning_failure_prevents_all_bigwig_calls() {
    let Ok(listener) = TcpListener::bind("127.0.0.1:0") else {
        return;
    };
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    let error = service(Some(bigwig_client(&base_url)))
        .resolve(GetBalancesCommand {
            as_of: AsOf::Latest,
            accounts: vec![
                account("base-mainnet", ACCOUNT_A, None),
                account("unknown-mainnet", ACCOUNT_B, None),
            ],
            tokens: token_slugs(["usdc"]),
            quote_currency: "USD".to_string(),
        })
        .await
        .unwrap_err();

    assert!(matches!(
        error,
        BalanceSnapshotServiceError::UnsupportedNetwork { ref network_slug }
            if network_slug == "unknown-mainnet"
    ));
    listener.set_nonblocking(true).unwrap();
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[tokio::test]
async fn unsupported_global_asset_is_a_whole_request_error() {
    let error = service(None)
        .resolve(GetBalancesCommand {
            as_of: AsOf::Latest,
            accounts: vec![account("base-mainnet", ACCOUNT_A, None)],
            tokens: token_slugs(["missing-asset"]),
            quote_currency: "USD".to_string(),
        })
        .await
        .unwrap_err();

    assert!(matches!(
        error,
        BalanceSnapshotServiceError::UnsupportedAsset {
            ref network_slug,
            ref asset_slug,
        } if network_slug == "base-mainnet" && asset_slug == "missing-asset"
    ));
}

fn spawn_static_server(
    status: StatusCode,
    body: Value,
) -> Option<(String, thread::JoinHandle<()>)> {
    let listener = match TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => listener,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return None,
        Err(error) => panic!("failed to bind orchestration test server: {error}"),
    };
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let _request = read_http_request(&mut stream);
        write_json_response(&mut stream, status, body);
    });

    Some((base_url, handle))
}

#[tokio::test]
async fn malformed_raw_amount_invalidates_group_evidence_before_quote_lookup() {
    let body = json!({
        "primitive": "evm_balances",
        "status": "complete",
        "network": {
            "network_slug": "base-mainnet",
            "chain_id": 8453
        },
        "requested_as_of": {"kind": "latest"},
        "resolved_evidence": {
            "kind": "observed_head",
            "block_number": "123",
            "block_hash": format!("0x{}", "a".repeat(64)),
            "block_timestamp": "2026-06-17T12:00:00Z"
        },
        "items": [{
            "account_address": ACCOUNT_A,
            "requested_token": {
                "kind": "erc20",
                "contract_address": "0x833589fcd6edb6e08f4c7c32d4f71b54bda02913"
            },
            "raw_balance": {
                "status": "resolved",
                "value": "1.0"
            }
        }]
    });
    let Some((base_url, server)) = spawn_static_server(StatusCode::OK, body) else {
        return;
    };

    let result = service(Some(bigwig_client(&base_url)))
        .resolve(GetBalancesCommand {
            as_of: AsOf::Latest,
            accounts: vec![account("base-mainnet", ACCOUNT_A, None)],
            tokens: token_slugs(["usdc"]),
            quote_currency: "USD".to_string(),
        })
        .await
        .unwrap();
    server.join().unwrap();

    assert_eq!(result.accounts[0].evidence, None);
    assert!(matches!(
        &result.accounts[0].items[0],
        BalanceItemOutcome::Failed {
            code: BalanceItemErrorCode::InternalError,
            ..
        }
    ));
}

#[test]
fn rejects_cross_asset_target_collisions_and_conflicting_metadata() {
    let group = grouped_accounts("base-mainnet", 1);
    let assets = vec!["asset-a".to_string(), "asset-b".to_string()];
    let collision = plan_asset_group(
        group.clone(),
        assets,
        vec![
            BalanceTargetResolution::Resolved(target(
                "base-mainnet",
                8453,
                "asset-a",
                BalanceTargetKind::Native,
            )),
            BalanceTargetResolution::Resolved(target(
                "base-mainnet",
                8453,
                "asset-b",
                BalanceTargetKind::Native,
            )),
        ],
    )
    .unwrap_err();
    assert!(matches!(
        collision,
        BalanceSnapshotServiceError::InvalidPlan {
            issue: BalancePlanIssue::TargetCollision,
            ..
        }
    ));

    let duplicate_assets = vec!["asset-a".to_string(), "asset-a".to_string()];
    let first = target("base-mainnet", 8453, "asset-a", BalanceTargetKind::Native);
    let mut conflicting = first.clone();
    conflicting.symbol = "DIFFERENT".to_string();
    let error = plan_asset_group(
        group,
        duplicate_assets,
        vec![
            BalanceTargetResolution::Resolved(first),
            BalanceTargetResolution::Resolved(conflicting),
        ],
    )
    .unwrap_err();
    assert!(matches!(
        error,
        BalanceSnapshotServiceError::InvalidPlan {
            issue: BalancePlanIssue::ConflictingTargetMetadata,
            ..
        }
    ));
}

#[test]
fn rejects_inconsistent_chain_ids_and_bigwig_group_limits() {
    let assets = vec!["asset-a".to_string(), "asset-b".to_string()];
    let inconsistent = plan_asset_group(
        grouped_accounts("base-mainnet", 1),
        assets,
        vec![
            BalanceTargetResolution::Resolved(target(
                "base-mainnet",
                8453,
                "asset-a",
                BalanceTargetKind::Native,
            )),
            BalanceTargetResolution::Resolved(target(
                "base-mainnet",
                1,
                "asset-b",
                BalanceTargetKind::Erc20 {
                    contract_address: "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
                },
            )),
        ],
    )
    .unwrap_err();
    assert!(matches!(
        inconsistent,
        BalanceSnapshotServiceError::InvalidPlan {
            issue: BalancePlanIssue::InconsistentChainId,
            ..
        }
    ));

    let too_many_accounts = plan_asset_group(
        grouped_accounts("base-mainnet", BIGWIG_MAX_ACCOUNTS + 1),
        vec!["asset-a".to_string()],
        vec![BalanceTargetResolution::Resolved(target(
            "base-mainnet",
            8453,
            "asset-a",
            BalanceTargetKind::Native,
        ))],
    )
    .unwrap_err();
    assert!(matches!(
        too_many_accounts,
        BalanceSnapshotServiceError::RequestTooLarge { .. }
    ));

    let many_assets = (0..=BIGWIG_MAX_TARGETS)
        .map(|index| format!("asset-{index}"))
        .collect::<Vec<_>>();
    let many_resolutions = many_assets
        .iter()
        .enumerate()
        .map(|(index, asset_slug)| {
            BalanceTargetResolution::Resolved(target(
                "base-mainnet",
                8453,
                asset_slug,
                BalanceTargetKind::Erc20 {
                    contract_address: format!("0x{index:040x}"),
                },
            ))
        })
        .collect();
    let too_many_targets = plan_asset_group(
        grouped_accounts("base-mainnet", 1),
        many_assets,
        many_resolutions,
    )
    .unwrap_err();
    assert!(matches!(
        too_many_targets,
        BalanceSnapshotServiceError::RequestTooLarge { .. }
    ));
}

#[test]
fn validates_complete_partial_and_failed_envelopes_with_evidence() {
    let plan = validation_plan();

    for (resolved, expected_status) in [
        (vec![true, true, true, true], BigwigEvidenceStatus::Complete),
        (
            vec![true, false, true, false],
            BigwigEvidenceStatus::Partial,
        ),
        (
            vec![false, false, false, false],
            BigwigEvidenceStatus::Failed,
        ),
    ] {
        let validated =
            validate_response(&plan, response_for_plan(&plan, resolved, expected_status)).unwrap();
        assert_eq!(validated.evidence.network_slug, "base-mainnet");
        assert_eq!(validated.evidence.observed_at, "2026-06-17T12:00:00Z");
        assert_eq!(validated.evidence.block_number, "123");
        assert_eq!(validated.target_outcomes.len(), 4);
    }

    let partial = response_for_plan(
        &plan,
        vec![true, false, true, false],
        BigwigEvidenceStatus::Partial,
    );
    let results = assemble_group_results(&plan, GroupExecution::Called(Ok(partial)));
    assert!(results
        .iter()
        .all(|(_, account)| account.evidence.is_some()));
    assert!(matches!(
        &results[0].1.items[0],
        RawBalanceItemOutcome::Resolved {
            raw_amount,
            ..
        } if raw_amount == "1000"
    ));
    assert!(matches!(
        &results[0].1.items[1],
        RawBalanceItemOutcome::Failed {
            code: BalanceItemErrorCode::BalanceResolutionFailed,
            ..
        }
    ));

    let failed = response_for_plan(
        &plan,
        vec![false, false, false, false],
        BigwigEvidenceStatus::Failed,
    );
    let failed_results = assemble_group_results(&plan, GroupExecution::Called(Ok(failed)));
    assert!(failed_results
        .iter()
        .all(|(_, account)| account.evidence.is_some()
            && account.items.iter().all(|item| matches!(
                item,
                RawBalanceItemOutcome::Failed {
                    code: BalanceItemErrorCode::BalanceResolutionFailed,
                    ..
                }
            ))));

    let request_failure =
        assemble_group_results(&plan, GroupExecution::Called(Err(BigwigError::RpcError)));
    assert!(request_failure
        .iter()
        .all(|(_, account)| account.evidence.is_none()
            && account.items.iter().all(|item| matches!(
                item,
                RawBalanceItemOutcome::Failed {
                    code: BalanceItemErrorCode::BalanceResolutionFailed,
                    ..
                }
            ))));
}

#[test]
fn rejects_malformed_bigwig_success_correlations_and_status() {
    let plan = validation_plan();
    let valid = || {
        response_for_plan(
            &plan,
            vec![true, true, true, true],
            BigwigEvidenceStatus::Complete,
        )
    };

    let mut wrong_network = valid();
    wrong_network.network.network_slug = "eth-mainnet".to_string();
    assert_eq!(
        validate_response(&plan, wrong_network).unwrap_err(),
        ResponseValidationIssue::WrongNetwork
    );

    let mut wrong_chain = valid();
    wrong_chain.network.chain_id = 1;
    assert_eq!(
        validate_response(&plan, wrong_chain).unwrap_err(),
        ResponseValidationIssue::WrongChainId
    );

    let mut missing = valid();
    missing.items.pop();
    assert_eq!(
        validate_response(&plan, missing).unwrap_err(),
        ResponseValidationIssue::WrongCardinality
    );

    let mut extra = valid();
    extra.items.push(extra.items[0].clone());
    assert_eq!(
        validate_response(&plan, extra).unwrap_err(),
        ResponseValidationIssue::WrongCardinality
    );

    let mut wrong_order = valid();
    wrong_order.items.swap(0, 1);
    assert_eq!(
        validate_response(&plan, wrong_order).unwrap_err(),
        ResponseValidationIssue::UnexpectedCorrelation
    );

    let mut duplicate = valid();
    duplicate.items[1] = duplicate.items[0].clone();
    assert_eq!(
        validate_response(&plan, duplicate).unwrap_err(),
        ResponseValidationIssue::DuplicateCorrelation
    );

    let mut wrong_status = valid();
    wrong_status.status = BigwigEvidenceStatus::Partial;
    assert_eq!(
        validate_response(&plan, wrong_status).unwrap_err(),
        ResponseValidationIssue::WrongStatus
    );
}

#[test]
fn maps_every_bigwig_request_wide_failure_class() {
    let resolution_failures = [
        BigwigError::UnsupportedNetwork,
        BigwigError::NetworkNotEnabledForOperation,
        BigwigError::NoRouteSatisfiesOperation,
        BigwigError::BlockOutOfRange,
        BigwigError::TimestampAnchorNotConfigured,
        BigwigError::TimestampOutOfRange,
        BigwigError::RpcError,
    ];
    for error in resolution_failures {
        assert_eq!(
            map_bigwig_error(&error),
            BalanceItemErrorCode::BalanceResolutionFailed
        );
    }

    let provider_failures = [
        BigwigError::Transport,
        BigwigError::Timeout,
        BigwigError::Unauthorized,
        BigwigError::RateLimited {
            retry_after_seconds: Some(7),
        },
        BigwigError::ProviderUnavailable {
            retry_after_seconds: Some(9),
        },
        BigwigError::ProviderTimeout,
        BigwigError::ExtractionTimeout,
        BigwigError::InternalError,
    ];
    for error in provider_failures {
        assert_eq!(
            map_bigwig_error(&error),
            BalanceItemErrorCode::BalanceProviderUnavailable
        );
    }

    let internal_failures = [
        BigwigError::InvalidExtractionRequest,
        BigwigError::InvalidAddress,
        BigwigError::InvalidContractAddress,
        BigwigError::InvalidDirection,
        BigwigError::InvalidAsOf,
        BigwigError::InvalidWindowShape,
        BigwigError::ReversedBlockRange,
        BigwigError::ReversedTimestampRange,
        BigwigError::LookbackTooLarge,
        BigwigError::RangeTooLarge,
        BigwigError::TooManyContractAddresses,
        BigwigError::RequestValidation(BigwigRequestValidationCode::MalformedBody),
        BigwigError::RequestValidation(BigwigRequestValidationCode::EmptyAccounts),
        BigwigError::RequestValidation(BigwigRequestValidationCode::EmptyTargets),
        BigwigError::RequestValidation(BigwigRequestValidationCode::InvalidAccount),
        BigwigError::RequestValidation(BigwigRequestValidationCode::DuplicateAccount),
        BigwigError::RequestValidation(BigwigRequestValidationCode::InvalidTarget),
        BigwigError::RequestValidation(BigwigRequestValidationCode::DuplicateTarget),
        BigwigError::RequestValidation(BigwigRequestValidationCode::RequestTooLarge),
        BigwigError::MalformedSuccessResponse,
        BigwigError::MalformedErrorResponse,
        BigwigError::UnexpectedSuccessStatus(201),
        BigwigError::UnexpectedErrorResponse { status: 418 },
    ];
    for error in internal_failures {
        assert_eq!(
            map_bigwig_error(&error),
            BalanceItemErrorCode::InternalError
        );
    }
}
