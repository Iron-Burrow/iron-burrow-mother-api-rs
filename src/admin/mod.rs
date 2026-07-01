use serde::Serialize;
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

use crate::{
    adapters::postgres::api_keys::{
        ApiKeyIssueRepositoryError, ApiKeyListItem, ApiKeyRevocation, ApiKeyUsageItem,
        IssueApiKeyInput, IssuedApiKey,
    },
    adapters::postgres::ApiKeyRepository,
    cli::{
        AdminApiKeyCommand, AdminCommand, ApiKeyIssueArgs, ApiKeyListArgs, ApiKeyRevokeArgs,
        ApiKeyUsageArgs, OutputFormat,
    },
    domain::api_keys::{ApiKeyGenerationError, RawApiKey},
};

const ISSUE_GENERATED_KEY_ATTEMPTS: usize = 3;

#[derive(Debug, thiserror::Error)]
pub(crate) enum AdminError {
    #[error("DATABASE_URL is required for admin commands")]
    MissingDatabaseUrl,
    #[error("failed to connect to database for admin command: {0}")]
    DatabaseConnection(String),
    #[error("{0}")]
    KeyGeneration(#[from] ApiKeyGenerationError),
    #[error("{0}")]
    Operation(String),
}

pub(crate) async fn run(command: AdminCommand) -> Result<(), AdminError> {
    let database_url = database_url_from_env()?;
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&database_url)
        .await
        .map_err(|error| AdminError::DatabaseConnection(error.to_string()))?;
    let repository = ApiKeyRepository::database(pool);
    let output = execute(command, &repository).await?;

    println!("{}", output.render()?);

    Ok(())
}

async fn execute(
    command: AdminCommand,
    repository: &ApiKeyRepository,
) -> Result<AdminOutput, AdminError> {
    match command {
        AdminCommand::ApiKey(AdminApiKeyCommand::Issue(args)) => issue(args, repository).await,
        AdminCommand::ApiKey(AdminApiKeyCommand::Revoke(args)) => revoke(args, repository).await,
        AdminCommand::ApiKey(AdminApiKeyCommand::List(args)) => list(args, repository).await,
        AdminCommand::ApiKey(AdminApiKeyCommand::Usage(args)) => usage(args, repository).await,
    }
}

async fn issue(
    args: ApiKeyIssueArgs,
    repository: &ApiKeyRepository,
) -> Result<AdminOutput, AdminError> {
    for _attempt in 0..ISSUE_GENERATED_KEY_ATTEMPTS {
        let raw_key = RawApiKey::generate()?;
        let issued = repository
            .issue_api_key(IssueApiKeyInput {
                consumer_slug: args.consumer_slug.clone(),
                display_name: args.display_name.clone(),
                category: args.category.clone(),
                label: args.label.clone(),
                key_prefix: raw_key
                    .key_prefix()
                    .map_err(|error| AdminError::Operation(error.to_string()))?,
                key_hash: raw_key.sha256_hash().to_vec(),
                requests_per_minute: args.requests_per_minute,
                requests_per_day: args.requests_per_day,
                expires_at: args.expires_at.clone(),
            })
            .await;

        match issued {
            Ok(issued) => {
                return Ok(AdminOutput {
                    format: args.format,
                    payload: AdminPayload::Issued(IssuedApiKeyOutput { raw_key, issued }),
                });
            }
            Err(ApiKeyIssueRepositoryError::GeneratedKeyCollision) => continue,
            Err(ApiKeyIssueRepositoryError::ConsumerConflict(message)) => {
                return Err(AdminError::Operation(message));
            }
            Err(ApiKeyIssueRepositoryError::Repository(error)) => {
                return Err(AdminError::Operation(error.to_string()));
            }
        }
    }

    Err(AdminError::Operation(
        "failed to generate a unique API key after 3 attempts".to_string(),
    ))
}

async fn revoke(
    args: ApiKeyRevokeArgs,
    repository: &ApiKeyRepository,
) -> Result<AdminOutput, AdminError> {
    let revocation = repository
        .revoke_by_prefix(&args.key_prefix)
        .await
        .map_err(|error| AdminError::Operation(error.to_string()))?
        .ok_or_else(|| {
            AdminError::Operation(format!(
                "API key prefix {:?} was not found",
                args.key_prefix
            ))
        })?;

    Ok(AdminOutput {
        format: args.format,
        payload: AdminPayload::Revoked(revocation),
    })
}

