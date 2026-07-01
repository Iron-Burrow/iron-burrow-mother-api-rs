use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use super::errors::RepositoryError;

#[derive(Clone, Debug)]
pub(crate) struct ApiKeyRepository {
    pool: PgPool,
}

impl ApiKeyRepository {
    pub(crate) fn database(pool: PgPool) -> Self {
        Self { pool }
    }

    pub(crate) async fn find_key_by_prefix_and_hash(
        &self,
        key_prefix: &str,
        key_hash: &[u8],
    ) -> Result<Option<ApiKeyLookup>, RepositoryError> {
        let row = sqlx::query_as::<_, ApiKeyLookupRow>(
            r#"
            select
                api_key.id as api_key_id,
                api_key.consumer_id,
                api_consumer.slug as consumer_slug,
                api_consumer.category as consumer_category,
                api_consumer.status as consumer_status,
                api_key.key_prefix,
                api_key.label as key_label,
                api_key.status as key_status,
                api_key.hash_algorithm,
                api_key.expires_at::text as expires_at,
                api_key.expires_at is not null
                    and api_key.expires_at <= now() as is_expired
            from mother_api.api_key api_key
            join mother_api.api_consumer api_consumer
                on api_consumer.id = api_key.consumer_id
            where api_key.key_prefix = $1
                and api_key.key_hash = $2
            "#,
        )
        .bind(key_prefix)
        .bind(key_hash)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::new)?;

