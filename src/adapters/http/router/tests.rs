use std::{
    io::{Read, Write},
    net::TcpListener,
};

use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use serde_json::Value;
use tower::ServiceExt;

use super::*;
use crate::test_utils::fixtures::global_assets::sample_assets;
use crate::{
    adapters::postgres::global_assets::GlobalAssetRepository,
    adapters::price_indexer::PriceIndexerClient,
    config::{Config, PublicApiSurface},
};
use crate::{domain::global_assets::GlobalAsset, state::AppState};

fn test_app() -> Router {
    build_router(AppState::with_asset_repository(
        Config::default(),
        GlobalAssetRepository::in_memory(sample_assets()),
    ))
}

fn beta_config() -> Config {
    Config {
        public_api_surface: PublicApiSurface::Beta,
        ..Config::default()
    }
}

fn test_app_with_price_indexer(price_indexer_url: &str, timeout_ms: u64) -> Router {
    let price_indexer_client =
        PriceIndexerClient::new(price_indexer_url, "test-token", timeout_ms).unwrap();

    build_router(AppState {
        config: Config::default(),
        version: env!("CARGO_PKG_VERSION"),
        database_pool: None,
        asset_repository: Some(GlobalAssetRepository::in_memory(sample_assets())),
        price_indexer_client: Some(price_indexer_client),
        dis_client: None,
        bigwig_client: None,
    })
}

#[tokio::test]
async fn balance_routes_are_registered_with_expected_methods() {
    for uri in ["/v1/balances", "/v1/balances/bulk"] {
        let response = test_app()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(uri)
                    .header("content-type", "application/json")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{uri}");
    }

    let response = test_app()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/balances")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn unknown_route_returns_stable_not_found() {
    let response = test_app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/not-a-route")
                .header("user-agent", "route-smoke-test")
                .header("x-request-id", "request-123")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn beta_surface_keeps_balance_and_health_routes_active() {
    let app = build_router(AppState::new(beta_config()));

    let health_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(health_response.status(), StatusCode::OK);

    for uri in ["/v1/balances", "/v1/balances/bulk"] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(uri)
                    .header("content-type", "application/json")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{uri}");
    }

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/balances")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn beta_surface_keeps_transfer_search_feature_gated() {
    let disabled_response = build_router(AppState::new(beta_config()))
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/erc20-transfers/search")
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(disabled_response.status(), StatusCode::NOT_FOUND);

    let enabled_response = build_router(AppState::new(Config {
        public_api_surface: PublicApiSurface::Beta,
        erc20_transfers_enabled: true,
        ..Config::default()
    }))
    .oneshot(
        Request::builder()
            .method("POST")
            .uri("/v1/erc20-transfers/search")
            .header("content-type", "application/json")
            .body(Body::from("{}"))
            .unwrap(),
    )
    .await
    .unwrap();

    assert_eq!(enabled_response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn beta_surface_returns_endpoint_disabled_for_known_non_beta_routes() {
    let app = build_router(AppState::new(beta_config()));

    for uri in [
        "/v1/status",
        "/v1/assets",
        "/v1/assets/resolve",
        "/v1/assets/bitcoin",
        "/v1/assets/bitcoin/signal/price-stats",
        "/v1/assets/bitcoin/signal/price-trend",
        "/v1/search-engine",
    ] {
        let response = app
            .clone()
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN, "{uri}");

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["ok"], false, "{uri}");
        assert_eq!(json["error"]["code"], "endpoint_disabled", "{uri}");
        assert_eq!(
            json["error"]["message"], "This endpoint is currently disabled for the Beta release.",
            "{uri}"
        );
    }
}

#[tokio::test]
async fn removed_prediction_routes_are_unmatched_in_alpha_and_beta() {
    for app in [test_app(), build_router(AppState::new(beta_config()))] {
        for uri in [
            "/v1/predictions/fifa-world-cup/winner",
            "/v1/predictions/fifa-world-cup/mexico",
        ] {
            let response = app
                .clone()
                .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
                .await
                .unwrap();

            assert_eq!(response.status(), StatusCode::NOT_FOUND, "{uri}");
            assert!(
                response.headers().get("deprecation").is_none(),
                "{uri} must not retain prediction deprecation metadata"
            );
        }
    }
}

#[tokio::test]
async fn beta_surface_treats_head_as_disabled_for_known_get_routes() {
    let response = build_router(AppState::new(beta_config()))
        .oneshot(
            Request::builder()
                .method("HEAD")
                .uri("/v1/assets/bitcoin")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok()),
        Some("application/json")
    );
}

#[tokio::test]
async fn beta_surface_preserves_not_found_for_unknown_routes() {
    let app = build_router(AppState::new(beta_config()));

    for uri in ["/v1/not-a-route", "/definitely-not-a-route"] {
        let response = app
            .clone()
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{uri}");
    }
}

#[test]
fn production_caddy_forwards_all_v1_methods_to_axum() {
    let caddyfile = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/infra/caddy/Caddyfile"
    ));

    assert!(caddyfile.contains("path /health /v1/*"));
    assert!(!caddyfile.contains("method GET"));
}

#[tokio::test]
async fn health_returns_stable_contract() {
    let app = build_router(AppState::new(Config::default()));

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["ok"], true);
    assert_eq!(json["service"], "iron-burrow-mother-api");
    assert_eq!(json["mascot"], "Capitan Sousa");
    assert_eq!(json["message"], "Happy squirrel, systems nominal.");
}

