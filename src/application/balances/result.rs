use crate::{
    application::balances::service::BalanceAccountResult, domain::onchain_time::as_of::AsOf,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct GetBalancesResult {
    pub as_of: AsOf,
    pub quote_currency: String,
    pub requested_token_count: usize,
    pub accounts: Vec<BalanceAccountResult>,
}
