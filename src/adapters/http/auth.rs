use axum::{
    extract::{Request, State},
    http::header::AUTHORIZATION,
    middleware::Next,
    response::{IntoResponse, Response},
};
use tracing::warn;
use uuid::Uuid;

use crate::{
    adapters::{http::error::ApiError, postgres::api_keys::ApiKeyLookup},
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
            request.extensions_mut().insert(principal);
            next.run(request).await
        }
        Err(AuthError::Unauthorized) => ApiError::unauthorized().into_response(),
        Err(AuthError::DatabaseUnavailable) => {
            ApiError::database_unavailable_for_auth().into_response()
        }
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
        adapters::postgres::ApiKeyRepository,
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

    async fn principal_echo(Extension(principal): Extension<ApiKeyPrincipal>) -> Json<Value> {
        Json(json!({
            "consumer_slug": principal.consumer_slug,
            "consumer_category": principal.consumer_category,
            "key_prefix": principal.key_prefix,
            "key_label": principal.key_label,
        }))
    }

    fn state_with_lookup(lookup: ApiKeyLookup) -> AppState {
        AppState {
            config: Config {
                public_api_surface: PublicApiSurface::Beta,
                ..Config::default()
            },
            version: env!("CARGO_PKG_VERSION"),
            database_pool: None,
            api_key_repository: Some(ApiKeyRepository::in_memory(vec![(
                TEST_PREFIX.to_string(),
                hash_presented_api_key(TEST_KEY).to_vec(),
                lookup,
            )])),
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
