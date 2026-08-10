use sqlx::{PgPool, Postgres, Transaction};

use crate::domain::{
    canonical_registry::{
        embedded_catalog_json, parse_catalog_json as parse_canonical_catalog_json,
        validate_catalog as validate_canonical_catalog, CanonicalRegistryError,
        CapabilityDeclaration, Catalog,
    },
    capabilities::Capability,
};

#[derive(Debug, thiserror::Error)]
pub(crate) enum ReferenceDataError {
    #[error("failed to parse reference-data catalog: {0}")]
    Parse(serde_json::Error),
    #[error("invalid reference-data catalog: {0}")]
    Invalid(String),
    #[error("failed to apply reference-data catalog: {0}")]
    Database(sqlx::Error),
}

pub(crate) async fn apply_embedded_catalog(pool: &PgPool) -> Result<(), ReferenceDataError> {
    let catalog = parse_catalog_json(embedded_catalog_json())?;
    apply_catalog(pool, &catalog).await
}

fn parse_catalog_json(json: &str) -> Result<Catalog, ReferenceDataError> {
    parse_canonical_catalog_json(json).map_err(map_registry_error)
}

async fn apply_catalog(pool: &PgPool, catalog: &Catalog) -> Result<(), ReferenceDataError> {
    validate_catalog(catalog)?;

    let mut transaction = pool.begin().await.map_err(ReferenceDataError::Database)?;

    sqlx::query(
        r#"
        select pg_advisory_xact_lock(
            hashtextextended('mother_api.reference_data', 0)
        )
        "#,
    )
    .execute(&mut *transaction)
    .await
    .map_err(ReferenceDataError::Database)?;

    for capability in &catalog.capabilities {
        upsert_capability(&mut transaction, capability).await?;
    }

    reconcile_legacy_capability_grants(&mut transaction).await?;

    seed_aave_v3_realized_yield_protocol(&mut transaction).await?;

    transaction
        .commit()
        .await
        .map_err(ReferenceDataError::Database)
}

async fn seed_aave_v3_realized_yield_protocol(
    transaction: &mut Transaction<'_, Postgres>,
) -> Result<(), ReferenceDataError> {
    let protocol_id: String = sqlx::query_scalar(
        r#"
        insert into mother_api.defi_protocol (
          slug, network_id, family, adapter_kind, adapter_version, enabled, verified, updated_at
        )
        select 'aave-v3', network.id, 'aave-v3', 'aave_v3_realized_yield', 'v1', true, true, now()
        from mother_api.network network
        where network.slug = 'eth-mainnet' and network.status = 'active'
        on conflict (slug) do update set
          network_id = excluded.network_id,
          family = excluded.family,
          adapter_kind = excluded.adapter_kind,
          adapter_version = excluded.adapter_version,
          enabled = excluded.enabled,
          verified = excluded.verified,
          updated_at = now()
        returning id::text
        "#,
    )
    .fetch_one(&mut **transaction)
    .await
    .map_err(ReferenceDataError::Database)?;

    sqlx::query(
        r#"
        insert into mother_api.defi_protocol_target (
          defi_protocol_id, target_key, target_kind, address, enabled, verified, updated_at
        ) values ($1::uuid, 'pool', 'pool', '0x87870bca3f3fd6335c3f4ce8392d69350b4fa4e2', true, true, now())
        on conflict (defi_protocol_id, target_key) do update set
          target_kind = excluded.target_kind, address = excluded.address,
          enabled = excluded.enabled, verified = excluded.verified, updated_at = now()
        "#,
    )
    .bind(&protocol_id)
    .execute(&mut **transaction)
    .await
    .map_err(ReferenceDataError::Database)?;

    for (slug, address) in [
        ("usdc", "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"),
        ("usdt", "0xdac17f958d2ee523a2206206994597c13d831ec7"),
        ("dai", "0x6b175474e89094c44da98b954eedeac495271d0f"),
        ("gho", "0x40d16fc0246ad3160ccc09b8d0d3a2cd28ae6c2f"),
    ] {
        sqlx::query(
            r#"
            insert into mother_api.defi_protocol_target (
              defi_protocol_id, target_key, target_kind, address, asset_chain_map_id,
              enabled, verified, updated_at
            )
            select $1::uuid, $2, 'reserve', $3, chain_map.id, true, true, now()
            from mother_api.asset_chain_map chain_map
            join mother_api.global_asset asset on asset.id = chain_map.asset_id
            join mother_api.network network on network.id = chain_map.network_id
            where asset.slug = $2 and network.slug = 'eth-mainnet'
              and chain_map.status = 'active' and asset.status = 'active'
              and lower(chain_map.deployment_address) = $3
            on conflict (defi_protocol_id, target_key) do update set
              target_kind = excluded.target_kind, address = excluded.address,
              asset_chain_map_id = excluded.asset_chain_map_id,
              enabled = excluded.enabled, verified = excluded.verified, updated_at = now()
            "#,
        )
        .bind(&protocol_id)
        .bind(slug)
        .bind(address)
        .execute(&mut **transaction)
        .await
        .map_err(ReferenceDataError::Database)?;
    }
    Ok(())
}

