use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
};

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::test_utils::{
    errors::assert_public_error,
    fixtures::{
        erc20_transfers::{
            erc20_transfers_enabled_config, erc20_transfers_request_with_tokens_body,
            valid_erc20_transfers_request_body,
        },
        router::{
            transfers_router, transfers_router_with_bigwig_client,
            transfers_router_without_repository,
        },
    },
    http::{post_json, post_raw},
};
use crate::{adapters::bigwig::BigwigClient, config::Config};

#[tokio::test]
async fn route_is_absent_when_disabled() {
    let response = transfers_router(Config::default())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/erc20-transfers/search")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&valid_erc20_transfers_request_body()).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn route_is_present_when_enabled() {
    let response = transfers_router(erc20_transfers_enabled_config())
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/erc20-transfers/search")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn valid_request_without_configured_bigwig_returns_extraction_unavailable() {
    let (status, response) = post_json(
        transfers_router(erc20_transfers_enabled_config()),
        "/v1/erc20-transfers/search",
        valid_erc20_transfers_request_body(),
    )
    .await;

    assert_public_error(
        status,
        &response,
        StatusCode::SERVICE_UNAVAILABLE,
        "extraction_unavailable",
    );
}

#[tokio::test]
async fn successful_bigwig_response_returns_final_public_response() {
    let Some((base_url, handle)) = spawn_bigwig_server(StatusCode::OK, bigwig_success_body())
    else {
        return;
    };
    let client = BigwigClient::new(&base_url, "test-token", 2_000).unwrap();

    let (status, response) = post_json(
        transfers_router_with_bigwig_client(erc20_transfers_enabled_config(), client),
        "/v1/erc20-transfers/search",
        valid_erc20_transfers_request_body(),
    )
    .await;
    let request = handle.join().unwrap();
    let (headers, body) = split_request(&request);

    assert_eq!(status, StatusCode::OK);
    assert!(headers.starts_with("POST /internal/v1/extractions/erc20-transfers HTTP/1.1\r\n"));
    assert_header(headers, "authorization", "Bearer test-token");
    assert_header(headers, "x-client-service", "mother-api");
    assert_eq!(
        serde_json::from_str::<Value>(body).unwrap(),
        json!({
            "network_slug": "eth-mainnet",
            "address": "0xabc0000000000000000000000000000000000000",
            "direction": "any",
            "contract_addresses": [
                "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
                "0x1111111111111111111111111111111111111111"
            ],
            "window": {
                "from_block": 18_600_000,
                "to_block": 18_600_500
            }
        })
    );
    let serialized = serde_json::from_str::<Value>(body).unwrap().to_string();
    assert!(!serialized.contains("asset_slug"));
    assert!(!serialized.contains("asset_slugs"));

    assert_eq!(response["ok"], true);
    assert_eq!(response["type"], "erc20_transfer_search");
    assert_eq!(response["transfers"][0]["amount"]["raw"], "12500000");
    assert_eq!(response["transfers"][0]["amount"]["decimal"], "12.5");
    assert_eq!(response["transfers"][0]["token"]["asset_slug"], "usdc");
    assert_eq!(response["transfers"][0]["token"]["symbol"], "USDC");
    assert_eq!(response["transfers"][0]["token"]["decimals"], 6);
    assert_eq!(response["transfers"][1]["amount"]["raw"], "1000000");
    assert_eq!(response["transfers"][1]["amount"]["decimal"], Value::Null);
    assert_eq!(response["transfers"][1]["token"]["asset_slug"], Value::Null);
    assert_eq!(response["limits"]["truncated"], true);
    assert_eq!(
        response["token_filters"]["resolved_contract_addresses"],
        json!([
            {
                "contract_address": "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
                "asset_slug": "usdc",
                "symbol": "USDC",
                "decimals": 6,
                "source": "asset_slug"
            },
            {
                "contract_address": "0x1111111111111111111111111111111111111111",
                "asset_slug": null,
                "symbol": null,
                "decimals": null,
                "source": "contract_address"
            }
        ])
    );
    assert_json_snapshot(
        &response,
        r#"{
  "address": "0xabc0000000000000000000000000000000000000",
  "direction": "any",
  "limits": {
    "max_rows": 5000,
    "truncated": true
  },
  "network_slug": "eth-mainnet",
  "ok": true,
  "token_filters": {
    "requested": {
      "asset_slugs": [
        "usdc"
      ],
      "contract_addresses": [
        "0x1111111111111111111111111111111111111111"
      ]
    },
    "resolved_contract_addresses": [
      {
        "asset_slug": "usdc",
        "contract_address": "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
        "decimals": 6,
        "source": "asset_slug",
        "symbol": "USDC"
      },
      {
        "asset_slug": null,
        "contract_address": "0x1111111111111111111111111111111111111111",
        "decimals": null,
        "source": "contract_address",
        "symbol": null
      }
    ]
  },
  "transfers": [
    {
      "amount": {
        "decimal": "12.5",
        "raw": "12500000"
      },
      "block_number": 18600001,
      "direction": "from",
      "from": "0xabc0000000000000000000000000000000000000",
      "log_index": 12,
      "to": "0x2222222222222222222222222222222222222222",
      "token": {
        "asset_slug": "usdc",
        "contract_address": "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
        "decimals": 6,
        "symbol": "USDC"
      },
      "tx_hash": "0x0000000000000000000000000000000000000000000000000000000000000001"
    },
    {
      "amount": {
        "decimal": null,
        "raw": "1000000"
      },
      "block_number": 18600002,
      "direction": "to",
      "from": "0x3333333333333333333333333333333333333333",
      "log_index": 13,
      "to": "0xabc0000000000000000000000000000000000000",
      "token": {
        "asset_slug": null,
        "contract_address": "0x1111111111111111111111111111111111111111",
        "decimals": null,
        "symbol": null
      },
      "tx_hash": "0x0000000000000000000000000000000000000000000000000000000000000002"
    }
  ],
  "type": "erc20_transfer_search",
  "window": {
    "from_block": 18600000,
    "to_block": 18600500
  }
}"#,
    );
}

