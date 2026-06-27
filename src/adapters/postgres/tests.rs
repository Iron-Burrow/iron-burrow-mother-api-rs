use sqlx::PgPool;

#[tokio::test]
async fn global_asset_slug_is_unique_in_database() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        return;
    };

    let pool = PgPool::connect(&database_url).await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();

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
    let sqlx::Error::Database(database_error) = error else {
        panic!("expected database error for duplicate slug");
    };

    assert_eq!(
        database_error.constraint(),
        Some("global_asset_slug_unique")
    );

    transaction.rollback().await.unwrap();
}

#[tokio::test]
async fn global_asset_slug_is_normalized_in_database() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        return;
    };

    let pool = PgPool::connect(&database_url).await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();

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
    let sqlx::Error::Database(database_error) = error else {
        panic!("expected database error for non-normalized slug");
    };

    assert_eq!(
        database_error.constraint(),
        Some("global_asset_slug_normalized")
    );

    transaction.rollback().await.unwrap();
}

#[tokio::test]
async fn full_migration_uses_canonical_evm_network_slugs() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        return;
    };

    let pool = PgPool::connect(&database_url).await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();

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
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        return;
    };

    let pool = PgPool::connect(&database_url).await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
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
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        return;
    };

    let pool = PgPool::connect(&database_url).await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let mut transaction = pool.begin().await.unwrap();

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
