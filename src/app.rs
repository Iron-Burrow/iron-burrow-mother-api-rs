use axum::{routing::get, Router};
use tower_http::trace::TraceLayer;

use crate::{
    routes::{assets::assets, health::health, resolve::resolve, status::status},
    state::AppState,
};

pub fn create_app(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/v1/status", get(status))
        .route("/v1/assets", get(assets))
        .route("/api/v1/resolve", get(resolve))
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
    async fn status_returns_static_alpha_state() {
        let config = Config {
            app_env: "test".to_string(),
            ..Config::default()
        };
        let app = create_app(AppState::new(config));

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
        assert_eq!(json["environment"], "test");
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
    async fn resolve_returns_usdc_for_aliases() {
        for query in ["usdc", "usdc%20coin%20usd", "usd%20coin"] {
            let json = resolve_json(&format!("/api/v1/resolve?q={query}")).await;

            assert_eq!(json["ok"], true);
            assert_eq!(json["resolved"], true);
            assert_eq!(json["type"], "resolve");
            assert_eq!(json["result"]["kind"], "asset");
            assert_eq!(json["result"]["canonical_path"], "/assets/usdc");
            assert_eq!(json["result"]["asset"]["asset_id"], "usdc");
        }
    }

    #[tokio::test]
    async fn resolve_returns_gold_for_spanish_and_symbol_aliases() {
        for query in ["oro%20de%20ley", "oro", "gold", "xau"] {
            let json = resolve_json(&format!("/api/v1/resolve?q={query}")).await;

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
            let json = resolve_json(&format!("/api/v1/resolve?q={query}")).await;

            assert_eq!(json["resolved"], true);
            assert_eq!(json["result"]["canonical_path"], path);
        }
    }

    #[tokio::test]
    async fn resolve_does_not_treat_network_aliases_as_assets() {
        for query in ["base", "base%20mainnet", "coinbase%20base"] {
            let json = resolve_json(&format!("/api/v1/resolve?q={query}")).await;

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
                    .uri("/api/v1/resolve?q=some%20unknown%20thing")
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
        assert!(json["result"]["recommendations"].as_array().unwrap().len() > 0);
    }

    #[tokio::test]
    async fn resolve_requires_query() {
        let response = test_app()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/resolve")
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
                    .uri("/api/v1/resolve?q=usdc")
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
        for uri in ["/api/v1/resolve?q=", "/api/v1/resolve?q=%20%20%20"] {
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
                    .uri(format!("/api/v1/resolve?q={overlong}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn resolve_normalizes_query_in_response() {
        let json = resolve_json("/api/v1/resolve?q=%20%20USDC,,,coin---USD%20%20").await;

        assert_eq!(json["query"]["raw"], "USDC,,,coin---USD");
        assert_eq!(json["query"]["normalized"], "usdc coin usd");
        assert_eq!(json["result"]["canonical_path"], "/assets/usdc");
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
