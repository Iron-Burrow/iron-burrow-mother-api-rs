use super::*;
use crate::test_utils::constants::{DIS_BASE_URL, INFRA_GATEWAY_URL};

#[test]
fn missing_dis_base_url_disables_client() {
    let state = AppState::new(Config::default());

    assert!(state.dis_client.is_none());
}

#[test]
fn valid_dis_base_url_creates_client() {
    let state = AppState::new(Config {
        dis_base_url: Some(DIS_BASE_URL.to_string()),
        ..Config::default()
    });

    assert!(state.dis_client.is_some());
}

#[test]
fn invalid_dis_base_url_disables_client_without_failing_startup() {
    let state = AppState::new(Config {
        dis_base_url: Some("not a url".to_string()),
        ..Config::default()
    });

    assert!(state.dis_client.is_none());
}

#[test]
fn missing_bigwig_config_disables_client() {
    let state = AppState::new(Config::default());

    assert!(state.bigwig_client.is_none());
}

#[test]
fn valid_bigwig_config_creates_client() {
    let state = AppState::new(Config {
        infra_gateway_url: Some(INFRA_GATEWAY_URL.to_string()),
        infra_gateway_token: Some("test-token".to_string()),
        ..Config::default()
    });

    let client = state
        .bigwig_client
        .expect("valid Bigwig config should create a client");
    assert_eq!(client.base_host(), Some("infra-gateway-hub"));
    assert_eq!(client.timeout_ms(), 30000);
}

#[test]
fn partial_bigwig_config_disables_client_without_failing_startup() {
    for config in [
        Config {
            infra_gateway_url: Some(INFRA_GATEWAY_URL.to_string()),
            ..Config::default()
        },
        Config {
            infra_gateway_token: Some("test-token".to_string()),
            ..Config::default()
        },
    ] {
        let state = AppState::new(config);
        assert!(state.bigwig_client.is_none());
    }
}

#[test]
fn invalid_bigwig_url_disables_client_without_failing_startup() {
    let state = AppState::new(Config {
        infra_gateway_url: Some("not a url".to_string()),
        infra_gateway_token: Some("test-token".to_string()),
        ..Config::default()
    });

    assert!(state.bigwig_client.is_none());
}
