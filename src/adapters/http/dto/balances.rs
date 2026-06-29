use std::collections::HashMap;

use serde::{de::IgnoredAny, Deserialize};

pub type ExtraFields = HashMap<String, IgnoredAny>;

#[derive(Debug, Deserialize)]
pub struct SingleBalanceRequest {
    pub(crate) as_of: BalanceAsOfRequest,
    pub(crate) account: BalanceAccountRequest,
    pub(crate) quote_currency: String,
    pub(crate) assets: Vec<BalanceAssetRequest>,
    #[serde(default, flatten)]
    pub(crate) extra: ExtraFields,
}

#[derive(Debug, Deserialize)]
pub struct BulkBalanceRequest {
    pub(crate) as_of: BalanceAsOfRequest,
    pub(crate) accounts: Vec<BalanceAccountRequest>,
    pub(crate) quote_currency: String,
    pub(crate) assets: Vec<BalanceAssetRequest>,
    #[serde(default, flatten)]
    pub(crate) extra: ExtraFields,
}

#[derive(Debug, Deserialize)]
pub struct BalanceAsOfRequest {
    pub(crate) kind: String,
}

#[derive(Debug, Deserialize)]
pub struct BalanceAccountRequest {
    pub(crate) network_slug: String,
    pub(crate) address: String,
    pub(crate) client_ref: Option<String>,
    #[serde(default, flatten)]
    pub(crate) extra: ExtraFields,
}

#[derive(Clone, Debug, Deserialize)]
pub struct BalanceAssetRequest {
    pub(crate) asset_slug: String,
}
