use serde::{de, Deserialize, Deserializer, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BigwigRequest {
    pub network_slug: String,
    pub as_of: BigwigAsOf,
    pub accounts: Vec<String>,
    pub tokens: Vec<BigwigTarget>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BigwigAsOf {
    Latest,
    Timestamp { timestamp: String },
    BlockNumber { block_number: String },
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
    pub requested_as_of: BigwigAsOf,
    pub resolved_evidence: BigwigResolvedEvidence,
    pub items: Vec<BigwigEvidenceItem>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
pub enum BigwigPrimitive {
    #[serde(rename = "evm_balances")]
    EvmBalances,
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
pub struct BigwigResolvedEvidence {
    pub kind: BigwigResolvedEvidenceKind,
    pub block_number: String,
    pub block_hash: String,
    pub block_timestamp: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum BigwigResolvedEvidenceKind {
    ExactBlock,
    ResolvedTimestamp,
    ObservedHead,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BigwigEvidenceItem {
    Resolved {
        account_address: String,
        target: BigwigTarget,
        raw_amount: String,
    },
    Failed {
        account_address: String,
        target: BigwigTarget,
        error: BigwigItemError,
    },
    Unavailable {
        account_address: String,
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
            Unavailable,
        }

        #[derive(Deserialize)]
        struct WireRawBalance {
            status: WireStatus,
            value: Option<String>,
            error: Option<BigwigItemError>,
        }

        #[derive(Deserialize)]
        struct WireItem {
            account_address: String,
            requested_token: BigwigTarget,
            raw_balance: WireRawBalance,
        }

        let item = WireItem::deserialize(deserializer)?;
        match (
            item.raw_balance.status,
            item.raw_balance.value,
            item.raw_balance.error,
        ) {
            (WireStatus::Resolved, Some(raw_amount), None) => Ok(Self::Resolved {
                account_address: item.account_address,
                target: item.requested_token,
                raw_amount,
            }),
            (WireStatus::Failed, None, Some(error)) => Ok(Self::Failed {
                account_address: item.account_address,
                target: item.requested_token,
                error,
            }),
            (WireStatus::Unavailable, None, Some(error)) => Ok(Self::Unavailable {
                account_address: item.account_address,
                target: item.requested_token,
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
    Erc20ContractCodeAbsentAtEvidenceBlock,
    Erc20BalanceofNotSupported,
    HistoricalEvidenceUnavailable,
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