#[tokio::test]
async fn status_returns_default_informational_state() {
    let app = build_router(AppState::new(Config::default()));

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["ok"], true);
    assert_eq!(json["service"], "iron-burrow-mother-api");
    assert_eq!(json["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(json["environment"], "development");
    assert_eq!(json["mascot"], "Capitan Sousa");
    assert_eq!(json["message"], "Mother API is online.");
    assert_eq!(json["checks"]["app"], "ok");
    assert_eq!(json["checks"]["database"], "skipped");
    assert_eq!(json["checks"]["price_indexer"], "not_configured");
    assert_eq!(json["checks"]["dis"], "not_configured");
    assert_eq!(json["checks"]["evm_indexer"], "not_connected");
}

#[tokio::test]
async fn assets_returns_default_limited_list() {
    let json = assets_json("/v1/assets").await;

    assert_eq!(json["ok"], true);
    assert_eq!(json["type"], "assets");
    assert_eq!(json["limit"], 100);
    assert_eq!(json["count"], 21);
    assert_eq!(json["assets"][0]["asset_id"], "bitcoin");
    assert_eq!(json["assets"][0]["canonical_path"], "/assets/bitcoin");
    assert_eq!(json["assets"][0]["price"]["status"], "unavailable");
    assert!(json["assets"][0]["price"]["price"].is_null());
    assert!(json["assets"][0]["id"].is_null());
    assert!(json["assets"][0]["aliases"].is_null());
}

#[tokio::test]
async fn assets_honors_limit_query_parameter() {
    let json = assets_json("/v1/assets?limit=2").await;

    assert_eq!(json["limit"], 2);
    assert_eq!(json["count"], 2);
    assert_eq!(json["assets"].as_array().unwrap().len(), 2);
    assert_eq!(json["assets"][0]["asset_id"], "bitcoin");
    assert_eq!(json["assets"][1]["asset_id"], "ethereum");
}

#[tokio::test]
async fn assets_list_requests_batch_price_enrichment_by_slug() {
    let Some((price_indexer_url, request_handle)) = spawn_batch_price_indexer() else {
        return;
    };
    let price_indexer_client =
        PriceIndexerClient::new(&price_indexer_url, "test-token", 2000).unwrap();
    let repository = GlobalAssetRepository::in_memory(vec![
        GlobalAsset {
            id: "test-bitcoin".to_string(),
            slug: "bitcoin".to_string(),
            symbol: "BTC".to_string(),
            name: "Bitcoin".to_string(),
            category: "crypto".to_string(),
            canonical_path: "/assets/bitcoin".to_string(),
            aliases: vec!["btc".to_string()],
            sort_order: 1,
        },
        GlobalAsset {
            id: "test-ethereum".to_string(),
            slug: "ethereum".to_string(),
            symbol: "ETH".to_string(),
            name: "Ethereum".to_string(),
            category: "crypto".to_string(),
            canonical_path: "/assets/ethereum".to_string(),
            aliases: vec!["eth".to_string()],
            sort_order: 2,
        },
    ]);
    let app = build_router(AppState {
        config: Config::default(),
        version: env!("CARGO_PKG_VERSION"),
        database_pool: None,
        asset_repository: Some(repository),
        price_indexer_client: Some(price_indexer_client),
        dis_client: None,
        bigwig_client: None,
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/assets?limit=2")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["assets"][0]["asset_id"], "bitcoin");
    assert_eq!(json["assets"][0]["price"]["status"], "unavailable");
    assert!(json["assets"][0]["price"]["price"].is_null());
    assert_eq!(json["assets"][1]["asset_id"], "ethereum");
    assert_eq!(json["assets"][1]["price"]["status"], "available");
    assert_eq!(json["assets"][1]["price"]["price"], "2500.123456");
    assert_eq!(json["assets"][1]["price"]["quote_currency"], "USD");
    assert_eq!(json["assets"][1]["price"]["source_type"], "chainlink");

    let request = request_handle.await.unwrap();
    assert!(request.starts_with("POST /prices/latest/batch "));
    assert!(request.contains("\"slugs\":[\"bitcoin\",\"ethereum\"]"));
    assert!(request.contains("\"quoteCurrency\":\"USD\""));
    assert!(!request.contains("symbol"));
}

#[tokio::test]
async fn assets_clamps_limit_above_maximum() {
    let json = assets_json("/v1/assets?limit=9999").await;

    assert_eq!(json["limit"], 1000);
    assert_eq!(json["count"], 21);
}

#[tokio::test]
async fn assets_rejects_invalid_limit() {
    for uri in [
        "/v1/assets?limit=0",
        "/v1/assets?limit=-1",
        "/v1/assets?limit=abc",
    ] {
        let response = test_app()
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["ok"], false);
        assert_eq!(json["error"]["code"], "invalid_limit");
    }
}

#[tokio::test]
async fn assets_reports_database_unavailable_when_repository_is_missing() {
    let response = build_router(AppState::new(Config::default()))
        .oneshot(
            Request::builder()
                .uri("/v1/assets")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["ok"], false);
    assert_eq!(json["error"]["code"], "database_unavailable");
}

#[tokio::test]
async fn asset_detail_returns_native_asset_network_map() {
    let json = assets_json("/v1/assets/bitcoin").await;

    assert_eq!(json["ok"], true);
    assert_eq!(json["type"], "asset");
    assert_eq!(json["asset"]["asset_id"], "bitcoin");
    assert_eq!(json["asset"]["symbol"], "BTC");
    assert_eq!(json["asset"]["canonical_path"], "/assets/bitcoin");
    assert_eq!(json["price"]["status"], "unavailable");
    assert!(json["price"]["price"].is_null());
    assert!(json.get("chain_maps").is_none());
    assert_eq!(
        json["asset_network_maps"][0]["network_slug"],
        "bitcoin-mainnet"
    );
    assert_eq!(
        json["asset_network_maps"][0]["network_name"],
        "Bitcoin Mainnet"
    );
    assert_eq!(
        json["asset_network_maps"][0]["caip2"],
        "bip122:000000000019d6689c085ae165831e93"
    );
    assert_eq!(json["asset_network_maps"][0]["is_native"], true);
    assert!(json["asset_network_maps"][0]["address"].is_null());
    assert!(json["asset_network_maps"][0].get("family").is_none());
    assert!(json["asset_network_maps"][0].get("chain_id").is_none());
    assert!(json.get("signals").is_none());
    assert!(json.get("enrichment_errors").is_none());
}

#[tokio::test]
async fn asset_detail_returns_deployed_asset_network_maps() {
    let json = assets_json("/v1/assets/usdc").await;
    let asset_network_maps = json["asset_network_maps"].as_array().unwrap();

    assert!(json.get("chain_maps").is_none());
    assert_eq!(asset_network_maps.len(), 5);
    assert_eq!(asset_network_maps[0]["network_slug"], "eth-mainnet");
    assert_eq!(
        asset_network_maps[0]["address"],
        "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
    );
    assert_eq!(asset_network_maps[0]["is_native"], false);
    assert_eq!(asset_network_maps[1]["network_slug"], "arbitrum-mainnet");
    assert_eq!(asset_network_maps[2]["network_slug"], "base-mainnet");
    assert_eq!(asset_network_maps[3]["network_slug"], "near");
    assert_eq!(asset_network_maps[4]["network_slug"], "mantle-mainnet");
}

#[tokio::test]
async fn asset_detail_requests_price_enrichment_by_slug() {
    let Some((price_indexer_url, request_handle)) = spawn_price_indexer() else {
        return;
    };
    let price_indexer_client =
        PriceIndexerClient::new(&price_indexer_url, "test-token", 2000).unwrap();
    let repository = GlobalAssetRepository::in_memory(vec![GlobalAsset {
        id: "test-usd-coin".to_string(),
        slug: "usd-coin".to_string(),
        symbol: "USDC".to_string(),
        name: "USD Coin".to_string(),
        category: "crypto".to_string(),
        canonical_path: "/assets/usd-coin".to_string(),
        aliases: vec!["usdc".to_string()],
        sort_order: 10,
    }]);
    let app = build_router(AppState {
        config: Config::default(),
        version: env!("CARGO_PKG_VERSION"),
        database_pool: None,
        asset_repository: Some(repository),
        price_indexer_client: Some(price_indexer_client),
        dis_client: None,
        bigwig_client: None,
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/assets/usd-coin")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["asset"]["asset_id"], "usd-coin");
    assert_eq!(json["asset"]["symbol"], "USDC");
    assert_eq!(json["price"]["status"], "available");

    let request = request_handle.await.unwrap();
    assert!(request.starts_with("GET /prices/latest?slug=usd-coin&quoteCurrency=USD "));
    assert!(!request.contains("symbol="));
}

#[tokio::test]
async fn asset_detail_forwards_quote_currency_to_latest_price() {
    let Some((price_indexer_url, request_handle)) =
        spawn_multi_price_indexer(vec![(StatusCode::OK, latest_price_body_with_quote("MXN"))])
    else {
        return;
    };
    let app = test_app_with_price_indexer(&price_indexer_url, 2000);

    let (status, json) = app_json(app, "/v1/assets/ethereum?quoteCurrency=mxn").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["price"]["status"], "available");
    assert_eq!(json["price"]["quote_currency"], "MXN");
    assert_eq!(json["price"]["source_type"], "fx-derived");
    assert_eq!(json["price"]["is_derived"], true);

    let requests = request_handle.await.unwrap();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].starts_with("GET /prices/latest?slug=ethereum&quoteCurrency=MXN "));
}

#[tokio::test]
async fn asset_detail_rejects_unsupported_quote_currency_before_upstream() {
    for uri in [
        "/v1/assets/ethereum?quoteCurrency=eur",
        "/v1/assets/ethereum?quoteCurrency=",
    ] {
        let (status, json) = app_json(test_app(), uri).await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(json["ok"], false);
        assert_eq!(json["error"]["code"], "invalid_request");
    }
}

#[tokio::test]
async fn asset_detail_reports_disabled_requested_enrichments_without_failing_page() {
    let json = assets_json(
        "/v1/assets/bitcoin?include=priceStats,priceTrend,priceSeries&quoteCurrency=MXN",
    )
    .await;

    assert_eq!(json["ok"], true);
    assert_eq!(json["asset"]["asset_id"], "bitcoin");
    assert_eq!(json["price"]["status"], "unavailable");
    let signals = json["signals"].as_object().unwrap();
    assert!(signals.get("price_stats").unwrap().is_null());
    assert!(signals.get("price_trend").unwrap().is_null());
    assert!(signals.get("price_series").unwrap().is_null());
    assert_eq!(signals.len(), 3);
    assert_eq!(json["enrichment_errors"].as_array().unwrap().len(), 3);
    assert_eq!(
        json["enrichment_errors"][0]["code"],
        "price_indexer_unavailable"
    );
}

#[tokio::test]
async fn asset_detail_without_requested_enrichments_survives_disabled_price_indexer() {
    let json = assets_json("/v1/assets/usdc").await;

    assert_eq!(json["ok"], true);
    assert_eq!(json["asset"]["asset_id"], "usdc");
    assert_eq!(json["price"]["status"], "unavailable");
    assert!(json["price"]["price"].is_null());
    assert!(!json["asset_network_maps"].as_array().unwrap().is_empty());
    assert!(json.get("chain_maps").is_none());
    assert!(json.get("signals").is_none());
    assert!(json.get("enrichment_errors").is_none());
}

#[tokio::test]
async fn asset_detail_treats_invalid_enrichment_params_as_partial_errors() {
    let json =
        assets_json("/v1/assets/bitcoin?include=priceStats,priceSeries&window=1h&granularity=1h")
            .await;

    assert_eq!(json["ok"], true);
    let signals = json["signals"].as_object().unwrap();
    assert!(signals.get("price_stats").unwrap().is_null());
    assert!(signals.get("price_series").unwrap().is_null());
    assert!(!signals.contains_key("price_trend"));
    assert_eq!(signals.len(), 2);
    assert_eq!(json["enrichment_errors"].as_array().unwrap().len(), 2);
    assert_eq!(json["enrichment_errors"][0]["code"], "invalid_request");
    assert_eq!(json["enrichment_errors"][0]["source"], "price_stats");
    assert_eq!(json["enrichment_errors"][1]["source"], "price_series");
}

#[tokio::test]
async fn asset_detail_ignores_unknown_include_tokens() {
    let json = assets_json("/v1/assets/bitcoin?include=unknown,alsoBad").await;

    assert_eq!(json["ok"], true);
    assert!(json.get("signals").is_none());
    assert!(json.get("enrichment_errors").is_none());
}

#[tokio::test]
async fn asset_detail_includes_requested_price_signals() {
    let stats_body = serde_json::json!({
        "slug": "ethereum",
        "assetId": "00000000-0000-0000-0000-000000000001",
        "quoteCurrency": "MXN",
        "window": "24h",
        "granularity": "1h",
        "percentChange": "0.020367",
        "warnings": ["low_series_coverage"],
        "futureInformationalField": {"preserved": true}
    })
    .to_string();
    let trend_body = serde_json::json!({
        "slug": "ethereum",
        "assetId": "00000000-0000-0000-0000-000000000001",
        "quoteCurrency": "MXN",
        "window": "24h",
        "granularity": "1h",
        "direction": "up",
        "confidence": "medium",
        "warnings": []
    })
    .to_string();
    let series_body = serde_json::json!({
        "assetId": "00000000-0000-0000-0000-000000000001",
        "quoteCurrency": "MXN",
        "window": "24h",
        "granularity": "1h",
        "points": [
            {
                "bucketStart": "2026-06-01T11:00:00.000Z",
                "price": "3812.45",
                "status": "observed"
            }
        ],
        "meta": {
            "expectedBucketCount": 24,
            "sampleCount": 1
        }
    })
    .to_string();
    let Some((price_indexer_url, request_handle)) = spawn_multi_price_indexer(vec![
        (StatusCode::OK, latest_price_body_with_quote("MXN")),
        (StatusCode::OK, stats_body),
        (StatusCode::OK, trend_body),
        (StatusCode::OK, series_body),
    ]) else {
        return;
    };
    let app = test_app_with_price_indexer(&price_indexer_url, 2000);

    let (status, json) = app_json(
        app,
        "/v1/assets/ethereum?include=priceStats,priceTrend,priceSeries&quoteCurrency=mxn&window=24h&granularity=1h&range=legacy&resolution=bad&asOf=2026-06-02T00:00:00Z",
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["ok"], true);
    assert_eq!(json["price"]["status"], "available");
    assert_eq!(json["price"]["quote_currency"], "MXN");
    assert_eq!(json["price"]["is_derived"], true);
    assert_eq!(json["signals"]["price_stats"]["percentChange"], "0.020367");
    assert_eq!(
        json["signals"]["price_stats"]["warnings"][0],
        "low_series_coverage"
    );
    assert_eq!(
        json["signals"]["price_stats"]["futureInformationalField"]["preserved"],
        true
    );
    assert_eq!(json["signals"]["price_trend"]["direction"], "up");
    assert_eq!(
        json["signals"]["price_series"]["points"][0]["price"],
        "3812.45"
    );
    assert_eq!(json["signals"]["price_series"]["meta"]["sampleCount"], 1);
    assert_eq!(json["enrichment_errors"].as_array().unwrap().len(), 0);

    let requests = request_handle.await.unwrap();
    assert_eq!(requests.len(), 4);
    assert!(requests[0].starts_with("GET /prices/latest?slug=ethereum&quoteCurrency=MXN "));
    assert!(requests[1].starts_with(
        "GET /prices/stats?slug=ethereum&quoteCurrency=MXN&window=24h&granularity=1h "
    ));
    assert!(requests[2].starts_with(
        "GET /prices/trend?slug=ethereum&quoteCurrency=MXN&window=24h&granularity=1h "
    ));
    assert!(requests[3].starts_with(
        "GET /prices/series?slug=ethereum&quoteCurrency=MXN&window=24h&granularity=1h "
    ));
    for request in requests {
        assert_no_legacy_signal_params(&request);
    }
}

#[tokio::test]
async fn asset_detail_isolates_failed_enrichments() {
    let stats_body = serde_json::json!({
        "slug": "bitcoin",
        "quoteCurrency": "USD",
        "window": "24h",
        "granularity": "1h",
        "warnings": []
    })
    .to_string();
    let trend_error_body = serde_json::json!({
        "error": {
            "code": "INTERNAL_ERROR",
            "message": "Upstream-owned message."
        }
    })
    .to_string();
    let Some((price_indexer_url, request_handle)) = spawn_multi_price_indexer(vec![
        (StatusCode::OK, latest_price_body()),
        (StatusCode::OK, stats_body),
        (StatusCode::INTERNAL_SERVER_ERROR, trend_error_body),
    ]) else {
        return;
    };
    let app = test_app_with_price_indexer(&price_indexer_url, 2000);

    let (status, json) = app_json(app, "/v1/assets/bitcoin?include=priceStats,priceTrend").await;

    assert_eq!(status, StatusCode::OK);
    assert!(json["signals"]["price_stats"].is_object());
    assert!(json["signals"]["price_trend"].is_null());
    assert_eq!(json["enrichment_errors"].as_array().unwrap().len(), 1);
    assert_eq!(json["enrichment_errors"][0]["source"], "price_trend");
    assert_eq!(json["enrichment_errors"][0]["code"], "price_indexer_error");
    assert_ne!(
        json["enrichment_errors"][0]["message"],
        "Upstream-owned message."
    );

    let requests = request_handle.await.unwrap();
    assert_eq!(requests.len(), 3);
}

#[tokio::test]
async fn asset_detail_maps_malformed_enrichment_to_partial_invalid_response_error() {
    let stats_body = serde_json::json!({
        "slug": "bitcoin",
        "quoteCurrency": "USD",
        "window": "24h",
        "granularity": "1h",
        "warnings": []
    })
    .to_string();
    let Some((price_indexer_url, request_handle)) = spawn_multi_price_indexer(vec![
        (StatusCode::OK, latest_price_body()),
        (StatusCode::OK, stats_body),
        (StatusCode::OK, "[]".to_string()),
    ]) else {
        return;
    };
    let app = test_app_with_price_indexer(&price_indexer_url, 2000);

    let (status, json) = app_json(app, "/v1/assets/bitcoin?include=priceStats,priceTrend").await;

    assert_eq!(status, StatusCode::OK);
    assert!(json["signals"]["price_stats"].is_object());
    assert!(json["signals"]["price_trend"].is_null());
    assert_eq!(json["enrichment_errors"].as_array().unwrap().len(), 1);
    assert_eq!(json["enrichment_errors"][0]["source"], "price_trend");
    assert_eq!(
        json["enrichment_errors"][0]["code"],
        "upstream_invalid_response"
    );

    let requests = request_handle.await.unwrap();
    assert_eq!(requests.len(), 3);
}

#[tokio::test]
async fn asset_detail_maps_missing_signal_to_partial_not_available_error() {
    let stats_body = serde_json::json!({
        "slug": "bitcoin",
        "quoteCurrency": "USD",
        "window": "24h",
        "granularity": "1h",
        "warnings": []
    })
    .to_string();
    let trend_error_body = serde_json::json!({
        "error": {
            "code": "NOT_FOUND",
            "message": "Upstream-owned message."
        }
    })
    .to_string();
    let Some((price_indexer_url, request_handle)) = spawn_multi_price_indexer(vec![
        (StatusCode::OK, latest_price_body()),
        (StatusCode::OK, stats_body),
        (StatusCode::NOT_FOUND, trend_error_body),
    ]) else {
        return;
    };
    let app = test_app_with_price_indexer(&price_indexer_url, 2000);

    let (status, json) = app_json(app, "/v1/assets/bitcoin?include=priceStats,priceTrend").await;

    assert_eq!(status, StatusCode::OK);
    assert!(json["signals"]["price_stats"].is_object());
    assert!(json["signals"]["price_trend"].is_null());
    assert_eq!(json["enrichment_errors"].as_array().unwrap().len(), 1);
    assert_eq!(json["enrichment_errors"][0]["source"], "price_trend");
    assert_eq!(json["enrichment_errors"][0]["code"], "signal_not_available");
    assert_ne!(
        json["enrichment_errors"][0]["message"],
        "Upstream-owned message."
    );

    let requests = request_handle.await.unwrap();
    assert_eq!(requests.len(), 3);
}

#[tokio::test]
async fn asset_detail_reports_not_found_for_unknown_slug() {
    let response = test_app()
        .oneshot(
            Request::builder()
                .uri("/v1/assets/does-not-exist")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["ok"], false);
    assert_eq!(json["error"]["code"], "asset_not_found");
    assert_eq!(json["error"]["message"], "Asset was not found.");
}

#[tokio::test]
async fn asset_detail_reports_database_unavailable_when_repository_is_missing() {
    let response = build_router(AppState::new(Config::default()))
        .oneshot(
            Request::builder()
                .uri("/v1/assets/bitcoin")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["ok"], false);
    assert_eq!(json["error"]["code"], "database_unavailable");
}

#[tokio::test]
async fn price_stats_signal_maps_query_and_preserves_raw_response() {
    let body = serde_json::json!({
        "slug": "ethereum",
        "assetId": "00000000-0000-0000-0000-000000000001",
        "quoteCurrency": "MXN",
        "window": "24h",
        "granularity": "1h",
        "from": "2026-06-01T11:00:00.000Z",
        "to": "2026-06-02T11:00:00.000Z",
        "expectedBucketCount": 24,
        "sampleCount": 20,
        "carryForwardBucketCount": 2,
        "missingBucketCount": 2,
        "coverageRatio": "0.833333",
        "firstPrice": "3812.45",
        "lastPrice": "3890.10",
        "minPrice": "3812.45",
        "maxPrice": "3890.10",
        "meanPrice": "3845.55",
        "medianPrice": "3840.00",
        "sampleStdDev": "12.340000",
        "coefficientOfVariation": "0.003210",
        "absoluteChange": "77.65",
        "percentChange": "0.020367",
        "minTimestamp": "2026-06-01T13:00:00.000Z",
        "maxTimestamp": "2026-06-02T10:00:00.000Z",
        "warnings": ["low_series_coverage", "custom_future_warning"],
        "futureInformationalField": {"preserved": true}
    });
    let Some((price_indexer_url, request_handle)) =
        spawn_signal_price_indexer(StatusCode::OK, body.to_string())
    else {
        return;
    };
    let app = test_app_with_price_indexer(&price_indexer_url, 2000);

    let (status, json) = app_json(
        app,
        "/v1/assets/ethereum/signal/price-stats?quoteCurrency=mxn&window=24h&granularity=1h&range=legacy&resolution=bad&asOf=2026-06-02T00:00:00Z",
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["ok"], true);
    assert_eq!(json["type"], "price_stats");
    assert_eq!(json["signal"]["percentChange"], "0.020367");
    assert_eq!(
        json["signal"]["warnings"],
        serde_json::json!(["low_series_coverage", "custom_future_warning"])
    );
    assert_eq!(
        json["signal"]["futureInformationalField"]["preserved"],
        true
    );

    let request = request_handle.await.unwrap();
    assert!(request.starts_with(
        "GET /prices/stats?slug=ethereum&quoteCurrency=MXN&window=24h&granularity=1h "
    ));
    assert_no_legacy_signal_params(&request);
}

#[tokio::test]
async fn price_trend_signal_defaults_and_omits_granularity() {
    let body = serde_json::json!({
        "slug": "bitcoin",
        "assetId": "00000000-0000-0000-0000-000000000002",
        "quoteCurrency": "USD",
        "window": "24h",
        "granularity": "1h",
        "from": "2026-06-01T11:00:00.000Z",
        "to": "2026-06-02T11:00:00.000Z",
        "expectedBucketCount": 24,
        "sampleCount": 24,
        "carryForwardBucketCount": 0,
        "missingBucketCount": 0,
        "coverageRatio": "1.000000",
        "firstPrice": "68000.00",
        "lastPrice": "68100.00",
        "percentChange": "0.001471",
        "direction": "up",
        "slope": "0.000061",
        "slopeUnit": "per_hour",
        "rSquared": "0.640000",
        "confidence": "medium",
        "warnings": []
    });
    let Some((price_indexer_url, request_handle)) =
        spawn_signal_price_indexer(StatusCode::OK, body.to_string())
    else {
        return;
    };
    let app = test_app_with_price_indexer(&price_indexer_url, 2000);

    let (status, json) = app_json(app, "/v1/assets/bitcoin/signal/price-trend").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["ok"], true);
    assert_eq!(json["type"], "price_trend");
    assert_eq!(json["signal"]["direction"], "up");

    let request = request_handle.await.unwrap();
    assert!(request.starts_with("GET /prices/trend?slug=bitcoin&quoteCurrency=USD&window=24h "));
    assert!(!request.contains("granularity="));
    assert_no_legacy_signal_params(&request);
}

