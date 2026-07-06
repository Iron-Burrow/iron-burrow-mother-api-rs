use crate::domain::networks::NetworkRef;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AssetChainMap {
    pub(crate) network: NetworkRef,
    pub(crate) is_native: bool,
    pub(crate) address: Option<String>,
    pub(crate) decimals: Option<i32>,
    pub(crate) token_standard: Option<String>,
}
