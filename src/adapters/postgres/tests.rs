use crate::test_utils::postgres::migrated_pool;
use serde_json::json;
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

    assert_eq!(consumer_count, 0);
    assert_eq!(key_count, 0);
}

#[tokio::test]
async fn valid_placeholder_api_consumer_and_key_can_be_inserted() {
    let Some(pool) = migrated_pool().await else {
        return;
    };

    let mut transaction = pool.begin().await.unwrap();
    let consumer_id = insert_api_consumer(&mut transaction, "test-consumer").await;
    let key_id = insert_api_key(&mut transaction, consumer_id, "test_prefix", vec![1_u8; 32]).await;

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
}
