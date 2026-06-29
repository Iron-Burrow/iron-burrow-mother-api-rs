use serde_json::{json, Value};

use crate::{
    adapters::http::dto::erc20_transfers::Erc20TransferSearchRequest,
    application::erc20_transfers::service::{build_command, Erc20TransferSearchCommand},
    test_utils::{fixtures::global_assets::global_assets_repository, json::json_object},
};

use crate::config::Config;

pub(crate) fn erc20_transfers_enabled_config() -> Config {
    Config {
        erc20_transfers_enabled: true,
        ..Config::default()
    }
}

pub(crate) fn valid_erc20_transfers_request_body() -> Value {
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

pub(crate) fn erc20_transfers_without_tokens_body() -> Value {
    let mut body = valid_erc20_transfers_request_body();
    body.as_object_mut().unwrap().remove("tokens");
    body
}

pub(crate) fn erc20_transfers_request_with_tokens_body(tokens: Value) -> Value {
    let mut body = erc20_transfers_without_tokens_body();
    body["tokens"] = tokens;
    body
}

pub(crate) async fn erc20_transfers_command_from_body(
    body: Value,
    max_token_filters: u64,
) -> Erc20TransferSearchCommand {
    let request = Erc20TransferSearchRequest::try_from(&json_object(body)).unwrap();
    build_command(request, Some(global_assets_repository()), max_token_filters)
        .await
        .unwrap()
}