#[tokio::test]
async fn bigwig_rpc_error_returns_upstream_provider_error() {
    let Some((base_url, handle)) =
        spawn_bigwig_server(StatusCode::BAD_GATEWAY, bigwig_error_body("rpc_error"))
    else {
        return;
    };
    let client = BigwigClient::new(&base_url, "test-token", 2_000).unwrap();

    let (status, response) = post_json(
        transfers_router_with_bigwig_client(erc20_transfers_enabled_config(), client),
        "/v1/erc20-transfers/search",
        valid_erc20_transfers_request_body(),
    )
    .await;

    assert_public_error(
        status,
        &response,
        StatusCode::BAD_GATEWAY,
        "upstream_provider_error",
    );
    handle.join().unwrap();
}

#[tokio::test]
async fn bigwig_provider_timeout_returns_upstream_provider_timeout() {
    let Some((base_url, handle)) = spawn_bigwig_server(
        StatusCode::GATEWAY_TIMEOUT,
        bigwig_error_body("provider_timeout"),
    ) else {
        return;
    };
    let client = BigwigClient::new(&base_url, "test-token", 2_000).unwrap();

    let (status, response) = post_json(
        transfers_router_with_bigwig_client(erc20_transfers_enabled_config(), client),
        "/v1/erc20-transfers/search",
        valid_erc20_transfers_request_body(),
    )
    .await;

    assert_public_error(
        status,
        &response,
        StatusCode::GATEWAY_TIMEOUT,
        "upstream_provider_timeout",
    );
    handle.join().unwrap();
}

#[tokio::test]
async fn bigwig_range_too_large_returns_window_too_large() {
    let Some((base_url, handle)) = spawn_bigwig_server(
        StatusCode::UNPROCESSABLE_ENTITY,
        bigwig_error_body("range_too_large"),
    ) else {
        return;
    };
    let client = BigwigClient::new(&base_url, "test-token", 2_000).unwrap();

    let (status, response) = post_json(
        transfers_router_with_bigwig_client(erc20_transfers_enabled_config(), client),
        "/v1/erc20-transfers/search",
        valid_erc20_transfers_request_body(),
    )
    .await;

    assert_public_error(
        status,
        &response,
        StatusCode::UNPROCESSABLE_ENTITY,
        "window_too_large",
    );
    handle.join().unwrap();
}

