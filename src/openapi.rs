use serde_json::Value;
use utoipa::{
    openapi::{example::ExampleBuilder, path::Operation, Content, RefOr},
    OpenApi,
};

use crate::adapters::http::dto::erc20_transfers::{
    examples as erc20_transfer_examples, Erc20TransferAmount, Erc20TransferRow,
    Erc20TransferSearchLimits, Erc20TransferSearchRequest, Erc20TransferSearchResponse,
    Erc20TransferToken,
};
use crate::adapters::http::dto::filters::onchain_window::{
    BlockWindowDTO, LookbackTargetDTO, LookbackWindowDTO, OnchainWindowDTO, TimestampWindowDTO,
};
use crate::adapters::http::dto::filters::token_filters::{
    ResolvedTokenFilterDTO, TokenFilterDTO, TokenFilterResolutionDTO, TokenFilterSourceDTO,
};
use crate::adapters::http::dto::filters::transfer_direction::TransferDirectionDTO;
use crate::adapters::http::error::{ErrorBody, ErrorResponse};
use crate::config::Config;

pub(crate) fn document(config: &Config) -> utoipa::openapi::OpenApi {
    let mut document = if config.erc20_transfers_enabled {
        Erc20TransfersApiDoc::openapi()
    } else {
        BaseApiDoc::openapi()
    };

    if config.erc20_transfers_enabled {
        add_erc20_transfer_examples(&mut document);
    }

    document
}

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Iron Burrow Mother API",
        version = env!("CARGO_PKG_VERSION")
    ),
    components(schemas(
        Erc20TransferAmount,
        BlockWindowDTO,
        TransferDirectionDTO,
        LookbackTargetDTO,
        LookbackWindowDTO,
        Erc20TransferRow,
        Erc20TransferSearchLimits,
        Erc20TransferSearchRequest,
        Erc20TransferSearchResponse,
        OnchainWindowDTO,
        TimestampWindowDTO,
        Erc20TransferToken,
        TokenFilterResolutionDTO,
        TokenFilterSourceDTO,
        TokenFilterDTO,
        ErrorBody,
        ErrorResponse,
        ResolvedTokenFilterDTO
    ))
)]
struct BaseApiDoc;

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Iron Burrow Mother API",
        version = env!("CARGO_PKG_VERSION")
    ),
    paths(erc20_transfer_search_operation),
    components(schemas(
        Erc20TransferAmount,
        BlockWindowDTO,
        TransferDirectionDTO,
        LookbackTargetDTO,
        LookbackWindowDTO,
        Erc20TransferRow,
        Erc20TransferSearchLimits,
        Erc20TransferSearchRequest,
        Erc20TransferSearchResponse,
        OnchainWindowDTO,
        TimestampWindowDTO,
        Erc20TransferToken,
        TokenFilterResolutionDTO,
        TokenFilterSourceDTO,
        TokenFilterDTO,
        ErrorBody,
        ErrorResponse,
        ResolvedTokenFilterDTO
    ))
)]
struct Erc20TransfersApiDoc;

#[utoipa::path(
    post,
    path = "/v1/erc20-transfers/search",
    tag = "erc20-transfers",
    request_body(
        content = Erc20TransferSearchRequest,
        content_type = "application/json"
    ),
    responses(
        (
            status = 200,
            description = "ERC-20 transfer search response",
            body = Erc20TransferSearchResponse
        ),
        (
            status = 400,
            description = "Malformed or semantically invalid transfer search request",
            body = ErrorResponse
        ),
        (
            status = 404,
            description = "Requested network is unsupported for transfer search",
            body = ErrorResponse
        ),
        (
            status = 422,
            description = "Transfer search request exceeds public validation limits",
            body = ErrorResponse
        ),
        (
            status = 500,
            description = "Mother API detected an internally inconsistent transfer search state",
            body = ErrorResponse
        ),
        (
            status = 502,
            description = "Upstream transfer provider failed",
            body = ErrorResponse
        ),
        (
            status = 503,
            description = "Transfer extraction or asset-contract mapping is temporarily unavailable",
            body = ErrorResponse
        ),
        (
            status = 504,
            description = "Upstream transfer provider timed out",
            body = ErrorResponse
        )
    )
)]
#[allow(dead_code)]
async fn erc20_transfer_search_operation() {}

