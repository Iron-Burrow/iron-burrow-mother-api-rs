use axum::{routing::get, Router};
use tower_http::trace::TraceLayer;

use crate::{
    routes::{
        assets::{get_asset, list_assets},
        health::health,
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
        .route("/assets/{slug}", get(get_asset));

    Router::new()
        .route("/health", get(health))
        .nest("/v1", v1_routes)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use serde_json::Value;
    use tower::ServiceExt;

    use super::*;
    use crate::{
        config::Config,
        repositories::global_assets::{demo_assets, GlobalAssetRepository},
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
        assert!(json["assets"][0]["price"].is_null());
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
}