#[tokio::test]
async fn bigwig_extraction_timeout_returns_extraction_timeout() {
    let Some((base_url, handle)) = spawn_bigwig_server(
        StatusCode::GATEWAY_TIMEOUT,
        bigwig_error_body("extraction_timeout"),
    ) else {
        return;
    };
    let client = BigwigClient::new(&base_url, "test-token", 2_000).unwrap();

    let (status, response) = post_json(
        transfers_router_with_bigwig_client(erc20_transfers_enabled_config(), client),
        "/v1/erc20-transfers/search",
        valid_erc20_transfers_request_body(),
    )
    .await;

    assert_public_error(
        status,
        &response,
        StatusCode::GATEWAY_TIMEOUT,
        "extraction_timeout",
    );
    handle.join().unwrap();
}

#[tokio::test]
async fn impossible_bigwig_validation_error_returns_internal_error() {
    let Some((base_url, handle)) = spawn_bigwig_server(
        StatusCode::BAD_REQUEST,
        bigwig_error_body("invalid_extraction_request"),
    ) else {
        return;
    };
    let client = BigwigClient::new(&base_url, "test-token", 2_000).unwrap();

    let (status, response) = post_json(
        transfers_router_with_bigwig_client(erc20_transfers_enabled_config(), client),
        "/v1/erc20-transfers/search",
        valid_erc20_transfers_request_body(),
    )
    .await;

    assert_public_error(
        status,
        &response,
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal_error",
    );
    handle.join().unwrap();
}

#[tokio::test]
async fn bigwig_transport_failure_returns_extraction_unavailable() {
    let Ok(listener) = TcpListener::bind("127.0.0.1:0") else {
        return;
    };
    let closed_url = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let client = BigwigClient::new(&closed_url, "test-token", 2_000).unwrap();

    let (status, response) = post_json(
        transfers_router_with_bigwig_client(erc20_transfers_enabled_config(), client),
        "/v1/erc20-transfers/search",
        valid_erc20_transfers_request_body(),
    )
    .await;

    assert_public_error(
        status,
        &response,
        StatusCode::SERVICE_UNAVAILABLE,
        "extraction_unavailable",
    );
}

#[tokio::test]
async fn request_without_asset_slugs_does_not_require_catalog_or_bigwig_to_exist() {
    let (status, response) = post_json(
        transfers_router_without_repository(erc20_transfers_enabled_config()),
        "/v1/erc20-transfers/search",
        erc20_transfers_request_with_tokens_body(json!({
            "contract_addresses": ["0x1111111111111111111111111111111111111111"]
        })),
    )
    .await;

    assert_public_error(
        status,
        &response,
        StatusCode::SERVICE_UNAVAILABLE,
        "extraction_unavailable",
    );
}

#[tokio::test]
async fn request_with_asset_slugs_requires_catalog() {
    let (status, response) = post_json(
        transfers_router_without_repository(erc20_transfers_enabled_config()),
        "/v1/erc20-transfers/search",
        erc20_transfers_request_with_tokens_body(json!({"asset_slugs": ["usdc"]})),
    )
    .await;

    assert_public_error(
        status,
        &response,
        StatusCode::SERVICE_UNAVAILABLE,
        "asset_contract_mapping_unavailable",
    );
    assert_ne!(response["error"]["code"], "extraction_unavailable");
}

#[tokio::test]
async fn native_asset_slug_rejects_whole_request_before_extraction() {
    let (status, response) = post_json(
        transfers_router(erc20_transfers_enabled_config()),
        "/v1/erc20-transfers/search",
        erc20_transfers_request_with_tokens_body(json!({"asset_slugs": ["ethereum"]})),
    )
    .await;

    assert_public_error(
        status,
        &response,
        StatusCode::UNPROCESSABLE_ENTITY,
        "asset_not_erc20_on_network",
    );
    assert_ne!(response["error"]["code"], "extraction_unavailable");
}