#[tokio::test]
async fn price_signal_routes_report_missing_price_indexer_config() {
    let (status, json) = app_json(
        build_router(AppState::new(Config::default())),
        "/v1/assets/bitcoin/signal/price-stats",
    )
    .await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(json["ok"], false);
    assert_eq!(json["error"]["code"], "price_indexer_unavailable");
}

#[tokio::test]
async fn price_signal_routes_validate_public_parameters_before_upstream() {
    for uri in [
        "/v1/assets/bitcoin/signal/price-stats?quoteCurrency=eur",
        "/v1/assets/bitcoin/signal/price-stats?window=2h",
        "/v1/assets/bitcoin/signal/price-stats?window=1h&granularity=1h",
        "/v1/assets/bitcoin/signal/price-stats?granularity=",
        "/v1/assets/bitcoin/signal/price-trend?window=30d&granularity=1h",
        "/v1/assets/bitcoin/signal/price-trend?granularity=15m",
    ] {
        let (status, json) = app_json(test_app(), uri).await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(json["ok"], false);
        assert_eq!(json["error"]["code"], "invalid_request");
    }
}

#[tokio::test]
async fn price_signal_routes_map_upstream_error_envelopes() {
    for (upstream_status, upstream_code, expected_status, expected_code) in [
        (
            StatusCode::BAD_REQUEST,
            "INVALID_REQUEST",
            StatusCode::BAD_REQUEST,
            "invalid_request",
        ),
        (
            StatusCode::NOT_FOUND,
            "NOT_FOUND",
            StatusCode::NOT_FOUND,
            "asset_not_found",
        ),
        (
            StatusCode::UNAUTHORIZED,
            "UNAUTHORIZED",
            StatusCode::BAD_GATEWAY,
            "upstream_auth_failed",
        ),
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL_ERROR",
            StatusCode::BAD_GATEWAY,
            "price_indexer_error",
        ),
    ] {
        let body = serde_json::json!({
            "error": {
                "code": upstream_code,
                "message": "Upstream-owned message."
            }
        });
        let Some((price_indexer_url, _request_handle)) =
            spawn_signal_price_indexer(upstream_status, body.to_string())
        else {
            return;
        };
        let app = test_app_with_price_indexer(&price_indexer_url, 2000);

        let (status, json) = app_json(app, "/v1/assets/bitcoin/signal/price-stats").await;

        assert_eq!(status, expected_status);
        assert_eq!(json["ok"], false);
        assert_eq!(json["error"]["code"], expected_code);
        assert_ne!(json["error"]["message"], "Upstream-owned message.");
    }
}

