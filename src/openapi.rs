use serde_json::Value;
use utoipa::{
    openapi::{
        example::ExampleBuilder,
        path::Operation,
        security::{HttpAuthScheme, HttpBuilder, SecurityRequirement, SecurityScheme},
        Components, Content, RefOr,
    },
    OpenApi,
};

use crate::adapters::http::dto::balances::{
    examples as balance_examples, BalanceAccountIdentityPayload, BalanceAccountPayload,
    BalanceAccountRequest, BalanceAmountPayload, BalanceAsOfRequest, BalanceBlockPayload,
    BalanceErrorPayload, BalanceEvidencePayload, BalancePositionPayload, BalanceQuotePayload,
    BalanceQuoteStatus, BalanceResponseStatus, BalanceSkippedPayload, BalanceSummaryPayload,
    BalanceTokenSelectorRequest, BulkAsOfPayload, BulkBalanceRequest, BulkBalanceResponse,
    SingleAsOfPayload, SingleBalanceRequest, SingleBalanceResponse,
};
use crate::adapters::http::dto::erc20_transfers::{
    examples as erc20_transfer_examples, Erc20TransferAccount, Erc20TransferAmount,
    Erc20TransferRow, Erc20TransferSearchLimits, Erc20TransferSearchRequest,
    Erc20TransferSearchResponse, Erc20TransferToken,
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

const BETA_API_KEY_AUTH_SCHEME: &str = "BetaApiKeyAuth";

pub(crate) fn document(config: &Config) -> utoipa::openapi::OpenApi {
    let mut document = if config.erc20_transfers_enabled {
        Erc20TransfersApiDoc::openapi()
    } else {
        BaseApiDoc::openapi()
    };

    add_balance_examples(&mut document);

    if config.erc20_transfers_enabled {
        add_erc20_transfer_examples(&mut document);
    }

    add_beta_api_key_security(&mut document);

    document
}

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Iron Burrow Mother API",
        version = env!("CARGO_PKG_VERSION")
    ),
    paths(resolve_single_balance_operation, resolve_bulk_balances_operation),
    components(schemas(
        BalanceAccountIdentityPayload,
        BalanceAccountPayload,
        BalanceAccountRequest,
        BalanceAmountPayload,
        BalanceAsOfRequest,
        BalanceTokenSelectorRequest,
        BalanceBlockPayload,
        BalanceErrorPayload,
        BalanceEvidencePayload,
        BalancePositionPayload,
        BalanceQuotePayload,
        BalanceQuoteStatus,
        BalanceResponseStatus,
        BalanceSkippedPayload,
        BalanceSummaryPayload,
        BulkAsOfPayload,
        BulkBalanceRequest,
        BulkBalanceResponse,
        Erc20TransferAmount,
        Erc20TransferAccount,
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
        ResolvedTokenFilterDTO,
        SingleAsOfPayload,
        SingleBalanceRequest,
        SingleBalanceResponse
    ))
)]
struct BaseApiDoc;

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Iron Burrow Mother API",
        version = env!("CARGO_PKG_VERSION")
    ),
    paths(
        resolve_single_balance_operation,
        resolve_bulk_balances_operation,
        erc20_transfer_search_operation
    ),
    components(schemas(
        BalanceAccountIdentityPayload,
        BalanceAccountPayload,
        BalanceAccountRequest,
        BalanceAmountPayload,
        BalanceAsOfRequest,
        BalanceTokenSelectorRequest,
        BalanceBlockPayload,
        BalanceErrorPayload,
        BalanceEvidencePayload,
        BalancePositionPayload,
        BalanceQuotePayload,
        BalanceQuoteStatus,
        BalanceResponseStatus,
        BalanceSkippedPayload,
        BalanceSummaryPayload,
        BulkAsOfPayload,
        BulkBalanceRequest,
        BulkBalanceResponse,
        Erc20TransferAmount,
        Erc20TransferAccount,
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
        ResolvedTokenFilterDTO,
        SingleAsOfPayload,
        SingleBalanceRequest,
        SingleBalanceResponse
    ))
)]
struct Erc20TransfersApiDoc;

