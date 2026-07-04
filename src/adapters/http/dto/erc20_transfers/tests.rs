use serde::Serialize;
use serde_json::{json, Value};

use crate::{
    adapters::http::dto::{
        accounts::{OnchainAccountRequest, OnchainAccountResponse},
        assets::token_selector::{
            ResolvedTokenSelectorRequest, TokenFilterResolutionDTO, TokenFilterSourceDTO,
            TokenSelectorRequest,
        },
        erc20_transfers::{
            examples,
            requests::Erc20TransferSearchRequest,
            response::{
                Erc20TransferAmount, Erc20TransferRow, Erc20TransferSearchLimits,
                Erc20TransferSearchResponse, Erc20TransferToken,
            },
        },
        onchain_time::onchain_window::{BlockWindowDTO, OnchainWindowDTO},
        transfers::transfer_direction::{TransferDirectionRequest, TransferDirectionResponse},
    },
    test_utils::{
        fixtures::erc20_transfers::{
            erc20_transfers_request_with_tokens_body, erc20_transfers_without_tokens_body,
            valid_erc20_transfers_request_body,
        },
        json::json_object,
    },
};

#[test]
fn request_serialization_snapshot_matches_public_shape() {
    let request = Erc20TransferSearchRequest {
        account: OnchainAccountRequest {
            network_slug: "eth-mainnet".to_string(),
            address: "0xabc0000000000000000000000000000000000000".to_string(),
            client_ref: Some("treasury-main".to_string()),
        },
        direction: TransferDirectionRequest::Any,
        tokens: Some(TokenSelectorRequest {
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
  "account": {
    "network_slug": "eth-mainnet",
    "address": "0xabc0000000000000000000000000000000000000",
    "client_ref": "treasury-main"
  },
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
        account: OnchainAccountResponse {
            network_slug: "eth-mainnet".to_string(),
            address: "0xabc0000000000000000000000000000000000000".to_string(),
            client_ref: Some("treasury-main".to_string()),
        },
        direction: TransferDirectionResponse::Any,
        window: OnchainWindowDTO::Block(BlockWindowDTO {
            from_block: 18_600_000,
            to_block: 18_600_500,
        }),
        token_filters: TokenFilterResolutionDTO {
            requested: TokenSelectorRequest {
                asset_slugs: vec!["usdc".to_string(), "usdt".to_string()],
                contract_addresses: vec!["0x1111111111111111111111111111111111111111".to_string()],
            },
            resolved_contract_addresses: vec![
                ResolvedTokenSelectorRequest {
                    contract_address: "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48".to_string(),
                    asset_slug: Some("usdc".to_string()),
                    symbol: Some("USDC".to_string()),
                    decimals: Some(6),
                    source: TokenFilterSourceDTO::AssetSlug,
                },
                ResolvedTokenSelectorRequest {
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
            direction: TransferDirectionResponse::From,
        }],
        limits: Erc20TransferSearchLimits {
            max_rows: 5_000,
            truncated: true,
        },
    };

    assert_json_snapshot(
        &response,
        r#"{
  "ok": true,
  "type": "erc20_transfer_search",
  "account": {
    "network_slug": "eth-mainnet",
    "address": "0xabc0000000000000000000000000000000000000",
    "client_ref": "treasury-main"
  },
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
    "max_rows": 5000,
    "truncated": true
  }
}"#,
    );
}

#[test]
fn documented_request_examples_match_public_dto_shape() {
    for example in [
        examples::unfiltered_request(),
        examples::asset_slug_request(),
        examples::contract_address_request(),
        examples::mixed_filter_request(),
        examples::native_asset_rejection_request(),
        examples::unknown_slug_rejection_request(),
        examples::too_many_filters_request(),
    ] {
        let request = Erc20TransferSearchRequest::try_from(&json_object(example.clone())).unwrap();

        assert_eq!(serde_json::to_value(request).unwrap(), example);
    }
}

#[test]
fn documented_success_examples_match_public_dto_shape() {
    for example in [
        examples::unfiltered_success_response(),
        examples::asset_slug_success_response(),
        examples::contract_address_success_response(),
        examples::mixed_success_response(),
        examples::truncated_success_response(),
    ] {
        let response: Erc20TransferSearchResponse =
            serde_json::from_value(example.clone()).unwrap();

        assert_eq!(serde_json::to_value(response).unwrap(), example);
    }
}

#[test]
fn request_rejects_unknown_top_level_fields() {
    let mut body = valid_erc20_transfers_request_body();
    body["chain"] = json!("eth-mainnet");

    assert!(serde_json::from_value::<Erc20TransferSearchRequest>(body).is_err());
}

#[test]
fn request_rejects_legacy_top_level_account_fields() {
    let mut body = valid_erc20_transfers_request_body();
    body["network_slug"] = json!("eth-mainnet");
    body["address"] = json!("0xabc0000000000000000000000000000000000000");

    assert!(Erc20TransferSearchRequest::try_from(&json_object(body)).is_err());
}

#[test]
fn account_rejects_unknown_fields() {
    let mut body = valid_erc20_transfers_request_body();
    body["account"]["chain"] = json!("eth-mainnet");

    assert!(Erc20TransferSearchRequest::try_from(&json_object(body)).is_err());
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

#[test]
fn validation_accepts_supported_window_shapes() {
    let request = json_object(valid_erc20_transfers_request_body());
    let block = Erc20TransferSearchRequest::try_from(&request).unwrap();
    assert!(matches!(
        block.window,
        OnchainWindowDTO::Block(BlockWindowDTO {
            from_block: 18_600_000,
            to_block: 18_600_500,
        })
    ));

    let mut timestamp_body = valid_erc20_transfers_request_body();
    timestamp_body["window"] = json!({
        "from_timestamp": "2026-06-25T00:00:00Z",
        "to_timestamp": "2026-06-25T01:00:00Z"
    });
    let timestamp = Erc20TransferSearchRequest::try_from(&json_object(timestamp_body)).unwrap();
    assert!(matches!(timestamp.window, OnchainWindowDTO::Timestamp(_)));

    let mut lookback_body = valid_erc20_transfers_request_body();
    lookback_body["window"] = json!({
        "lookback_seconds": 600,
        "to": "latest"
    });
    let lookback = Erc20TransferSearchRequest::try_from(&json_object(lookback_body)).unwrap();
    assert!(matches!(lookback.window, OnchainWindowDTO::Lookback(_)));
}

#[test]
fn validation_accepts_omitted_null_and_empty_tokens() {
    let omitted_tokens =
        Erc20TransferSearchRequest::try_from(&json_object(erc20_transfers_without_tokens_body()))
            .unwrap();
    assert_eq!(
        omitted_tokens.tokens.unwrap_or_default(),
        TokenSelectorRequest::default()
    );

    let mut null_tokens_body = valid_erc20_transfers_request_body();
    null_tokens_body["tokens"] = Value::Null;
    let null_tokens = Erc20TransferSearchRequest::try_from(&json_object(null_tokens_body)).unwrap();
    assert_eq!(
        null_tokens.tokens.unwrap_or_default(),
        TokenSelectorRequest::default()
    );

    let mut empty_tokens_body = valid_erc20_transfers_request_body();
    empty_tokens_body["tokens"] = json!({});
    let empty_tokens =
        Erc20TransferSearchRequest::try_from(&json_object(empty_tokens_body)).unwrap();
    assert_eq!(
        empty_tokens.tokens.unwrap_or_default(),
        TokenSelectorRequest::default()
    );

    let mut empty_token_arrays_body = valid_erc20_transfers_request_body();
    empty_token_arrays_body["tokens"] = json!({
        "asset_slugs": [],
        "contract_addresses": []
    });
    let empty_token_arrays =
        Erc20TransferSearchRequest::try_from(&json_object(empty_token_arrays_body)).unwrap();
    assert_eq!(
        empty_token_arrays.tokens.unwrap_or_default(),
        TokenSelectorRequest::default()
    );
}

#[test]
fn validation_normalizes_explicit_contract_addresses_to_lowercase() {
    let mut body = valid_erc20_transfers_request_body();
    body["tokens"]["contract_addresses"] = json!(["0xA0B86991C6218B36C1D19D4A2E9EB0CE3606EB48"]);

    let request = Erc20TransferSearchRequest::try_from(&json_object(body)).unwrap();

    assert_eq!(
        request.tokens.unwrap().contract_addresses,
        ["0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"]
    );
}

#[test]
fn validation_accepts_minimal_asset_contract_and_mixed_token_filter_shapes() {
    let cases = [
        (
            erc20_transfers_without_tokens_body(),
            TokenSelectorRequest::default(),
        ),
        (
            erc20_transfers_request_with_tokens_body(json!({
                "asset_slugs": ["usdc", "wrapped-ether"]
            })),
            TokenSelectorRequest {
                asset_slugs: vec!["usdc".to_string(), "wrapped-ether".to_string()],
                contract_addresses: Vec::new(),
            },
        ),
        (
            erc20_transfers_request_with_tokens_body(json!({
                "contract_addresses": [
                    "0x1111111111111111111111111111111111111111",
                    "0xA0B86991C6218B36C1D19D4A2E9EB0CE3606EB48"
                ]
            })),
            TokenSelectorRequest {
                asset_slugs: Vec::new(),
                contract_addresses: vec![
                    "0x1111111111111111111111111111111111111111".to_string(),
                    "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48".to_string(),
                ],
            },
        ),
        (
            valid_erc20_transfers_request_body(),
            TokenSelectorRequest {
                asset_slugs: vec!["usdc".to_string()],
                contract_addresses: vec!["0x1111111111111111111111111111111111111111".to_string()],
            },
        ),
    ];

    for (body, expected_tokens) in cases {
        let request = Erc20TransferSearchRequest::try_from(&json_object(body)).unwrap();

        assert_eq!(request.account.network_slug, "eth-mainnet");
        assert_eq!(
            request.account.address,
            "0xabc0000000000000000000000000000000000000"
        );
        assert_eq!(
            request.account.client_ref,
            Some("treasury-main".to_string())
        );
        assert_eq!(request.direction, TransferDirectionRequest::Any);
        assert_eq!(request.tokens.unwrap_or_default(), expected_tokens);
        assert!(matches!(
            request.window,
            OnchainWindowDTO::Block(BlockWindowDTO {
                from_block: 18_600_000,
                to_block: 18_600_500,
            })
        ));
    }
}

fn assert_json_snapshot<T>(value: &T, expected: &str)
where
    T: Serialize,
{
    let actual = serde_json::to_string_pretty(value).unwrap();
    assert_eq!(actual, expected);
}