#[tokio::test]
async fn unknown_asset_slug_rejects_whole_request_before_extraction() {
    let (status, response) = post_json(
        transfers_router(erc20_transfers_enabled_config()),
        "/v1/erc20-transfers/search",
        erc20_transfers_request_with_tokens_body(
            json!({"asset_slugs": ["missing-but-syntactically-valid"]}),
        ),
    )
    .await;

    assert_public_error(status, &response, StatusCode::NOT_FOUND, "asset_not_found");
    assert_ne!(response["error"]["code"], "extraction_unavailable");
}

#[tokio::test]
async fn globally_known_asset_unavailable_on_network_rejects_whole_request() {
    let (status, response) = post_json(
        transfers_router(erc20_transfers_enabled_config()),
        "/v1/erc20-transfers/search",
        erc20_transfers_request_with_tokens_body(json!({"asset_slugs": ["mantle"]})),
    )
    .await;

    assert_public_error(
        status,
        &response,
        StatusCode::UNPROCESSABLE_ENTITY,
        "asset_not_available_on_network",
    );
    assert_ne!(response["error"]["code"], "extraction_unavailable");
}

#[tokio::test]
async fn mixed_valid_and_invalid_asset_slug_rejects_whole_request() {
    let (status, response) = post_json(
        transfers_router(erc20_transfers_enabled_config()),
        "/v1/erc20-transfers/search",
        erc20_transfers_request_with_tokens_body(json!({
            "asset_slugs": ["usdc", "ethereum"],
            "contract_addresses": ["0x1111111111111111111111111111111111111111"]
        })),
    )
    .await;

    assert_public_error(
        status,
        &response,
        StatusCode::UNPROCESSABLE_ENTITY,
        "asset_not_erc20_on_network",
    );
    assert_ne!(response["error"]["code"], "extraction_unavailable");
}

#[tokio::test]
async fn duplicate_explicit_and_resolved_address_dedupes_before_limit() {
    let (status, response) = post_json(
        transfers_router(Config {
            erc20_transfers_enabled: true,
            erc20_transfers_max_token_filters: 1,
            bigwig_max_contract_addresses: 20,
            ..Config::default()
        }),
        "/v1/erc20-transfers/search",
        erc20_transfers_request_with_tokens_body(json!({
            "asset_slugs": ["usdc"],
            "contract_addresses": ["0xA0B86991C6218B36C1D19D4A2E9EB0CE3606EB48"]
        })),
    )
    .await;

    assert_public_error(
        status,
        &response,
        StatusCode::SERVICE_UNAVAILABLE,
        "extraction_unavailable",
    );
}

#[tokio::test]
async fn duplicate_explicit_contract_addresses_dedupe_before_limit() {
    let (status, response) = post_json(
        transfers_router_without_repository(Config {
            erc20_transfers_enabled: true,
            erc20_transfers_max_token_filters: 1,
            bigwig_max_contract_addresses: 20,
            ..Config::default()
        }),
        "/v1/erc20-transfers/search",
        erc20_transfers_request_with_tokens_body(json!({
            "contract_addresses": [
                "0xA0B86991C6218B36C1D19D4A2E9EB0CE3606EB48",
                "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
            ]
        })),
    )
    .await;

    assert_public_error(
        status,
        &response,
        StatusCode::SERVICE_UNAVAILABLE,
        "extraction_unavailable",
    );
}

#[tokio::test]
async fn validation_failures_do_not_require_catalog_or_bigwig_to_exist() {
    let mut body = valid_erc20_transfers_request_body();
    body["tokens"]["asset_slugs"] = json!(["USDC"]);

    let (status, response) = post_json(
        transfers_router_without_repository(erc20_transfers_enabled_config()),
        "/v1/erc20-transfers/search",
        body,
    )
    .await;

    assert_public_error(
        status,
        &response,
        StatusCode::BAD_REQUEST,
        "invalid_asset_slug",
    );
}

#[tokio::test]
async fn malformed_json_raw_body_returns_invalid_json() {
    let (status, response) = post_raw(
        transfers_router(erc20_transfers_enabled_config()),
        "/v1/erc20-transfers/search",
        Some("application/json"),
        br#"{"network_slug":"eth-mainnet""#.to_vec(),
    )
    .await;

    assert_public_error(status, &response, StatusCode::BAD_REQUEST, "invalid_json");
}

