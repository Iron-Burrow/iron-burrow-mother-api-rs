use std::{
    io::{Read, Write},
    net::TcpListener,
};

use axum::{
    body::Body,
    http::{
        header::{
            AUTHORIZATION, CACHE_CONTROL, CONTENT_SECURITY_POLICY, CONTENT_TYPE, REFERRER_POLICY,
            X_CONTENT_TYPE_OPTIONS, X_FRAME_OPTIONS,
        },
        Request, StatusCode,
    },
    Router,
};
use serde_json::Value;
use tower::ServiceExt;
use uuid::Uuid;

use super::*;
use crate::{
    adapters::{
        http::state::HttpStateTestBuilder,
        postgres::{
            api_keys::{ApiKeyAuthorizationGrants, ApiKeyLookup},
            ApiKeyRepository,
        },
        price_indexer::PriceIndexerClient,
    },
    config::{Config, PublicApiSurface},
    domain::{
        api_keys::hash_presented_api_key,
        capabilities::{Capability, CapabilityGrant, NetworkScope},
    },
    test_utils::postgres::{migrated_pool, POSTGRES_TEST_DATABASE_URL_ENV},
};

const TEST_API_KEY: &str =
    "ib_live_0123456789abcdef.0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const TEST_API_KEY_PREFIX: &str = "ib_live_0123456789abcdef";

fn test_app() -> Router {
    build_router(HttpStateTestBuilder::new(Config::default()).build())
}

fn beta_config() -> Config {
    Config {
        public_api_surface: PublicApiSurface::Beta,
        ..Config::default()
    }
}

fn beta_app_with_api_key_repository(api_key_repository: Option<ApiKeyRepository>) -> Router {
    let mut builder = HttpStateTestBuilder::new(beta_config());
    if let Some(api_key_repository) = api_key_repository {
        builder = builder.with_api_key_repository(api_key_repository);
    }
    build_router(builder.build())
}

fn async_reports_callback_app() -> Router {
    build_router(
        HttpStateTestBuilder::new(Config {
            public_api_surface: PublicApiSurface::Beta,
            async_reports_enabled: true,
            bigwig_report_outcome_token: Some("bigwig-outcome-token".to_string()),
            ..Config::default()
        })
        .build(),
    )
}

fn beta_app_with_lookup(lookup: ApiKeyLookup) -> Router {
    beta_app_with_api_key_repository(Some(ApiKeyRepository::in_memory(vec![(
        TEST_API_KEY_PREFIX.to_string(),
        hash_presented_api_key(TEST_API_KEY).to_vec(),
        lookup,
    )])))
}

fn test_app_with_price_indexer(price_indexer_url: &str, timeout_ms: u64) -> Router {
    let price_indexer_client =
        PriceIndexerClient::new(price_indexer_url, "test-token", timeout_ms).unwrap();

    build_router(
        HttpStateTestBuilder::new(Config::default())
            .with_price_indexer_client(price_indexer_client)
            .build(),
    )
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
async fn public_html_routes_render_with_the_reviewed_security_headers() {
    for (uri, expected_text) in [
        ("/", "Understand what happened on-chain."),
        (
            "/scan",
            "The public Scan interface is preparing for its first release.",
        ),
        (
            "/scan/eth-mainnet",
            "Scan for <code>eth-mainnet</code> is preparing",
        ),
        ("/access", "API access is currently private Beta."),
        ("/docs", "Build with the Mother API"),
    ] {
        let response = test_app()
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK, "{uri}");
        assert_eq!(
            response.headers().get(CONTENT_TYPE).unwrap(),
            "text/html; charset=utf-8",
            "{uri}"
        );
        assert_eq!(
            response.headers().get(CONTENT_SECURITY_POLICY).unwrap(),
            "default-src 'self'; base-uri 'self'; form-action 'self'; frame-ancestors 'none'; object-src 'none'; script-src 'self'; style-src 'self'; img-src 'self'; connect-src 'self'",
            "{uri}"
        );
        assert_eq!(
            response.headers().get(X_CONTENT_TYPE_OPTIONS).unwrap(),
            "nosniff",
            "{uri}"
        );
        assert_eq!(
            response.headers().get(X_FRAME_OPTIONS).unwrap(),
            "DENY",
            "{uri}"
        );
        assert_eq!(
            response.headers().get(REFERRER_POLICY).unwrap(),
            "strict-origin-when-cross-origin",
            "{uri}"
        );

        let body = String::from_utf8(
            axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap()
                .to_vec(),
        )
        .unwrap();
        assert!(body.contains(expected_text), "{uri}");
        assert!(!body.contains("El Vasco"), "{uri}");
        assert!(!body.contains("El Malo"), "{uri}");
    }
}

