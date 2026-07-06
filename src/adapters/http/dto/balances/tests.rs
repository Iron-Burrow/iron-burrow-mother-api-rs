use axum::{body, response::IntoResponse};
use serde_json::{json, Value};

use super::requests::{BulkBalanceRequest, SingleBalanceRequest};
use super::*;
use crate::adapters::http::error::ApiError;
use crate::{
    application::balances::service::{
        BalanceAccountResult, BalanceEvidence, BalanceItemOutcome, BalanceQuoteOutcome,
        BalanceTokenSelector, ResolvedBalanceTarget,
    },
    domain::assets::balance_catalog::BalanceTargetKind,
    test_utils::json::json_object,
};

#[test]
fn documented_request_examples_match_public_dto_shape() {
    let single = examples::single_request();
    let request: SingleBalanceRequest = serde_json::from_value(single.clone()).unwrap();
    assert_eq!(serde_json::to_value(request).unwrap(), single);

    let bulk = examples::bulk_request();
    let request: BulkBalanceRequest = serde_json::from_value(bulk.clone()).unwrap();
    assert_eq!(serde_json::to_value(request).unwrap(), bulk);
}

#[tokio::test]
async fn request_validation_rejects_unknown_fields_with_unknown_field() {
    for body in [
        {
            let mut body = examples::single_request();
            body["future"] = json!(true);
            body
        },
        {
            let mut body = examples::single_request();
            body["as_of"]["observed_at"] = json!("2026-06-18T12:00:00Z");
            body
        },
        {
            let mut body = examples::single_request();
            body["account"]["label"] = json!("treasury");
            body
        },
        {
            let mut body = examples::single_request();
            body["tokens"]["symbol"] = json!("ETH");
            body
        },
        {
            let mut body = examples::single_request();
            body["assets"] = json!([{"asset_slug": "ethereum"}]);
            body
        },
    ] {
        assert_api_error_code(
            SingleBalanceRequest::try_from(json_object(body)),
            "unknown_field",
        )
        .await;
    }

    for body in [
        {
            let mut body = examples::bulk_request();
            body["future"] = json!(true);
            body
        },
        {
            let mut body = examples::bulk_request();
            body["accounts"][0]["label"] = json!("base");
            body
        },
        {
            let mut body = examples::bulk_request();
            body["tokens"]["symbol"] = json!("USDC");
            body
        },
        {
            let mut body = examples::bulk_request();
            body["assets"] = json!([{"asset_slug": "usdc"}]);
            body
        },
    ] {
        assert_api_error_code(
            BulkBalanceRequest::try_from(json_object(body)),
            "unknown_field",
        )
        .await;
    }
}

#[tokio::test]
async fn request_validation_rejects_reserved_aliases_with_invalid_request() {
    for body in [
        {
            let mut body = examples::single_request();
            body["chain"] = json!("eth-mainnet");
            body
        },
        {
            let mut body = examples::single_request();
            body["as_of"]["chain_id"] = json!(1);
            body
        },
        {
            let mut body = examples::single_request();
            body["account"]["chain_slug"] = json!("eth-mainnet");
            body
        },
        {
            let mut body = examples::single_request();
            body["tokens"]["chain"] = json!("eth-mainnet");
            body
        },
        {
            let mut body = examples::single_request();
            body["future"] = json!({"chain": "eth-mainnet"});
            body
        },
    ] {
        assert_api_error_code(
            SingleBalanceRequest::try_from(json_object(body)),
            "invalid_request",
        )
        .await;
    }

    for body in [
        {
            let mut body = examples::bulk_request();
            body["chain_id"] = json!(1);
            body
        },
        {
            let mut body = examples::bulk_request();
            body["accounts"][0]["chain"] = json!("base-mainnet");
            body
        },
    ] {
        assert_api_error_code(
            BulkBalanceRequest::try_from(json_object(body)),
            "invalid_request",
        )
        .await;
    }
}