#[tokio::test]
async fn price_signal_routes_map_malformed_upstream_bodies() {
    for body in ["not-json", "[]"] {
        let Some((price_indexer_url, _request_handle)) =
            spawn_signal_price_indexer(StatusCode::OK, body.to_string())
        else {
            return;
        };
        let app = test_app_with_price_indexer(&price_indexer_url, 2000);

        let (status, json) = app_json(app, "/v1/assets/bitcoin/signal/price-trend").await;

        assert_eq!(status, StatusCode::BAD_GATEWAY);
        assert_eq!(json["error"]["code"], "upstream_invalid_response");
    }

    let Some((price_indexer_url, _request_handle)) =
        spawn_signal_price_indexer(StatusCode::INTERNAL_SERVER_ERROR, "not-json".to_string())
    else {
        return;
    };
    let app = test_app_with_price_indexer(&price_indexer_url, 2000);

    let (status, json) = app_json(app, "/v1/assets/bitcoin/signal/price-trend").await;

    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert_eq!(json["error"]["code"], "upstream_invalid_response");
}

#[tokio::test]
async fn price_signal_routes_map_transport_failure_and_timeout() {
    let Ok(listener) = TcpListener::bind("127.0.0.1:0") else {
        return;
    };
    let closed_url = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let app = test_app_with_price_indexer(&closed_url, 2000);

    let (status, json) = app_json(app, "/v1/assets/bitcoin/signal/price-stats").await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(json["error"]["code"], "price_indexer_unavailable");

    let Ok(listener) = TcpListener::bind("127.0.0.1:0") else {
        return;
    };
    let timeout_url = format!("http://{}", listener.local_addr().unwrap());
    let handle = tokio::task::spawn_blocking(move || {
        let (_stream, _) = listener.accept().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(100));
    });
    let app = test_app_with_price_indexer(&timeout_url, 10);

    let (status, json) = app_json(app, "/v1/assets/bitcoin/signal/price-stats").await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(json["error"]["code"], "price_indexer_unavailable");
    handle.await.unwrap();
}

