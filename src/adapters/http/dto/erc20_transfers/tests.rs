use axum::{
    body::Body,
    http::{Request, StatusCode},
    response::IntoResponse,
    Router,
};
use serde_json::{json, Value};
use tower::ServiceExt;

use super::*;
use crate::{
    adapters::http::{
        dto::filters::{
            onchain_window::{BlockWindowDTO, OnchainWindowDTO},
            token_filters::TokenFilterDTO,
            transfer_direction::TransferDirectionDTO,
        },
        router::build_router,
        types::JsonObject,
    },
    application::{
        erc20_transfers::service::{
            build_command, Erc20TransferCommandTokenFilters, Erc20TransferSearchCommand,
        },
        filters::{
            onchain_window::{BlockWindow, OnchainWindow},
            transfer_direction::TransferDirection,
        },
    },
    common::rfc3339::parse_rfc3339,
    test_utils::{
        errors::assert_public_error,
        fixtures::{
            erc20_transfers::{
                erc20_transfers_command_from_body, erc20_transfers_request_with_tokens_body,
                erc20_transfers_without_tokens_body, valid_erc20_transfers_request_body,
            },
            global_assets::{global_assets_repository, sample_assets},
        },
    },
};
use crate::{adapters::postgres::global_assets::GlobalAssetRepository, config::Config};

use crate::test_utils::json::json_object;

const TEST_MAX_TOKEN_FILTERS: u64 = 20;

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
        TokenFilterDTO::default()
    );

    let mut null_tokens_body = valid_erc20_transfers_request_body();
    null_tokens_body["tokens"] = Value::Null;
    let null_tokens = Erc20TransferSearchRequest::try_from(&json_object(null_tokens_body)).unwrap();
    assert_eq!(
        null_tokens.tokens.unwrap_or_default(),
        TokenFilterDTO::default()
    );

    let mut empty_tokens_body = valid_erc20_transfers_request_body();
    empty_tokens_body["tokens"] = json!({});
    let empty_tokens =
        Erc20TransferSearchRequest::try_from(&json_object(empty_tokens_body)).unwrap();
    assert_eq!(
        empty_tokens.tokens.unwrap_or_default(),
        TokenFilterDTO::default()
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

#[tokio::test]
async fn command_resolves_usdc_and_dedupes_duplicate_explicit_address() {
    let mut body = valid_erc20_transfers_request_body();
    body["address"] = json!("0xABC0000000000000000000000000000000000000");
    body["tokens"]["contract_addresses"] = json!(["0xA0B86991C6218B36C1D19D4A2E9EB0CE3606EB48"]);

    let command = erc20_transfers_command_from_body(body, TEST_MAX_TOKEN_FILTERS).await;

    assert_eq!(
        command,
        Erc20TransferSearchCommand {
            network_slug: "eth-mainnet".to_string(),
            address: "0xabc0000000000000000000000000000000000000".to_string(),
            direction: TransferDirection::Any,
            tokens: Erc20TransferCommandTokenFilters {
                contract_addresses: vec!["0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48".to_string()],
            },
            window: OnchainWindow::Block(BlockWindow::new(18_600_000, 18_600_500).unwrap()),
        }
    );
}

#[test]
fn validation_accepts_minimal_asset_contract_and_mixed_token_filter_shapes() {
    let cases = [
        (
            erc20_transfers_without_tokens_body(),
            TokenFilterDTO::default(),
        ),
        (
            erc20_transfers_request_with_tokens_body(json!({
                "asset_slugs": ["usdc", "wrapped-ether"]
            })),
            TokenFilterDTO {
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
            TokenFilterDTO {
                asset_slugs: Vec::new(),
                contract_addresses: vec![
                    "0x1111111111111111111111111111111111111111".to_string(),
                    "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48".to_string(),
                ],
            },
        ),
        (
            valid_erc20_transfers_request_body(),
            TokenFilterDTO {
                asset_slugs: vec!["usdc".to_string()],
                contract_addresses: vec!["0x1111111111111111111111111111111111111111".to_string()],
            },
        ),
    ];

    for (body, expected_tokens) in cases {
        let request = Erc20TransferSearchRequest::try_from(&json_object(body)).unwrap();

        assert_eq!(request.network_slug, "eth-mainnet");
        assert_eq!(
            request.address,
            "0xabc0000000000000000000000000000000000000"
        );
        assert_eq!(request.direction, TransferDirectionDTO::Any);
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

#[tokio::test]
async fn command_enforces_final_token_filter_limit_after_dedupe() {
    let body = erc20_transfers_request_with_tokens_body(json!({
        "asset_slugs": ["usdc"],
        "contract_addresses": ["0xA0B86991C6218B36C1D19D4A2E9EB0CE3606EB48"]
    }));
    let command = erc20_transfers_command_from_body(body, 1).await;
    assert_eq!(
        command.tokens.contract_addresses,
        ["0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"]
    );

    let body = erc20_transfers_request_with_tokens_body(json!({
        "asset_slugs": ["usdc"],
        "contract_addresses": ["0x1111111111111111111111111111111111111111"]
    }));
    let request = Erc20TransferSearchRequest::try_from(&json_object(body)).unwrap();
    let error = build_command(request, Some(global_assets_repository()), 1)
        .await
        .unwrap_err();

    assert_api_error(
        error,
        StatusCode::UNPROCESSABLE_ENTITY,
        "too_many_token_filters",
    )
    .await;

    async fn assert_api_error(error: ApiError, expected_status: StatusCode, expected_code: &str) {
        let response = error.into_response();
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();

        assert_public_error(status, &json, expected_status, expected_code);
    }
}