#[tokio::test]
async fn missing_or_non_json_content_type_returns_invalid_json() {
    for content_type in [None, Some("text/plain")] {
        let (status, response) = post_raw(
            transfers_router(erc20_transfers_enabled_config()),
            "/v1/erc20-transfers/search",
            content_type,
            serde_json::to_vec(&valid_erc20_transfers_request_body()).unwrap(),
        )
        .await;

        assert_public_error(status, &response, StatusCode::BAD_REQUEST, "invalid_json");
    }
}

#[tokio::test]
async fn invalid_requests_return_stable_public_codes() {
    let app = transfers_router(erc20_transfers_enabled_config());
    let cases = [
        (
            Some("application/json"),
            serde_json::to_vec(&json!([])).unwrap(),
            StatusCode::BAD_REQUEST,
            "invalid_json",
        ),
        (
            Some("application/json"),
            serde_json::to_vec(&{
                let mut body = valid_erc20_transfers_request_body();
                body["future"] = json!(true);
                body
            })
            .unwrap(),
            StatusCode::BAD_REQUEST,
            "unknown_field",
        ),
        (
            Some("application/json"),
            serde_json::to_vec(&{
                let mut body = valid_erc20_transfers_request_body();
                body["tokens"]["symbol"] = json!("USDC");
                body
            })
            .unwrap(),
            StatusCode::BAD_REQUEST,
            "unknown_field",
        ),
        (
            Some("application/json"),
            serde_json::to_vec(&{
                let mut body = valid_erc20_transfers_request_body();
                body["window"]["cursor"] = json!("next");
                body
            })
            .unwrap(),
            StatusCode::BAD_REQUEST,
            "unknown_field",
        ),
        (
            Some("application/json"),
            serde_json::to_vec(&{
                let mut body = valid_erc20_transfers_request_body();
                body.as_object_mut().unwrap().remove("network_slug");
                body
            })
            .unwrap(),
            StatusCode::BAD_REQUEST,
            "missing_network_slug",
        ),
        (
            Some("application/json"),
            serde_json::to_vec(&{
                let mut body = valid_erc20_transfers_request_body();
                body["network_slug"] = json!("");
                body
            })
            .unwrap(),
            StatusCode::BAD_REQUEST,
            "missing_network_slug",
        ),
        (
            Some("application/json"),
            serde_json::to_vec(&{
                let mut body = valid_erc20_transfers_request_body();
                body["network_slug"] = json!(null);
                body
            })
            .unwrap(),
            StatusCode::BAD_REQUEST,
            "missing_network_slug",
        ),
        (
            Some("application/json"),
            serde_json::to_vec(&{
                let mut body = valid_erc20_transfers_request_body();
                body["network_slug"] = json!("ETH-MAINNET");
                body
            })
            .unwrap(),
            StatusCode::NOT_FOUND,
            "unsupported_network",
        ),
        (
            Some("application/json"),
            serde_json::to_vec(&{
                let mut body = valid_erc20_transfers_request_body();
                body["network_slug"] = json!("base-mainnet");
                body
            })
            .unwrap(),
            StatusCode::NOT_FOUND,
            "unsupported_network",
        ),
        (
            Some("application/json"),
            serde_json::to_vec(&{
                let mut body = valid_erc20_transfers_request_body();
                body["address"] = json!("0x1234");
                body
            })
            .unwrap(),
            StatusCode::BAD_REQUEST,
            "invalid_address",
        ),
        (
            Some("application/json"),
            serde_json::to_vec(&{
                let mut body = valid_erc20_transfers_request_body();
                body["direction"] = json!("ANY");
                body
            })
            .unwrap(),
            StatusCode::BAD_REQUEST,
            "invalid_direction",
        ),
        (
            Some("application/json"),
            serde_json::to_vec(&{
                let mut body = valid_erc20_transfers_request_body();
                body["direction"] = json!("sideways");
                body
            })
            .unwrap(),
            StatusCode::BAD_REQUEST,
            "invalid_direction",
        ),
        (
            Some("application/json"),
            serde_json::to_vec(&{
                let mut body = valid_erc20_transfers_request_body();
                body.as_object_mut().unwrap().remove("window");
                body
            })
            .unwrap(),
            StatusCode::BAD_REQUEST,
            "invalid_window",
        ),
        (
            Some("application/json"),
            serde_json::to_vec(&{
                let mut body = valid_erc20_transfers_request_body();
                body["window"] = json!({});
                body
            })
            .unwrap(),
            StatusCode::BAD_REQUEST,
            "invalid_window",
        ),
        (
            Some("application/json"),
            serde_json::to_vec(&{
                let mut body = valid_erc20_transfers_request_body();
                body["window"] = json!({
                    "from_block": "18600000",
                    "to_block": 18_600_500
                });
                body
            })
            .unwrap(),
            StatusCode::BAD_REQUEST,
            "invalid_window",
        ),
        (
            Some("application/json"),
            serde_json::to_vec(&{
                let mut body = valid_erc20_transfers_request_body();
                body["window"] = json!({
                    "from_block": 18_600_500,
                    "to_block": 18_600_000
                });
                body
            })
            .unwrap(),
            StatusCode::BAD_REQUEST,
            "invalid_window",
        ),
        (
            Some("application/json"),
            serde_json::to_vec(&{
                let mut body = valid_erc20_transfers_request_body();
                body["window"] = json!({
                    "from_timestamp": "not-a-timestamp",
                    "to_timestamp": "2026-06-25T01:00:00Z"
                });
                body
            })
            .unwrap(),
            StatusCode::BAD_REQUEST,
            "invalid_window",
        ),
        (
            Some("application/json"),
            serde_json::to_vec(&{
                let mut body = valid_erc20_transfers_request_body();
                body["window"] = json!({
                    "from_timestamp": "2026-06-25T02:00:00Z",
                    "to_timestamp": "2026-06-25T01:00:00Z"
                });
                body
            })
            .unwrap(),
            StatusCode::BAD_REQUEST,
            "invalid_window",
        ),
        (
            Some("application/json"),
            serde_json::to_vec(&{
                let mut body = valid_erc20_transfers_request_body();
                body["window"] = json!({
                    "from_block": 18_600_000,
                    "to_block": 18_600_500,
                    "from_timestamp": "2026-06-25T00:00:00Z",
                    "to_timestamp": "2026-06-25T01:00:00Z"
                });
                body
            })
            .unwrap(),
            StatusCode::BAD_REQUEST,
            "invalid_window",
        ),
        (
            Some("application/json"),
            serde_json::to_vec(&{
                let mut body = valid_erc20_transfers_request_body();
                body["window"] = json!({
                    "lookback_seconds": 0,
                    "to": "latest"
                });
                body
            })
            .unwrap(),
            StatusCode::BAD_REQUEST,
            "invalid_window",
        ),
        (
            Some("application/json"),
            serde_json::to_vec(&{
                let mut body = valid_erc20_transfers_request_body();
                body["tokens"]["asset_slugs"] = json!(["USDC"]);
                body
            })
            .unwrap(),
            StatusCode::BAD_REQUEST,
            "invalid_asset_slug",
        ),
        (
            Some("application/json"),
            serde_json::to_vec(&{
                let mut body = valid_erc20_transfers_request_body();
                body["tokens"]["asset_slugs"] = json!([""]);
                body
            })
            .unwrap(),
            StatusCode::BAD_REQUEST,
            "invalid_asset_slug",
        ),
        (
            Some("application/json"),
            serde_json::to_vec(&{
                let mut body = valid_erc20_transfers_request_body();
                body["tokens"]["asset_slugs"] = json!(null);
                body
            })
            .unwrap(),
            StatusCode::BAD_REQUEST,
            "invalid_asset_slug",
        ),
        (
            Some("application/json"),
            serde_json::to_vec(&{
                let mut body = valid_erc20_transfers_request_body();
                body["tokens"]["contract_addresses"] = json!(["0x1234"]);
                body
            })
            .unwrap(),
            StatusCode::BAD_REQUEST,
            "invalid_contract_address",
        ),
        (
            Some("application/json"),
            serde_json::to_vec(&{
                let mut body = valid_erc20_transfers_request_body();
                body["tokens"]["contract_addresses"] = json!([""]);
                body
            })
            .unwrap(),
            StatusCode::BAD_REQUEST,
            "invalid_contract_address",
        ),
        (
            Some("application/json"),
            serde_json::to_vec(&{
                let mut body = valid_erc20_transfers_request_body();
                body["tokens"]["contract_addresses"] = json!(null);
                body
            })
            .unwrap(),
            StatusCode::BAD_REQUEST,
            "invalid_contract_address",
        ),
    ];

    for (content_type, body, expected_status, expected_code) in cases {
        let (status, response) = post_raw(
            app.clone(),
            "/v1/erc20-transfers/search",
            content_type,
            body,
        )
        .await;

        assert_public_error(status, &response, expected_status, expected_code);
        assert_ne!(response["error"]["code"], "extraction_unavailable");
    }
}

