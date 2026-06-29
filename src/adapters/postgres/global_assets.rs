use std::sync::Arc;

use sqlx::{FromRow, PgPool};

use crate::domain::asset_match::{confidence_rank, AssetMatch, ExactMatchConfidence};
use crate::domain::global_assets::{GlobalAsset, GlobalAssetDetail};
use crate::domain::networks::NetworkRef;

use super::asset_chain_map::{
    demo_chain_maps_for_assets, map_chain_map_row, AssetChainMapRow, InMemoryAssetChainMap,
};
use super::asset_match::{map_match_row, AssetMatchRow};
use super::balance_catalog::{BalanceCatalogRow, Erc20TokenCatalogRow};
use super::errors::RepositoryError;

#[derive(FromRow)]
struct GlobalAssetRow {
    id: String,
    slug: String,
    symbol: String,
    name: String,
    category: Option<String>,
    canonical_path: String,
    aliases: Vec<String>,
    sort_order: i32,
}

#[derive(Clone, Debug)]
pub(crate) struct InMemoryGlobalAssets {
    assets: Vec<GlobalAsset>,
    chain_maps: Vec<InMemoryAssetChainMap>,
}

impl InMemoryGlobalAssets {
    pub(super) fn new(assets: Vec<GlobalAsset>) -> Self {
        let chain_maps = demo_chain_maps_for_assets(&assets);

        Self { assets, chain_maps }
    }
}

#[derive(Clone, Debug)]
pub enum GlobalAssetRepository {
    Database(PgPool),
    #[allow(dead_code)]
    InMemory(Arc<InMemoryGlobalAssets>),
}

impl GlobalAssetRepository {
    pub fn database(pool: PgPool) -> Self {
        Self::Database(pool)
    }

    #[allow(dead_code)]
    pub fn in_memory(assets: Vec<GlobalAsset>) -> Self {
        Self::InMemory(Arc::new(InMemoryGlobalAssets::new(assets)))
    }

    #[cfg(test)]
    fn in_memory_with_chain_maps(
        assets: Vec<GlobalAsset>,
        chain_maps: Vec<InMemoryAssetChainMap>,
    ) -> Self {
        Self::InMemory(Arc::new(InMemoryGlobalAssets { assets, chain_maps }))
    }

    pub async fn find_confident_match(
        &self,
        normalized_query: &str,
    ) -> Result<Option<AssetMatch>, RepositoryError> {
        match self {
            Self::Database(pool) => find_confident_match_db(pool, normalized_query).await,
            Self::InMemory(catalog) => Ok(find_confident_match_in_memory(
                &catalog.assets,
                normalized_query,
            )),
        }
    }

    pub async fn list_recommendations(
        &self,
        normalized_query: &str,
        limit: i64,
    ) -> Result<Vec<GlobalAsset>, RepositoryError> {
        match self {
            Self::Database(pool) => list_recommendations_db(pool, normalized_query, limit).await,
            Self::InMemory(catalog) => Ok(list_recommendations_in_memory(
                &catalog.assets,
                normalized_query,
                limit as usize,
            )),
        }
    }

    pub async fn list_assets(&self, limit: u64) -> Result<Vec<GlobalAsset>, RepositoryError> {
        match self {
            Self::Database(pool) => list_assets_db(pool, limit).await,
            Self::InMemory(catalog) => Ok(list_assets_in_memory(
                &catalog.assets,
                usize::try_from(limit).unwrap_or(usize::MAX),
            )),
        }
    }

    pub async fn get_asset_detail_by_slug(
        &self,
        slug: &str,
    ) -> Result<Option<GlobalAssetDetail>, RepositoryError> {
        match self {
            Self::Database(pool) => get_asset_detail_by_slug_db(pool, slug).await,
            Self::InMemory(catalog) => Ok(get_asset_detail_by_slug_in_memory(catalog, slug)),
        }
    }

    #[allow(dead_code)]
    pub(crate) async fn load_balance_catalog_rows(
        &self,
        network_slug: &str,
        ordered_asset_slugs: &[String],
    ) -> Result<Vec<BalanceCatalogRow>, RepositoryError> {
        match self {
            Self::Database(pool) => {
                load_balance_catalog_rows_db(pool, network_slug, ordered_asset_slugs).await
            }
            Self::InMemory(catalog) => Ok(load_balance_catalog_rows_in_memory(
                catalog,
                network_slug,
                ordered_asset_slugs,
            )),
        }
    }

