use sqlx::{postgres::PgPoolOptions, PgPool};

pub fn create_pool(database_url: Option<&str>) -> Result<Option<PgPool>, sqlx::Error> {
    database_url
        .map(|url| PgPoolOptions::new().max_connections(5).connect_lazy(url))
        .transpose()
}

pub async fn health_status(pool: Option<&PgPool>) -> &'static str {
    let Some(pool) = pool else {
        return "skipped";
    };

    match sqlx::query("select 1").execute(pool).await {
        Ok(_) => "reachable",
        Err(_) => "unreachable",
    }
}
