use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
    time::Duration,
};

use reqwest::StatusCode;
use serde_json::{json, Value};

use super::*;
use crate::application::erc20_transfers::service::Erc20TransferExtractionRequest;
use crate::domain::onchain_time::onchain_window::{
    BlockWindow, LookbackWindow, OnchainWindow, TimestampWindow,
};
use crate::domain::transfers::transfer_direction::TransferDirection;

const ADDRESS: &str = "0xABC0000000000000000000000000000000000000";
const CONTRACT: &str = "0xA0B86991C6218B36C1D19D4A2E9EB0CE3606EB48";

#[test]
fn request_serializes_to_exact_bigwig_shape_without_public_fields() {
    let value = serde_json::to_value(BigwigErc20TransferRequest::from(extraction_request(
        block_window(),
        vec![CONTRACT.to_string()],
    )))
    .unwrap();

    assert_eq!(
        value,
        json!({
            "network_slug": "eth-mainnet",
            "address": "0xabc0000000000000000000000000000000000000",
            "direction": "any",
            "contract_addresses": ["0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"],
            "window": {
                "from_block": 18_600_000,
                "to_block": 18_600_500
            }
        })
    );

    let serialized = value.to_string();
    for forbidden in [
        "asset_slug",
        "asset_slugs",
        "token_filters",
        "transfers",
        "limits",
        "decimal",
        "symbol",
    ] {
        assert!(!serialized.contains(forbidden));
    }
}

#[test]
fn empty_contract_addresses_preserve_unfiltered_search() {
    let value = serde_json::to_value(BigwigErc20TransferRequest::from(extraction_request(
        block_window(),
        Vec::new(),
    )))
    .unwrap();

    assert_eq!(value["contract_addresses"], json!([]));
}

#[test]
fn timestamp_window_serializes_correctly() {
    let value = serde_json::to_value(BigwigErc20TransferRequest::from(extraction_request(
        OnchainWindow::Timestamp(
            TimestampWindow::new(
                "2026-06-25T00:00:00Z".to_string(),
                "2026-06-25T01:00:00Z".to_string(),
            )
            .unwrap(),
        ),
        Vec::new(),
    )))
    .unwrap();

    assert_eq!(
        value["window"],
        json!({
            "from_timestamp": "2026-06-25T00:00:00Z",
            "to_timestamp": "2026-06-25T01:00:00Z"
        })
    );
}

#[test]
fn lookback_window_serializes_correctly() {
    let value = serde_json::to_value(BigwigErc20TransferRequest::from(extraction_request(
        OnchainWindow::Lookback(LookbackWindow::latest(600).unwrap()),
        Vec::new(),
    )))
    .unwrap();

    assert_eq!(
        value["window"],
        json!({"lookback_seconds": 600, "to": "latest"})
    );
}

#[test]
fn success_response_defaults_missing_truncated_to_false() {
    let response = serde_json::from_value::<BigwigErc20TransferResponse>(success_body())
        .expect("fixture should match Bigwig success response");
    let extraction =
        Erc20TransferExtractionResult::try_from(response).expect("fixture should convert");

    assert!(!extraction.truncated);
}

#[test]
fn success_response_propagates_truncated_flag() {
    let mut body = success_body();
    body["truncated"] = json!(true);
    let response = serde_json::from_value::<BigwigErc20TransferResponse>(body)
        .expect("fixture should match Bigwig success response");
    let extraction =
        Erc20TransferExtractionResult::try_from(response).expect("fixture should convert");

    assert!(extraction.truncated);
}

#[tokio::test]
async fn client_sends_authenticated_transfer_extraction_request() {
    let Some((base_url, handle)) = spawn_server(StatusCode::OK, success_body(), &[]) else {
        return;
    };
    let client = BigwigClient::new(&base_url, "test-token", 2_000).unwrap();

    let response = BigwigClient::search_erc20_transfers(
        &client,
        &BigwigErc20TransferRequest::from(extraction_request(
            block_window(),
            vec![CONTRACT.to_string()],
        )),
    )
    .await
    .unwrap();
    let request = handle.join().unwrap();
    let (headers, body) = split_request(&request);

    assert!(headers.starts_with("POST /internal/v1/extractions/erc20-transfers HTTP/1.1\r\n"));
    assert_header(headers, "authorization", "Bearer test-token");
    assert_header(headers, "x-client-service", "mother-api");
    assert_header(headers, "content-type", "application/json");
    assert_eq!(
        serde_json::from_str::<Value>(body).unwrap(),
        json!({
            "network_slug": "eth-mainnet",
            "address": "0xabc0000000000000000000000000000000000000",
            "direction": "any",
            "contract_addresses": ["0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"],
            "window": {
                "from_block": 18_600_000,
                "to_block": 18_600_500
            }
        })
    );
    assert_eq!(response.rows_extracted, 1);
}

