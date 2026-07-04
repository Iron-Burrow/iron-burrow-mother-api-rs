use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

use serde_json::{json, Value};

use super::*;

#[test]
fn winner_request_serializes_event_slug_only() {
    let request = PolymarketSnapshotRequest {
        event_slug: "fifa-world-cup-2026-winner".to_string(),
        country: None,
    };

    assert_eq!(
        serde_json::to_value(&request).unwrap(),
        json!({ "event_slug": "fifa-world-cup-2026-winner" })
    );
}

#[test]
fn country_request_serializes_event_slug_and_country() {
    let request = PolymarketSnapshotRequest {
        event_slug: "fifa-world-cup-2026-country-probability".to_string(),
        country: Some("mexico".to_string()),
    };

    assert_eq!(
        serde_json::to_value(&request).unwrap(),
        json!({
            "event_slug": "fifa-world-cup-2026-country-probability",
            "country": "mexico"
        })
    );
}

#[test]
fn current_winner_response_decodes_and_preserves_decimal_strings() {
    let body = serde_json::to_vec(&winner_dis_body()).unwrap();
    let response = decode_success_response(&winner_request(), StatusCode::OK, &body).unwrap();

    let PolymarketSnapshotResponse::Winner(response) = response else {
        panic!("winner request should decode the winner response variant");
    };

    assert_eq!(response.event_title, "World Cup Winner ");
    assert_eq!(response.outcomes.len(), 2);
    assert_eq!(response.outcomes[0].name, "France");
    assert_eq!(response.outcomes[0].probability, "0.1595");
    assert_eq!(response.outcomes[0].price, "0.1595");
    assert_eq!(response.outcomes[1].name, "Spain");
}

#[test]
fn current_country_response_decodes_and_preserves_decimal_strings() {
    let body = serde_json::to_vec(&country_dis_body()).unwrap();
    let response = decode_success_response(&country_request(), StatusCode::OK, &body).unwrap();

    let PolymarketSnapshotResponse::Country(response) = response else {
        panic!("country request should decode the country response variant");
    };

    assert_eq!(response.subject.slug, "mexico");
    assert_eq!(response.subject.name, "Mexico");
    assert_eq!(response.probability, "0.535");
    assert_eq!(response.price, "0.535");
    assert_eq!(response.currency, "USDC");
}

#[test]
fn wrong_shaped_success_response_maps_to_unsupported_response_schema() {
    let body = serde_json::to_vec(&json!({
        "ok": true,
        "event": "Legacy shape",
        "odds": []
    }))
    .unwrap();

    assert_eq!(
        decode_success_response(&winner_request(), StatusCode::OK, &body),
        Err(DisClientError::UnsupportedResponseSchema)
    );
}

#[test]
fn diagnostic_field_names_are_sorted_capped_and_exclude_values() {
    let mut fields = serde_json::Map::new();
    for index in (0..20).rev() {
        fields.insert(
            format!("field_{index:02}"),
            Value::String(format!("secret-provider-value-{index}")),
        );
    }
    fields.insert(
        "z".repeat(MAX_LOGGED_FIELD_NAME_CHARS + 10),
        Value::String("secret-provider-url".to_string()),
    );
    let body = serde_json::to_vec(&Value::Object(fields)).unwrap();

    let names = top_level_json_field_names(&body);

    assert_eq!(names.len(), MAX_LOGGED_TOP_LEVEL_FIELDS);
    assert!(names.windows(2).all(|pair| pair[0] <= pair[1]));
    assert!(names
        .iter()
        .all(|name| name.chars().count() <= MAX_LOGGED_FIELD_NAME_CHARS));
    let logged_names = names.join(",");
    assert!(!logged_names.contains("secret-provider-value"));
    assert!(!logged_names.contains("secret-provider-url"));
}

