use crate::cli::DbCommand;
use sqlx::{migrate::Migrator, postgres::PgPoolOptions};

static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LifecycleStep {
    Migrate,
    ApplyReference,
}

#[derive(Debug, Eq, PartialEq, thiserror::Error)]
pub(crate) enum LifecycleError {
    #[error("DATABASE_URL is required for database lifecycle commands")]
    MissingDatabaseUrl,
    #[error("failed to connect to database for lifecycle command: {0}")]
    DatabaseConnection(String),
    #[error("failed to run embedded SQLx migrations: {0}")]
    Migration(String),
    #[error("mother-api db apply-reference is not implemented until SPEC-009 Slice 4")]
    ApplyReferenceNotImplemented,
}

pub(crate) async fn run(command: DbCommand) -> Result<(), LifecycleError> {
    let database_url = database_url_from_env()?;
    match command {
        DbCommand::Migrate => run_migrations(database_url.as_str()).await,
        DbCommand::ApplyReference => apply_reference(),
        DbCommand::Apply => {
            run_migrations(database_url.as_str()).await?;
            apply_reference()
        }
    }
}

fn database_url_from_env() -> Result<String, LifecycleError> {
    database_url_from_value(std::env::var("DATABASE_URL").ok().as_deref())
}

fn database_url_from_value(value: Option<&str>) -> Result<String, LifecycleError> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or(LifecycleError::MissingDatabaseUrl)
}

fn apply_reference() -> Result<(), LifecycleError> {
    Err(LifecycleError::ApplyReferenceNotImplemented)
}

async fn run_migrations(database_url: &str) -> Result<(), LifecycleError> {
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(database_url)
        .await
        .map_err(|error| LifecycleError::DatabaseConnection(error.to_string()))?;

    MIGRATOR
        .run(&pool)
        .await
        .map_err(|error| LifecycleError::Migration(error.to_string()))
}

#[cfg(test)]
fn run_with_executor<F>(
    command: DbCommand,
    database_url: &str,
    mut execute_step: F,
) -> Result<(), LifecycleError>
where
    F: FnMut(LifecycleStep, &str) -> Result<(), LifecycleError>,
{
    match command {
        DbCommand::Migrate => execute_step(LifecycleStep::Migrate, database_url),
        DbCommand::ApplyReference => execute_step(LifecycleStep::ApplyReference, database_url),
        DbCommand::Apply => {
            execute_step(LifecycleStep::Migrate, database_url)?;
            execute_step(LifecycleStep::ApplyReference, database_url)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DATABASE_URL: &str = "postgres://postgres:postgres@localhost:5432/ibdb";

    #[test]
    fn database_url_is_required() {
        assert_eq!(
            database_url_from_value(None).unwrap_err(),
            LifecycleError::MissingDatabaseUrl
        );
        assert_eq!(
            database_url_from_value(Some("   ")).unwrap_err(),
            LifecycleError::MissingDatabaseUrl
        );
    }

    #[test]
    fn database_url_is_trimmed() {
        assert_eq!(
            database_url_from_value(Some("  postgres://db  ")).unwrap(),
            "postgres://db"
        );
    }

    fn record_step(
        calls: &mut Vec<(LifecycleStep, String)>,
        step: LifecycleStep,
        database_url: &str,
        result: Result<(), LifecycleError>,
    ) -> Result<(), LifecycleError> {
        calls.push((step, database_url.to_string()));
        result
    }

    #[test]
    fn populated_database_url_reaches_migrate_step() {
        let mut calls = Vec::new();

        let result = run_with_executor(DbCommand::Migrate, DATABASE_URL, |step, database_url| {
            record_step(&mut calls, step, database_url, Ok(()))
        });

        assert_eq!(result, Ok(()));
        assert_eq!(
            calls,
            vec![(LifecycleStep::Migrate, DATABASE_URL.to_string())]
        );
    }

    #[test]
    fn populated_database_url_reaches_apply_reference_scaffold() {
        let mut calls = Vec::new();

        let result = run_with_executor(
            DbCommand::ApplyReference,
            DATABASE_URL,
            |step, database_url| {
                record_step(
                    &mut calls,
                    step,
                    database_url,
                    Err(LifecycleError::ApplyReferenceNotImplemented),
                )
            },
        );

        assert_eq!(
            result.unwrap_err(),
            LifecycleError::ApplyReferenceNotImplemented
        );
        assert_eq!(
            calls,
            vec![(LifecycleStep::ApplyReference, DATABASE_URL.to_string())]
        );
    }

    #[test]
    fn db_apply_attempts_migrate_before_apply_reference() {
        let mut calls = Vec::new();

        let result = run_with_executor(DbCommand::Apply, DATABASE_URL, |step, database_url| {
            record_step(&mut calls, step, database_url, Ok(()))
        });

        assert_eq!(result, Ok(()));
        assert_eq!(
            calls,
            vec![
                (LifecycleStep::Migrate, DATABASE_URL.to_string()),
                (LifecycleStep::ApplyReference, DATABASE_URL.to_string())
            ]
        );
    }

    #[test]
    fn db_apply_stops_when_migrate_fails() {
        let mut calls = Vec::new();

        let result = run_with_executor(DbCommand::Apply, DATABASE_URL, |step, database_url| {
            record_step(
                &mut calls,
                step,
                database_url,
                Err(LifecycleError::Migration("migration failed".to_string())),
            )
        });

        assert_eq!(
            result.unwrap_err(),
            LifecycleError::Migration("migration failed".to_string())
        );
        assert_eq!(
            calls,
            vec![(LifecycleStep::Migrate, DATABASE_URL.to_string())]
        );
    }

    #[tokio::test]
    async fn embedded_migrator_runs_only_with_explicit_opt_in() {
        if std::env::var("MOTHER_API_RUN_DB_MIGRATION_TESTS").as_deref() != Ok("true") {
            return;
        }

        let Ok(database_url) = std::env::var("DATABASE_URL") else {
            return;
        };

        let database_url = database_url_from_value(Some(&database_url)).unwrap();

        run_migrations(&database_url).await.unwrap();
    }
}
