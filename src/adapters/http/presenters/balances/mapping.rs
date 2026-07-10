use crate::{
    adapters::http::dto::balances::{
        BalanceAccountIdentityPayload, BalanceBlockPayload, BalanceEvidencePayload,
        BalanceSelectorPayload,
    },
    application::balances::result::{BalanceEvidence, BalanceTokenSelector},
    domain::accounts::OnchainAccount,
};

impl From<OnchainAccount> for BalanceAccountIdentityPayload {
    fn from(account: OnchainAccount) -> Self {
        Self {
            network_slug: account.network_slug,
            address: account.address,
            client_ref: account.client_ref,
        }
    }
}

impl From<BalanceEvidence> for BalanceEvidencePayload {
    fn from(evidence: BalanceEvidence) -> Self {
        Self {
            source: "bigwig".to_string(),
            network_slug: evidence.network_slug,
            block: BalanceBlockPayload {
                number: evidence.block_number,
                hash: evidence.block_hash,
                timestamp: evidence.block_timestamp,
            },
            observed_at: evidence.observed_at,
        }
    }
}

impl From<BalanceTokenSelector> for BalanceSelectorPayload {
    fn from(selector: BalanceTokenSelector) -> Self {
        match selector {
            BalanceTokenSelector::AssetSlug(asset_slug) => BalanceSelectorPayload {
                kind: "asset_slug".to_string(),
                value: asset_slug.clone(),
            },
            BalanceTokenSelector::ContractAddress(contract_address) => BalanceSelectorPayload {
                kind: "contract_address".to_string(),
                value: contract_address.clone(),
            },
        }
    }
}
