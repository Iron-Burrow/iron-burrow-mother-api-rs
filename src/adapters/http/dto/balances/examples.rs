use serde_json::{json, Value};

const ETH_NETWORK: &str = "eth-mainnet";
const BASE_NETWORK: &str = "base-mainnet";
const ACCOUNT_A: &str = "0x1234567890abcdef1234567890abcdef1234beef";
const ACCOUNT_B: &str = "0x2222222222222222222222222222222222222222";

pub(crate) fn single_request() -> Value {
    json!({
        "as_of": {
            "kind": "latest"
        },
        "account": {
            "network_slug": ETH_NETWORK,
            "address": ACCOUNT_A,
            "client_ref": "main-safe"
        },
        "quote_currency": "MXN",
        "tokens": {
            "asset_slugs": ["ethereum"],
            "contract_addresses": []
        }
    })
}

pub(crate) fn bulk_request() -> Value {
    json!({
        "as_of": {
            "kind": "latest"
        },
        "accounts": [
            {
                "network_slug": BASE_NETWORK,
                "address": ACCOUNT_A,
                "client_ref": "treasury-base"
            },
            {
                "network_slug": ETH_NETWORK,
                "address": ACCOUNT_B,
                "client_ref": "treasury-eth"
            }
        ],
        "quote_currency": "USD",
        "tokens": {
            "asset_slugs": ["usdc", "ethereum"],
            "contract_addresses": []
        }
    })
}

pub(crate) fn single_success_response() -> Value {
    json!({
        "ok": true,
        "type": "balances",
        "status": "complete",
        "as_of": {
            "kind": "latest",
            "observed_at": "2026-06-18T12:00:00Z"
        },
        "quote_currency": "MXN",
        "account": {
            "network_slug": ETH_NETWORK,
            "address": ACCOUNT_A,
            "client_ref": "main-safe"
        },
        "evidence": eth_evidence(),
        "positions": [
            ethereum_position("MXN", "35000.50", "35000.500000000000000000")
        ],
        "skipped": [],
        "errors": []
    })
}

pub(crate) fn single_item_level_failure_response() -> Value {
    json!({
        "ok": true,
        "type": "balances",
        "status": "failed",
        "as_of": {
            "kind": "latest",
            "observed_at": null
        },
        "quote_currency": "MXN",
        "account": {
            "network_slug": ETH_NETWORK,
            "address": ACCOUNT_A,
            "client_ref": "main-safe"
        },
        "evidence": null,
        "positions": [],
        "skipped": [],
        "errors": [
            {
                "network_slug": ETH_NETWORK,
                "asset_slug": "ethereum",
                "code": "balance_provider_unavailable",
                "message": "Balance is temporarily unavailable for this asset on this network."
            }
        ]
    })
}

pub(crate) fn bulk_success_response() -> Value {
    json!({
        "ok": true,
        "type": "balances_bulk",
        "status": "complete",
        "as_of": {
            "kind": "latest"
        },
        "quote_currency": "USD",
        "summary": {
            "requested_accounts": 2,
            "requested_assets": 2,
            "requested_resolution_items": 4,
            "positions_returned": 3,
            "skipped_items": 1,
            "failed_items": 0
        },
        "accounts": [
            {
                "status": "complete",
                "account": {
                    "network_slug": BASE_NETWORK,
                    "address": ACCOUNT_A,
                    "client_ref": "treasury-base"
                },
                "evidence": base_evidence(),
                "positions": [
                    usdc_position(BASE_NETWORK, "USD", "1.00", "1.250000")
                ],
                "skipped": [
                    skipped_item(BASE_NETWORK, "ethereum")
                ],
                "errors": []
            },
            {
                "status": "complete",
                "account": {
                    "network_slug": ETH_NETWORK,
                    "address": ACCOUNT_B,
                    "client_ref": "treasury-eth"
                },
                "evidence": eth_evidence(),
                "positions": [
                    usdc_position(ETH_NETWORK, "USD", "1.00", "12.500000"),
                    ethereum_position("USD", "2000.00", "4000.000000000000000000")
                ],
                "skipped": [],
                "errors": []
            }
        ],
        "errors": []
    })
}

