#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BalanceTarget {
    pub(crate) network_slug: String,
    pub(crate) chain_id: i64,
    pub(crate) asset_slug: String,
    pub(crate) symbol: String,
    pub(crate) name: String,
    pub(crate) decimals: u8,
    pub(crate) pricing_asset_slug: String,
    pub(crate) kind: BalanceTargetKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum BalanceTargetKind {
    Native,
    Erc20 { contract_address: String },
}
