use sqlx::PgPool;

const IDENTITY_CONSTRAINTS_MIGRATION_SQL: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/migrations/0006_reference_data_identity_constraints.sql"
));

async fn migrated_pool() -> Option<PgPool> {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        return None;
    };

    let pool = PgPool::connect(&database_url).await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();

    Some(pool)
}

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
