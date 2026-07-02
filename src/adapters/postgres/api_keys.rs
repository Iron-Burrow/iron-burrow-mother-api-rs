#[cfg(test)]
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use serde::Serialize;
use sqlx::{FromRow, PgPool, Postgres, Transaction};
use uuid::Uuid;

use super::errors::RepositoryError;

#[derive(Clone, Debug)]
pub(crate) enum ApiKeyRepository {
    Database(PgPool),
    #[cfg(test)]
    InMemory(Arc<Mutex<InMemoryApiKeys>>),
    #[cfg(test)]
    Unavailable,
}

#[cfg(test)]
#[derive(Clone, Debug)]
pub(crate) struct InMemoryApiKeys {
    keys: Vec<InMemoryApiKey>,
    policies: HashMap<Uuid, ApiKeyPolicy>,
    usage: HashMap<Uuid, InMemoryApiKeyUsage>,
}

#[cfg(test)]
#[derive(Clone, Debug)]
struct InMemoryApiKey {
    key_prefix: String,
    key_hash: Vec<u8>,
    lookup: ApiKeyLookup,
}

#[cfg(test)]
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct InMemoryApiKeyUsageSnapshot {
    pub(crate) accepted_requests: i64,
    pub(crate) rate_limited_requests: i64,
    pub(crate) successful_responses: i64,
    pub(crate) client_error_responses: i64,
    pub(crate) server_error_responses: i64,
    pub(crate) api_key_last_used_updated: bool,
    pub(crate) daily_last_used_updated: bool,
}

#[cfg(test)]
#[derive(Clone, Debug, Default)]
struct InMemoryApiKeyUsage {
    accepted_requests: i64,
    rate_limited_requests: i64,
    successful_responses: i64,
    client_error_responses: i64,
    server_error_responses: i64,
    api_key_last_used_updated: bool,
    daily_last_used_updated: bool,
}

#[cfg(test)]
impl From<&InMemoryApiKeyUsage> for InMemoryApiKeyUsageSnapshot {
    fn from(usage: &InMemoryApiKeyUsage) -> Self {
        Self {
            accepted_requests: usage.accepted_requests,
            rate_limited_requests: usage.rate_limited_requests,
            successful_responses: usage.successful_responses,
            client_error_responses: usage.client_error_responses,
            server_error_responses: usage.server_error_responses,
            api_key_last_used_updated: usage.api_key_last_used_updated,
            daily_last_used_updated: usage.daily_last_used_updated,
        }
    }
}

impl ApiKeyRepository {
    pub(crate) fn database(pool: PgPool) -> Self {
        Self::Database(pool)
    }

    #[cfg(test)]
    pub(crate) fn in_memory(keys: Vec<(String, Vec<u8>, ApiKeyLookup)>) -> Self {
        Self::in_memory_with_policies(keys, HashMap::new())
    }

    #[cfg(test)]
    pub(crate) fn in_memory_with_policies(
        keys: Vec<(String, Vec<u8>, ApiKeyLookup)>,
        policies: HashMap<Uuid, ApiKeyPolicy>,
    ) -> Self {
        let mut defaulted_policies = policies;
        let keys = keys
            .into_iter()
            .map(|(key_prefix, key_hash, lookup)| {
                defaulted_policies
                    .entry(lookup.api_key_id)
                    .or_insert(ApiKeyPolicy {
                        api_key_id: lookup.api_key_id,
                        requests_per_minute: 60,
                        requests_per_day: 5000,
                    });

                InMemoryApiKey {
                    key_prefix,
                    key_hash,
                    lookup,
                }
            })
            .collect();

        Self::InMemory(Arc::new(Mutex::new(InMemoryApiKeys {
            keys,
            policies: defaulted_policies,
            usage: HashMap::new(),
        })))
    }

    #[cfg(test)]
    pub(crate) fn in_memory_usage(&self, api_key_id: Uuid) -> InMemoryApiKeyUsageSnapshot {
        let Self::InMemory(keys) = self else {
            return InMemoryApiKeyUsageSnapshot::default();
        };
        keys.lock()
            .expect("in-memory API-key repository mutex poisoned")
            .usage
            .get(&api_key_id)
            .map(InMemoryApiKeyUsageSnapshot::from)
            .unwrap_or_default()
    }