#[utoipa::path(
    post,
    path = "/v1/balances",
    tag = "balances",
    summary = "Resolve one latest balance snapshot",
    description = "Resolves one latest EVM balance snapshot for a canonical network_slug and explicit tokens.asset_slugs. Requests use network_slug, never chain or chain_id. Supported quote_currency values are USD, MXN, USDC, and BTC. The single endpoint accepts exactly one account and up to 20 asset_slug selectors, for at most 20 account-token resolution items. tokens.contract_addresses and historical as_of forms are reserved for later SPEC-012 slices and are rejected by this implementation.",
    request_body(
        content = SingleBalanceRequest,
        content_type = "application/json"
    ),
    responses(
        (
            status = 200,
            description = "Single-account balance snapshot. Provider failures for supported balance items remain item-level errors inside this response.",
            body = SingleBalanceResponse
        ),
        (
            status = 400,
            description = "Malformed, semantically invalid, or oversized balance request",
            body = ErrorResponse
        ),
        (
            status = 401,
            description = "The protected Beta route request lacks a valid active API key",
            body = ErrorResponse
        ),
        (
            status = 429,
            description = "The valid Beta API key exceeded a configured request limit",
            body = ErrorResponse
        ),
        (
            status = 500,
            description = "Mother API detected an internally inconsistent balance state",
            body = ErrorResponse
        ),
        (
            status = 503,
            description = "Mother API balance catalog or API-key authentication storage is temporarily unavailable",
            body = ErrorResponse
        )
    )
)]
#[allow(dead_code)]
async fn resolve_single_balance_operation() {}

#[utoipa::path(
    post,
    path = "/v1/balances/bulk",
    tag = "balances",
    summary = "Resolve latest balance snapshots in bulk",
    description = "Resolves latest EVM balance snapshots for explicit canonical network_slug accounts and tokens.asset_slugs. Requests use network_slug, never chain or chain_id. Supported quote_currency values are USD, MXN, USDC, and BTC. Bulk accepts 1 to 50 accounts, up to 20 asset_slug selectors, and up to 1,000 account-token resolution items. tokens.contract_addresses and historical as_of forms are reserved for later SPEC-012 slices and are rejected by this implementation.",
    request_body(
        content = BulkBalanceRequest,
        content_type = "application/json"
    ),
    responses(
        (
            status = 200,
            description = "Bulk balance snapshot response. Provider failures for supported balance items remain per-account item-level errors inside this response.",
            body = BulkBalanceResponse
        ),
        (
            status = 400,
            description = "Malformed, semantically invalid, or oversized balance request",
            body = ErrorResponse
        ),
        (
            status = 401,
            description = "The protected Beta route request lacks a valid active API key",
            body = ErrorResponse
        ),
        (
            status = 429,
            description = "The valid Beta API key exceeded a configured request limit",
            body = ErrorResponse
        ),
        (
            status = 500,
            description = "Mother API detected an internally inconsistent balance state",
            body = ErrorResponse
        ),
        (
            status = 503,
            description = "Mother API balance catalog or API-key authentication storage is temporarily unavailable",
            body = ErrorResponse
        )
    )
)]
#[allow(dead_code)]
async fn resolve_bulk_balances_operation() {}

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
            status = 401,
            description = "The protected Beta route request lacks a valid active API key",
            body = ErrorResponse
        ),
        (
            status = 404,
            description = "Requested network is unsupported for transfer search",
            body = ErrorResponse
        ),
        (
            status = 429,
            description = "The valid Beta API key exceeded a configured request limit",
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
            description = "Transfer extraction, asset-contract mapping, or API-key authentication storage is temporarily unavailable",
            body = ErrorResponse
        ),
        (
            status = 504,
            description = "Transfer extraction or upstream provider timed out",
            body = ErrorResponse
        )
    )
)]
#[allow(dead_code)]
async fn erc20_transfer_search_operation() {}

fn add_beta_api_key_security(document: &mut utoipa::openapi::OpenApi) {
    let components = document.components.get_or_insert_with(Components::new);
    components.add_security_scheme(
        BETA_API_KEY_AUTH_SCHEME,
        SecurityScheme::Http(
            HttpBuilder::new()
                .scheme(HttpAuthScheme::Bearer)
                .bearer_format("API key")
                .description(Some(
                    "Private Beta API key presented as `Authorization: Bearer <api_key>`.",
                ))
                .build(),
        ),
    );

    for path_item in document.paths.paths.values_mut() {
        for operation in [
            path_item.get.as_mut(),
            path_item.post.as_mut(),
            path_item.put.as_mut(),
            path_item.delete.as_mut(),
            path_item.options.as_mut(),
            path_item.head.as_mut(),
            path_item.patch.as_mut(),
            path_item.trace.as_mut(),
        ]
        .into_iter()
        .flatten()
        {
            operation.security = Some(vec![SecurityRequirement::new(
                BETA_API_KEY_AUTH_SCHEME,
                Vec::<String>::new(),
            )]);
        }
    }
}

