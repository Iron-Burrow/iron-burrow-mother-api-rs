use axum::{routing::get, Router};
use tower_http::trace::TraceLayer;

use crate::{
    routes::{health::health, resolve::resolve, status::status},
    state::AppState,
};

pub fn create_app(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/v1/status", get(status))
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
            ("bitcoin", "/assets/bitcoin"),
            ("btc", "/assets/bitcoin"),
            ("ethereum", "/assets/ethereum"),
            ("eth", "/assets/ethereum"),
            ("mantle", "/assets/mantle"),
            ("mnt", "/assets/mantle"),
            ("near%20protocol", "/assets/near"),
        ] {
            let json = resolve_json(&format!("/api/v1/resolve?q={query}")).await;

            assert_eq!(json["resolved"], true);
            assert_eq!(json["result"]["canonical_path"], path);
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
}
