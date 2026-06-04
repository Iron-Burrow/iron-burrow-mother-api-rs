use axum::{routing::get, Router};
use tower_http::trace::TraceLayer;

use crate::{
    routes::{
        assets::{get_asset, list_assets},
        health::health,
        price_signals::{latest_price, price_stats, price_trend},
        resolve::resolve,
        status::status,
    },
    state::AppState,
};

pub fn create_app(state: AppState) -> Router {
    let v1_routes = Router::new()
        .route("/status", get(status))
        .route("/resolve", get(resolve))
        .route("/assets", get(list_assets))
        .route("/assets/{slug}/price/latest", get(latest_price))
        .route("/assets/{slug}/signal/price-stats", get(price_stats))
        .route("/assets/{slug}/signal/price-trend", get(price_trend))
        .route("/assets/{slug}", get(get_asset));

    Router::new()
        .route("/health", get(health))
        .nest("/v1", v1_routes)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        net::TcpListener,
    };

    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use serde_json::Value;
    use tower::ServiceExt;

    use super::*;
    use crate::{
        config::Config,
        price_indexer::PriceIndexerClient,
        repositories::global_assets::{demo_assets, GlobalAsset, GlobalAssetRepository},
    };

    fn test_app() -> Router {
        create_app(AppState::with_asset_repository(
            Config::default(),
            GlobalAssetRepository::in_memory(demo_assets()),
        ))
    }

    #[tokio::test]
    async fn health_returns_stable_contract() {
        let app = create_app(AppState::new(Config::default()));

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
        let app = create_app(AppState::new(Config::default()));

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
        assert_eq!(json["checks"]["price_indexer"], "not_connected");
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
        let app = create_app(AppState {
            config: Config::default(),
            version: env!("CARGO_PKG_VERSION"),
            database_pool: None,
            asset_repository: Some(repository),
            price_indexer_client: Some(price_indexer_client),
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
        let response = create_app(AppState::new(Config::default()))
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
    async fn asset_detail_returns_native_chain_map() {
        let json = assets_json("/v1/assets/bitcoin").await;

        assert_eq!(json["ok"], true);
        assert_eq!(json["type"], "asset");
        assert_eq!(json["asset"]["asset_id"], "bitcoin");
        assert_eq!(json["asset"]["symbol"], "BTC");
        assert_eq!(json["asset"]["canonical_path"], "/assets/bitcoin");
        assert_eq!(json["price"]["status"], "unavailable");
        assert!(json["price"]["price"].is_null());
        assert_eq!(json["chain_maps"][0]["network"]["slug"], "bitcoin-mainnet");
        assert_eq!(json["chain_maps"][0]["network"]["name"], "Bitcoin Mainnet");
        assert_eq!(
            json["chain_maps"][0]["network"]["caip2"],
            "bip122:000000000019d6689c085ae165831e93"
        );
        assert_eq!(json["chain_maps"][0]["is_native"], true);
        assert!(json["chain_maps"][0]["address"].is_null());
        assert!(json["chain_maps"][0]["network"]["family"].is_null());
        assert!(json["chain_maps"][0]["network"]["chain_id"].is_null());
    }

    #[tokio::test]
    async fn asset_detail_returns_deployed_chain_maps() {
        let json = assets_json("/v1/assets/usdc").await;
        let chain_maps = json["chain_maps"].as_array().unwrap();

        assert_eq!(chain_maps.len(), 5);
        assert_eq!(chain_maps[0]["network"]["slug"], "eth-mainnet");
        assert_eq!(
            chain_maps[0]["address"],
            "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
        );
        assert_eq!(chain_maps[0]["is_native"], false);
        assert_eq!(chain_maps[1]["network"]["slug"], "arbitrum-one");
        assert_eq!(chain_maps[2]["network"]["slug"], "base");
        assert_eq!(chain_maps[3]["network"]["slug"], "near");
        assert_eq!(chain_maps[4]["network"]["slug"], "mantle");
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
        let app = create_app(AppState {
            config: Config::default(),
            version: env!("CARGO_PKG_VERSION"),
            database_pool: None,
            asset_repository: Some(repository),
            price_indexer_client: Some(price_indexer_client),
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
        assert!(request.starts_with("GET /prices/latest?slug=usd-coin "));
        assert!(!request.contains("symbol="));
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
        let response = create_app(AppState::new(Config::default()))
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
    async fn latest_price_route_returns_internal_ql_price_and_billing() {
        let body = serde_json::json!({
            "asset": {
                "slug": "ethereum",
                "symbol": "ETH"
            },
            "currency": "USD",
            "price": "3811.450000",
            "published_at": "2026-05-29T00:00:00Z",
            "source": "chainlink",
            "freshness_status": "fresh"
        });
        let Some((price_indexer_url, request_handle)) =
            spawn_price_indexer_response("200 OK", body)
        else {
            return;
        };
        let app = price_signal_app(&price_indexer_url);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/assets/ethereum/price/latest")
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
        assert_eq!(json["type"], "asset_price_latest");
        assert_eq!(json["asset"]["slug"], "ethereum");
        assert_eq!(json["asset"]["symbol"], "ETH");
        assert_eq!(json["price"]["currency"], "USD");
        assert_eq!(json["price"]["value"], "3811.450000");
        assert_eq!(json["price"]["published_at"], "2026-05-29T00:00:00Z");
        assert_eq!(json["price"]["source"], "chainlink");
        assert_eq!(json["billing"]["billable"], true);
        assert_eq!(json["billing"]["amount"], "0.000100");

        let request = request_handle.await.unwrap();
        assert!(request.starts_with("GET /internal/v1/prices/latest?slug=ethereum&currency=USD "));
    }

    #[tokio::test]
    async fn latest_price_route_reports_unknown_asset_like_asset_detail() {
        let response = test_app()
            .oneshot(
                Request::builder()
                    .uri("/v1/assets/does-not-exist/price/latest")
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

        assert_eq!(json["error"]["code"], "asset_not_found");
    }

    #[tokio::test]
    async fn latest_price_route_reports_upstream_unavailable() {
        let body = serde_json::json!({
            "error": {
                "code": "UPSTREAM_DOWN",
                "message": "Price indexer is unavailable."
            }
        });
        let Some((price_indexer_url, _request_handle)) =
            spawn_price_indexer_response("503 Service Unavailable", body)
        else {
            return;
        };
        let app = price_signal_app(&price_indexer_url);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/assets/ethereum/price/latest")
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
        assert_eq!(json["error"]["code"], "price_indexer_unavailable");
    }

    #[tokio::test]
    async fn price_stats_endpoint_accepts_supported_windows() {
        for window in ["7d", "1w", "1m"] {
            let Some((price_indexer_url, request_handle)) =
                spawn_price_indexer_response("200 OK", empty_series_json())
            else {
                return;
            };
            let app = price_signal_app(&price_indexer_url);

            let response = app
                .oneshot(
                    Request::builder()
                        .uri(format!(
                            "/v1/assets/ethereum/signal/price-stats?window={window}"
                        ))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(response.status(), StatusCode::OK);

            let request = request_handle.await.unwrap();
            assert!(request
                .starts_with("GET /internal/v1/prices/series?slug=ethereum&currency=USD&from="));
            assert!(request.contains("&granularity=1h "));
        }
    }

    #[tokio::test]
    async fn price_signal_endpoints_reject_invalid_query_shapes() {
        for uri in [
            "/v1/assets/ethereum/signal/price-stats",
            "/v1/assets/ethereum/signal/price-stats?window=7d&fromDate=2020-05-21&toDate=2020-05-29",
            "/v1/assets/ethereum/signal/price-stats?fromDate=2020-05-21",
            "/v1/assets/ethereum/signal/price-stats?toDate=2020-05-29",
            "/v1/assets/ethereum/signal/price-stats?fromDate=2020-5-21&toDate=2020-05-29",
            "/v1/assets/ethereum/signal/price-stats?fromDate=2999-05-21&toDate=2999-05-29",
            "/v1/assets/ethereum/signal/price-stats?fromDate=2020-05-30&toDate=2020-05-29",
            "/v1/assets/ethereum/signal/price-stats?fromDate=2020-05-01&toDate=2020-06-02",
            "/v1/assets/ethereum/signal/price-stats?window=7d&window=1w",
            "/v1/assets/ethereum/signal/price-stats?window=7d&currency=USD",
        ] {
            let response = test_app()
                .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
                .await
                .unwrap();

            assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{uri}");

            let body = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            let json: Value = serde_json::from_slice(&body).unwrap();

            assert_eq!(json["error"]["code"], "invalid_price_signal_query");
        }
    }

    #[tokio::test]
    async fn price_stats_endpoint_calculates_deterministic_stats() {
        let Some((price_indexer_url, request_handle)) =
            spawn_price_indexer_response("200 OK", increasing_series_json())
        else {
            return;
        };
        let app = price_signal_app(&price_indexer_url);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/assets/ethereum/signal/price-stats?fromDate=2020-05-21&toDate=2020-05-28")
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

        assert_eq!(json["signal"]["type"], "price_stats");
        assert_eq!(json["signal"]["recipe"], "price_stats_v1");
        assert_eq!(json["signal"]["status"], "found");
        assert_eq!(json["signal"]["input"]["observations"], 3);
        assert_eq!(json["signal"]["stats"]["first_price"], "100.000000");
        assert_eq!(json["signal"]["stats"]["last_price"], "120.000000");
        assert_eq!(json["signal"]["stats"]["min_price"], "100.000000");
        assert_eq!(json["signal"]["stats"]["max_price"], "120.000000");
        assert_eq!(json["signal"]["stats"]["avg_price"], "110.000000");
        assert_eq!(json["signal"]["stats"]["change_abs"], "20.000000");
        assert_eq!(json["signal"]["stats"]["change_pct"], "20.000000");
        assert_eq!(json["signal"]["billing"]["amount"], "0.000500");
        assert_eq!(json["signal"]["source"]["service"], "price-indexer");

        let request = request_handle.await.unwrap();
        assert!(request.contains("from=2020-05-21T00%3A00%3A00Z"));
        assert!(request.contains("to=2020-05-28T00%3A00%3A00Z"));
    }

    #[tokio::test]
    async fn price_stats_endpoint_returns_non_billable_insufficient_data() {
        let Some((price_indexer_url, _request_handle)) =
            spawn_price_indexer_response("200 OK", empty_series_json())
        else {
            return;
        };
        let app = price_signal_app(&price_indexer_url);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/assets/ethereum/signal/price-stats?window=7d")
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

        assert_eq!(json["signal"]["status"], "insufficient_data");
        assert!(json["signal"]["stats"].is_null());
        assert_eq!(json["signal"]["billing"]["billable"], false);
        assert_eq!(json["signal"]["billing"]["reason"], "insufficient_data");
    }

    #[tokio::test]
    async fn price_trend_endpoint_returns_positive_evidence_without_advice() {
        let Some((price_indexer_url, _request_handle)) =
            spawn_price_indexer_response("200 OK", increasing_series_json())
        else {
            return;
        };
        let app = price_signal_app(&price_indexer_url);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/assets/ethereum/signal/price-trend?fromDate=2020-05-21&toDate=2020-05-28")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body_text = String::from_utf8(body.to_vec()).unwrap();
        let json: Value = serde_json::from_str(&body_text).unwrap();

        assert_eq!(json["signal"]["type"], "price_trend_evidence");
        assert_eq!(json["signal"]["recipe"], "price_trend_evidence_v1");
        assert_eq!(json["signal"]["evidence"]["positive_models"], 3);
        assert_eq!(json["signal"]["evidence"]["agreement"], "positive");
        assert_eq!(json["signal"]["models"][0]["direction"], "positive");
        assert_eq!(json["signal"]["billing"]["amount"], "0.001000");
        assert!(!body_text.contains("buy"));
        assert!(!body_text.contains("sell"));
        assert!(!body_text.contains("bullish"));
        assert!(!body_text.contains("bearish"));
    }

    #[tokio::test]
    async fn price_trend_endpoint_skips_log_model_for_non_positive_prices() {
        let Some((price_indexer_url, _request_handle)) =
            spawn_price_indexer_response("200 OK", non_positive_series_json())
        else {
            return;
        };
        let app = price_signal_app(&price_indexer_url);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/assets/ethereum/signal/price-trend?window=7d")
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

        assert_eq!(json["signal"]["models"][1]["name"], "log_linear_price");
        assert_eq!(json["signal"]["models"][1]["status"], "skipped");
        assert_eq!(json["signal"]["models"][1]["reason"], "non_positive_price");
        assert_eq!(json["signal"]["evidence"]["skipped_models"], 1);
    }

    #[tokio::test]
    async fn price_trend_endpoint_returns_non_billable_insufficient_data() {
        let Some((price_indexer_url, _request_handle)) =
            spawn_price_indexer_response("200 OK", one_point_series_json())
        else {
            return;
        };
        let app = price_signal_app(&price_indexer_url);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/assets/ethereum/signal/price-trend?window=7d")
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

        assert_eq!(json["signal"]["status"], "insufficient_data");
        assert_eq!(json["signal"]["models"].as_array().unwrap().len(), 0);
        assert_eq!(json["signal"]["evidence"]["total_models"], 0);
        assert_eq!(json["signal"]["billing"]["billable"], false);
    }

    #[tokio::test]
    async fn resolve_returns_usdc_for_aliases() {
        for query in ["usdc", "usdc%20coin%20usd", "usd%20coin"] {
            let json = resolve_json(&format!("/v1/resolve?q={query}")).await;

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
            let json = resolve_json(&format!("/v1/resolve?q={query}")).await;

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
            let json = resolve_json(&format!("/v1/resolve?q={query}")).await;

            assert_eq!(json["resolved"], true);
            assert_eq!(json["result"]["canonical_path"], path);
        }
    }

    #[tokio::test]
    async fn resolve_does_not_treat_network_aliases_as_assets() {
        for query in ["base", "base%20mainnet", "coinbase%20base"] {
            let json = resolve_json(&format!("/v1/resolve?q={query}")).await;

            assert_eq!(json["resolved"], false);
            assert_eq!(json["result"]["kind"], "unknown");
            assert!(json["result"]["recommendations"].as_array().unwrap().len() > 0);
        }
    }

    #[tokio::test]
    async fn resolve_unknown_returns_recommendations_without_404() {
        let response = test_app()
            .oneshot(
                Request::builder()
                    .uri("/v1/resolve?q=some%20unknown%20thing")
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
        assert!(json["result"]["recommendations"].as_array().unwrap().len() > 0);
    }

    #[tokio::test]
    async fn resolve_requires_query() {
        let response = test_app()
            .oneshot(
                Request::builder()
                    .uri("/v1/resolve")
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
        let response = create_app(AppState::new(Config::default()))
            .oneshot(
                Request::builder()
                    .uri("/v1/resolve?q=usdc")
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
        for uri in ["/v1/resolve?q=", "/v1/resolve?q=%20%20%20"] {
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
                    .uri(format!("/v1/resolve?q={overlong}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn resolve_normalizes_query_in_response() {
        let json = resolve_json("/v1/resolve?q=%20%20USDC,,,coin---USD%20%20").await;

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

    fn price_signal_app(price_indexer_url: &str) -> Router {
        let price_indexer_client =
            PriceIndexerClient::new(price_indexer_url, "test-token", 2000).unwrap();
        let repository = GlobalAssetRepository::in_memory(vec![GlobalAsset {
            id: "test-ethereum".to_string(),
            slug: "ethereum".to_string(),
            symbol: "ETH".to_string(),
            name: "Ethereum".to_string(),
            category: "crypto".to_string(),
            canonical_path: "/assets/ethereum".to_string(),
            aliases: vec!["eth".to_string()],
            sort_order: 10,
        }]);

        create_app(AppState {
            config: Config::default(),
            version: env!("CARGO_PKG_VERSION"),
            database_pool: None,
            asset_repository: Some(repository),
            price_indexer_client: Some(price_indexer_client),
        })
    }

    fn empty_series_json() -> Value {
        price_series_json(Vec::new())
    }

    fn increasing_series_json() -> Value {
        price_series_json(vec![
            ("2020-05-21T00:00:00Z", "100.000000"),
            ("2020-05-22T00:00:00Z", "110.000000"),
            ("2020-05-23T00:00:00Z", "120.000000"),
        ])
    }

    fn non_positive_series_json() -> Value {
        price_series_json(vec![
            ("2020-05-21T00:00:00Z", "100.000000"),
            ("2020-05-22T00:00:00Z", "0.000000"),
        ])
    }

    fn one_point_series_json() -> Value {
        price_series_json(vec![("2020-05-21T00:00:00Z", "100.000000")])
    }

    fn price_series_json(points: Vec<(&str, &str)>) -> Value {
        let points = points
            .into_iter()
            .map(|(timestamp, price)| {
                serde_json::json!({
                    "timestamp": timestamp,
                    "price": price,
                    "source": "chainlink"
                })
            })
            .collect::<Vec<_>>();

        serde_json::json!({
            "asset": {
                "slug": "ethereum",
                "symbol": "ETH"
            },
            "currency": "USD",
            "granularity": "1h",
            "range": {
                "from": "2020-05-21T00:00:00Z",
                "to": "2020-05-28T00:00:00Z"
            },
            "normalization": {
                "timezone": "UTC",
                "price_precision": "decimal_string",
                "duplicate_policy": "latest_recorded_wins",
                "missing_points_policy": "no_fill",
                "source_priority_policy": "configured_price_feed_priority"
            },
            "points": points
        })
    }

    fn spawn_price_indexer_response(
        status: &str,
        body: Value,
    ) -> Option<(String, tokio::task::JoinHandle<String>)> {
        let listener = match TcpListener::bind("127.0.0.1:0") {
            Ok(listener) => listener,
            Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return None,
            Err(error) => panic!("failed to bind test price-indexer: {error}"),
        };
        let url = format!("http://{}", listener.local_addr().unwrap());
        let body = body.to_string();
        let status = status.to_string();
        let handle = tokio::task::spawn_blocking(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_http_request(&mut stream);
            let response = format!(
                "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );

            stream.write_all(response.as_bytes()).unwrap();

            request
        });

        Some((url, handle))
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

    fn read_http_request(stream: &mut impl Read) -> String {
        let mut request = Vec::new();
        let mut buffer = [0; 1024];

        loop {
            let bytes_read = stream.read(&mut buffer).unwrap();
            if bytes_read == 0 {
                break;
            }

            request.extend_from_slice(&buffer[..bytes_read]);

            let Some(headers_end) = request.windows(4).position(|window| window == b"\r\n\r\n")
            else {
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
}
