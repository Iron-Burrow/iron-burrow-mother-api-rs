use serde::Serialize;

use super::asset_chain_map::AssetChainMap;

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
