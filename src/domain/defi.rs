#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RealizedYieldProtocol {
    pub(crate) slug: String,
    pub(crate) network_slug: String,
    pub(crate) chain_id: i64,
    pub(crate) adapter_kind: String,
    pub(crate) adapter_version: String,
    pub(crate) pool_address: String,
    pub(crate) reserves: Vec<RealizedYieldReserve>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RealizedYieldReserve {
    pub(crate) asset_slug: String,
    pub(crate) asset_symbol: String,
    pub(crate) underlying_asset_address: String,
}
