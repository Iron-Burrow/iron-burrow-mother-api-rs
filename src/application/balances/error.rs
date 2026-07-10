use std::fmt;

use crate::domain::assets::balance_catalog::CatalogResolverError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BalanceItemErrorCode {
    BalanceResolutionFailed,
    BalanceProviderUnavailable,
    PriceResolutionFailed,
    PriceProviderUnavailable,
    InternalError,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BalancePlanIssue {
    ResolutionCountMismatch,
    UnexpectedResolutionNetwork,
    InconsistentChainId,
    TargetCollision,
    ConflictingTargetMetadata,
}

#[derive(Debug)]
pub enum BalanceSnapshotServiceError {
    Catalog(CatalogResolverError),
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

impl From<CatalogResolverError> for BalanceSnapshotServiceError {
    fn from(error: CatalogResolverError) -> Self {
        Self::Catalog(error)
    }
}

impl fmt::Display for BalanceSnapshotServiceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Catalog(error) => write!(formatter, "balance catalog resolution failed: {error}"),
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
