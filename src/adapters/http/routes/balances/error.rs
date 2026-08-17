use tracing::warn;

use crate::adapters::http::error::ApiError;
use crate::adapters::http::presenters::balances::error::BalancesResponsePresenterError;
use crate::application::balances::error::BalanceSnapshotServiceError;

pub(super) fn balance_service_error_to_api_error(error: BalanceSnapshotServiceError) -> ApiError {
    match error {
        BalanceSnapshotServiceError::UnsupportedNetwork { .. } => ApiError::unsupported_network(),
        BalanceSnapshotServiceError::UnsupportedAsset { .. } => ApiError::unsupported_asset(),
        BalanceSnapshotServiceError::RequestTooLarge { .. } => ApiError::request_too_large(),
        BalanceSnapshotServiceError::InvalidPlan {
            network_slug,
            issue,
        } => {
            warn!(
                network_slug,
                ?issue,
                "Balance orchestration plan is invalid"
            );
            ApiError::internal_error()
        }
        BalanceSnapshotServiceError::ExecutionTaskFailed => {
            warn!("Balance orchestration task failed");
            ApiError::internal_error()
        }
    }
}

pub(super) fn balance_assembler_error_to_api_error(
    error: BalancesResponsePresenterError,
) -> ApiError {
    warn!(?error, "Balance response assembly failed");
    ApiError::internal_error()
}