        Ok(row.map(Into::into))
    }

    pub(crate) async fn create_policy(
        &self,
        api_key_id: Uuid,
        requests_per_minute: i32,
        requests_per_day: i32,
    ) -> Result<ApiKeyPolicy, RepositoryError> {
        let row = sqlx::query_as::<_, ApiKeyPolicy>(
            r#"
            insert into mother_api.api_key_policy (
                api_key_id,
                requests_per_minute,
                requests_per_day
            )
            values ($1, $2, $3)
            returning
                api_key_id,
                requests_per_minute,
                requests_per_day
            "#,
        )
        .bind(api_key_id)
        .bind(requests_per_minute)
        .bind(requests_per_day)
        .fetch_one(&self.pool)
        .await
        .map_err(RepositoryError::new)?;

        Ok(row)
    }

    pub(crate) async fn find_policy(
        &self,
        api_key_id: Uuid,
    ) -> Result<Option<ApiKeyPolicy>, RepositoryError> {
        let row = sqlx::query_as::<_, ApiKeyPolicy>(
            r#"
            select
                api_key_id,
                requests_per_minute,
                requests_per_day
            from mother_api.api_key_policy
            where api_key_id = $1
            "#,
        )
        .bind(api_key_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::new)?;

        Ok(row)
    }

    pub(crate) async fn revoke_by_prefix(
        &self,
        key_prefix: &str,
    ) -> Result<Option<ApiKeyRevocation>, RepositoryError> {
        let row = sqlx::query_as::<_, ApiKeyRevocationRow>(
            r#"
            update mother_api.api_key
            set
                status = 'revoked',
                revoked_at = coalesce(revoked_at, now()),
                updated_at = now()
            where key_prefix = $1
            returning
                id as api_key_id,
                key_prefix,
                status,
                revoked_at::text as revoked_at
            "#,
        )
        .bind(key_prefix)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::new)?;

        Ok(row.map(Into::into))
    }

    pub(crate) async fn update_last_used(&self, api_key_id: Uuid) -> Result<bool, RepositoryError> {
        let updated = sqlx::query_scalar::<_, bool>(
            r#"
            update mother_api.api_key
            set
                last_used_at = now(),
                updated_at = now()
            where id = $1
            returning true
            "#,
        )
        .bind(api_key_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::new)?;

        Ok(updated.unwrap_or(false))
    }

    pub(crate) async fn increment_daily_accepted(
        &self,
        api_key_id: Uuid,
    ) -> Result<DailyAcceptedOutcome, RepositoryError> {
        let outcome = sqlx::query_scalar::<_, String>(
            r#"
            with policy as (
                select requests_per_day
                from mother_api.api_key_policy
                where api_key_id = $1
            ),
            accepted_update as (
                insert into mother_api.api_key_usage_daily (
                    api_key_id,
                    usage_date,
                    accepted_requests,
                    last_used_at
                )
                select
                    $1,
                    (now() at time zone 'utc')::date,
                    1,
                    now()
                from policy
                where requests_per_day > 0
                on conflict (api_key_id, usage_date) do update
                set
                    accepted_requests =
                        mother_api.api_key_usage_daily.accepted_requests + 1,
                    last_used_at = now(),
                    updated_at = now()
                where mother_api.api_key_usage_daily.accepted_requests
                    < (select requests_per_day from policy)
                returning accepted_requests
            )
            select case
                when not exists (select 1 from policy) then 'missing_policy'
                when exists (select 1 from accepted_update) then 'accepted'
                else 'limit_exceeded'
            end
            "#,
        )
        .bind(api_key_id)
        .fetch_one(&self.pool)
        .await
        .map_err(RepositoryError::new)?;

        Ok(DailyAcceptedOutcome::from_database_value(&outcome))
    }

    pub(crate) async fn increment_daily_rate_limited(
        &self,
        api_key_id: Uuid,
    ) -> Result<(), RepositoryError> {
        sqlx::query(
            r#"
            insert into mother_api.api_key_usage_daily (
                api_key_id,
                usage_date,
                rate_limited_requests
            )
            values ($1, (now() at time zone 'utc')::date, 1)
            on conflict (api_key_id, usage_date) do update
            set
                rate_limited_requests =
                    mother_api.api_key_usage_daily.rate_limited_requests + 1,
                updated_at = now()
            "#,
        )
        .bind(api_key_id)
        .execute(&self.pool)
        .await
        .map_err(RepositoryError::new)?;

        Ok(())
    }

    pub(crate) async fn increment_daily_response(
        &self,
        api_key_id: Uuid,
        response_class: UsageResponseClass,
    ) -> Result<(), RepositoryError> {
        match response_class {
            UsageResponseClass::Successful => {
                increment_response_counter(&self.pool, api_key_id, ResponseCounter::Successful)
                    .await
            }
            UsageResponseClass::ClientError => {
                increment_response_counter(&self.pool, api_key_id, ResponseCounter::ClientError)
                    .await
            }
            UsageResponseClass::ServerError => {
                increment_response_counter(&self.pool, api_key_id, ResponseCounter::ServerError)
                    .await
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ApiKeyLookup {
    pub(crate) api_key_id: Uuid,
    pub(crate) consumer_id: Uuid,
    pub(crate) consumer_slug: String,
    pub(crate) consumer_category: String,
    pub(crate) consumer_status: String,
    pub(crate) key_prefix: String,
    pub(crate) key_label: String,
    pub(crate) key_status: String,
    pub(crate) hash_algorithm: String,
    pub(crate) expires_at: Option<String>,
    pub(crate) is_expired: bool,
}

#[derive(FromRow)]
struct ApiKeyLookupRow {
    api_key_id: Uuid,
    consumer_id: Uuid,
    consumer_slug: String,
    consumer_category: String,
    consumer_status: String,
    key_prefix: String,
    key_label: String,
    key_status: String,
    hash_algorithm: String,
    expires_at: Option<String>,
    is_expired: bool,
}

impl From<ApiKeyLookupRow> for ApiKeyLookup {
    fn from(row: ApiKeyLookupRow) -> Self {
        Self {
            api_key_id: row.api_key_id,
            consumer_id: row.consumer_id,
            consumer_slug: row.consumer_slug,
            consumer_category: row.consumer_category,
            consumer_status: row.consumer_status,
            key_prefix: row.key_prefix,
            key_label: row.key_label,
            key_status: row.key_status,
            hash_algorithm: row.hash_algorithm,
            expires_at: row.expires_at,
            is_expired: row.is_expired,
        }
    }
}

#[derive(Clone, Debug, Eq, FromRow, PartialEq)]
pub(crate) struct ApiKeyPolicy {
    pub(crate) api_key_id: Uuid,
    pub(crate) requests_per_minute: i32,
    pub(crate) requests_per_day: i32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ApiKeyRevocation {
    pub(crate) api_key_id: Uuid,
    pub(crate) key_prefix: String,
    pub(crate) status: String,
    pub(crate) revoked_at: String,
}

#[derive(FromRow)]
struct ApiKeyRevocationRow {
    api_key_id: Uuid,
    key_prefix: String,
    status: String,
    revoked_at: String,
}

impl From<ApiKeyRevocationRow> for ApiKeyRevocation {
    fn from(row: ApiKeyRevocationRow) -> Self {
        Self {
            api_key_id: row.api_key_id,
            key_prefix: row.key_prefix,
            status: row.status,
            revoked_at: row.revoked_at,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DailyAcceptedOutcome {
    Accepted,
    LimitExceeded,
    MissingPolicy,
}

impl DailyAcceptedOutcome {
    fn from_database_value(value: &str) -> Self {
        match value {
            "accepted" => Self::Accepted,
            "limit_exceeded" => Self::LimitExceeded,
            "missing_policy" => Self::MissingPolicy,
            unexpected => panic!("unexpected daily accepted outcome {unexpected:?}"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum UsageResponseClass {
    Successful,
    ClientError,
    ServerError,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ResponseCounter {
    Successful,
    ClientError,
    ServerError,
}

async fn increment_response_counter(
    pool: &PgPool,
    api_key_id: Uuid,
    counter: ResponseCounter,
) -> Result<(), RepositoryError> {
    match counter {
        ResponseCounter::Successful => {
            sqlx::query(
                r#"
                insert into mother_api.api_key_usage_daily (
                    api_key_id,
                    usage_date,
                    successful_responses
                )
                values ($1, (now() at time zone 'utc')::date, 1)
                on conflict (api_key_id, usage_date) do update
                set
                    successful_responses =
                        mother_api.api_key_usage_daily.successful_responses + 1,
                    updated_at = now()
                "#,
            )
            .bind(api_key_id)
            .execute(pool)
            .await
            .map_err(RepositoryError::new)?;
        }
        ResponseCounter::ClientError => {
            sqlx::query(
                r#"
                insert into mother_api.api_key_usage_daily (
                    api_key_id,
                    usage_date,
                    client_error_responses
                )
                values ($1, (now() at time zone 'utc')::date, 1)
                on conflict (api_key_id, usage_date) do update
                set
                    client_error_responses =
                        mother_api.api_key_usage_daily.client_error_responses + 1,
                    updated_at = now()
                "#,
            )
            .bind(api_key_id)
            .execute(pool)
            .await
            .map_err(RepositoryError::new)?;
        }
        ResponseCounter::ServerError => {
            sqlx::query(
                r#"
                insert into mother_api.api_key_usage_daily (
                    api_key_id,
                    usage_date,
                    server_error_responses
                )
                values ($1, (now() at time zone 'utc')::date, 1)
                on conflict (api_key_id, usage_date) do update
                set
                    server_error_responses =
                        mother_api.api_key_usage_daily.server_error_responses + 1,
                    updated_at = now()
                "#,
            )
            .bind(api_key_id)
            .execute(pool)
            .await
            .map_err(RepositoryError::new)?;
        }
    }

    Ok(())
}
