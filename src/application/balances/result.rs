use crate::{
    application::balances::error::BalanceItemErrorCode,
    domain::{
        accounts::OnchainAccount, assets::balance_catalog::BalanceTargetKind,
        onchain_time::as_of::AsOf,
    },
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct GetBalancesResult {
    pub as_of: AsOf,
    pub quote_currency: String,
    pub requested_token_count: usize,
    pub accounts: Vec<BalancesAccountResult>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BalancesAccountResult {
    pub account: OnchainAccount,
    pub evidence: Option<BalanceEvidence>,
    pub items: Vec<BalanceItemOutcome>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BalanceEvidence {
    pub network_slug: String,
    pub observed_at: String,
    pub block_number: String,
    pub block_hash: String,
    pub block_timestamp: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BalanceItemOutcome {
    Resolved {
        target: ResolvedBalanceTarget,
        raw_amount: String,
        amount: Option<String>,
        quote: BalanceQuoteOutcome,
    },
    Skipped {
        network_slug: String,
        asset_slug: String,
    },
    Failed {
        target: ResolvedBalanceTarget,
        code: BalanceItemErrorCode,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedBalanceTarget {
    pub selector: BalanceTokenSelector,
    pub network_slug: String,
    pub chain_id: i64,
    pub asset_slug: Option<String>,
    pub symbol: Option<String>,
    pub name: Option<String>,
    pub decimals: Option<u8>,
    pub pricing_asset_slug: Option<String>,
    pub kind: BalanceTargetKind,
}

impl ResolvedBalanceTarget {
    pub(crate) fn contract_address(&self) -> Option<String> {
        match &self.kind {
            BalanceTargetKind::Native => None,
            BalanceTargetKind::Erc20 { contract_address } => Some(contract_address.clone()),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BalanceTokenSelector {
    AssetSlug(String),
    ContractAddress(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BalanceQuoteOutcome {
    Available {
        currency: String,
        unit_price: String,
        value: String,
        price_as_of: String,
    },
    Unavailable {
        code: BalanceItemErrorCode,
    },
    Unsupported,
}