async fn list(
    args: ApiKeyListArgs,
    repository: &ApiKeyRepository,
) -> Result<AdminOutput, AdminError> {
    let keys = repository
        .list_for_consumer(&args.consumer_slug)
        .await
        .map_err(|error| AdminError::Operation(error.to_string()))?;

    Ok(AdminOutput {
        format: args.format,
        payload: AdminPayload::List {
            consumer_slug: args.consumer_slug,
            keys,
        },
    })
}

async fn usage(
    args: ApiKeyUsageArgs,
    repository: &ApiKeyRepository,
) -> Result<AdminOutput, AdminError> {
    let rows = repository
        .usage_for_consumer(&args.consumer_slug, args.days)
        .await
        .map_err(|error| AdminError::Operation(error.to_string()))?;

    Ok(AdminOutput {
        format: args.format,
        payload: AdminPayload::Usage {
            consumer_slug: args.consumer_slug,
            days: args.days,
            rows,
        },
    })
}

fn database_url_from_env() -> Result<String, AdminError> {
    std::env::var("DATABASE_URL")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or(AdminError::MissingDatabaseUrl)
}

#[derive(Debug)]
struct AdminOutput {
    format: OutputFormat,
    payload: AdminPayload,
}

#[derive(Debug)]
enum AdminPayload {
    Issued(IssuedApiKeyOutput),
    Revoked(ApiKeyRevocation),
    List {
        consumer_slug: String,
        keys: Vec<ApiKeyListItem>,
    },
    Usage {
        consumer_slug: String,
        days: u32,
        rows: Vec<ApiKeyUsageItem>,
    },
}

#[derive(Debug)]
struct IssuedApiKeyOutput {
    raw_key: RawApiKey,
    issued: IssuedApiKey,
}

impl AdminOutput {
    fn render(&self) -> Result<String, AdminError> {
        match self.format {
            OutputFormat::Human => Ok(self.render_human()),
            OutputFormat::Json => self.render_json(),
        }
    }

    fn render_human(&self) -> String {
        match &self.payload {
            AdminPayload::Issued(output) => format!(
                "Issued API key\napi_key: {}\nkey_prefix: {}\nconsumer_slug: {}\nconsumer_reused: {}\nlabel: {}\nstatus: {}\nrequests_per_minute: {}\nrequests_per_day: {}\nexpires_at: {}\ncreated_at: {}",
                output.raw_key.expose_secret(),
                output.issued.key_prefix,
                output.issued.consumer_slug,
                output.issued.consumer_reused,
                output.issued.label,
                output.issued.status,
                output.issued.requests_per_minute,
                output.issued.requests_per_day,
                display_optional(output.issued.expires_at.as_deref()),
                output.issued.created_at
            ),
            AdminPayload::Revoked(revocation) => format!(
                "Revoked API key\nkey_prefix: {}\nstatus: {}\nrevoked_at: {}",
                revocation.key_prefix, revocation.status, revocation.revoked_at
            ),
            AdminPayload::List {
                consumer_slug,
                keys,
            } => {
                if keys.is_empty() {
                    return format!("No API keys found for consumer {consumer_slug}");
                }

                let mut lines = vec![format!("API keys for {consumer_slug}")];
                for key in keys {
                    lines.push(format!(
                        "key_prefix={} label={} status={} expires_at={} created_at={} last_used_at={}",
                        key.key_prefix,
                        key.label,
                        key.status,
                        display_optional(key.expires_at.as_deref()),
                        key.created_at,
                        display_optional(key.last_used_at.as_deref())
                    ));
                }
                lines.join("\n")
            }
            AdminPayload::Usage {
                consumer_slug,
                days,
                rows,
            } => {
                if rows.is_empty() {
                    return format!(
                        "No API-key usage found for consumer {consumer_slug} in the last {days} days"
                    );
                }

                let mut lines = vec![format!(
                    "API-key usage for {consumer_slug} in the last {days} days"
                )];
                for row in rows {
                    lines.push(format!(
                        "usage_date={} key_prefix={} accepted={} rate_limited={} successful={} client_errors={} server_errors={} last_used_at={}",
                        row.usage_date,
                        row.key_prefix,
                        row.accepted_requests,
                        row.rate_limited_requests,
                        row.successful_responses,
                        row.client_error_responses,
                        row.server_error_responses,
                        display_optional(row.last_used_at.as_deref())
                    ));
                }
                lines.join("\n")
            }
        }
    }

