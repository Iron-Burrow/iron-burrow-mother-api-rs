use crate::cli::DbCommand;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LifecycleStep {
    Migrate,
    ApplyReference,
}

#[derive(Debug, Eq, PartialEq, thiserror::Error)]
pub(crate) enum LifecycleError {
    #[error("DATABASE_URL is required for database lifecycle commands")]
    MissingDatabaseUrl,
    #[error("mother-api db migrate is not implemented until SPEC-009 Slice 2")]
    MigrateNotImplemented,
    #[error("mother-api db apply-reference is not implemented until SPEC-009 Slice 4")]
    ApplyReferenceNotImplemented,
}

pub(crate) fn run(command: DbCommand) -> Result<(), LifecycleError> {
    let database_url = database_url_from_env()?;
    run_with_executor(command, database_url.as_str(), scaffold_step)
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

fn scaffold_step(step: LifecycleStep, _database_url: &str) -> Result<(), LifecycleError> {
    match step {
        LifecycleStep::Migrate => Err(LifecycleError::MigrateNotImplemented),
        LifecycleStep::ApplyReference => Err(LifecycleError::ApplyReferenceNotImplemented),
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

    #[test]
    fn populated_database_url_reaches_migrate_scaffold() {
        let mut calls = Vec::new();

        let result = run_with_executor(DbCommand::Migrate, DATABASE_URL, |step, database_url| {
            calls.push((step, database_url.to_string()));
            Err(LifecycleError::MigrateNotImplemented)
        });

        assert_eq!(result.unwrap_err(), LifecycleError::MigrateNotImplemented);
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
                calls.push((step, database_url.to_string()));
                Err(LifecycleError::ApplyReferenceNotImplemented)
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
            calls.push((step, database_url.to_string()));
            Ok(())
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
    fn db_apply_stops_when_migrate_scaffold_fails() {
        let mut calls = Vec::new();

        let result = run_with_executor(DbCommand::Apply, DATABASE_URL, |step, database_url| {
            calls.push((step, database_url.to_string()));
            Err(LifecycleError::MigrateNotImplemented)
        });

        assert_eq!(result.unwrap_err(), LifecycleError::MigrateNotImplemented);
        assert_eq!(
            calls,
            vec![(LifecycleStep::Migrate, DATABASE_URL.to_string())]
        );
    }
}
