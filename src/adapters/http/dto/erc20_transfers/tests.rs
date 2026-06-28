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
        erc20_transfers::service::{Erc20TransferCommandTokenFilters, Erc20TransferSearchCommand},
        filters::{
            onchain_window::{BlockWindow, OnchainWindow},
            transfer_direction::TransferDirection,
        },
    },
    common::rfc3339::parse_rfc3339,
    test_utils::{
        fixtures::erc20_transfers::valid_erc20_transfers_request_body,
        fixtures::global_assets::sample_assets,
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
