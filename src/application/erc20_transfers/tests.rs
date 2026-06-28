use serde::Serialize;
use serde_json::{json, Value};

use crate::{
    adapters::http::dto::{
        erc20_transfers::{
            Erc20TransferAmount, Erc20TransferRow, Erc20TransferSearchLimits,
            Erc20TransferSearchRequest, Erc20TransferSearchResponse, Erc20TransferToken,
        },
        filters::{
            onchain_window::{BlockWindowDTO, OnchainWindowDTO},
            token_filters::{
                ResolvedTokenFilterDTO, TokenFilterDTO, TokenFilterResolutionDTO,
                TokenFilterSourceDTO,
            },
            transfer_direction::TransferDirectionDTO,
        },
    },
    test_utils::fixtures::erc20_transfers::valid_erc20_transfers_request_body,
};

#[test]
fn request_serialization_snapshot_matches_public_shape() {
    let request = Erc20TransferSearchRequest {
        network_slug: "eth-mainnet".to_string(),
        address: "0xabc0000000000000000000000000000000000000".to_string(),
        direction: TransferDirectionDTO::Any,
        tokens: Some(TokenFilterDTO {
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
        direction: TransferDirectionDTO::Any,
        window: OnchainWindowDTO::Block(BlockWindowDTO {
            from_block: 18_600_000,
            to_block: 18_600_500,
        }),
        token_filters: TokenFilterResolutionDTO {
            requested: TokenFilterDTO {
                asset_slugs: vec!["usdc".to_string(), "usdt".to_string()],
                contract_addresses: vec!["0x1111111111111111111111111111111111111111".to_string()],
            },
            resolved_contract_addresses: vec![
                ResolvedTokenFilterDTO {
                    contract_address: "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48".to_string(),
                    asset_slug: Some("usdc".to_string()),
                    symbol: Some("USDC".to_string()),
                    decimals: Some(6),
                    source: TokenFilterSourceDTO::AssetSlug,
                },
                ResolvedTokenFilterDTO {
                    contract_address: "0x1111111111111111111111111111111111111111".to_string(),
                    asset_slug: None,
                    symbol: None,
                    decimals: None,
                    source: TokenFilterSourceDTO::ContractAddress,
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
            direction: TransferDirectionDTO::From,
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
    let mut body = valid_erc20_transfers_request_body();
    body["chain"] = json!("eth-mainnet");

    assert!(serde_json::from_value::<Erc20TransferSearchRequest>(body).is_err());
}

#[test]
fn token_filters_reject_unknown_fields() {
    let mut body = valid_erc20_transfers_request_body();
    body["tokens"]["symbol"] = json!("USDC");

    assert!(serde_json::from_value::<Erc20TransferSearchRequest>(body).is_err());
}

#[test]
fn windows_reject_unknown_fields() {
    let mut body = valid_erc20_transfers_request_body();
    body["window"]["extra"] = json!(true);

    assert!(serde_json::from_value::<Erc20TransferSearchRequest>(body).is_err());
}

#[test]
fn request_allows_timestamp_and_lookback_window_shapes() {
    let mut timestamp = valid_erc20_transfers_request_body();
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

    let mut lookback = valid_erc20_transfers_request_body();
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

fn assert_json_snapshot<T>(value: &T, expected: &str)
where
    T: Serialize,
{
    let actual = serde_json::to_string_pretty(value).unwrap();
    assert_eq!(actual, expected);
}
