use axum::{body::Bytes, extract::State, http::HeaderMap};

use crate::adapters::http::dto::erc20_transfers::validate_request;
use crate::adapters::http::json_body::parse_json_object_body;
use crate::adapters::http::validation::ensure_json_content_type;
use crate::application::erc20_transfers::service::{
    build_command, extraction_unavailable_placeholder,
};
use crate::{adapters::http::error::ApiError, state::AppState};

pub async fn search_erc20_transfers(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<(), ApiError> {
    ensure_json_content_type(&headers)?;
    let request = parse_json_object_body(&body)?;
    let request = validate_request(&request)?;
    let command = build_command(
        request,
        state.asset_repository.clone(),
        state.config.erc20_transfers_max_token_filters,
    )
    .await?;

    extraction_unavailable_placeholder(command).await
}

#[cfg(test)]
mod tests {
    use axum::{
        body::Body,
        http::{Request, StatusCode},
        response::IntoResponse,
        Router,
    };
    use serde_json::json;
    use serde_json::{Map, Value};
    use tower::ServiceExt;

    use super::*;
    use crate::{
        adapters::http::dto::{
            erc20_transfers::Erc20TransferTokenFilters,
            onchain_window::{BlockWindowDTO, OnchainWindowDTO},
        },
        application::erc20_transfers::service::{
            Erc20TransferCommandDirection, Erc20TransferCommandTokenFilters,
            Erc20TransferSearchCommand,
        },
        test_utils::global_assets::asset_fixtures,
    };
    use crate::{
        adapters::postgres::global_assets::GlobalAssetRepository, app::create_app, config::Config,
    };

    const TEST_MAX_TOKEN_FILTERS: u64 = 20;

    #[test]
    fn validation_accepts_supported_window_shapes() {
        let block = validate_request(&json_object(valid_request_body())).unwrap();
        assert!(matches!(
            block.window,
            OnchainWindowDTO::Block(BlockWindowDTO {
                from_block: 18_600_000,
                to_block: 18_600_500,
            })
        ));

        let mut timestamp_body = valid_request_body();
        timestamp_body["window"] = json!({
            "from_timestamp": "2026-06-25T00:00:00Z",
            "to_timestamp": "2026-06-25T01:00:00Z"
        });
        let timestamp = validate_request(&json_object(timestamp_body)).unwrap();
        assert!(matches!(timestamp.window, OnchainWindowDTO::Timestamp(_)));

        let mut lookback_body = valid_request_body();
        lookback_body["window"] = json!({
            "lookback_seconds": 600,
            "to": "latest"
        });
        let lookback = validate_request(&json_object(lookback_body)).unwrap();
        assert!(matches!(lookback.window, OnchainWindowDTO::Lookback(_)));
    }

    #[test]
    fn validation_accepts_omitted_null_and_empty_tokens() {
        let omitted_tokens = validate_request(&json_object(body_without_tokens())).unwrap();
        assert_eq!(
            omitted_tokens.tokens.unwrap_or_default(),
            Erc20TransferTokenFilters::default()
        );

        let mut null_tokens_body = valid_request_body();
        null_tokens_body["tokens"] = Value::Null;
        let null_tokens = validate_request(&json_object(null_tokens_body)).unwrap();
        assert_eq!(
            null_tokens.tokens.unwrap_or_default(),
            Erc20TransferTokenFilters::default()
        );

        let mut empty_tokens_body = valid_request_body();
        empty_tokens_body["tokens"] = json!({});
        let empty_tokens = validate_request(&json_object(empty_tokens_body)).unwrap();
        assert_eq!(
            empty_tokens.tokens.unwrap_or_default(),
            Erc20TransferTokenFilters::default()
        );
    }

    #[test]
    fn validation_normalizes_explicit_contract_addresses_to_lowercase() {
        let mut body = valid_request_body();
        body["tokens"]["contract_addresses"] =
            json!(["0xA0B86991C6218B36C1D19D4A2E9EB0CE3606EB48"]);

        let request = validate_request(&json_object(body)).unwrap();

        assert_eq!(
            request.tokens.unwrap().contract_addresses,
            ["0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"]
        );
    }

    #[tokio::test]
    async fn command_resolves_usdc_and_dedupes_duplicate_explicit_address() {
        let mut body = valid_request_body();
        body["address"] = json!("0xABC0000000000000000000000000000000000000");
        body["tokens"]["contract_addresses"] =
            json!(["0xA0B86991C6218B36C1D19D4A2E9EB0CE3606EB48"]);

        let command = command_from_body(body, TEST_MAX_TOKEN_FILTERS).await;

        assert_eq!(
            command,
            Erc20TransferSearchCommand {
                network_slug: "eth-mainnet".to_string(),
                address: "0xabc0000000000000000000000000000000000000".to_string(),
                direction: Erc20TransferCommandDirection::Any,
                tokens: Erc20TransferCommandTokenFilters {
                    contract_addresses: vec![
                        "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48".to_string()
                    ],
                },
                window: OnchainWindowDTO::Block {
                    from_block: 18_600_000,
                    to_block: 18_600_500,
                },
            }
        );
    }

    #[test]
    fn validation_accepts_minimal_asset_contract_and_mixed_token_filter_shapes() {
        let cases = [
            (body_without_tokens(), Erc20TransferTokenFilters::default()),
            (
                request_with_tokens(json!({
                    "asset_slugs": ["usdc", "wrapped-ether"]
                })),
                Erc20TransferTokenFilters {
                    asset_slugs: vec!["usdc".to_string(), "wrapped-ether".to_string()],
                    contract_addresses: Vec::new(),
                },
            ),
            (
                request_with_tokens(json!({
                    "contract_addresses": [
                        "0x1111111111111111111111111111111111111111",
                        "0xA0B86991C6218B36C1D19D4A2E9EB0CE3606EB48"
                    ]
                })),
                Erc20TransferTokenFilters {
                    asset_slugs: Vec::new(),
                    contract_addresses: vec![
                        "0x1111111111111111111111111111111111111111".to_string(),
                        "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48".to_string(),
                    ],
                },
            ),
            (
                valid_request_body(),
                Erc20TransferTokenFilters {
                    asset_slugs: vec!["usdc".to_string()],
                    contract_addresses: vec![
                        "0x1111111111111111111111111111111111111111".to_string()
                    ],
                },
            ),
        ];

        for (body, expected_tokens) in cases {
            let request = validate_request(&json_object(body)).unwrap();

            assert_eq!(request.network_slug, "eth-mainnet");
            assert_eq!(
                request.address,
                "0xabc0000000000000000000000000000000000000"
            );
            assert_eq!(request.direction, Erc20TransferDirection::Any);
            assert_eq!(request.tokens.unwrap_or_default(), expected_tokens);
            assert!(matches!(
                request.window,
                Erc20TransferSearchWindow::Block(Erc20TransferBlockWindow {
                    from_block: 18_600_000,
                    to_block: 18_600_500,
                })
            ));
        }
    }

    #[tokio::test]
    async fn command_enforces_final_token_filter_limit_after_dedupe() {
        let body = request_with_tokens(json!({
            "asset_slugs": ["usdc"],
            "contract_addresses": ["0xA0B86991C6218B36C1D19D4A2E9EB0CE3606EB48"]
        }));
        let command = command_from_body(body, 1).await;
        assert_eq!(
            command.tokens.contract_addresses,
            ["0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"]
        );

        let body = request_with_tokens(json!({
            "asset_slugs": ["usdc"],
            "contract_addresses": ["0x1111111111111111111111111111111111111111"]
        }));
        let request = validate_request(&json_object(body)).unwrap();
        let error = build_command(request, Some(repository()), 1)
            .await
            .unwrap_err();

        assert_api_error(
            error,
            StatusCode::UNPROCESSABLE_ENTITY,
            "too_many_token_filters",
        )
        .await;
    }

    #[tokio::test]
    async fn route_is_absent_when_disabled() {
        let response = transfers_app(Config::default())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/erc20-transfers/search")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&valid_request_body()).unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn route_is_present_when_enabled() {
        let response = transfers_app(enabled_config())
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/erc20-transfers/search")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    }

    #[tokio::test]
    async fn valid_request_returns_extraction_unavailable_placeholder() {
        let (status, response) = post_json(
            transfers_app(enabled_config()),
            "/v1/erc20-transfers/search",
            valid_request_body(),
        )
        .await;

        assert_public_error(
            status,
            &response,
            StatusCode::SERVICE_UNAVAILABLE,
            "extraction_unavailable",
        );
    }

    #[tokio::test]
    async fn request_without_asset_slugs_does_not_require_catalog_or_bigwig_to_exist() {
        let (status, response) = post_json(
            transfers_app_without_repository(enabled_config()),
            "/v1/erc20-transfers/search",
            request_with_tokens(json!({
                "contract_addresses": ["0x1111111111111111111111111111111111111111"]
            })),
        )
        .await;

        assert_public_error(
            status,
            &response,
            StatusCode::SERVICE_UNAVAILABLE,
            "extraction_unavailable",
        );
    }

    #[tokio::test]
    async fn request_with_asset_slugs_requires_catalog() {
        let (status, response) = post_json(
            transfers_app_without_repository(enabled_config()),
            "/v1/erc20-transfers/search",
            request_with_tokens(json!({"asset_slugs": ["usdc"]})),
        )
        .await;

        assert_public_error(
            status,
            &response,
            StatusCode::SERVICE_UNAVAILABLE,
            "asset_contract_mapping_unavailable",
        );
        assert_ne!(response["error"]["code"], "extraction_unavailable");
    }

    #[tokio::test]
    async fn native_asset_slug_rejects_whole_request_before_extraction() {
        let (status, response) = post_json(
            transfers_app(enabled_config()),
            "/v1/erc20-transfers/search",
            request_with_tokens(json!({"asset_slugs": ["ethereum"]})),
        )
        .await;

        assert_public_error(
            status,
            &response,
            StatusCode::UNPROCESSABLE_ENTITY,
            "asset_not_erc20_on_network",
        );
        assert_ne!(response["error"]["code"], "extraction_unavailable");
    }

    #[tokio::test]
    async fn unknown_asset_slug_rejects_whole_request_before_extraction() {
        let (status, response) = post_json(
            transfers_app(enabled_config()),
            "/v1/erc20-transfers/search",
            request_with_tokens(json!({"asset_slugs": ["missing-but-syntactically-valid"]})),
        )
        .await;

        assert_public_error(status, &response, StatusCode::NOT_FOUND, "asset_not_found");
        assert_ne!(response["error"]["code"], "extraction_unavailable");
    }

    #[tokio::test]
    async fn globally_known_asset_unavailable_on_network_rejects_whole_request() {
        let (status, response) = post_json(
            transfers_app(enabled_config()),
            "/v1/erc20-transfers/search",
            request_with_tokens(json!({"asset_slugs": ["mantle"]})),
        )
        .await;

        assert_public_error(
            status,
            &response,
            StatusCode::UNPROCESSABLE_ENTITY,
            "asset_not_available_on_network",
        );
        assert_ne!(response["error"]["code"], "extraction_unavailable");
    }

    #[tokio::test]
    async fn mixed_valid_and_invalid_asset_slug_rejects_whole_request() {
        let (status, response) = post_json(
            transfers_app(enabled_config()),
            "/v1/erc20-transfers/search",
            request_with_tokens(json!({
                "asset_slugs": ["usdc", "ethereum"],
                "contract_addresses": ["0x1111111111111111111111111111111111111111"]
            })),
        )
        .await;

        assert_public_error(
            status,
            &response,
            StatusCode::UNPROCESSABLE_ENTITY,
            "asset_not_erc20_on_network",
        );
        assert_ne!(response["error"]["code"], "extraction_unavailable");
    }

    #[tokio::test]
    async fn duplicate_explicit_and_resolved_address_dedupes_before_limit() {
        let (status, response) = post_json(
            transfers_app(Config {
                erc20_transfers_enabled: true,
                erc20_transfers_max_token_filters: 1,
                bigwig_max_contract_addresses: 20,
                ..Config::default()
            }),
            "/v1/erc20-transfers/search",
            request_with_tokens(json!({
                "asset_slugs": ["usdc"],
                "contract_addresses": ["0xA0B86991C6218B36C1D19D4A2E9EB0CE3606EB48"]
            })),
        )
        .await;

        assert_public_error(
            status,
            &response,
            StatusCode::SERVICE_UNAVAILABLE,
            "extraction_unavailable",
        );
    }

    #[tokio::test]
    async fn duplicate_explicit_contract_addresses_dedupe_before_limit() {
        let (status, response) = post_json(
            transfers_app_without_repository(Config {
                erc20_transfers_enabled: true,
                erc20_transfers_max_token_filters: 1,
                bigwig_max_contract_addresses: 20,
                ..Config::default()
            }),
            "/v1/erc20-transfers/search",
            request_with_tokens(json!({
                "contract_addresses": [
                    "0xA0B86991C6218B36C1D19D4A2E9EB0CE3606EB48",
                    "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
                ]
            })),
        )
        .await;

        assert_public_error(
            status,
            &response,
            StatusCode::SERVICE_UNAVAILABLE,
            "extraction_unavailable",
        );
    }

    #[tokio::test]
    async fn validation_failures_do_not_require_catalog_or_bigwig_to_exist() {
        let mut body = valid_request_body();
        body["tokens"]["asset_slugs"] = json!(["USDC"]);

        let (status, response) = post_json(
            transfers_app_without_repository(enabled_config()),
            "/v1/erc20-transfers/search",
            body,
        )
        .await;

        assert_public_error(
            status,
            &response,
            StatusCode::BAD_REQUEST,
            "invalid_asset_slug",
        );
    }

    #[tokio::test]
    async fn malformed_json_raw_body_returns_invalid_json() {
        let (status, response) = post_raw(
            transfers_app(enabled_config()),
            "/v1/erc20-transfers/search",
            Some("application/json"),
            br#"{"network_slug":"eth-mainnet""#.to_vec(),
        )
        .await;

        assert_public_error(status, &response, StatusCode::BAD_REQUEST, "invalid_json");
    }

    #[tokio::test]
    async fn missing_or_non_json_content_type_returns_invalid_json() {
        for content_type in [None, Some("text/plain")] {
            let (status, response) = post_raw(
                transfers_app(enabled_config()),
                "/v1/erc20-transfers/search",
                content_type,
                serde_json::to_vec(&valid_request_body()).unwrap(),
            )
            .await;

            assert_public_error(status, &response, StatusCode::BAD_REQUEST, "invalid_json");
        }
    }

    #[tokio::test]
    async fn invalid_requests_return_stable_public_codes() {
        let app = transfers_app(enabled_config());
        let cases = [
            (
                Some("application/json"),
                serde_json::to_vec(&json!([])).unwrap(),
                StatusCode::BAD_REQUEST,
                "invalid_json",
            ),
            (
                Some("application/json"),
                serde_json::to_vec(&{
                    let mut body = valid_request_body();
                    body["future"] = json!(true);
                    body
                })
                .unwrap(),
                StatusCode::BAD_REQUEST,
                "unknown_field",
            ),
            (
                Some("application/json"),
                serde_json::to_vec(&{
                    let mut body = valid_request_body();
                    body["tokens"]["symbol"] = json!("USDC");
                    body
                })
                .unwrap(),
                StatusCode::BAD_REQUEST,
                "unknown_field",
            ),
            (
                Some("application/json"),
                serde_json::to_vec(&{
                    let mut body = valid_request_body();
                    body["window"]["cursor"] = json!("next");
                    body
                })
                .unwrap(),
                StatusCode::BAD_REQUEST,
                "unknown_field",
            ),
            (
                Some("application/json"),
                serde_json::to_vec(&{
                    let mut body = valid_request_body();
                    body.as_object_mut().unwrap().remove("network_slug");
                    body
                })
                .unwrap(),
                StatusCode::BAD_REQUEST,
                "missing_network_slug",
            ),
            (
                Some("application/json"),
                serde_json::to_vec(&{
                    let mut body = valid_request_body();
                    body["network_slug"] = json!("");
                    body
                })
                .unwrap(),
                StatusCode::BAD_REQUEST,
                "missing_network_slug",
            ),
            (
                Some("application/json"),
                serde_json::to_vec(&{
                    let mut body = valid_request_body();
                    body["network_slug"] = json!(null);
                    body
                })
                .unwrap(),
                StatusCode::BAD_REQUEST,
                "missing_network_slug",
            ),
            (
                Some("application/json"),
                serde_json::to_vec(&{
                    let mut body = valid_request_body();
                    body["network_slug"] = json!("ETH-MAINNET");
                    body
                })
                .unwrap(),
                StatusCode::NOT_FOUND,
                "unsupported_network",
            ),
            (
                Some("application/json"),
                serde_json::to_vec(&{
                    let mut body = valid_request_body();
                    body["network_slug"] = json!("base-mainnet");
                    body
                })
                .unwrap(),
                StatusCode::NOT_FOUND,
                "unsupported_network",
            ),
            (
                Some("application/json"),
                serde_json::to_vec(&{
                    let mut body = valid_request_body();
                    body["address"] = json!("0x1234");
                    body
                })
                .unwrap(),
                StatusCode::BAD_REQUEST,
                "invalid_address",
            ),
            (
                Some("application/json"),
                serde_json::to_vec(&{
                    let mut body = valid_request_body();
                    body["direction"] = json!("ANY");
                    body
                })
                .unwrap(),
                StatusCode::BAD_REQUEST,
                "invalid_direction",
            ),
            (
                Some("application/json"),
                serde_json::to_vec(&{
                    let mut body = valid_request_body();
                    body["direction"] = json!("sideways");
                    body
                })
                .unwrap(),
                StatusCode::BAD_REQUEST,
                "invalid_direction",
            ),
            (
                Some("application/json"),
                serde_json::to_vec(&{
                    let mut body = valid_request_body();
                    body.as_object_mut().unwrap().remove("window");
                    body
                })
                .unwrap(),
                StatusCode::BAD_REQUEST,
                "invalid_window",
            ),
            (
                Some("application/json"),
                serde_json::to_vec(&{
                    let mut body = valid_request_body();
                    body["window"] = json!({});
                    body
                })
                .unwrap(),
                StatusCode::BAD_REQUEST,
                "invalid_window",
            ),
            (
                Some("application/json"),
                serde_json::to_vec(&{
                    let mut body = valid_request_body();
                    body["window"] = json!({
                        "from_block": "18600000",
                        "to_block": 18_600_500
                    });
                    body
                })
                .unwrap(),
                StatusCode::BAD_REQUEST,
                "invalid_window",
            ),
            (
                Some("application/json"),
                serde_json::to_vec(&{
                    let mut body = valid_request_body();
                    body["window"] = json!({
                        "from_block": 18_600_500,
                        "to_block": 18_600_000
                    });
                    body
                })
                .unwrap(),
                StatusCode::BAD_REQUEST,
                "invalid_window",
            ),
            (
                Some("application/json"),
                serde_json::to_vec(&{
                    let mut body = valid_request_body();
                    body["window"] = json!({
                        "from_timestamp": "not-a-timestamp",
                        "to_timestamp": "2026-06-25T01:00:00Z"
                    });
                    body
                })
                .unwrap(),
                StatusCode::BAD_REQUEST,
                "invalid_window",
            ),
            (
                Some("application/json"),
                serde_json::to_vec(&{
                    let mut body = valid_request_body();
                    body["window"] = json!({
                        "from_timestamp": "2026-06-25T02:00:00Z",
                        "to_timestamp": "2026-06-25T01:00:00Z"
                    });
                    body
                })
                .unwrap(),
                StatusCode::BAD_REQUEST,
                "invalid_window",
            ),
            (
                Some("application/json"),
                serde_json::to_vec(&{
                    let mut body = valid_request_body();
                    body["window"] = json!({
                        "from_block": 18_600_000,
                        "to_block": 18_600_500,
                        "from_timestamp": "2026-06-25T00:00:00Z",
                        "to_timestamp": "2026-06-25T01:00:00Z"
                    });
                    body
                })
                .unwrap(),
                StatusCode::BAD_REQUEST,
                "invalid_window",
            ),
            (
                Some("application/json"),
                serde_json::to_vec(&{
                    let mut body = valid_request_body();
                    body["window"] = json!({
                        "lookback_seconds": 0,
                        "to": "latest"
                    });
                    body
                })
                .unwrap(),
                StatusCode::BAD_REQUEST,
                "invalid_window",
            ),
            (
                Some("application/json"),
                serde_json::to_vec(&{
                    let mut body = valid_request_body();
                    body["tokens"]["asset_slugs"] = json!(["USDC"]);
                    body
                })
                .unwrap(),
                StatusCode::BAD_REQUEST,
                "invalid_asset_slug",
            ),
            (
                Some("application/json"),
                serde_json::to_vec(&{
                    let mut body = valid_request_body();
                    body["tokens"]["asset_slugs"] = json!([""]);
                    body
                })
                .unwrap(),
                StatusCode::BAD_REQUEST,
                "invalid_asset_slug",
            ),
            (
                Some("application/json"),
                serde_json::to_vec(&{
                    let mut body = valid_request_body();
                    body["tokens"]["asset_slugs"] = json!(null);
                    body
                })
                .unwrap(),
                StatusCode::BAD_REQUEST,
                "invalid_asset_slug",
            ),
            (
                Some("application/json"),
                serde_json::to_vec(&{
                    let mut body = valid_request_body();
                    body["tokens"]["contract_addresses"] = json!(["0x1234"]);
                    body
                })
                .unwrap(),
                StatusCode::BAD_REQUEST,
                "invalid_contract_address",
            ),
            (
                Some("application/json"),
                serde_json::to_vec(&{
                    let mut body = valid_request_body();
                    body["tokens"]["contract_addresses"] = json!([""]);
                    body
                })
                .unwrap(),
                StatusCode::BAD_REQUEST,
                "invalid_contract_address",
            ),
            (
                Some("application/json"),
                serde_json::to_vec(&{
                    let mut body = valid_request_body();
                    body["tokens"]["contract_addresses"] = json!(null);
                    body
                })
                .unwrap(),
                StatusCode::BAD_REQUEST,
                "invalid_contract_address",
            ),
        ];

        for (content_type, body, expected_status, expected_code) in cases {
            let (status, response) = post_raw(
                app.clone(),
                "/v1/erc20-transfers/search",
                content_type,
                body,
            )
            .await;

            assert_public_error(status, &response, expected_status, expected_code);
            assert_ne!(response["error"]["code"], "extraction_unavailable");
        }
    }

    #[tokio::test]
    async fn too_many_token_filters_uses_configured_public_limit() {
        let (status, response) = post_json(
            transfers_app(Config {
                erc20_transfers_enabled: true,
                erc20_transfers_max_token_filters: 1,
                bigwig_max_contract_addresses: 20,
                ..Config::default()
            }),
            "/v1/erc20-transfers/search",
            request_with_tokens(json!({
                "asset_slugs": ["usdc"],
                "contract_addresses": ["0x1111111111111111111111111111111111111111"]
            })),
        )
        .await;

        assert_public_error(
            status,
            &response,
            StatusCode::UNPROCESSABLE_ENTITY,
            "too_many_token_filters",
        );
    }

    #[tokio::test]
    async fn command_token_filters_have_no_asset_slug_field() {
        let command = command_from_body(valid_request_body(), TEST_MAX_TOKEN_FILTERS).await;
        let debug = format!("{:?}", command.tokens);

        assert!(!debug.contains("asset_slugs"));
        assert_eq!(
            command.tokens.contract_addresses,
            [
                "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
                "0x1111111111111111111111111111111111111111",
            ]
        );
    }

    #[test]
    fn rfc3339_parser_accepts_offsets_and_rejects_invalid_values() {
        assert_eq!(
            parse_rfc3339("2026-06-25T00:00:00Z"),
            parse_rfc3339("2026-06-24T19:00:00-05:00")
        );
        assert!(parse_rfc3339("2026-02-29T00:00:00Z").is_none());
        assert!(parse_rfc3339("2026-06-25 00:00:00Z").is_none());
    }

    fn enabled_config() -> Config {
        Config {
            erc20_transfers_enabled: true,
            ..Config::default()
        }
    }

    fn transfers_app(config: Config) -> Router {
        create_app(AppState::with_asset_repository(config, repository()))
    }

    fn transfers_app_without_repository(config: Config) -> Router {
        create_app(AppState::new(config))
    }

    fn valid_request_body() -> Value {
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

    fn body_without_tokens() -> Value {
        let mut body = valid_request_body();
        body.as_object_mut().unwrap().remove("tokens");
        body
    }

    fn request_with_tokens(tokens: Value) -> Value {
        let mut body = body_without_tokens();
        body["tokens"] = tokens;
        body
    }

    fn repository() -> GlobalAssetRepository {
        GlobalAssetRepository::in_memory(asset_fixtures())
    }

    async fn command_from_body(body: Value, max_token_filters: u64) -> Erc20TransferSearchCommand {
        let request = validate_request(&json_object(body)).unwrap();
        build_command(request, Some(repository()), max_token_filters)
            .await
            .unwrap()
    }

    fn json_object(value: Value) -> JsonObject {
        match value {
            Value::Object(object) => object,
            other => panic!("expected JSON object, got {other:?}"),
        }
    }

    async fn post_json(app: Router, uri: &str, body: Value) -> (StatusCode, Value) {
        post_raw(
            app,
            uri,
            Some("application/json"),
            serde_json::to_vec(&body).unwrap(),
        )
        .await
    }

    async fn post_raw(
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

    fn assert_public_error(
        status: StatusCode,
        response: &Value,
        expected_status: StatusCode,
        expected_code: &str,
    ) {
        assert_eq!(status, expected_status);
        assert_eq!(response["ok"], false);
        assert_eq!(response["error"]["code"], expected_code);
        assert!(response["error"]["message"]
            .as_str()
            .is_some_and(|message| !message.is_empty()));
    }

    async fn assert_api_error(error: ApiError, expected_status: StatusCode, expected_code: &str) {
        let response = error.into_response();
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();

        assert_public_error(status, &json, expected_status, expected_code);
    }

    #[tokio::test]
    async fn transfer_unsupported_network_uses_not_found_status() {
        let response = ApiError::transfer_unsupported_network().into_response();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