pub(crate) fn validation_error_response() -> Value {
    error_response("invalid_request", "Request parameters are invalid.")
}

pub(crate) fn skipped_item_response() -> Value {
    json!({
        "ok": true,
        "type": "balances_bulk",
        "status": "complete",
        "as_of": {
            "kind": "latest"
        },
        "quote_currency": "USD",
        "summary": {
            "requested_accounts": 1,
            "requested_assets": 2,
            "requested_resolution_items": 2,
            "positions_returned": 1,
            "skipped_items": 1,
            "failed_items": 0
        },
        "accounts": [
            {
                "status": "complete",
                "account": {
                    "network_slug": BASE_NETWORK,
                    "address": ACCOUNT_A,
                    "client_ref": "treasury-base"
                },
                "evidence": base_evidence(),
                "positions": [
                    usdc_position(BASE_NETWORK, "USD", "1.00", "1.250000")
                ],
                "skipped": [
                    skipped_item(BASE_NETWORK, "bitso-mxn")
                ],
                "errors": []
            }
        ],
        "errors": []
    })
}

pub(crate) fn item_level_failure_response() -> Value {
    json!({
        "ok": true,
        "type": "balances_bulk",
        "status": "partial",
        "as_of": {
            "kind": "latest"
        },
        "quote_currency": "USD",
        "summary": {
            "requested_accounts": 1,
            "requested_assets": 2,
            "requested_resolution_items": 2,
            "positions_returned": 1,
            "skipped_items": 0,
            "failed_items": 1
        },
        "accounts": [
            {
                "status": "partial",
                "account": {
                    "network_slug": BASE_NETWORK,
                    "address": ACCOUNT_A,
                    "client_ref": "treasury-base"
                },
                "evidence": base_evidence(),
                "positions": [
                    usdc_position(BASE_NETWORK, "USD", "1.00", "1.250000")
                ],
                "skipped": [],
                "errors": [
                    {
                        "network_slug": BASE_NETWORK,
                        "asset_slug": "ethereum",
                        "code": "balance_provider_unavailable",
                        "message": "Balance is temporarily unavailable for this asset on this network."
                    }
                ]
            }
        ],
        "errors": []
    })
}

pub(crate) fn request_too_large_response() -> Value {
    error_response(
        "request_too_large",
        "Balance request exceeds the public limits.",
    )
}

fn eth_evidence() -> Value {
    evidence(ETH_NETWORK, "22900000", &format!("0x{}", "a".repeat(64)))
}

fn base_evidence() -> Value {
    evidence(BASE_NETWORK, "32000000", &format!("0x{}", "b".repeat(64)))
}

fn evidence(network_slug: &str, block_number: &str, block_hash: &str) -> Value {
    json!({
        "source": "bigwig",
        "network_slug": network_slug,
        "block": {
            "number": block_number,
            "hash": block_hash
        },
        "observed_at": "2026-06-18T12:00:00Z"
    })
}

fn ethereum_position(currency: &str, unit_price: &str, value: &str) -> Value {
    json!({
        "network_slug": ETH_NETWORK,
        "asset_slug": "ethereum",
        "symbol": "ETH",
        "balance": {
            "raw_amount": "1000000000000000000",
            "amount": "1.000000000000000000",
            "decimals": 18
        },
        "quote": {
            "status": "available",
            "currency": currency,
            "unit_price": unit_price,
            "value": value,
            "price_as_of": "2026-06-18T11:59:59Z"
        }
    })
}

fn usdc_position(network_slug: &str, currency: &str, unit_price: &str, value: &str) -> Value {
    json!({
        "network_slug": network_slug,
        "asset_slug": "usdc",
        "symbol": "USDC",
        "balance": {
            "raw_amount": "1250000",
            "amount": "1.250000",
            "decimals": 6
        },
        "quote": {
            "status": "available",
            "currency": currency,
            "unit_price": unit_price,
            "value": value,
            "price_as_of": "2026-06-18T11:59:59Z"
        }
    })
}

fn skipped_item(network_slug: &str, asset_slug: &str) -> Value {
    json!({
        "network_slug": network_slug,
        "asset_slug": asset_slug,
        "reason": "asset_not_supported_on_network"
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
