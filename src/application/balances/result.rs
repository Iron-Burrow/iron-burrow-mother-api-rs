use crate::{
    application::balances::service::{BalanceEvidence, BalanceItemOutcome},
    domain::{accounts::OnchainAccount, onchain_time::as_of::AsOf},
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