#[tokio::test]
async fn resolve_returns_usdc_for_aliases() {
    for query in ["usdc", "usdc%20coin%20usd", "usd%20coin"] {
        let json = resolve_json(&format!("/v1/assets/resolve?q={query}")).await;

        assert_eq!(json["ok"], true);
        assert_eq!(json["resolved"], true);
        assert_eq!(json["type"], "resolve");
        assert_eq!(json["result"]["kind"], "asset");
        assert_eq!(json["result"]["canonical_path"], "/assets/usdc");
        assert_eq!(json["result"]["resource_url"], "/v1/assets/usdc");
        assert_eq!(json["result"]["asset"]["asset_id"], "usdc");
    }
}

#[tokio::test]
async fn resolve_returns_gold_for_spanish_and_symbol_aliases() {
    for query in ["oro%20de%20ley", "oro", "gold", "xau"] {
        let json = resolve_json(&format!("/v1/assets/resolve?q={query}")).await;

        assert_eq!(json["resolved"], true);
        assert_eq!(json["result"]["canonical_path"], "/assets/gold");
        assert_eq!(json["result"]["asset"]["symbol"], "XAU");
    }
}

#[tokio::test]
async fn resolve_returns_core_crypto_assets() {
    for (query, path) in [
        ("aave", "/assets/aave"),
        ("ausd", "/assets/ausd"),
        ("bitcoin", "/assets/bitcoin"),
        ("btc", "/assets/bitcoin"),
        ("usds", "/assets/usds"),
        ("ethereum", "/assets/ethereum"),
        ("eth", "/assets/ethereum"),
        ("fbtc", "/assets/fbtc"),
        ("gho", "/assets/gho"),
        ("wbtc", "/assets/wrapped-bitcoin"),
        ("wrapped%20bitcoin", "/assets/wrapped-bitcoin"),
        ("mantle", "/assets/mantle"),
        ("mnt", "/assets/mantle"),
        ("mpdao", "/assets/mpdao"),
        ("near%20protocol", "/assets/near"),
        ("stnear", "/assets/stnear"),
        ("usdt", "/assets/usdt"),
        ("usdt0", "/assets/usdt0"),
        ("usde", "/assets/usde"),
        ("weth", "/assets/wrapped-ether"),
        ("cmeth", "/assets/cmeth"),
        ("meth", "/assets/meth"),
        ("susde", "/assets/susde"),
    ] {
        let json = resolve_json(&format!("/v1/assets/resolve?q={query}")).await;

        assert_eq!(json["resolved"], true);
        assert_eq!(json["result"]["canonical_path"], path);
    }
}

