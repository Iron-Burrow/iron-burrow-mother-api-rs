use sqlx::FromRow;

use crate::domain::assets::asset_chain_map::AssetChainMap;
use crate::domain::assets::global_assets::GlobalAsset;
use crate::domain::networks::NetworkRef;

#[derive(FromRow)]
pub(super) struct AssetChainMapRow {
    network_slug: String,
    network_name: String,
    network_caip2: Option<String>,
    network_family: String,
    network_chain_id: Option<i64>,
    is_native: bool,
    address: Option<String>,
    decimals: Option<i32>,
    token_standard: Option<String>,
}

#[derive(Clone, Debug)]
pub(super) struct InMemoryAssetChainMap {
    pub(super) asset_slug: String,
    pub(super) chain_map: AssetChainMap,
    pub(super) sort_order: i32,
}

pub(super) fn map_chain_map_row(row: AssetChainMapRow) -> AssetChainMap {
    AssetChainMap {
        network: NetworkRef {
            slug: row.network_slug,
            name: row.network_name,
            caip2: row.network_caip2,
            family: row.network_family,
            chain_id: row.network_chain_id,
        },
        is_native: row.is_native,
        address: row.address,
        decimals: row.decimals,
        token_standard: row.token_standard,
    }
}

pub(super) fn in_memory_chain_map(
    asset_slug: &str,
    network_slug: &str,
    network_name: &str,
    caip2: Option<&str>,
    is_native: bool,
    address: Option<&str>,
    sort_order: i32,
) -> InMemoryAssetChainMap {
    let (family, chain_id) = match caip2 {
        Some(value) if value.starts_with("eip155:") => (
            "evm",
            value
                .strip_prefix("eip155:")
                .and_then(|chain_id| chain_id.parse::<i64>().ok()),
        ),
        Some(value) if value.starts_with("bip122:") => ("bitcoin", None),
        Some(value) if value.starts_with("near:") => ("near", None),
        _ => ("unknown", None),
    };
    let decimals = match asset_slug {
        "bitcoin" => 8,
        "usdc" => 6,
        _ => 18,
    };
    let token_standard = if is_native {
        "native"
    } else if family == "evm" {
        "erc20"
    } else {
        "nep141"
    };

    InMemoryAssetChainMap {
        asset_slug: asset_slug.to_string(),
        chain_map: AssetChainMap {
            network: NetworkRef {
                slug: network_slug.to_string(),
                name: network_name.to_string(),
                caip2: caip2.map(str::to_string),
                family: family.to_string(),
                chain_id,
            },
            is_native,
            address: address.map(str::to_string),
            decimals: Some(decimals),
            token_standard: Some(token_standard.to_string()),
        },
        sort_order,
    }
}

pub(super) fn demo_chain_maps_for_assets(assets: &[GlobalAsset]) -> Vec<InMemoryAssetChainMap> {
    let mut chain_maps = Vec::new();

    for asset in assets {
        match asset.slug.as_str() {
            "bitcoin" => chain_maps.push(in_memory_chain_map(
                "bitcoin",
                "bitcoin-mainnet",
                "Bitcoin Mainnet",
                Some("bip122:000000000019d6689c085ae165831e93"),
                true,
                None,
                10,
            )),
            "ethereum" => chain_maps.extend([
                in_memory_chain_map(
                    "ethereum",
                    "eth-mainnet",
                    "Ethereum Mainnet",
                    Some("eip155:1"),
                    true,
                    None,
                    20,
                ),
                in_memory_chain_map(
                    "ethereum",
                    "arbitrum-mainnet",
                    "Arbitrum One",
                    Some("eip155:42161"),
                    true,
                    None,
                    30,
                ),
                in_memory_chain_map(
                    "ethereum",
                    "base-mainnet",
                    "Base",
                    Some("eip155:8453"),
                    true,
                    None,
                    40,
                ),
            ]),
            "usdc" => chain_maps.extend([
                in_memory_chain_map(
                    "usdc",
                    "eth-mainnet",
                    "Ethereum Mainnet",
                    Some("eip155:1"),
                    false,
                    Some("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"),
                    240,
                ),
                in_memory_chain_map(
                    "usdc",
                    "arbitrum-mainnet",
                    "Arbitrum One",
                    Some("eip155:42161"),
                    false,
                    Some("0xaf88d065e77c8cc2239327c5edb3a432268e5831"),
                    250,
                ),
                in_memory_chain_map(
                    "usdc",
                    "base-mainnet",
                    "Base",
                    Some("eip155:8453"),
                    false,
                    Some("0x833589fcd6edb6e08f4c7c32d4f71b54bda02913"),
                    260,
                ),
                in_memory_chain_map(
                    "usdc",
                    "near",
                    "NEAR Mainnet",
                    Some("near:mainnet"),
                    false,
                    Some("17208628f84f5d6ad33f0da3bbbeb27ffcb398eac501a31bd6ad2011e36133a1"),
                    270,
                ),
                in_memory_chain_map(
                    "usdc",
                    "mantle-mainnet",
                    "Mantle",
                    Some("eip155:5000"),
                    false,
                    Some("0x09bc4e0d864854c6afb6eb9a9cdf58ac190d0df9"),
                    280,
                ),
            ]),
            "mantle" => chain_maps.push(in_memory_chain_map(
                "mantle",
                "mantle-mainnet",
                "Mantle",
                Some("eip155:5000"),
                true,
                None,
                50,
            )),
            _ => {}
        }
    }

    chain_maps
}