#[tokio::test]
async fn request_validation_rejects_invalid_as_of_field_values_with_invalid_request() {
    for body in [
        {
            let mut body = examples::single_request();
            body["as_of"]["kind"] = json!(null);
            body
        },
        {
            let mut body = examples::single_request();
            body["as_of"]["kind"] = json!("   ");
            body
        },
        {
            let mut body = examples::single_request();
            body["as_of"]["timestamp"] = json!(null);
            body
        },
        {
            let mut body = examples::single_request();
            body["as_of"]["timestamp"] = json!("   ");
            body
        },
        {
            let mut body = examples::single_request();
            body["as_of"]["timestamp"] = json!(123);
            body
        },
        {
            let mut body = examples::single_request();
            body["as_of"]["block_number"] = json!(null);
            body
        },
        {
            let mut body = examples::single_request();
            body["as_of"]["block_number"] = json!("   ");
            body
        },
        {
            let mut body = examples::single_request();
            body["as_of"]["block_number"] = json!(123);
            body
        },
    ] {
        assert_api_error_code(
            SingleBalanceRequest::try_from(json_object(body)),
            "invalid_request",
        )
        .await;
    }

    let mut historical = examples::single_request();
    historical["as_of"] = json!({
        "kind": "timestamp",
        "timestamp": "2026-07-03T00:00:00Z"
    });
    let request = SingleBalanceRequest::try_from(json_object(historical)).unwrap();
    assert_eq!(
        request.as_of.timestamp.as_deref(),
        Some("2026-07-03T00:00:00Z")
    );
}

#[tokio::test]
async fn request_validation_trims_string_fields_at_boundary() {
    let mut latest = examples::single_request();
    latest["as_of"]["kind"] = json!(" latest ");
    latest["account"]["network_slug"] = json!(" eth-mainnet ");
    latest["quote_currency"] = json!(" mxn ");

    let request = SingleBalanceRequest::try_from(json_object(latest)).unwrap();
    assert_eq!(request.as_of.kind, "latest");
    assert_eq!(request.account.network_slug, "eth-mainnet");
    assert_eq!(request.quote_currency, "mxn");

    let mut historical = examples::single_request();
    historical["as_of"] = json!({
        "kind": " timestamp ",
        "timestamp": " 2026-07-03T00:00:00Z "
    });

    let request = SingleBalanceRequest::try_from(json_object(historical)).unwrap();
    assert_eq!(request.as_of.kind, "timestamp");
    assert_eq!(
        request.as_of.timestamp.as_deref(),
        Some("2026-07-03T00:00:00Z")
    );
}

#[tokio::test]
async fn request_validation_rejects_blank_required_strings() {
    for (body, code) in [
        {
            let mut body = examples::single_request();
            body["quote_currency"] = json!("   ");
            (body, "invalid_request")
        },
        {
            let mut body = examples::single_request();
            body["account"]["network_slug"] = json!("   ");
            (body, "missing_network_slug")
        },
    ] {
        assert_api_error_code(SingleBalanceRequest::try_from(json_object(body)), code).await;
    }
}

#[tokio::test]
async fn request_validation_rejects_empty_missing_and_invalid_tokens() {
    for body in [
        {
            let mut body = examples::single_request();
            body.as_object_mut().unwrap().remove("tokens");
            body
        },
        {
            let mut body = examples::single_request();
            body["tokens"] = json!({});
            body
        },
        {
            let mut body = examples::single_request();
            body["tokens"] = json!({"asset_slugs": [], "contract_addresses": []});
            body
        },
    ] {
        assert_api_error_code(
            SingleBalanceRequest::try_from(json_object(body)),
            "empty_tokens",
        )
        .await;
    }

    for body in [
        {
            let mut body = examples::bulk_request();
            body["tokens"]["asset_slugs"] = json!(["USDC"]);
            body
        },
        {
            let mut body = examples::bulk_request();
            body["tokens"]["asset_slugs"] = json!([""]);
            body
        },
        {
            let mut body = examples::bulk_request();
            body["tokens"]["asset_slugs"] = json!(null);
            body
        },
    ] {
        assert_api_error_code(
            BulkBalanceRequest::try_from(json_object(body)),
            "invalid_asset_slug",
        )
        .await;
    }

    for body in [
        {
            let mut body = examples::single_request();
            body["tokens"]["contract_addresses"] = json!(["0x1234"]);
            body
        },
        {
            let mut body = examples::single_request();
            body["tokens"]["contract_addresses"] = json!([""]);
            body
        },
        {
            let mut body = examples::single_request();
            body["tokens"]["contract_addresses"] = json!(null);
            body
        },
    ] {
        assert_api_error_code(
            SingleBalanceRequest::try_from(json_object(body)),
            "invalid_contract_address",
        )
        .await;
    }
}

