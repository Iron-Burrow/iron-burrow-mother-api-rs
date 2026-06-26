use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Erc20TransferSearchRequest {
    pub network_slug: String,
    pub address: String,
    pub direction: Erc20TransferDirection,
    pub tokens: Option<Erc20TransferTokenFilters>,
    pub window: Erc20TransferSearchWindow,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
pub struct Erc20TransferSearchResponse {
    pub ok: bool,
    #[serde(rename = "type")]
    pub response_type: String,
    pub network_slug: String,
    pub address: String,
    pub direction: Erc20TransferDirection,
    pub window: Erc20TransferSearchWindow,
    pub token_filters: Erc20TransferTokenFilterResolution,
    pub transfers: Vec<Erc20TransferRow>,
    pub limits: Erc20TransferSearchLimits,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
#[serde(untagged)]
pub enum Erc20TransferSearchWindow {
    Block(Erc20TransferBlockWindow),
    Timestamp(Erc20TransferTimestampWindow),
    Lookback(Erc20TransferLookbackWindow),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Erc20TransferBlockWindow {
    pub from_block: u64,
    pub to_block: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Erc20TransferTimestampWindow {
    pub from_timestamp: String,
    pub to_timestamp: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Erc20TransferLookbackWindow {
    pub lookback_seconds: u64,
    pub to: Erc20TransferLookbackTarget,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum Erc20TransferLookbackTarget {
    Latest,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Erc20TransferTokenFilters {
    #[serde(default)]
    pub asset_slugs: Vec<String>,
    #[serde(default)]
    pub contract_addresses: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
pub struct Erc20TransferTokenFilterResolution {
    pub requested: Erc20TransferTokenFilters,
    pub resolved_contract_addresses: Vec<ResolvedErc20TokenFilter>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
pub struct ResolvedErc20TokenFilter {
    pub contract_address: String,
    pub asset_slug: Option<String>,
    pub symbol: Option<String>,
    pub decimals: Option<u8>,
    pub source: Erc20TransferTokenFilterSource,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Erc20TransferTokenFilterSource {
    AssetSlug,
    ContractAddress,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
pub struct Erc20TransferRow {
    pub block_number: u64,
    pub tx_hash: String,
    pub log_index: u64,
    pub token: Erc20TransferToken,
    pub from: String,
    pub to: String,
    pub amount: Erc20TransferAmount,
    pub direction: Erc20TransferDirection,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
pub struct Erc20TransferToken {
    pub contract_address: String,
    pub asset_slug: Option<String>,
    pub symbol: Option<String>,
    pub decimals: Option<u8>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
pub struct Erc20TransferAmount {
    pub raw: String,
    pub decimal: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum Erc20TransferDirection {
    Any,
    From,
    To,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
pub struct Erc20TransferSearchLimits {
    pub max_rows: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Erc20TransferSearchCommand {
    pub network_slug: String,
    pub address: String,
    pub direction: Erc20TransferCommandDirection,
    pub tokens: Erc20TransferCommandTokenFilters,
    pub window: Erc20TransferCommandWindow,
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Erc20TransferCommandWindow {
    Blocks {
        from_block: u64,
        to_block: u64,
    },
    Timestamps {
        from_timestamp: String,
        to_timestamp: String,
    },
    Lookback {
        lookback_seconds: u64,
    },
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

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
            window: Erc20TransferSearchWindow::Block(Erc20TransferBlockWindow {
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
            window: Erc20TransferSearchWindow::Block(Erc20TransferBlockWindow {
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
            Erc20TransferSearchWindow::Timestamp(_)
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
            Erc20TransferSearchWindow::Lookback(_)
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
