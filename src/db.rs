use sqlx::{postgres::PgPoolOptions, PgPool};

pub fn create_pool(database_url: Option<&str>) -> Result<Option<PgPool>, sqlx::Error> {
    database_url
        .map(|url| PgPoolOptions::new().max_connections(5).connect_lazy(url))
        .transpose()
}