    fn render_json(&self) -> Result<String, AdminError> {
        match &self.payload {
            AdminPayload::Issued(output) => serde_json::to_string_pretty(&IssuedJson {
                ok: true,
                api_key: output.raw_key.expose_secret(),
                key_prefix: &output.issued.key_prefix,
                api_key_id: output.issued.api_key_id,
                consumer_id: output.issued.consumer_id,
                consumer_slug: &output.issued.consumer_slug,
                consumer_reused: output.issued.consumer_reused,
                label: &output.issued.label,
                status: &output.issued.status,
                requests_per_minute: output.issued.requests_per_minute,
                requests_per_day: output.issued.requests_per_day,
                expires_at: output.issued.expires_at.as_deref(),
                created_at: &output.issued.created_at,
            }),
            AdminPayload::Revoked(revocation) => serde_json::to_string_pretty(&RevokedJson {
                ok: true,
                api_key_id: revocation.api_key_id,
                key_prefix: &revocation.key_prefix,
                status: &revocation.status,
                revoked_at: &revocation.revoked_at,
            }),
            AdminPayload::List {
                consumer_slug,
                keys,
            } => serde_json::to_string_pretty(&ListJson {
                ok: true,
                consumer_slug,
                count: keys.len(),
                keys,
            }),
            AdminPayload::Usage {
                consumer_slug,
                days,
                rows,
            } => serde_json::to_string_pretty(&UsageJson {
                ok: true,
                consumer_slug,
                days: *days,
                count: rows.len(),
                usage: rows,
            }),
        }
        .map_err(|error| AdminError::Operation(format!("failed to render JSON output: {error}")))
    }
}

fn display_optional(value: Option<&str>) -> &str {
    value.unwrap_or("-")
}

#[derive(Serialize)]
struct IssuedJson<'a> {
    ok: bool,
    api_key: &'a str,
    key_prefix: &'a str,
    api_key_id: Uuid,
    consumer_id: Uuid,
    consumer_slug: &'a str,
    consumer_reused: bool,
    label: &'a str,
    status: &'a str,
    requests_per_minute: i32,
    requests_per_day: i32,
    expires_at: Option<&'a str>,
    created_at: &'a str,
}

#[derive(Serialize)]
struct RevokedJson<'a> {
    ok: bool,
    api_key_id: Uuid,
    key_prefix: &'a str,
    status: &'a str,
    revoked_at: &'a str,
}

#[derive(Serialize)]
struct ListJson<'a> {
    ok: bool,
    consumer_slug: &'a str,
    count: usize,
    keys: &'a [ApiKeyListItem],
}

