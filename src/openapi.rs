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
            status = 200,
            description = "ERC-20 transfer search response",
            body = Erc20TransferSearchResponse
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

        assert!(json["paths"]
            .as_object()
            .expect("OpenAPI paths should be an object")
            .get("/v1/erc20-transfers/search")
            .is_some());
    }

    fn document_json(config: &Config) -> Value {
        serde_json::to_value(document(config)).expect("OpenAPI should serialize")
    }
}
