use axum::{
    http::{header::USER_AGENT, HeaderMap, Method, StatusCode, Uri},
    middleware,
    routing::{get, post},
    Router,
};
use tower_http::trace::TraceLayer;
use tracing::{debug, warn};

use crate::{
    routes::{
        assets::{get_asset, get_price_stats_signal, get_price_trend_signal, list_assets},
        balances::{resolve_bulk_balances, resolve_single_balance},
        erc20_transfers::search_erc20_transfers,
        health::health,
        predictions::{
            add_deprecation_header, get_world_cup_country_prediction,
            get_world_cup_winner_prediction,
        },
        resolve::resolve,
        status::status,
    },
    state::AppState,
};

pub const BALANCE_ROUTE_INVENTORY: &str = "POST /v1/balances, POST /v1/balances/bulk";

pub fn create_app(state: AppState) -> Router {
    let deprecated_prediction_routes = Router::new()
        .route(
            "/predictions/fifa-world-cup/winner",
            get(get_world_cup_winner_prediction),
        )
        .route(
            "/predictions/fifa-world-cup/{country}",
            get(get_world_cup_country_prediction),
        )
        .layer(middleware::map_response(add_deprecation_header));

    let mut v1_routes = Router::new()
        .route("/status", get(status))
        .route("/resolve", get(resolve))
        .route("/balances", post(resolve_single_balance))
        .route("/balances/bulk", post(resolve_bulk_balances))
        .route("/assets", get(list_assets))
        .route("/assets/{slug}", get(get_asset))
        .route(
            "/assets/{slug}/signal/price-stats",
            get(get_price_stats_signal),
        )
        .route(
            "/assets/{slug}/signal/price-trend",
            get(get_price_trend_signal),
        )
        .merge(deprecated_prediction_routes);

    if state.config.erc20_transfers_enabled {
        v1_routes = v1_routes.route("/erc20-transfers/search", post(search_erc20_transfers));
    }

    Router::new()
        .route("/health", get(health))
        .nest("/v1", v1_routes)
        .fallback(unmatched_route)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn unmatched_route(method: Method, uri: Uri, headers: HeaderMap) -> StatusCode {
    let user_agent = headers
        .get(USER_AGENT)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("-");
    let request_id = headers
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("-");

    if uri.path() == "/v1" || uri.path().starts_with("/v1/") {
        warn!(
            method = %method,
            path = uri.path(),
            user_agent,
            request_id,
            status = StatusCode::NOT_FOUND.as_u16(),
            "unmatched API route"
        );
    } else {
        debug!(
            method = %method,
            path = uri.path(),
            user_agent,
            request_id,
            status = StatusCode::NOT_FOUND.as_u16(),
            "unmatched non-API route"
        );
    }

    StatusCode::NOT_FOUND
}

#[cfg(test)]
mod tests {
    use std::{
        io::ErrorKind,
        io::{Read, Write},
        net::{TcpListener, TcpStream},
        time::{Duration, Instant},
    };

    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use serde_json::Value;
    use tower::ServiceExt;

    use super::*;
    use crate::{
        adapters::dis::DisClient,
        adapters::postgres::global_assets::{demo_assets, GlobalAsset, GlobalAssetRepository},
        config::Config,
        price_indexer::PriceIndexerClient,
    };

    const TEST_DIS_ACCEPT_TIMEOUT: Duration = Duration::from_secs(2);

    fn test_app() -> Router {
        create_app(AppState::with_asset_repository(
            Config::default(),
            GlobalAssetRepository::in_memory(demo_assets()),
        ))
    }

    fn test_app_with_price_indexer(price_indexer_url: &str, timeout_ms: u64) -> Router {
        let price_indexer_client =
            PriceIndexerClient::new(price_indexer_url, "test-token", timeout_ms).unwrap();

        create_app(AppState {
            config: Config::default(),
            version: env!("CARGO_PKG_VERSION"),
            database_pool: None,
            asset_repository: Some(GlobalAssetRepository::in_memory(demo_assets())),
            price_indexer_client: Some(price_indexer_client),
            dis_client: None,
            bigwig_latest_balances_client: None,
        })
    }

    fn test_app_with_dis(dis_url: &str) -> Router {
        test_app_with_dis_timeout(dis_url, 2000)
    }

    fn test_app_with_dis_timeout(dis_url: &str, timeout_ms: u64) -> Router {
        let dis_client = DisClient::new(dis_url, timeout_ms, 1).unwrap();

        create_app(AppState {
            config: Config::default(),
            version: env!("CARGO_PKG_VERSION"),
            database_pool: None,
            asset_repository: Some(GlobalAssetRepository::in_memory(demo_assets())),
            price_indexer_client: None,
            dis_client: Some(dis_client),
            bigwig_latest_balances_client: None,
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

    #[test]
    fn production_caddy_forwards_all_v1_methods_to_axum() {
        let caddyfile = include_str!("../infra/caddy/Caddyfile");

        assert!(caddyfile.contains("path /health /v1/*"));
        assert!(!caddyfile.contains("method GET"));
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
        assert_eq!(json["checks"]["price_indexer"], "not_configured");
        assert_eq!(json["checks"]["dis"], "not_configured");
        assert_eq!(json["checks"]["evm_indexer"], "not_connected");
    }

    #[tokio::test]
    async fn prediction_winner_route_calls_dis_and_sanitizes_response() {
        let Some((dis_url, request_handle)) =
            spawn_prediction_dis(vec![(StatusCode::OK, winner_prediction_body())])
        else {
            return;
        };
        let app = test_app_with_dis(&dis_url);

        let (status, json) = app_json(app, "/v1/predictions/fifa-world-cup/winner").await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            json,
            serde_json::json!({
                "ok": true,
                "event": "World Cup Winner ",
                "event_slug": "fifa-world-cup-2026-winner",
                "odds": [
                    {
                        "team": "France",
                        "probability": "0.1595",
                        "price": "0.1595",
                        "currency": "USDC"
                    },
                    {
                        "team": "Spain",
                        "probability": "0.1595",
                        "price": "0.1595",
                        "currency": "USDC"
                    }
                ],
                "source": "polymarket",
                "deterministic": true,
                "captured_at": "2026-06-06T03:21:42.512048Z"
            })
        );
        assert!(json["odds"][0]["probability"].is_string());
        assert!(json["odds"][0]["price"].is_string());

        let requests = request_handle.await.unwrap();
        assert_eq!(requests.len(), 1);
        assert!(
            requests[0].starts_with("POST /internal/v1/prediction-markets/polymarket/snapshot ")
        );
        assert_eq!(
            request_body_json(&requests[0]),
            serde_json::json!({ "event_slug": "fifa-world-cup-2026-winner" })
        );
    }

    #[tokio::test]
    async fn prediction_success_responses_include_deprecation_header() {
        let Some((dis_url, request_handle)) = spawn_prediction_dis(vec![
            (StatusCode::OK, winner_prediction_body()),
            (StatusCode::OK, country_prediction_body("mexico")),
        ]) else {
            return;
        };
        let app = test_app_with_dis(&dis_url);

        for uri in [
            "/v1/predictions/fifa-world-cup/winner",
            "/v1/predictions/fifa-world-cup/mexico",
        ] {
            let response = app
                .clone()
                .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
                .await
                .unwrap();

            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(
                response
                    .headers()
                    .get("deprecation")
                    .and_then(|value| value.to_str().ok()),
                Some(crate::routes::predictions::DEPRECATION_HEADER_VALUE)
            );
        }

        assert_eq!(request_handle.await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn prediction_error_responses_are_deprecated_without_marking_other_routes() {
        let prediction_response = test_app()
            .oneshot(
                Request::builder()
                    .uri("/v1/predictions/fifa-world-cup/winner")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            prediction_response.status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
        assert_eq!(
            prediction_response
                .headers()
                .get("deprecation")
                .and_then(|value| value.to_str().ok()),
            Some(crate::routes::predictions::DEPRECATION_HEADER_VALUE)
        );

        let health_response = test_app()
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(health_response.status(), StatusCode::OK);
        assert!(health_response.headers().get("deprecation").is_none());
    }

    #[tokio::test]
    async fn prediction_country_route_calls_dis_with_normalized_country() {
        let Some((dis_url, request_handle)) = spawn_prediction_dis(vec![
            (StatusCode::OK, country_prediction_body("mexico")),
            (StatusCode::OK, country_prediction_body("mexico")),
            (StatusCode::OK, country_prediction_body("mexico")),
        ]) else {
            return;
        };
        let app = test_app_with_dis(&dis_url);

        for country in ["Mexico", "mexico", "MEXICO"] {
            let (status, json) = app_json(
                app.clone(),
                &format!("/v1/predictions/fifa-world-cup/{country}"),
            )
            .await;

            assert_eq!(status, StatusCode::OK);
            assert_eq!(
                json,
                serde_json::json!({
                    "ok": true,
                    "market": "Will Mexico reach the Round of 16 at the 2026 FIFA World Cup?",
                    "country": {
                        "slug": "mexico",
                        "name": "Mexico"
                    },
                    "probability": "0.535",
                    "price": "0.535",
                    "currency": "USDC",
                    "source": "polymarket",
                    "deterministic": true,
                    "captured_at": "2026-06-06T03:22:11.593940Z"
                })
            );
            assert!(json["probability"].is_string());
            assert!(json["price"].is_string());
        }

        let requests = request_handle.await.unwrap();
        assert_eq!(requests.len(), 3);
        for request in requests {
            assert_eq!(
                request_body_json(&request),
                serde_json::json!({
                    "event_slug": "fifa-world-cup-2026-country-probability",
                    "country": "mexico"
                })
            );
        }
    }

    #[tokio::test]
    async fn prediction_demo_smoke_routes_return_contract_success_shapes() {
        let Some((dis_url, request_handle)) = spawn_prediction_dis(vec![
            (StatusCode::OK, winner_prediction_body()),
            (StatusCode::OK, country_prediction_body("mexico")),
        ]) else {
            return;
        };
        let app = test_app_with_dis(&dis_url);

        let (winner_status, winner_json) =
            app_json(app.clone(), "/v1/predictions/fifa-world-cup/winner").await;
        let (country_status, country_json) =
            app_json(app, "/v1/predictions/fifa-world-cup/mexico").await;

        assert_eq!(winner_status, StatusCode::OK);
        assert_eq!(winner_json["ok"], true);
        assert_eq!(winner_json["event"], "World Cup Winner ");
        assert_eq!(winner_json["event_slug"], "fifa-world-cup-2026-winner");
        assert_eq!(winner_json["odds"][0]["team"], "France");
        assert_eq!(winner_json["odds"][1]["team"], "Spain");
        assert!(winner_json["odds"][0]["probability"].is_string());
        assert!(winner_json["odds"][0]["price"].is_string());
        assert_eq!(winner_json["odds"][0]["currency"], "USDC");
        assert_eq!(winner_json["source"], "polymarket");
        assert_eq!(winner_json["deterministic"], true);
        assert_eq!(winner_json["captured_at"], "2026-06-06T03:21:42.512048Z");
        assert!(winner_json.get("provider_market").is_none());
        assert!(winner_json["odds"][0].get("provider_market").is_none());

        assert_eq!(country_status, StatusCode::OK);
        assert_eq!(country_json["ok"], true);
        assert_eq!(
            country_json["market"],
            "Will Mexico reach the Round of 16 at the 2026 FIFA World Cup?"
        );
        assert_eq!(country_json["country"]["slug"], "mexico");
        assert_eq!(country_json["country"]["name"], "Mexico");
        assert_eq!(country_json["probability"], "0.535");
        assert_eq!(country_json["price"], "0.535");
        assert!(country_json["probability"].is_string());
        assert!(country_json["price"].is_string());
        assert_eq!(country_json["currency"], "USDC");
        assert_eq!(country_json["source"], "polymarket");
        assert_eq!(country_json["deterministic"], true);
        assert_eq!(country_json["captured_at"], "2026-06-06T03:22:11.593940Z");
        assert!(country_json.get("provider_market").is_none());
        assert!(country_json["country"].get("provider_market").is_none());

        let requests = request_handle.await.unwrap();
        assert_eq!(requests.len(), 2);
        assert_eq!(
            request_body_json(&requests[0]),
            serde_json::json!({ "event_slug": "fifa-world-cup-2026-winner" })
        );
        assert_eq!(
            request_body_json(&requests[1]),
            serde_json::json!({
                "event_slug": "fifa-world-cup-2026-country-probability",
                "country": "mexico"
            })
        );
    }

    #[tokio::test]
    async fn prediction_winner_ignores_unknown_query_params() {
        let Some((dis_url, request_handle)) =
            spawn_prediction_dis(vec![(StatusCode::OK, winner_prediction_body())])
        else {
            return;
        };
        let app = test_app_with_dis(&dis_url);

        let (status, json) = app_json(
            app,
            "/v1/predictions/fifa-world-cup/winner?ignored=true&anything=else",
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(json["event_slug"], "fifa-world-cup-2026-winner");

        let requests = request_handle.await.unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(
            request_body_json(&requests[0]),
            serde_json::json!({ "event_slug": "fifa-world-cup-2026-winner" })
        );
    }

    #[tokio::test]
    async fn prediction_winner_response_uses_contract_event_slug_not_dis_echo() {
        let mut body = winner_prediction_body();
        body["event_slug"] = serde_json::json!("wrong-upstream-echo");
        let Some((dis_url, _request_handle)) = spawn_prediction_dis(vec![(StatusCode::OK, body)])
        else {
            return;
        };
        let app = test_app_with_dis(&dis_url);

        let (status, json) = app_json(app, "/v1/predictions/fifa-world-cup/winner").await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(json["event_slug"], "fifa-world-cup-2026-winner");
    }

    #[tokio::test]
    async fn prediction_routes_report_missing_dis_client() {
        let (status, json) = app_json(test_app(), "/v1/predictions/fifa-world-cup/winner").await;

        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_public_error(&json, "prediction_resolver_unavailable");
    }

    #[tokio::test]
    async fn prediction_routes_map_dis_connection_failure_to_resolver_unavailable() {
        let Ok(listener) = TcpListener::bind("127.0.0.1:0") else {
            return;
        };
        let dis_url = format!("http://{}", listener.local_addr().unwrap());
        drop(listener);
        let app = test_app_with_dis(&dis_url);

        let (status, json) = app_json(app, "/v1/predictions/fifa-world-cup/winner").await;

        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_public_error(&json, "prediction_resolver_unavailable");
    }

    #[tokio::test]
    async fn prediction_routes_map_dis_timeout_to_resolver_timeout() {
        let Ok(listener) = TcpListener::bind("127.0.0.1:0") else {
            return;
        };
        let dis_url = format!("http://{}", listener.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            let (_stream, _) = listener.accept().expect("test request should connect");
            std::thread::sleep(Duration::from_millis(100));
        });
        let app = test_app_with_dis_timeout(&dis_url, 10);

        let (status, json) = app_json(app, "/v1/predictions/fifa-world-cup/winner").await;

        assert_eq!(status, StatusCode::GATEWAY_TIMEOUT);
        assert_public_error(&json, "prediction_resolver_timeout");
        handle.join().expect("test listener thread should finish");
    }

    #[tokio::test]
    async fn prediction_routes_map_unsupported_subject_from_dis() {
        let Some((dis_url, _request_handle)) = spawn_prediction_dis(vec![(
            StatusCode::BAD_REQUEST,
            serde_json::json!({
                "error": {
                    "code": "unsupported_prediction_subject",
                    "message": "DIS-owned message.",
                    "details": { "provider_market": { "hidden": true } }
                }
            }),
        )]) else {
            return;
        };
        let app = test_app_with_dis(&dis_url);

        let (status, json) = app_json(app, "/v1/predictions/fifa-world-cup/atlantis").await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_public_error(&json, "unsupported_prediction_subject");
    }

    #[tokio::test]
    async fn prediction_routes_map_provider_failures_from_dis() {
        for (dis_status, dis_code, expected_status, expected_code) in [
            (
                StatusCode::SERVICE_UNAVAILABLE,
                "prediction_provider_unavailable",
                StatusCode::SERVICE_UNAVAILABLE,
                "prediction_provider_unavailable",
            ),
            (
                StatusCode::GATEWAY_TIMEOUT,
                "prediction_provider_timeout",
                StatusCode::GATEWAY_TIMEOUT,
                "prediction_provider_timeout",
            ),
        ] {
            let Some((dis_url, _request_handle)) = spawn_prediction_dis(vec![(
                dis_status,
                serde_json::json!({
                    "error": {
                        "code": dis_code,
                        "message": "DIS-owned message.",
                        "details": {
                            "provider_market": { "hidden": true },
                            "provider_body": "must not leak"
                        }
                    }
                }),
            )]) else {
                return;
            };
            let app = test_app_with_dis(&dis_url);

            let (status, json) = app_json(app, "/v1/predictions/fifa-world-cup/winner").await;

            assert_eq!(status, expected_status);
            assert_public_error(&json, expected_code);
        }
    }

    #[tokio::test]
    async fn prediction_routes_map_dis_internal_error_to_resolver_error() {
        let Some((dis_url, _request_handle)) = spawn_prediction_dis(vec![(
            StatusCode::INTERNAL_SERVER_ERROR,
            serde_json::json!({
                "error": {
                    "code": "internal_error",
                    "message": "DIS-owned message.",
                    "details": {
                        "provider_market": { "hidden": true },
                        "transport": "internal stack trace"
                    }
                }
            }),
        )]) else {
            return;
        };
        let app = test_app_with_dis(&dis_url);

        let (status, json) = app_json(app, "/v1/predictions/fifa-world-cup/winner").await;

        assert_eq!(status, StatusCode::BAD_GATEWAY);
        assert_public_error(&json, "prediction_resolver_error");
    }

    #[tokio::test]
    async fn prediction_routes_map_wrong_shaped_success_to_schema_mismatch() {
        let Some((dis_url, _request_handle)) = spawn_prediction_dis(vec![(
            StatusCode::OK,
            serde_json::json!({
                "ok": true,
                "event": "Legacy response shape",
                "odds": [],
                "provider_market": {
                    "id": "must-not-leak",
                    "url": "https://provider.invalid/must-not-leak"
                }
            }),
        )]) else {
            return;
        };
        let app = test_app_with_dis(&dis_url);

        let (status, json) = app_json(app, "/v1/predictions/fifa-world-cup/winner").await;

        assert_eq!(status, StatusCode::BAD_GATEWAY);
        assert_public_error(&json, "prediction_resolver_schema_mismatch");
        assert_ne!(
            json["error"]["code"],
            serde_json::json!("prediction_resolver_unavailable")
        );
    }

    #[tokio::test]
    async fn prediction_routes_map_unknown_dis_error_code_to_resolver_error() {
        let Some((dis_url, _request_handle)) = spawn_prediction_dis(vec![(
            StatusCode::SERVICE_UNAVAILABLE,
            serde_json::json!({
                "error": {
                    "code": "future_dis_error",
                    "message": "Future DIS error.",
                    "details": {
                        "provider_market": { "id": "must-not-leak" }
                    }
                }
            }),
        )]) else {
            return;
        };
        let app = test_app_with_dis(&dis_url);

        let (status, json) = app_json(app, "/v1/predictions/fifa-world-cup/winner").await;

        assert_eq!(status, StatusCode::BAD_GATEWAY);
        assert_public_error(&json, "prediction_resolver_error");
        assert_ne!(
            json["error"]["code"],
            serde_json::json!("prediction_resolver_schema_mismatch")
        );
        assert_ne!(
            json["error"]["code"],
            serde_json::json!("prediction_resolver_unavailable")
        );
    }

    #[tokio::test]
    async fn prediction_routes_map_invalid_dis_error_body_to_malformed_response() {
        let Some((dis_url, _request_handle)) = spawn_prediction_dis_raw(vec![(
            StatusCode::INTERNAL_SERVER_ERROR,
            "not-json".to_string(),
        )]) else {
            return;
        };
        let app = test_app_with_dis(&dis_url);

        let (status, json) = app_json(app, "/v1/predictions/fifa-world-cup/winner").await;

        assert_eq!(status, StatusCode::BAD_GATEWAY);
        assert_public_error(&json, "prediction_resolver_malformed_response");
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
            dis_client: None,
            bigwig_latest_balances_client: None,
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
        let app = create_app(AppState {
            config: Config::default(),
            version: env!("CARGO_PKG_VERSION"),
            database_pool: None,
            asset_repository: Some(repository),
            price_indexer_client: Some(price_indexer_client),
            dis_client: None,
            bigwig_latest_balances_client: None,
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
        assert!(json["asset_network_maps"].as_array().unwrap().len() > 0);
        assert!(json.get("chain_maps").is_none());
        assert!(json.get("signals").is_none());
        assert!(json.get("enrichment_errors").is_none());
    }

    #[tokio::test]
    async fn asset_detail_treats_invalid_enrichment_params_as_partial_errors() {
        let json = assets_json(
            "/v1/assets/bitcoin?include=priceStats,priceSeries&window=1h&granularity=1h",
        )
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

        let (status, json) =
            app_json(app, "/v1/assets/bitcoin?include=priceStats,priceTrend").await;

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

        let (status, json) =
            app_json(app, "/v1/assets/bitcoin?include=priceStats,priceTrend").await;

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

        let (status, json) =
            app_json(app, "/v1/assets/bitcoin?include=priceStats,priceTrend").await;

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
            create_app(AppState::new(Config::default())),
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

    fn assert_public_error(json: &Value, expected_code: &str) {
        let top_level = json
            .as_object()
            .expect("public error response should be an object");
        assert_eq!(top_level.len(), 2);
        assert!(top_level.contains_key("ok"));
        assert!(top_level.contains_key("error"));

        let error = json["error"]
            .as_object()
            .expect("public error body should be an object");
        assert_eq!(error.len(), 2);
        assert_eq!(json["ok"], false);
        assert_eq!(json["error"]["code"], expected_code);
        assert!(json["error"]["message"]
            .as_str()
            .is_some_and(|message| !message.is_empty()));

        let serialized = json.to_string();
        for forbidden in [
            "DIS-owned message",
            "details",
            "provider_market",
            "provider_body",
            "hidden",
            "internal stack trace",
            "reqwest",
            "transport error",
        ] {
            assert!(
                !serialized.contains(forbidden),
                "public error leaked forbidden content: {forbidden}"
            );
        }
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

    fn spawn_prediction_dis(
        responses: Vec<(StatusCode, Value)>,
    ) -> Option<(String, tokio::task::JoinHandle<Vec<String>>)> {
        spawn_prediction_dis_raw(
            responses
                .into_iter()
                .map(|(status, body)| (status, body.to_string()))
                .collect(),
        )
    }

    fn spawn_prediction_dis_raw(
        responses: Vec<(StatusCode, String)>,
    ) -> Option<(String, tokio::task::JoinHandle<Vec<String>>)> {
        let listener = match TcpListener::bind("127.0.0.1:0") {
            Ok(listener) => listener,
            Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return None,
            Err(error) => panic!("failed to bind test DIS: {error}"),
        };
        listener
            .set_nonblocking(true)
            .expect("test DIS listener should allow nonblocking mode");
        let url = format!("http://{}", listener.local_addr().unwrap());
        let handle = tokio::task::spawn_blocking(move || {
            let mut requests = Vec::new();

            for (status, body) in responses {
                let mut stream = accept_prediction_connection(&listener);
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

    fn accept_prediction_connection(listener: &TcpListener) -> TcpStream {
        let deadline = Instant::now() + TEST_DIS_ACCEPT_TIMEOUT;

        loop {
            match listener.accept() {
                Ok((stream, _)) => {
                    stream
                        .set_nonblocking(false)
                        .expect("test DIS stream should allow blocking mode");
                    stream
                        .set_read_timeout(Some(TEST_DIS_ACCEPT_TIMEOUT))
                        .expect("test DIS stream should allow read timeout");

                    return stream;
                }
                Err(error)
                    if error.kind() == ErrorKind::WouldBlock && Instant::now() < deadline =>
                {
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(error) if error.kind() == ErrorKind::WouldBlock => {
                    panic!("timed out waiting for expected DIS test request");
                }
                Err(error) => panic!("failed to accept test DIS request: {error}"),
            }
        }
    }

    fn winner_prediction_body() -> Value {
        serde_json::json!({
            "event_slug": "fifa-world-cup-2026-winner",
            "event_title": "World Cup Winner ",
            "source": "polymarket",
            "source_kind": "public_market_data_api",
            "mode": "live_passthrough",
            "deterministic": true,
            "captured_at": "2026-06-06T03:21:42.512048Z",
            "provider_market": {
                "id": "558936",
                "slug": "will-france-win-the-2026-fifa-world-cup-924",
                "condition_id": "0x9b6fef249040fd17e9c107955b37ac2c3e923509b6b0ff01cc463a331ddeb894",
                "url": "https://polymarket.com/event/will-france-win-the-2026-fifa-world-cup-924"
            },
            "warnings": [
                {
                    "code": "probability_interpreted_from_price",
                    "message": "Outcome probabilities are interpreted from public market prices."
                }
            ],
            "outcomes": [
                {
                    "name": "France",
                    "probability": "0.1595",
                    "price": "0.1595",
                    "currency": "USDC"
                },
                {
                    "name": "Spain",
                    "probability": "0.1595",
                    "price": "0.1595",
                    "currency": "USDC"
                }
            ]
        })
    }

    fn country_prediction_body(country_slug: &str) -> Value {
        serde_json::json!({
            "event_slug": "fifa-world-cup-2026-country-probability",
            "event_title": "FIFA World Cup 2026 Country Probability",
            "source": "polymarket",
            "source_kind": "public_market_data_api",
            "mode": "live_passthrough",
            "deterministic": true,
            "captured_at": "2026-06-06T03:22:11.593940Z",
            "provider_market": {
                "id": "2415420",
                "slug": "will-mexico-reach-the-round-of-16-at-the-2026-fifa-world-cup-20260602025120735",
                "condition_id": "0x2b3237da39d6c7b1f7adef29c5f675e4214cec25f585ca151c7b8cc9271871e1",
                "url": "https://polymarket.com/event/will-mexico-reach-the-round-of-16-at-the-2026-fifa-world-cup-20260602025120735"
            },
            "warnings": [
                {
                    "code": "probability_interpreted_from_price",
                    "message": "Outcome probability is interpreted from public market price."
                }
            ],
            "subject": {
                "kind": "country",
                "slug": country_slug,
                "name": "Mexico"
            },
            "market": "Will Mexico reach the Round of 16 at the 2026 FIFA World Cup?",
            "probability": "0.535",
            "price": "0.535",
            "currency": "USDC"
        })
    }

    fn request_body_json(request: &str) -> Value {
        let (_, body) = request
            .split_once("\r\n\r\n")
            .expect("test request should include a body separator");

        serde_json::from_str(body).unwrap()
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
