use crate::{
    adapters::http::error::ApiError, application::balances::error::GetBalancesCommandError,
};

pub(super) fn command_error_to_api_error(error: GetBalancesCommandError) -> ApiError {
    match error {
        GetBalancesCommandError::EmptyAccounts => ApiError::empty_accounts(),
        GetBalancesCommandError::EmptyTokens => ApiError::empty_tokens(),
        GetBalancesCommandError::RequestTooLarge => ApiError::request_too_large(),
        GetBalancesCommandError::UnsupportedQuoteCurrency => ApiError::unsupported_quote_currency(),
        GetBalancesCommandError::InvalidAccount => ApiError::invalid_account(),
        GetBalancesCommandError::DuplicateAccount => ApiError::duplicate_account(),
        GetBalancesCommandError::DuplicateAsset => ApiError::duplicate_asset(),
    }
}
