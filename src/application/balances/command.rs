use std::collections::HashSet;

use crate::{
    adapters::http::{
        dto::{
            accounts::OnchainAccountRequest,
            assets::token_selector::TokenSelectorRequest,
            balances::requests::{BulkBalanceRequest, SingleBalanceRequest},
            onchain_time::as_of::AsOfRequest,
        },
        error::ApiError,
    },
    domain::{
        accounts::OnchainAccount, assets::token_selector::TokenSelector, onchain_time::as_of::AsOf,
        validation::is_evm_address,
    },
};

const MAX_ACCOUNTS: usize = 50;
const MAX_TOKENS: usize = 20;
const MAX_RESOLUTION_ITEMS: usize = 1_000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GetBalancesCommand {
    pub as_of: AsOf,
    pub accounts: Vec<OnchainAccount>,
    pub tokens: TokenSelector,
    pub quote_currency: String,
}

// impl TryFrom<JsonObject> for SingleBalanceRequest {
//     type Error = ApiError;

//     fn try_from(request: JsonObject) -> Result<Self, Self::Error> {
//         reject_reserved_alias_fields_in_object(&request)?;
//         reject_unknown_fields(&request, &SINGLE_BALANCE_FIELDS)?;

//         Ok(Self {
//             as_of: validate_as_of_object(request.get("as_of"))?,
//             account: validate_account_object(
//                 request.get("account"),
//                 &SUPPORTED_BALANCE_NETWORK_SLUGS,
//             )?,
//             quote_currency: validate_required_string(request.get("quote_currency"))?,
//             tokens: validate_required_non_empty_tokens_object(request.get("tokens"))?,
//         })
//     }
// }

// impl TryFrom<JsonObject> for BulkBalanceRequest {

impl TryFrom<SingleBalanceRequest> for GetBalancesCommand {
    type Error = ApiError;

    fn try_from(request: SingleBalanceRequest) -> Result<Self, Self::Error> {
        validate_request(
            request.as_of,
            vec![request.account],
            request.quote_currency,
            request.tokens,
        )
    }
}

impl TryFrom<BulkBalanceRequest> for GetBalancesCommand {
    type Error = ApiError;

    fn try_from(request: BulkBalanceRequest) -> Result<Self, Self::Error> {
        validate_request(
            request.as_of,
            request.accounts,
            request.quote_currency,
            request.tokens,
        )
    }
}

fn validate_request(
    as_of: AsOfRequest,
    accounts: Vec<OnchainAccountRequest>,
    quote_currency: String,
    tokens: TokenSelectorRequest,
) -> Result<GetBalancesCommand, ApiError> {
    let as_of = AsOf::try_from(as_of)?;
    if accounts.is_empty() {
        return Err(ApiError::empty_accounts());
    }
    if tokens.asset_slugs.is_empty() && tokens.contract_addresses.is_empty() {
        return Err(ApiError::empty_tokens());
    }

    let mut seen_contracts = HashSet::<String>::with_capacity(tokens.contract_addresses.len());
    let contract_addresses = tokens
        .contract_addresses
        .into_iter()
        .map(|contract_address| contract_address.to_ascii_lowercase())
        .filter(|contract_address| seen_contracts.insert(contract_address.clone()))
        .collect::<Vec<_>>();
    let token_count = tokens.asset_slugs.len() + contract_addresses.len();

    let resolution_items = accounts
        .len()
        .checked_mul(token_count)
        .ok_or_else(ApiError::request_too_large)?;
    if accounts.len() > MAX_ACCOUNTS
        || token_count > MAX_TOKENS
        || resolution_items > MAX_RESOLUTION_ITEMS
    {
        return Err(ApiError::request_too_large());
    }

    let quote_currency = quote_currency.trim().to_ascii_uppercase();
    if !matches!(quote_currency.as_str(), "USD" | "MXN" | "USDC" | "BTC") {
        return Err(ApiError::unsupported_quote_currency());
    }

    if accounts
        .iter()
        .any(|account| !is_evm_address(&account.address))
    {
        return Err(ApiError::invalid_account());
    }

    let mut seen_accounts = HashSet::<(String, String)>::with_capacity(accounts.len());
    for account in &accounts {
        if !seen_accounts.insert((
            account.network_slug.clone(),
            account.address.to_ascii_lowercase(),
        )) {
            return Err(ApiError::duplicate_account());
        }
    }

    let mut seen_assets = HashSet::<String>::with_capacity(tokens.asset_slugs.len());
    for asset_slug in &tokens.asset_slugs {
        if !seen_assets.insert(asset_slug.clone()) {
            return Err(ApiError::duplicate_asset());
        }
    }

    Ok(GetBalancesCommand {
        as_of,
        accounts: accounts
            .into_iter()
            .map(|account| OnchainAccount {
                network_slug: account.network_slug,
                address: account.address,
                client_ref: account.client_ref,
            })
            .collect(),
        tokens: TokenSelector {
            asset_slugs: tokens.asset_slugs,
            contract_addresses,
        },
        quote_currency,
    })
}