#[tokio::test]
async fn port_maps_bigwig_runtime_errors_to_pr4_extraction_errors() {
    assert_eq!(
        map_bigwig_transfer_error(BigwigError::RangeTooLarge),
        Erc20TransferExtractionError::WindowTooLarge
    );
    assert_eq!(
        map_bigwig_transfer_error(BigwigError::LookbackTooLarge),
        Erc20TransferExtractionError::WindowTooLarge
    );
    assert_eq!(
        map_bigwig_transfer_error(BigwigError::ReversedBlockRange),
        Erc20TransferExtractionError::InvalidWindow
    );
    assert_eq!(
        map_bigwig_transfer_error(BigwigError::ExtractionTimeout),
        Erc20TransferExtractionError::ExtractionTimeout
    );
    assert_eq!(
        map_bigwig_transfer_error(BigwigError::RpcError),
        Erc20TransferExtractionError::UpstreamProviderError
    );
    assert_eq!(
        map_bigwig_transfer_error(BigwigError::ProviderTimeout),
        Erc20TransferExtractionError::UpstreamProviderTimeout
    );
    assert_eq!(
        map_bigwig_transfer_error(BigwigError::Timeout),
        Erc20TransferExtractionError::ExtractionTimeout
    );
    assert_eq!(
        map_bigwig_transfer_error(BigwigError::Transport),
        Erc20TransferExtractionError::ExtractionUnavailable
    );
    assert_eq!(
        map_bigwig_transfer_error(BigwigError::InvalidExtractionRequest),
        Erc20TransferExtractionError::InternalError
    );
    assert_eq!(
        map_bigwig_transfer_error(BigwigError::MalformedSuccessResponse),
        Erc20TransferExtractionError::InternalError
    );
}

#[tokio::test]
async fn malformed_success_body_is_handled_safely_by_port() {
    let Some((base_url, handle)) = spawn_server(StatusCode::OK, json!({"extractor": "wrong"}), &[])
    else {
        return;
    };
    let client = BigwigClient::new(&base_url, "test-token", 2_000).unwrap();

    let error = Erc20TransferExtractor::search_erc20_transfers(
        &client,
        extraction_request(block_window(), Vec::new()),
    )
    .await
    .expect_err("malformed success response should be an internal error");

    assert_eq!(error, Erc20TransferExtractionError::InternalError);
    handle.join().unwrap();
}

#[tokio::test]
async fn transport_and_timeout_are_classified_for_port() {
    let Ok(listener) = TcpListener::bind("127.0.0.1:0") else {
        return;
    };
    let closed_url = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let client = BigwigClient::new(&closed_url, "test-token", 2_000).unwrap();

    assert_eq!(
        Erc20TransferExtractor::search_erc20_transfers(
            &client,
            extraction_request(block_window(), Vec::new()),
        )
        .await,
        Err(Erc20TransferExtractionError::ExtractionUnavailable)
    );

    let Ok(listener) = TcpListener::bind("127.0.0.1:0") else {
        return;
    };
    let timeout_url = format!("http://{}", listener.local_addr().unwrap());
    let handle = thread::spawn(move || {
        let (_stream, _) = listener.accept().unwrap();
        thread::sleep(Duration::from_millis(100));
    });
    let client = BigwigClient::new(&timeout_url, "test-token", 10).unwrap();

    assert_eq!(
        Erc20TransferExtractor::search_erc20_transfers(
            &client,
            extraction_request(block_window(), Vec::new()),
        )
        .await,
        Err(Erc20TransferExtractionError::ExtractionTimeout)
    );
    handle.join().unwrap();
}

