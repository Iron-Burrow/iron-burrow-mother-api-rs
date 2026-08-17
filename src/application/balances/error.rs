use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GetBalancesCommandError {
    EmptyAccounts,
    EmptyTokens,
    RequestTooLarge,
    UnsupportedQuoteCurrency,
    InvalidAccount,
    DuplicateAccount,
    DuplicateAsset,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BalanceItemErrorCode {
    BalanceResolutionFailed,
    BalanceProviderUnavailable,
    PriceResolutionFailed,
    PriceProviderUnavailable,
    InternalError,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BalancePlanIssue {
    ResolutionCountMismatch,
    UnexpectedResolutionNetwork,
    InconsistentChainId,
    TargetCollision,
    ConflictingTargetMetadata,
}

#[derive(Debug)]
pub enum BalanceSnapshotServiceError {
    UnsupportedNetwork {
        network_slug: String,
    },
    UnsupportedAsset {
        network_slug: String,
        asset_slug: String,
    },
    RequestTooLarge {
        network_slug: String,
    },
    InvalidPlan {
        network_slug: String,
        issue: BalancePlanIssue,
    },
    ExecutionTaskFailed,
}

impl fmt::Display for BalanceSnapshotServiceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedNetwork { network_slug } => {
                write!(formatter, "unsupported balance network: {network_slug}")
            }
            Self::UnsupportedAsset {
                network_slug,
                asset_slug,
            } => write!(
                formatter,
                "unsupported balance asset {asset_slug} while planning network {network_slug}"
            ),
            Self::RequestTooLarge { network_slug } => {
                write!(
                    formatter,
                    "Bigwig balance group is too large: {network_slug}"
                )
            }
            Self::InvalidPlan {
                network_slug,
                issue,
            } => write!(
                formatter,
                "invalid balance orchestration plan for {network_slug}: {issue:?}"
            ),
            Self::ExecutionTaskFailed => write!(formatter, "balance orchestration task failed"),
        }
    }
}

impl std::error::Error for BalanceSnapshotServiceError {}
