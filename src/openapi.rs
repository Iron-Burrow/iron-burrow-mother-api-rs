use utoipa::OpenApi;

use crate::{
    config::Config,
    erc20_transfers::{
        Erc20TransferAmount, Erc20TransferBlockWindow, Erc20TransferDirection,
        Erc20TransferLookbackTarget, Erc20TransferLookbackWindow, Erc20TransferRow,
        Erc20TransferSearchLimits, Erc20TransferSearchRequest, Erc20TransferSearchResponse,
        Erc20TransferSearchWindow, Erc20TransferTimestampWindow, Erc20TransferToken,
        Erc20TransferTokenFilterResolution, Erc20TransferTokenFilterSource,
        Erc20TransferTokenFilters, ResolvedErc20TokenFilter,
    },
    error::{ErrorBody, ErrorResponse},
};

pub fn document(config: &Config) -> utoipa::openapi::OpenApi {
    if config.erc20_transfers_enabled {
        Erc20TransfersApiDoc::openapi()
    } else {
        BaseApiDoc::openapi()
    }
}

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Iron Burrow Mother API",
        version = env!("CARGO_PKG_VERSION")
    ),
    components(schemas(
        Erc20TransferAmount,
        Erc20TransferBlockWindow,
        Erc20TransferDirection,
        Erc20TransferLookbackTarget,
        Erc20TransferLookbackWindow,
        Erc20TransferRow,
        Erc20TransferSearchLimits,
        Erc20TransferSearchRequest,
        Erc20TransferSearchResponse,
        Erc20TransferSearchWindow,
        Erc20TransferTimestampWindow,
        Erc20TransferToken,
        Erc20TransferTokenFilterResolution,
        Erc20TransferTokenFilterSource,
        Erc20TransferTokenFilters,
        ErrorBody,
        ErrorResponse,
        ResolvedErc20TokenFilter
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
        Erc20TransferBlockWindow,
        Erc20TransferDirection,
        Erc20TransferLookbackTarget,
        Erc20TransferLookbackWindow,
        Erc20TransferRow,
        Erc20TransferSearchLimits,
        Erc20TransferSearchRequest,
        Erc20TransferSearchResponse,
        Erc20TransferSearchWindow,
        Erc20TransferTimestampWindow,
        Erc20TransferToken,
        Erc20TransferTokenFilterResolution,
        Erc20TransferTokenFilterSource,
        Erc20TransferTokenFilters,
        ErrorBody,
        ErrorResponse,
        ResolvedErc20TokenFilter
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
            status = 503,
            description = "Transfer extraction is not available in this validation-only slice",
            body = ErrorResponse
        )
    )
)]
#[allow(dead_code)]
async fn erc20_transfer_search_operation() {}

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
            "Erc20TransferSearchWindow",
            "Erc20TransferTokenFilters",
            "ResolvedErc20TokenFilter",
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
        assert!(responses.contains_key("400"));
        assert!(responses.contains_key("404"));
        assert!(responses.contains_key("422"));
        assert!(responses.contains_key("503"));
        assert!(!responses.contains_key("200"));
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
        let enum_values = schemas["Erc20TransferDirection"]["enum"]
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
}
