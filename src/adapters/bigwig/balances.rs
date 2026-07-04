use serde::{de, Deserialize, Deserializer, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BigwigRequest {
    pub network_slug: String,
    pub accounts: Vec<BigwigAccount>,
    pub targets: Vec<BigwigTarget>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BigwigAccount {
    pub address: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum BigwigTarget {
    Native,
    Erc20 { contract_address: String },
}

impl<'de> Deserialize<'de> for BigwigTarget {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireTarget {
            kind: String,
            contract_address: Option<String>,
        }

        let target = WireTarget::deserialize(deserializer)?;
        match (target.kind.as_str(), target.contract_address) {
            ("native", None) => Ok(Self::Native),
            ("erc20", Some(contract_address)) => Ok(Self::Erc20 { contract_address }),
            ("native" | "erc20", _) => Err(de::Error::custom("invalid balance target shape")),
            _ => Err(de::Error::custom("unknown balance target kind")),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct BigwigResponse {
    pub primitive: BigwigPrimitive,
    pub status: BigwigEvidenceStatus,
    pub network: BigwigEvidenceNetwork,
    pub observed_at: String,
    pub block: BigwigEvidenceBlock,
    pub items: Vec<BigwigEvidenceItem>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
pub enum BigwigPrimitive {
    #[serde(rename = "evm_latest_balances")]
    EvmLatestBalances,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum BigwigEvidenceStatus {
    Complete,
    Partial,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct BigwigEvidenceNetwork {
    pub network_slug: String,
    pub chain_id: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct BigwigEvidenceBlock {
    pub number: String,
    pub hash: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BigwigEvidenceItem {
    Resolved {
        account: BigwigAccount,
        target: BigwigTarget,
        raw_amount: String,
    },
    Failed {
        account: BigwigAccount,
        target: BigwigTarget,
        error: BigwigItemError,
    },
}

impl<'de> Deserialize<'de> for BigwigEvidenceItem {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "lowercase")]
        enum WireStatus {
            Resolved,
            Failed,
        }

        #[derive(Deserialize)]
        struct WireItem {
            status: WireStatus,
            account: BigwigAccount,
            target: BigwigTarget,
            raw_amount: Option<String>,
            error: Option<BigwigItemError>,
        }

        let item = WireItem::deserialize(deserializer)?;
        match (item.status, item.raw_amount, item.error) {
            (WireStatus::Resolved, Some(raw_amount), None) => Ok(Self::Resolved {
                account: item.account,
                target: item.target,
                raw_amount,
            }),
            (WireStatus::Failed, None, Some(error)) => Ok(Self::Failed {
                account: item.account,
                target: item.target,
                error,
            }),
            _ => Err(de::Error::custom("invalid balance evidence item shape")),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct BigwigItemError {
    pub code: BigwigItemErrorCode,
    pub message: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum BigwigItemErrorCode {
    NativeBalanceCallFailed,
    Erc20BalanceCallFailed,
    Erc20BadResponse,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BigwigRequestValidationCode {
    MalformedBody,
    EmptyAccounts,
    EmptyTargets,
    InvalidAccount,
    DuplicateAccount,
    InvalidTarget,
    DuplicateTarget,
    RequestTooLarge,
}

#[cfg(test)]
mod tests;
