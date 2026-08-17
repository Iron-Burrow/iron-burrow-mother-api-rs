use axum::{
    body::Bytes,
    extract::{Extension, Path, State},
    http::{header::HeaderName, HeaderMap, StatusCode},
    Json,
};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use crate::{
    adapters::{
        http::{
            auth::ApiKeyPrincipal,
            error::ApiError,
            json_body::parse_json_object_body,
            validation::{ensure_json_content_type, reject_unknown_fields},
        },
        postgres::{
            async_reports::{sha256_hex, AsyncReport, CreateAsyncReport},
            AsyncReportRepository,
        },
    },
    application::async_reports::{lookup, validate_input, validate_report},
    state::AppState,
};

const MAX_REPORT_BYTES: usize = 1024 * 1024;
const IDEMPOTENCY_KEY: HeaderName = HeaderName::from_static("idempotency-key");

pub async fn create_report(
    State(state): State<AppState>,
    Extension(principal): Extension<ApiKeyPrincipal>,
    Path(report_type): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let account_id = account_principal(&principal)?;
    ensure_json_content_type(&headers)?;
    let idempotency_key = idempotency_key(&headers)?;
    let request = parse_json_object_body(&body)?;
    reject_unknown_fields(&request, &["report_version", "input"])?;
    let report_version = request
        .get("report_version")
        .and_then(Value::as_i64)
        .and_then(|value| i32::try_from(value).ok())
        .filter(|value| *value > 0)
        .ok_or_else(ApiError::invalid_request)?;
    let input = request
        .get("input")
        .cloned()
        .ok_or_else(ApiError::invalid_request)?;
    let definition = lookup(&report_type, report_version)
        .filter(|definition| validate_input(*definition, &input))
        .ok_or_else(ApiError::unsupported_report_type)?;
    let repository = repository(&state)?;
    let canonical_request = canonical_json(
        &json!({"report_type": definition.report_type, "report_version": report_version, "input": input}),
    );
    let request_digest = sha256_hex(canonical_request.as_bytes());
    let key_hash = Sha256::digest(idempotency_key.as_bytes()).to_vec();
    let (report, created) = repository
        .create_or_find(CreateAsyncReport {
            account_id,
            api_key_id: principal.api_key_id,
            client_id: principal.client_id,
            report_type: definition.report_type.to_string(),
            report_version,
            input,
            idempotency_key_hash: key_hash,
            request_digest: request_digest.clone(),
        })
        .await
        .map_err(|_| ApiError::database_unavailable_for_auth())?;
    if !created && report.request_digest != request_digest {
        return Err(ApiError::idempotency_conflict());
    }
    if created || report.status == "accepted" {
        let Some(bigwig) = state.bigwig_client.as_ref() else {
            return Err(ApiError::report_execution_unavailable());
        };
        bigwig
            .execute_async_report(
                &report.public_id,
                &report.report_type,
                report.report_version,
                &report.input,
                state.config.bigwig_report_start_timeout_ms,
            )
            .await
            .map_err(|_| ApiError::report_execution_unavailable())?;
        repository
            .mark_running(&report.public_id)
            .await
            .map_err(|_| ApiError::database_unavailable_for_auth())?;
    }
    Ok((
        StatusCode::ACCEPTED,
        Json(json!({"ok":true,"report_id":report.public_id,"status":"accepted"})),
    ))
}