#[cfg(test)]
mod tests {
    use axum::{http::StatusCode, response::IntoResponse, Router};
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

    fn latest_as_of() -> AsOfRequest {
        AsOfRequest {
            kind: "latest".to_string(),
            timestamp: None,
            block_number: None,
        }
    }

    fn account(network_slug: &str, address: &str) -> OnchainAccountRequest {
        OnchainAccountRequest {
            network_slug: network_slug.to_string(),
            address: address.to_string(),
            client_ref: None,
        }
    }

    fn tokens<const N: usize>(asset_slugs: [&str; N]) -> TokenSelectorRequest {
        token_vec(asset_slugs.into_iter().map(str::to_string).collect())
    }

    fn token_vec(asset_slugs: Vec<String>) -> TokenSelectorRequest {
        TokenSelectorRequest {
            asset_slugs,
            contract_addresses: Vec::new(),
        }
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

    #[test]
    fn validation_normalizes_quote_and_preserves_public_identifiers() {
        let request = validate_request(
            latest_as_of(),
            vec![account("eth-mainnet", ACCOUNT_A)],
            " mxn ".to_string(),
            tokens(["ethereum"]),
        )
        .unwrap();

        assert_eq!(request.quote_currency, "MXN");
        assert_eq!(request.accounts[0].network_slug, "eth-mainnet");
        assert_eq!(request.accounts[0].address, ACCOUNT_A);
        assert_eq!(request.tokens.asset_slugs, ["ethereum"]);
        assert!(request.tokens.contract_addresses.is_empty());
    }

    #[test]
    fn validation_allows_same_address_on_different_networks() {
        let request = validate_request(
            latest_as_of(),
            vec![
                account("eth-mainnet", ACCOUNT_A),
                account("base-mainnet", ACCOUNT_A),
            ],
            "USD".to_string(),
            tokens(["usdc"]),
        )
        .unwrap();

        assert_eq!(request.accounts.len(), 2);
    }

    #[test]
    fn validation_rejects_duplicate_accounts_case_insensitively() {
        let error = validate_request(
            latest_as_of(),
            vec![
                account("eth-mainnet", "0xabcdefabcdefabcdefabcdefabcdefabcdefabcd"),
                account("eth-mainnet", "0xABCDEFABCDEFABCDEFABCDEFABCDEFABCDEFABCD"),
            ],
            "USD".to_string(),
            tokens(["usdc"]),
        )
        .unwrap_err();

        assert_eq!(
            error.into_response().status(),
            axum::http::StatusCode::BAD_REQUEST
        );
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
                StatusCode::BAD_REQUEST,
                "request_too_large",
            );
        }
    }

    #[test]
    fn validation_accepts_exact_public_limit_boundaries() {
        let accounts = (0..MAX_ACCOUNTS)
            .map(|index| account("eth-mainnet", &format!("0x{index:040x}")))
            .collect();
        let asset_slugs = (0..MAX_TOKENS)
            .map(|index| format!("asset-{index}"))
            .collect::<Vec<_>>();

        let request = validate_request(
            latest_as_of(),
            accounts,
            "USD".to_string(),
            token_vec(asset_slugs),
        )
        .unwrap();

        assert_eq!(request.accounts.len(), MAX_ACCOUNTS);
        assert_eq!(request.tokens.asset_slugs.len(), MAX_TOKENS);
    }

    #[test]
    fn validation_deduplicates_explicit_contracts_case_insensitively_before_limits() {
        let request = validate_request(
            latest_as_of(),
            vec![account("eth-mainnet", ACCOUNT_A)],
            "USD".to_string(),
            TokenSelectorRequest {
                asset_slugs: Vec::new(),
                contract_addresses: vec![
                    "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48".to_string(),
                    "0xA0B86991C6218B36C1D19D4A2E9EB0CE3606EB48".to_string(),
                ],
            },
        )
        .unwrap();

        assert_eq!(
            request.tokens.contract_addresses,
            ["0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"]
        );
        assert!(request.tokens.asset_slugs.is_empty());
    }
}
