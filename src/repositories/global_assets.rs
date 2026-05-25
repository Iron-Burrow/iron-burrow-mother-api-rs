use std::sync::Arc;

use serde::Serialize;
use sqlx::{FromRow, PgPool};

#[derive(Clone, Debug)]
pub enum GlobalAssetRepository {
    Database(PgPool),
    InMemory(Arc<Vec<GlobalAsset>>),
}

impl GlobalAssetRepository {
    pub fn database(pool: PgPool) -> Self {
        Self::Database(pool)
    }

    pub fn in_memory(assets: Vec<GlobalAsset>) -> Self {
        Self::InMemory(Arc::new(assets))
    }

    pub async fn find_confident_match(
        &self,
        normalized_query: &str,
    ) -> Result<Option<AssetMatch>, RepositoryError> {
        match self {
            Self::Database(pool) => find_confident_match_db(pool, normalized_query).await,
            Self::InMemory(assets) => Ok(find_confident_match_in_memory(assets, normalized_query)),
        }
    }

    pub async fn list_recommendations(
        &self,
        normalized_query: &str,
        limit: i64,
    ) -> Result<Vec<GlobalAsset>, RepositoryError> {
        match self {
            Self::Database(pool) => list_recommendations_db(pool, normalized_query, limit).await,
            Self::InMemory(assets) => Ok(list_recommendations_in_memory(
                assets,
                normalized_query,
                limit as usize,
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