    #[allow(dead_code)]
    pub(crate) async fn load_erc20_token_catalog_rows(
        &self,
        network_slug: &str,
        contract_addresses: &[String],
    ) -> Result<Vec<Erc20TokenCatalogRow>, RepositoryError> {
        if contract_addresses.is_empty() {
            return Ok(Vec::new());
        }

        match self {
            Self::Database(pool) => {
                load_erc20_token_catalog_rows_db(pool, network_slug, contract_addresses).await
            }
            Self::InMemory(catalog) => Ok(load_erc20_token_catalog_rows_in_memory(
                catalog,
                network_slug,
                contract_addresses,
            )),
        }
    }
}

fn map_row(row: GlobalAssetRow) -> GlobalAsset {
    GlobalAsset {
        id: row.id,
        slug: row.slug,
        symbol: row.symbol,
        name: row.name,
        category: row.category.unwrap_or_else(|| "asset".to_string()),
        canonical_path: row.canonical_path,
        aliases: row.aliases,
        sort_order: row.sort_order,
    }
}

async fn find_confident_match_db(
    pool: &PgPool,
    normalized_query: &str,
) -> Result<Option<AssetMatch>, RepositoryError> {
    let row = sqlx::query_as::<_, AssetMatchRow>(
        r#"
        select
          id::text,
          slug,
          symbol,
          name,
          coalesce(category, asset_kind) as category,
          canonical_path,
          aliases,
          sort_order,
          case
            when lower(slug) = $1 then 'slug_exact'
            when lower(symbol) = $1 then 'symbol_exact'
            when lower(name) = $1 then 'name_exact'
            else 'alias_exact'
          end as match_kind
        from mother_api.global_asset
        where status = 'active'
          and (
            lower(slug) = $1
            or lower(symbol) = $1
            or lower(name) = $1
            or exists (
              select 1
              from unnest(aliases) as alias
              where lower(alias) = $1
            )
          )
        order by
          case
            when lower(slug) = $1 then 0
            when lower(symbol) = $1 then 1
            when lower(name) = $1 then 2
            else 3
          end,
          sort_order asc,
          lower(symbol) asc
        limit 1
        "#,
    )
    .bind(normalized_query)
    .fetch_optional(pool)
    .await
    .map_err(RepositoryError::new)?;

    Ok(row.map(map_match_row))
}