#[tokio::test]
async fn too_many_token_filters_uses_configured_public_limit() {
    let (status, response) = post_json(
        transfers_router(Config {
            erc20_transfers_enabled: true,
            erc20_transfers_max_token_filters: 1,
            bigwig_max_contract_addresses: 20,
            ..Config::default()
        }),
        "/v1/erc20-transfers/search",
        erc20_transfers_request_with_tokens_body(json!({
            "asset_slugs": ["usdc"],
            "contract_addresses": ["0x1111111111111111111111111111111111111111"]
        })),
    )
    .await;

    assert_public_error(
        status,
        &response,
        StatusCode::UNPROCESSABLE_ENTITY,
        "too_many_token_filters",
    );
}

fn bigwig_success_body() -> Value {
    json!({
        "extractor": "evm_erc20_transfers_by_address",
        "network_slug": "eth-mainnet",
        "address": "0xabc0000000000000000000000000000000000000",
        "direction": "any",
        "window_kind": "block",
        "from_block": 18_600_000,
        "to_block": 18_600_500,
        "latest_block": 18_600_500,
        "safe_block": 18_600_488,
        "finality": {
            "status": "mixed",
            "safe_block": 18_600_488,
            "latest_block": 18_600_500,
            "reorg_risk": true,
            "policy": "confirmation_lag",
            "confirmation_lag": 12
        },
        "truncated": true,
        "rows_extracted": 2,
        "results": [
            {
                "block_number": 18_600_001,
                "tx_hash": "0x0000000000000000000000000000000000000000000000000000000000000001",
                "log_index": 12,
                "token": "0xA0B86991C6218B36C1D19D4A2E9EB0CE3606EB48",
                "from": "0xABC0000000000000000000000000000000000000",
                "to": "0x2222222222222222222222222222222222222222",
                "value": "12500000"
            },
            {
                "block_number": 18_600_002,
                "tx_hash": "0x0000000000000000000000000000000000000000000000000000000000000002",
                "log_index": 13,
                "token": "0x1111111111111111111111111111111111111111",
                "from": "0x3333333333333333333333333333333333333333",
                "to": "0xABC0000000000000000000000000000000000000",
                "value": "1000000"
            }
        ]
    })
}

fn bigwig_error_body(code: &str) -> Value {
    json!({
        "error": {
            "code": code,
            "message": "Bigwig-owned message must not leak.",
            "details": {}
        }
    })
}

fn spawn_bigwig_server(
    status: StatusCode,
    body: Value,
) -> Option<(String, thread::JoinHandle<String>)> {
    let listener = match TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => listener,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return None,
        Err(error) => panic!("failed to bind HTTP route Bigwig test server: {error}"),
    };
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let request = read_http_request(&mut stream);
        write_json_response(&mut stream, status, body);
        request
    });

    Some((base_url, handle))
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

fn split_request(request: &str) -> (&str, &str) {
    request.split_once("\r\n\r\n").unwrap()
}

fn assert_header(headers: &str, name: &str, expected_value: &str) {
    let expected = format!("{name}: {expected_value}");
    assert!(
        headers
            .lines()
            .any(|line| line.eq_ignore_ascii_case(&expected)),
        "missing header {expected}; headers were:\n{headers}"
    );
}

fn assert_json_snapshot(value: &Value, expected: &str) {
    let actual = serde_json::to_string_pretty(value).unwrap();
    assert_eq!(actual, expected);
}