#[test]
fn maps_dis_error_envelopes() {
    for (status, code, expected) in [
        (
            StatusCode::BAD_REQUEST,
            "unsupported_prediction_subject",
            DisClientError::UnsupportedSubject,
        ),
        (
            StatusCode::BAD_REQUEST,
            "unsupported_country",
            DisClientError::UnsupportedSubject,
        ),
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "prediction_provider_unavailable",
            DisClientError::ProviderUnavailable,
        ),
        (
            StatusCode::GATEWAY_TIMEOUT,
            "prediction_provider_timeout",
            DisClientError::ProviderTimeout,
        ),
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "prediction_resolver_unavailable",
            DisClientError::ResolverUnavailable,
        ),
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            DisClientError::ResolverError,
        ),
    ] {
        let body = serde_json::to_vec(&json!({
            "error": {
                "code": code,
                "message": "DIS-owned message.",
                "details": { "ignored": true }
            }
        }))
        .unwrap();

        assert_eq!(map_error_response(status, &body), expected);
    }
}

#[test]
fn unknown_error_code_maps_to_unknown_resolver_error_code() {
    let body = serde_json::to_vec(&json!({
        "error": {
            "code": "future_dis_error",
            "message": "Future DIS error."
        }
    }))
    .unwrap();

    assert_eq!(
        map_error_response(StatusCode::SERVICE_UNAVAILABLE, &body),
        DisClientError::UnknownResolverErrorCode("future_dis_error".to_string())
    );
}

#[test]
fn malformed_error_body_maps_to_malformed_error_response_for_any_status() {
    for status in [
        StatusCode::INTERNAL_SERVER_ERROR,
        StatusCode::SERVICE_UNAVAILABLE,
        StatusCode::GATEWAY_TIMEOUT,
    ] {
        assert_eq!(
            map_error_response(status, b"not-json"),
            DisClientError::MalformedErrorResponse
        );
    }
}

#[tokio::test]
async fn prediction_snapshot_request_maps_transport_failure() {
    let Ok(listener) = TcpListener::bind("127.0.0.1:0") else {
        return;
    };
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let client = DisClient::new(&base_url, 2000, 1).unwrap();

    let error = client
        .get_polymarket_prediction_snapshot(winner_request())
        .await
        .expect_err("closed listener should cause transport failure");

    assert_eq!(error, DisClientError::Transport);
}

#[tokio::test]
async fn prediction_snapshot_request_maps_timeout() {
    let Ok(listener) = TcpListener::bind("127.0.0.1:0") else {
        return;
    };
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    let handle = thread::spawn(move || {
        let (_stream, _) = listener.accept().expect("test request should connect");
        thread::sleep(Duration::from_millis(1000));
    });
    let client = DisClient::new(&base_url, 100, 1).unwrap();

    let error = client
        .get_polymarket_prediction_snapshot(winner_request())
        .await
        .expect_err("held connection should time out");

    assert_eq!(error, DisClientError::Timeout);
    handle.join().expect("test listener thread should finish");
}

#[tokio::test]
async fn retry_budget_caps_retryable_failures() {
    let Ok(listener) = TcpListener::bind("127.0.0.1:0") else {
        return;
    };
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    let attempts = Arc::new(AtomicUsize::new(0));
    let server_attempts = Arc::clone(&attempts);
    let handle = thread::spawn(move || {
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().expect("test request should connect");
            server_attempts.fetch_add(1, Ordering::SeqCst);
            let mut buffer = [0; 1024];
            let _ = stream.read(&mut buffer);
            write_response(
                &mut stream,
                StatusCode::SERVICE_UNAVAILABLE,
                json!({
                    "error": {
                        "code": "prediction_provider_unavailable",
                        "message": "Provider unavailable."
                    }
                }),
            );
        }
    });
    let client = DisClient::new(&base_url, 2000, 2).unwrap();

    let error = client
        .get_polymarket_prediction_snapshot(winner_request())
        .await
        .expect_err("retryable response should exhaust attempts");

    assert_eq!(error, DisClientError::ProviderUnavailable);
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
    handle.join().expect("test listener thread should finish");
}