async fn list_recommendations_db(
    pool: &PgPool,
    normalized_query: &str,
    limit: i64,
) -> Result<Vec<GlobalAsset>, RepositoryError> {
    let contains_pattern = format!("%{}%", escape_like_pattern(normalized_query));
    let rows = sqlx::query_as::<_, GlobalAssetRow>(
        r#"
        select
          id::text,
          slug,
          symbol,
          name,
          coalesce(category, asset_kind) as category,
          canonical_path,
          aliases,
          sort_order
        from mother_api.global_asset
        where status = 'active'
          and (
            lower(slug) like $1 escape '\'
            or lower(symbol) like $1 escape '\'
            or lower(name) like $1 escape '\'
            or exists (
              select 1
              from unnest(aliases) as alias
              where lower(alias) like $1 escape '\'
            )
          )
        order by sort_order asc, lower(symbol) asc
        limit $2
        "#,
    )
    .bind(&contains_pattern)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(RepositoryError::new)?;

    if !rows.is_empty() {
        return Ok(rows.into_iter().map(map_row).collect());
    }

    let fallback_rows = sqlx::query_as::<_, GlobalAssetRow>(
        r#"
        select
          id::text,
          slug,
          symbol,
          name,
          coalesce(category, asset_kind) as category,
          canonical_path,
          aliases,
          sort_order
        from mother_api.global_asset
        where status = 'active'
        order by sort_order asc, lower(symbol) asc
        limit $1
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(RepositoryError::new)?;

    Ok(fallback_rows.into_iter().map(map_row).collect())
}

async fn list_assets_db(pool: &PgPool, limit: u64) -> Result<Vec<GlobalAsset>, RepositoryError> {
    let rows = sqlx::query_as::<_, GlobalAssetRow>(
        r#"
        select
          id::text,
          slug,
          symbol,
          name,
          coalesce(category, asset_kind) as category,
          canonical_path,
          aliases,
          sort_order
        from mother_api.global_asset
        where status = 'active'
        order by sort_order asc, lower(symbol) asc
        limit $1
        "#,
    )
    .bind(i64::try_from(limit).unwrap_or(i64::MAX))
    .fetch_all(pool)
    .await
    .map_err(RepositoryError::new)?;

    Ok(rows.into_iter().map(map_row).collect())
}

async fn get_asset_detail_by_slug_db(
    pool: &PgPool,
    slug: &str,
) -> Result<Option<GlobalAssetDetail>, RepositoryError> {
    let asset = sqlx::query_as::<_, GlobalAssetRow>(
        r#"
        select
          id::text,
          slug,
          symbol,
          name,
          coalesce(category, asset_kind) as category,
          canonical_path,
          aliases,
          sort_order
        from mother_api.global_asset
        where status = 'active'
          and lower(slug) = lower($1)
        limit 1
        "#,
    )
    .bind(slug)
    .fetch_optional(pool)
    .await
    .map_err(RepositoryError::new)?
    .map(map_row);

    let Some(asset) = asset else {
        return Ok(None);
    };

    let chain_maps = sqlx::query_as::<_, AssetChainMapRow>(
        r#"
        select
          network.slug as network_slug,
          network.name as network_name,
          network.caip2 as network_caip2,
          network.family as network_family,
          network.chain_id as network_chain_id,
          asset_chain_map.is_native,
          asset_chain_map.deployment_address as address,
          asset_chain_map.decimals,
          asset_chain_map.token_standard
        from mother_api.asset_chain_map
        join mother_api.network network
          on network.id = asset_chain_map.network_id
        where asset_chain_map.status = 'active'
          and network.status = 'active'
          and asset_chain_map.asset_id = $1::uuid
        order by asset_chain_map.sort_order asc, lower(network.slug) asc
        "#,
    )
    .bind(&asset.id)
    .fetch_all(pool)
    .await
    .map_err(RepositoryError::new)?
    .into_iter()
    .map(map_chain_map_row)
    .collect();

    Ok(Some(GlobalAssetDetail { asset, chain_maps }))
}

#[allow(dead_code)]
async fn load_balance_catalog_rows_db(
    pool: &PgPool,
    network_slug: &str,
    ordered_asset_slugs: &[String],
) -> Result<Vec<BalanceCatalogRow>, RepositoryError> {
    sqlx::query_as::<_, BalanceCatalogRow>(
        r#"
        with requested_assets as (
          select
            requested.asset_slug,
            requested.ordinality::bigint as ordinal
          from unnest($2::text[]) with ordinality
            as requested(asset_slug, ordinality)
        ),
        requested_network as (
          select
            id,
            slug,
            family,
            chain_id
          from mother_api.network
          where status = 'active'
            and slug = $1
        )
        select
          requested_assets.ordinal,
          requested_assets.asset_slug as requested_asset_slug,
          requested_network.slug as network_slug,
          requested_network.family as network_family,
          requested_network.chain_id as network_chain_id,
          global_asset.slug as asset_slug,
          global_asset.symbol as asset_symbol,
          global_asset.name as asset_name,
          asset_chain_map.id::text as mapping_id,
          asset_chain_map.is_native,
          asset_chain_map.deployment_address,
          asset_chain_map.decimals,
          asset_chain_map.token_standard
        from requested_assets
        left join requested_network
          on true
        left join mother_api.global_asset global_asset
          on global_asset.status = 'active'
         and global_asset.slug = requested_assets.asset_slug
        left join mother_api.asset_chain_map asset_chain_map
          on asset_chain_map.status = 'active'
         and asset_chain_map.network_id = requested_network.id
         and asset_chain_map.asset_id = global_asset.id
        order by requested_assets.ordinal asc, asset_chain_map.id asc
        "#,
    )
    .bind(network_slug)
    .bind(ordered_asset_slugs)
    .fetch_all(pool)
    .await
    .map_err(RepositoryError::new)
}

#[allow(dead_code)]
async fn load_erc20_token_catalog_rows_db(
    pool: &PgPool,
    network_slug: &str,
    contract_addresses: &[String],
) -> Result<Vec<Erc20TokenCatalogRow>, RepositoryError> {
    sqlx::query_as::<_, Erc20TokenCatalogRow>(
        r#"
        with requested_contracts as (
          select lower(contract_address) as contract_address
          from unnest($2::text[]) as requested(contract_address)
          group by lower(contract_address)
        )
        select
          requested_contracts.contract_address,
          global_asset.slug as asset_slug,
          global_asset.symbol as asset_symbol,
          asset_chain_map.decimals
        from requested_contracts
        join mother_api.network network
          on network.status = 'active'
         and network.slug = $1
         and network.family = 'evm'
        join mother_api.asset_chain_map asset_chain_map
          on asset_chain_map.status = 'active'
         and asset_chain_map.network_id = network.id
         and asset_chain_map.is_native = false
         and asset_chain_map.token_standard = 'erc20'
         and lower(asset_chain_map.deployment_address) = requested_contracts.contract_address
        join mother_api.global_asset global_asset
          on global_asset.status = 'active'
         and global_asset.id = asset_chain_map.asset_id
        order by requested_contracts.contract_address asc, asset_chain_map.sort_order asc
        "#,
    )
    .bind(network_slug)
    .bind(contract_addresses)
    .fetch_all(pool)
    .await
    .map_err(RepositoryError::new)
}

fn find_confident_match_in_memory(
    assets: &[GlobalAsset],
    normalized_query: &str,
) -> Option<AssetMatch> {
    let mut candidates = assets
        .iter()
        .filter_map(|asset| {
            let confidence = if asset.slug.eq_ignore_ascii_case(normalized_query) {
                ExactMatchConfidence::Slug
            } else if asset.symbol.eq_ignore_ascii_case(normalized_query) {
                ExactMatchConfidence::Symbol
            } else if asset.name.eq_ignore_ascii_case(normalized_query) {
                ExactMatchConfidence::Name
            } else if asset
                .aliases
                .iter()
                .any(|alias| alias.eq_ignore_ascii_case(normalized_query))
            {
                ExactMatchConfidence::Alias
            } else {
                return None;
            };

            Some(AssetMatch {
                asset: asset.clone(),
                confidence,
            })
        })
        .collect::<Vec<_>>();

    candidates.sort_by(|left, right| {
        confidence_rank(left.confidence)
            .cmp(&confidence_rank(right.confidence))
            .then_with(|| left.asset.sort_order.cmp(&right.asset.sort_order))
            .then_with(|| left.asset.symbol.cmp(&right.asset.symbol))
    });

    candidates.into_iter().next()
}

fn get_asset_detail_by_slug_in_memory(
    catalog: &InMemoryGlobalAssets,
    slug: &str,
) -> Option<GlobalAssetDetail> {
    let asset = catalog
        .assets
        .iter()
        .find(|asset| asset.slug.eq_ignore_ascii_case(slug))?
        .clone();

    let mut chain_maps = catalog
        .chain_maps
        .iter()
        .filter(|chain_map| chain_map.asset_slug.eq_ignore_ascii_case(&asset.slug))
        .cloned()
        .collect::<Vec<_>>();

    chain_maps.sort_by(|left, right| {
        left.sort_order.cmp(&right.sort_order).then_with(|| {
            left.chain_map
                .network
                .slug
                .to_lowercase()
                .cmp(&right.chain_map.network.slug.to_lowercase())
        })
    });

    Some(GlobalAssetDetail {
        asset,
        chain_maps: chain_maps
            .into_iter()
            .map(|chain_map| chain_map.chain_map)
            .collect(),
    })
}

#[allow(dead_code)]
fn load_balance_catalog_rows_in_memory(
    catalog: &InMemoryGlobalAssets,
    network_slug: &str,
    ordered_asset_slugs: &[String],
) -> Vec<BalanceCatalogRow> {
    let network = catalog
        .chain_maps
        .iter()
        .find(|mapping| mapping.chain_map.network.slug == network_slug)
        .map(|mapping| &mapping.chain_map.network);

    let mut rows = Vec::new();

    for (index, requested_asset_slug) in ordered_asset_slugs.iter().enumerate() {
        let ordinal = i64::try_from(index + 1).unwrap_or(i64::MAX);
        let asset = catalog
            .assets
            .iter()
            .find(|asset| asset.slug == *requested_asset_slug);
        let mappings = asset
            .map(|asset| {
                catalog
                    .chain_maps
                    .iter()
                    .enumerate()
                    .filter(|(_, mapping)| {
                        mapping.asset_slug == asset.slug
                            && mapping.chain_map.network.slug == network_slug
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        if mappings.is_empty() {
            rows.push(in_memory_balance_catalog_row(
                ordinal,
                requested_asset_slug,
                network,
                asset,
                None,
            ));
            continue;
        }

        for (mapping_index, mapping) in mappings {
            rows.push(in_memory_balance_catalog_row(
                ordinal,
                requested_asset_slug,
                network,
                asset,
                Some((mapping_index, mapping)),
            ));
        }
    }

    rows
}

#[allow(dead_code)]
fn in_memory_balance_catalog_row(
    ordinal: i64,
    requested_asset_slug: &str,
    network: Option<&NetworkRef>,
    asset: Option<&GlobalAsset>,
    mapping: Option<(usize, &InMemoryAssetChainMap)>,
) -> BalanceCatalogRow {
    BalanceCatalogRow {
        ordinal,
        requested_asset_slug: requested_asset_slug.to_string(),
        network_slug: network.map(|network| network.slug.clone()),
        network_family: network.map(|network| network.family.clone()),
        network_chain_id: network.and_then(|network| network.chain_id),
        asset_slug: asset.map(|asset| asset.slug.clone()),
        asset_symbol: asset.map(|asset| asset.symbol.clone()),
        asset_name: asset.map(|asset| asset.name.clone()),
        mapping_id: mapping.map(|(index, _)| format!("in-memory-mapping-{index}")),
        is_native: mapping.map(|(_, mapping)| mapping.chain_map.is_native),
        deployment_address: mapping.and_then(|(_, mapping)| mapping.chain_map.address.clone()),
        decimals: mapping.and_then(|(_, mapping)| mapping.chain_map.decimals),
        token_standard: mapping.and_then(|(_, mapping)| mapping.chain_map.token_standard.clone()),
    }
}

#[allow(dead_code)]
fn load_erc20_token_catalog_rows_in_memory(
    catalog: &InMemoryGlobalAssets,
    network_slug: &str,
    contract_addresses: &[String],
) -> Vec<Erc20TokenCatalogRow> {
    let requested = contract_addresses
        .iter()
        .map(|contract_address| contract_address.to_ascii_lowercase())
        .collect::<std::collections::HashSet<_>>();
    let mut rows = Vec::new();

    for mapping in &catalog.chain_maps {
        let chain_map = &mapping.chain_map;
        let Some(address) = chain_map.address.as_deref() else {
            continue;
        };
        let contract_address = address.to_ascii_lowercase();
        if chain_map.network.slug != network_slug
            || chain_map.is_native
            || chain_map.token_standard.as_deref() != Some("erc20")
            || !requested.contains(&contract_address)
        {
            continue;
        }

        let Some(asset) = catalog
            .assets
            .iter()
            .find(|asset| asset.slug == mapping.asset_slug)
        else {
            continue;
        };

        rows.push(Erc20TokenCatalogRow {
            contract_address,
            asset_slug: asset.slug.clone(),
            asset_symbol: asset.symbol.clone(),
            decimals: chain_map.decimals,
        });
    }

    rows.sort_by(|left, right| {
        left.contract_address
            .cmp(&right.contract_address)
            .then_with(|| left.asset_slug.cmp(&right.asset_slug))
    });
    rows.dedup_by(|left, right| left.contract_address == right.contract_address);
    rows
}

fn list_recommendations_in_memory(
    assets: &[GlobalAsset],
    normalized_query: &str,
    limit: usize,
) -> Vec<GlobalAsset> {
    let mut matches = assets
        .iter()
        .filter(|asset| asset_contains(asset, normalized_query))
        .cloned()
        .collect::<Vec<_>>();

    if matches.is_empty() {
        matches = assets.to_vec();
    }

    matches.sort_by(|left, right| {
        left.sort_order
            .cmp(&right.sort_order)
            .then_with(|| left.symbol.cmp(&right.symbol))
    });
    matches.truncate(limit);
    matches
}

fn list_assets_in_memory(assets: &[GlobalAsset], limit: usize) -> Vec<GlobalAsset> {
    let mut assets = assets.to_vec();
    assets.sort_by(|left, right| {
        left.sort_order
            .cmp(&right.sort_order)
            .then_with(|| left.symbol.to_lowercase().cmp(&right.symbol.to_lowercase()))
    });
    assets.truncate(limit);
    assets
}

fn asset_contains(asset: &GlobalAsset, normalized_query: &str) -> bool {
    let query = normalized_query.to_ascii_lowercase();
    asset.slug.to_ascii_lowercase().contains(&query)
        || asset.symbol.to_ascii_lowercase().contains(&query)
        || asset.name.to_ascii_lowercase().contains(&query)
        || asset
            .aliases
            .iter()
            .any(|alias| alias.to_ascii_lowercase().contains(&query))
}

fn escape_like_pattern(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

#[cfg(test)]
mod tests {
    use super::GlobalAssetRepository;
    use crate::adapters::postgres::asset_chain_map::in_memory_chain_map;
    use crate::test_utils::fixtures::global_assets::{sample_asset, sample_assets};

    #[tokio::test]
    async fn list_assets_truncates_to_limit() {
        let repository = GlobalAssetRepository::in_memory(sample_assets());

        let assets = repository.list_assets(2).await.unwrap();

        assert_eq!(assets.len(), 2);
        assert_eq!(assets[0].slug, "bitcoin");
        assert_eq!(assets[1].slug, "ethereum");
    }

    #[tokio::test]
    async fn list_assets_returns_assets_in_stable_order() {
        let repository = GlobalAssetRepository::in_memory(vec![
            sample_asset("zeta", "ZZZ", "Zeta", "crypto", "/assets/zeta", &[], 20),
            sample_asset("alpha", "BBB", "Alpha", "crypto", "/assets/alpha", &[], 10),
            sample_asset("beta", "AAA", "Beta", "crypto", "/assets/beta", &[], 10),
        ]);

        let assets = repository.list_assets(10).await.unwrap();

        assert_eq!(
            assets
                .into_iter()
                .map(|asset| asset.slug)
                .collect::<Vec<_>>(),
            ["beta", "alpha", "zeta"]
        );
    }

    #[tokio::test]
    async fn asset_detail_lookup_is_case_insensitive() {
        let repository = GlobalAssetRepository::in_memory(sample_assets());

        let detail = repository
            .get_asset_detail_by_slug("BitCoin")
            .await
            .unwrap()
            .unwrap();

        assert_eq!(detail.asset.slug, "bitcoin");
        assert_eq!(detail.chain_maps[0].network.slug, "bitcoin-mainnet");
        assert!(detail.chain_maps[0].is_native);
        assert_eq!(detail.chain_maps[0].address, None);
    }

    #[tokio::test]
    async fn asset_detail_returns_chain_maps_in_stable_order() {
        let repository = GlobalAssetRepository::in_memory_with_chain_maps(
            vec![sample_asset(
                "sample",
                "SMP",
                "Sample",
                "crypto",
                "/assets/sample",
                &[],
                10,
            )],
            vec![
                in_memory_chain_map(
                    "sample",
                    "zeta",
                    "Zeta",
                    Some("eip155:999"),
                    false,
                    Some("0x02"),
                    20,
                ),
                in_memory_chain_map(
                    "sample",
                    "alpha",
                    "Alpha",
                    Some("eip155:111"),
                    false,
                    Some("0x03"),
                    20,
                ),
                in_memory_chain_map("sample", "beta", "Beta", Some("eip155:222"), true, None, 10),
            ],
        );

        let detail = repository
            .get_asset_detail_by_slug("sample")
            .await
            .unwrap()
            .unwrap();

        assert_eq!(
            detail
                .chain_maps
                .into_iter()
                .map(|chain_map| chain_map.network.slug)
                .collect::<Vec<_>>(),
            ["beta", "alpha", "zeta"]
        );
    }
}
