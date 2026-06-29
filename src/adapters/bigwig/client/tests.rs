use crate::{
    application::balances::{
        catalog::CatalogBalanceTargetResolver,
        quote::PriceQuoteClient,
        service::{
            BalanceItemErrorCode, BalanceItemOutcome, BalanceSnapshotAccount,
            BalanceSnapshotRequest, BalanceSnapshotService,
        },
    },
    test_utils::constants::INFRA_GATEWAY_URL,
};

use super::*;

use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
};

use reqwest::StatusCode;
use serde_json::{json, Value};

const ACCOUNT_A: &str = "0x1111111111111111111111111111111111111111";

use crate::adapters::postgres::global_assets::GlobalAssetRepository;
use crate::test_utils::fixtures::global_assets::sample_assets;

#[tokio::test]
async fn malformed_success_body_becomes_internal_item_failure() {
    let Some((base_url, server)) =
        spawn_static_server(StatusCode::OK, json!({ "primitive": "wrong" }))
    else {
        return;
    };
    let result = service(Some(bigwig_client(&base_url)))
        .resolve_latest(BalanceSnapshotRequest {
            accounts: vec![account("base-mainnet", ACCOUNT_A, None)],
            asset_slugs: vec!["usdc".to_string()],
            quote_currency: "USD".to_string(),
        })
        .await
        .unwrap();
    server.join().unwrap();

    assert_eq!(result.accounts[0].evidence, None);
    assert!(matches!(
        &result.accounts[0].items[0],
        BalanceItemOutcome::Failed {
            code: BalanceItemErrorCode::InternalError,
            ..
        }
    ));
}

fn account(network_slug: &str, address: &str, client_ref: Option<&str>) -> BalanceSnapshotAccount {
    BalanceSnapshotAccount {
        network_slug: network_slug.to_string(),
        address: address.to_string(),
        client_ref: client_ref.map(str::to_string),
    }
}

fn bigwig_client(base_url: &str) -> BigwigClient {
    BigwigClient::new(base_url, "test-token", 2_000).unwrap()
}

fn spawn_static_server(
    status: StatusCode,
    body: Value,
) -> Option<(String, thread::JoinHandle<()>)> {
    let listener = match TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => listener,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return None,
        Err(error) => panic!("failed to bind orchestration test server: {error}"),
    };
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let _request = read_http_request(&mut stream);
        write_json_response(&mut stream, status, body);
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

fn write_json_response(stream: &mut impl Write, status: StatusCode, body: Value) {
    let body = serde_json::to_string(&body).unwrap();
    let reason = status.canonical_reason().unwrap_or("Unknown");
    let response = format!(
            "HTTP/1.1 {} {}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            status.as_u16(),
            reason,
            body.len(),
            body
        );
    stream.write_all(response.as_bytes()).unwrap();
}

fn valid_config() -> Config {
    Config {
        infra_gateway_url: Some(INFRA_GATEWAY_URL.to_string()),
        infra_gateway_token: Some("test-token".to_string()),
        ..Config::default()
    }
}

#[test]
fn try_from_valid_config_creates_client() {
    let config = valid_config();

    let client = BigwigClient::try_from(&config).expect("valid config should create a client");

    assert_eq!(client.base_host(), Some("infra-gateway-hub"));
    assert_eq!(client.timeout_ms(), 30000);
}

#[test]
fn try_from_missing_url_fails() {
    let config = Config {
        infra_gateway_token: Some("test-token".to_string()),
        ..Config::default()
    };

    let error = BigwigClient::try_from(&config).expect_err("missing URL should fail");

    assert_eq!(error, BigwigClientInitError::MissingBaseUrl);
}

#[test]
fn try_from_missing_token_fails() {
    let config = Config {
        infra_gateway_url: Some(INFRA_GATEWAY_URL.to_string()),
        ..Config::default()
    };

    let error = BigwigClient::try_from(&config).expect_err("missing token should fail");

    assert_eq!(error, BigwigClientInitError::MissingToken);
}

#[test]
fn client_new_rejects_empty_token() {
    let error =
        BigwigClient::new(INFRA_GATEWAY_URL, " ", 30000).expect_err("empty token should fail");

    assert_eq!(error, BigwigClientInitError::EmptyToken);
}

#[test]
fn client_new_rejects_zero_timeout() {
    let error = BigwigClient::new(INFRA_GATEWAY_URL, "test-token", 0)
        .expect_err("zero timeout should fail");

    assert_eq!(error, BigwigClientInitError::InvalidTimeout);
}

#[test]
fn create_client_returns_none_when_bigwig_config_is_absent() {
    assert!(create_bigwig_client(&Config::default()).is_none());
}

#[test]
fn create_client_returns_none_for_partial_or_invalid_bigwig_config() {
    for config in [
        Config {
            infra_gateway_url: Some(INFRA_GATEWAY_URL.to_string()),
            ..Config::default()
        },
        Config {
            infra_gateway_token: Some("test-token".to_string()),
            ..Config::default()
        },
        Config {
            infra_gateway_url: Some("not a url".to_string()),
            infra_gateway_token: Some("test-token".to_string()),
            ..Config::default()
        },
        Config {
            infra_gateway_url: Some(INFRA_GATEWAY_URL.to_string()),
            infra_gateway_token: Some(" ".to_string()),
            ..Config::default()
        },
        Config {
            infra_gateway_url: Some(INFRA_GATEWAY_URL.to_string()),
            infra_gateway_token: Some("test-token".to_string()),
            bigwig_request_timeout_ms: 0,
            ..Config::default()
        },
    ] {
        assert!(create_bigwig_client(&config).is_none());
    }
}

fn service(client: Option<BigwigClient>) -> BalanceSnapshotService {
    service_with_quote(client, None)
}

fn service_with_quote(
    client: Option<BigwigClient>,
    price_quote_client: Option<PriceQuoteClient>,
) -> BalanceSnapshotService {
    BalanceSnapshotService::new(
        CatalogBalanceTargetResolver::new(GlobalAssetRepository::in_memory(sample_assets())),
        client,
        price_quote_client,
    )
}
