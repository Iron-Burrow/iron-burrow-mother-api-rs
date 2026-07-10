use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{application::balances::result::BalanceEvidence, domain::accounts::OnchainAccount};

#[allow(dead_code)]
pub(crate) mod examples;
pub(crate) mod requests;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub(crate) enum BalanceResponseStatus {
    Complete,
    Partial,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
pub struct SingleBalanceResponse {
    pub(crate) ok: bool,
    #[serde(rename = "type")]
    pub(crate) response_type: String,
    pub(crate) status: BalanceResponseStatus,
    pub(crate) as_of: BalanceAsOfPayload,
    pub(crate) quote_currency: String,
    pub(crate) account: BalanceAccountIdentityPayload,
    pub(crate) evidence: Option<BalanceEvidencePayload>,
    pub(crate) positions: Vec<BalancePositionPayload>,
    pub(crate) skipped: Vec<BalanceSkippedPayload>,
    pub(crate) errors: Vec<BalanceErrorPayload>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
pub struct BulkBalanceResponse {
    pub(crate) ok: bool,
    #[serde(rename = "type")]
    pub(crate) response_type: String,
    pub(crate) status: BalanceResponseStatus,
    pub(crate) as_of: BalanceAsOfPayload,
    pub(crate) quote_currency: String,
    pub(crate) summary: BalanceSummaryPayload,
    pub(crate) accounts: Vec<BalanceAccountPayload>,
    pub(crate) errors: Vec<BalanceErrorPayload>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
pub(crate) struct BalanceAsOfPayload {
    pub(crate) kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) timestamp: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) block_number: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) observed_at: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
pub(crate) struct BalanceSummaryPayload {
    pub(crate) requested_accounts: usize,
    pub(crate) requested_assets: usize,
    pub(crate) requested_resolution_items: usize,
    pub(crate) positions_returned: usize,
    pub(crate) skipped_items: usize,
    pub(crate) failed_items: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
pub(crate) struct BalanceAccountPayload {
    pub(crate) status: BalanceResponseStatus,
    pub(crate) account: BalanceAccountIdentityPayload,
    pub(crate) evidence: Option<BalanceEvidencePayload>,
    pub(crate) positions: Vec<BalancePositionPayload>,
    pub(crate) skipped: Vec<BalanceSkippedPayload>,
    pub(crate) errors: Vec<BalanceErrorPayload>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
pub(crate) struct BalanceAccountIdentityPayload {
    pub(crate) network_slug: String,
    pub(crate) address: String,
    pub(crate) client_ref: Option<String>,
}

impl From<OnchainAccount> for BalanceAccountIdentityPayload {
    fn from(account: OnchainAccount) -> Self {
        Self {
            network_slug: account.network_slug,
            address: account.address,
            client_ref: account.client_ref,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
pub(crate) struct BalanceEvidencePayload {
    pub(crate) source: String,
    pub(crate) network_slug: String,
    pub(crate) block: BalanceBlockPayload,
    pub(crate) observed_at: String,
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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
pub(crate) struct BalanceBlockPayload {
    number: String,
    hash: String,
    timestamp: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
pub(crate) struct BalancePositionPayload {
    pub selector: BalanceSelectorPayload,
    pub network_slug: String,
    pub contract_address: Option<String>,
    pub asset_slug: Option<String>,
    pub symbol: Option<String>,
    pub balance: BalanceAmountPayload,
    pub quote: BalanceQuotePayload,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
pub(crate) struct BalanceSelectorPayload {
    pub(crate) kind: String,
    pub(crate) value: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
pub(crate) struct BalanceAmountPayload {
    pub raw_amount: String,
    pub amount: Option<String>,
    pub decimals: Option<u8>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
pub(crate) struct BalanceQuotePayload {
    pub(crate) status: BalanceQuoteStatus,
    pub(crate) currency: Option<String>,
    pub(crate) unit_price: Option<String>,
    pub(crate) value: Option<String>,
    pub(crate) price_as_of: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub(crate) enum BalanceQuoteStatus {
    Available,
    Unavailable,
    Unsupported,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
pub(crate) struct BalanceSkippedPayload {
    pub(crate) network_slug: String,
    pub(crate) asset_slug: String,
    pub(crate) reason: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
pub(crate) struct BalanceErrorPayload {
    pub(crate) network_slug: String,
    pub(crate) selector: BalanceSelectorPayload,
    pub(crate) contract_address: Option<String>,
    pub(crate) asset_slug: Option<String>,
    pub(crate) code: String,
    pub(crate) message: String,
}

#[cfg(test)]
mod tests;
