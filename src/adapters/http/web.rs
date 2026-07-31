use askama::Template;
use axum::{
    extract::{Extension, Path, Request, State},
    http::{
        header::{
            CONTENT_SECURITY_POLICY, CONTENT_TYPE, REFERRER_POLICY, X_CONTENT_TYPE_OPTIONS,
            X_FRAME_OPTIONS,
        },
        HeaderValue, StatusCode,
    },
    middleware::Next,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};

use crate::state::AppState;

const HTML_CONTENT_TYPE: &str = "text/html; charset=utf-8";
const CONTENT_SECURITY_POLICY_VALUE: &str = "default-src 'self'; base-uri 'self'; form-action 'self'; frame-ancestors 'none'; object-src 'none'; script-src 'self'; style-src 'self'; img-src 'self'; connect-src 'self'";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BrowserPrincipal {
    Anonymous,
}

pub(crate) fn routes() -> Router<AppState> {
    html_routes()
}

fn html_routes() -> Router<AppState> {
    Router::new()
        .route("/", get(home))
        .route("/scan", get(scan))
        .route("/scan/{network_slug}", get(scan_network))
        .route("/access", get(access))
        .route("/docs", get(docs))
        .route_layer(axum::middleware::from_fn(attach_anonymous_browser_context))
}

pub(crate) async fn attach_anonymous_browser_context(mut request: Request, next: Next) -> Response {
    request.extensions_mut().insert(BrowserPrincipal::Anonymous);
    next.run(request).await
}

async fn home(Extension(principal): Extension<BrowserPrincipal>) -> Response {
    debug_assert_eq!(principal, BrowserPrincipal::Anonymous);
    html_response(HomeTemplate)
}

async fn scan(Extension(principal): Extension<BrowserPrincipal>) -> Response {
    debug_assert_eq!(principal, BrowserPrincipal::Anonymous);
    html_response(ScanTemplate { network_slug: None })
}

async fn scan_network(
    Extension(principal): Extension<BrowserPrincipal>,
    Path(network_slug): Path<String>,
) -> Response {
    debug_assert_eq!(principal, BrowserPrincipal::Anonymous);
    html_response(ScanTemplate {
        network_slug: Some(network_slug),
    })
}

async fn access(Extension(principal): Extension<BrowserPrincipal>) -> Response {
    debug_assert_eq!(principal, BrowserPrincipal::Anonymous);
    html_response(AccessTemplate)
}

async fn docs(
    Extension(principal): Extension<BrowserPrincipal>,
    State(state): State<AppState>,
) -> Response {
    debug_assert_eq!(principal, BrowserPrincipal::Anonymous);
    html_response(DocsTemplate {
        openapi_url: format!(
            "{}/openapi.json",
            state.config.public_api_base_url.trim_end_matches('/')
        ),
    })
}

pub(crate) async fn openapi_document(
    State(state): State<AppState>,
) -> Json<utoipa::openapi::OpenApi> {
    Json(crate::openapi::document(&state.config))
}

fn html_response(template: impl Template) -> Response {
    let Ok(body) = template.render() else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };

    let mut response = body.into_response();
    let headers = response.headers_mut();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static(HTML_CONTENT_TYPE));
    headers.insert(
        CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(CONTENT_SECURITY_POLICY_VALUE),
    );
    headers.insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    headers.insert(X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    headers.insert(
        REFERRER_POLICY,
        HeaderValue::from_static("strict-origin-when-cross-origin"),
    );
    response
}

#[derive(Template)]
#[template(path = "web/home.html")]
struct HomeTemplate;

#[derive(Template)]
#[template(path = "web/scan.html")]
struct ScanTemplate {
    network_slug: Option<String>,
}

#[derive(Template)]
#[template(path = "web/access.html")]
struct AccessTemplate;

#[derive(Template)]
#[template(path = "web/docs.html")]
struct DocsTemplate {
    openapi_url: String,
}
