use serde_json::{json, Value};

const NETWORK_SLUG: &str = "eth-mainnet";
const WATCHED_ADDRESS: &str = "0xabc0000000000000000000000000000000000000";
const USDC_CONTRACT: &str = "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48";
const UNKNOWN_CONTRACT: &str = "0x1111111111111111111111111111111111111111";
const TX_HASH_1: &str = "0x0000000000000000000000000000000000000000000000000000000000000001";
const TX_HASH_2: &str = "0x0000000000000000000000000000000000000000000000000000000000000002";

pub(crate) fn unfiltered_request() -> Value {
    json!({
        "account": {
            "network_slug": NETWORK_SLUG,
            "address": WATCHED_ADDRESS,
            "client_ref": "treasury-main"
        },
        "direction": "any",
        "tokens": null,
        "window": block_window()
    })
}

pub(crate) fn asset_slug_request() -> Value {
    request_with_tokens(json!({
        "asset_slugs": ["usdc"],
        "contract_addresses": []
    }))
}

pub(crate) fn contract_address_request() -> Value {
    request_with_tokens(json!({
        "asset_slugs": [],
        "contract_addresses": [UNKNOWN_CONTRACT]
    }))
}

pub(crate) fn mixed_filter_request() -> Value {
    request_with_tokens(json!({
        "asset_slugs": ["usdc"],
        "contract_addresses": [UNKNOWN_CONTRACT]
    }))
}

pub(crate) fn native_asset_rejection_request() -> Value {
    request_with_tokens(json!({
        "asset_slugs": ["ethereum"],
        "contract_addresses": []
    }))
}

pub(crate) fn unknown_slug_rejection_request() -> Value {
    request_with_tokens(json!({
        "asset_slugs": ["missing-but-syntactically-valid"],
        "contract_addresses": []
    }))
}

pub(crate) fn too_many_filters_request() -> Value {
    request_with_tokens(json!({
        "asset_slugs": [],
        "contract_addresses": too_many_contract_addresses()
    }))
}

#[cfg(test)]
pub(crate) fn unfiltered_success_response() -> Value {
    success_response(
        json!({
            "requested": {
                "asset_slugs": [],
                "contract_addresses": []
            },
            "resolved_contract_addresses": []
        }),
        Vec::new(),
        false,
    )
}

#[cfg(test)]
pub(crate) fn asset_slug_success_response() -> Value {
    success_response(
        json!({
            "requested": {
                "asset_slugs": ["usdc"],
                "contract_addresses": []
            },
            "resolved_contract_addresses": [usdc_resolved_filter()]
        }),
        Vec::new(),
        false,
    )
}

#[cfg(test)]
pub(crate) fn contract_address_success_response() -> Value {
    success_response(
        json!({
            "requested": {
                "asset_slugs": [],
                "contract_addresses": [UNKNOWN_CONTRACT]
            },
            "resolved_contract_addresses": [unknown_resolved_filter()]
        }),
        Vec::new(),
        false,
    )
}

pub(crate) fn mixed_success_response() -> Value {
    success_response(
        json!({
            "requested": {
                "asset_slugs": ["usdc"],
                "contract_addresses": [UNKNOWN_CONTRACT]
            },
            "resolved_contract_addresses": [
                usdc_resolved_filter(),
                unknown_resolved_filter()
            ]
        }),
        vec![usdc_transfer_row(), unknown_transfer_row()],
        false,
    )
}

pub(crate) fn truncated_success_response() -> Value {
    let mut response = mixed_success_response();
    response["limits"]["truncated"] = json!(true);
    response
}

pub(crate) fn native_asset_rejection_response() -> Value {
    error_response(
        "asset_not_erc20_on_network",
        "Asset is not an ERC-20 token on the requested network.",
    )
}

pub(crate) fn unknown_slug_rejection_response() -> Value {
    error_response("asset_not_found", "Asset was not found.")
}

pub(crate) fn too_many_filters_response() -> Value {
    error_response(
        "too_many_token_filters",
        "Too many token filters were requested.",
    )
}

pub(crate) fn window_too_large_response() -> Value {
    error_response(
        "window_too_large",
        "Transfer search window exceeds the public limit.",
    )
}

pub(crate) fn invalid_asset_slug_response() -> Value {
    error_response("invalid_asset_slug", "Asset slug is invalid.")
}

pub(crate) fn extraction_unavailable_response() -> Value {
    error_response(
        "extraction_unavailable",
        "ERC-20 transfer extraction is temporarily unavailable.",
    )
}

pub(crate) fn extraction_timeout_response() -> Value {
    error_response(
        "extraction_timeout",
        "ERC-20 transfer extraction timed out.",
    )
}

pub(crate) fn upstream_provider_error_response() -> Value {
    error_response(
        "upstream_provider_error",
        "ERC-20 transfer provider failed.",
    )
}

pub(crate) fn upstream_provider_timeout_response() -> Value {
    error_response(
        "upstream_provider_timeout",
        "ERC-20 transfer provider timed out.",
    )
}

