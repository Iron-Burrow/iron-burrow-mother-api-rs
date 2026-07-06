use sqlx::FromRow;

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

#[allow(dead_code)]
#[derive(Clone, Debug, Eq, FromRow, PartialEq)]
pub(crate) struct BalanceNetworkCatalogRow {
    pub network_slug: String,
    pub network_family: String,
    pub network_chain_id: Option<i64>,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Eq, FromRow, PartialEq)]
pub(crate) struct Erc20TokenCatalogRow {
    pub contract_address: String,
    pub network_slug: String,
    pub network_chain_id: Option<i64>,
    pub asset_slug: String,
    pub asset_symbol: String,
    pub asset_name: String,
    pub decimals: Option<i32>,
}
