use sqlx::{postgres::PgPoolOptions, PgPool};

pub(crate) const POSTGRES_TEST_DATABASE_URL_ENV: &str = "MOTHER_API_POSTGRES_TEST_DATABASE_URL";
const POSTGRES_TEST_DATABASE_NAME: &str = "mother_api_postgres_regression_test";

pub(crate) async fn migrated_pool() -> Option<PgPool> {
    let database_url = std::env::var(POSTGRES_TEST_DATABASE_URL_ENV).ok()?;
    let database_url = database_url.trim();
    if database_url.is_empty() {
        return None;
    }

    validate_postgres_test_database_url(database_url)
        .expect("Postgres-backed tests require a disposable local test database URL");

    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(database_url)
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();

    Some(pool)
}

fn validate_postgres_test_database_url(database_url: &str) -> Result<(), String> {
    let database_url = database_url.trim();
    let Some(rest) = database_url
        .strip_prefix("postgres://")
        .or_else(|| database_url.strip_prefix("postgresql://"))
    else {
        return Err(format!(
            "{POSTGRES_TEST_DATABASE_URL_ENV} must use postgres:// or postgresql://"
        ));
    };

    let rest = rest.split_once('?').map_or(rest, |(prefix, _query)| prefix);
    let rest = rest
        .split_once('#')
        .map_or(rest, |(prefix, _fragment)| prefix);
    let Some((authority, database_path)) = rest.split_once('/') else {
        return Err(format!(
            "{POSTGRES_TEST_DATABASE_URL_ENV} must include a database name"
        ));
    };

    if database_path != POSTGRES_TEST_DATABASE_NAME {
        return Err(format!(
            "{POSTGRES_TEST_DATABASE_URL_ENV} must target database {POSTGRES_TEST_DATABASE_NAME:?}"
        ));
    }

    let host_port = authority.rsplit('@').next().unwrap_or(authority);
    let host = host_port.split(':').next().unwrap_or("");

    if !matches!(host, "127.0.0.1" | "localhost") {
        return Err(format!(
            "{POSTGRES_TEST_DATABASE_URL_ENV} must target localhost or 127.0.0.1"
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_local_disposable_postgres_test_database_urls() {
        validate_postgres_test_database_url(
            "postgres://postgres:postgres@127.0.0.1:5432/mother_api_postgres_regression_test",
        )
        .unwrap();
        validate_postgres_test_database_url(
            "postgresql://postgres:postgres@localhost:5432/mother_api_postgres_regression_test",
        )
        .unwrap();
    }

    #[test]
    fn rejects_arbitrary_postgres_database_urls() {
        for database_url in [
            "postgres://postgres:postgres@localhost:5432/ibdb",
            "postgres://postgres:postgres@db.internal:5432/mother_api_postgres_regression_test",
            "postgres://postgres:postgres@127.0.0.1:5432/postgres",
            "https://127.0.0.1:5432/mother_api_postgres_regression_test",
        ] {
            assert!(
                validate_postgres_test_database_url(database_url).is_err(),
                "database URL should be rejected: {database_url}"
            );
        }
    }
}