#[tokio::test]
async fn unsupported_subject_is_not_retried() {
    let Ok(listener) = TcpListener::bind("127.0.0.1:0") else {
        return;
    };
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    let attempts = Arc::new(AtomicUsize::new(0));
    let server_attempts = Arc::clone(&attempts);
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("test request should connect");
        server_attempts.fetch_add(1, Ordering::SeqCst);
        let mut buffer = [0; 1024];
        let _ = stream.read(&mut buffer);
        write_response(
            &mut stream,
            StatusCode::BAD_REQUEST,
            json!({
                "error": {
                    "code": "unsupported_prediction_subject",
                    "message": "Unsupported subject."
                }
            }),
        );
    });
    let client = DisClient::new(&base_url, 2000, 3).unwrap();

    let error = client
        .get_polymarket_prediction_snapshot(winner_request())
        .await
        .expect_err("unsupported subject should fail");

    assert_eq!(error, DisClientError::UnsupportedSubject);
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
    handle.join().expect("test listener thread should finish");
}

fn winner_request() -> PolymarketSnapshotRequest {
    PolymarketSnapshotRequest {
        event_slug: "fifa-world-cup-2026-winner".to_string(),
        country: None,
    }
}

fn country_request() -> PolymarketSnapshotRequest {
    PolymarketSnapshotRequest {
        event_slug: "fifa-world-cup-2026-country-probability".to_string(),
        country: Some("mexico".to_string()),
    }
}

fn winner_dis_body() -> Value {
    json!({
        "event_slug": "fifa-world-cup-2026-winner",
        "event_title": "World Cup Winner ",
        "source": "polymarket",
        "source_kind": "public_market_data_api",
        "mode": "live_passthrough",
        "deterministic": true,
        "captured_at": "2026-06-06T03:21:42.512048Z",
        "provider_market": {
            "id": "558936",
            "slug": "will-france-win-the-2026-fifa-world-cup-924",
            "condition_id": "0x9b6fef249040fd17e9c107955b37ac2c3e923509b6b0ff01cc463a331ddeb894",
            "url": "https://polymarket.com/event/will-france-win-the-2026-fifa-world-cup-924"
        },
        "warnings": [
            {
                "code": "probability_interpreted_from_price",
                "message": "Outcome probabilities are interpreted from public market prices."
            }
        ],
        "outcomes": [
            {
                "name": "France",
                "probability": "0.1595",
                "price": "0.1595",
                "currency": "USDC"
            },
            {
                "name": "Spain",
                "probability": "0.1595",
                "price": "0.1595",
                "currency": "USDC"
            }
        ]
    })
}

fn country_dis_body() -> Value {
    json!({
        "event_slug": "fifa-world-cup-2026-country-probability",
        "event_title": "FIFA World Cup 2026 Country Probability",
        "source": "polymarket",
        "source_kind": "public_market_data_api",
        "mode": "live_passthrough",
        "deterministic": true,
        "captured_at": "2026-06-06T03:22:11.593940Z",
        "provider_market": {
            "id": "2415420",
            "slug": "will-mexico-reach-the-round-of-16-at-the-2026-fifa-world-cup-20260602025120735",
            "condition_id": "0x2b3237da39d6c7b1f7adef29c5f675e4214cec25f585ca151c7b8cc9271871e1",
            "url": "https://polymarket.com/event/will-mexico-reach-the-round-of-16-at-the-2026-fifa-world-cup-20260602025120735"
        },
        "warnings": [
            {
                "code": "probability_interpreted_from_price",
                "message": "Outcome probability is interpreted from public market price."
            }
        ],
        "subject": {
            "kind": "country",
            "slug": "mexico",
            "name": "Mexico"
        },
        "market": "Will Mexico reach the Round of 16 at the 2026 FIFA World Cup?",
        "probability": "0.535",
        "price": "0.535",
        "currency": "USDC"
    })
}

fn write_response(stream: &mut std::net::TcpStream, status: StatusCode, body: Value) {
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
