use serde_json::Value;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use super::errors::RepositoryError;

#[derive(Clone, Debug)]
pub(crate) struct PortfolioSimulationRepository {
    pool: PgPool,
}

#[derive(Clone, Debug)]
pub(crate) struct PortfolioSimulationRun {
    pub(crate) public_id: String,
    pub(crate) outcome: String,
    pub(crate) request_schema_version: i32,
    pub(crate) strategy_slug: String,
    pub(crate) strategy_version: String,
    pub(crate) engine_version: String,
    pub(crate) evidence_digest: String,
    pub(crate) input: Value,
    pub(crate) evidence: Value,
    pub(crate) result: Value,
    pub(crate) created_at: String,
}

#[derive(Clone, Debug)]
pub(crate) struct CreatePortfolioSimulationRun {
    pub(crate) account_id: Uuid,
    pub(crate) outcome: String,
    pub(crate) strategy_slug: String,
    pub(crate) strategy_version: String,
    pub(crate) engine_version: String,
    pub(crate) evidence_digest: String,
    pub(crate) input: Value,
    pub(crate) evidence: Value,
    pub(crate) result: Value,
}

#[derive(FromRow)]
struct RunRow {
    public_id: String,
    outcome: String,
    request_schema_version: i32,
    strategy_slug: String,
    strategy_version: String,
    engine_version: String,
    evidence_digest: String,
    input: Value,
    evidence: Value,
    result: Value,
    created_at: String,
}

impl From<RunRow> for PortfolioSimulationRun {
    fn from(row: RunRow) -> Self {
        Self {
            public_id: row.public_id,
            outcome: row.outcome,
            request_schema_version: row.request_schema_version,
            strategy_slug: row.strategy_slug,
            strategy_version: row.strategy_version,
            engine_version: row.engine_version,
            evidence_digest: row.evidence_digest,
            input: row.input,
            evidence: row.evidence,
            result: row.result,
            created_at: row.created_at,
        }
    }
}

impl PortfolioSimulationRepository {
    pub(crate) fn database(pool: PgPool) -> Self {
        Self { pool }
    }

    pub(crate) async fn create(
        &self,
        request: CreatePortfolioSimulationRun,
    ) -> Result<PortfolioSimulationRun, RepositoryError> {
        let id = Uuid::new_v4();
        let public_id = format!("psr_{}", id.simple());
        sqlx::query_as::<_, RunRow>(
            r#"insert into mother_api.portfolio_simulation_run
              (id, public_id, ib_account_id, outcome, request_schema_version, strategy_slug,
               strategy_version, engine_version, evidence_digest, input, evidence, result)
              values ($1, $2, $3, $4, 1, $5, $6, $7, $8, $9, $10, $11)
              returning public_id, outcome, request_schema_version, strategy_slug, strategy_version,
                engine_version, evidence_digest, input, evidence, result,
                to_char(created_at at time zone 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"') as created_at"#,
        )
        .bind(id)
        .bind(public_id)
        .bind(request.account_id)
        .bind(request.outcome)
        .bind(request.strategy_slug)
        .bind(request.strategy_version)
        .bind(request.engine_version)
        .bind(request.evidence_digest)
        .bind(request.input)
        .bind(request.evidence)
        .bind(request.result)
        .fetch_one(&self.pool)
        .await
        .map_err(RepositoryError::new)
        .map(Into::into)
    }

    pub(crate) async fn find_owned(
        &self,
        account_id: Uuid,
        public_id: &str,
    ) -> Result<Option<PortfolioSimulationRun>, RepositoryError> {
        sqlx::query_as::<_, RunRow>(
            r#"select public_id, outcome, request_schema_version, strategy_slug, strategy_version,
                 engine_version, evidence_digest, input, evidence, result,
                 to_char(created_at at time zone 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"') as created_at
               from mother_api.portfolio_simulation_run
               where ib_account_id = $1 and public_id = $2"#,
        )
        .bind(account_id)
        .bind(public_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::new)
        .map(|row| row.map(Into::into))
    }
}
