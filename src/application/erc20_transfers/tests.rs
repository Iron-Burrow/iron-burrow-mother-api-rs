use crate::{
    application::{
        erc20_transfers::service::{
            build_command, Erc20TransferCommandTokenFilters, Erc20TransferSearchCommand,
            Erc20TransferSearchError, Erc20TransferSearchInput, Erc20TransferSearchTokenFilters,
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
async fn command_resolves_usdc_and_dedupes_duplicate_explicit_address() {
    let input = transfer_search_input(
        "0xABC0000000000000000000000000000000000000",
        vec!["usdc".to_string()],
        vec!["0xA0B86991C6218B36C1D19D4A2E9EB0CE3606EB48".to_string()],
    );

    let command = build_command(
        input,
        Some(global_assets_repository()),
        TEST_MAX_TOKEN_FILTERS,
    )
    .await
    .unwrap();

    assert_eq!(
        command,
        Erc20TransferSearchCommand {
            network_slug: "eth-mainnet".to_string(),
            address: "0xabc0000000000000000000000000000000000000".to_string(),
            direction: TransferDirection::Any,
            tokens: Erc20TransferCommandTokenFilters {
                contract_addresses: vec!["0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48".to_string()],
            },
            window: block_window(),
        }
    );
}

#[tokio::test]
async fn command_enforces_final_token_filter_limit_after_dedupe() {
    let input = transfer_search_input(
        "0xabc0000000000000000000000000000000000000",
        vec!["usdc".to_string()],
        vec!["0xA0B86991C6218B36C1D19D4A2E9EB0CE3606EB48".to_string()],
    );
    let command = build_command(input, Some(global_assets_repository()), 1)
        .await
        .unwrap();
    assert_eq!(
        command.tokens.contract_addresses,
        ["0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"]
    );

    let input = transfer_search_input(
        "0xabc0000000000000000000000000000000000000",
        vec!["usdc".to_string()],
        vec!["0x1111111111111111111111111111111111111111".to_string()],
    );
    let error = build_command(input, Some(global_assets_repository()), 1)
        .await
        .unwrap_err();

    assert!(matches!(
        error,
        Erc20TransferSearchError::TooManyTokenFilters
    ));
}

#[tokio::test]
async fn command_token_filters_have_no_asset_slug_field() {
    let input = transfer_search_input(
        "0xabc0000000000000000000000000000000000000",
        vec!["usdc".to_string()],
        vec!["0x1111111111111111111111111111111111111111".to_string()],
    );
    let command = build_command(
        input,
        Some(global_assets_repository()),
        TEST_MAX_TOKEN_FILTERS,
    )
    .await
    .unwrap();
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

fn transfer_search_input(
    address: &str,
    asset_slugs: Vec<String>,
    contract_addresses: Vec<String>,
) -> Erc20TransferSearchInput {
    Erc20TransferSearchInput {
        network_slug: "eth-mainnet".to_string(),
        address: address.to_string(),
        direction: TransferDirection::Any,
        tokens: Erc20TransferSearchTokenFilters {
            asset_slugs,
            contract_addresses,
        },
        window: block_window(),
    }
}

fn block_window() -> OnchainWindow {
    OnchainWindow::Block(BlockWindow::new(18_600_000, 18_600_500).unwrap())
}