#[test]
fn documented_success_examples_match_public_dto_shape() {
    let single = examples::single_success_response();
    let response: SingleBalanceResponse = serde_json::from_value(single.clone()).unwrap();
    assert_eq!(serde_json::to_value(response).unwrap(), single);

    let single_failure = examples::single_item_level_failure_response();
    let response: SingleBalanceResponse = serde_json::from_value(single_failure.clone()).unwrap();
    assert_eq!(serde_json::to_value(response).unwrap(), single_failure);

    for example in [
        examples::bulk_success_response(),
        examples::skipped_item_response(),
        examples::item_level_failure_response(),
    ] {
        let response: BulkBalanceResponse = serde_json::from_value(example.clone()).unwrap();
        assert_eq!(serde_json::to_value(response).unwrap(), example);
    }
}

#[test]
fn documented_error_examples_match_public_error_envelope_shape() {
    for example in [
        examples::validation_error_response(),
        examples::request_too_large_response(),
    ] {
        assert_eq!(example["ok"], false);
        assert!(example["error"]["code"].is_string());
        assert!(example["error"]["message"].is_string());
        assert_error_shape(&example);
    }
}

#[test]
fn documented_balance_examples_do_not_expose_reserved_or_internal_fields() {
    for example in [
        examples::single_request(),
        examples::bulk_request(),
        examples::single_success_response(),
        examples::single_item_level_failure_response(),
        examples::bulk_success_response(),
        examples::skipped_item_response(),
        examples::item_level_failure_response(),
    ] {
        assert!(!contains_key(&example, "chain"));
        assert!(!contains_key(&example, "chain_id"));
        assert!(!contains_key(&example, "chain_slug"));
        assert!(!contains_key(&example, "route_id"));
        assert!(!contains_key(&example, "provider_id"));
        assert!(!contains_key(&example, "upstream_url"));
    }
}

#[test]
fn single_response_serializes_complete_shape_and_observation_time() {
    let response = BalanceResponseAssembler
        .single(snapshot(vec![resolved(
            "usdc",
            "450000000",
            "450.000000",
            BalanceQuoteOutcome::Available {
                currency: "MXN".to_string(),
                unit_price: "18.45".to_string(),
                value: "8302.500000".to_string(),
                price_as_of: "2026-06-17T11:59:59Z".to_string(),
            },
        )]))
        .unwrap();
    let value = serde_json::to_value(response).unwrap();

    assert_eq!(
        value,
        json!({
            "ok": true,
            "type": "balances",
            "status": "complete",
            "as_of": {
                "kind": "latest",
                "observed_at": "2026-06-17T12:00:00Z"
            },
            "quote_currency": "MXN",
            "account": {
                "network_slug": "base-mainnet",
                "address": "0x1111111111111111111111111111111111111111",
                "client_ref": "treasury"
            },
            "evidence": {
                "source": "bigwig",
                "network_slug": "base-mainnet",
                "block": {
                    "number": "123",
                    "hash": format!("0x{}", "a".repeat(64))
                },
                "observed_at": "2026-06-17T12:00:00Z"
            },
            "positions": [{
                "selector": {
                    "kind": "asset_slug",
                    "value": "usdc"
                },
                "network_slug": "base-mainnet",
                "contract_address": null,
                "asset_slug": "usdc",
                "symbol": "USDC",
                "balance": {
                    "raw_amount": "450000000",
                    "amount": "450.000000",
                    "decimals": 6
                },
                "quote": {
                    "status": "available",
                    "currency": "MXN",
                    "unit_price": "18.45",
                    "value": "8302.500000",
                    "price_as_of": "2026-06-17T11:59:59Z"
                }
            }],
            "skipped": [],
            "errors": []
        })
    );
}

