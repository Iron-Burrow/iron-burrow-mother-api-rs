use askama::Template;
use axum::{
    extract::{Extension, Form, Request, State},
    http::{
        header::{
            CACHE_CONTROL, CONTENT_SECURITY_POLICY, CONTENT_TYPE, COOKIE, ORIGIN, REFERRER_POLICY,
            SET_COOKIE, X_CONTENT_TYPE_OPTIONS, X_FRAME_OPTIONS,
        },
        HeaderMap, HeaderName, HeaderValue, StatusCode,
    },
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tracing::warn;
use uuid::Uuid;

use crate::{
    adapters::{http::state::HttpState, postgres::accounts::SignupOutcome},
    domain::{api_keys::RawApiKey, passwords},
};

const HTML_CONTENT_TYPE: &str = "text/html; charset=utf-8";
const CONTENT_SECURITY_POLICY_VALUE: &str = "default-src 'self'; base-uri 'self'; form-action 'self'; frame-ancestors 'none'; object-src 'none'; script-src 'self'; style-src 'self'; img-src 'self'; connect-src 'self'";
const SESSION_COOKIE: &str = "__Host-ib_session";
const CSRF_COOKIE: &str = "__Host-ib_csrf";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum BrowserPrincipal {
    Anonymous,
    Authenticated {
        account_id: Uuid,
        account_public_id: String,
        csrf_hash: Vec<u8>,
    },
}

pub(crate) fn routes(state: HttpState) -> Router<HttpState> {
    html_routes(state.clone())
        .merge(super::workspaces::routes(state.clone()))
        .merge(super::data_lab::routes(state))
}

fn html_routes(state: HttpState) -> Router<HttpState> {
    Router::new()
        .route("/", get(home))
        .route("/scan", get(scan))
        .route("/scan/{network_slug}", get(scan_network))
        .route("/access", get(access))
        .route("/access/demo", post(issue_demo))
        .route("/docs", get(docs))
        .route("/signup", get(signup).post(signup_submit))
        .route("/login", get(login).post(login_submit))
        .route("/logout", post(logout))
        .route_layer(axum::middleware::from_fn_with_state(
            state,
            attach_browser_context,
        ))
}
pub(crate) async fn attach_browser_context(
    State(state): State<HttpState>,
    mut request: Request,
    next: Next,
) -> Response {
    let principal = match (
        state.account_repository.as_ref(),
        cookie_value(request.headers(), SESSION_COOKIE),
    ) {
        (Some(repository), Some(value)) => match repository.find_session(&hash(value)).await {
            Ok(Some(session)) => BrowserPrincipal::Authenticated {
                account_id: session.ib_account_id,
                account_public_id: session.public_id,
                csrf_hash: session.csrf_hash,
            },
            Ok(None) => BrowserPrincipal::Anonymous,
            Err(error) => {
                warn!(%error, "browser session lookup failed");
                BrowserPrincipal::Anonymous
            }
        },
        _ => BrowserPrincipal::Anonymous,
    };
    request.extensions_mut().insert(principal);
    next.run(request).await
}

async fn home(Extension(_): Extension<BrowserPrincipal>) -> Response {
    html_response(HomeTemplate)
}
async fn scan(Extension(_): Extension<BrowserPrincipal>) -> Response {
    html_response(ScanTemplate { network_slug: None })
}
async fn scan_network(
    Extension(_): Extension<BrowserPrincipal>,
    axum::extract::Path(network_slug): axum::extract::Path<String>,
) -> Response {
    html_response(ScanTemplate {
        network_slug: Some(network_slug),
    })
}

async fn access(State(state): State<HttpState>) -> Response {
    let intent = match create_token() {
        Some(token) => match state.account_repository.as_ref() {
            Some(repository) if repository.create_demo_intent(&hash(&token)).await.is_ok() => {
                Some(token)
            }
            _ => None,
        },
        None => None,
    };
    access_response(intent)
}

async fn docs(State(state): State<HttpState>) -> Response {
    html_response(DocsTemplate {
        openapi_url: format!(
            "{}/openapi.json",
            state.config.public_api_base_url.trim_end_matches('/')
        ),
    })
}
async fn signup(Extension(principal): Extension<BrowserPrincipal>) -> Response {
    account_entry_response("/signup", "Create your account", None, principal)
}
async fn login(Extension(principal): Extension<BrowserPrincipal>) -> Response {
    account_entry_response("/login", "Sign in", None, principal)
}

#[derive(Deserialize)]
struct AccountForm {
    email: String,
    password: String,
    csrf: String,
}
async fn signup_submit(
    State(state): State<HttpState>,
    headers: HeaderMap,
    Extension(principal): Extension<BrowserPrincipal>,
    Form(form): Form<AccountForm>,
) -> Response {
    if matches!(principal, BrowserPrincipal::Authenticated { .. }) {
        return Redirect::to("/lab").into_response();
    }
    if !entry_csrf_valid(&state, &headers, &form.csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Some(email) = normalize_email(&form.email) else {
        return account_entry_response(
            "/signup",
            "Create your account",
            Some("Enter a valid email address."),
            BrowserPrincipal::Anonymous,
        );
    };
    let Ok(password_hash) = passwords::hash(&form.password) else {
        return account_entry_response(
            "/signup",
            "Create your account",
            Some("Use a password between 12 and 128 characters."),
            BrowserPrincipal::Anonymous,
        );
    };
    let (Some(repository), Some(pepper), Some(session), Some(csrf)) = (
        state.account_repository.as_ref(),
        state.config.account_email_lookup_pepper.as_deref(),
        create_token(),
        create_token(),
    ) else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match repository
        .signup_and_create_session(
            &email,
            &hash_with_pepper(&email, pepper),
            &password_hash,
            &hash(&session),
            &hash(&csrf),
        )
        .await
    {
        Ok(SignupOutcome::Created) => authenticated_redirect(&session, &csrf),
        Ok(SignupOutcome::AlreadyRegistered) => account_entry_response(
            "/signup",
            "Create your account",
            Some("We could not create an account with those credentials."),
            BrowserPrincipal::Anonymous,
        ),
        Err(error) => {
            warn!(%error, "account signup failed");
            StatusCode::SERVICE_UNAVAILABLE.into_response()
        }
    }
}
async fn login_submit(
    State(state): State<HttpState>,
    headers: HeaderMap,
    Extension(principal): Extension<BrowserPrincipal>,
    Form(form): Form<AccountForm>,
) -> Response {
    if matches!(principal, BrowserPrincipal::Authenticated { .. }) {
        return Redirect::to("/lab").into_response();
    }
    if !entry_csrf_valid(&state, &headers, &form.csrf) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Some(email) = normalize_email(&form.email) else {
        passwords::verify_dummy(&form.password);
        return invalid_credentials_response();
    };
    let Some(repository) = state.account_repository.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let Some(pepper) = state.config.account_email_lookup_pepper.as_deref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let identity = match repository
        .find_password_login_identity(&hash_with_pepper(&email, pepper))
        .await
    {
        Ok(value) => value,
        Err(error) => {
            warn!(%error, "password login lookup failed");
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
    };
    let Some(identity) = identity else {
        passwords::verify_dummy(&form.password);
        return invalid_credentials_response();
    };
    let usable = identity.account_status == "active" && identity.identity_status != "disabled";
    if !usable || identity.password_hash.is_empty() {
        passwords::verify_dummy(&form.password);
        return invalid_credentials_response();
    }
    if !passwords::verify(&form.password, &identity.password_hash) {
        return invalid_credentials_response();
    }
    let (Some(session), Some(csrf)) = (create_token(), create_token()) else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    if passwords::needs_rehash(&identity.password_hash) {
        if let Ok(replacement) = passwords::hash(&form.password) {
            if let Err(error) = repository
                .update_password_hash(identity.account_identity_id, &replacement)
                .await
            {
                warn!(%error, "password hash upgrade failed");
                return StatusCode::SERVICE_UNAVAILABLE.into_response();
            }
        }
    }
    match repository
        .create_password_login_session(identity.account_identity_id, &hash(&session), &hash(&csrf))
        .await
    {
        Ok(true) => authenticated_redirect(&session, &csrf),
        Ok(false) => invalid_credentials_response(),
        Err(error) => {
            warn!(%error, "password login session creation failed");
            StatusCode::SERVICE_UNAVAILABLE.into_response()
        }
    }
}

fn account_entry_response(
    action: &'static str,
    heading: &'static str,
    error: Option<&'static str>,
    principal: BrowserPrincipal,
) -> Response {
    if matches!(principal, BrowserPrincipal::Authenticated { .. }) {
        return Redirect::to("/lab").into_response();
    }
    let Some(csrf) = create_token() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let mut response = secret_html_response(AccountEntryTemplate {
        action,
        heading,
        csrf: csrf.clone(),
        error,
    });
    response
        .headers_mut()
        .append(SET_COOKIE, cookie_header(CSRF_COOKIE, &csrf, false));
    response
}

fn invalid_credentials_response() -> Response {
    let mut response = account_entry_response(
        "/login",
        "Sign in",
        Some("Invalid email or password."),
        BrowserPrincipal::Anonymous,
    );
    *response.status_mut() = StatusCode::UNAUTHORIZED;
    response
}

fn entry_csrf_valid(state: &HttpState, headers: &HeaderMap, submitted: &str) -> bool {
    same_origin(headers, &state.config.public_web_base_url)
        && cookie_value(headers, CSRF_COOKIE) == Some(submitted)
}

fn authenticated_redirect(session: &str, csrf: &str) -> Response {
    let mut response = Redirect::to("/lab").into_response();
    response
        .headers_mut()
        .append(SET_COOKIE, cookie_header(SESSION_COOKIE, session, true));
    response
        .headers_mut()
        .append(SET_COOKIE, cookie_header(CSRF_COOKIE, csrf, false));
    response
}

#[derive(Deserialize)]
struct DemoForm {
    intent: String,
}
async fn issue_demo(
    State(state): State<HttpState>,
    headers: HeaderMap,
    Form(form): Form<DemoForm>,
) -> Response {
    if !same_origin(&headers, &state.config.public_web_base_url) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let (Some(repository), Ok(raw_key)) =
        (state.account_repository.as_ref(), RawApiKey::generate())
    else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let prefix = match raw_key.key_prefix() {
        Ok(value) => value,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    match repository
        .consume_demo_intent_and_issue_key(&hash(&form.intent), &prefix, &raw_key.sha256_hash())
        .await
    {
        Ok(Some(issued)) => secret_html_response(DemoKeyTemplate {
            raw_key: raw_key.expose_secret().to_string(),
            expires_at: issued.expires_at,
        }),
        Ok(None) => html_response(MessageTemplate {
            heading: "Demo key unavailable",
            message: "Request a new demo key from the access page.",
        }),
        Err(error) => {
            warn!(%error, "anonymous demo issuance failed");
            StatusCode::SERVICE_UNAVAILABLE.into_response()
        }
    }
}

#[derive(Deserialize)]
struct LogoutForm {
    csrf: String,
}
async fn logout(
    State(state): State<HttpState>,
    headers: HeaderMap,
    Extension(principal): Extension<BrowserPrincipal>,
    Form(form): Form<LogoutForm>,
) -> Response {
    let (
        BrowserPrincipal::Authenticated { csrf_hash, .. },
        Some(session),
        Some(csrf),
        Some(repository),
    ) = (
        principal,
        cookie_value(&headers, SESSION_COOKIE),
        cookie_value(&headers, CSRF_COOKIE),
        state.account_repository.as_ref(),
    )
    else {
        return Redirect::to("/").into_response();
    };
    if !same_origin(&headers, &state.config.public_web_base_url)
        || csrf != form.csrf
        || hash(csrf).as_slice() != csrf_hash.as_slice()
    {
        return StatusCode::FORBIDDEN.into_response();
    }
    if let Err(error) = repository.revoke_session(&hash(session)).await {
        warn!(%error, "browser logout failed");
    }
    let mut response = Redirect::to("/").into_response();
    response
        .headers_mut()
        .append(SET_COOKIE, expired_cookie(SESSION_COOKIE, true));
    response
        .headers_mut()
        .append(SET_COOKIE, expired_cookie(CSRF_COOKIE, false));
    response
}

pub(crate) async fn openapi_document(
    State(state): State<HttpState>,
) -> Json<utoipa::openapi::OpenApi> {
    Json(crate::openapi::document(&state.config))
}

pub(crate) fn html_response(template: impl Template) -> Response {
    response_with_template(template, false)
}
pub(crate) fn secret_html_response(template: impl Template) -> Response {
    response_with_template(template, true)
}
pub(crate) fn private_html_response(template: impl Template) -> Response {
    let mut response = html_response(template);
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    response
}
fn access_response(demo_intent: Option<String>) -> Response {
    let response_is_secret = demo_intent.is_some();
    let template = AccessTemplate { demo_intent };
    if response_is_secret {
        secret_html_response(template)
    } else {
        html_response(template)
    }
}
fn response_with_template(template: impl Template, secret: bool) -> Response {
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
        HeaderValue::from_static(if secret {
            "no-referrer"
        } else {
            "strict-origin-when-cross-origin"
        }),
    );
    if secret {
        headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
        headers.insert(
            HeaderName::from_static("x-robots-tag"),
            HeaderValue::from_static("noindex, nofollow"),
        );
    }
    response
}
fn create_token() -> Option<String> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes).ok()?;
    Some(hex::encode(bytes))
}
pub(crate) fn hash(value: &str) -> Vec<u8> {
    Sha256::digest(value.as_bytes()).to_vec()
}
fn hash_with_pepper(value: &str, pepper: &str) -> Vec<u8> {
    let mut digest = Sha256::new();
    digest.update(value.as_bytes());
    digest.update([0]);
    digest.update(pepper.as_bytes());
    digest.finalize().to_vec()
}
fn normalize_email(value: &str) -> Option<String> {
    let value = value.trim().to_ascii_lowercase();
    (value.len() <= 254 && value.contains('@') && !value.contains(char::is_whitespace))
        .then_some(value)
}
pub(crate) fn cookie_value<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get(COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .map(str::trim)
        .find_map(|part| part.strip_prefix(&format!("{name}=")))
}
fn cookie_header(name: &str, value: &str, http_only: bool) -> HeaderValue {
    HeaderValue::from_str(&format!(
        "{name}={value}; Path=/; Secure; SameSite=Lax{}",
        if http_only { "; HttpOnly" } else { "" }
    ))
    .expect("generated cookie is valid")
}
fn expired_cookie(name: &str, http_only: bool) -> HeaderValue {
    HeaderValue::from_str(&format!(
        "{name}=; Path=/; Secure; SameSite=Lax; Max-Age=0{}",
        if http_only { "; HttpOnly" } else { "" }
    ))
    .expect("generated cookie is valid")
}
pub(crate) fn same_origin(headers: &HeaderMap, expected: &str) -> bool {
    headers.get(ORIGIN).and_then(|value| value.to_str().ok())
        == Some(expected.trim_end_matches('/'))
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
struct AccessTemplate {
    demo_intent: Option<String>,
}
#[derive(Template)]
#[template(path = "web/docs.html")]
struct DocsTemplate {
    openapi_url: String,
}
#[derive(Template)]
#[template(path = "web/account_entry.html")]
struct AccountEntryTemplate<'a> {
    action: &'a str,
    heading: &'a str,
    csrf: String,
    error: Option<&'a str>,
}
#[derive(Template)]
#[template(path = "web/demo_key.html")]
struct DemoKeyTemplate {
    raw_key: String,
    expires_at: String,
}
#[derive(Template)]
#[template(path = "web/message.html")]
struct MessageTemplate<'a> {
    heading: &'a str,
    message: &'a str,
}

#[cfg(test)]
mod tests {
    use axum::http::{header::CACHE_CONTROL, header::REFERRER_POLICY, HeaderName};

    use super::*;

    #[tokio::test]
    async fn access_responses_with_demo_intents_are_not_cacheable() {
        let secret_response = access_response(Some("demo-intent".to_string()));
        assert_eq!(
            secret_response.headers().get(CACHE_CONTROL).unwrap(),
            "no-store"
        );
        assert_eq!(
            secret_response.headers().get(REFERRER_POLICY).unwrap(),
            "no-referrer"
        );
        assert_eq!(
            secret_response
                .headers()
                .get(HeaderName::from_static("x-robots-tag"))
                .unwrap(),
            "noindex, nofollow"
        );

        let public_response = access_response(None);
        assert!(public_response.headers().get(CACHE_CONTROL).is_none());
        assert_eq!(
            public_response.headers().get(REFERRER_POLICY).unwrap(),
            "strict-origin-when-cross-origin"
        );
        assert!(public_response
            .headers()
            .get(HeaderName::from_static("x-robots-tag"))
            .is_none());
    }
}