pub(crate) fn internal_error_response() -> Value {
    error_response(
        "internal_error",
        "Mother API encountered an unexpected error.",
    )
}

fn request_with_tokens(tokens: Value) -> Value {
    json!({
        "account": {
            "network_slug": NETWORK_SLUG,
            "address": WATCHED_ADDRESS,
            "client_ref": "treasury-main"
        },
        "direction": "any",
        "tokens": tokens,
        "window": block_window()
    })
}

fn block_window() -> Value {
    json!({
        "from_block": 18600000,
        "to_block": 18600500
    })
}

fn success_response(token_filters: Value, transfers: Vec<Value>, truncated: bool) -> Value {
    json!({
        "ok": true,
        "type": "erc20_transfer_search",
        "account": {
            "network_slug": NETWORK_SLUG,
            "address": WATCHED_ADDRESS,
            "client_ref": "treasury-main"
        },
        "direction": "any",
        "window": block_window(),
        "token_filters": token_filters,
        "transfers": transfers,
        "limits": {
            "max_rows": 5000,
            "truncated": truncated
        }
    })
}

fn usdc_resolved_filter() -> Value {
    json!({
        "contract_address": USDC_CONTRACT,
        "asset_slug": "usdc",
        "symbol": "USDC",
        "decimals": 6,
        "source": "asset_slug"
    })
}

fn unknown_resolved_filter() -> Value {
    json!({
        "contract_address": UNKNOWN_CONTRACT,
        "asset_slug": null,
        "symbol": null,
        "decimals": null,
        "source": "contract_address"
    })
}

fn usdc_transfer_row() -> Value {
    json!({
        "block_number": 18600001,
        "tx_hash": TX_HASH_1,
        "log_index": 12,
        "token": {
            "contract_address": USDC_CONTRACT,
            "asset_slug": "usdc",
            "symbol": "USDC",
            "decimals": 6
        },
        "from": WATCHED_ADDRESS,
        "to": "0x2222222222222222222222222222222222222222",
        "amount": {
            "raw": "12500000",
            "decimal": "12.5"
        },
        "direction": "from"
    })
}

fn unknown_transfer_row() -> Value {
    json!({
        "block_number": 18600002,
        "tx_hash": TX_HASH_2,
        "log_index": 13,
        "token": {
            "contract_address": UNKNOWN_CONTRACT,
            "asset_slug": null,
            "symbol": null,
            "decimals": null
        },
        "from": "0x3333333333333333333333333333333333333333",
        "to": WATCHED_ADDRESS,
        "amount": {
            "raw": "1000000",
            "decimal": null
        },
        "direction": "to"
    })
}

fn error_response(code: &str, message: &str) -> Value {
    json!({
        "ok": false,
        "error": {
            "code": code,
            "message": message
        }
    })
}

fn too_many_contract_addresses() -> Vec<String> {
    (1..=21).map(|index| format!("0x{index:040x}")).collect()
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::*;

    #[test]
    fn contract_examples_match_shared_fixtures() {
        for (marker, expected) in [
            ("request-unfiltered", unfiltered_request()),
            ("response-unfiltered", unfiltered_success_response()),
            ("request-asset-slug", asset_slug_request()),
            ("response-asset-slug", asset_slug_success_response()),
            ("request-contract-address", contract_address_request()),
            (
                "response-contract-address",
                contract_address_success_response(),
            ),
            ("request-mixed-filters", mixed_filter_request()),
            ("response-mixed-filters", mixed_success_response()),
            (
                "request-native-asset-rejection",
                native_asset_rejection_request(),
            ),
            (
                "error-native-asset-rejection",
                native_asset_rejection_response(),
            ),
            (
                "request-unknown-slug-rejection",
                unknown_slug_rejection_request(),
            ),
            (
                "error-unknown-slug-rejection",
                unknown_slug_rejection_response(),
            ),
            ("request-too-many-filters", too_many_filters_request()),
            ("error-too-many-filters", too_many_filters_response()),
            ("response-truncated", truncated_success_response()),
        ] {
            assert_eq!(contract_json_example(marker), expected, "{marker}");
        }
    }

    fn contract_json_example(marker: &str) -> Value {
        let contract = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/CONTRACTS.md"));
        let marker = format!("<!-- erc20-transfer-example: {marker} -->");
        let after_marker = contract
            .split_once(&marker)
            .unwrap_or_else(|| panic!("missing contract marker {marker}"))
            .1;
        let after_fence = after_marker
            .split_once("```json")
            .unwrap_or_else(|| panic!("missing JSON fence for {marker}"))
            .1;
        let json_source = after_fence
            .split_once("```")
            .unwrap_or_else(|| panic!("missing closing JSON fence for {marker}"))
            .0
            .trim();

        serde_json::from_str(json_source)
            .unwrap_or_else(|error| panic!("invalid JSON for {marker}: {error}"))
    }
}
