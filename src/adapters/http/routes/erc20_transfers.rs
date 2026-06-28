use axum::{body::Bytes, extract::State, http::HeaderMap};

use crate::adapters::http::dto::erc20_transfers::Erc20TransferSearchRequest;
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
    let request = Erc20TransferSearchRequest::try_from(&request)?;
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
    use serde_json::{json, Value};
    use tower::ServiceExt;

    use super::*;
    use crate::{
        adapters::http::{
            dto::filters::{
                onchain_window::{BlockWindowDTO, OnchainWindowDTO},
                token_filters::TokenFilterDTO,
                transfer_direction::TransferDirectionDTO,
            },
            router::build_router,
            types::JsonObject,
        },
        application::{
            erc20_transfers::service::{
                Erc20TransferCommandTokenFilters, Erc20TransferSearchCommand,
            },
            filters::{
                onchain_window::{BlockWindow, OnchainWindow},
                transfer_direction::TransferDirection,
            },
        },
        common::rfc3339::parse_rfc3339,
        test_utils::{
            errors::assert_public_error,
            fixtures::{
                erc20_transfers::{
                    erc20_transfers_command_from_body, erc20_transfers_request_with_tokens_body,
                    erc20_transfers_without_tokens_body, valid_erc20_transfers_request_body,
                },
                global_assets::{global_assets_repository, sample_assets},
            },
            json::json_object,
        },
    };
    use crate::{adapters::postgres::global_assets::GlobalAssetRepository, config::Config};

    const TEST_MAX_TOKEN_FILTERS: u64 = 20;

    #[tokio::test]
    async fn route_is_absent_when_disabled() {
        let response = transfers_app(Config::default())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/erc20-transfers/search")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&valid_erc20_transfers_request_body()).unwrap(),
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
            valid_erc20_transfers_request_body(),
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
            erc20_transfers_request_with_tokens_body(json!({
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
            erc20_transfers_request_with_tokens_body(json!({"asset_slugs": ["usdc"]})),
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
            erc20_transfers_request_with_tokens_body(json!({"asset_slugs": ["ethereum"]})),
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
            erc20_transfers_request_with_tokens_body(
                json!({"asset_slugs": ["missing-but-syntactically-valid"]}),
            ),
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
            erc20_transfers_request_with_tokens_body(json!({"asset_slugs": ["mantle"]})),
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
            erc20_transfers_request_with_tokens_body(json!({
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
            erc20_transfers_request_with_tokens_body(json!({
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
            erc20_transfers_request_with_tokens_body(json!({
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
        let mut body = valid_erc20_transfers_request_body();
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
                serde_json::to_vec(&valid_erc20_transfers_request_body()).unwrap(),
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
                    let mut body = valid_erc20_transfers_request_body();
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
                    let mut body = valid_erc20_transfers_request_body();
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
                    let mut body = valid_erc20_transfers_request_body();
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
                    let mut body = valid_erc20_transfers_request_body();
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
                    let mut body = valid_erc20_transfers_request_body();
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
                    let mut body = valid_erc20_transfers_request_body();
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
                    let mut body = valid_erc20_transfers_request_body();
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
                    let mut body = valid_erc20_transfers_request_body();
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
                    let mut body = valid_erc20_transfers_request_body();
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
                    let mut body = valid_erc20_transfers_request_body();
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
                    let mut body = valid_erc20_transfers_request_body();
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
                    let mut body = valid_erc20_transfers_request_body();
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
                    let mut body = valid_erc20_transfers_request_body();
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
                    let mut body = valid_erc20_transfers_request_body();
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
                    let mut body = valid_erc20_transfers_request_body();
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
                    let mut body = valid_erc20_transfers_request_body();
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
                    let mut body = valid_erc20_transfers_request_body();
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
                    let mut body = valid_erc20_transfers_request_body();
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
                    let mut body = valid_erc20_transfers_request_body();
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
                    let mut body = valid_erc20_transfers_request_body();
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
                    let mut body = valid_erc20_transfers_request_body();
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
                    let mut body = valid_erc20_transfers_request_body();
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
                    let mut body = valid_erc20_transfers_request_body();
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
                    let mut body = valid_erc20_transfers_request_body();
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
                    let mut body = valid_erc20_transfers_request_body();
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
            erc20_transfers_request_with_tokens_body(json!({
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
        let command = erc20_transfers_command_from_body(
            valid_erc20_transfers_request_body(),
            TEST_MAX_TOKEN_FILTERS,
        )
        .await;
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
        build_router(AppState::with_asset_repository(
            config,
            global_assets_repository(),
        ))
    }

    fn transfers_app_without_repository(config: Config) -> Router {
        build_router(AppState::new(config))
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

    #[tokio::test]
    async fn transfer_unsupported_network_uses_not_found_status() {
        let response = ApiError::transfer_unsupported_network().into_response();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