fn add_erc20_transfer_examples(document: &mut utoipa::openapi::OpenApi) {
    let Some(path_item) = document.paths.paths.get_mut("/v1/erc20-transfers/search") else {
        return;
    };
    let Some(operation) = path_item.post.as_mut() else {
        return;
    };

    if let Some(request_body) = operation.request_body.as_mut() {
        if let Some(content) = request_body.content.get_mut("application/json") {
            add_content_examples(
                content,
                [
                    (
                        "unfiltered_search",
                        "Unfiltered ERC-20 transfer search",
                        erc20_transfer_examples::unfiltered_request(),
                    ),
                    (
                        "asset_slug_filters",
                        "Search using catalog asset slugs",
                        erc20_transfer_examples::asset_slug_request(),
                    ),
                    (
                        "contract_address_filters",
                        "Search using explicit contract addresses",
                        erc20_transfer_examples::contract_address_request(),
                    ),
                    (
                        "mixed_filters",
                        "Search using asset slugs and explicit contracts",
                        erc20_transfer_examples::mixed_filter_request(),
                    ),
                    (
                        "native_asset_rejection",
                        "Native asset slug rejection",
                        erc20_transfer_examples::native_asset_rejection_request(),
                    ),
                    (
                        "unknown_slug_rejection",
                        "Unknown asset slug rejection",
                        erc20_transfer_examples::unknown_slug_rejection_request(),
                    ),
                    (
                        "too_many_filters",
                        "Too many token filters",
                        erc20_transfer_examples::too_many_filters_request(),
                    ),
                ],
            );
        }
    }

    add_response_examples(
        operation,
        "200",
        [
            (
                "mixed_filters",
                "Successful mixed filter response",
                erc20_transfer_examples::mixed_success_response(),
            ),
            (
                "truncated_response",
                "Successful response capped by upstream row limit",
                erc20_transfer_examples::truncated_success_response(),
            ),
        ],
    );
    add_response_examples(
        operation,
        "400",
        [(
            "invalid_asset_slug",
            "Invalid asset slug syntax",
            erc20_transfer_examples::invalid_asset_slug_response(),
        )],
    );
    add_response_examples(
        operation,
        "404",
        [(
            "unknown_asset_slug",
            "Unknown asset slug",
            erc20_transfer_examples::unknown_slug_rejection_response(),
        )],
    );
    add_response_examples(
        operation,
        "422",
        [
            (
                "native_asset_rejection",
                "Native asset is not an ERC-20 token",
                erc20_transfer_examples::native_asset_rejection_response(),
            ),
            (
                "too_many_filters",
                "Unique token-filter limit exceeded",
                erc20_transfer_examples::too_many_filters_response(),
            ),
        ],
    );
    add_response_examples(
        operation,
        "500",
        [(
            "internal_error",
            "Internal transfer response-shaping failure",
            erc20_transfer_examples::internal_error_response(),
        )],
    );
    add_response_examples(
        operation,
        "502",
        [(
            "upstream_provider_error",
            "Upstream provider failure",
            erc20_transfer_examples::upstream_provider_error_response(),
        )],
    );
    add_response_examples(
        operation,
        "503",
        [(
            "extraction_unavailable",
            "Transfer extraction unavailable",
            erc20_transfer_examples::extraction_unavailable_response(),
        )],
    );
    add_response_examples(
        operation,
        "504",
        [(
            "upstream_provider_timeout",
            "Upstream provider timeout",
            erc20_transfer_examples::upstream_provider_timeout_response(),
        )],
    );
}

fn add_response_examples<const N: usize>(
    operation: &mut Operation,
    status: &str,
    examples: [(&'static str, &'static str, Value); N],
) {
    let Some(RefOr::T(response)) = operation.responses.responses.get_mut(status) else {
        return;
    };
    let Some(content) = response.content.get_mut("application/json") else {
        return;
    };

    add_content_examples(content, examples);
}