#[test]
fn unavailable_and_unsupported_quotes_preserve_positions_and_make_partial() {
    let response = BalanceResponseAssembler.bulk(snapshot(vec![
        resolved(
            "usdc",
            "1",
            "0.000001",
            BalanceQuoteOutcome::Unavailable {
                code: BalanceItemErrorCode::PriceResolutionFailed,
            },
        ),
        resolved(
            "ethereum",
            "1",
            "0.000000000000000001",
            BalanceQuoteOutcome::Unsupported,
        ),
    ]));
    let value = serde_json::to_value(response).unwrap();

    assert_eq!(value["status"], "partial");
    assert_eq!(value["summary"]["positions_returned"], 2);
    assert_eq!(value["summary"]["failed_items"], 0);
    assert_eq!(
        value["accounts"][0]["positions"][0]["quote"]["status"],
        "unavailable"
    );
    assert_eq!(
        value["accounts"][0]["positions"][0]["quote"]["currency"],
        json!(null)
    );
    assert_eq!(
        value["accounts"][0]["positions"][1]["quote"]["status"],
        "unsupported"
    );
    assert_eq!(value["accounts"][0]["errors"].as_array().unwrap().len(), 1);
    assert_eq!(
        value["accounts"][0]["errors"][0]["code"],
        "price_resolution_failed"
    );
    assert_eq!(
        value["accounts"][0]["errors"][0]["message"],
        "Quote could not be resolved for this asset."
    );
}

#[test]
fn skipped_only_is_complete_and_all_supported_failures_are_failed() {
    let skipped = BalanceResponseAssembler.bulk(snapshot(vec![BalanceItemOutcome::Skipped {
        network_slug: "base-mainnet".to_string(),
        asset_slug: "bitso-mxn".to_string(),
    }]));
    let skipped = serde_json::to_value(skipped).unwrap();
    assert_eq!(skipped["status"], "complete");
    assert_eq!(skipped["summary"]["skipped_items"], 1);
    assert_eq!(skipped["summary"]["failed_items"], 0);

    let failed = BalanceResponseAssembler.bulk(snapshot(vec![BalanceItemOutcome::Failed {
        target: target("usdc"),
        code: BalanceItemErrorCode::BalanceProviderUnavailable,
    }]));
    let failed = serde_json::to_value(failed).unwrap();
    assert_eq!(failed["status"], "failed");
    assert_eq!(failed["summary"]["failed_items"], 1);
    assert_eq!(
        failed["accounts"][0]["errors"][0]["message"],
        "Balance is temporarily unavailable for this asset on this network."
    );
}

#[test]
fn bulk_status_is_partial_when_any_account_degrades_but_another_resolves() {
    let mut first = account_result(vec![resolved(
        "usdc",
        "1000000",
        "1.000000",
        available_quote(),
    )]);
    first.account.address = "0x1111111111111111111111111111111111111111".to_string();
    let mut second = account_result(vec![BalanceItemOutcome::Failed {
        target: target("usdc"),
        code: BalanceItemErrorCode::BalanceResolutionFailed,
    }]);
    second.account.address = "0x2222222222222222222222222222222222222222".to_string();
    let response = BalanceResponseAssembler.bulk(BalanceSnapshotResult {
        quote_currency: "MXN".to_string(),
        requested_token_count: 1,
        accounts: vec![first, second],
    });
    let value = serde_json::to_value(response).unwrap();

    assert_eq!(value["status"], "partial");
    assert_eq!(value["accounts"][0]["status"], "complete");
    assert_eq!(value["accounts"][1]["status"], "failed");
    assert_eq!(value["as_of"], json!({"kind": "latest"}));
    assert!(value["as_of"].get("observed_at").is_none());
}