fn add_balance_examples(document: &mut utoipa::openapi::OpenApi) {
    add_single_balance_examples(document);
    add_bulk_balance_examples(document);
}

fn add_single_balance_examples(document: &mut utoipa::openapi::OpenApi) {
    let Some(path_item) = document.paths.paths.get_mut("/v1/balances") else {
        return;
    };
    let Some(operation) = path_item.post.as_mut() else {
        return;
    };

    if let Some(request_body) = operation.request_body.as_mut() {
        if let Some(content) = request_body.content.get_mut("application/json") {
            add_content_examples(
                content,
                [(
                    "single_balance",
                    "Single latest balance request",
                    balance_examples::single_request(),
                )],
            );
        }
    }

    add_response_examples(
        operation,
        "200",
        [
            (
                "success",
                "Successful single-account balance response",
                balance_examples::single_success_response(),
            ),
            (
                "item_level_provider_failure",
                "Item-level balance provider failure response",
                balance_examples::single_item_level_failure_response(),
            ),
        ],
    );
    add_response_examples(
        operation,
        "400",
        [
            (
                "validation_error",
                "Invalid balance request",
                balance_examples::validation_error_response(),
            ),
            (
                "request_too_large",
                "Balance request exceeds public limits",
                balance_examples::request_too_large_response(),
            ),
        ],
    );
    add_protected_route_error_examples(operation);
}

fn add_bulk_balance_examples(document: &mut utoipa::openapi::OpenApi) {
    let Some(path_item) = document.paths.paths.get_mut("/v1/balances/bulk") else {
        return;
    };
    let Some(operation) = path_item.post.as_mut() else {
        return;
    };

    if let Some(request_body) = operation.request_body.as_mut() {
        if let Some(content) = request_body.content.get_mut("application/json") {
            add_content_examples(
                content,
                [(
                    "bulk_balances",
                    "Bulk latest balance request",
                    balance_examples::bulk_request(),
                )],
            );
        }
    }

    add_response_examples(
        operation,
        "200",
        [
            (
                "success",
                "Successful bulk balance response",
                balance_examples::bulk_success_response(),
            ),
            (
                "skipped_item",
                "Unsupported asset-network pair skipped",
                balance_examples::skipped_item_response(),
            ),
            (
                "item_level_provider_failure",
                "Per-account item-level balance provider failure response",
                balance_examples::item_level_failure_response(),
            ),
        ],
    );
    add_response_examples(
        operation,
        "400",
        [
            (
                "validation_error",
                "Invalid balance request",
                balance_examples::validation_error_response(),
            ),
            (
                "request_too_large",
                "Balance request exceeds public limits",
                balance_examples::request_too_large_response(),
            ),
        ],
    );
    add_protected_route_error_examples(operation);
}

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
            (
                "window_too_large",
                "Search window exceeds the public limit",
                erc20_transfer_examples::window_too_large_response(),
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
        [
            (
                "upstream_provider_timeout",
                "Upstream provider timeout",
                erc20_transfer_examples::upstream_provider_timeout_response(),
            ),
            (
                "extraction_timeout",
                "Overall extraction timeout",
                erc20_transfer_examples::extraction_timeout_response(),
            ),
        ],
    );
    add_protected_route_error_examples(operation);
}

fn add_protected_route_error_examples(operation: &mut Operation) {
    add_response_examples(
        operation,
        "401",
        [(
            "unauthorized",
            "Missing or invalid Beta API key",
            unauthorized_response(),
        )],
    );
    add_response_examples(
        operation,
        "429",
        [(
            "rate_limited",
            "Valid Beta API key exceeded a configured request limit",
            rate_limited_response(),
        )],
    );
    append_response_examples(
        operation,
        "503",
        [(
            "api_key_authentication_unavailable",
            "API-key authentication storage unavailable",
            database_unavailable_for_auth_response(),
        )],
    );
}

fn unauthorized_response() -> Value {
    serde_json::json!({
        "ok": false,
        "error": {
            "code": "unauthorized",
            "message": "The request lacks a valid active API key."
        }
    })
}

