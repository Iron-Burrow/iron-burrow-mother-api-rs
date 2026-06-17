use std::sync::Arc;

use serde::Serialize;
use sqlx::{FromRow, PgPool};

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
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct GlobalAsset {
    pub id: String,
    pub slug: String,
    pub symbol: String,
    pub name: String,
    pub category: String,
    pub canonical_path: String,
    pub aliases: Vec<String>,
    pub sort_order: i32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GlobalAssetDetail {
    pub asset: GlobalAsset,
    pub chain_maps: Vec<AssetChainMap>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssetChainMap {
    pub network: NetworkRef,
    pub is_native: bool,
    pub address: Option<String>,
    pub decimals: Option<i32>,
    pub token_standard: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NetworkRef {
    pub slug: String,
    pub name: String,
    pub caip2: Option<String>,
    pub family: String,
    pub chain_id: Option<i64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssetMatch {
    pub asset: GlobalAsset,
    pub confidence: MatchConfidence,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MatchConfidence {
    SlugExact,
    SymbolExact,
    NameExact,
    AliasExact,
}

impl MatchConfidence {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SlugExact => "slug_exact",
            Self::SymbolExact => "symbol_exact",
            Self::NameExact => "name_exact",
            Self::AliasExact => "alias_exact",
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct InMemoryGlobalAssets {
    assets: Vec<GlobalAsset>,
    chain_maps: Vec<InMemoryAssetChainMap>,
}

impl InMemoryGlobalAssets {
    fn new(assets: Vec<GlobalAsset>) -> Self {
        let chain_maps = demo_chain_maps_for_assets(&assets);

        Self { assets, chain_maps }
    }
}

#[derive(Clone, Debug)]
struct InMemoryAssetChainMap {
    asset_slug: String,
    chain_map: AssetChainMap,
    sort_order: i32,
}

#[derive(Debug)]
pub struct RepositoryError {
    source: sqlx::Error,
}

impl RepositoryError {
    fn new(source: sqlx::Error) -> Self {
        Self { source }
    }
}

impl std::fmt::Display for RepositoryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "global asset repository error: {}", self.source)
    }
}

impl std::error::Error for RepositoryError {}

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

#[derive(FromRow)]
struct AssetMatchRow {
    id: String,
    slug: String,
    symbol: String,
    name: String,
    category: Option<String>,
    canonical_path: String,
    aliases: Vec<String>,
    sort_order: i32,
    match_kind: String,
}

#[derive(FromRow)]
struct AssetChainMapRow {
    network_slug: String,
    network_name: String,
    network_caip2: Option<String>,
    network_family: String,
    network_chain_id: Option<i64>,
    is_native: bool,
    address: Option<String>,
    decimals: Option<i32>,
    token_standard: Option<String>,
}

#[allow(dead_code)]
#[derive(Clone, Debug, FromRow)]
pub(crate) struct BalanceCatalogRow {
    pub ordinal: i64,
    pub requested_asset_slug: String,
    pub network_slug: Option<String>,
    pub network_family: Option<String>,
    pub network_chain_id: Option<i64>,
    pub asset_slug: Option<String>,
    pub asset_symbol: Option<String>,
    pub asset_name: Option<String>,
    pub mapping_id: Option<String>,
    pub is_native: Option<bool>,
    pub deployment_address: Option<String>,
    pub decimals: Option<i32>,
    pub token_standard: Option<String>,
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

fn map_chain_map_row(row: AssetChainMapRow) -> AssetChainMap {
    AssetChainMap {
        network: NetworkRef {
            slug: row.network_slug,
            name: row.network_name,
            caip2: row.network_caip2,
            family: row.network_family,
            chain_id: row.network_chain_id,
        },
        is_native: row.is_native,
        address: row.address,
        decimals: row.decimals,
        token_standard: row.token_standard,
    }
}

fn map_match_row(row: AssetMatchRow) -> AssetMatch {
    let confidence = match row.match_kind.as_str() {
        "slug_exact" => MatchConfidence::SlugExact,
        "symbol_exact" => MatchConfidence::SymbolExact,
        "name_exact" => MatchConfidence::NameExact,
        _ => MatchConfidence::AliasExact,
    };

    AssetMatch {
        asset: GlobalAsset {
            id: row.id,
            slug: row.slug,
            symbol: row.symbol,
            name: row.name,
            category: row.category.unwrap_or_else(|| "asset".to_string()),
            canonical_path: row.canonical_path,
            aliases: row.aliases,
            sort_order: row.sort_order,
        },
        confidence,
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

fn find_confident_match_in_memory(
    assets: &[GlobalAsset],
    normalized_query: &str,
) -> Option<AssetMatch> {
    let mut candidates = assets
        .iter()
        .filter_map(|asset| {
            let confidence = if asset.slug.eq_ignore_ascii_case(normalized_query) {
                MatchConfidence::SlugExact
            } else if asset.symbol.eq_ignore_ascii_case(normalized_query) {
                MatchConfidence::SymbolExact
            } else if asset.name.eq_ignore_ascii_case(normalized_query) {
                MatchConfidence::NameExact
            } else if asset
                .aliases
                .iter()
                .any(|alias| alias.eq_ignore_ascii_case(normalized_query))
            {
                MatchConfidence::AliasExact
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

fn demo_chain_maps_for_assets(assets: &[GlobalAsset]) -> Vec<InMemoryAssetChainMap> {
    let mut chain_maps = Vec::new();

    for asset in assets {
        match asset.slug.as_str() {
            "bitcoin" => chain_maps.push(in_memory_chain_map(
                "bitcoin",
                "bitcoin-mainnet",
                "Bitcoin Mainnet",
                Some("bip122:000000000019d6689c085ae165831e93"),
                true,
                None,
                10,
            )),
            "ethereum" => chain_maps.extend([
                in_memory_chain_map(
                    "ethereum",
                    "eth-mainnet",
                    "Ethereum Mainnet",
                    Some("eip155:1"),
                    true,
                    None,
                    20,
                ),
                in_memory_chain_map(
                    "ethereum",
                    "arbitrum-mainnet",
                    "Arbitrum One",
                    Some("eip155:42161"),
                    true,
                    None,
                    30,
                ),
                in_memory_chain_map(
                    "ethereum",
                    "base-mainnet",
                    "Base",
                    Some("eip155:8453"),
                    true,
                    None,
                    40,
                ),
            ]),
            "usdc" => chain_maps.extend([
                in_memory_chain_map(
                    "usdc",
                    "eth-mainnet",
                    "Ethereum Mainnet",
                    Some("eip155:1"),
                    false,
                    Some("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"),
                    240,
                ),
                in_memory_chain_map(
                    "usdc",
                    "arbitrum-mainnet",
                    "Arbitrum One",
                    Some("eip155:42161"),
                    false,
                    Some("0xaf88d065e77c8cc2239327c5edb3a432268e5831"),
                    250,
                ),
                in_memory_chain_map(
                    "usdc",
                    "base-mainnet",
                    "Base",
                    Some("eip155:8453"),
                    false,
                    Some("0x833589fcd6edb6e08f4c7c32d4f71b54bda02913"),
                    260,
                ),
                in_memory_chain_map(
                    "usdc",
                    "near",
                    "NEAR Mainnet",
                    Some("near:mainnet"),
                    false,
                    Some("17208628f84f5d6ad33f0da3bbbeb27ffcb398eac501a31bd6ad2011e36133a1"),
                    270,
                ),
                in_memory_chain_map(
                    "usdc",
                    "mantle-mainnet",
                    "Mantle",
                    Some("eip155:5000"),
                    false,
                    Some("0x09bc4e0d864854c6afb6eb9a9cdf58ac190d0df9"),
                    280,
                ),
            ]),
            "mantle" => chain_maps.push(in_memory_chain_map(
                "mantle",
                "mantle-mainnet",
                "Mantle",
                Some("eip155:5000"),
                true,
                None,
                50,
            )),
            _ => {}
        }
    }

    chain_maps
}

fn in_memory_chain_map(
    asset_slug: &str,
    network_slug: &str,
    network_name: &str,
    caip2: Option<&str>,
    is_native: bool,
    address: Option<&str>,
    sort_order: i32,
) -> InMemoryAssetChainMap {
    let (family, chain_id) = match caip2 {
        Some(value) if value.starts_with("eip155:") => (
            "evm",
            value
                .strip_prefix("eip155:")
                .and_then(|chain_id| chain_id.parse::<i64>().ok()),
        ),
        Some(value) if value.starts_with("bip122:") => ("bitcoin", None),
        Some(value) if value.starts_with("near:") => ("near", None),
        _ => ("unknown", None),
    };
    let decimals = match asset_slug {
        "bitcoin" => 8,
        "usdc" => 6,
        _ => 18,
    };
    let token_standard = if is_native {
        "native"
    } else if family == "evm" {
        "erc20"
    } else {
        "nep141"
    };

    InMemoryAssetChainMap {
        asset_slug: asset_slug.to_string(),
        chain_map: AssetChainMap {
            network: NetworkRef {
                slug: network_slug.to_string(),
                name: network_name.to_string(),
                caip2: caip2.map(str::to_string),
                family: family.to_string(),
                chain_id,
            },
            is_native,
            address: address.map(str::to_string),
            decimals: Some(decimals),
            token_standard: Some(token_standard.to_string()),
        },
        sort_order,
    }
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

fn confidence_rank(confidence: MatchConfidence) -> u8 {
    match confidence {
        MatchConfidence::SlugExact => 0,
        MatchConfidence::SymbolExact => 1,
        MatchConfidence::NameExact => 2,
        MatchConfidence::AliasExact => 3,
    }
}

fn escape_like_pattern(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn list_assets_returns_assets_in_stable_order() {
        let repository = GlobalAssetRepository::in_memory(vec![
            demo_asset("zeta", "ZZZ", "Zeta", "crypto", "/assets/zeta", &[], 20),
            demo_asset("alpha", "BBB", "Alpha", "crypto", "/assets/alpha", &[], 10),
            demo_asset("beta", "AAA", "Beta", "crypto", "/assets/beta", &[], 10),
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
    async fn list_assets_truncates_to_limit() {
        let repository = GlobalAssetRepository::in_memory(demo_assets());

        let assets = repository.list_assets(2).await.unwrap();

        assert_eq!(assets.len(), 2);
        assert_eq!(assets[0].slug, "bitcoin");
        assert_eq!(assets[1].slug, "ethereum");
    }

    #[tokio::test]
    async fn asset_detail_lookup_is_case_insensitive() {
        let repository = GlobalAssetRepository::in_memory(demo_assets());

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
            vec![demo_asset(
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

        sqlx::raw_sql(include_str!(
            "../../migrations/0005_canonical_evm_network_slugs.sql"
        ))
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
            "../../migrations/0005_canonical_evm_network_slugs.sql"
        ))
        .execute(&mut *transaction)
        .await;

        assert!(result.is_err());
        transaction.rollback().await.unwrap();
    }
}

#[cfg(test)]
pub fn demo_assets() -> Vec<GlobalAsset> {
    vec![
        demo_asset(
            "bitcoin",
            "BTC",
            "Bitcoin",
            "crypto",
            "/assets/bitcoin",
            &["btc", "bitcoin", "bit coin"],
            10,
        ),
        demo_asset(
            "ethereum",
            "ETH",
            "Ethereum",
            "crypto",
            "/assets/ethereum",
            &["eth", "ether", "ethereum"],
            20,
        ),
        demo_asset(
            "usdc",
            "USDC",
            "USD Coin",
            "crypto",
            "/assets/usdc",
            &[
                "usdc",
                "usd coin",
                "usdc coin",
                "usdc coin usd",
                "circle usd coin",
                "circle usdc",
                "dollar coin",
            ],
            30,
        ),
        demo_asset(
            "wrapped-bitcoin",
            "WBTC",
            "Wrapped Bitcoin",
            "crypto",
            "/assets/wrapped-bitcoin",
            &["wbtc", "wrapped bitcoin", "wrapped btc"],
            35,
        ),
        demo_asset(
            "gold",
            "XAU",
            "Gold",
            "commodity",
            "/assets/gold",
            &[
                "gold",
                "oro",
                "oro de ley",
                "xau",
                "precious metal",
                "metal precioso",
            ],
            40,
        ),
        demo_asset(
            "mantle",
            "MNT",
            "Mantle",
            "crypto",
            "/assets/mantle",
            &["mnt", "mantle"],
            50,
        ),
        demo_asset(
            "near",
            "NEAR",
            "NEAR Protocol",
            "crypto",
            "/assets/near",
            &["near", "near protocol"],
            60,
        ),
        demo_asset(
            "aave",
            "AAVE",
            "Aave",
            "crypto",
            "/assets/aave",
            &["aave"],
            70,
        ),
        demo_asset(
            "ausd",
            "AUSD",
            "AUSD",
            "stablecoin",
            "/assets/ausd",
            &["ausd"],
            80,
        ),
        demo_asset(
            "usds",
            "USDS",
            "Sky Dollar",
            "stablecoin",
            "/assets/usds",
            &["usds", "sky dollar"],
            90,
        ),
        demo_asset(
            "fbtc",
            "FBTC",
            "FBTC",
            "crypto",
            "/assets/fbtc",
            &["fbtc"],
            100,
        ),
        demo_asset(
            "gho",
            "GHO",
            "GHO",
            "stablecoin",
            "/assets/gho",
            &["gho"],
            110,
        ),
        demo_asset(
            "mpdao",
            "MPDAO",
            "MPDAO",
            "crypto",
            "/assets/mpdao",
            &["mpdao"],
            120,
        ),
        demo_asset(
            "stnear",
            "STNEAR",
            "Staked NEAR",
            "crypto",
            "/assets/stnear",
            &["stnear", "staked near"],
            130,
        ),
        demo_asset(
            "usdt",
            "USDT",
            "Tether USD",
            "stablecoin",
            "/assets/usdt",
            &["usdt", "tether", "tether usd"],
            140,
        ),
        demo_asset(
            "usdt0",
            "USDT0",
            "USDT0",
            "stablecoin",
            "/assets/usdt0",
            &["usdt0", "usdt zero"],
            150,
        ),
        demo_asset(
            "usde",
            "USDe",
            "USDe",
            "stablecoin",
            "/assets/usde",
            &["usde"],
            160,
        ),
        demo_asset(
            "wrapped-ether",
            "WETH",
            "Wrapped Ether",
            "crypto",
            "/assets/wrapped-ether",
            &["weth", "wrapped ether", "wrapped eth"],
            170,
        ),
        demo_asset(
            "cmeth",
            "cmETH",
            "cmETH",
            "crypto",
            "/assets/cmeth",
            &["cmeth", "cmeth token"],
            180,
        ),
        demo_asset(
            "meth",
            "mETH",
            "mETH",
            "crypto",
            "/assets/meth",
            &["meth", "meth token"],
            190,
        ),
        demo_asset(
            "susde",
            "sUSDe",
            "sUSDe",
            "stablecoin",
            "/assets/susde",
            &["susde", "staked usde"],
            200,
        ),
    ]
}

#[cfg(test)]
fn demo_asset(
    slug: &str,
    symbol: &str,
    name: &str,
    category: &str,
    canonical_path: &str,
    aliases: &[&str],
    sort_order: i32,
) -> GlobalAsset {
    GlobalAsset {
        id: format!("test-{slug}"),
        slug: slug.to_string(),
        symbol: symbol.to_string(),
        name: name.to_string(),
        category: category.to_string(),
        canonical_path: canonical_path.to_string(),
        aliases: aliases.iter().map(|alias| alias.to_string()).collect(),
        sort_order,
    }
}