fn validate_catalog(catalog: &Catalog) -> Result<(), ReferenceDataError> {
    validate_canonical_catalog(catalog).map_err(map_registry_error)
}

fn map_registry_error(error: CanonicalRegistryError) -> ReferenceDataError {
    match error {
        CanonicalRegistryError::Parse(error) => ReferenceDataError::Parse(error),
        CanonicalRegistryError::Invalid(message) => ReferenceDataError::Invalid(message),
    }
}

async fn upsert_capability(
    transaction: &mut Transaction<'_, Postgres>,
    capability: &CapabilityDeclaration,
) -> Result<(), ReferenceDataError> {
    sqlx::query(
        r#"
        insert into mother_api.capability as existing (id, description)
        values ($1, $2)
        on conflict (id) do update
        set
            description = excluded.description,
            updated_at = now()
        where existing.description is distinct from excluded.description
        "#,
    )
    .bind(&capability.id)
    .bind(&capability.description)
    .execute(&mut **transaction)
    .await
    .map_err(ReferenceDataError::Database)?;

    Ok(())
}

async fn reconcile_legacy_capability_grants(
    transaction: &mut Transaction<'_, Postgres>,
) -> Result<(), ReferenceDataError> {
    for capability in Capability::LEGACY_BASELINE {
        sqlx::query(
            r#"
            insert into mother_api.api_consumer_capability_grant (
                consumer_id, capability_id, network_scope
            )
            select id, $1, '*'
            from mother_api.api_consumer
            on conflict (consumer_id, capability_id, network_scope) do nothing
            "#,
        )
        .bind(capability.id())
        .execute(&mut **transaction)
        .await
        .map_err(ReferenceDataError::Database)?;

        sqlx::query(
            r#"
            insert into mother_api.api_key_capability_grant (
                api_key_id, capability_id, network_scope
            )
            select id, $1, '*'
            from mother_api.api_key
            on conflict (api_key_id, capability_id, network_scope) do nothing
            "#,
        )
        .bind(capability.id())
        .execute(&mut **transaction)
        .await
        .map_err(ReferenceDataError::Database)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::domain::canonical_registry::{
        CanonicalAsset as AssetDeclaration, CanonicalAssetChainMap as AssetChainMapDeclaration,
        CanonicalNetwork as NetworkDeclaration,
    };
    use crate::test_utils::postgres::migrated_pool;

    #[test]
    fn embedded_catalog_parses_and_validates() {
        parse_catalog_json(embedded_catalog_json()).unwrap();
    }

    #[test]
    fn duplicate_asset_slugs_fail_validation() {
        let mut catalog = minimal_catalog("duplicate-asset-slugs");
        catalog.assets.push(catalog.assets[0].clone());

        assert_invalid(catalog, "duplicate asset slug");
    }

    #[test]
    fn duplicate_network_slugs_fail_validation() {
        let mut catalog = minimal_catalog("duplicate-network-slugs");
        catalog.networks.push(catalog.networks[0].clone());

        assert_invalid(catalog, "duplicate network slug");
    }

    #[test]
    fn unresolved_mapping_asset_fails_validation() {
        let mut catalog = minimal_catalog("unresolved-asset");
        catalog.asset_chain_maps[0].asset_slug = "missing-asset".to_string();

        assert_invalid(catalog, "references undeclared asset");
    }

    #[test]
    fn unresolved_mapping_network_fails_validation() {
        let mut catalog = minimal_catalog("unresolved-network");
        catalog.asset_chain_maps[0].network_slug = "missing-network".to_string();

        assert_invalid(catalog, "references undeclared network");
    }

    #[test]
    fn unknown_chain_field_fails_to_parse() {
        let json = r#"
        {
          "version": 2,
          "capabilities": [
            {"id": "balances.read", "description": "Read supported latest and historical balance snapshots."},
            {"id": "transfers.read", "description": "Search bounded ERC-20 transfers."}
          ],
          "assets": [],
          "networks": [{"slug": "eth-mainnet", "name": "Ethereum Mainnet", "family": "evm", "chain": 1, "chain_id": 1, "caip2": "eip155:1", "metadata": {}, "status": "active", "sort_order": 10}],
          "asset_chain_maps": []
        }
        "#;

        assert!(matches!(
            parse_catalog_json(json).unwrap_err(),
            ReferenceDataError::Parse(_)
        ));
    }

    #[test]
    fn invalid_evm_address_fails_validation() {
        let mut catalog = minimal_catalog("invalid-evm-address");
        catalog.asset_chain_maps[0].deployment_address = Some("0xnot-an-address".to_string());

        assert_invalid(catalog, "invalid deployment_address");
    }

    #[test]
    fn native_mapping_with_deployment_address_fails_validation() {
        let mut catalog = minimal_catalog("native-with-address");
        catalog.asset_chain_maps[0].is_native = true;
        catalog.asset_chain_maps[0].token_standard = "native".to_string();

        assert_invalid(catalog, "must not declare deployment_address");
    }

    #[test]
    fn deployed_mapping_without_deployment_address_fails_validation() {
        let mut catalog = minimal_catalog("deployed-without-address");
        catalog.asset_chain_maps[0].deployment_address = None;

        assert_invalid(catalog, "requires deployment_address");
    }

    #[test]
    fn mixed_case_non_erc20_address_fails_validation() {
        let mut catalog = minimal_catalog("mixed-case-nep141");
        catalog.networks[0].family = "near".to_string();
        catalog.networks[0].chain_id = None;
        catalog.networks[0].caip2 = Some("near:mainnet".to_string());
        catalog.asset_chain_maps[0].deployment_address = Some("Token.NEAR".to_string());
        catalog.asset_chain_maps[0].deployment_block = None;
        catalog.asset_chain_maps[0].token_standard = "nep141".to_string();

        assert_invalid(catalog, "requires lowercase deployment_address");
    }

    #[test]
    fn duplicate_active_native_mappings_fail_validation() {
        let mut catalog = minimal_catalog("duplicate-native");
        catalog.assets[0].slug = "native-duplicate-native".to_string();
        catalog.assets[0].canonical_path = "/assets/native-duplicate-native".to_string();
        catalog.assets[0].aliases = vec!["native-duplicate-native".to_string()];
        catalog.asset_chain_maps[0].asset_slug = "native-duplicate-native".to_string();
        catalog.asset_chain_maps[0].is_native = true;
        catalog.asset_chain_maps[0].deployment_address = None;
        catalog.asset_chain_maps[0].deployment_block = None;
        catalog.asset_chain_maps[0].token_standard = "native".to_string();
        catalog.assets.push(asset("other-duplicate-native"));
        catalog.asset_chain_maps.push(AssetChainMapDeclaration {
            asset_slug: "other-duplicate-native".to_string(),
            network_slug: catalog.networks[0].slug.clone(),
            is_native: true,
            deployment_address: None,
            deployment_block: None,
            decimals: Some(18),
            token_standard: "native".to_string(),
            metadata: json!({}),
            status: "active".to_string(),
            sort_order: 20,
        });

        assert_invalid(catalog, "duplicate active native mapping");
    }

    #[test]
    fn duplicate_active_deployment_addresses_fail_validation() {
        let mut catalog = minimal_catalog("duplicate-address");
        catalog.assets.push(asset("other-duplicate-address"));
        catalog.asset_chain_maps.push(AssetChainMapDeclaration {
            asset_slug: "other-duplicate-address".to_string(),
            network_slug: catalog.networks[0].slug.clone(),
            is_native: false,
            deployment_address: catalog.asset_chain_maps[0].deployment_address.clone(),
            deployment_block: Some(2),
            decimals: Some(18),
            token_standard: "erc20".to_string(),
            metadata: json!({}),
            status: "active".to_string(),
            sort_order: 20,
        });

        assert_invalid(catalog, "duplicate active deployment address");
    }

    #[tokio::test]
    async fn apply_reference_reconciles_capabilities_without_touching_catalog_rows() {
        let Some(pool) = migrated_pool().await else {
            return;
        };

        let before = catalog_audit_rows(&pool).await;
        apply_embedded_catalog(&pool).await.unwrap();
        let after = catalog_audit_rows(&pool).await;

        assert_eq!(after, before);
        let capability_count =
            sqlx::query_scalar::<_, i64>("select count(*) from mother_api.capability")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(capability_count, Capability::ALL.len() as i64);
    }

    #[tokio::test]
    async fn apply_reference_leaves_existing_catalog_overrides_unchanged() {
        let Some(pool) = migrated_pool().await else {
            return;
        };
        sqlx::query("update mother_api.global_asset set name = 'Historical override' where slug = 'bitso-mxn'")
            .execute(&pool)
            .await
            .unwrap();

        let before = catalog_audit_rows(&pool).await;
        apply_embedded_catalog(&pool).await.unwrap();
        let after = catalog_audit_rows(&pool).await;
        let name = sqlx::query_scalar::<_, String>(
            "select name from mother_api.global_asset where slug = 'bitso-mxn'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(after, before);
        assert_eq!(name, "Historical override");
    }

    #[tokio::test]
    async fn apply_reference_restores_changed_capability_descriptions() {
        let Some(pool) = migrated_pool().await else {
            return;
        };
        sqlx::query(
            "update mother_api.capability set description = 'stale' where id = 'balances.read'",
        )
        .execute(&pool)
        .await
        .unwrap();

        apply_embedded_catalog(&pool).await.unwrap();
        let description = sqlx::query_scalar::<_, String>(
            "select description from mother_api.capability where id = 'balances.read'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(
            description,
            "Read supported latest and historical balance snapshots."
        );
    }

    #[tokio::test]
    async fn invalid_reference_data_rolls_back_without_partial_writes() {
        let Some(pool) = migrated_pool().await else {
            return;
        };
        let mut catalog = parse_catalog_json(embedded_catalog_json()).unwrap();
        catalog.capabilities[0].id = "unknown.capability".to_string();
        let before = catalog_audit_rows(&pool).await;
        let capability_count_before =
            sqlx::query_scalar::<_, i64>("select count(*) from mother_api.capability")
                .fetch_one(&pool)
                .await
                .unwrap();

        let error = apply_catalog(&pool, &catalog).await.unwrap_err();
        assert!(error.to_string().contains("unknown capability id"));
        let after = catalog_audit_rows(&pool).await;
        let capability_count_after =
            sqlx::query_scalar::<_, i64>("select count(*) from mother_api.capability")
                .fetch_one(&pool)
                .await
                .unwrap();

        assert_eq!(after, before);
        assert_eq!(capability_count_after, capability_count_before);
    }

    fn assert_invalid(catalog: Catalog, expected: &str) {
        let error = validate_catalog(&catalog).unwrap_err();
        assert!(
            error.to_string().contains(expected),
            "expected error to contain {expected:?}, got {error}"
        );
    }

    fn minimal_catalog(suffix: &str) -> Catalog {
        let asset_slug = format!("test-asset-{suffix}");
        let network_slug = format!("test-network-{suffix}");
        let mut declared_asset = asset(&asset_slug);
        declared_asset.symbol = format!("T{}", &suffix[..8]);
        let chain_id = unique_chain_id(suffix);

        Catalog {
            version: crate::domain::canonical_registry::CATALOG_VERSION,
            capabilities: Capability::ALL
                .into_iter()
                .map(|capability| CapabilityDeclaration {
                    id: capability.id().to_string(),
                    description: capability.description().to_string(),
                })
                .collect(),
            assets: vec![declared_asset],
            networks: vec![NetworkDeclaration {
                slug: network_slug.clone(),
                name: format!("Test Network {suffix}"),
                family: "evm".to_string(),
                chain_id: Some(chain_id),
                caip2: Some(format!("eip155:{chain_id}")),
                metadata: json!({}),
                status: "active".to_string(),
                sort_order: 10,
            }],
            asset_chain_maps: vec![AssetChainMapDeclaration {
                asset_slug,
                network_slug,
                is_native: false,
                deployment_address: Some("0x1111111111111111111111111111111111111111".to_string()),
                deployment_block: Some(1),
                decimals: Some(18),
                token_standard: "erc20".to_string(),
                metadata: json!({}),
                status: "active".to_string(),
                sort_order: 10,
            }],
        }
    }

    fn asset(slug: &str) -> AssetDeclaration {
        AssetDeclaration {
            slug: slug.to_string(),
            symbol: "TST".to_string(),
            name: format!("Test Asset {slug}"),
            asset_kind: "crypto".to_string(),
            category: Some("crypto".to_string()),
            canonical_path: format!("/assets/{slug}"),
            aliases: vec![slug.to_string()],
            metadata: json!({}),
            status: "active".to_string(),
            sort_order: 10,
        }
    }

    async fn catalog_audit_rows(pool: &PgPool) -> Vec<(String, String, String, String)> {
        sqlx::query_as::<_, (String, String, String, String)>(
            r#"
            select 'asset', id::text, created_at::text, updated_at::text
            from mother_api.global_asset
            where slug = 'bitso-mxn'
            union all
            select 'network', id::text, created_at::text, updated_at::text
            from mother_api.network
            where slug = 'arbitrum-mainnet'
            union all
            select 'mapping', mapping.id::text, mapping.created_at::text, mapping.updated_at::text
            from mother_api.asset_chain_map mapping
            join mother_api.global_asset asset on asset.id = mapping.asset_id
            join mother_api.network network on network.id = mapping.network_id
            where asset.slug = 'bitso-mxn' and network.slug = 'arbitrum-mainnet'
            order by 1
            "#,
        )
        .fetch_all(pool)
        .await
        .unwrap()
    }

    fn unique_chain_id(suffix: &str) -> i64 {
        let offset = suffix.bytes().fold(0_i64, |accumulator, byte| {
            (accumulator * 31 + i64::from(byte)) % 50_000_000
        });
        900_000_000 + offset
    }
}
