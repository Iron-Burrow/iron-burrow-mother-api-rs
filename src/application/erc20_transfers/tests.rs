use std::sync::Mutex;

use crate::{
    application::{
        erc20_transfers::service::{
            build_search_plan, search_erc20_transfers, Erc20TransferExtractionError,
            Erc20TransferExtractionRequest, Erc20TransferExtractionResult,
            Erc20TransferExtractionRow, Erc20TransferExtractor, Erc20TransferSearchError,
            Erc20TransferSearchInput, Erc20TransferTokenFilterSource,
        },
        filters::{
            onchain_window::{BlockWindow, OnchainWindow},
            transfer_direction::TransferDirection,
        },
    },
    test_utils::fixtures::global_assets::global_assets_repository,
};

const TEST_MAX_TOKEN_FILTERS: u64 = 20;

#[tokio::test]
async fn search_resolves_tokens_before_calling_extractor() {
    let extractor = RecordingExtractor::ok();
    let input = transfer_search_input(
        "0xABC0000000000000000000000000000000000000",
        vec!["usdc".to_string()],
        vec!["0xA0B86991C6218B36C1D19D4A2E9EB0CE3606EB48".to_string()],
    );

    let result = search_erc20_transfers(
        input,
        Some(global_assets_repository()),
        TEST_MAX_TOKEN_FILTERS,
        &extractor,
    )
    .await
    .unwrap();

    let requests = extractor.requests.lock().unwrap();
    assert_eq!(
        *requests,
        [Erc20TransferExtractionRequest {
            network_slug: "eth-mainnet".to_string(),
            address: "0xabc0000000000000000000000000000000000000".to_string(),
            direction: TransferDirection::Any,
            window: block_window(),
            contract_addresses: vec!["0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48".to_string()],
        }]
    );
    assert_eq!(
        result.plan.extraction_request.contract_addresses,
        ["0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"]
    );
    assert_eq!(result.extraction.rows.len(), 1);
}

#[tokio::test]
async fn resolution_failure_does_not_call_extractor() {
    let extractor = RecordingExtractor::ok();
    let input = transfer_search_input(
        "0xabc0000000000000000000000000000000000000",
        vec!["missing-but-syntactically-valid".to_string()],
        Vec::new(),
    );

    let error = search_erc20_transfers(
        input,
        Some(global_assets_repository()),
        TEST_MAX_TOKEN_FILTERS,
        &extractor,
    )
    .await
    .unwrap_err();

    assert!(matches!(error, Erc20TransferSearchError::AssetNotFound));
    assert!(extractor.requests.lock().unwrap().is_empty());
}

#[tokio::test]
async fn max_token_filter_enforcement_happens_before_extraction() {
    let extractor = RecordingExtractor::ok();
    let input = transfer_search_input(
        "0xabc0000000000000000000000000000000000000",
        vec!["usdc".to_string()],
        vec!["0x1111111111111111111111111111111111111111".to_string()],
    );

    let error = search_erc20_transfers(input, Some(global_assets_repository()), 1, &extractor)
        .await
        .unwrap_err();

    assert!(matches!(
        error,
        Erc20TransferSearchError::TooManyTokenFilters
    ));
    assert!(extractor.requests.lock().unwrap().is_empty());
}

#[tokio::test]
async fn empty_token_filters_preserve_unfiltered_extraction() {
    let extractor = RecordingExtractor::ok();
    let input = transfer_search_input(
        "0xabc0000000000000000000000000000000000000",
        Vec::new(),
        Vec::new(),
    );

    search_erc20_transfers(input, None, TEST_MAX_TOKEN_FILTERS, &extractor)
        .await
        .unwrap();

    let requests = extractor.requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].contract_addresses.is_empty());
}

#[tokio::test]
async fn executable_request_keeps_public_filters_outside_extraction_intent() {
    let input = transfer_search_input(
        "0xabc0000000000000000000000000000000000000",
        vec!["usdc".to_string()],
        vec!["0x1111111111111111111111111111111111111111".to_string()],
    );

    let plan = build_search_plan(
        input,
        Some(global_assets_repository()),
        TEST_MAX_TOKEN_FILTERS,
    )
    .await
    .unwrap();

    assert_eq!(plan.requested_token_filters.asset_slugs, ["usdc"]);
    assert_eq!(
        plan.extraction_request.contract_addresses,
        [
            "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
            "0x1111111111111111111111111111111111111111",
        ]
    );
}

#[tokio::test]
async fn explicit_known_contract_filter_gets_catalog_metadata() {
    let extractor = RecordingExtractor::ok();
    let input = transfer_search_input(
        "0xabc0000000000000000000000000000000000000",
        Vec::new(),
        vec!["0xA0B86991C6218B36C1D19D4A2E9EB0CE3606EB48".to_string()],
    );

    let result = search_erc20_transfers(
        input,
        Some(global_assets_repository()),
        TEST_MAX_TOKEN_FILTERS,
        &extractor,
    )
    .await
    .unwrap();

    assert_eq!(result.plan.resolved_token_filters.len(), 1);
    assert_eq!(
        result.plan.resolved_token_filters[0].contract_address,
        "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
    );
    assert_eq!(
        result.plan.resolved_token_filters[0].source,
        Erc20TransferTokenFilterSource::ContractAddress
    );
    assert_eq!(
        result.plan.resolved_token_filters[0].asset_slug.as_deref(),
        Some("usdc")
    );
    assert_eq!(
        result.plan.resolved_token_filters[0].symbol.as_deref(),
        Some("USDC")
    );
    assert_eq!(result.plan.resolved_token_filters[0].decimals, Some(6));
    assert_eq!(result.token_metadata.len(), 1);
    assert_eq!(result.token_metadata[0].asset_slug, "usdc");
}