fn rate_limited_response() -> Value {
    serde_json::json!({
        "ok": false,
        "error": {
            "code": "rate_limited",
            "message": "The valid API key exceeded a request limit."
        }
    })
}

fn database_unavailable_for_auth_response() -> Value {
    serde_json::json!({
        "ok": false,
        "error": {
            "code": "database_unavailable",
            "message": "API-key authentication is temporarily unavailable."
        }
    })
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

fn append_response_examples<const N: usize>(
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

    append_content_examples(content, examples);
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

fn append_content_examples<const N: usize>(
    content: &mut Content,
    examples: [(&'static str, &'static str, Value); N],
) {
    content.example = None;
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
    use std::collections::BTreeSet;

    use serde_json::Value;

    use super::*;

    #[test]
    fn openapi_includes_balance_schemas_by_default() {
        let json = document_json(&Config::default());
        let schemas = json["components"]["schemas"]
            .as_object()
            .expect("OpenAPI components.schemas should be an object");

        for schema in [
            "SingleBalanceRequest",
            "BulkBalanceRequest",
            "BalanceAsOfRequest",
            "BalanceAccountRequest",
            "BalanceTokenSelectorRequest",
            "SingleBalanceResponse",
            "BulkBalanceResponse",
            "BalanceResponseStatus",
            "BalanceQuoteStatus",
            "BalanceAccountPayload",
            "BalancePositionPayload",
            "BalanceErrorPayload",
            "ErrorResponse",
        ] {
            assert!(schemas.contains_key(schema), "missing schema {schema}");
        }
    }

    #[test]
    fn default_openapi_paths_match_beta_balance_surface() {
        let json = document_json(&Config::default());
        let paths = path_set(&json);

        assert_eq!(
            paths,
            BTreeSet::from(["/v1/balances".to_string(), "/v1/balances/bulk".to_string()])
        );
    }

    #[test]
    fn enabled_openapi_paths_add_transfer_search() {
        let json = document_json(&Config {
            erc20_transfers_enabled: true,
            ..Config::default()
        });
        let paths = path_set(&json);

        assert_eq!(
            paths,
            BTreeSet::from([
                "/v1/balances".to_string(),
                "/v1/balances/bulk".to_string(),
                "/v1/erc20-transfers/search".to_string()
            ])
        );
    }

    #[test]
    fn openapi_declares_beta_api_key_bearer_security() {
        let json = document_json(&Config::default());
        let scheme = &json["components"]["securitySchemes"][BETA_API_KEY_AUTH_SCHEME];

        assert_eq!(scheme["type"], "http");
        assert_eq!(scheme["scheme"], "bearer");
        assert_eq!(scheme["bearerFormat"], "API key");
        assert!(scheme["description"]
            .as_str()
            .expect("security scheme should include a description")
            .contains("Authorization: Bearer <api_key>"));
    }

    #[test]
    fn generated_beta_paths_require_beta_api_key_security() {
        let json = document_json(&Config {
            erc20_transfers_enabled: true,
            ..Config::default()
        });

        for path in [
            "/v1/balances",
            "/v1/balances/bulk",
            "/v1/erc20-transfers/search",
        ] {
            assert_beta_api_key_security(&json["paths"][path]["post"]);
        }
    }

    #[test]
    fn balance_paths_include_request_bodies_statuses_and_public_notes() {
        let json = document_json(&Config::default());

        assert_balance_operation(
            &json,
            "/v1/balances",
            "SingleBalanceRequest",
            "SingleBalanceResponse",
            ["one account", "20 asset_slug selectors", "20 account-token"],
        );
        assert_balance_operation(
            &json,
            "/v1/balances/bulk",
            "BulkBalanceRequest",
            "BulkBalanceResponse",
            ["50 accounts", "20 asset_slug selectors", "1,000"],
        );
    }

    #[test]
    fn balance_schemas_use_stable_public_fields() {
        let json = document_json(&Config::default());
        let schemas = &json["components"]["schemas"];

        assert_schema_properties(
            schemas,
            "SingleBalanceRequest",
            &["account", "as_of", "quote_currency", "tokens"],
        );
        assert_schema_properties(
            schemas,
            "BulkBalanceRequest",
            &["accounts", "as_of", "quote_currency", "tokens"],
        );
        assert_schema_properties(
            schemas,
            "BalanceAsOfRequest",
            &["block_number", "kind", "timestamp"],
        );
        assert_schema_properties(
            schemas,
            "BalanceAccountRequest",
            &["address", "client_ref", "network_slug"],
        );
        assert_schema_properties(
            schemas,
            "BalanceTokenSelectorRequest",
            &["asset_slugs", "contract_addresses"],
        );
        assert_schema_properties(
            schemas,
            "SingleBalanceResponse",
            &[
                "account",
                "as_of",
                "errors",
                "evidence",
                "ok",
                "positions",
                "quote_currency",
                "skipped",
                "status",
                "type",
            ],
        );
        assert_schema_properties(
            schemas,
            "BulkBalanceResponse",
            &[
                "accounts",
                "as_of",
                "errors",
                "ok",
                "quote_currency",
                "status",
                "summary",
                "type",
            ],
        );
        assert_schema_properties(schemas, "SingleAsOfPayload", &["kind", "observed_at"]);
        assert_schema_properties(schemas, "BulkAsOfPayload", &["kind"]);
        assert_schema_properties(
            schemas,
            "BalanceSummaryPayload",
            &[
                "failed_items",
                "positions_returned",
                "requested_accounts",
                "requested_assets",
                "requested_resolution_items",
                "skipped_items",
            ],
        );
        assert_schema_properties(
            schemas,
            "BalanceAccountPayload",
            &[
                "account",
                "errors",
                "evidence",
                "positions",
                "skipped",
                "status",
            ],
        );
        assert_schema_properties(
            schemas,
            "BalanceAccountIdentityPayload",
            &["address", "client_ref", "network_slug"],
        );
        assert_schema_properties(
            schemas,
            "BalanceEvidencePayload",
            &["block", "network_slug", "observed_at", "source"],
        );
        assert_schema_properties(schemas, "BalanceBlockPayload", &["hash", "number"]);
        assert_schema_properties(
            schemas,
            "BalancePositionPayload",
            &["asset_slug", "balance", "network_slug", "quote", "symbol"],
        );
        assert_schema_properties(
            schemas,
            "BalanceAmountPayload",
            &["amount", "decimals", "raw_amount"],
        );
        assert_schema_properties(
            schemas,
            "BalanceQuotePayload",
            &["currency", "price_as_of", "status", "unit_price", "value"],
        );
        assert_schema_properties(
            schemas,
            "BalanceSkippedPayload",
            &["asset_slug", "network_slug", "reason"],
        );
        assert_schema_properties(
            schemas,
            "BalanceErrorPayload",
            &["asset_slug", "code", "message", "network_slug"],
        );

        for disallowed_field in [
            "chain",
            "chain_id",
            "chain_slug",
            "route_id",
            "provider_id",
            "upstream_url",
        ] {
            assert!(
                !schema_property_exists(schemas, disallowed_field),
                "OpenAPI schemas must not expose {disallowed_field}"
            );
        }
    }

    #[test]
    fn balance_openapi_enums_match_public_values() {
        let json = document_json(&Config::default());
        let schemas = &json["components"]["schemas"];

        assert_eq!(
            schema_enum_values(schemas, "BalanceResponseStatus"),
            ["complete", "partial", "failed"]
        );
        assert_eq!(
            schema_enum_values(schemas, "BalanceQuoteStatus"),
            ["available", "unavailable", "unsupported"]
        );
    }

    #[test]
    fn balance_openapi_includes_public_examples_and_they_are_valid() {
        let json = document_json(&Config::default());

        let single_operation = &json["paths"]["/v1/balances"]["post"];
        let single_request_examples =
            &single_operation["requestBody"]["content"]["application/json"]["examples"];
        let single_responses = &single_operation["responses"];

        let single_request = example_value(single_request_examples, "single_balance");
        assert_eq!(single_request, balance_examples::single_request());
        serde_json::from_value::<SingleBalanceRequest>(single_request)
            .expect("single request example should deserialize");

        let single_success = response_example_value(single_responses, "200", "success");
        assert_eq!(single_success, balance_examples::single_success_response());
        serde_json::from_value::<SingleBalanceResponse>(single_success)
            .expect("single success example should deserialize");

        let single_failure =
            response_example_value(single_responses, "200", "item_level_provider_failure");
        assert_eq!(
            single_failure,
            balance_examples::single_item_level_failure_response()
        );
        serde_json::from_value::<SingleBalanceResponse>(single_failure)
            .expect("single item-level failure example should deserialize");

        assert_error_example(response_example_value(
            single_responses,
            "400",
            "validation_error",
        ));
        assert_error_example(response_example_value(
            single_responses,
            "400",
            "request_too_large",
        ));
        assert_protected_route_error_examples(single_responses);

        let bulk_operation = &json["paths"]["/v1/balances/bulk"]["post"];
        let bulk_request_examples =
            &bulk_operation["requestBody"]["content"]["application/json"]["examples"];
        let bulk_responses = &bulk_operation["responses"];

        let bulk_request = example_value(bulk_request_examples, "bulk_balances");
        assert_eq!(bulk_request, balance_examples::bulk_request());
        serde_json::from_value::<BulkBalanceRequest>(bulk_request)
            .expect("bulk request example should deserialize");

        for (name, expected) in [
            ("success", balance_examples::bulk_success_response()),
            ("skipped_item", balance_examples::skipped_item_response()),
            (
                "item_level_provider_failure",
                balance_examples::item_level_failure_response(),
            ),
        ] {
            let value = response_example_value(bulk_responses, "200", name);
            assert_eq!(value, expected);
            serde_json::from_value::<BulkBalanceResponse>(value)
                .expect("bulk response example should deserialize");
        }

        assert_error_example(response_example_value(
            bulk_responses,
            "400",
            "validation_error",
        ));
        assert_error_example(response_example_value(
            bulk_responses,
            "400",
            "request_too_large",
        ));
        assert_protected_route_error_examples(bulk_responses);
    }

    #[test]
    fn openapi_does_not_expose_hidden_or_disabled_routes() {
        let json = document_json(&Config::default());
        let paths = json["paths"]
            .as_object()
            .expect("OpenAPI paths should be an object");

        for hidden_path in [
            "/v1/status",
            "/v1/assets",
            "/v1/assets/{slug}",
            "/v1/assets/{slug}/signal/price-stats",
            "/v1/assets/{slug}/signal/price-trend",
            "/v1/search-engine",
            "/v1/erc20-transfers/search",
        ] {
            assert!(
                !paths.contains_key(hidden_path),
                "OpenAPI must not expose hidden or disabled path {hidden_path}"
            );
        }

        for path in paths.keys() {
            assert!(
                !path.starts_with("/v1/predictions"),
                "OpenAPI must not expose removed prediction path {path}"
            );
        }
    }

    #[test]
    fn openapi_includes_erc20_transfer_schemas_by_default() {
        let json = document_json(&Config::default());
        let schemas = json["components"]["schemas"]
            .as_object()
            .expect("OpenAPI components.schemas should be an object");

        for schema in [
            "Erc20TransferSearchRequest",
            "Erc20TransferSearchResponse",
            "Erc20TransferAccount",
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
        for status in [
            "200", "400", "401", "404", "429", "422", "500", "502", "503", "504",
        ] {
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
            response_example_value(responses, "422", "window_too_large"),
            erc20_transfer_examples::window_too_large_response()
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
        assert_protected_route_error_examples(responses);
        assert_eq!(
            response_example_value(responses, "504", "upstream_provider_timeout"),
            erc20_transfer_examples::upstream_provider_timeout_response()
        );
        assert_eq!(
            response_example_value(responses, "504", "extraction_timeout"),
            erc20_transfer_examples::extraction_timeout_response()
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

        for field in ["account", "direction", "tokens", "window"] {
            assert!(
                properties.contains_key(field),
                "missing request field {field}"
            );
        }
        for disallowed_field in ["network_slug", "address", "chain", "chain_id", "chain_slug"] {
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
        for field in ["account", "direction", "window"] {
            assert!(required.contains(&field), "missing required field {field}");
        }
        assert!(!required.contains(&"tokens"));

        let account_schema = schemas
            .get("Erc20TransferAccount")
            .expect("missing account schema");
        let account_properties = account_schema["properties"]
            .as_object()
            .expect("account schema should have object properties");
        for field in ["network_slug", "address", "client_ref"] {
            assert!(
                account_properties.contains_key(field),
                "missing account field {field}"
            );
        }
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

    fn path_set(json: &Value) -> BTreeSet<String> {
        json["paths"]
            .as_object()
            .expect("OpenAPI paths should be an object")
            .keys()
            .cloned()
            .collect()
    }

    fn assert_balance_operation(
        json: &Value,
        path: &str,
        request_schema: &str,
        response_schema: &str,
        expected_description_fragments: [&str; 3],
    ) {
        let operation = &json["paths"][path]["post"];
        assert!(
            operation.is_object(),
            "balance path {path} should expose a POST operation"
        );
        assert_eq!(
            operation["requestBody"]["content"]["application/json"]["schema"]["$ref"],
            format!("#/components/schemas/{request_schema}")
        );
        assert_eq!(
            operation["responses"]["200"]["content"]["application/json"]["schema"]["$ref"],
            format!("#/components/schemas/{response_schema}")
        );

        for status in ["400", "401", "429", "500", "503"] {
            assert_eq!(
                operation["responses"][status]["content"]["application/json"]["schema"]["$ref"],
                "#/components/schemas/ErrorResponse",
                "balance path {path} should expose ErrorResponse for {status}"
            );
        }

        let responses = operation["responses"]
            .as_object()
            .expect("balance responses should be an object");
        assert_eq!(
            responses.keys().cloned().collect::<BTreeSet<_>>(),
            BTreeSet::from([
                "200".to_string(),
                "400".to_string(),
                "401".to_string(),
                "429".to_string(),
                "500".to_string(),
                "503".to_string()
            ])
        );

        let description = operation["description"]
            .as_str()
            .expect("balance operation should have a public description");
        for fragment in [
            "network_slug",
            "USD",
            "MXN",
            "USDC",
            "BTC",
            expected_description_fragments[0],
            expected_description_fragments[1],
            expected_description_fragments[2],
        ] {
            assert!(
                description.contains(fragment),
                "balance path {path} description should mention {fragment}"
            );
        }
    }

    fn assert_schema_properties(schemas: &Value, schema_name: &str, expected_fields: &[&str]) {
        let properties = schemas[schema_name]["properties"]
            .as_object()
            .unwrap_or_else(|| panic!("{schema_name} should define object properties"));
        let actual = properties.keys().cloned().collect::<BTreeSet<_>>();
        let expected = expected_fields
            .iter()
            .map(|field| field.to_string())
            .collect::<BTreeSet<_>>();

        assert_eq!(actual, expected, "unexpected fields for {schema_name}");
    }

    fn schema_property_exists(schema: &Value, property_name: &str) -> bool {
        match schema {
            Value::Object(object) => {
                if object
                    .get("properties")
                    .and_then(Value::as_object)
                    .is_some_and(|properties| properties.contains_key(property_name))
                {
                    return true;
                }

                object
                    .values()
                    .any(|value| schema_property_exists(value, property_name))
            }
            Value::Array(values) => values
                .iter()
                .any(|value| schema_property_exists(value, property_name)),
            _ => false,
        }
    }

    fn schema_enum_values<'a>(schemas: &'a Value, schema_name: &str) -> Vec<&'a str> {
        schemas[schema_name]["enum"]
            .as_array()
            .unwrap_or_else(|| panic!("{schema_name} should define enum values"))
            .iter()
            .map(|value| value.as_str().expect("enum value should be a string"))
            .collect()
    }

    fn assert_error_example(value: Value) {
        assert_eq!(value["ok"], false);
        assert!(
            value["error"]["code"].is_string(),
            "error example should expose error.code"
        );
        assert!(
            value["error"]["message"].is_string(),
            "error example should expose error.message"
        );
        assert_eq!(
            value
                .as_object()
                .expect("error response should be an object")
                .keys()
                .cloned()
                .collect::<BTreeSet<_>>(),
            BTreeSet::from(["error".to_string(), "ok".to_string()])
        );
    }

    fn assert_beta_api_key_security(operation: &Value) {
        assert_eq!(
            operation["security"],
            serde_json::json!([{ BETA_API_KEY_AUTH_SCHEME: [] }])
        );
    }

    fn assert_protected_route_error_examples(responses: &Value) {
        let unauthorized = response_example_value(responses, "401", "unauthorized");
        assert_error_example(unauthorized.clone());
        assert_eq!(unauthorized, unauthorized_response());

        let rate_limited = response_example_value(responses, "429", "rate_limited");
        assert_error_example(rate_limited.clone());
        assert_eq!(rate_limited, rate_limited_response());

        let database_unavailable =
            response_example_value(responses, "503", "api_key_authentication_unavailable");
        assert_error_example(database_unavailable.clone());
        assert_eq!(
            database_unavailable,
            database_unavailable_for_auth_response()
        );
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