    #[cfg(test)]
    pub(crate) fn unavailable() -> Self {
        Self::Unavailable
    }

    fn database_pool(&self) -> Result<&PgPool, RepositoryError> {
        match self {
            Self::Database(pool) => Ok(pool),
            #[cfg(test)]
            Self::InMemory(_) | Self::Unavailable => Err(RepositoryError::test()),
        }
    }

    pub(crate) async fn find_key_by_prefix_and_hash(
        &self,
        key_prefix: &str,
        key_hash: &[u8],
    ) -> Result<Option<ApiKeyLookup>, RepositoryError> {
        #[cfg(test)]
        if let Self::InMemory(keys) = self {
            let keys = keys
                .lock()
                .expect("in-memory API-key repository mutex poisoned");
            return Ok(keys
                .keys
                .iter()
                .find(|key| key.key_prefix == key_prefix && key.key_hash == key_hash)
                .map(|key| key.lookup.clone()));
        }

        #[cfg(test)]
        if matches!(self, Self::Unavailable) {
            return Err(RepositoryError::test());
        }

        let pool = self.database_pool()?;
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
        .fetch_optional(pool)
        .await
        .map_err(RepositoryError::new)?;

        Ok(row.map(Into::into))
    }

    pub(crate) async fn issue_api_key(
        &self,
        input: IssueApiKeyInput,
    ) -> Result<IssuedApiKey, ApiKeyIssueRepositoryError> {
        let pool = self
            .database_pool()
            .map_err(ApiKeyIssueRepositoryError::Repository)?;
        let mut transaction = pool
            .begin()
            .await
            .map_err(|error| ApiKeyIssueRepositoryError::Repository(RepositoryError::new(error)))?;

        let consumer = upsert_consumer_for_issue(
            &mut transaction,
            &input.consumer_slug,
            &input.display_name,
            &input.category,
        )
        .await?;

        let api_key = insert_issued_key(&mut transaction, &consumer, &input).await?;
        insert_policy_for_issue(
            &mut transaction,
            api_key.api_key_id,
            input.requests_per_minute,
            input.requests_per_day,
        )
        .await?;

        transaction
            .commit()
            .await
            .map_err(|error| ApiKeyIssueRepositoryError::Repository(RepositoryError::new(error)))?;

        Ok(api_key)
    }

    #[cfg(test)]
    pub(crate) async fn create_policy(
        &self,
        api_key_id: Uuid,
        requests_per_minute: i32,
        requests_per_day: i32,
    ) -> Result<ApiKeyPolicy, RepositoryError> {
        #[cfg(test)]
        if let Self::InMemory(keys) = self {
            let policy = ApiKeyPolicy {
                api_key_id,
                requests_per_minute,
                requests_per_day,
            };
            keys.lock()
                .expect("in-memory API-key repository mutex poisoned")
                .policies
                .insert(api_key_id, policy.clone());
            return Ok(policy);
        }

        #[cfg(test)]
        if matches!(self, Self::Unavailable) {
            return Err(RepositoryError::test());
        }

        let pool = self.database_pool()?;
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
        .fetch_one(pool)
        .await
        .map_err(RepositoryError::new)?;

        Ok(row)
    }

    pub(crate) async fn find_policy(
        &self,
        api_key_id: Uuid,
    ) -> Result<Option<ApiKeyPolicy>, RepositoryError> {
        #[cfg(test)]
        if let Self::InMemory(keys) = self {
            return Ok(keys
                .lock()
                .expect("in-memory API-key repository mutex poisoned")
                .policies
                .get(&api_key_id)
                .cloned());
        }

        #[cfg(test)]
        if matches!(self, Self::Unavailable) {
            return Err(RepositoryError::test());
        }

        let pool = self.database_pool()?;
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
        .fetch_optional(pool)
        .await
        .map_err(RepositoryError::new)?;

        Ok(row)
    }

