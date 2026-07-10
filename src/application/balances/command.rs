use std::collections::HashSet;

use super::error::GetBalancesCommandError;
use crate::domain::{
    accounts::OnchainAccount, assets::token_selector::TokenSelector, onchain_time::as_of::AsOf,
    validation::is_evm_address,
};

pub(crate) const MAX_ACCOUNTS: usize = 50;
pub(crate) const MAX_TOKENS: usize = 20;
pub(crate) const MAX_RESOLUTION_ITEMS: usize = 1_000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GetBalancesCommand {
    as_of: AsOf,
    accounts: Vec<OnchainAccount>,
    tokens: TokenSelector,
    quote_currency: String,
}

impl GetBalancesCommand {
    pub(crate) fn try_new(
        as_of: AsOf,
        accounts: Vec<OnchainAccount>,
        quote_currency: String,
        tokens: TokenSelector,
    ) -> Result<Self, GetBalancesCommandError> {
        if accounts.is_empty() {
            return Err(GetBalancesCommandError::EmptyAccounts);
        }

        if tokens.asset_slugs.is_empty() && tokens.contract_addresses.is_empty() {
            return Err(GetBalancesCommandError::EmptyTokens);
        }

        let mut seen_contracts = HashSet::<String>::with_capacity(tokens.contract_addresses.len());

        let contract_addresses = tokens
            .contract_addresses
            .into_iter()
            .map(|address| address.to_ascii_lowercase())
            .filter(|address| seen_contracts.insert(address.clone()))
            .collect::<Vec<_>>();

        let token_count = tokens.asset_slugs.len() + contract_addresses.len();

        let resolution_items = accounts
            .len()
            .checked_mul(token_count)
            .ok_or(GetBalancesCommandError::RequestTooLarge)?;

        if accounts.len() > MAX_ACCOUNTS
            || token_count > MAX_TOKENS
            || resolution_items > MAX_RESOLUTION_ITEMS
        {
            return Err(GetBalancesCommandError::RequestTooLarge);
        }

        let quote_currency = quote_currency.trim().to_ascii_uppercase();

        if !matches!(quote_currency.as_str(), "USD" | "MXN" | "USDC" | "BTC") {
            return Err(GetBalancesCommandError::UnsupportedQuoteCurrency);
        }

        if accounts
            .iter()
            .any(|account| !is_evm_address(&account.address))
        {
            return Err(GetBalancesCommandError::InvalidAccount);
        }

        let mut seen_accounts = HashSet::<(String, String)>::with_capacity(accounts.len());

        for account in &accounts {
            let identity = (
                account.network_slug.clone(),
                account.address.to_ascii_lowercase(),
            );

            if !seen_accounts.insert(identity) {
                return Err(GetBalancesCommandError::DuplicateAccount);
            }
        }

        let mut seen_assets = HashSet::<&str>::with_capacity(tokens.asset_slugs.len());

        for asset_slug in &tokens.asset_slugs {
            if !seen_assets.insert(asset_slug.as_str()) {
                return Err(GetBalancesCommandError::DuplicateAsset);
            }
        }

        Ok(Self {
            as_of,
            accounts,
            tokens: TokenSelector {
                asset_slugs: tokens.asset_slugs,
                contract_addresses,
            },
            quote_currency,
        })
    }

    pub(crate) fn as_of(&self) -> &AsOf {
        &self.as_of
    }

    pub(crate) fn accounts(&self) -> &[OnchainAccount] {
        &self.accounts
    }

    pub(crate) fn tokens(&self) -> &TokenSelector {
        &self.tokens
    }

    pub(crate) fn quote_currency(&self) -> &str {
        &self.quote_currency
    }
}

#[cfg(test)]
mod tests {
    use axum::{http::StatusCode, Router};
    use serde_json::{json, Value};

    use crate::state::AppState;
    use crate::test_utils::{
        errors::assert_public_error, fixtures::global_assets::sample_assets, http::post_raw,
    };
    use crate::{
        adapters::{
            bigwig::client::BigwigClient, http::router::build_router,
            postgres::global_assets::GlobalAssetRepository, price_indexer::PriceIndexerClient,
        },
        config::Config,
    };

    use super::*;

    const ACCOUNT_A: &str = "0x1111111111111111111111111111111111111111";

    fn latest_as_of() -> AsOf {
        AsOf::Latest
    }

    async fn post_json(app: Router, uri: &str, body: Value) -> (StatusCode, Value) {
        post_raw(
            app,
            uri,
            Some("application/json"),
            serde_json::to_vec(&body).unwrap(),
        )
        .await
    }

    fn balance_app(bigwig_url: Option<&str>, price_url: Option<&str>) -> Router {
        let bigwig_client =
            bigwig_url.map(|url| BigwigClient::new(url, "test-bigwig-token", 2_000).unwrap());
        let price_indexer_client =
            price_url.map(|url| PriceIndexerClient::new(url, "test-price-token", 2_000).unwrap());

        build_router(AppState {
            config: Config::default(),
            version: env!("CARGO_PKG_VERSION"),
            database_pool: None,
            api_key_repository: None,
            api_key_minute_limiter: crate::adapters::http::rate_limit::ApiKeyMinuteLimiter::default(
            ),
            asset_repository: Some(GlobalAssetRepository::in_memory(sample_assets())),
            price_indexer_client,
            dis_client: None,
            bigwig_client,
        })
    }

    #[tokio::test]
    async fn public_limits_reject_only_values_above_the_boundary() {
        let app = balance_app(None, None);
        let too_many_accounts = (0..=MAX_ACCOUNTS)
            .map(|index| {
                json!({
                    "network_slug": "eth-mainnet",
                    "address": format!("0x{index:040x}")
                })
            })
            .collect::<Vec<_>>();
        let too_many_assets = (0..=MAX_TOKENS)
            .map(|index| format!("asset-{index}"))
            .collect::<Vec<_>>();

        for body in [
            json!({
                "as_of": {"kind": "latest"},
                "accounts": too_many_accounts,
                "quote_currency": "USD",
                "tokens": {"asset_slugs": ["ethereum"], "contract_addresses": []}
            }),
            json!({
                "as_of": {"kind": "latest"},
                "accounts": [{
                    "network_slug": "eth-mainnet",
                    "address": ACCOUNT_A
                }],
                "quote_currency": "USD",
                "tokens": {"asset_slugs": too_many_assets, "contract_addresses": []}
            }),
        ] {
            let (status, response) = post_json(app.clone(), "/v1/balances/bulk", body).await;
            assert_public_error(
                status,
                &response,
                StatusCode::PAYLOAD_TOO_LARGE,
                "request_too_large",
            );
        }
    }

    #[test]
    fn accepts_exact_command_limit_boundaries() {
        let accounts = (0..MAX_ACCOUNTS)
            .map(|index| OnchainAccount {
                network_slug: "eth-mainnet".to_string(),
                address: format!("0x{index:040x}"),
                client_ref: None,
            })
            .collect();

        let asset_slugs = (0..MAX_TOKENS)
            .map(|index| format!("asset-{index}"))
            .collect();

        let command = GetBalancesCommand::try_new(
            latest_as_of(),
            accounts,
            "USD".to_string(),
            TokenSelector {
                asset_slugs,
                contract_addresses: Vec::new(),
            },
        )
        .unwrap();

        assert_eq!(command.accounts().len(), MAX_ACCOUNTS);
        assert_eq!(command.tokens().asset_slugs.len(), MAX_TOKENS);
    }
}
