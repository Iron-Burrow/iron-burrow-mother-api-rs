use serde_json::{json, Value};

use crate::config::Config;

pub(crate) fn erc20_transfers_enabled_config() -> Config {
    Config {
        erc20_transfers_enabled: true,
        ..Config::default()
    }
}

pub(crate) fn valid_erc20_transfers_request_body() -> Value {
    json!({
        "account": {
            "network_slug": "eth-mainnet",
            "address": "0xabc0000000000000000000000000000000000000",
            "client_ref": "treasury-main"
        },
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
