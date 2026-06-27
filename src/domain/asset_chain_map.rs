use crate::domain::networks::NetworkRef;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssetChainMap {
    pub network: NetworkRef,
    pub is_native: bool,
    pub address: Option<String>,
    pub decimals: Option<i32>,
    pub token_standard: Option<String>,
}
