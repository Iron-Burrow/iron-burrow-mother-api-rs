use crate::{
    application::balances::{
        error::BalanceItemErrorCode,
        result::{BalanceEvidence, ResolvedBalanceTarget},
    },
    domain::accounts::OnchainAccount,
};

#[derive(Clone, Debug)]
pub(super) struct RawBalancesAccountResult {
    pub(super) account: OnchainAccount,
    pub(super) evidence: Option<BalanceEvidence>,
    pub(super) items: Vec<RawBalanceItemOutcome>,
}

#[derive(Clone, Debug)]
pub(super) enum RawBalanceItemOutcome {
    Resolved {
        target: ResolvedBalanceTarget,
        raw_amount: String,
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
