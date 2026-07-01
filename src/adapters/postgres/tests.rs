use crate::adapters::postgres::api_keys::{DailyAcceptedOutcome, UsageResponseClass};
use crate::adapters::postgres::ApiKeyRepository;
use crate::test_utils::postgres::migrated_pool;
use serde_json::json;
use sqlx::PgPool;
use sqlx::{Postgres, Transaction};

const IDENTITY_CONSTRAINTS_MIGRATION_SQL: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/migrations/0006_reference_data_identity_constraints.sql"
));

fn assert_database_constraint(error: sqlx::Error, expected_constraint: &str) {
    let sqlx::Error::Database(database_error) = error else {
        panic!("expected database error for constraint violation");
    };

    assert_eq!(database_error.constraint(), Some(expected_constraint));
}

fn assert_database_message_contains(error: sqlx::Error, expected_message: &str) {
    let sqlx::Error::Database(database_error) = error else {
        panic!("expected database error for migration prevalidation failure");
    };

    assert!(
        database_error.message().contains(expected_message),
        "expected database error message to contain {expected_message:?}, got {:?}",
        database_error.message()
    );
}

async fn insert_api_consumer(
    transaction: &mut Transaction<'_, Postgres>,
    slug: &str,
) -> uuid::Uuid {
    sqlx::query_scalar::<_, uuid::Uuid>(
        r#"
        insert into mother_api.api_consumer (
            slug,
            display_name,
            category,
            status
        )
        values ($1, 'Test Consumer', 'friend', 'active')
        returning id
        "#,
    )
    .bind(slug)
    .fetch_one(&mut **transaction)
    .await
    .unwrap()
}

async fn insert_api_key(
    transaction: &mut Transaction<'_, Postgres>,
    consumer_id: uuid::Uuid,
    key_prefix: &str,
    key_hash: Vec<u8>,
) -> uuid::Uuid {
    sqlx::query_scalar::<_, uuid::Uuid>(
        r#"
        insert into mother_api.api_key (
            consumer_id,
            label,
            key_prefix,
            key_hash
        )
        values ($1, 'Test Key', $2, $3)
        returning id
        "#,
    )
    .bind(consumer_id)
    .bind(key_prefix)
    .bind(key_hash)
    .fetch_one(&mut **transaction)
    .await
    .unwrap()
}

