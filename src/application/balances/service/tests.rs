use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
};

use reqwest::StatusCode;
use serde_json::{json, Value};

use super::*;
use crate::test_utils::fixtures::global_assets::sample_assets;
use crate::{
    adapters::bigwig::balances::{
        BigwigEvidenceBlock, BigwigEvidenceNetwork, BigwigItemError, BigwigItemErrorCode,
        BigwigRequestValidationCode,
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
    let request = BalanceSnapshotRequest {
        accounts: vec![
            account("base-mainnet", ACCOUNT_A, Some("base-a")),
            account("eth-mainnet", ACCOUNT_B, Some("eth-b")),
            account("base-mainnet", ACCOUNT_C, Some("base-c")),
        ],
        asset_slugs: vec!["usdc".to_string(), "ethereum".to_string()],
        quote_currency: "MXN".to_string(),
    };

    let result = service.resolve_latest(request.clone()).await.unwrap();
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
        json!([{ "address": ACCOUNT_A }, { "address": ACCOUNT_C }])
    );
    assert_eq!(
        requests_by_network["eth-mainnet"]["accounts"],
        json!([{ "address": ACCOUNT_B }])
    );
    for network_slug in ["base-mainnet", "eth-mainnet"] {
        assert_eq!(
            requests_by_network[network_slug]["targets"][0]["kind"],
            "erc20"
        );
        assert_eq!(
            requests_by_network[network_slug]["targets"][1],
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
    assert_eq!(result.requested_asset_slugs, request.asset_slugs);
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
    let request = BalanceSnapshotRequest {
        accounts: vec![
            account("base-mainnet", ACCOUNT_A, Some("base")),
            account("eth-mainnet", ACCOUNT_B, Some("eth")),
        ],
        asset_slugs: vec!["usdc".to_string(), "ethereum".to_string()],
        quote_currency: "MXN".to_string(),
    };

    let result = service_with_quote(
        Some(bigwig_client(&bigwig_url)),
        Some(price_quote_client(&price_url)),
    )
    .resolve_latest(request.clone())
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
        } if amount == "0.001000"
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
        } if amount == "0.000000000000001000"
            && value == "0.000000000000002500"
    ));
}

#[test]
fn matches_quotes_with_the_same_normalized_pricing_slug_used_for_collection() {
    let mut pricing_target = target("eth-mainnet", 1, "ethereum", BalanceTargetKind::Native);
    pricing_target.pricing_asset_slug = " Ethereum ".to_string();
    let accounts = vec![RawBalanceAccountResult {
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
        } if target.pricing_asset_slug == " Ethereum "
            && amount == "1.000000000000000000"
            && currency == "USD"
            && unit_price == "2.50"
            && value == "2.500000000000000000"
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

fn account(network_slug: &str, address: &str, client_ref: Option<&str>) -> BalanceSnapshotAccount {
    BalanceSnapshotAccount {
        network_slug: network_slug.to_string(),
        address: address.to_string(),
        client_ref: client_ref.map(str::to_string),
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
    let targets = request["targets"].as_array().unwrap();
    let items = accounts
        .iter()
        .flat_map(|account| {
            targets.iter().map(move |target| {
                json!({
                    "status": "resolved",
                    "account": account,
                    "target": target,
                    "raw_amount": "1000"
                })
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
        "observed_at": "2026-06-17T12:00:00Z",
        "block": {
            "number": "123",
            "hash": format!("0x{}", "a".repeat(64))
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
    plan_network_group(
        grouped_accounts("base-mainnet", 2),
        &["usdc".to_string(), "ethereum".to_string()],
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
                    BigwigAccount {
                        address: account.account.address.to_ascii_uppercase(),
                    },
                    target.wire_target.clone(),
                )
            })
        })
        .zip(resolved)
        .enumerate()
        .map(|(index, ((account, target), resolved))| {
            if resolved {
                BigwigEvidenceItem::Resolved {
                    account,
                    target,
                    raw_amount: format!("{}000", index + 1),
                }
            } else {
                BigwigEvidenceItem::Failed {
                    account,
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
        primitive: BigwigPrimitive::EvmLatestBalances,
        status,
        network: BigwigEvidenceNetwork {
            network_slug: plan.network_slug.clone(),
            chain_id: u64::try_from(plan.chain_id.unwrap()).unwrap(),
        },
        observed_at: "2026-06-17T12:00:00Z".to_string(),
        block: BigwigEvidenceBlock {
            number: "123".to_string(),
            hash: format!("0x{}", "a".repeat(64)),
        },
        items,
    }
}
