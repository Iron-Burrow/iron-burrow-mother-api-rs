use sqlx::FromRow;

use crate::domain::asset_match::{AssetMatch, MatchConfidence};
use crate::domain::global_assets::GlobalAsset;

#[derive(FromRow)]
pub(super) struct AssetMatchRow {
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

pub(super) fn map_match_row(row: AssetMatchRow) -> AssetMatch {
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