#[tokio::test]
async fn unfiltered_known_row_token_gets_catalog_metadata() {
    let extractor = RecordingExtractor::ok();
    let input = transfer_search_input(
        "0xabc0000000000000000000000000000000000000",
        Vec::new(),
        Vec::new(),
    );

    let result = search_erc20_transfers(
        input,
        Some(global_assets_repository()),
        TEST_MAX_TOKEN_FILTERS,
        &extractor,
    )
    .await
    .unwrap();

    assert!(result.plan.resolved_token_filters.is_empty());
    assert_eq!(result.token_metadata.len(), 1);
    assert_eq!(
        result.token_metadata[0].contract_address,
        "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
    );
    assert_eq!(result.token_metadata[0].asset_slug, "usdc");
    assert_eq!(result.token_metadata[0].symbol, "USDC");
    assert_eq!(result.token_metadata[0].decimals, 6);
}

#[tokio::test]
async fn extractor_errors_map_to_pr4_search_errors() {
    for (extractor_error, expected) in [
        (
            Erc20TransferExtractionError::ExtractionUnavailable,
            Erc20TransferSearchError::ExtractionUnavailable,
        ),
        (
            Erc20TransferExtractionError::UpstreamProviderError,
            Erc20TransferSearchError::UpstreamProviderError,
        ),
        (
            Erc20TransferExtractionError::UpstreamProviderTimeout,
            Erc20TransferSearchError::UpstreamProviderTimeout,
        ),
    ] {
        let extractor = RecordingExtractor::err(extractor_error);
        let input = transfer_search_input(
            "0xabc0000000000000000000000000000000000000",
            Vec::new(),
            Vec::new(),
        );

        let error = search_erc20_transfers(input, None, TEST_MAX_TOKEN_FILTERS, &extractor)
            .await
            .unwrap_err();

        assert_same_search_error(error, expected);
    }
}

struct RecordingExtractor {
    requests: Mutex<Vec<Erc20TransferExtractionRequest>>,
    result: Result<Erc20TransferExtractionResult, Erc20TransferExtractionError>,
}

impl RecordingExtractor {
    fn ok() -> Self {
        Self {
            requests: Mutex::new(Vec::new()),
            result: Ok(Erc20TransferExtractionResult {
                truncated: false,
                rows: vec![Erc20TransferExtractionRow {
                    block_number: 18_600_001,
                    tx_hash: "0x0000000000000000000000000000000000000000000000000000000000000001"
                        .to_string(),
                    log_index: 12,
                    token: "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48".to_string(),
                    from: "0xabc0000000000000000000000000000000000000".to_string(),
                    to: "0x2222222222222222222222222222222222222222".to_string(),
                    value: "1000000".to_string(),
                }],
            }),
        }
    }

    fn err(error: Erc20TransferExtractionError) -> Self {
        Self {
            requests: Mutex::new(Vec::new()),
            result: Err(error),
        }
    }
}

impl Erc20TransferExtractor for RecordingExtractor {
    fn search_erc20_transfers(
        &self,
        request: Erc20TransferExtractionRequest,
    ) -> impl std::future::Future<
        Output = Result<Erc20TransferExtractionResult, Erc20TransferExtractionError>,
    > + Send {
        async move {
            self.requests.lock().unwrap().push(request);
            self.result.clone()
        }
    }
}

fn transfer_search_input(
    address: &str,
    asset_slugs: Vec<String>,
    contract_addresses: Vec<String>,
) -> Erc20TransferSearchInput {
    Erc20TransferSearchInput {
        network_slug: "eth-mainnet".to_string(),
        address: address.to_string(),
        direction: TransferDirection::Any,
        window: block_window(),
        asset_slugs,
        contract_addresses,
    }
}

fn block_window() -> OnchainWindow {
    OnchainWindow::Block(BlockWindow::new(18_600_000, 18_600_500).unwrap())
}

fn assert_same_search_error(actual: Erc20TransferSearchError, expected: Erc20TransferSearchError) {
    match (actual, expected) {
        (
            Erc20TransferSearchError::ExtractionUnavailable,
            Erc20TransferSearchError::ExtractionUnavailable,
        )
        | (
            Erc20TransferSearchError::UpstreamProviderError,
            Erc20TransferSearchError::UpstreamProviderError,
        )
        | (
            Erc20TransferSearchError::UpstreamProviderTimeout,
            Erc20TransferSearchError::UpstreamProviderTimeout,
        ) => {}
        (actual, expected) => panic!("expected {expected:?}, got {actual:?}"),
    }
}