fn extraction_request(
    window: OnchainWindow,
    contract_addresses: Vec<String>,
) -> Erc20TransferExtractionRequest {
    Erc20TransferExtractionRequest {
        network_slug: "eth-mainnet".to_string(),
        address: ADDRESS.to_string(),
        direction: TransferDirection::Any,
        window,
        contract_addresses,
    }
}

fn block_window() -> OnchainWindow {
    OnchainWindow::Block(BlockWindow::new(18_600_000, 18_600_500).unwrap())
}

fn success_body() -> Value {
    json!({
        "extractor": "evm_erc20_transfers_by_address",
        "network_slug": "eth-mainnet",
        "address": "0xabc0000000000000000000000000000000000000",
        "direction": "any",
        "window_kind": "block",
        "from_block": 18_600_000,
        "to_block": 18_600_500,
        "latest_block": 18_600_500,
        "safe_block": 18_600_488,
        "finality": {
            "status": "mixed",
            "safe_block": 18_600_488,
            "latest_block": 18_600_500,
            "reorg_risk": true,
            "policy": "confirmation_lag",
            "confirmation_lag": 12
        },
        "rows_extracted": 1,
        "results": [{
            "block_number": 18_600_001,
            "tx_hash": "0x0000000000000000000000000000000000000000000000000000000000000001",
            "log_index": 12,
            "token": "0xA0B86991C6218B36C1D19D4A2E9EB0CE3606EB48",
            "from": "0xABC0000000000000000000000000000000000000",
            "to": "0x2222222222222222222222222222222222222222",
            "value": "1000000"
        }]
    })
}

fn spawn_server(
    status: StatusCode,
    body: Value,
    extra_headers: &[(&str, &str)],
) -> Option<(String, thread::JoinHandle<String>)> {
    let listener = match TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => listener,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return None,
        Err(error) => panic!("failed to bind Bigwig transfer test server: {error}"),
    };
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    let extra_headers = extra_headers
        .iter()
        .map(|(name, value)| (name.to_string(), value.to_string()))
        .collect::<Vec<_>>();
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let request = read_http_request(&mut stream);
        write_json_response(&mut stream, status, body, &extra_headers);
        request
    });

    Some((base_url, handle))
}

fn read_http_request(stream: &mut impl Read) -> String {
    let mut request = Vec::new();
    let mut buffer = [0; 1024];

    loop {
        let bytes_read = stream.read(&mut buffer).unwrap();
        if bytes_read == 0 {
            break;
        }
        request.extend_from_slice(&buffer[..bytes_read]);

        let Some(headers_end) = request.windows(4).position(|window| window == b"\r\n\r\n") else {
            continue;
        };
        let headers = String::from_utf8_lossy(&request[..headers_end]);
        let content_length = headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().ok())
                    .flatten()
            })
            .unwrap_or(0);
        if request.len() >= headers_end + 4 + content_length {
            break;
        }
    }

    String::from_utf8(request).unwrap()
}

fn write_json_response(
    stream: &mut impl Write,
    status: StatusCode,
    body: Value,
    extra_headers: &[(String, String)],
) {
    let body = serde_json::to_string(&body).unwrap();
    let reason = status.canonical_reason().unwrap_or("Unknown");
    let mut headers = String::new();
    for (name, value) in extra_headers {
        headers.push_str(name);
        headers.push_str(": ");
        headers.push_str(value);
        headers.push_str("\r\n");
    }
    let response = format!(
            "HTTP/1.1 {} {}\r\ncontent-type: application/json\r\n{}content-length: {}\r\nconnection: close\r\n\r\n{}",
            status.as_u16(),
            reason,
            headers,
            body.len(),
            body
        );
    stream.write_all(response.as_bytes()).unwrap();
}

fn split_request(request: &str) -> (&str, &str) {
    request.split_once("\r\n\r\n").unwrap()
}

fn assert_header(headers: &str, name: &str, expected_value: &str) {
    let expected = format!("{name}: {expected_value}");
    assert!(
        headers
            .lines()
            .any(|line| line.eq_ignore_ascii_case(&expected)),
        "missing header {expected}; headers were:\n{headers}"
    );
}
