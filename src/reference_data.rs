use std::collections::HashSet;

use serde::Deserialize;
use serde_json::Value;
use sqlx::{PgPool, Postgres, Transaction};

use crate::domain::validation::{is_asset_slug, is_evm_address};

const EMBEDDED_CATALOG_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/reference-data/catalog.json"
));
const CATALOG_VERSION: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub(crate) enum ReferenceDataError {
    #[error("failed to parse reference-data catalog: {0}")]
    Parse(serde_json::Error),
    #[error("invalid reference-data catalog: {0}")]
    Invalid(String),
    #[error("failed to apply reference-data catalog: {0}")]
    Database(sqlx::Error),
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Catalog {
    version: u32,
    assets: Vec<AssetDeclaration>,
    networks: Vec<NetworkDeclaration>,
    asset_chain_maps: Vec<AssetChainMapDeclaration>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AssetDeclaration {
    slug: String,
    symbol: String,
    name: String,
    asset_kind: String,
    category: Option<String>,
    canonical_path: String,
    aliases: Vec<String>,
    metadata: Value,
    status: String,
    sort_order: i32,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct NetworkDeclaration {
    slug: String,
    name: String,
    family: String,
    chain_id: Option<i64>,
    caip2: Option<String>,
    metadata: Value,
    status: String,
    sort_order: i32,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AssetChainMapDeclaration {
    asset_slug: String,
    network_slug: String,
    is_native: bool,
    deployment_address: Option<String>,
    deployment_block: Option<i64>,
    decimals: Option<i32>,
    token_standard: String,
    metadata: Value,
    status: String,
    sort_order: i32,
}

pub(crate) async fn apply_embedded_catalog(pool: &PgPool) -> Result<(), ReferenceDataError> {
    let catalog = parse_catalog_json(EMBEDDED_CATALOG_JSON)?;
    apply_catalog(pool, &catalog).await
}

fn parse_catalog_json(json: &str) -> Result<Catalog, ReferenceDataError> {
    let catalog = serde_json::from_str::<Catalog>(json).map_err(ReferenceDataError::Parse)?;
    validate_catalog(&catalog)?;
    Ok(catalog)
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

    for asset in &catalog.assets {
        upsert_asset(&mut transaction, asset).await?;
    }

    for network in &catalog.networks {
        upsert_network(&mut transaction, network).await?;
    }

    for mapping in &catalog.asset_chain_maps {
        upsert_asset_chain_map(&mut transaction, mapping).await?;
    }

    transaction
        .commit()
        .await
        .map_err(ReferenceDataError::Database)
}

fn validate_catalog(catalog: &Catalog) -> Result<(), ReferenceDataError> {
    if catalog.version != CATALOG_VERSION {
        return invalid(format!(
            "unsupported catalog version {}, expected {CATALOG_VERSION}",
            catalog.version
        ));
    }

    let mut asset_slugs = HashSet::new();
    for asset in &catalog.assets {
        validate_asset(asset)?;
        if !asset_slugs.insert(asset.slug.as_str()) {
            return invalid(format!("duplicate asset slug {:?}", asset.slug));
        }
    }

    let mut network_slugs = HashSet::new();
    for network in &catalog.networks {
        validate_network(network)?;
        if !network_slugs.insert(network.slug.as_str()) {
            return invalid(format!("duplicate network slug {:?}", network.slug));
        }
    }

    let mut mapping_keys = HashSet::new();
    let mut active_native_networks = HashSet::new();
    let mut active_network_addresses = HashSet::new();

    for mapping in &catalog.asset_chain_maps {
        validate_mapping(mapping)?;

        if !asset_slugs.contains(mapping.asset_slug.as_str()) {
            return invalid(format!(
                "asset_chain_map references undeclared asset {:?}",
                mapping.asset_slug
            ));
        }
        if !network_slugs.contains(mapping.network_slug.as_str()) {
            return invalid(format!(
                "asset_chain_map references undeclared network {:?}",
                mapping.network_slug
            ));
        }

        let mapping_key = (mapping.asset_slug.as_str(), mapping.network_slug.as_str());
        if !mapping_keys.insert(mapping_key) {
            return invalid(format!(
                "duplicate asset_chain_map identity ({:?}, {:?})",
                mapping.asset_slug, mapping.network_slug
            ));
        }

        if mapping.status == "active" && mapping.is_native {
            if !active_native_networks.insert(mapping.network_slug.as_str()) {
                return invalid(format!(
                    "duplicate active native mapping for network {:?}",
                    mapping.network_slug
                ));
            }
        }

        if mapping.status == "active" {
            if let Some(address) = mapping.deployment_address.as_deref() {
                let address_key = (mapping.network_slug.as_str(), address);
                if !active_network_addresses.insert(address_key) {
                    return invalid(format!(
                        "duplicate active deployment address {:?} on network {:?}",
                        address, mapping.network_slug
                    ));
                }
            }
        }
    }

    Ok(())
}

fn validate_asset(asset: &AssetDeclaration) -> Result<(), ReferenceDataError> {
    validate_slug("asset slug", &asset.slug)?;
    validate_non_empty("asset symbol", &asset.symbol)?;
    validate_non_empty("asset name", &asset.name)?;
    validate_non_empty("asset kind", &asset.asset_kind)?;
    if let Some(category) = asset.category.as_deref() {
        validate_non_empty("asset category", category)?;
    }
    if asset.canonical_path != format!("/assets/{}", asset.slug) {
        return invalid(format!(
            "asset {:?} has invalid canonical_path {:?}",
            asset.slug, asset.canonical_path
        ));
    }
    validate_status("asset", &asset.status)?;
    validate_sort_order("asset", asset.sort_order)?;
    validate_aliases(&asset.slug, &asset.aliases)?;
    validate_metadata("asset", &asset.slug, &asset.metadata)
}

fn validate_network(network: &NetworkDeclaration) -> Result<(), ReferenceDataError> {
    validate_slug("network slug", &network.slug)?;
    validate_non_empty("network name", &network.name)?;
    validate_non_empty("network family", &network.family)?;
    validate_status("network", &network.status)?;
    validate_sort_order("network", network.sort_order)?;
    validate_metadata("network", &network.slug, &network.metadata)?;

    if network.family == "evm" {
        let chain_id = network
            .chain_id
            .filter(|chain_id| *chain_id > 0)
            .ok_or_else(|| {
                ReferenceDataError::Invalid(format!(
                    "evm network {:?} requires positive chain_id",
                    network.slug
                ))
            })?;
        let expected_caip2 = format!("eip155:{chain_id}");
        if network.caip2.as_deref() != Some(expected_caip2.as_str()) {
            return invalid(format!(
                "evm network {:?} requires caip2 {:?}",
                network.slug, expected_caip2
            ));
        }
    }

    Ok(())
}

fn validate_mapping(mapping: &AssetChainMapDeclaration) -> Result<(), ReferenceDataError> {
    validate_slug("mapping asset_slug", &mapping.asset_slug)?;
    validate_slug("mapping network_slug", &mapping.network_slug)?;
    validate_status("mapping", &mapping.status)?;
    validate_sort_order("mapping", mapping.sort_order)?;
    validate_metadata("mapping", &mapping.asset_slug, &mapping.metadata)?;

    let decimals = mapping.decimals.ok_or_else(|| {
        ReferenceDataError::Invalid(format!(
            "mapping ({:?}, {:?}) requires decimals",
            mapping.asset_slug, mapping.network_slug
        ))
    })?;
    if !(0..=255).contains(&decimals) {
        return invalid(format!(
            "mapping ({:?}, {:?}) has invalid decimals {}",
            mapping.asset_slug, mapping.network_slug, decimals
        ));
    }

    if mapping.is_native {
        if mapping.deployment_address.is_some() {
            return invalid(format!(
                "native mapping ({:?}, {:?}) must not declare deployment_address",
                mapping.asset_slug, mapping.network_slug
            ));
        }
        if mapping.token_standard != "native" {
            return invalid(format!(
                "native mapping ({:?}, {:?}) requires token_standard \"native\"",
                mapping.asset_slug, mapping.network_slug
            ));
        }
        return Ok(());
    }

    validate_non_empty("mapping token_standard", &mapping.token_standard)?;
    if mapping.token_standard == "native" {
        return invalid(format!(
            "non-native mapping ({:?}, {:?}) must not use token_standard \"native\"",
            mapping.asset_slug, mapping.network_slug
        ));
    }

    let Some(address) = mapping.deployment_address.as_deref() else {
        return invalid(format!(
            "non-native mapping ({:?}, {:?}) requires deployment_address",
            mapping.asset_slug, mapping.network_slug
        ));
    };

    if mapping.token_standard == "erc20" && !is_evm_address(address) {
        return invalid(format!(
            "erc20 mapping ({:?}, {:?}) has invalid deployment_address {:?}",
            mapping.asset_slug, mapping.network_slug, address
        ));
    }

    if mapping.token_standard == "erc20" && address != address.to_ascii_lowercase() {
        return invalid(format!(
            "erc20 mapping ({:?}, {:?}) requires lowercase deployment_address",
            mapping.asset_slug, mapping.network_slug
        ));
    }

    Ok(())
}

fn validate_slug(label: &str, slug: &str) -> Result<(), ReferenceDataError> {
    if is_asset_slug(slug) {
        Ok(())
    } else {
        invalid(format!("{label} {slug:?} is not normalized"))
    }
}

fn validate_non_empty(label: &str, value: &str) -> Result<(), ReferenceDataError> {
    if value.trim().is_empty() {
        invalid(format!("{label} must not be empty"))
    } else {
        Ok(())
    }
}

fn validate_aliases(asset_slug: &str, aliases: &[String]) -> Result<(), ReferenceDataError> {
    let mut seen = HashSet::new();
    for alias in aliases {
        if alias.is_empty() || alias.trim() != alias || alias != &alias.to_ascii_lowercase() {
            return invalid(format!(
                "asset {asset_slug:?} has non-normalized alias {alias:?}"
            ));
        }
        if !seen.insert(alias.as_str()) {
            return invalid(format!(
                "asset {asset_slug:?} has duplicate alias {alias:?}"
            ));
        }
    }
    Ok(())
}

fn validate_status(owner: &str, status: &str) -> Result<(), ReferenceDataError> {
    match status {
        "active" | "inactive" | "deprecated" | "hidden" | "pending" | "unsupported"
        | "archived" => Ok(()),
        _ => invalid(format!("{owner} has invalid status {status:?}")),
    }
}

fn validate_sort_order(owner: &str, sort_order: i32) -> Result<(), ReferenceDataError> {
    if sort_order < 0 {
        invalid(format!("{owner} has negative sort_order {sort_order}"))
    } else {
        Ok(())
    }
}

fn validate_metadata(
    owner: &str,
    identity: &str,
    metadata: &Value,
) -> Result<(), ReferenceDataError> {
    if metadata.is_object() {
        Ok(())
    } else {
        invalid(format!(
            "{owner} {identity:?} metadata must be a JSON object"
        ))
    }
}

async fn upsert_asset(
    transaction: &mut Transaction<'_, Postgres>,
    asset: &AssetDeclaration,
) -> Result<(), ReferenceDataError> {
    sqlx::query(
        r#"
        insert into mother_api.global_asset as existing (
            slug,
            symbol,
            name,
            asset_kind,
            category,
            canonical_path,
            aliases,
            metadata,
            status,
            sort_order
        )
        values (
            $1,
            $2,
            $3,
            $4,
            $5,
            $6,
            $7,
            $8::jsonb,
            $9::mother_api.global_asset_status,
            $10
        )
        on conflict (slug) do update
        set
            symbol = excluded.symbol,
            name = excluded.name,
            asset_kind = excluded.asset_kind,
            category = excluded.category,
            canonical_path = excluded.canonical_path,
            aliases = excluded.aliases,
            metadata = excluded.metadata,
            status = excluded.status,
            sort_order = excluded.sort_order,
            updated_at = now()
        where
            existing.symbol is distinct from excluded.symbol
            or existing.name is distinct from excluded.name
            or existing.asset_kind is distinct from excluded.asset_kind
            or existing.category is distinct from excluded.category
            or existing.canonical_path is distinct from excluded.canonical_path
            or existing.aliases is distinct from excluded.aliases
            or existing.metadata is distinct from excluded.metadata
            or existing.status is distinct from excluded.status
            or existing.sort_order is distinct from excluded.sort_order
        "#,
    )
    .bind(&asset.slug)
    .bind(&asset.symbol)
    .bind(&asset.name)
    .bind(&asset.asset_kind)
    .bind(&asset.category)
    .bind(&asset.canonical_path)
    .bind(&asset.aliases)
    .bind(asset.metadata.to_string())
    .bind(&asset.status)
    .bind(asset.sort_order)
    .execute(&mut **transaction)
    .await
    .map_err(ReferenceDataError::Database)?;

    Ok(())
}

async fn upsert_network(
    transaction: &mut Transaction<'_, Postgres>,
    network: &NetworkDeclaration,
) -> Result<(), ReferenceDataError> {
    sqlx::query(
        r#"
        insert into mother_api.network as existing (
            slug,
            name,
            family,
            chain_id,
            caip2,
            metadata,
            status,
            sort_order
        )
        values (
            $1,
            $2,
            $3,
            $4,
            $5,
            $6::jsonb,
            $7,
            $8
        )
        on conflict (slug) do update
        set
            name = excluded.name,
            family = excluded.family,
            chain_id = excluded.chain_id,
            caip2 = excluded.caip2,
            metadata = excluded.metadata,
            status = excluded.status,
            sort_order = excluded.sort_order,
            updated_at = now()
        where
            existing.name is distinct from excluded.name
            or existing.family is distinct from excluded.family
            or existing.chain_id is distinct from excluded.chain_id
            or existing.caip2 is distinct from excluded.caip2
            or existing.metadata is distinct from excluded.metadata
            or existing.status is distinct from excluded.status
            or existing.sort_order is distinct from excluded.sort_order
        "#,
    )
    .bind(&network.slug)
    .bind(&network.name)
    .bind(&network.family)
    .bind(network.chain_id)
    .bind(&network.caip2)
    .bind(network.metadata.to_string())
    .bind(&network.status)
    .bind(network.sort_order)
    .execute(&mut **transaction)
    .await
    .map_err(ReferenceDataError::Database)?;

    Ok(())
}

async fn upsert_asset_chain_map(
    transaction: &mut Transaction<'_, Postgres>,
    mapping: &AssetChainMapDeclaration,
) -> Result<(), ReferenceDataError> {
    sqlx::query(
        r#"
        with resolved as (
            select
                asset.id as asset_id,
                network.id as network_id
            from mother_api.global_asset asset
            join mother_api.network network
                on network.slug = $2
            where asset.slug = $1
        )
        insert into mother_api.asset_chain_map as existing (
            asset_id,
            network_id,
            is_native,
            deployment_address,
            deployment_block,
            decimals,
            token_standard,
            metadata,
            status,
            sort_order
        )
        select
            resolved.asset_id,
            resolved.network_id,
            $3,
            $4,
            $5,
            $6,
            $7,
            $8::jsonb,
            $9,
            $10
        from resolved
        on conflict (asset_id, network_id) do update
        set
            is_native = excluded.is_native,
            deployment_address = excluded.deployment_address,
            deployment_block = excluded.deployment_block,
            decimals = excluded.decimals,
            token_standard = excluded.token_standard,
            metadata = excluded.metadata,
            status = excluded.status,
            sort_order = excluded.sort_order,
            updated_at = now()
        where
            existing.is_native is distinct from excluded.is_native
            or existing.deployment_address is distinct from excluded.deployment_address
            or existing.deployment_block is distinct from excluded.deployment_block
            or existing.decimals is distinct from excluded.decimals
            or existing.token_standard is distinct from excluded.token_standard
            or existing.metadata is distinct from excluded.metadata
            or existing.status is distinct from excluded.status
            or existing.sort_order is distinct from excluded.sort_order
        "#,
    )
    .bind(&mapping.asset_slug)
    .bind(&mapping.network_slug)
    .bind(mapping.is_native)
    .bind(&mapping.deployment_address)
    .bind(mapping.deployment_block)
    .bind(mapping.decimals)
    .bind(&mapping.token_standard)
    .bind(mapping.metadata.to_string())
    .bind(&mapping.status)
    .bind(mapping.sort_order)
    .execute(&mut **transaction)
    .await
    .map_err(ReferenceDataError::Database)?;

    Ok(())
}

fn invalid<T>(message: String) -> Result<T, ReferenceDataError> {
    Err(ReferenceDataError::Invalid(message))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::test_utils::postgres::migrated_pool;

    #[test]
    fn embedded_catalog_parses_and_validates() {
        parse_catalog_json(EMBEDDED_CATALOG_JSON).unwrap();
    }

    #[test]
    fn duplicate_asset_slugs_fail_validation() {
        let mut catalog = minimal_catalog("duplicate-asset-slugs");
        catalog.assets.push(catalog.assets[0].clone());

        assert_invalid(catalog, "duplicate asset slug");
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
          "version": 1,
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
    async fn apply_reference_succeeds_after_migrations() {
        let Some(pool) = migrated_pool().await else {
            return;
        };

        apply_embedded_catalog(&pool).await.unwrap();

        let bitso_mxn_arbitrum = sqlx::query_scalar::<_, i64>(
            r#"
            select count(*)
            from mother_api.asset_chain_map mapping
            join mother_api.global_asset asset
                on asset.id = mapping.asset_id
            join mother_api.network network
                on network.id = mapping.network_id
            where asset.slug = 'bitso-mxn'
                and network.slug = 'arbitrum-mainnet'
                and mapping.deployment_address = '0xf197ffc28c23e0309b5559e7a166f2c6164c80aa'
            "#,
        )
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(bitso_mxn_arbitrum, 1);
    }

    #[tokio::test]
    async fn apply_reference_preserves_ids_and_created_at_without_noop_updated_at_churn() {
        let Some(pool) = migrated_pool().await else {
            return;
        };
        let suffix = unique_suffix();
        let catalog = minimal_catalog(&suffix);

        apply_catalog(&pool, &catalog).await.unwrap();
        let before = asset_audit_row(&pool, &catalog.assets[0].slug).await;

        apply_catalog(&pool, &catalog).await.unwrap();
        let after = asset_audit_row(&pool, &catalog.assets[0].slug).await;

        assert_eq!(after, before);
    }

    #[tokio::test]
    async fn changed_declared_value_updates_only_affected_row() {
        let Some(pool) = migrated_pool().await else {
            return;
        };
        let suffix = unique_suffix();
        let mut catalog = minimal_catalog(&suffix);

        apply_catalog(&pool, &catalog).await.unwrap();
        let asset_before = asset_audit_row(&pool, &catalog.assets[0].slug).await;
        let network_before = network_audit_row(&pool, &catalog.networks[0].slug).await;

        catalog.assets[0].name = format!("{} Updated", catalog.assets[0].name);
        apply_catalog(&pool, &catalog).await.unwrap();

        let asset_after = asset_audit_row(&pool, &catalog.assets[0].slug).await;
        let network_after = network_audit_row(&pool, &catalog.networks[0].slug).await;

        assert_eq!(asset_after.0, asset_before.0);
        assert_eq!(asset_after.1, asset_before.1);
        assert_ne!(asset_after.2, asset_before.2);
        assert_eq!(network_after, network_before);
    }

    #[tokio::test]
    async fn invalid_reference_data_rolls_back_without_partial_writes() {
        let Some(pool) = migrated_pool().await else {
            return;
        };
        let suffix = unique_suffix();
        let mut catalog = minimal_catalog(&suffix);
        catalog.asset_chain_maps[0].asset_slug = "missing-rollback-asset".to_string();

        let error = apply_catalog(&pool, &catalog).await.unwrap_err();
        assert!(error.to_string().contains("references undeclared asset"));

        let asset_count = sqlx::query_scalar::<_, i64>(
            "select count(*) from mother_api.global_asset where slug = $1",
        )
        .bind(&catalog.assets[0].slug)
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(asset_count, 0);
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
            version: CATALOG_VERSION,
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

    async fn asset_audit_row(pool: &PgPool, slug: &str) -> (String, String, String) {
        sqlx::query_as::<_, (String, String, String)>(
            r#"
            select id::text, created_at::text, updated_at::text
            from mother_api.global_asset
            where slug = $1
            "#,
        )
        .bind(slug)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    async fn network_audit_row(pool: &PgPool, slug: &str) -> (String, String, String) {
        sqlx::query_as::<_, (String, String, String)>(
            r#"
            select id::text, created_at::text, updated_at::text
            from mother_api.network
            where slug = $1
            "#,
        )
        .bind(slug)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    fn unique_suffix() -> String {
        uuid::Uuid::new_v4().simple().to_string()
    }

    fn unique_chain_id(suffix: &str) -> i64 {
        let offset = suffix.bytes().fold(0_i64, |accumulator, byte| {
            (accumulator * 31 + i64::from(byte)) % 50_000_000
        });
        900_000_000 + offset
    }
}