#[tokio::test]
async fn homepage_and_docs_link_only_to_available_web_and_api_surfaces() {
    let home_response = test_app()
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let home = String::from_utf8(
        axum::body::to_bytes(home_response.into_body(), usize::MAX)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(home.contains("href=\"/scan\""));
    assert!(home.contains("href=\"/access\""));
    assert!(home.contains("href=\"/docs\""));
    assert!(home.contains("href=\"/login\""));
    assert!(home.contains("href=\"/signup\""));
    assert!(!home.contains("/app"));
    assert!(!home.contains("/get-api-key"));

    let docs_response = test_app()
        .oneshot(Request::builder().uri("/docs").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let docs = String::from_utf8(
        axum::body::to_bytes(docs_response.into_body(), usize::MAX)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(docs.contains("href=\"http://localhost:3000/openapi.json\""));
    assert!(docs.contains("network_slug"));
}

#[tokio::test]
async fn docs_link_to_the_configured_machine_api_origin() {
    let app = build_router(
        HttpStateTestBuilder::new(Config {
            public_api_base_url: "https://api.example.test/".to_string(),
            ..Config::default()
        })
        .build(),
    );
    let response = app
        .oneshot(Request::builder().uri("/docs").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let body = String::from_utf8(
        axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();

    assert!(body.contains("href=\"https://api.example.test/openapi.json\""));
}

#[tokio::test]
async fn password_account_entry_routes_are_human_html_only() {
    for (uri, expected_link) in [("/signup", "/login"), ("/login", "/signup")] {
        let response = test_app()
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{uri}");
        assert!(response
            .headers()
            .get(CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("text/html"));
        assert_eq!(response.headers().get(CACHE_CONTROL).unwrap(), "no-store");
        assert_eq!(
            response.headers().get(REFERRER_POLICY).unwrap(),
            "no-referrer"
        );
        let cookie = response
            .headers()
            .get("set-cookie")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(cookie.contains("__Host-ib_csrf="));
        assert!(cookie.contains("Path=/; Secure; SameSite=Lax"));
        assert!(!cookie.contains("HttpOnly"));

        let body = String::from_utf8(
            axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap()
                .to_vec(),
        )
        .unwrap();
        assert!(body.contains(&format!("href=\"{expected_link}\"")), "{uri}");
    }

    let response = test_app()
        .oneshot(
            Request::builder()
                .uri("/verify-email?token=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn retired_and_future_human_routes_are_unmatched() {
    for uri in ["/app", "/app/workspaces", "/account", "/api-keys"] {
        let response = test_app()
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{uri}");
    }
}

#[tokio::test]
async fn workspace_routes_require_an_authenticated_browser_session() {
    let response = test_app()
        .oneshot(
            Request::builder()
                .uri("/workspaces")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/login");
}

#[tokio::test]
async fn api_openapi_document_reflects_the_enabled_transfer_route() {
    let disabled = test_app()
        .oneshot(
            Request::builder()
                .uri("/openapi.json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(disabled.status(), StatusCode::OK);
    assert!(disabled
        .headers()
        .get(CONTENT_TYPE)
        .unwrap()
        .to_str()
        .unwrap()
        .starts_with("application/json"));
    let disabled_json: Value = serde_json::from_slice(
        &axum::body::to_bytes(disabled.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(disabled_json["paths"]["/v1/erc20-transfers/search"].is_null());

    let enabled = build_router(
        HttpStateTestBuilder::new(Config {
            erc20_transfers_enabled: true,
            ..Config::default()
        })
        .build(),
    )
    .oneshot(
        Request::builder()
            .uri("/openapi.json")
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap();
    let enabled_json: Value = serde_json::from_slice(
        &axum::body::to_bytes(enabled.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(enabled_json["paths"]["/v1/erc20-transfers/search"].is_object());
}

#[tokio::test]
async fn static_assets_are_bounded_and_have_an_explicit_cache_policy() {
    let response = test_app()
        .oneshot(
            Request::builder()
                .uri("/assets/site.css")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(CACHE_CONTROL).unwrap(),
        "public, max-age=3600"
    );
    assert!(response
        .headers()
        .get(CONTENT_TYPE)
        .unwrap()
        .to_str()
        .unwrap()
        .starts_with("text/css"));

    for uri in [
        "/app/assets/site.css",
        "/docs/openapi.json",
        "/assets",
        "/assets/missing.css",
        "/assets/%2e%2e/Cargo.toml",
    ] {
        let response = test_app()
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{uri}");
    }
}

#[tokio::test]
async fn beta_surface_keeps_balance_and_health_routes_active() {
    let app = build_router(HttpStateTestBuilder::new(beta_config()).build());

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

        assert_public_auth_error(response, StatusCode::UNAUTHORIZED, "unauthorized").await;
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
    let disabled_response = build_router(HttpStateTestBuilder::new(beta_config()).build())
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

    let enabled_response = build_router(
        HttpStateTestBuilder::new(Config {
            public_api_surface: PublicApiSurface::Beta,
            erc20_transfers_enabled: true,
            ..Config::default()
        })
        .build(),
    )
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

    assert_public_auth_error(enabled_response, StatusCode::UNAUTHORIZED, "unauthorized").await;
}

#[tokio::test]
async fn beta_protected_routes_require_api_key_authentication() {
    let app = beta_app_with_api_key_repository(None);

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

        assert_public_auth_error(response, StatusCode::UNAUTHORIZED, "unauthorized").await;
    }
}

#[tokio::test]
async fn beta_auth_rejects_malformed_unsupported_and_unknown_keys() {
    for (_name, api_key_repository, header) in [
        (
            "malformed",
            Some(ApiKeyRepository::in_memory(Vec::new())),
            "Bearer not-a-key".to_string(),
        ),
        (
            "unsupported",
            Some(ApiKeyRepository::in_memory(Vec::new())),
            format!("Basic {TEST_API_KEY}"),
        ),
        (
            "unknown",
            Some(ApiKeyRepository::in_memory(Vec::new())),
            format!("Bearer {TEST_API_KEY}"),
        ),
    ] {
        let response = beta_app_with_api_key_repository(api_key_repository)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/balances")
                    .header("content-type", "application/json")
                    .header(AUTHORIZATION, header)
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_public_auth_error(response, StatusCode::UNAUTHORIZED, "unauthorized").await;
    }
}

#[tokio::test]
async fn beta_auth_rejects_inactive_or_expired_credentials_without_enumerating() {
    for (_name, lookup) in [
        (
            "disabled",
            ApiKeyLookup {
                key_status: "disabled".to_string(),
                ..active_api_key_lookup()
            },
        ),
        (
            "revoked",
            ApiKeyLookup {
                key_status: "revoked".to_string(),
                ..active_api_key_lookup()
            },
        ),
        (
            "expired",
            ApiKeyLookup {
                expires_at: Some("2026-01-01T00:00:00Z".to_string()),
                is_expired: true,
                ..active_api_key_lookup()
            },
        ),
        (
            "disabled-consumer",
            ApiKeyLookup {
                consumer_status: "disabled".to_string(),
                ..active_api_key_lookup()
            },
        ),
    ] {
        let response = beta_app_with_lookup(lookup)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/balances")
                    .header("content-type", "application/json")
                    .header(AUTHORIZATION, format!("Bearer {TEST_API_KEY}"))
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_public_auth_error(response, StatusCode::UNAUTHORIZED, "unauthorized").await;
    }
}

#[tokio::test]
async fn beta_auth_reports_database_unavailable_for_valid_key_when_repository_fails() {
    let response = beta_app_with_api_key_repository(Some(ApiKeyRepository::unavailable()))
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/balances")
                .header("content-type", "application/json")
                .header(AUTHORIZATION, format!("Bearer {TEST_API_KEY}"))
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_public_auth_error(
        response,
        StatusCode::SERVICE_UNAVAILABLE,
        "database_unavailable",
    )
    .await;
}

#[tokio::test]
async fn beta_auth_allows_valid_key_to_reach_protected_route_handler() {
    let response = beta_app_with_lookup(active_api_key_lookup())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/balances")
                .header("content-type", "application/json")
                .header(AUTHORIZATION, format!("Bearer {TEST_API_KEY}"))
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn beta_route_capabilities_preserve_balance_access_and_restrict_transfer_access() {
    let lookup = active_api_key_lookup();
    let balance_only_grants = ApiKeyAuthorizationGrants {
        owner_grants: vec![CapabilityGrant::active(
            Capability::BalancesRead,
            NetworkScope::Any,
        )],
        key_grants: vec![CapabilityGrant::active(
            Capability::BalancesRead,
            NetworkScope::Any,
        )],
        client_grants: None,
    };
    let repository = ApiKeyRepository::in_memory_with_policies_and_grants(
        vec![(
            TEST_API_KEY_PREFIX.to_string(),
            hash_presented_api_key(TEST_API_KEY).to_vec(),
            lookup.clone(),
        )],
        Default::default(),
        std::collections::HashMap::from([(lookup.api_key_id, balance_only_grants)]),
    );
    let app = build_router(
        HttpStateTestBuilder::new(Config {
            public_api_surface: PublicApiSurface::Beta,
            erc20_transfers_enabled: true,
            ..Config::default()
        })
        .with_api_key_repository(repository)
        .build(),
    );

    let balance = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/balances")
                .header("content-type", "application/json")
                .header(AUTHORIZATION, format!("Bearer {TEST_API_KEY}"))
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(balance.status(), StatusCode::BAD_REQUEST);

    let transfer = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/erc20-transfers/search")
                .header("content-type", "application/json")
                .header(AUTHORIZATION, format!("Bearer {TEST_API_KEY}"))
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_public_auth_error(transfer, StatusCode::FORBIDDEN, "capability_not_granted").await;
}

#[tokio::test]
async fn beta_surface_returns_endpoint_disabled_for_known_non_beta_routes() {
    let app = build_router(HttpStateTestBuilder::new(beta_config()).build());

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
    for app in [
        test_app(),
        build_router(HttpStateTestBuilder::new(beta_config()).build()),
    ] {
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
    let response = build_router(HttpStateTestBuilder::new(beta_config()).build())
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
    let app = build_router(HttpStateTestBuilder::new(beta_config()).build());

    for uri in ["/v1/not-a-route", "/definitely-not-a-route"] {
        let response = app
            .clone()
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{uri}");
    }
}

#[tokio::test]
async fn realized_yield_is_generic_browser_lab_route_and_legacy_aave_route_is_absent() {
    let app = test_app();
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/lab/defi-protocols/realized-yield")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/login");

    for uri in [
        "/lab/aave-v3/realized-yield",
        "/v1/defi-protocols/realized-yield",
    ] {
        let response = app
            .clone()
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{uri}");
    }
}

#[tokio::test]
async fn portfolio_simulation_is_a_private_browser_lab_route() {
    let app = test_app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/lab/portfolio-simulation")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/login");

    let response = test_app()
        .oneshot(
            Request::builder()
                .uri("/v1/portfolio-simulation")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[test]
fn production_caddy_separates_machine_and_human_route_surfaces() {
    let caddyfile = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/infra/caddy/Caddyfile"
    ));

    assert!(caddyfile.contains("{$CADDY_API_DOMAIN}"));
    assert!(caddyfile.contains("{$CADDY_WEB_DOMAIN}"));
    let (api_site, web_site) = caddyfile
        .split_once("{$CADDY_WEB_DOMAIN}")
        .expect("Caddy must declare a dedicated web site");
    assert!(api_site.contains("path /v1/* /health /openapi.json"));
    assert!(!api_site.contains("/scan"));
    assert!(!api_site.contains("/internal/v1"));
    assert!(web_site.contains("path / /scan /scan/* /access /access/demo /docs /docs/* /assets/* /signup /login /logout /workspaces /workspaces/* /catalog /catalog/* /prices /prices/* /lab /lab/* /lab.json"));
    assert!(!web_site.contains("/v1/*"));
    assert!(!web_site.contains("/internal/v1"));
    assert!(caddyfile.contains("reverse_proxy mother-api:3000"));
    assert!(!caddyfile.contains("CADDY_DOMAIN"));
    assert!(!caddyfile.contains("/app"));
    assert!(!caddyfile.contains("method GET"));
    assert!(!caddyfile.contains("rewrite"));
    assert!(!caddyfile.contains("uri strip_prefix"));
    assert!(caddyfile.contains("object-src 'none'"));
    assert!(!caddyfile.contains("'unsafe-inline'"));
}

#[tokio::test]
async fn async_report_callbacks_require_the_dedicated_bigwig_outcome_token() {
    let callback = "/internal/v1/reports/rpt_0123456789abcdef0123456789abcdef/complete";
    let customer_api_key = format!("Bearer {TEST_API_KEY}");

    for authorization in [
        None,
        Some("Bearer gateway-token"),
        Some(customer_api_key.as_str()),
    ] {
        let mut request = Request::builder()
            .method("POST")
            .uri(callback)
            .header(CONTENT_TYPE, "application/json");
        if let Some(authorization) = authorization {
            request = request.header(AUTHORIZATION, authorization);
        }
        let response = async_reports_callback_app()
            .oneshot(
                request
                    .body(Body::from(
                        r#"{"report_type":"test","report_version":1,"report":{}}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    let response = async_reports_callback_app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(callback)
                .header(CONTENT_TYPE, "application/json")
                .header(AUTHORIZATION, "Bearer bigwig-outcome-token")
                .body(Body::from(
                    r#"{"report_type":"test","report_version":1,"report":{}}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn async_report_callback_token_protects_persisted_reports() {
    let Some(pool) = migrated_pool().await else {
        return;
    };
    let database_url = std::env::var(POSTGRES_TEST_DATABASE_URL_ENV).unwrap();
    let account_id = Uuid::new_v4();
    let account_public_id = format!("iba_{}", Uuid::new_v4().simple());
    let report_id = format!("rpt_{}", Uuid::new_v4().simple());

    sqlx::query(
        "insert into mother_api.ib_account (id, public_id, status) values ($1, $2, 'active')",
    )
    .bind(account_id)
    .bind(&account_public_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("insert into mother_api.async_report (id, public_id, ib_account_id, report_type, report_version, input, idempotency_key_hash, request_digest) values ($1, $2, $3, 'unregistered.test.v1', 1, '{}'::jsonb, $4, $5)")
        .bind(Uuid::new_v4())
        .bind(&report_id)
        .bind(account_id)
        .bind(vec![7_u8; 32])
        .bind("0".repeat(64))
        .execute(&pool)
        .await
        .unwrap();

    let app = build_router(
        HttpStateTestBuilder::new(Config {
            public_api_surface: PublicApiSurface::Beta,
            database_url: Some(database_url),
            async_reports_enabled: true,
            bigwig_report_outcome_token: Some("bigwig-outcome-token".to_string()),
            ..Config::default()
        })
        .build(),
    );
    let callback = format!("/internal/v1/reports/{report_id}/complete");
    let body = r#"{"report_type":"unregistered.test.v1","report_version":1,"report":{}}"#;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(&callback)
                .header(CONTENT_TYPE, "application/json")
                .header(AUTHORIZATION, "Bearer wrong-token")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let status = sqlx::query_scalar::<_, String>(
        "select status from mother_api.async_report where public_id = $1",
    )
    .bind(&report_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(status, "accepted");

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(&callback)
                .header(CONTENT_TYPE, "application/json")
                .header(AUTHORIZATION, "Bearer bigwig-outcome-token")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    sqlx::query("delete from mother_api.async_report where public_id = $1")
        .bind(&report_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("delete from mother_api.ib_account where id = $1")
        .bind(account_id)
        .execute(&pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn health_returns_stable_contract() {
    let app = build_router(HttpStateTestBuilder::new(Config::default()).build());

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
    let app = build_router(HttpStateTestBuilder::new(Config::default()).build());

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
    assert_eq!(json["checks"].as_object().unwrap().len(), 4);
    assert_eq!(json["checks"]["evm_indexer"], "not_connected");
}

#[tokio::test]
async fn assets_returns_default_limited_list() {
    let json = assets_json("/v1/assets").await;

    assert_eq!(json["ok"], true);
    assert_eq!(json["type"], "assets");
    assert_eq!(json["limit"], 100);
    assert_eq!(json["count"], 22);
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
    let app = test_app_with_price_indexer(&price_indexer_url, 2000);

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
    assert_eq!(json["count"], 22);
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
async fn assets_resolve_without_a_database() {
    let response = build_router(HttpStateTestBuilder::new(Config::default()).build())
        .oneshot(
            Request::builder()
                .uri("/v1/assets")
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
    assert_eq!(json["type"], "assets");
    assert_eq!(json["assets"][0]["asset_id"], "bitcoin");
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
    let app = test_app_with_price_indexer(&price_indexer_url, 2000);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/assets/usdc")
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

    assert_eq!(json["asset"]["asset_id"], "usdc");
    assert_eq!(json["asset"]["symbol"], "USDC");
    assert_eq!(json["price"]["status"], "available");

    let request = request_handle.await.unwrap();
    assert!(request.starts_with("GET /prices/latest?slug=usdc&quoteCurrency=USD "));
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
async fn asset_detail_resolves_without_a_database() {
    let response = build_router(HttpStateTestBuilder::new(Config::default()).build())
        .oneshot(
            Request::builder()
                .uri("/v1/assets/bitcoin")
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
    assert_eq!(json["asset"]["asset_id"], "bitcoin");
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
        build_router(HttpStateTestBuilder::new(Config::default()).build()),
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
        ("ausd", "/assets/agora-usd"),
        ("bitcoin", "/assets/bitcoin"),
        ("btc", "/assets/bitcoin"),
        ("usds", "/assets/usds"),
        ("ethereum", "/assets/ethereum"),
        ("eth", "/assets/ethereum"),
        ("gho", "/assets/gho"),
        ("wbtc", "/assets/wrapped-bitcoin"),
        ("wrapped%20bitcoin", "/assets/wrapped-bitcoin"),
        ("mantle", "/assets/mantle"),
        ("mnt", "/assets/mantle"),
        ("mpdao", "/assets/metapool-dao"),
        ("near%20protocol", "/assets/near"),
        ("stnear", "/assets/staked-near"),
        ("usdt", "/assets/usdt"),
        ("usdt0", "/assets/usdt0"),
        ("usde", "/assets/usde"),
        ("weth", "/assets/wrapped-ether"),
        ("cmeth", "/assets/mantle-cmeth"),
        ("meth", "/assets/mantle-staked-ether"),
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
async fn resolve_resolves_without_a_database() {
    let response = build_router(HttpStateTestBuilder::new(Config::default()).build())
        .oneshot(
            Request::builder()
                .uri("/v1/assets/resolve?q=usdc")
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
    assert_eq!(json["resolved"], true);
    assert_eq!(json["result"]["asset"]["asset_id"], "usdc");
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

async fn assert_public_auth_error(
    response: axum::response::Response,
    expected_status: StatusCode,
    expected_code: &str,
) {
    assert_eq!(response.status(), expected_status);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["ok"], false);
    assert_eq!(json["error"]["code"], expected_code);
}

fn active_api_key_lookup() -> ApiKeyLookup {
    ApiKeyLookup {
        api_key_id: Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap(),
        ib_account_id: None,
        client_id: None,
        key_kind: "legacy".to_string(),
        consumer_id: Uuid::parse_str("22222222-2222-4222-8222-222222222222").unwrap(),
        consumer_slug: "first-customer".to_string(),
        consumer_category: "partner".to_string(),
        consumer_status: "active".to_string(),
        key_prefix: TEST_API_KEY_PREFIX.to_string(),
        key_label: "beta access key".to_string(),
        key_status: "active".to_string(),
        hash_algorithm: "sha256".to_string(),
        expires_at: None,
        is_expired: false,
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