fn add_content_examples<const N: usize>(
    content: &mut Content,
    examples: [(&'static str, &'static str, Value); N],
) {
    content.example = None;
    content.examples.clear();
    content
        .examples
        .extend(examples.into_iter().map(|(name, summary, value)| {
            (
                name.to_string(),
                ExampleBuilder::new()
                    .summary(summary)
                    .value(Some(value))
                    .build()
                    .into(),
            )
        }));
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::*;

    #[test]
    fn openapi_includes_erc20_transfer_schemas_by_default() {
        let json = document_json(&Config::default());
        let schemas = json["components"]["schemas"]
            .as_object()
            .expect("OpenAPI components.schemas should be an object");

        for schema in [
            "Erc20TransferSearchRequest",
            "Erc20TransferSearchResponse",
            "OnchainWindowDTO",
            "TokenFilterDTO",
            "ResolvedTokenFilterDTO",
            "Erc20TransferRow",
            "Erc20TransferToken",
            "Erc20TransferAmount",
        ] {
            assert!(schemas.contains_key(schema), "missing schema {schema}");
        }
    }

    #[test]
    fn erc20_transfer_search_path_is_absent_when_disabled() {
        let json = document_json(&Config::default());

        assert!(json["paths"]
            .as_object()
            .expect("OpenAPI paths should be an object")
            .get("/v1/erc20-transfers/search")
            .is_none());
    }

    #[test]
    fn erc20_transfer_search_path_is_present_when_enabled() {
        let json = document_json(&Config {
            erc20_transfers_enabled: true,
            ..Config::default()
        });

        let operation = json["paths"]
            .as_object()
            .expect("OpenAPI paths should be an object")
            .get("/v1/erc20-transfers/search")
            .expect("missing enabled transfer-search path");

        let responses = operation["post"]["responses"]
            .as_object()
            .expect("transfer-search responses should be an object");
        for status in ["200", "400", "404", "422", "500", "502", "503", "504"] {
            assert!(responses.contains_key(status), "missing response {status}");
        }
    }

    #[test]
    fn erc20_transfer_search_openapi_includes_public_examples() {
        let json = document_json(&Config {
            erc20_transfers_enabled: true,
            ..Config::default()
        });
        let operation = &json["paths"]["/v1/erc20-transfers/search"]["post"];
        let request_examples = &operation["requestBody"]["content"]["application/json"]["examples"];

        assert_eq!(
            example_value(request_examples, "unfiltered_search"),
            erc20_transfer_examples::unfiltered_request()
        );
        assert_eq!(
            example_value(request_examples, "asset_slug_filters"),
            erc20_transfer_examples::asset_slug_request()
        );
        assert_eq!(
            example_value(request_examples, "contract_address_filters"),
            erc20_transfer_examples::contract_address_request()
        );
        assert_eq!(
            example_value(request_examples, "mixed_filters"),
            erc20_transfer_examples::mixed_filter_request()
        );
        assert_eq!(
            example_value(request_examples, "native_asset_rejection"),
            erc20_transfer_examples::native_asset_rejection_request()
        );
        assert_eq!(
            example_value(request_examples, "unknown_slug_rejection"),
            erc20_transfer_examples::unknown_slug_rejection_request()
        );
        assert_eq!(
            example_value(request_examples, "too_many_filters"),
            erc20_transfer_examples::too_many_filters_request()
        );

        let responses = &operation["responses"];
        assert_eq!(
            response_example_value(responses, "200", "mixed_filters"),
            erc20_transfer_examples::mixed_success_response()
        );
        assert_eq!(
            response_example_value(responses, "200", "truncated_response"),
            erc20_transfer_examples::truncated_success_response()
        );
        assert_eq!(
            response_example_value(responses, "400", "invalid_asset_slug"),
            erc20_transfer_examples::invalid_asset_slug_response()
        );
        assert_eq!(
            response_example_value(responses, "404", "unknown_asset_slug"),
            erc20_transfer_examples::unknown_slug_rejection_response()
        );
        assert_eq!(
            response_example_value(responses, "422", "native_asset_rejection"),
            erc20_transfer_examples::native_asset_rejection_response()
        );
        assert_eq!(
            response_example_value(responses, "422", "too_many_filters"),
            erc20_transfer_examples::too_many_filters_response()
        );
        assert_eq!(
            response_example_value(responses, "500", "internal_error"),
            erc20_transfer_examples::internal_error_response()
        );
        assert_eq!(
            response_example_value(responses, "502", "upstream_provider_error"),
            erc20_transfer_examples::upstream_provider_error_response()
        );
        assert_eq!(
            response_example_value(responses, "503", "extraction_unavailable"),
            erc20_transfer_examples::extraction_unavailable_response()
        );
        assert_eq!(
            response_example_value(responses, "504", "upstream_provider_timeout"),
            erc20_transfer_examples::upstream_provider_timeout_response()
        );
    }

    #[test]
    fn erc20_transfer_request_schema_uses_stable_public_fields() {
        let json = document_json(&Config::default());
        let schemas = json["components"]["schemas"]
            .as_object()
            .expect("OpenAPI components.schemas should be an object");
        let request_schema = schemas
            .get("Erc20TransferSearchRequest")
            .expect("missing request schema");
        let properties = request_schema["properties"]
            .as_object()
            .expect("request schema should have object properties");

        for field in ["network_slug", "address", "direction", "tokens", "window"] {
            assert!(
                properties.contains_key(field),
                "missing request field {field}"
            );
        }
        for disallowed_field in ["chain", "chain_id", "chain_slug"] {
            assert!(
                !properties.contains_key(disallowed_field),
                "request schema must not expose {disallowed_field}"
            );
        }

        let required = request_schema["required"]
            .as_array()
            .expect("request schema should declare required fields")
            .iter()
            .map(|value| value.as_str().expect("required field should be a string"))
            .collect::<Vec<_>>();
        for field in ["network_slug", "address", "direction", "window"] {
            assert!(required.contains(&field), "missing required field {field}");
        }
        assert!(!required.contains(&"tokens"));
    }

    #[test]
    fn erc20_transfer_direction_schema_matches_public_enum_values() {
        let json = document_json(&Config::default());
        let schemas = json["components"]["schemas"]
            .as_object()
            .expect("OpenAPI components.schemas should be an object");
        let enum_values = schemas["TransferDirectionDTO"]["enum"]
            .as_array()
            .expect("direction schema should define enum values")
            .iter()
            .map(|value| value.as_str().expect("enum value should be a string"))
            .collect::<Vec<_>>();

        assert_eq!(enum_values, ["any", "from", "to"]);
    }

    fn document_json(config: &Config) -> Value {
        serde_json::to_value(document(config)).expect("OpenAPI should serialize")
    }

    fn response_example_value(responses: &Value, status: &str, name: &str) -> Value {
        example_value(
            &responses[status]["content"]["application/json"]["examples"],
            name,
        )
    }

    fn example_value(examples: &Value, name: &str) -> Value {
        examples[name]["value"].clone()
    }
}