#[derive(Serialize)]
struct UsageJson<'a> {
    ok: bool,
    consumer_slug: &'a str,
    days: u32,
    count: usize,
    usage: &'a [ApiKeyUsageItem],
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        cli::{
            AdminApiKeyCommand, AdminCommand, ApiKeyIssueArgs, ApiKeyListArgs, ApiKeyRevokeArgs,
            ApiKeyUsageArgs,
        },
        domain::api_keys::parse_presented_api_key,
        test_utils::postgres::migrated_pool,
    };
    use sqlx::PgPool;

    fn sample_raw_key() -> RawApiKey {
        RawApiKey::from_test_value(
            "ib_live_0123456789abcdef.0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        )
    }

    fn sample_issued() -> IssuedApiKey {
        IssuedApiKey {
            api_key_id: Uuid::new_v4(),
            consumer_id: Uuid::new_v4(),
            consumer_slug: "first-customer".to_string(),
            consumer_reused: false,
            key_prefix: "ib_live_0123456789abcdef".to_string(),
            label: "beta key".to_string(),
            status: "active".to_string(),
            expires_at: Some("2026-09-30 00:00:00+00".to_string()),
            created_at: "2026-07-01 00:00:00+00".to_string(),
            requests_per_minute: 60,
            requests_per_day: 5000,
        }
    }

    async fn delete_test_consumer(pool: &PgPool, consumer_slug: &str) {
        sqlx::query(
            r#"
            delete from mother_api.api_key
            where consumer_id in (
                select id
                from mother_api.api_consumer
                where slug = $1
            )
            "#,
        )
        .bind(consumer_slug)
        .execute(pool)
        .await
        .unwrap();

        sqlx::query("delete from mother_api.api_consumer where slug = $1")
            .bind(consumer_slug)
            .execute(pool)
            .await
            .unwrap();
    }

    fn issue_command(consumer_slug: &str, display_name: &str, category: &str) -> AdminCommand {
        AdminCommand::ApiKey(AdminApiKeyCommand::Issue(ApiKeyIssueArgs {
            consumer_slug: consumer_slug.to_string(),
            display_name: display_name.to_string(),
            category: category.to_string(),
            label: "beta key".to_string(),
            requests_per_minute: 11,
            requests_per_day: 22,
            expires_at: Some("2026-09-30T00:00:00Z".to_string()),
            format: OutputFormat::Json,
        }))
    }

    #[test]
    fn issue_human_output_prints_raw_key_once_without_hash() {
        let raw_key = sample_raw_key();
        let output = AdminOutput {
            format: OutputFormat::Human,
            payload: AdminPayload::Issued(IssuedApiKeyOutput {
                raw_key: raw_key.clone(),
                issued: sample_issued(),
            }),
        };

        let rendered = output.render().unwrap();

        assert_eq!(rendered.matches(raw_key.expose_secret()).count(), 1);
        assert!(!rendered.contains("key_hash"));
        assert!(!rendered.contains(&hex::encode(raw_key.sha256_hash())));
    }

    #[test]
    fn issue_json_output_prints_raw_key_once_without_hash() {
        let raw_key = sample_raw_key();
        let output = AdminOutput {
            format: OutputFormat::Json,
            payload: AdminPayload::Issued(IssuedApiKeyOutput {
                raw_key: raw_key.clone(),
                issued: sample_issued(),
            }),
        };

        let rendered = output.render().unwrap();

        assert_eq!(rendered.matches(raw_key.expose_secret()).count(), 1);
        assert!(!rendered.contains("key_hash"));
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&rendered).unwrap()["api_key"],
            raw_key.expose_secret()
        );
    }

    #[test]
    fn non_issue_outputs_do_not_include_raw_keys_or_hashes() {
        let raw_key = sample_raw_key();
        let outputs = [
            AdminOutput {
                format: OutputFormat::Human,
                payload: AdminPayload::Revoked(ApiKeyRevocation {
                    api_key_id: Uuid::new_v4(),
                    key_prefix: "ib_live_0123456789abcdef".to_string(),
                    status: "revoked".to_string(),
                    revoked_at: "2026-07-01 00:00:00+00".to_string(),
                }),
            },
            AdminOutput {
                format: OutputFormat::Json,
                payload: AdminPayload::List {
                    consumer_slug: "first-customer".to_string(),
                    keys: vec![ApiKeyListItem {
                        key_prefix: "ib_live_0123456789abcdef".to_string(),
                        label: "beta key".to_string(),
                        status: "active".to_string(),
                        expires_at: None,
                        created_at: "2026-07-01 00:00:00+00".to_string(),
                        last_used_at: None,
                    }],
                },
            },
            AdminOutput {
                format: OutputFormat::Human,
                payload: AdminPayload::Usage {
                    consumer_slug: "first-customer".to_string(),
                    days: 30,
                    rows: vec![ApiKeyUsageItem {
                        usage_date: "2026-07-01".to_string(),
                        key_prefix: "ib_live_0123456789abcdef".to_string(),
                        accepted_requests: 1,
                        rate_limited_requests: 2,
                        successful_responses: 3,
                        client_error_responses: 4,
                        server_error_responses: 5,
                        last_used_at: None,
                    }],
                },
            },
        ];

        for output in outputs {
            let rendered = output.render().unwrap();
            assert!(!rendered.contains(raw_key.expose_secret()));
            assert!(!rendered.contains("key_hash"));
        }
    }

    #[tokio::test]
    async fn issue_command_creates_key_policy_and_reuses_matching_consumer() {
        let Some(pool) = migrated_pool().await else {
            return;
        };

        let repository = ApiKeyRepository::database(pool.clone());
        let consumer_slug = format!("admin-issue-{}", Uuid::new_v4().simple());
        delete_test_consumer(&pool, &consumer_slug).await;

        let first = execute(
            issue_command(&consumer_slug, "Admin Issue Consumer", "partner"),
            &repository,
        )
        .await
        .unwrap();
        let AdminPayload::Issued(first_issue) = &first.payload else {
            panic!("expected issue output");
        };
        assert!(!first_issue.issued.consumer_reused);

        let raw_key = first_issue.raw_key.expose_secret().to_string();
        let parsed = parse_presented_api_key(&raw_key).unwrap();
        let key_hash = first_issue.raw_key.sha256_hash();
        let lookup = repository
            .find_key_by_prefix_and_hash(&parsed.key_prefix, &key_hash)
            .await
            .unwrap()
            .expect("issued key should be findable by prefix and hash");
        let policy = repository
            .find_policy(lookup.api_key_id)
            .await
            .unwrap()
            .expect("issued key should have policy");

        assert_eq!(lookup.consumer_slug, consumer_slug);
        assert_eq!(lookup.key_label, "beta key");
        assert_eq!(policy.requests_per_minute, 11);
        assert_eq!(policy.requests_per_day, 22);

        let persisted_raw_count = sqlx::query_scalar::<_, i64>(
            r#"
            select count(*)
            from mother_api.api_consumer consumer
            join mother_api.api_key api_key
                on api_key.consumer_id = consumer.id
            where consumer.slug = $1
                and (
                    consumer.slug = $2
                    or consumer.display_name = $2
                    or consumer.metadata::text like $3
                    or api_key.label = $2
                    or api_key.key_prefix = $2
                    or api_key.metadata::text like $3
                )
            "#,
        )
        .bind(&consumer_slug)
        .bind(&raw_key)
        .bind(format!("%{raw_key}%"))
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(persisted_raw_count, 0);

        let second = execute(
            issue_command(&consumer_slug, "Admin Issue Consumer", "partner"),
            &repository,
        )
        .await
        .unwrap();
        let AdminPayload::Issued(second_issue) = &second.payload else {
            panic!("expected issue output");
        };
        assert!(second_issue.issued.consumer_reused);

        let key_count = sqlx::query_scalar::<_, i64>(
            r#"
            select count(*)
            from mother_api.api_key api_key
            join mother_api.api_consumer consumer
                on consumer.id = api_key.consumer_id
            where consumer.slug = $1
            "#,
        )
        .bind(&consumer_slug)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(key_count, 2);

        delete_test_consumer(&pool, &consumer_slug).await;
    }

    #[tokio::test]
    async fn concurrent_issue_commands_reuse_the_same_new_consumer() {
        let Some(pool) = migrated_pool().await else {
            return;
        };

        let repository = ApiKeyRepository::database(pool.clone());
        let consumer_slug = format!("admin-concurrent-{}", Uuid::new_v4().simple());
        delete_test_consumer(&pool, &consumer_slug).await;

        let (first, second) = tokio::join!(
            execute(
                issue_command(&consumer_slug, "Concurrent Consumer", "partner"),
                &repository
            ),
            execute(
                issue_command(&consumer_slug, "Concurrent Consumer", "partner"),
                &repository
            )
        );
        let first = first.unwrap();
        let second = second.unwrap();
        let AdminPayload::Issued(first_issue) = first.payload else {
            panic!("expected issue output");
        };
        let AdminPayload::Issued(second_issue) = second.payload else {
            panic!("expected issue output");
        };

        let mut reused_flags = vec![
            first_issue.issued.consumer_reused,
            second_issue.issued.consumer_reused,
        ];
        reused_flags.sort();
        assert_eq!(reused_flags, vec![false, true]);
        assert_eq!(
            first_issue.issued.consumer_id,
            second_issue.issued.consumer_id
        );

        let counts = sqlx::query_as::<_, (i64, i64, i64)>(
            r#"
            select
                count(distinct consumer.id) as consumer_count,
                count(distinct api_key.id) as key_count,
                count(distinct policy.api_key_id) as policy_count
            from mother_api.api_consumer consumer
            left join mother_api.api_key api_key
                on api_key.consumer_id = consumer.id
            left join mother_api.api_key_policy policy
                on policy.api_key_id = api_key.id
            where consumer.slug = $1
            "#,
        )
        .bind(&consumer_slug)
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(counts, (1, 2, 2));

        delete_test_consumer(&pool, &consumer_slug).await;
    }

    #[tokio::test]
    async fn issue_command_rejects_consumer_conflicts_without_partial_key() {
        let Some(pool) = migrated_pool().await else {
            return;
        };

        let repository = ApiKeyRepository::database(pool.clone());
        let consumer_slug = format!("admin-conflict-{}", Uuid::new_v4().simple());
        delete_test_consumer(&pool, &consumer_slug).await;

        execute(
            issue_command(&consumer_slug, "Conflict Consumer", "partner"),
            &repository,
        )
        .await
        .unwrap();

        let error = execute(
            issue_command(&consumer_slug, "Different Consumer", "partner"),
            &repository,
        )
        .await
        .unwrap_err();

        assert!(error.to_string().contains("different display name"));

        let key_count = sqlx::query_scalar::<_, i64>(
            r#"
            select count(*)
            from mother_api.api_key api_key
            join mother_api.api_consumer consumer
                on consumer.id = api_key.consumer_id
            where consumer.slug = $1
            "#,
        )
        .bind(&consumer_slug)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(key_count, 1);

        delete_test_consumer(&pool, &consumer_slug).await;
    }

    #[tokio::test]
    async fn revoke_list_and_usage_commands_omit_raw_keys_and_hashes() {
        let Some(pool) = migrated_pool().await else {
            return;
        };

        let repository = ApiKeyRepository::database(pool.clone());
        let consumer_slug = format!("admin-ops-{}", Uuid::new_v4().simple());
        delete_test_consumer(&pool, &consumer_slug).await;

        let issued = execute(
            issue_command(&consumer_slug, "Admin Ops Consumer", "friend"),
            &repository,
        )
        .await
        .unwrap();
        let AdminPayload::Issued(issued) = issued.payload else {
            panic!("expected issue output");
        };
        let raw_key = issued.raw_key.expose_secret().to_string();

        sqlx::query(
            r#"
            insert into mother_api.api_key_usage_daily (
                api_key_id,
                usage_date,
                accepted_requests,
                rate_limited_requests,
                successful_responses,
                client_error_responses,
                server_error_responses,
                last_used_at
            )
            values ($1, (now() at time zone 'utc')::date, 1, 2, 3, 4, 5, now())
            "#,
        )
        .bind(issued.issued.api_key_id)
        .execute(&pool)
        .await
        .unwrap();

        let list_output = execute(
            AdminCommand::ApiKey(AdminApiKeyCommand::List(ApiKeyListArgs {
                consumer_slug: consumer_slug.clone(),
                format: OutputFormat::Json,
            })),
            &repository,
        )
        .await
        .unwrap();
        let list_rendered = list_output.render().unwrap();
        assert!(list_rendered.contains(&issued.issued.key_prefix));
        assert!(!list_rendered.contains(&raw_key));
        assert!(!list_rendered.contains("key_hash"));

        let usage_output = execute(
            AdminCommand::ApiKey(AdminApiKeyCommand::Usage(ApiKeyUsageArgs {
                consumer_slug: consumer_slug.clone(),
                days: 30,
                format: OutputFormat::Human,
            })),
            &repository,
        )
        .await
        .unwrap();
        let usage_rendered = usage_output.render().unwrap();
        assert!(usage_rendered.contains("accepted=1"));
        assert!(usage_rendered.contains("rate_limited=2"));
        assert!(!usage_rendered.contains(&raw_key));
        assert!(!usage_rendered.contains("key_hash"));

        let revoke_output = execute(
            AdminCommand::ApiKey(AdminApiKeyCommand::Revoke(ApiKeyRevokeArgs {
                key_prefix: issued.issued.key_prefix.clone(),
                format: OutputFormat::Human,
            })),
            &repository,
        )
        .await
        .unwrap();
        let revoke_rendered = revoke_output.render().unwrap();
        assert!(revoke_rendered.contains("status: revoked"));
        assert!(!revoke_rendered.contains(&raw_key));
        assert!(!revoke_rendered.contains("key_hash"));

        let second_revoke = execute(
            AdminCommand::ApiKey(AdminApiKeyCommand::Revoke(ApiKeyRevokeArgs {
                key_prefix: issued.issued.key_prefix.clone(),
                format: OutputFormat::Json,
            })),
            &repository,
        )
        .await
        .unwrap();
        assert!(second_revoke
            .render()
            .unwrap()
            .contains("\"status\": \"revoked\""));

        delete_test_consumer(&pool, &consumer_slug).await;
    }
}
