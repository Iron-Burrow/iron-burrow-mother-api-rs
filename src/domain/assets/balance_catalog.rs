use crate::adapters::postgres::errors::RepositoryError;

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

#[derive(Debug, thiserror::Error)]
pub(crate) enum CatalogResolverError {
    #[error("balance catalog lookup failed: {0}")]
    Repository(RepositoryError),
    #[error("invalid balance catalog for network {network_slug}")]
    InvalidCatalog {
        network_slug: String,
        asset_slug: Option<String>,
        issue: CatalogIntegrityIssue,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CatalogIntegrityIssue {
    MissingLookupRow,
    UnexpectedLookupRow,
    InvalidChainId,
    AmbiguousMapping,
    IncompleteMapping,
    InvalidDecimals,
    ContradictoryNativeMapping,
    MalformedErc20Address,
}

impl From<RepositoryError> for CatalogResolverError {
    fn from(error: RepositoryError) -> Self {
        Self::Repository(error)
    }
}