async fn insert_api_consumer_with_pool(pool: &PgPool, slug: &str) -> uuid::Uuid {
    sqlx::query_scalar::<_, uuid::Uuid>(
        r#"
        insert into mother_api.api_consumer (
            slug,
            display_name,
            category,
            status
        )
        values ($1, 'Repository Test Consumer', 'partner', 'active')
        returning id
        "#,
    )
    .bind(slug)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn insert_api_key_with_pool(
    pool: &PgPool,
    consumer_id: uuid::Uuid,
    key_prefix: &str,
    key_hash: Vec<u8>,
) -> uuid::Uuid {
    sqlx::query_scalar::<_, uuid::Uuid>(
        r#"
        insert into mother_api.api_key (
            consumer_id,
            label,
            key_prefix,
            key_hash
        )
        values ($1, 'Repository Test Key', $2, $3)
        returning id
        "#,
    )
    .bind(consumer_id)
    .bind(key_prefix)
    .bind(key_hash)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn delete_api_consumer_test_rows(pool: &PgPool, consumer_slug: &str) {
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

async fn insert_repository_test_key(
    pool: &PgPool,
    suffix: &str,
    key_hash: Vec<u8>,
) -> (String, String, uuid::Uuid, uuid::Uuid) {
    let consumer_slug = format!("repository-{suffix}");
    let key_prefix = format!("repository_{suffix}");

    delete_api_consumer_test_rows(pool, &consumer_slug).await;
    let consumer_id = insert_api_consumer_with_pool(pool, &consumer_slug).await;
    let key_id = insert_api_key_with_pool(pool, consumer_id, &key_prefix, key_hash).await;

    (consumer_slug, key_prefix, consumer_id, key_id)
}

#[tokio::test]
async fn global_asset_slug_is_unique_in_database() {
    let Some(pool) = migrated_pool().await else {
        return;
    };

    let mut transaction = pool.begin().await.unwrap();
    let slug = format!("slug-unique-test-{}", uuid::Uuid::new_v4());
    let canonical_path = format!("/assets/{slug}");

    sqlx::query(
        r#"
        insert into mother_api.global_asset (
            slug,
            symbol,
            name,
            canonical_path,
            status
        )
        values ($1, $2, $3, $4, 'inactive'::mother_api.global_asset_status)
        "#,
    )
    .bind(&slug)
    .bind("SLUGUNIQUEA")
    .bind("Slug Unique Test A")
    .bind(&canonical_path)
    .execute(&mut *transaction)
    .await
    .unwrap();

    let duplicate_result = sqlx::query(
        r#"
        insert into mother_api.global_asset (
            slug,
            symbol,
            name,
            canonical_path,
            status
        )
        values ($1, $2, $3, $4, 'inactive'::mother_api.global_asset_status)
        "#,
    )
    .bind(&slug)
    .bind("SLUGUNIQUEB")
    .bind("Slug Unique Test B")
    .bind(&canonical_path)
    .execute(&mut *transaction)
    .await;

    let error = duplicate_result.expect_err("duplicate slug should violate constraint");
    assert_database_constraint(error, "global_asset_slug_unique");

    transaction.rollback().await.unwrap();
}

#[tokio::test]
async fn global_asset_slug_is_normalized_in_database() {
    let Some(pool) = migrated_pool().await else {
        return;
    };

    let mut transaction = pool.begin().await.unwrap();
    let insert_result = sqlx::query(
        r#"
        insert into mother_api.global_asset (
            slug,
            symbol,
            name,
            canonical_path,
            status
        )
        values ($1, $2, $3, $4, 'inactive'::mother_api.global_asset_status)
        "#,
    )
    .bind("Slug-Normalized-Test")
    .bind("SLUGNORMALIZED")
    .bind("Slug Normalized Test")
    .bind("/assets/slug-normalized-test")
    .execute(&mut *transaction)
    .await;

    let error = insert_result.expect_err("mixed-case slug should violate constraint");
    assert_database_constraint(error, "global_asset_slug_normalized");

    transaction.rollback().await.unwrap();
}

#[tokio::test]
async fn full_migration_uses_canonical_evm_network_slugs() {
    let Some(pool) = migrated_pool().await else {
        return;
    };

    let canonical_networks = sqlx::query_as::<_, (String, i64, String)>(
        r#"
        select slug, chain_id, caip2
        from mother_api.network
        where status = 'active'
            and slug in ('base-mainnet', 'mantle-mainnet', 'arbitrum-mainnet')
        order by chain_id
        "#,
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    let legacy_count = sqlx::query_scalar::<_, i64>(
        r#"
        select count(*)
        from mother_api.network
        where status = 'active'
            and slug in ('base', 'mantle', 'arbitrum-one')
        "#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(
        canonical_networks,
        vec![
            (
                "mantle-mainnet".to_string(),
                5000,
                "eip155:5000".to_string()
            ),
            ("base-mainnet".to_string(), 8453, "eip155:8453".to_string()),
            (
                "arbitrum-mainnet".to_string(),
                42161,
                "eip155:42161".to_string()
            ),
        ]
    );
    assert_eq!(legacy_count, 0);
}

#[tokio::test]
async fn canonical_network_migration_preserves_ids_and_mapping_counts() {
    let Some(pool) = migrated_pool().await else {
        return;
    };

    let mut transaction = pool.begin().await.unwrap();

    let before = sqlx::query_as::<_, (String, String, i64)>(
        r#"
        select
            network.id::text,
            network.slug,
            count(asset_chain_map.id)
        from mother_api.network network
        left join mother_api.asset_chain_map asset_chain_map
            on asset_chain_map.network_id = network.id
        where network.status = 'active'
            and network.slug in (
            'base-mainnet',
            'mantle-mainnet',
            'arbitrum-mainnet'
            )
        group by network.id, network.slug
        order by network.id
        "#,
    )
    .fetch_all(&mut *transaction)
    .await
    .unwrap();

    sqlx::query(
        r#"
        update mother_api.network
        set slug = case slug
            when 'base-mainnet' then 'base'
            when 'mantle-mainnet' then 'mantle'
            when 'arbitrum-mainnet' then 'arbitrum-one'
        end
        where status = 'active'
            and slug in (
            'base-mainnet',
            'mantle-mainnet',
            'arbitrum-mainnet'
            )
        "#,
    )
    .execute(&mut *transaction)
    .await
    .unwrap();

    sqlx::raw_sql(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/migrations/0005_canonical_evm_network_slugs.sql"
    )))
    .execute(&mut *transaction)
    .await
    .unwrap();

    let after = sqlx::query_as::<_, (String, String, i64)>(
        r#"
        select
            network.id::text,
            network.slug,
            count(asset_chain_map.id)
        from mother_api.network network
        left join mother_api.asset_chain_map asset_chain_map
            on asset_chain_map.network_id = network.id
        where network.status = 'active'
            and network.slug in (
            'base-mainnet',
            'mantle-mainnet',
            'arbitrum-mainnet'
            )
        group by network.id, network.slug
        order by network.id
        "#,
    )
    .fetch_all(&mut *transaction)
    .await
    .unwrap();

    assert_eq!(after, before);

    transaction.rollback().await.unwrap();
}

#[tokio::test]
async fn canonical_network_migration_rejects_conflicting_rows() {
    let Some(pool) = migrated_pool().await else {
        return;
    };

    let mut transaction = pool.begin().await.unwrap();
    sqlx::query("alter table mother_api.network drop constraint network_slug_unique")
        .execute(&mut *transaction)
        .await
        .unwrap();

    sqlx::query(
        r#"
        update mother_api.network
        set slug = 'base'
        where status = 'active'
            and slug = 'base-mainnet'
        "#,
    )
    .execute(&mut *transaction)
    .await
    .unwrap();
    sqlx::query(
        r#"
        insert into mother_api.network (
            slug,
            name,
            family,
            chain_id,
            status
        )
        values (
            'base-mainnet',
            'Conflicting Base Mainnet',
            'evm',
            8453,
            'inactive'
        )
        "#,
    )
    .execute(&mut *transaction)
    .await
    .unwrap();

    let result = sqlx::raw_sql(include_str!(
        "../../../migrations/0005_canonical_evm_network_slugs.sql"
    ))
    .execute(&mut *transaction)
    .await;

    assert!(result.is_err());
    transaction.rollback().await.unwrap();
}

#[tokio::test]
async fn network_slug_is_globally_unique_in_database() {
    let Some(pool) = migrated_pool().await else {
        return;
    };

    let mut transaction = pool.begin().await.unwrap();
    let insert_result = sqlx::query(
        r#"
        insert into mother_api.network (
            slug,
            name,
            family,
            chain_id,
            status
        )
        values (
            'eth-mainnet',
            'Duplicate Ethereum Mainnet',
            'evm',
            1,
            'inactive'
        )
        "#,
    )
    .execute(&mut *transaction)
    .await;

    let error = insert_result.expect_err("duplicate network slug should violate constraint");
    assert_database_constraint(error, "network_slug_unique");

    transaction.rollback().await.unwrap();
}

#[tokio::test]
async fn network_slug_is_normalized_in_database() {
    let Some(pool) = migrated_pool().await else {
        return;
    };

    let mut transaction = pool.begin().await.unwrap();
    let insert_result = sqlx::query(
        r#"
        insert into mother_api.network (
            slug,
            name,
            family,
            chain_id,
            status
        )
        values (
            'Network-Slug-Normalized-Test',
            'Network Slug Normalized Test',
            'evm',
            999999,
            'inactive'
        )
        "#,
    )
    .execute(&mut *transaction)
    .await;

    let error = insert_result.expect_err("mixed-case network slug should violate constraint");
    assert_database_constraint(error, "network_slug_normalized");

    transaction.rollback().await.unwrap();
}

#[tokio::test]
async fn asset_chain_map_asset_network_identity_is_unique_in_database() {
    let Some(pool) = migrated_pool().await else {
        return;
    };

    let mut transaction = pool.begin().await.unwrap();
    let insert_result = sqlx::query(
        r#"
        insert into mother_api.asset_chain_map (
            asset_id,
            network_id,
            status
        )
        select
            asset.id,
            network.id,
            'inactive'
        from mother_api.global_asset asset
        join mother_api.network network
            on network.slug = 'eth-mainnet'
        where asset.slug = 'usdc'
        "#,
    )
    .execute(&mut *transaction)
    .await;

    let error =
        insert_result.expect_err("duplicate asset/network mapping should violate constraint");
    assert_database_constraint(error, "asset_chain_map_asset_network_unique");

    transaction.rollback().await.unwrap();
}

#[tokio::test]
async fn identity_constraint_migration_rejects_existing_duplicate_network_slugs() {
    let Some(pool) = migrated_pool().await else {
        return;
    };

    let mut transaction = pool.begin().await.unwrap();
    sqlx::query("alter table mother_api.network drop constraint network_slug_unique")
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query(
        r#"
        insert into mother_api.network (
            slug,
            name,
            family,
            chain_id,
            status
        )
        values (
            'eth-mainnet',
            'Duplicate Ethereum Mainnet',
            'evm',
            1,
            'inactive'
        )
        "#,
    )
    .execute(&mut *transaction)
    .await
    .unwrap();

    let result = sqlx::raw_sql(IDENTITY_CONSTRAINTS_MIGRATION_SQL)
        .execute(&mut *transaction)
        .await;

    let error = result.expect_err("migration should reject preexisting duplicate networks");
    assert_database_message_contains(error, "network contains duplicate slug identities");

    transaction.rollback().await.unwrap();
}

#[tokio::test]
async fn identity_constraint_migration_rejects_existing_non_normalized_network_slugs() {
    let Some(pool) = migrated_pool().await else {
        return;
    };

    let mut transaction = pool.begin().await.unwrap();
    sqlx::query("alter table mother_api.network drop constraint network_slug_normalized")
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query(
        r#"
        insert into mother_api.network (
            slug,
            name,
            family,
            chain_id,
            status
        )
        values (
            'Network-Slug-Prevalidation-Test',
            'Network Slug Prevalidation Test',
            'evm',
            999998,
            'inactive'
        )
        "#,
    )
    .execute(&mut *transaction)
    .await
    .unwrap();

    let result = sqlx::raw_sql(IDENTITY_CONSTRAINTS_MIGRATION_SQL)
        .execute(&mut *transaction)
        .await;

    let error = result.expect_err("migration should reject preexisting non-normalized networks");
    assert_database_message_contains(error, "network contains non-normalized slug values");

    transaction.rollback().await.unwrap();
}

#[tokio::test]
async fn identity_constraint_migration_rejects_existing_duplicate_asset_network_mappings() {
    let Some(pool) = migrated_pool().await else {
        return;
    };

    let mut transaction = pool.begin().await.unwrap();
    sqlx::query(
        "alter table mother_api.asset_chain_map drop constraint asset_chain_map_asset_network_unique",
    )
    .execute(&mut *transaction)
    .await
    .unwrap();
    sqlx::query(
        r#"
        insert into mother_api.asset_chain_map (
            asset_id,
            network_id,
            status
        )
        select
            asset.id,
            network.id,
            'inactive'
        from mother_api.global_asset asset
        join mother_api.network network
            on network.slug = 'eth-mainnet'
        where asset.slug = 'usdc'
        "#,
    )
    .execute(&mut *transaction)
    .await
    .unwrap();

    let result = sqlx::raw_sql(IDENTITY_CONSTRAINTS_MIGRATION_SQL)
        .execute(&mut *transaction)
        .await;

    let error = result.expect_err("migration should reject duplicate asset/network mappings");
    assert_database_message_contains(
        error,
        "asset_chain_map contains duplicate asset/network identities",
    );

    transaction.rollback().await.unwrap();
}

#[tokio::test]
async fn active_native_network_safety_constraint_is_preserved() {
    let Some(pool) = migrated_pool().await else {
        return;
    };

    let mut transaction = pool.begin().await.unwrap();
    let insert_result = sqlx::query(
        r#"
        insert into mother_api.asset_chain_map (
            asset_id,
            network_id,
            is_native,
            token_standard,
            status
        )
        select
            asset.id,
            network.id,
            true,
            'native',
            'active'
        from mother_api.global_asset asset
        join mother_api.network network
            on network.slug = 'eth-mainnet'
        where asset.slug = 'bitcoin'
        "#,
    )
    .execute(&mut *transaction)
    .await;

    let error = insert_result.expect_err("second active native mapping should violate constraint");
    assert_database_constraint(error, "uq_mother_api_asset_chain_map_active_native_network");

    transaction.rollback().await.unwrap();
}

#[tokio::test]
async fn active_network_address_safety_constraint_is_preserved() {
    let Some(pool) = migrated_pool().await else {
        return;
    };

    let mut transaction = pool.begin().await.unwrap();
    let insert_result = sqlx::query(
        r#"
        insert into mother_api.asset_chain_map (
            asset_id,
            network_id,
            is_native,
            deployment_address,
            token_standard,
            status
        )
        select
            asset.id,
            network.id,
            false,
            '0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48',
            'erc20',
            'active'
        from mother_api.global_asset asset
        join mother_api.network network
            on network.slug = 'eth-mainnet'
        where asset.slug = 'bitcoin'
        "#,
    )
    .execute(&mut *transaction)
    .await;

    let error = insert_result.expect_err("duplicate active deployment address should violate");
    assert_database_constraint(
        error,
        "uq_mother_api_asset_chain_map_active_network_address",
    );

    transaction.rollback().await.unwrap();
}

#[tokio::test]
async fn api_key_adoption_tables_exist_after_migration() {
    let Some(pool) = migrated_pool().await else {
        return;
    };

    let tables = sqlx::query_scalar::<_, String>(
        r#"
        select table_name
        from information_schema.tables
        where table_schema = 'mother_api'
            and table_name in ('api_consumer', 'api_key')
        order by table_name
        "#,
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    let constraints = sqlx::query_scalar::<_, String>(
        r#"
        select constraint_record.conname
        from pg_constraint constraint_record
        join pg_class relation_record
            on relation_record.oid = constraint_record.conrelid
        join pg_namespace namespace_record
            on namespace_record.oid = relation_record.relnamespace
        where namespace_record.nspname = 'mother_api'
            and relation_record.relname in ('api_consumer', 'api_key')
            and constraint_record.conname in (
                'api_consumer_slug_normalized',
                'api_consumer_category_known',
                'api_consumer_status_known',
                'api_consumer_metadata_object',
                'api_consumer_timestamps_sane',
                'api_key_prefix_normalized',
                'api_key_hash_algorithm_known',
                'api_key_hash_sha256_length',
                'api_key_status_known',
                'api_key_metadata_object',
                'api_key_timestamps_sane',
                'api_key_revoked_at_matches_status'
            )
        order by constraint_record.conname
        "#,
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    let indexes = sqlx::query_scalar::<_, String>(
        r#"
        select indexname
        from pg_indexes
        where schemaname = 'mother_api'
            and indexname in (
                'api_consumer_slug_unique',
                'api_key_key_prefix_unique',
                'api_key_key_hash_unique',
                'idx_api_key_consumer_id',
                'idx_api_key_active_key_prefix'
            )
        order by indexname
        "#,
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    assert_eq!(
        tables,
        vec!["api_consumer".to_string(), "api_key".to_string()]
    );
    assert_eq!(constraints.len(), 12);
    assert_eq!(
        indexes,
        vec![
            "api_consumer_slug_unique".to_string(),
            "api_key_key_hash_unique".to_string(),
            "api_key_key_prefix_unique".to_string(),
            "idx_api_key_active_key_prefix".to_string(),
            "idx_api_key_consumer_id".to_string()
        ]
    );
}

#[tokio::test]
async fn api_key_policy_usage_tables_exist_after_migration() {
    let Some(pool) = migrated_pool().await else {
        return;
    };

    let tables = sqlx::query_scalar::<_, String>(
        r#"
        select table_name
        from information_schema.tables
        where table_schema = 'mother_api'
            and table_name in ('api_key_policy', 'api_key_usage_daily')
        order by table_name
        "#,
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    let constraints = sqlx::query_scalar::<_, String>(
        r#"
        select constraint_record.conname
        from pg_constraint constraint_record
        join pg_class relation_record
            on relation_record.oid = constraint_record.conrelid
        join pg_namespace namespace_record
            on namespace_record.oid = relation_record.relnamespace
        where namespace_record.nspname = 'mother_api'
            and relation_record.relname in ('api_key_policy', 'api_key_usage_daily')
            and constraint_record.conname in (
                'api_key_policy_pkey',
                'api_key_policy_api_key_id_fkey',
                'api_key_policy_requests_per_minute_non_negative',
                'api_key_policy_requests_per_day_non_negative',
                'api_key_policy_timestamps_sane',
                'api_key_usage_daily_pkey',
                'api_key_usage_daily_api_key_id_fkey',
                'api_key_usage_daily_counts_non_negative',
                'api_key_usage_daily_timestamps_sane'
            )
        order by constraint_record.conname
        "#,
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    let indexes = sqlx::query_scalar::<_, String>(
        r#"
        select indexname
        from pg_indexes
        where schemaname = 'mother_api'
            and indexname in (
                'api_key_policy_pkey',
                'api_key_usage_daily_pkey'
            )
        order by indexname
        "#,
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    assert_eq!(
        tables,
        vec![
            "api_key_policy".to_string(),
            "api_key_usage_daily".to_string()
        ]
    );
    assert_eq!(constraints.len(), 9);
    assert_eq!(
        indexes,
        vec![
            "api_key_policy_pkey".to_string(),
            "api_key_usage_daily_pkey".to_string()
        ]
    );
}

#[tokio::test]
async fn api_key_adoption_migrations_and_reference_data_create_no_real_keys() {
    let Some(pool) = migrated_pool().await else {
        return;
    };

    crate::reference_data::apply_embedded_catalog(&pool)
        .await
        .unwrap();

    let consumer_count =
        sqlx::query_scalar::<_, i64>("select count(*) from mother_api.api_consumer")
            .fetch_one(&pool)
            .await
            .unwrap();
    let key_count = sqlx::query_scalar::<_, i64>("select count(*) from mother_api.api_key")
        .fetch_one(&pool)
        .await
        .unwrap();
    let policy_count =
        sqlx::query_scalar::<_, i64>("select count(*) from mother_api.api_key_policy")
            .fetch_one(&pool)
            .await
            .unwrap();
    let usage_count =
        sqlx::query_scalar::<_, i64>("select count(*) from mother_api.api_key_usage_daily")
            .fetch_one(&pool)
            .await
            .unwrap();

    assert_eq!(consumer_count, 0);
    assert_eq!(key_count, 0);
    assert_eq!(policy_count, 0);
    assert_eq!(usage_count, 0);
}

#[tokio::test]
async fn valid_placeholder_api_consumer_and_key_can_be_inserted() {
    let Some(pool) = migrated_pool().await else {
        return;
    };

    let mut transaction = pool.begin().await.unwrap();
    let consumer_id = insert_api_consumer(&mut transaction, "test-consumer").await;
    let key_id = insert_api_key(&mut transaction, consumer_id, "test_prefix", vec![1_u8; 32]).await;
    sqlx::query(
        r#"
        insert into mother_api.api_key_policy (
            api_key_id,
            requests_per_minute,
            requests_per_day
        )
        values ($1, 0, 0)
        "#,
    )
    .bind(key_id)
    .execute(&mut *transaction)
    .await
    .unwrap();
    sqlx::query(
        r#"
        insert into mother_api.api_key_usage_daily (
            api_key_id,
            usage_date
        )
        values ($1, (now() at time zone 'utc')::date)
        "#,
    )
    .bind(key_id)
    .execute(&mut *transaction)
    .await
    .unwrap();

    assert_ne!(consumer_id, uuid::Uuid::nil());
    assert_ne!(key_id, uuid::Uuid::nil());

    transaction.rollback().await.unwrap();
}

#[tokio::test]
async fn api_consumer_slug_is_unique_and_normalized() {
    let Some(pool) = migrated_pool().await else {
        return;
    };

    let mut duplicate_transaction = pool.begin().await.unwrap();
    insert_api_consumer(&mut duplicate_transaction, "duplicate-consumer").await;
    let duplicate_result = sqlx::query(
        r#"
        insert into mother_api.api_consumer (
            slug,
            display_name,
            category,
            status
        )
        values ('duplicate-consumer', 'Duplicate Consumer', 'friend', 'active')
        "#,
    )
    .execute(&mut *duplicate_transaction)
    .await;

    let error = duplicate_result.expect_err("duplicate consumer slug should fail");
    assert_database_constraint(error, "api_consumer_slug_unique");
    duplicate_transaction.rollback().await.unwrap();

    let mut normalized_transaction = pool.begin().await.unwrap();
    let normalized_result = sqlx::query(
        r#"
        insert into mother_api.api_consumer (
            slug,
            display_name,
            category,
            status
        )
        values ('Bad_Consumer', 'Bad Consumer', 'friend', 'active')
        "#,
    )
    .execute(&mut *normalized_transaction)
    .await;

    let error = normalized_result.expect_err("non-normalized consumer slug should fail");
    assert_database_constraint(error, "api_consumer_slug_normalized");
    normalized_transaction.rollback().await.unwrap();
}

#[tokio::test]
async fn api_consumer_category_status_and_metadata_are_constrained() {
    let Some(pool) = migrated_pool().await else {
        return;
    };

    let mut display_name_transaction = pool.begin().await.unwrap();
    let display_name_result = sqlx::query(
        r#"
        insert into mother_api.api_consumer (
            slug,
            display_name,
            category,
            status
        )
        values ('blank-display-name-consumer', '   ', 'friend', 'active')
        "#,
    )
    .execute(&mut *display_name_transaction)
    .await;
    let error = display_name_result.expect_err("blank consumer display_name should fail");
    assert_database_constraint(error, "api_consumer_display_name_non_empty");
    display_name_transaction.rollback().await.unwrap();

    let mut category_transaction = pool.begin().await.unwrap();
    let category_result = sqlx::query(
        r#"
        insert into mother_api.api_consumer (
            slug,
            display_name,
            category,
            status
        )
        values ('bad-category-consumer', 'Bad Category Consumer', 'enterprise', 'active')
        "#,
    )
    .execute(&mut *category_transaction)
    .await;
    let error = category_result.expect_err("invalid consumer category should fail");
    assert_database_constraint(error, "api_consumer_category_known");
    category_transaction.rollback().await.unwrap();

    let mut status_transaction = pool.begin().await.unwrap();
    let status_result = sqlx::query(
        r#"
        insert into mother_api.api_consumer (
            slug,
            display_name,
            category,
            status
        )
        values ('bad-status-consumer', 'Bad Status Consumer', 'friend', 'pending')
        "#,
    )
    .execute(&mut *status_transaction)
    .await;
    let error = status_result.expect_err("invalid consumer status should fail");
    assert_database_constraint(error, "api_consumer_status_known");
    status_transaction.rollback().await.unwrap();

    let mut metadata_transaction = pool.begin().await.unwrap();
    let metadata_result = sqlx::query(
        r#"
        insert into mother_api.api_consumer (
            slug,
            display_name,
            category,
            status,
            metadata
        )
        values ('bad-metadata-consumer', 'Bad Metadata Consumer', 'friend', 'active', $1)
        "#,
    )
    .bind(json!(["not", "an", "object"]))
    .execute(&mut *metadata_transaction)
    .await;
    let error = metadata_result.expect_err("non-object consumer metadata should fail");
    assert_database_constraint(error, "api_consumer_metadata_object");
    metadata_transaction.rollback().await.unwrap();

    let mut timestamp_transaction = pool.begin().await.unwrap();
    let timestamp_result = sqlx::query(
        r#"
        insert into mother_api.api_consumer (
            slug,
            display_name,
            category,
            status,
            created_at,
            updated_at
        )
        values (
            'bad-consumer-timestamps',
            'Bad Consumer Timestamps',
            'friend',
            'active',
            '2026-01-01T00:00:00Z',
            '2025-12-31T00:00:00Z'
        )
        "#,
    )
    .execute(&mut *timestamp_transaction)
    .await;
    let error = timestamp_result.expect_err("consumer updated_at before created_at should fail");
    assert_database_constraint(error, "api_consumer_timestamps_sane");
    timestamp_transaction.rollback().await.unwrap();
}

#[tokio::test]
async fn api_key_prefix_hash_and_status_are_constrained() {
    let Some(pool) = migrated_pool().await else {
        return;
    };

    let mut duplicate_prefix_transaction = pool.begin().await.unwrap();
    let consumer_id = insert_api_consumer(
        &mut duplicate_prefix_transaction,
        "duplicate-prefix-consumer",
    )
    .await;
    insert_api_key(
        &mut duplicate_prefix_transaction,
        consumer_id,
        "duplicate_prefix",
        vec![1_u8; 32],
    )
    .await;
    let duplicate_prefix_result = sqlx::query(
        r#"
        insert into mother_api.api_key (
            consumer_id,
            label,
            key_prefix,
            key_hash
        )
        values ($1, 'Duplicate Prefix Key', 'duplicate_prefix', $2)
        "#,
    )
    .bind(consumer_id)
    .bind(vec![2_u8; 32])
    .execute(&mut *duplicate_prefix_transaction)
    .await;
    let error = duplicate_prefix_result.expect_err("duplicate key prefix should fail");
    assert_database_constraint(error, "api_key_key_prefix_unique");
    duplicate_prefix_transaction.rollback().await.unwrap();

    let mut duplicate_hash_transaction = pool.begin().await.unwrap();
    let consumer_id =
        insert_api_consumer(&mut duplicate_hash_transaction, "duplicate-hash-consumer").await;
    insert_api_key(
        &mut duplicate_hash_transaction,
        consumer_id,
        "first_hash_prefix",
        vec![3_u8; 32],
    )
    .await;
    let duplicate_hash_result = sqlx::query(
        r#"
        insert into mother_api.api_key (
            consumer_id,
            label,
            key_prefix,
            key_hash
        )
        values ($1, 'Duplicate Hash Key', 'second_hash_prefix', $2)
        "#,
    )
    .bind(consumer_id)
    .bind(vec![3_u8; 32])
    .execute(&mut *duplicate_hash_transaction)
    .await;
    let error = duplicate_hash_result.expect_err("duplicate key hash should fail");
    assert_database_constraint(error, "api_key_key_hash_unique");
    duplicate_hash_transaction.rollback().await.unwrap();

    let mut prefix_transaction = pool.begin().await.unwrap();
    let consumer_id = insert_api_consumer(&mut prefix_transaction, "bad-prefix-consumer").await;
    let prefix_result = sqlx::query(
        r#"
        insert into mother_api.api_key (
            consumer_id,
            label,
            key_prefix,
            key_hash
        )
        values ($1, 'Bad Prefix Key', 'Bad-Prefix', $2)
        "#,
    )
    .bind(consumer_id)
    .bind(vec![4_u8; 32])
    .execute(&mut *prefix_transaction)
    .await;
    let error = prefix_result.expect_err("non-normalized key prefix should fail");
    assert_database_constraint(error, "api_key_prefix_normalized");
    prefix_transaction.rollback().await.unwrap();

    let mut status_transaction = pool.begin().await.unwrap();
    let consumer_id = insert_api_consumer(&mut status_transaction, "bad-key-status-consumer").await;
    let status_result = sqlx::query(
        r#"
        insert into mother_api.api_key (
            consumer_id,
            label,
            key_prefix,
            key_hash,
            status
        )
        values ($1, 'Bad Status Key', 'bad_status_prefix', $2, 'pending')
        "#,
    )
    .bind(consumer_id)
    .bind(vec![5_u8; 32])
    .execute(&mut *status_transaction)
    .await;
    let error = status_result.expect_err("invalid key status should fail");
    assert_database_constraint(error, "api_key_status_known");
    status_transaction.rollback().await.unwrap();
}

#[tokio::test]
async fn api_key_hash_algorithm_length_and_metadata_are_constrained() {
    let Some(pool) = migrated_pool().await else {
        return;
    };

    let mut label_transaction = pool.begin().await.unwrap();
    let consumer_id = insert_api_consumer(&mut label_transaction, "blank-key-label-consumer").await;
    let label_result = sqlx::query(
        r#"
        insert into mother_api.api_key (
            consumer_id,
            label,
            key_prefix,
            key_hash
        )
        values ($1, '   ', 'blank_label_prefix', $2)
        "#,
    )
    .bind(consumer_id)
    .bind(vec![14_u8; 32])
    .execute(&mut *label_transaction)
    .await;
    let error = label_result.expect_err("blank key label should fail");
    assert_database_constraint(error, "api_key_label_non_empty");
    label_transaction.rollback().await.unwrap();

    let mut algorithm_transaction = pool.begin().await.unwrap();
    let consumer_id =
        insert_api_consumer(&mut algorithm_transaction, "bad-algorithm-consumer").await;
    let algorithm_result = sqlx::query(
        r#"
        insert into mother_api.api_key (
            consumer_id,
            label,
            key_prefix,
            key_hash,
            hash_algorithm
        )
        values ($1, 'Bad Algorithm Key', 'bad_algorithm_prefix', $2, 'bcrypt')
        "#,
    )
    .bind(consumer_id)
    .bind(vec![6_u8; 32])
    .execute(&mut *algorithm_transaction)
    .await;
    let error = algorithm_result.expect_err("invalid hash algorithm should fail");
    assert_database_constraint(error, "api_key_hash_algorithm_known");
    algorithm_transaction.rollback().await.unwrap();

    let mut hash_length_transaction = pool.begin().await.unwrap();
    let consumer_id =
        insert_api_consumer(&mut hash_length_transaction, "bad-hash-length-consumer").await;
    let hash_length_result = sqlx::query(
        r#"
        insert into mother_api.api_key (
            consumer_id,
            label,
            key_prefix,
            key_hash
        )
        values ($1, 'Bad Hash Length Key', 'bad_hash_length_prefix', $2)
        "#,
    )
    .bind(consumer_id)
    .bind(vec![7_u8; 31])
    .execute(&mut *hash_length_transaction)
    .await;
    let error = hash_length_result.expect_err("short key hash should fail");
    assert_database_constraint(error, "api_key_hash_sha256_length");
    hash_length_transaction.rollback().await.unwrap();

    let mut metadata_transaction = pool.begin().await.unwrap();
    let consumer_id =
        insert_api_consumer(&mut metadata_transaction, "bad-key-metadata-consumer").await;
    let metadata_result = sqlx::query(
        r#"
        insert into mother_api.api_key (
            consumer_id,
            label,
            key_prefix,
            key_hash,
            metadata
        )
        values ($1, 'Bad Metadata Key', 'bad_metadata_prefix', $2, $3)
        "#,
    )
    .bind(consumer_id)
    .bind(vec![8_u8; 32])
    .bind(json!(["not", "an", "object"]))
    .execute(&mut *metadata_transaction)
    .await;
    let error = metadata_result.expect_err("non-object key metadata should fail");
    assert_database_constraint(error, "api_key_metadata_object");
    metadata_transaction.rollback().await.unwrap();
}

#[tokio::test]
async fn api_consumer_delete_is_restricted_when_keys_exist() {
    let Some(pool) = migrated_pool().await else {
        return;
    };

    let mut transaction = pool.begin().await.unwrap();
    let consumer_id = insert_api_consumer(&mut transaction, "restricted-delete-consumer").await;
    insert_api_key(
        &mut transaction,
        consumer_id,
        "restricted_delete_prefix",
        vec![9_u8; 32],
    )
    .await;

    let delete_result = sqlx::query("delete from mother_api.api_consumer where id = $1")
        .bind(consumer_id)
        .execute(&mut *transaction)
        .await;

    let error = delete_result.expect_err("consumer delete should be restricted while keys exist");
    assert_database_constraint(error, "api_key_consumer_id_fkey");

    transaction.rollback().await.unwrap();
}

#[tokio::test]
async fn api_key_timestamp_sanity_is_enforced() {
    let Some(pool) = migrated_pool().await else {
        return;
    };

    let mut expires_transaction = pool.begin().await.unwrap();
    let consumer_id =
        insert_api_consumer(&mut expires_transaction, "bad-expires-at-consumer").await;
    let expires_result = sqlx::query(
        r#"
        insert into mother_api.api_key (
            consumer_id,
            label,
            key_prefix,
            key_hash,
            created_at,
            expires_at
        )
        values (
            $1,
            'Bad Expires Key',
            'bad_expires_prefix',
            $2,
            '2026-01-01T00:00:00Z',
            '2025-12-31T00:00:00Z'
        )
        "#,
    )
    .bind(consumer_id)
    .bind(vec![10_u8; 32])
    .execute(&mut *expires_transaction)
    .await;
    let error = expires_result.expect_err("expires_at before created_at should fail");
    assert_database_constraint(error, "api_key_timestamps_sane");
    expires_transaction.rollback().await.unwrap();

    let mut revoked_transaction = pool.begin().await.unwrap();
    let consumer_id =
        insert_api_consumer(&mut revoked_transaction, "missing-revoked-at-consumer").await;
    let revoked_result = sqlx::query(
        r#"
        insert into mother_api.api_key (
            consumer_id,
            label,
            key_prefix,
            key_hash,
            status
        )
        values ($1, 'Missing Revoked At Key', 'missing_revoked_prefix', $2, 'revoked')
        "#,
    )
    .bind(consumer_id)
    .bind(vec![11_u8; 32])
    .execute(&mut *revoked_transaction)
    .await;
    let error = revoked_result.expect_err("revoked key without revoked_at should fail");
    assert_database_constraint(error, "api_key_revoked_at_matches_status");
    revoked_transaction.rollback().await.unwrap();

    let mut revoked_timestamp_transaction = pool.begin().await.unwrap();
    let consumer_id = insert_api_consumer(
        &mut revoked_timestamp_transaction,
        "bad-revoked-at-consumer",
    )
    .await;
    let revoked_timestamp_result = sqlx::query(
        r#"
        insert into mother_api.api_key (
            consumer_id,
            label,
            key_prefix,
            key_hash,
            status,
            created_at,
            revoked_at
        )
        values (
            $1,
            'Bad Revoked At Key',
            'bad_revoked_at_prefix',
            $2,
            'revoked',
            '2026-01-01T00:00:00Z',
            '2025-12-31T00:00:00Z'
        )
        "#,
    )
    .bind(consumer_id)
    .bind(vec![13_u8; 32])
    .execute(&mut *revoked_timestamp_transaction)
    .await;
    let error = revoked_timestamp_result.expect_err("revoked_at before created_at should fail");
    assert_database_constraint(error, "api_key_timestamps_sane");
    revoked_timestamp_transaction.rollback().await.unwrap();

    let mut last_used_transaction = pool.begin().await.unwrap();
    let consumer_id =
        insert_api_consumer(&mut last_used_transaction, "bad-last-used-at-consumer").await;
    let last_used_result = sqlx::query(
        r#"
        insert into mother_api.api_key (
            consumer_id,
            label,
            key_prefix,
            key_hash,
            created_at,
            last_used_at
        )
        values (
            $1,
            'Bad Last Used Key',
            'bad_last_used_prefix',
            $2,
            '2026-01-01T00:00:00Z',
            '2025-12-31T00:00:00Z'
        )
        "#,
    )
    .bind(consumer_id)
    .bind(vec![12_u8; 32])
    .execute(&mut *last_used_transaction)
    .await;
    let error = last_used_result.expect_err("last_used_at before created_at should fail");
    assert_database_constraint(error, "api_key_timestamps_sane");
    last_used_transaction.rollback().await.unwrap();

    let mut updated_transaction = pool.begin().await.unwrap();
    let consumer_id =
        insert_api_consumer(&mut updated_transaction, "bad-key-updated-at-consumer").await;
    let updated_result = sqlx::query(
        r#"
        insert into mother_api.api_key (
            consumer_id,
            label,
            key_prefix,
            key_hash,
            created_at,
            updated_at
        )
        values (
            $1,
            'Bad Updated At Key',
            'bad_updated_at_prefix',
            $2,
            '2026-01-01T00:00:00Z',
            '2025-12-31T00:00:00Z'
        )
        "#,
    )
    .bind(consumer_id)
    .bind(vec![15_u8; 32])
    .execute(&mut *updated_transaction)
    .await;
    let error = updated_result.expect_err("key updated_at before created_at should fail");
    assert_database_constraint(error, "api_key_timestamps_sane");
    updated_transaction.rollback().await.unwrap();
}

#[tokio::test]
async fn api_key_policy_limits_and_timestamps_are_constrained() {
    let Some(pool) = migrated_pool().await else {
        return;
    };

    let mut zero_limit_transaction = pool.begin().await.unwrap();
    let consumer_id =
        insert_api_consumer(&mut zero_limit_transaction, "zero-policy-consumer").await;
    let key_id = insert_api_key(
        &mut zero_limit_transaction,
        consumer_id,
        "zero_policy_prefix",
        vec![16_u8; 32],
    )
    .await;
    sqlx::query(
        r#"
        insert into mother_api.api_key_policy (
            api_key_id,
            requests_per_minute,
            requests_per_day
        )
        values ($1, 0, 0)
        "#,
    )
    .bind(key_id)
    .execute(&mut *zero_limit_transaction)
    .await
    .unwrap();
    zero_limit_transaction.rollback().await.unwrap();

    let mut minute_transaction = pool.begin().await.unwrap();
    let consumer_id =
        insert_api_consumer(&mut minute_transaction, "negative-minute-consumer").await;
    let key_id = insert_api_key(
        &mut minute_transaction,
        consumer_id,
        "negative_minute_prefix",
        vec![17_u8; 32],
    )
    .await;
    let minute_result = sqlx::query(
        r#"
        insert into mother_api.api_key_policy (
            api_key_id,
            requests_per_minute,
            requests_per_day
        )
        values ($1, -1, 5000)
        "#,
    )
    .bind(key_id)
    .execute(&mut *minute_transaction)
    .await;
    let error = minute_result.expect_err("negative minute policy should fail");
    assert_database_constraint(error, "api_key_policy_requests_per_minute_non_negative");
    minute_transaction.rollback().await.unwrap();

    let mut day_transaction = pool.begin().await.unwrap();
    let consumer_id = insert_api_consumer(&mut day_transaction, "negative-day-consumer").await;
    let key_id = insert_api_key(
        &mut day_transaction,
        consumer_id,
        "negative_day_prefix",
        vec![18_u8; 32],
    )
    .await;
    let day_result = sqlx::query(
        r#"
        insert into mother_api.api_key_policy (
            api_key_id,
            requests_per_minute,
            requests_per_day
        )
        values ($1, 60, -1)
        "#,
    )
    .bind(key_id)
    .execute(&mut *day_transaction)
    .await;
    let error = day_result.expect_err("negative day policy should fail");
    assert_database_constraint(error, "api_key_policy_requests_per_day_non_negative");
    day_transaction.rollback().await.unwrap();

    let mut timestamp_transaction = pool.begin().await.unwrap();
    let consumer_id =
        insert_api_consumer(&mut timestamp_transaction, "bad-policy-timestamp-consumer").await;
    let key_id = insert_api_key(
        &mut timestamp_transaction,
        consumer_id,
        "bad_policy_timestamp_prefix",
        vec![19_u8; 32],
    )
    .await;
    let timestamp_result = sqlx::query(
        r#"
        insert into mother_api.api_key_policy (
            api_key_id,
            created_at,
            updated_at
        )
        values (
            $1,
            '2026-01-01T00:00:00Z',
            '2025-12-31T00:00:00Z'
        )
        "#,
    )
    .bind(key_id)
    .execute(&mut *timestamp_transaction)
    .await;
    let error = timestamp_result.expect_err("policy updated_at before created_at should fail");
    assert_database_constraint(error, "api_key_policy_timestamps_sane");
    timestamp_transaction.rollback().await.unwrap();
}

#[tokio::test]
async fn api_key_usage_daily_counts_dates_and_timestamps_are_constrained() {
    let Some(pool) = migrated_pool().await else {
        return;
    };

    let mut duplicate_transaction = pool.begin().await.unwrap();
    let consumer_id =
        insert_api_consumer(&mut duplicate_transaction, "duplicate-usage-consumer").await;
    let key_id = insert_api_key(
        &mut duplicate_transaction,
        consumer_id,
        "duplicate_usage_prefix",
        vec![20_u8; 32],
    )
    .await;
    sqlx::query(
        r#"
        insert into mother_api.api_key_usage_daily (
            api_key_id,
            usage_date
        )
        values ($1, '2026-07-01')
        "#,
    )
    .bind(key_id)
    .execute(&mut *duplicate_transaction)
    .await
    .unwrap();
    let duplicate_result = sqlx::query(
        r#"
        insert into mother_api.api_key_usage_daily (
            api_key_id,
            usage_date
        )
        values ($1, '2026-07-01')
        "#,
    )
    .bind(key_id)
    .execute(&mut *duplicate_transaction)
    .await;
    let error = duplicate_result.expect_err("duplicate daily usage row should fail");
    assert_database_constraint(error, "api_key_usage_daily_pkey");
    duplicate_transaction.rollback().await.unwrap();

    let mut count_transaction = pool.begin().await.unwrap();
    let consumer_id = insert_api_consumer(&mut count_transaction, "negative-usage-consumer").await;
    let key_id = insert_api_key(
        &mut count_transaction,
        consumer_id,
        "negative_usage_prefix",
        vec![21_u8; 32],
    )
    .await;
    let count_result = sqlx::query(
        r#"
        insert into mother_api.api_key_usage_daily (
            api_key_id,
            usage_date,
            accepted_requests
        )
        values ($1, '2026-07-01', -1)
        "#,
    )
    .bind(key_id)
    .execute(&mut *count_transaction)
    .await;
    let error = count_result.expect_err("negative usage counter should fail");
    assert_database_constraint(error, "api_key_usage_daily_counts_non_negative");
    count_transaction.rollback().await.unwrap();

    let mut timestamp_transaction = pool.begin().await.unwrap();
    let consumer_id =
        insert_api_consumer(&mut timestamp_transaction, "bad-usage-timestamp-consumer").await;
    let key_id = insert_api_key(
        &mut timestamp_transaction,
        consumer_id,
        "bad_usage_timestamp_prefix",
        vec![22_u8; 32],
    )
    .await;
    let timestamp_result = sqlx::query(
        r#"
        insert into mother_api.api_key_usage_daily (
            api_key_id,
            usage_date,
            created_at,
            updated_at,
            last_used_at
        )
        values (
            $1,
            '2026-07-01',
            '2026-01-01T00:00:00Z',
            '2025-12-31T00:00:00Z',
            '2025-12-31T00:00:00Z'
        )
        "#,
    )
    .bind(key_id)
    .execute(&mut *timestamp_transaction)
    .await;
    let error = timestamp_result.expect_err("invalid usage timestamps should fail");
    assert_database_constraint(error, "api_key_usage_daily_timestamps_sane");
    timestamp_transaction.rollback().await.unwrap();
}

#[tokio::test]
async fn api_key_delete_cascades_policy_and_usage_rows() {
    let Some(pool) = migrated_pool().await else {
        return;
    };

    let mut transaction = pool.begin().await.unwrap();
    let consumer_id = insert_api_consumer(&mut transaction, "api-key-cascade-consumer").await;
    let key_id = insert_api_key(
        &mut transaction,
        consumer_id,
        "api_key_cascade_prefix",
        vec![23_u8; 32],
    )
    .await;

    sqlx::query("insert into mother_api.api_key_policy (api_key_id) values ($1)")
        .bind(key_id)
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query(
        r#"
        insert into mother_api.api_key_usage_daily (
            api_key_id,
            usage_date,
            accepted_requests
        )
        values ($1, (now() at time zone 'utc')::date, 1)
        "#,
    )
    .bind(key_id)
    .execute(&mut *transaction)
    .await
    .unwrap();

    sqlx::query("delete from mother_api.api_key where id = $1")
        .bind(key_id)
        .execute(&mut *transaction)
        .await
        .unwrap();

    let policy_count = sqlx::query_scalar::<_, i64>(
        "select count(*) from mother_api.api_key_policy where api_key_id = $1",
    )
    .bind(key_id)
    .fetch_one(&mut *transaction)
    .await
    .unwrap();
    let usage_count = sqlx::query_scalar::<_, i64>(
        "select count(*) from mother_api.api_key_usage_daily where api_key_id = $1",
    )
    .bind(key_id)
    .fetch_one(&mut *transaction)
    .await
    .unwrap();

    assert_eq!(policy_count, 0);
    assert_eq!(usage_count, 0);

    transaction.rollback().await.unwrap();
}

#[tokio::test]
async fn api_key_repository_finds_key_by_exact_prefix_and_hash() {
    let Some(pool) = migrated_pool().await else {
        return;
    };

    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let key_hash = vec![24_u8; 32];
    let (consumer_slug, key_prefix, consumer_id, key_id) =
        insert_repository_test_key(&pool, &suffix, key_hash.clone()).await;
    let repository = ApiKeyRepository::database(pool.clone());

    let lookup = repository
        .find_key_by_prefix_and_hash(&key_prefix, &key_hash)
        .await
        .unwrap()
        .expect("exact prefix and hash should find key");
    let wrong_hash = repository
        .find_key_by_prefix_and_hash(&key_prefix, &[25_u8; 32])
        .await
        .unwrap();
    let wrong_prefix = repository
        .find_key_by_prefix_and_hash("missing_prefix", &key_hash)
        .await
        .unwrap();

    assert_eq!(lookup.api_key_id, key_id);
    assert_eq!(lookup.consumer_id, consumer_id);
    assert_eq!(lookup.consumer_slug, consumer_slug);
    assert_eq!(lookup.consumer_category, "partner");
    assert_eq!(lookup.consumer_status, "active");
    assert_eq!(lookup.key_prefix, key_prefix);
    assert_eq!(lookup.key_label, "Repository Test Key");
    assert_eq!(lookup.key_status, "active");
    assert_eq!(lookup.hash_algorithm, "sha256");
    assert_eq!(lookup.expires_at, None);
    assert!(!lookup.is_expired);
    assert_eq!(wrong_hash, None);
    assert_eq!(wrong_prefix, None);

    delete_api_consumer_test_rows(&pool, &consumer_slug).await;
}

#[tokio::test]
async fn api_key_repository_creates_and_finds_policy() {
    let Some(pool) = migrated_pool().await else {
        return;
    };

    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let (consumer_slug, _key_prefix, _consumer_id, key_id) =
        insert_repository_test_key(&pool, &suffix, vec![26_u8; 32]).await;
    let repository = ApiKeyRepository::database(pool.clone());

    let created = repository.create_policy(key_id, 12, 345).await.unwrap();
    let found = repository
        .find_policy(key_id)
        .await
        .unwrap()
        .expect("created policy should be found");

    assert_eq!(created.api_key_id, key_id);
    assert_eq!(created.requests_per_minute, 12);
    assert_eq!(created.requests_per_day, 345);
    assert_eq!(found, created);
    assert_eq!(
        repository.find_policy(uuid::Uuid::new_v4()).await.unwrap(),
        None
    );

    delete_api_consumer_test_rows(&pool, &consumer_slug).await;
}

#[tokio::test]
async fn api_key_repository_revokes_by_prefix_idempotently() {
    let Some(pool) = migrated_pool().await else {
        return;
    };

    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let (consumer_slug, key_prefix, _consumer_id, key_id) =
        insert_repository_test_key(&pool, &suffix, vec![27_u8; 32]).await;
    let repository = ApiKeyRepository::database(pool.clone());

    let first = repository
        .revoke_by_prefix(&key_prefix)
        .await
        .unwrap()
        .expect("key should be revoked");
    let second = repository
        .revoke_by_prefix(&key_prefix)
        .await
        .unwrap()
        .expect("already revoked key should still be returned");
    let missing = repository
        .revoke_by_prefix("missing_revocation_prefix")
        .await
        .unwrap();

    assert_eq!(first.api_key_id, key_id);
    assert_eq!(first.key_prefix, key_prefix);
    assert_eq!(first.status, "revoked");
    assert_eq!(first.revoked_at, second.revoked_at);
    assert_eq!(missing, None);

    delete_api_consumer_test_rows(&pool, &consumer_slug).await;
}

#[tokio::test]
async fn api_key_repository_updates_last_used() {
    let Some(pool) = migrated_pool().await else {
        return;
    };

    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let (consumer_slug, _key_prefix, _consumer_id, key_id) =
        insert_repository_test_key(&pool, &suffix, vec![28_u8; 32]).await;
    let repository = ApiKeyRepository::database(pool.clone());

    assert!(repository.update_last_used(key_id).await.unwrap());
    assert!(!repository
        .update_last_used(uuid::Uuid::new_v4())
        .await
        .unwrap());

    let last_used_exists = sqlx::query_scalar::<_, bool>(
        "select last_used_at is not null from mother_api.api_key where id = $1",
    )
    .bind(key_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert!(last_used_exists);

    delete_api_consumer_test_rows(&pool, &consumer_slug).await;
}

#[tokio::test]
async fn api_key_repository_increments_daily_accepted_with_limit() {
    let Some(pool) = migrated_pool().await else {
        return;
    };

    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let (consumer_slug, _key_prefix, _consumer_id, key_id) =
        insert_repository_test_key(&pool, &suffix, vec![29_u8; 32]).await;
    let repository = ApiKeyRepository::database(pool.clone());

    assert_eq!(
        repository.increment_daily_accepted(key_id).await.unwrap(),
        DailyAcceptedOutcome::MissingPolicy
    );

    repository.create_policy(key_id, 60, 2).await.unwrap();

    assert_eq!(
        repository.increment_daily_accepted(key_id).await.unwrap(),
        DailyAcceptedOutcome::Accepted
    );
    assert_eq!(
        repository.increment_daily_accepted(key_id).await.unwrap(),
        DailyAcceptedOutcome::Accepted
    );
    assert_eq!(
        repository.increment_daily_accepted(key_id).await.unwrap(),
        DailyAcceptedOutcome::LimitExceeded
    );

    let accepted_requests = sqlx::query_scalar::<_, i64>(
        r#"
        select accepted_requests
        from mother_api.api_key_usage_daily
        where api_key_id = $1
            and usage_date = (now() at time zone 'utc')::date
        "#,
    )
    .bind(key_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(accepted_requests, 2);

    delete_api_consumer_test_rows(&pool, &consumer_slug).await;
}

#[tokio::test]
async fn api_key_repository_allows_only_one_concurrent_daily_accept_at_limit_one() {
    let Some(pool) = migrated_pool().await else {
        return;
    };

    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let (consumer_slug, _key_prefix, _consumer_id, key_id) =
        insert_repository_test_key(&pool, &suffix, vec![30_u8; 32]).await;
    let repository = ApiKeyRepository::database(pool.clone());
    repository.create_policy(key_id, 60, 1).await.unwrap();

    let repository_a = repository.clone();
    let repository_b = repository.clone();
    let (first, second) = tokio::join!(
        async move { repository_a.increment_daily_accepted(key_id).await.unwrap() },
        async move { repository_b.increment_daily_accepted(key_id).await.unwrap() }
    );
    let mut outcomes = vec![first, second];
    outcomes.sort_by_key(|outcome| match outcome {
        DailyAcceptedOutcome::Accepted => 0,
        DailyAcceptedOutcome::LimitExceeded => 1,
        DailyAcceptedOutcome::MissingPolicy => 2,
    });

    assert_eq!(
        outcomes,
        vec![
            DailyAcceptedOutcome::Accepted,
            DailyAcceptedOutcome::LimitExceeded
        ]
    );

    let accepted_requests = sqlx::query_scalar::<_, i64>(
        r#"
        select accepted_requests
        from mother_api.api_key_usage_daily
        where api_key_id = $1
            and usage_date = (now() at time zone 'utc')::date
        "#,
    )
    .bind(key_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(accepted_requests, 1);

    delete_api_consumer_test_rows(&pool, &consumer_slug).await;
}

#[tokio::test]
async fn api_key_repository_daily_limit_zero_accepts_no_requests() {
    let Some(pool) = migrated_pool().await else {
        return;
    };

    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let (consumer_slug, _key_prefix, _consumer_id, key_id) =
        insert_repository_test_key(&pool, &suffix, vec![31_u8; 32]).await;
    let repository = ApiKeyRepository::database(pool.clone());
    repository.create_policy(key_id, 60, 0).await.unwrap();

    assert_eq!(
        repository.increment_daily_accepted(key_id).await.unwrap(),
        DailyAcceptedOutcome::LimitExceeded
    );

    let usage_count = sqlx::query_scalar::<_, i64>(
        "select count(*) from mother_api.api_key_usage_daily where api_key_id = $1",
    )
    .bind(key_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(usage_count, 0);

    delete_api_consumer_test_rows(&pool, &consumer_slug).await;
}

#[tokio::test]
async fn api_key_repository_increments_rate_limited_and_response_counters() {
    let Some(pool) = migrated_pool().await else {
        return;
    };

    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let (consumer_slug, _key_prefix, _consumer_id, key_id) =
        insert_repository_test_key(&pool, &suffix, vec![32_u8; 32]).await;
    let repository = ApiKeyRepository::database(pool.clone());

    repository
        .increment_daily_rate_limited(key_id)
        .await
        .unwrap();
    repository
        .increment_daily_response(key_id, UsageResponseClass::Successful)
        .await
        .unwrap();
    repository
        .increment_daily_response(key_id, UsageResponseClass::ClientError)
        .await
        .unwrap();
    repository
        .increment_daily_response(key_id, UsageResponseClass::ServerError)
        .await
        .unwrap();

    let counters = sqlx::query_as::<_, (i64, i64, i64, i64, i64)>(
        r#"
        select
            accepted_requests,
            rate_limited_requests,
            successful_responses,
            client_error_responses,
            server_error_responses
        from mother_api.api_key_usage_daily
        where api_key_id = $1
            and usage_date = (now() at time zone 'utc')::date
        "#,
    )
    .bind(key_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(counters, (0, 1, 1, 1, 1));

    delete_api_consumer_test_rows(&pool, &consumer_slug).await;
}
