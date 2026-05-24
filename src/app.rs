use axum::{routing::get, Router};
use tower_http::trace::TraceLayer;

use crate::{
    routes::{health::health, status::status},
    state::AppState,
};

pub fn create_app(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/v1/status", get(status))
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
    use crate::config::Config;

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
}