    pub(crate) async fn revoke_by_prefix(
        &self,
        key_prefix: &str,
    ) -> Result<Option<ApiKeyRevocation>, RepositoryError> {
        let pool = self.database_pool()?;
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
        .fetch_optional(pool)
        .await
        .map_err(RepositoryError::new)?;

        Ok(row.map(Into::into))
    }

    pub(crate) async fn list_for_consumer(
        &self,
        consumer_slug: &str,
    ) -> Result<Vec<ApiKeyListItem>, RepositoryError> {
        let pool = self.database_pool()?;
        let rows = sqlx::query_as::<_, ApiKeyListItem>(
            r#"
            select
                api_key.key_prefix,
                api_key.label,
                api_key.status,
                api_key.expires_at::text as expires_at,
                api_key.created_at::text as created_at,
                api_key.last_used_at::text as last_used_at
            from mother_api.api_key api_key
            join mother_api.api_consumer api_consumer
                on api_consumer.id = api_key.consumer_id
            where api_consumer.slug = $1
            order by api_key.created_at desc, api_key.key_prefix asc
            "#,
        )
        .bind(consumer_slug)
        .fetch_all(pool)
        .await
        .map_err(RepositoryError::new)?;

        Ok(rows)
    }

    pub(crate) async fn usage_for_consumer(
        &self,
        consumer_slug: &str,
        days: u32,
    ) -> Result<Vec<ApiKeyUsageItem>, RepositoryError> {
        let pool = self.database_pool()?;
        let rows = sqlx::query_as::<_, ApiKeyUsageItem>(
            r#"
            select
                usage.usage_date::text as usage_date,
                api_key.key_prefix,
                usage.accepted_requests,
                usage.rate_limited_requests,
                usage.successful_responses,
                usage.client_error_responses,
                usage.server_error_responses,
                usage.last_used_at::text as last_used_at
            from mother_api.api_key_usage_daily usage
            join mother_api.api_key api_key
                on api_key.id = usage.api_key_id
            join mother_api.api_consumer api_consumer
                on api_consumer.id = api_key.consumer_id
            where api_consumer.slug = $1
                and usage.usage_date >= (now() at time zone 'utc')::date - ($2::integer - 1)
            order by usage.usage_date desc, api_key.key_prefix asc
            "#,
        )
        .bind(consumer_slug)
        .bind(i32::try_from(days).unwrap_or(i32::MAX))
        .fetch_all(pool)
        .await
        .map_err(RepositoryError::new)?;

        Ok(rows)
    }

    #[cfg(test)]
    pub(crate) async fn update_last_used(&self, api_key_id: Uuid) -> Result<bool, RepositoryError> {
        #[cfg(test)]
        if let Self::InMemory(keys) = self {
            let mut keys = keys
                .lock()
                .expect("in-memory API-key repository mutex poisoned");
            let exists = keys
                .keys
                .iter()
                .any(|key| key.lookup.api_key_id == api_key_id);
            if exists {
                keys.usage
                    .entry(api_key_id)
                    .or_default()
                    .api_key_last_used_updated = true;
            }
            return Ok(exists);
        }

        #[cfg(test)]
        if matches!(self, Self::Unavailable) {
            return Err(RepositoryError::test());
        }

        let pool = self.database_pool()?;
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
        .fetch_optional(pool)
        .await
        .map_err(RepositoryError::new)?;

        Ok(updated.unwrap_or(false))
    }