#[tokio::test]
async fn resolve_does_not_treat_network_aliases_as_assets() {
    for query in ["base", "base%20mainnet", "coinbase%20base"] {
        let json = resolve_json(&format!("/v1/assets/resolve?q={query}")).await;

        assert_eq!(json["resolved"], false);
        assert_eq!(json["result"]["kind"], "unknown");
        assert!(!json["result"]["recommendations"]
            .as_array()
            .unwrap()
            .is_empty());
    }
}

#[tokio::test]
async fn resolve_unknown_returns_recommendations_without_404() {
    let response = test_app()
        .oneshot(
            Request::builder()
                .uri("/v1/assets/resolve?q=some%20unknown%20thing")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["resolved"], false);
    assert_eq!(json["result"]["kind"], "unknown");
    assert!(json["result"]["resource_url"].is_null());
    assert!(!json["result"]["recommendations"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn resolve_requires_query() {
    let response = test_app()
        .oneshot(
            Request::builder()
                .uri("/v1/assets/resolve")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["ok"], false);
    assert_eq!(json["error"]["code"], "missing_query");
    assert_eq!(json["error"]["message"], "Query parameter `q` is required.");
}

#[tokio::test]
async fn resolve_reports_database_unavailable_when_repository_is_missing() {
    let response = build_router(AppState::new(Config::default()))
        .oneshot(
            Request::builder()
                .uri("/v1/assets/resolve?q=usdc")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["ok"], false);
    assert_eq!(json["error"]["code"], "database_unavailable");
}

#[tokio::test]
async fn resolve_rejects_empty_whitespace_and_overlong_query() {
    for uri in ["/v1/assets/resolve?q=", "/v1/assets/resolve?q=%20%20%20"] {
        let response = test_app()
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    let overlong = "a".repeat(129);
    let response = test_app()
        .oneshot(
            Request::builder()
                .uri(format!("/v1/assets/resolve?q={overlong}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn resolve_normalizes_query_in_response() {
    let json = resolve_json("/v1/assets/resolve?q=%20%20USDC,,,coin---USD%20%20").await;

    assert_eq!(json["query"]["raw"], "USDC,,,coin---USD");
    assert_eq!(json["query"]["normalized"], "usdc coin usd");
    assert_eq!(json["result"]["canonical_path"], "/assets/usdc");
    assert_eq!(json["result"]["resource_url"], "/v1/assets/usdc");
}

async fn resolve_json(uri: &str) -> Value {
    let response = test_app()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap()
}

async fn assets_json(uri: &str) -> Value {
    let response = test_app()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap()
}

async fn app_json(app: Router, uri: &str) -> (StatusCode, Value) {
    let response = app
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json = serde_json::from_slice(&body).unwrap();

    (status, json)
}

fn spawn_price_indexer() -> Option<(String, tokio::task::JoinHandle<String>)> {
    let listener = match TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => listener,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return None,
        Err(error) => panic!("failed to bind test price-indexer: {error}"),
    };
    let url = format!("http://{}", listener.local_addr().unwrap());
    let handle = tokio::task::spawn_blocking(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let request = read_http_request(&mut stream);

        let body = serde_json::json!({
            "assetId": "usd-coin",
            "symbol": "USDC",
            "name": "USD Coin",
            "quoteCurrency": "USD",
            "price": "1.0001",
            "sourceType": "coingecko",
            "sourcePriority": 10,
            "riskCategory": "normal",
            "confidenceScore": 95,
            "confidenceLabel": "high",
            "publishedAt": "2026-05-26T12:00:00Z",
            "recordedAt": "2026-05-26T12:00:05Z",
            "freshnessStatus": "fresh",
            "isFallback": false,
            "isDerived": false,
            "derivationPath": null,
            "staleness": {
                "ageSeconds": 5,
                "isStale": false,
                "warningThresholdSeconds": 300
            }
        })
        .to_string();
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

fn spawn_multi_price_indexer(
    responses: Vec<(StatusCode, String)>,
) -> Option<(String, tokio::task::JoinHandle<Vec<String>>)> {
    let listener = match TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => listener,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return None,
        Err(error) => panic!("failed to bind test price-indexer: {error}"),
    };
    let url = format!("http://{}", listener.local_addr().unwrap());
    let handle = tokio::task::spawn_blocking(move || {
        let mut requests = Vec::new();

        for (status, body) in responses {
            let (mut stream, _) = listener.accept().unwrap();
            requests.push(read_http_request(&mut stream));

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

        requests
    });

    Some((url, handle))
}

fn latest_price_body() -> String {
    latest_price_body_with_quote("USD")
}

fn latest_price_body_with_quote(quote_currency: &str) -> String {
    let is_derived = quote_currency != "USD";
    serde_json::json!({
        "assetId": "test-asset",
        "symbol": "TEST",
        "name": "Test Asset",
        "quoteCurrency": quote_currency,
        "price": "1.0001",
        "sourceType": if is_derived { "fx-derived" } else { "coingecko" },
        "sourcePriority": 10,
        "riskCategory": "normal",
        "confidenceScore": 95,
        "confidenceLabel": "high",
        "publishedAt": "2026-05-26T12:00:00Z",
        "recordedAt": "2026-05-26T12:00:05Z",
        "freshnessStatus": "fresh",
        "isFallback": false,
        "isDerived": is_derived,
        "derivationPath": if is_derived {
            serde_json::json!(["TEST/USD", format!("{quote_currency}/USD")])
        } else {
            serde_json::Value::Null
        },
        "staleness": {
            "ageSeconds": 5,
            "isStale": false,
            "warningThresholdSeconds": 300
        }
    })
    .to_string()
}

fn spawn_batch_price_indexer() -> Option<(String, tokio::task::JoinHandle<String>)> {
    let listener = match TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => listener,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return None,
        Err(error) => panic!("failed to bind test price-indexer: {error}"),
    };
    let url = format!("http://{}", listener.local_addr().unwrap());
    let handle = tokio::task::spawn_blocking(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let request = read_http_request(&mut stream);

        let body = serde_json::json!({
            "quoteCurrency": "USD",
            "requestedCount": 2,
            "uniqueCount": 2,
            "results": [
                {
                    "requestedSlug": "ethereum",
                    "normalizedSlug": "ethereum",
                    "assetId": "ethereum",
                    "slug": "ethereum",
                    "name": "Ethereum",
                    "status": "found",
                    "freshnessStatus": "fresh",
                    "price": {
                        "assetId": "ethereum",
                        "slug": "ethereum",
                        "quoteCurrency": "USD",
                        "price": "2500.123456",
                        "sourceType": "chainlink",
                        "publishedAt": "2026-05-20T12:00:00.000Z",
                        "recordedAt": "2026-05-20T12:00:01.000Z",
                        "freshnessStatus": "fresh",
                        "staleness": {
                            "ageSeconds": 30,
                            "isStale": false,
                            "warningThresholdSeconds": 300
                        }
                    },
                    "error": null
                },
                {
                    "requestedSlug": "bitcoin",
                    "normalizedSlug": "bitcoin",
                    "assetId": "bitcoin",
                    "slug": "bitcoin",
                    "name": "Bitcoin",
                    "status": "unavailable",
                    "freshnessStatus": "unavailable",
                    "price": null,
                    "error": null
                }
            ]
        })
        .to_string();
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

fn spawn_signal_price_indexer(
    status: StatusCode,
    body: String,
) -> Option<(String, tokio::task::JoinHandle<String>)> {
    let listener = match TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => listener,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return None,
        Err(error) => panic!("failed to bind test price-indexer: {error}"),
    };
    let url = format!("http://{}", listener.local_addr().unwrap());
    let handle = tokio::task::spawn_blocking(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let request = read_http_request(&mut stream);
        let reason = status.canonical_reason().unwrap_or("Unknown");
        let response = format!(
            "HTTP/1.1 {} {}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            status.as_u16(),
            reason,
            body.len(),
            body
        );

        stream.write_all(response.as_bytes()).unwrap();

        request
    });

    Some((url, handle))
}

fn assert_no_legacy_signal_params(request: &str) {
    for legacy_param in [
        "range=",
        "resolution=",
        "from=",
        "to=",
        "interval=",
        "sourceType=",
        "limit=",
        "beforeId=",
        "asOf=",
    ] {
        assert!(
            !request.contains(legacy_param),
            "unexpected legacy signal param {legacy_param}"
        );
    }
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

                if name.eq_ignore_ascii_case("content-length") {
                    value.trim().parse::<usize>().ok()
                } else {
                    None
                }
            })
            .unwrap_or(0);
        let request_length = headers_end + 4 + content_length;

        if request.len() >= request_length {
            break;
        }
    }

    String::from_utf8(request).unwrap()
}
