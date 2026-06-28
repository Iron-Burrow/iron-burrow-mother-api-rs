use serde::Serialize;

use super::asset_chain_map::AssetChainMap;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct GlobalAsset {
    pub(crate) id: String,
    pub(crate) slug: String,
    pub(crate) symbol: String,
    pub(crate) name: String,
    pub(crate) category: String,
    pub(crate) canonical_path: String,
    pub(crate) aliases: Vec<String>,
    pub(crate) sort_order: i32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct GlobalAssetDetail {
    pub(crate) asset: GlobalAsset,
    pub(crate) chain_maps: Vec<AssetChainMap>,
}