pub async fn get_report(
    State(state): State<AppState>,
    Extension(principal): Extension<ApiKeyPrincipal>,
    Path(report_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let account_id = account_principal(&principal)?;
    let report = repository(&state)?
        .find_owned(account_id, &report_id)
        .await
        .map_err(|_| ApiError::database_unavailable_for_auth())?
        .ok_or_else(ApiError::report_not_found)?;
    Ok(Json(report_response(report)))
}

pub async fn complete_report(
    State(state): State<AppState>,
    Path(report_id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    ensure_json_content_type(&headers)?;
    if body.len() > MAX_REPORT_BYTES {
        return Err(ApiError::report_too_large());
    }
    let payload = parse_json_object_body(&body)?;
    reject_unknown_fields(&payload, &["report_type", "report_version", "report"])?;
    let report = payload
        .get("report")
        .cloned()
        .ok_or_else(ApiError::invalid_request)?;
    let existing = repository(&state)?
        .find(&report_id)
        .await
        .map_err(|_| ApiError::database_unavailable_for_auth())?
        .ok_or_else(ApiError::report_not_found)?;
    terminal_identity_matches(&existing, &payload)?;
    let definition = lookup(&existing.report_type, existing.report_version)
        .filter(|definition| validate_report(*definition, &report))
        .ok_or_else(ApiError::unsupported_report_type)?;
    let _ = definition;
    let digest = sha256_hex(canonical_json(&report).as_bytes());
    let repository = repository(&state)?;
    if repository
        .complete(&report_id, report, &digest)
        .await
        .map_err(|_| ApiError::database_unavailable_for_auth())?
    {
        return Ok((
            StatusCode::OK,
            Json(json!({"ok":true,"report_id":report_id,"status":"completed"})),
        ));
    }
    let current = repository
        .find(&report_id)
        .await
        .map_err(|_| ApiError::database_unavailable_for_auth())?
        .ok_or_else(ApiError::report_not_found)?;
    if current.status == "completed" && current.report_digest.as_deref() == Some(&digest) {
        return Ok((
            StatusCode::OK,
            Json(json!({"ok":true,"report_id":report_id,"status":"completed"})),
        ));
    }
    Err(ApiError::idempotency_conflict())
}

pub async fn fail_report(
    State(state): State<AppState>,
    Path(report_id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    ensure_json_content_type(&headers)?;
    let payload = parse_json_object_body(&body)?;
    reject_unknown_fields(&payload, &["report_type", "report_version", "failure_code"])?;
    if payload.get("failure_code").and_then(Value::as_str) != Some("execution_failed") {
        return Err(ApiError::invalid_request());
    }
    let repository = repository(&state)?;
    let existing = repository
        .find(&report_id)
        .await
        .map_err(|_| ApiError::database_unavailable_for_auth())?
        .ok_or_else(ApiError::report_not_found)?;
    terminal_identity_matches(&existing, &payload)?;
    if repository
        .fail(&report_id)
        .await
        .map_err(|_| ApiError::database_unavailable_for_auth())?
        || existing.status == "failed"
    {
        return Ok((
            StatusCode::OK,
            Json(json!({"ok":true,"report_id":report_id,"status":"failed"})),
        ));
    }
    Err(ApiError::idempotency_conflict())
}

fn repository(state: &AppState) -> Result<AsyncReportRepository, ApiError> {
    state
        .database_pool
        .clone()
        .map(AsyncReportRepository::database)
        .ok_or_else(ApiError::database_unavailable_for_auth)
}
fn account_principal(principal: &ApiKeyPrincipal) -> Result<uuid::Uuid, ApiError> {
    if !matches!(principal.key_kind.as_str(), "account" | "agent") {
        return Err(ApiError::capability_not_granted());
    }
    principal
        .ib_account_id
        .ok_or_else(ApiError::capability_not_granted)
}
fn idempotency_key(headers: &HeaderMap) -> Result<&str, ApiError> {
    let value = headers
        .get(&IDEMPOTENCY_KEY)
        .ok_or_else(ApiError::idempotency_key_required)?
        .to_str()
        .map_err(|_| ApiError::invalid_idempotency_key())?;
    if value.is_empty() || value.len() > 255 {
        Err(ApiError::invalid_idempotency_key())
    } else {
        Ok(value)
    }
}
fn terminal_identity_matches(
    existing: &AsyncReport,
    payload: &Map<String, Value>,
) -> Result<(), ApiError> {
    if payload.get("report_type").and_then(Value::as_str) == Some(existing.report_type.as_str())
        && payload.get("report_version").and_then(Value::as_i64)
            == Some(existing.report_version as i64)
    {
        Ok(())
    } else {
        Err(ApiError::invalid_request())
    }
}
fn report_response(report: AsyncReport) -> Value {
    let mut response = json!({"ok":true,"type":"async_report","report_id":report.public_id,"report_type":report.report_type,"report_version":report.report_version,"status":report.status,"accepted_at":report.accepted_at});
    if let Some(started_at) = report.started_at {
        response["started_at"] = json!(started_at);
    }
    if let Some(completed_at) = report.completed_at {
        response["completed_at"] = json!(completed_at);
        response["report"] = report.report.unwrap_or(Value::Null);
    }
    if let Some(failed_at) = report.failed_at {
        response["failed_at"] = json!(failed_at);
        response["failure"] =
            json!({"code":report.failure_code.unwrap_or_else(|| "execution_failed".to_string())});
    }
    response
}
fn canonical_json(value: &Value) -> String {
    match value {
        Value::Object(object) => {
            let mut entries = object.iter().collect::<Vec<_>>();
            entries.sort_by_key(|(key, _)| *key);
            format!(
                "{{{}}}",
                entries
                    .into_iter()
                    .map(|(key, value)| format!(
                        "{}:{}",
                        serde_json::to_string(key).expect("key"),
                        canonical_json(value)
                    ))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        }
        Value::Array(values) => format!(
            "[{}]",
            values
                .iter()
                .map(canonical_json)
                .collect::<Vec<_>>()
                .join(",")
        ),
        _ => serde_json::to_string(value).expect("JSON serializes"),
    }
}
