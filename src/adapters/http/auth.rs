use axum::{
    extract::{Request, State},
    http::{header::AUTHORIZATION, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use tracing::warn;
use uuid::Uuid;

use crate::{
    adapters::{
        http::error::ApiError,
        postgres::api_keys::{ApiKeyLookup, DailyAcceptedOutcome, UsageResponseClass},
    },
    domain::api_keys::{hash_presented_api_key, parse_presented_api_key},
    state::AppState,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ApiKeyPrincipal {
    pub(crate) api_key_id: Uuid,
    pub(crate) consumer_id: Uuid,
    pub(crate) consumer_slug: String,
    pub(crate) consumer_category: String,
    pub(crate) key_prefix: String,
    pub(crate) key_label: String,
}

pub(crate) async fn require_api_key(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Response {
    match authenticate(&state, request.headers()).await {
        Ok(principal) => {
            let repository = state
                .api_key_repository
                .as_ref()
                .expect("authenticated API-key request should have repository");
            let policy = match repository.find_policy(principal.api_key_id).await {
                Ok(Some(policy)) => policy,
                Ok(None) => {
                    warn!(
                        api_key_id = %principal.api_key_id,
                        key_prefix = %principal.key_prefix,
                        "API-key policy missing for authenticated request"
                    );
                    return ApiError::database_unavailable_for_auth().into_response();
                }
                Err(error) => {
                    warn!(
                        %error,
                        api_key_id = %principal.api_key_id,
                        key_prefix = %principal.key_prefix,
                        "API-key policy lookup failed"
                    );
                    return ApiError::database_unavailable_for_auth().into_response();
                }
            };

            let Some(minute_reservation) = state
                .api_key_minute_limiter
                .reserve(principal.api_key_id, policy.requests_per_minute)
            else {
                record_rate_limited(repository, &principal).await;
                return ApiError::rate_limited().into_response();
            };

            match repository
                .increment_daily_accepted(principal.api_key_id)
                .await
            {
                Ok(DailyAcceptedOutcome::Accepted) => {}
                Ok(DailyAcceptedOutcome::LimitExceeded) => {
                    state.api_key_minute_limiter.release(minute_reservation);
                    record_rate_limited(repository, &principal).await;
                    return ApiError::rate_limited().into_response();
                }
                Ok(DailyAcceptedOutcome::MissingPolicy) => {
                    state.api_key_minute_limiter.release(minute_reservation);
                    warn!(
                        api_key_id = %principal.api_key_id,
                        key_prefix = %principal.key_prefix,
                        "API-key policy disappeared before daily usage update"
                    );
                    return ApiError::database_unavailable_for_auth().into_response();
                }
                Err(error) => {
                    state.api_key_minute_limiter.release(minute_reservation);
                    warn!(
                        %error,
                        api_key_id = %principal.api_key_id,
                        key_prefix = %principal.key_prefix,
                        "API-key daily accepted counter update failed"
                    );
                    return ApiError::database_unavailable_for_auth().into_response();
                }
            }

            request.extensions_mut().insert(principal.clone());
            let response = next.run(request).await;
            record_response_class(repository, &principal, response.status()).await;
            response
        }
        Err(AuthError::Unauthorized) => ApiError::unauthorized().into_response(),
        Err(AuthError::DatabaseUnavailable) => {
            ApiError::database_unavailable_for_auth().into_response()
        }
    }
}

async fn record_rate_limited(
    repository: &crate::adapters::postgres::ApiKeyRepository,
    principal: &ApiKeyPrincipal,
) {
    if let Err(error) = repository
        .increment_daily_rate_limited(principal.api_key_id)
        .await
    {
        warn!(
            %error,
            api_key_id = %principal.api_key_id,
            key_prefix = %principal.key_prefix,
            "API-key rate-limited counter update failed"
        );
    }
}

async fn record_response_class(
    repository: &crate::adapters::postgres::ApiKeyRepository,
    principal: &ApiKeyPrincipal,
    status: StatusCode,
) {
    let response_class = if status.is_success() || status.is_redirection() {
        Some(UsageResponseClass::Successful)
    } else if status.is_client_error() {
        Some(UsageResponseClass::ClientError)
    } else if status.is_server_error() {
        Some(UsageResponseClass::ServerError)
    } else {
        None
    };

    let Some(response_class) = response_class else {
        return;
    };

    if let Err(error) = repository
        .increment_daily_response(principal.api_key_id, response_class)
        .await
    {
        warn!(
            %error,
            api_key_id = %principal.api_key_id,
            key_prefix = %principal.key_prefix,
            status = status.as_u16(),
            "API-key response usage counter update failed"
        );
    }
}

async fn authenticate(
    state: &AppState,
    headers: &axum::http::HeaderMap,
) -> Result<ApiKeyPrincipal, AuthError> {
    let presented_key = bearer_credential(headers)?;
    let parsed_key = parse_presented_api_key(presented_key).map_err(|_| AuthError::Unauthorized)?;
    let key_hash = hash_presented_api_key(presented_key);
    let repository = state
        .api_key_repository
        .as_ref()
        .ok_or(AuthError::DatabaseUnavailable)?;
    let lookup = repository
        .find_key_by_prefix_and_hash(&parsed_key.key_prefix, &key_hash)
        .await
        .map_err(|error| {
            warn!(%error, "API-key authentication repository lookup failed");
            AuthError::DatabaseUnavailable
        })?
        .ok_or(AuthError::Unauthorized)?;

    principal_from_lookup(lookup)
}

fn bearer_credential(headers: &axum::http::HeaderMap) -> Result<&str, AuthError> {
    let mut values = headers.get_all(AUTHORIZATION).iter();
    let Some(value) = values.next() else {
        return Err(AuthError::Unauthorized);
    };
    if values.next().is_some() {
        return Err(AuthError::Unauthorized);
    }

    let value = value.to_str().map_err(|_| AuthError::Unauthorized)?;
    let mut parts = value.split_ascii_whitespace();
    let Some(scheme) = parts.next() else {
        return Err(AuthError::Unauthorized);
    };
    let Some(credential) = parts.next() else {
        return Err(AuthError::Unauthorized);
    };
    if parts.next().is_some() || !scheme.eq_ignore_ascii_case("Bearer") {
        return Err(AuthError::Unauthorized);
    }

    Ok(credential)
}

fn principal_from_lookup(lookup: ApiKeyLookup) -> Result<ApiKeyPrincipal, AuthError> {
    if lookup.key_status != "active"
        || lookup.consumer_status != "active"
        || lookup.is_expired
        || lookup.hash_algorithm != "sha256"
    {
        return Err(AuthError::Unauthorized);
    }

    Ok(ApiKeyPrincipal {
        api_key_id: lookup.api_key_id,
        consumer_id: lookup.consumer_id,
        consumer_slug: lookup.consumer_slug,
        consumer_category: lookup.consumer_category,
        key_prefix: lookup.key_prefix,
        key_label: lookup.key_label,
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AuthError {
    Unauthorized,
    DatabaseUnavailable,
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        },
    };

    use axum::{
        body::Body,
        extract::Extension,
        http::{Request, StatusCode},
        middleware,
        response::Json,
        routing::get,
        Router,
    };
    use serde_json::{json, Value};
    use tower::ServiceExt;

    use super::*;
    use crate::{
        adapters::postgres::{
            api_keys::{ApiKeyPolicy, InMemoryApiKeyUsageSnapshot},
            ApiKeyRepository,
        },
        config::{Config, PublicApiSurface},
        domain::api_keys::hash_presented_api_key,
    };

    const TEST_KEY: &str =
        "ib_live_0123456789abcdef.0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    const TEST_PREFIX: &str = "ib_live_0123456789abcdef";

    #[tokio::test]
    async fn attaches_api_key_principal_to_request_extensions() {
        let app = Router::new()
            .route("/protected", get(principal_echo))
            .route_layer(middleware::from_fn_with_state(
                state_with_lookup(active_lookup()),
                require_api_key,
            ));

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/protected")
                    .header(AUTHORIZATION, format!("Bearer {TEST_KEY}"))
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

        assert_eq!(json["consumer_slug"], "first-customer");
        assert_eq!(json["consumer_category"], "partner");
        assert_eq!(json["key_prefix"], TEST_PREFIX);
        assert_eq!(json["key_label"], "beta access key");
    }

    #[tokio::test]
    async fn accepted_request_records_usage_and_last_used() {
        let repository = repository_with_policy(60, 5000);
        let app = Router::new()
            .route("/protected", get(|| async { StatusCode::OK }))
            .route_layer(middleware::from_fn_with_state(
                state_with_repository(repository.clone()),
                require_api_key,
            ));

        let response = app.oneshot(authorized_request("/protected")).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            repository.in_memory_usage(active_lookup().api_key_id),
            InMemoryApiKeyUsageSnapshot {
                accepted_requests: 1,
                successful_responses: 1,
                api_key_last_used_updated: true,
                daily_last_used_updated: true,
                ..InMemoryApiKeyUsageSnapshot::default()
            }
        );
    }

    #[tokio::test]
    async fn minute_limit_returns_429_counts_rate_limit_and_skips_handler() {
        let repository = repository_with_policy(1, 5000);
        let handler_calls = Arc::new(AtomicUsize::new(0));
        let calls = handler_calls.clone();
        let app = Router::new()
            .route(
                "/protected",
                get(move || {
                    let calls = calls.clone();
                    async move {
                        calls.fetch_add(1, Ordering::SeqCst);
                        StatusCode::OK
                    }
                }),
            )
            .route_layer(middleware::from_fn_with_state(
                state_with_repository(repository.clone()),
                require_api_key,
            ));

        let accepted = app
            .clone()
            .oneshot(authorized_request("/protected"))
            .await
            .unwrap();
        let limited = app.oneshot(authorized_request("/protected")).await.unwrap();

        assert_eq!(accepted.status(), StatusCode::OK);
        assert_rate_limited(limited).await;
        assert_eq!(handler_calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            repository.in_memory_usage(active_lookup().api_key_id),
            InMemoryApiKeyUsageSnapshot {
                accepted_requests: 1,
                rate_limited_requests: 1,
                successful_responses: 1,
                api_key_last_used_updated: true,
                daily_last_used_updated: true,
                ..InMemoryApiKeyUsageSnapshot::default()
            }
        );
    }

    #[tokio::test]
    async fn daily_limit_returns_429_counts_rate_limit_and_releases_minute_slot() {
        let repository = repository_with_policy(2, 1);
        let app = Router::new()
            .route("/protected", get(|| async { StatusCode::OK }))
            .route_layer(middleware::from_fn_with_state(
                state_with_repository(repository.clone()),
                require_api_key,
            ));

        let accepted = app
            .clone()
            .oneshot(authorized_request("/protected"))
            .await
            .unwrap();
        let limited = app
            .clone()
            .oneshot(authorized_request("/protected"))
            .await
            .unwrap();
        let still_daily_limited = app.oneshot(authorized_request("/protected")).await.unwrap();

        assert_eq!(accepted.status(), StatusCode::OK);
        assert_rate_limited(limited).await;
        assert_rate_limited(still_daily_limited).await;
        assert_eq!(
            repository.in_memory_usage(active_lookup().api_key_id),
            InMemoryApiKeyUsageSnapshot {
                accepted_requests: 1,
                rate_limited_requests: 2,
                successful_responses: 1,
                api_key_last_used_updated: true,
                daily_last_used_updated: true,
                ..InMemoryApiKeyUsageSnapshot::default()
            }
        );
    }

    #[tokio::test]
    async fn response_status_classes_increment_exactly_one_counter_each() {
        let repository = repository_with_policy(60, 5000);
        let app = Router::new()
            .route("/ok", get(|| async { StatusCode::NO_CONTENT }))
            .route("/client", get(|| async { StatusCode::BAD_REQUEST }))
            .route(
                "/server",
                get(|| async { StatusCode::INTERNAL_SERVER_ERROR }),
            )
            .route_layer(middleware::from_fn_with_state(
                state_with_repository(repository.clone()),
                require_api_key,
            ));

        for path in ["/ok", "/client", "/server"] {
            let _ = app.clone().oneshot(authorized_request(path)).await.unwrap();
        }

        assert_eq!(
            repository.in_memory_usage(active_lookup().api_key_id),
            InMemoryApiKeyUsageSnapshot {
                accepted_requests: 3,
                successful_responses: 1,
                client_error_responses: 1,
                server_error_responses: 1,
                api_key_last_used_updated: true,
                daily_last_used_updated: true,
                ..InMemoryApiKeyUsageSnapshot::default()
            }
        );
    }

    async fn principal_echo(Extension(principal): Extension<ApiKeyPrincipal>) -> Json<Value> {
        Json(json!({
            "consumer_slug": principal.consumer_slug,
            "consumer_category": principal.consumer_category,
            "key_prefix": principal.key_prefix,
            "key_label": principal.key_label,
        }))
    }

    async fn assert_rate_limited(response: Response) {
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["ok"], false);
        assert_eq!(json["error"]["code"], "rate_limited");
        assert!(json["error"]["message"].as_str().unwrap().contains("limit"));
    }

    fn authorized_request(path: &str) -> Request<Body> {
        Request::builder()
            .uri(path)
            .header(AUTHORIZATION, format!("Bearer {TEST_KEY}"))
            .body(Body::empty())
            .unwrap()
    }

    fn repository_with_policy(requests_per_minute: i32, requests_per_day: i32) -> ApiKeyRepository {
        let lookup = active_lookup();
        let mut policies = HashMap::new();
        policies.insert(
            lookup.api_key_id,
            ApiKeyPolicy {
                api_key_id: lookup.api_key_id,
                requests_per_minute,
                requests_per_day,
            },
        );

        ApiKeyRepository::in_memory_with_policies(
            vec![(
                TEST_PREFIX.to_string(),
                hash_presented_api_key(TEST_KEY).to_vec(),
                lookup,
            )],
            policies,
        )
    }

    fn state_with_lookup(lookup: ApiKeyLookup) -> AppState {
        state_with_repository(ApiKeyRepository::in_memory(vec![(
            TEST_PREFIX.to_string(),
            hash_presented_api_key(TEST_KEY).to_vec(),
            lookup,
        )]))
    }

    fn state_with_repository(api_key_repository: ApiKeyRepository) -> AppState {
        AppState {
            config: Config {
                public_api_surface: PublicApiSurface::Beta,
                ..Config::default()
            },
            version: env!("CARGO_PKG_VERSION"),
            database_pool: None,
            api_key_repository: Some(api_key_repository),
            api_key_minute_limiter: crate::adapters::http::rate_limit::ApiKeyMinuteLimiter::default(
            ),
            asset_repository: None,
            price_indexer_client: None,
            dis_client: None,
            bigwig_client: None,
        }
    }

    fn active_lookup() -> ApiKeyLookup {
        ApiKeyLookup {
            api_key_id: Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap(),
            consumer_id: Uuid::parse_str("22222222-2222-4222-8222-222222222222").unwrap(),
            consumer_slug: "first-customer".to_string(),
            consumer_category: "partner".to_string(),
            consumer_status: "active".to_string(),
            key_prefix: TEST_PREFIX.to_string(),
            key_label: "beta access key".to_string(),
            key_status: "active".to_string(),
            hash_algorithm: "sha256".to_string(),
            expires_at: None,
            is_expired: false,
        }
    }
}
