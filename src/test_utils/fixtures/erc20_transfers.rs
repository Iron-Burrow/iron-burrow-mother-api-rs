use serde_json::{json, Value};

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
