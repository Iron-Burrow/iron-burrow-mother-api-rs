use axum::Router;
use reqwest::StatusCode;
use serde_json::Value;

use axum::body::Body;
use tower::ServiceExt;

use axum::http::Request;

// use crate::{
//     application::{
//         erc20_transfers::service::{Erc20TransferCommandTokenFilters, Erc20TransferSearchCommand},
//         filters::{
//             onchain_window::{BlockWindow, OnchainWindow},
//             transfer_direction::TransferDirection,
//         },
//     },
//     common::rfc3339::parse_rfc3339,
//     test_utils::{
//         errors::assert_public_error,
//         fixtures::{
//             erc20_transfers::{
//                 erc20_transfers_command_from_body, erc20_transfers_request_with_tokens_body,
//                 erc20_transfers_without_tokens_body, valid_erc20_transfers_request_body,
//             },
//             global_assets::{global_assets_repository, sample_assets},
//             router::transfers_router,
//         },
//         json::json_object,
//     },
// };

pub(crate) async fn post_json(app: Router, uri: &str, body: Value) -> (StatusCode, Value) {
    post_raw(
        app,
        uri,
        Some("application/json"),
        serde_json::to_vec(&body).unwrap(),
    )
    .await
}

pub(crate) async fn post_raw(
    app: Router,
    uri: &str,
    content_type: Option<&str>,
    body: Vec<u8>,
) -> (StatusCode, Value) {
    let mut request = Request::builder().method("POST").uri(uri);
    if let Some(content_type) = content_type {
        request = request.header("content-type", content_type);
    }
    let response = app
        .oneshot(request.body(Body::from(body)).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json = serde_json::from_slice(&body).unwrap();

    (status, json)
}