#[test]
fn single_without_evidence_serializes_null_observed_at_and_evidence() {
    let mut snapshot = snapshot(vec![BalanceItemOutcome::Skipped {
        network_slug: "base-mainnet".to_string(),
        asset_slug: "bitso-mxn".to_string(),
    }]);
    snapshot.accounts[0].evidence = None;
    let response = BalanceResponseAssembler.single(snapshot).unwrap();
    let value = serde_json::to_value(response).unwrap();

    assert_eq!(value["as_of"]["observed_at"], json!(null));
    assert_eq!(value["evidence"], json!(null));
}

fn snapshot(items: Vec<BalanceItemOutcome>) -> BalanceSnapshotResult {
    BalanceSnapshotResult {
        quote_currency: "MXN".to_string(),
        requested_token_count: 2,
        accounts: vec![account_result(items)],
    }
}

fn account_result(items: Vec<BalanceItemOutcome>) -> BalanceAccountResult {
    BalanceAccountResult {
        account: OnchainAccount {
            network_slug: "base-mainnet".to_string(),
            address: "0x1111111111111111111111111111111111111111".to_string(),
            client_ref: Some("treasury".to_string()),
        },
        evidence: Some(BalanceEvidence {
            network_slug: "base-mainnet".to_string(),
            observed_at: "2026-06-17T12:00:00Z".to_string(),
            block_number: "123".to_string(),
            block_hash: format!("0x{}", "a".repeat(64)),
        }),
        items,
    }
}

fn resolved(
    asset_slug: &str,
    raw_amount: &str,
    amount: &str,
    quote: BalanceQuoteOutcome,
) -> BalanceItemOutcome {
    BalanceItemOutcome::Resolved {
        target: target(asset_slug),
        raw_amount: raw_amount.to_string(),
        amount: Some(amount.to_string()),
        quote,
    }
}

fn available_quote() -> BalanceQuoteOutcome {
    BalanceQuoteOutcome::Available {
        currency: "MXN".to_string(),
        unit_price: "18.45".to_string(),
        value: "18.450000".to_string(),
        price_as_of: "2026-06-17T11:59:59Z".to_string(),
    }
}

fn target(asset_slug: &str) -> ResolvedBalanceTarget {
    ResolvedBalanceTarget {
        selector: BalanceTokenSelector::AssetSlug(asset_slug.to_string()),
        network_slug: "base-mainnet".to_string(),
        chain_id: 8453,
        asset_slug: Some(asset_slug.to_string()),
        symbol: Some(asset_slug.to_ascii_uppercase()),
        name: Some(format!("{asset_slug} display name")),
        decimals: Some(if asset_slug == "ethereum" { 18 } else { 6 }),
        pricing_asset_slug: Some(asset_slug.to_string()),
        kind: BalanceTargetKind::Native,
    }
}

fn assert_error_shape(json: &Value) {
    let top_level = json
        .as_object()
        .expect("error response should be an object");
    assert_eq!(top_level.len(), 2);
    assert!(top_level.contains_key("ok"));
    assert!(top_level.contains_key("error"));

    let error = json["error"]
        .as_object()
        .expect("error body should be an object");
    assert_eq!(error.len(), 2);
    assert!(error.contains_key("code"));
    assert!(error.contains_key("message"));
}

fn contains_key(value: &Value, needle: &str) -> bool {
    match value {
        Value::Object(object) => object
            .iter()
            .any(|(key, value)| key == needle || contains_key(value, needle)),
        Value::Array(values) => values.iter().any(|value| contains_key(value, needle)),
        _ => false,
    }
}

async fn assert_api_error_code<T>(result: Result<T, ApiError>, expected_code: &str)
where
    T: std::fmt::Debug,
{
    let response = result.unwrap_err().into_response();
    let body = body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["ok"], false);
    assert_eq!(json["error"]["code"], expected_code);
}
