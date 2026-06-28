use std::collections::HashSet;
use tracing::warn;

use crate::adapters::http::dto::erc20_transfers::{
    Erc20TransferDirection, Erc20TransferSearchRequest, Erc20TransferTokenFilters,
};
use crate::adapters::http::error::ApiError;
use crate::adapters::postgres::global_assets::GlobalAssetRepository;
use crate::application::balances::catalog::{
    BalanceTargetKind, BalanceTargetResolution, CatalogBalanceTargetResolver,
    CatalogIntegrityIssue, CatalogResolverError,
};
use crate::domain::onchain_window::OnchainWindow;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Erc20TransferSearchCommand {
    pub network_slug: String,
    pub address: String,
    pub direction: Erc20TransferCommandDirection,
    pub tokens: Erc20TransferCommandTokenFilters,
    pub window: OnchainWindow,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Erc20TransferCommandDirection {
    Any,
    From,
    To,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Erc20TransferCommandTokenFilters {
    pub contract_addresses: Vec<String>,
}

// #[derive(Clone, Debug, Eq, PartialEq)]
// pub(crate) enum Erc20TransferCommandWindow {
//     Blocks {
//         from_block: u64,
//         to_block: u64,
//     },
//     Timestamps {
//         from_timestamp: String,
//         to_timestamp: String,
//     },
//     Lookback {
//         lookback_seconds: u64,
//     },
// }

pub(crate) async fn build_command(
    request: Erc20TransferSearchRequest,
    repository: Option<GlobalAssetRepository>,
    max_token_filters: u64,
) -> Result<Erc20TransferSearchCommand, ApiError> {
    let tokens = request.tokens.unwrap_or_default();
    let contract_addresses =
        resolve_token_filters(repository, &request.network_slug, tokens).await?;
    enforce_token_filter_limit(&contract_addresses, max_token_filters)?;

    Ok(Erc20TransferSearchCommand {
        network_slug: request.network_slug,
        address: request.address.to_ascii_lowercase(),
        direction: command_direction(request.direction),
        tokens: Erc20TransferCommandTokenFilters { contract_addresses },
        window: OnchainWindow::try_from(request.window)?,
    })
}

pub(crate) async fn extraction_unavailable_placeholder(
    _command: Erc20TransferSearchCommand,
) -> Result<(), ApiError> {
    Err(ApiError::extraction_unavailable())
}

fn command_direction(direction: Erc20TransferDirection) -> Erc20TransferCommandDirection {
    match direction {
        Erc20TransferDirection::Any => Erc20TransferCommandDirection::Any,
        Erc20TransferDirection::From => Erc20TransferCommandDirection::From,
        Erc20TransferDirection::To => Erc20TransferCommandDirection::To,
    }
}

async fn resolve_token_filters(
    repository: Option<GlobalAssetRepository>,
    network_slug: &str,
    tokens: Erc20TransferTokenFilters,
) -> Result<Vec<String>, ApiError> {
    let mut contract_addresses = Vec::new();
    let mut seen = HashSet::new();

    if !tokens.asset_slugs.is_empty() {
        let repository = repository.ok_or_else(ApiError::asset_contract_mapping_unavailable)?;
        let resolver = CatalogBalanceTargetResolver::new(repository);
        let resolved_contracts = resolver
            .resolve_network(network_slug, &tokens.asset_slugs)
            .await
            .map_err(catalog_resolver_error_to_api_error)
            .and_then(|resolutions| {
                resolved_contract_addresses_from_catalog(
                    network_slug,
                    &tokens.asset_slugs,
                    resolutions,
                )
            })?;

        for contract_address in resolved_contracts {
            push_unique_contract_address(&mut contract_addresses, &mut seen, contract_address);
        }
    }

    for contract_address in tokens.contract_addresses {
        push_unique_contract_address(&mut contract_addresses, &mut seen, contract_address);
    }

    Ok(contract_addresses)
}

fn resolved_contract_addresses_from_catalog(
    network_slug: &str,
    requested_asset_slugs: &[String],
    resolutions: Vec<BalanceTargetResolution>,
) -> Result<Vec<String>, ApiError> {
    if resolutions.len() != requested_asset_slugs.len() {
        warn!(
            network_slug,
            requested_count = requested_asset_slugs.len(),
            resolution_count = resolutions.len(),
            "ERC-20 transfer catalog resolver returned an unexpected resolution count"
        );
        return Err(ApiError::internal_error());
    }

    let mut contract_addresses = Vec::with_capacity(resolutions.len());

    for (requested_asset_slug, resolution) in requested_asset_slugs.iter().zip(resolutions) {
        match resolution {
            BalanceTargetResolution::Resolved(target) => {
                if target.network_slug != network_slug || target.asset_slug != *requested_asset_slug
                {
                    warn!(
                        network_slug,
                        requested_asset_slug,
                        resolved_network_slug = target.network_slug,
                        resolved_asset_slug = target.asset_slug,
                        "ERC-20 transfer catalog resolver returned a mismatched resolution"
                    );
                    return Err(ApiError::internal_error());
                }

                match target.kind {
                    BalanceTargetKind::Erc20 { contract_address } => {
                        contract_addresses.push(contract_address);
                    }
                    BalanceTargetKind::Native => {
                        return Err(ApiError::asset_not_erc20_on_network());
                    }
                }
            }
            BalanceTargetResolution::UnsupportedAsset { .. } => {
                return Err(ApiError::asset_not_found());
            }
            BalanceTargetResolution::UnsupportedNetwork { .. }
            | BalanceTargetResolution::UnsupportedPair { .. } => {
                return Err(ApiError::asset_not_available_on_network());
            }
            BalanceTargetResolution::UnsupportedTokenStandard { .. } => {
                return Err(ApiError::asset_not_erc20_on_network());
            }
        }
    }

    Ok(contract_addresses)
}

fn catalog_resolver_error_to_api_error(error: CatalogResolverError) -> ApiError {
    match error {
        CatalogResolverError::Repository(error) => {
            warn!(%error, "ERC-20 transfer asset catalog lookup failed");
            ApiError::asset_contract_mapping_unavailable()
        }
        CatalogResolverError::InvalidCatalog { issue, .. }
            if matches!(
                issue,
                CatalogIntegrityIssue::IncompleteMapping
                    | CatalogIntegrityIssue::InvalidDecimals
                    | CatalogIntegrityIssue::MalformedErc20Address
            ) =>
        {
            warn!(
                ?issue,
                "ERC-20 transfer asset catalog mapping is incomplete"
            );
            ApiError::asset_contract_mapping_unavailable()
        }
        CatalogResolverError::InvalidCatalog { issue, .. } => {
            warn!(
                ?issue,
                "ERC-20 transfer asset catalog is internally inconsistent"
            );
            ApiError::internal_error()
        }
    }
}

fn push_unique_contract_address(
    contract_addresses: &mut Vec<String>,
    seen: &mut HashSet<String>,
    contract_address: String,
) {
    let contract_address = contract_address.to_ascii_lowercase();

    if seen.insert(contract_address.clone()) {
        contract_addresses.push(contract_address);
    }
}

fn enforce_token_filter_limit(
    contract_addresses: &[String],
    max_token_filters: u64,
) -> Result<(), ApiError> {
    let token_filter_count = u64::try_from(contract_addresses.len()).unwrap_or(u64::MAX);

    if token_filter_count > max_token_filters {
        Err(ApiError::too_many_token_filters())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use serde::Serialize;
    use serde_json::{json, Value};

    use crate::adapters::http::dto::{
        erc20_transfers::{
            Erc20TransferAmount, Erc20TransferRow, Erc20TransferSearchLimits,
            Erc20TransferSearchResponse, Erc20TransferToken, Erc20TransferTokenFilterResolution,
            Erc20TransferTokenFilterSource, ResolvedErc20TokenFilter,
        },
        onchain_window::{BlockWindowDTO, OnchainWindowDTO},
    };

    use super::*;

    #[test]
    fn request_serialization_snapshot_matches_public_shape() {
        let request = Erc20TransferSearchRequest {
            network_slug: "eth-mainnet".to_string(),
            address: "0xabc0000000000000000000000000000000000000".to_string(),
            direction: Erc20TransferDirection::Any,
            tokens: Some(Erc20TransferTokenFilters {
                asset_slugs: vec!["usdc".to_string(), "usdt".to_string()],
                contract_addresses: vec!["0x1111111111111111111111111111111111111111".to_string()],
            }),
            window: OnchainWindowDTO::Block(BlockWindowDTO {
                from_block: 18_600_000,
                to_block: 18_600_500,
            }),
        };

        assert_json_snapshot(
            &request,
            r#"{
  "network_slug": "eth-mainnet",
  "address": "0xabc0000000000000000000000000000000000000",
  "direction": "any",
  "tokens": {
    "asset_slugs": [
      "usdc",
      "usdt"
    ],
    "contract_addresses": [
      "0x1111111111111111111111111111111111111111"
    ]
  },
  "window": {
    "from_block": 18600000,
    "to_block": 18600500
  }
}"#,
        );
    }

    #[test]
    fn response_serialization_snapshot_matches_public_shape() {
        let response = Erc20TransferSearchResponse {
            ok: true,
            response_type: "erc20_transfer_search".to_string(),
            network_slug: "eth-mainnet".to_string(),
            address: "0xabc0000000000000000000000000000000000000".to_string(),
            direction: Erc20TransferDirection::Any,
            window: OnchainWindowDTO::Block(BlockWindowDTO {
                from_block: 18_600_000,
                to_block: 18_600_500,
            }),
            token_filters: Erc20TransferTokenFilterResolution {
                requested: Erc20TransferTokenFilters {
                    asset_slugs: vec!["usdc".to_string(), "usdt".to_string()],
                    contract_addresses: vec![
                        "0x1111111111111111111111111111111111111111".to_string()
                    ],
                },
                resolved_contract_addresses: vec![
                    ResolvedErc20TokenFilter {
                        contract_address: "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48".to_string(),
                        asset_slug: Some("usdc".to_string()),
                        symbol: Some("USDC".to_string()),
                        decimals: Some(6),
                        source: Erc20TransferTokenFilterSource::AssetSlug,
                    },
                    ResolvedErc20TokenFilter {
                        contract_address: "0x1111111111111111111111111111111111111111".to_string(),
                        asset_slug: None,
                        symbol: None,
                        decimals: None,
                        source: Erc20TransferTokenFilterSource::ContractAddress,
                    },
                ],
            },
            transfers: vec![Erc20TransferRow {
                block_number: 18_600_001,
                tx_hash: "0x0000000000000000000000000000000000000000000000000000000000000000"
                    .to_string(),
                log_index: 12,
                token: Erc20TransferToken {
                    contract_address: "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48".to_string(),
                    asset_slug: Some("usdc".to_string()),
                    symbol: Some("USDC".to_string()),
                    decimals: Some(6),
                },
                from: "0xabc0000000000000000000000000000000000000".to_string(),
                to: "0xdef0000000000000000000000000000000000000".to_string(),
                amount: Erc20TransferAmount {
                    raw: "12500000".to_string(),
                    decimal: Some("12.5".to_string()),
                },
                direction: Erc20TransferDirection::From,
            }],
            limits: Erc20TransferSearchLimits { max_rows: 5_000 },
        };

        assert_json_snapshot(
            &response,
            r#"{
  "ok": true,
  "type": "erc20_transfer_search",
  "network_slug": "eth-mainnet",
  "address": "0xabc0000000000000000000000000000000000000",
  "direction": "any",
  "window": {
    "from_block": 18600000,
    "to_block": 18600500
  },
  "token_filters": {
    "requested": {
      "asset_slugs": [
        "usdc",
        "usdt"
      ],
      "contract_addresses": [
        "0x1111111111111111111111111111111111111111"
      ]
    },
    "resolved_contract_addresses": [
      {
        "contract_address": "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
        "asset_slug": "usdc",
        "symbol": "USDC",
        "decimals": 6,
        "source": "asset_slug"
      },
      {
        "contract_address": "0x1111111111111111111111111111111111111111",
        "asset_slug": null,
        "symbol": null,
        "decimals": null,
        "source": "contract_address"
      }
    ]
  },
  "transfers": [
    {
      "block_number": 18600001,
      "tx_hash": "0x0000000000000000000000000000000000000000000000000000000000000000",
      "log_index": 12,
      "token": {
        "contract_address": "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
        "asset_slug": "usdc",
        "symbol": "USDC",
        "decimals": 6
      },
      "from": "0xabc0000000000000000000000000000000000000",
      "to": "0xdef0000000000000000000000000000000000000",
      "amount": {
        "raw": "12500000",
        "decimal": "12.5"
      },
      "direction": "from"
    }
  ],
  "limits": {
    "max_rows": 5000
  }
}"#,
        );
    }

    #[test]
    fn request_rejects_unknown_top_level_fields() {
        let mut body = valid_request_body();
        body["chain"] = json!("eth-mainnet");

        assert!(serde_json::from_value::<Erc20TransferSearchRequest>(body).is_err());
    }

    #[test]
    fn token_filters_reject_unknown_fields() {
        let mut body = valid_request_body();
        body["tokens"]["symbol"] = json!("USDC");

        assert!(serde_json::from_value::<Erc20TransferSearchRequest>(body).is_err());
    }

    #[test]
    fn windows_reject_unknown_fields() {
        let mut body = valid_request_body();
        body["window"]["extra"] = json!(true);

        assert!(serde_json::from_value::<Erc20TransferSearchRequest>(body).is_err());
    }

    #[test]
    fn request_allows_timestamp_and_lookback_window_shapes() {
        let mut timestamp = valid_request_body();
        timestamp["window"] = json!({
            "from_timestamp": "2026-06-25T00:00:00Z",
            "to_timestamp": "2026-06-25T01:00:00Z"
        });
        let timestamp_request =
            serde_json::from_value::<Erc20TransferSearchRequest>(timestamp).unwrap();
        assert!(matches!(
            timestamp_request.window,
            OnchainWindowDTO::Timestamp(_)
        ));

        let mut lookback = valid_request_body();
        lookback["window"] = json!({
            "lookback_seconds": 600,
            "to": "latest"
        });
        let lookback_request = serde_json::from_value::<Erc20TransferSearchRequest>(lookback)
            .expect("lookback window should deserialize");
        assert!(matches!(
            lookback_request.window,
            OnchainWindowDTO::Lookback(_)
        ));
    }

    fn valid_request_body() -> Value {
        json!({
            "network_slug": "eth-mainnet",
            "address": "0xabc0000000000000000000000000000000000000",
            "direction": "any",
            "tokens": {
                "asset_slugs": ["usdc"],
                "contract_addresses": ["0x1111111111111111111111111111111111111111"]
            },
            "window": {
                "from_block": 18600000,
                "to_block": 18600500
            }
        })
    }

    fn assert_json_snapshot<T>(value: &T, expected: &str)
    where
        T: Serialize,
    {
        let actual = serde_json::to_string_pretty(value).unwrap();
        assert_eq!(actual, expected);
    }
}
