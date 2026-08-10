use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use super::errors::RepositoryError;

#[derive(Clone, Debug)]
pub(crate) struct AsyncReportRepository {
    pool: PgPool,
}

#[derive(Clone, Debug)]
pub(crate) struct AsyncReport {
    pub(crate) public_id: String,
    pub(crate) report_type: String,
    pub(crate) report_version: i32,
    pub(crate) input: Value,
    pub(crate) request_digest: String,
    pub(crate) status: String,
    pub(crate) report: Option<Value>,
    pub(crate) report_digest: Option<String>,
    pub(crate) failure_code: Option<String>,
    pub(crate) accepted_at: String,
    pub(crate) started_at: Option<String>,
    pub(crate) completed_at: Option<String>,
    pub(crate) failed_at: Option<String>,
}

#[derive(FromRow)]
struct AsyncReportRow {
    public_id: String, report_type: String, report_version: i32, input: Value,
    request_digest: String, status: String, report: Option<Value>, report_digest: Option<String>,
    failure_code: Option<String>, accepted_at: String, started_at: Option<String>,
    completed_at: Option<String>, failed_at: Option<String>,
}
impl From<AsyncReportRow> for AsyncReport { fn from(row: AsyncReportRow) -> Self { Self { public_id: row.public_id, report_type: row.report_type, report_version: row.report_version, input: row.input, request_digest: row.request_digest, status: row.status, report: row.report, report_digest: row.report_digest, failure_code: row.failure_code, accepted_at: row.accepted_at, started_at: row.started_at, completed_at: row.completed_at, failed_at: row.failed_at } } }

impl AsyncReportRepository {
    pub(crate) fn database(pool: PgPool) -> Self { Self { pool } }
    pub(crate) async fn create_or_find(&self, account_id: Uuid, api_key_id: Uuid, client_id: Option<Uuid>, report_type: &str, report_version: i32, input: Value, idempotency_key_hash: Vec<u8>, request_digest: String) -> Result<(AsyncReport, bool), RepositoryError> {
        let id = Uuid::new_v4(); let public_id = format!("rpt_{}", id.simple());
        let inserted = sqlx::query_as::<_, AsyncReportRow>("insert into mother_api.async_report (id, public_id, ib_account_id, requesting_api_key_id, requesting_client_id, report_type, report_version, input, idempotency_key_hash, request_digest) values ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) on conflict (ib_account_id, idempotency_key_hash) do nothing returning public_id, report_type, report_version, input, request_digest, status, report, report_digest, failure_code, accepted_at::text as accepted_at, started_at::text as started_at, completed_at::text as completed_at, failed_at::text as failed_at")
            .bind(id).bind(&public_id).bind(account_id).bind(api_key_id).bind(client_id).bind(report_type).bind(report_version).bind(&input).bind(&idempotency_key_hash).bind(&request_digest).fetch_optional(&self.pool).await.map_err(RepositoryError::new)?;
        if let Some(row) = inserted { return Ok((row.into(), true)); }
        let row = sqlx::query_as::<_, AsyncReportRow>("select public_id, report_type, report_version, input, request_digest, status, report, report_digest, failure_code, accepted_at::text as accepted_at, started_at::text as started_at, completed_at::text as completed_at, failed_at::text as failed_at from mother_api.async_report where ib_account_id=$1 and idempotency_key_hash=$2")
            .bind(account_id).bind(idempotency_key_hash).fetch_one(&self.pool).await.map_err(RepositoryError::new)?;
        Ok((row.into(), false))
    }
    pub(crate) async fn find_owned(&self, account_id: Uuid, public_id: &str) -> Result<Option<AsyncReport>, RepositoryError> {
        sqlx::query_as::<_, AsyncReportRow>("select public_id, report_type, report_version, input, request_digest, status, report, report_digest, failure_code, accepted_at::text as accepted_at, started_at::text as started_at, completed_at::text as completed_at, failed_at::text as failed_at from mother_api.async_report where ib_account_id=$1 and public_id=$2").bind(account_id).bind(public_id).fetch_optional(&self.pool).await.map_err(RepositoryError::new).map(|row| row.map(Into::into))
    }
    pub(crate) async fn find(&self, public_id: &str) -> Result<Option<AsyncReport>, RepositoryError> {
        sqlx::query_as::<_, AsyncReportRow>("select public_id, report_type, report_version, input, request_digest, status, report, report_digest, failure_code, accepted_at::text as accepted_at, started_at::text as started_at, completed_at::text as completed_at, failed_at::text as failed_at from mother_api.async_report where public_id=$1").bind(public_id).fetch_optional(&self.pool).await.map_err(RepositoryError::new).map(|row| row.map(Into::into))
    }
    pub(crate) async fn mark_running(&self, public_id: &str) -> Result<(), RepositoryError> { sqlx::query("update mother_api.async_report set status='running', started_at=coalesce(started_at,now()) where public_id=$1 and status='accepted'").bind(public_id).execute(&self.pool).await.map_err(RepositoryError::new).map(|_| ()) }
    pub(crate) async fn complete(&self, public_id: &str, report: Value, digest: &str) -> Result<bool, RepositoryError> { sqlx::query("update mother_api.async_report set status='completed', report=$2, report_digest=$3, completed_at=now() where public_id=$1 and status in ('accepted','running')").bind(public_id).bind(report).bind(digest).execute(&self.pool).await.map_err(RepositoryError::new).map(|result| result.rows_affected() == 1) }
    pub(crate) async fn fail(&self, public_id: &str) -> Result<bool, RepositoryError> { sqlx::query("update mother_api.async_report set status='failed', failure_code='execution_failed', failed_at=now() where public_id=$1 and status in ('accepted','running')").bind(public_id).execute(&self.pool).await.map_err(RepositoryError::new).map(|result| result.rows_affected() == 1) }
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String { hex::encode(Sha256::digest(bytes)) }