    pub(crate) async fn increment_daily_accepted(
        &self,
        api_key_id: Uuid,
    ) -> Result<DailyAcceptedOutcome, RepositoryError> {
        #[cfg(test)]
        if let Self::InMemory(keys) = self {
            let mut keys = keys
                .lock()
                .expect("in-memory API-key repository mutex poisoned");
            let Some(policy) = keys.policies.get(&api_key_id) else {
                return Ok(DailyAcceptedOutcome::MissingPolicy);
            };
            let limit = policy.requests_per_day;
            let usage = keys.usage.entry(api_key_id).or_default();
            if limit <= 0 || usage.accepted_requests >= i64::from(limit) {
                return Ok(DailyAcceptedOutcome::LimitExceeded);
            }

            usage.accepted_requests += 1;
            usage.api_key_last_used_updated = true;
            usage.daily_last_used_updated = true;
            return Ok(DailyAcceptedOutcome::Accepted);
        }

        #[cfg(test)]
        if matches!(self, Self::Unavailable) {
            return Err(RepositoryError::test());
        }

        let pool = self.database_pool()?;
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
            ),
            key_update as (
                update mother_api.api_key
                set
                    last_used_at = now(),
                    updated_at = now()
                where id = $1
                    and exists (select 1 from accepted_update)
                returning true
            )
            select case
                when not exists (select 1 from policy) then 'missing_policy'
                when exists (select 1 from accepted_update)
                    and exists (select 1 from key_update) then 'accepted'
                else 'limit_exceeded'
            end
            "#,
        )
        .bind(api_key_id)
        .fetch_one(pool)
        .await
        .map_err(RepositoryError::new)?;

        DailyAcceptedOutcome::from_database_value(&outcome)
    }

    pub(crate) async fn increment_daily_rate_limited(
        &self,
        api_key_id: Uuid,
    ) -> Result<(), RepositoryError> {
        #[cfg(test)]
        if let Self::InMemory(keys) = self {
            let mut keys = keys
                .lock()
                .expect("in-memory API-key repository mutex poisoned");
            let usage = keys.usage.entry(api_key_id).or_default();
            usage.rate_limited_requests += 1;
            usage.daily_last_used_updated = true;
            return Ok(());
        }

        #[cfg(test)]
        if matches!(self, Self::Unavailable) {
            return Err(RepositoryError::test());
        }

        let pool = self.database_pool()?;
        sqlx::query(
            r#"
            insert into mother_api.api_key_usage_daily (
                api_key_id,
                usage_date,
                rate_limited_requests,
                last_used_at
            )
            values ($1, (now() at time zone 'utc')::date, 1, now())
            on conflict (api_key_id, usage_date) do update
            set
                rate_limited_requests =
                    mother_api.api_key_usage_daily.rate_limited_requests + 1,
                last_used_at = now(),
                updated_at = now()
            "#,
        )
        .bind(api_key_id)
        .execute(pool)
        .await
        .map_err(RepositoryError::new)?;

        Ok(())
    }

    pub(crate) async fn increment_daily_response(
        &self,
        api_key_id: Uuid,
        response_class: UsageResponseClass,
    ) -> Result<(), RepositoryError> {
        #[cfg(test)]
        if let Self::InMemory(keys) = self {
            let mut keys = keys
                .lock()
                .expect("in-memory API-key repository mutex poisoned");
            let usage = keys.usage.entry(api_key_id).or_default();
            match response_class {
                UsageResponseClass::Successful => usage.successful_responses += 1,
                UsageResponseClass::ClientError => usage.client_error_responses += 1,
                UsageResponseClass::ServerError => usage.server_error_responses += 1,
            }
            usage.daily_last_used_updated = true;
            return Ok(());
        }

        #[cfg(test)]
        if matches!(self, Self::Unavailable) {
            return Err(RepositoryError::test());
        }

        let pool = self.database_pool()?;
        match response_class {
            UsageResponseClass::Successful => {
                increment_response_counter(pool, api_key_id, ResponseCounter::Successful).await
            }
            UsageResponseClass::ClientError => {
                increment_response_counter(pool, api_key_id, ResponseCounter::ClientError).await
            }
            UsageResponseClass::ServerError => {
                increment_response_counter(pool, api_key_id, ResponseCounter::ServerError).await
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct IssueApiKeyInput {
    pub(crate) consumer_slug: String,
    pub(crate) display_name: String,
    pub(crate) category: String,
    pub(crate) label: String,
    pub(crate) key_prefix: String,
    pub(crate) key_hash: Vec<u8>,
    pub(crate) requests_per_minute: i32,
    pub(crate) requests_per_day: i32,
    pub(crate) expires_at: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct IssuedApiKey {
    pub(crate) api_key_id: Uuid,
    pub(crate) consumer_id: Uuid,
    pub(crate) consumer_slug: String,
    pub(crate) consumer_reused: bool,
    pub(crate) key_prefix: String,
    pub(crate) label: String,
    pub(crate) status: String,
    pub(crate) expires_at: Option<String>,
    pub(crate) created_at: String,
    pub(crate) requests_per_minute: i32,
    pub(crate) requests_per_day: i32,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum ApiKeyIssueRepositoryError {
    #[error("{0}")]
    Repository(#[from] RepositoryError),
    #[error("generated API key collided with an existing key")]
    GeneratedKeyCollision,
    #[error("{0}")]
    ConsumerConflict(String),
}

#[derive(FromRow)]
struct IssueConsumerRow {
    consumer_id: Uuid,
    consumer_slug: String,
    display_name: String,
    category: String,
    consumer_reused: bool,
}

#[derive(FromRow)]
struct IssuedApiKeyRow {
    api_key_id: Uuid,
    key_prefix: String,
    label: String,
    status: String,
    expires_at: Option<String>,
    created_at: String,
}

async fn upsert_consumer_for_issue(
    transaction: &mut Transaction<'_, Postgres>,
    consumer_slug: &str,
    display_name: &str,
    category: &str,
) -> Result<IssueConsumerRow, ApiKeyIssueRepositoryError> {
    let inserted = sqlx::query_as::<_, IssueConsumerRow>(
        r#"
        insert into mother_api.api_consumer (
            slug,
            display_name,
            category,
            status
        )
        values ($1, $2, $3, 'active')
        on conflict (slug) do nothing
        returning
            id as consumer_id,
            slug as consumer_slug,
            display_name,
            category,
            false as consumer_reused
        "#,
    )
    .bind(consumer_slug)
    .bind(display_name)
    .bind(category)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(|error| ApiKeyIssueRepositoryError::Repository(RepositoryError::new(error)))?;

    if let Some(consumer) = inserted {
        return Ok(consumer);
    }

    let existing = sqlx::query_as::<_, IssueConsumerRow>(
        r#"
        select
            id as consumer_id,
            slug as consumer_slug,
            display_name,
            category,
            true as consumer_reused
        from mother_api.api_consumer
        where slug = $1
        for update
        "#,
    )
    .bind(consumer_slug)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(|error| ApiKeyIssueRepositoryError::Repository(RepositoryError::new(error)))?
    .ok_or_else(|| {
        ApiKeyIssueRepositoryError::Repository(RepositoryError::protocol(format!(
            "API consumer {consumer_slug:?} conflicted during issue but could not be reloaded"
        )))
    })?;

    if existing.display_name != display_name {
        return Err(ApiKeyIssueRepositoryError::ConsumerConflict(format!(
            "existing API consumer {consumer_slug:?} has a different display name"
        )));
    }
    if existing.category != category {
        return Err(ApiKeyIssueRepositoryError::ConsumerConflict(format!(
            "existing API consumer {consumer_slug:?} has a different category"
        )));
    }

    Ok(existing)
}

async fn insert_issued_key(
    transaction: &mut Transaction<'_, Postgres>,
    consumer: &IssueConsumerRow,
    input: &IssueApiKeyInput,
) -> Result<IssuedApiKey, ApiKeyIssueRepositoryError> {
    let row = sqlx::query_as::<_, IssuedApiKeyRow>(
        r#"
        insert into mother_api.api_key (
            consumer_id,
            label,
            key_prefix,
            key_hash,
            expires_at
        )
        values ($1, $2, $3, $4, $5::timestamptz)
        returning
            id as api_key_id,
            key_prefix,
            label,
            status,
            expires_at::text as expires_at,
            created_at::text as created_at
        "#,
    )
    .bind(consumer.consumer_id)
    .bind(&input.label)
    .bind(&input.key_prefix)
    .bind(&input.key_hash)
    .bind(&input.expires_at)
    .fetch_one(&mut **transaction)
    .await
    .map_err(map_issue_insert_error)?;

    Ok(IssuedApiKey {
        api_key_id: row.api_key_id,
        consumer_id: consumer.consumer_id,
        consumer_slug: consumer.consumer_slug.clone(),
        consumer_reused: consumer.consumer_reused,
        key_prefix: row.key_prefix,
        label: row.label,
        status: row.status,
        expires_at: row.expires_at,
        created_at: row.created_at,
        requests_per_minute: input.requests_per_minute,
        requests_per_day: input.requests_per_day,
    })
}

async fn insert_policy_for_issue(
    transaction: &mut Transaction<'_, Postgres>,
    api_key_id: Uuid,
    requests_per_minute: i32,
    requests_per_day: i32,
) -> Result<(), ApiKeyIssueRepositoryError> {
    sqlx::query(
        r#"
        insert into mother_api.api_key_policy (
            api_key_id,
            requests_per_minute,
            requests_per_day
        )
        values ($1, $2, $3)
        "#,
    )
    .bind(api_key_id)
    .bind(requests_per_minute)
    .bind(requests_per_day)
    .execute(&mut **transaction)
    .await
    .map_err(|error| ApiKeyIssueRepositoryError::Repository(RepositoryError::new(error)))?;

    Ok(())
}

fn map_issue_insert_error(error: sqlx::Error) -> ApiKeyIssueRepositoryError {
    if let sqlx::Error::Database(database_error) = &error {
        if matches!(
            database_error.constraint(),
            Some("api_key_key_prefix_unique" | "api_key_key_hash_unique")
        ) {
            return ApiKeyIssueRepositoryError::GeneratedKeyCollision;
        }
    }

    ApiKeyIssueRepositoryError::Repository(RepositoryError::new(error))
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

#[derive(Clone, Debug, Eq, FromRow, PartialEq, Serialize)]
pub(crate) struct ApiKeyListItem {
    pub(crate) key_prefix: String,
    pub(crate) label: String,
    pub(crate) status: String,
    pub(crate) expires_at: Option<String>,
    pub(crate) created_at: String,
    pub(crate) last_used_at: Option<String>,
}

#[derive(Clone, Debug, Eq, FromRow, PartialEq, Serialize)]
pub(crate) struct ApiKeyUsageItem {
    pub(crate) usage_date: String,
    pub(crate) key_prefix: String,
    pub(crate) accepted_requests: i64,
    pub(crate) rate_limited_requests: i64,
    pub(crate) successful_responses: i64,
    pub(crate) client_error_responses: i64,
    pub(crate) server_error_responses: i64,
    pub(crate) last_used_at: Option<String>,
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
    fn from_database_value(value: &str) -> Result<Self, RepositoryError> {
        match value {
            "accepted" => Ok(Self::Accepted),
            "limit_exceeded" => Ok(Self::LimitExceeded),
            "missing_policy" => Ok(Self::MissingPolicy),
            unexpected => Err(RepositoryError::protocol(format!(
                "unexpected daily accepted outcome {unexpected:?}"
            ))),
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
                    successful_responses,
                    last_used_at
                )
                values ($1, (now() at time zone 'utc')::date, 1, now())
                on conflict (api_key_id, usage_date) do update
                set
                    successful_responses =
                        mother_api.api_key_usage_daily.successful_responses + 1,
                    last_used_at = now(),
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
                    client_error_responses,
                    last_used_at
                )
                values ($1, (now() at time zone 'utc')::date, 1, now())
                on conflict (api_key_id, usage_date) do update
                set
                    client_error_responses =
                        mother_api.api_key_usage_daily.client_error_responses + 1,
                    last_used_at = now(),
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
                    server_error_responses,
                    last_used_at
                )
                values ($1, (now() at time zone 'utc')::date, 1, now())
                on conflict (api_key_id, usage_date) do update
                set
                    server_error_responses =
                        mother_api.api_key_usage_daily.server_error_responses + 1,
                    last_used_at = now(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daily_accepted_outcome_rejects_unexpected_database_values() {
        let error = DailyAcceptedOutcome::from_database_value("surprise").unwrap_err();

        assert!(error
            .to_string()
            .contains("unexpected daily accepted outcome"));
    }
}
