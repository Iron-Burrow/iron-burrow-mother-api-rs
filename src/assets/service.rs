use serde::Serialize;
use tracing::{info, warn};

use crate::{
    price_indexer::{LatestAssetPrice, PriceIndexerClient, PriceLookupError},
    repositories::global_assets::{
        AssetChainMap, GlobalAsset, GlobalAssetDetail, GlobalAssetRepository, NetworkRef,
        RepositoryError,
    },
};

const DEFAULT_LIMIT: u64 = 100;
const MAX_LIMIT: u64 = 1000;

#[derive(Clone, Debug)]
pub struct AssetsService {
    repository: GlobalAssetRepository,
    price_indexer_client: Option<PriceIndexerClient>,
}

impl AssetsService {
    pub fn new(
        repository: GlobalAssetRepository,
        price_indexer_client: Option<PriceIndexerClient>,
    ) -> Self {
        Self {
            repository,
            price_indexer_client,
        }
    }

    pub async fn list_assets(
        &self,
        raw_limit: Option<&str>,
    ) -> Result<AssetsResponse, AssetsServiceError> {
        let limit = parse_limit(raw_limit)?;
        let assets = self.repository.list_assets(limit).await?;

        Ok(AssetsResponse::new(limit, assets))
    }

    pub async fn get_asset(&self, raw_slug: &str) -> Result<AssetResponse, AssetsServiceError> {
        let slug = raw_slug.trim().to_ascii_lowercase();
        let detail = self
            .repository
            .get_asset_detail_by_slug(&slug)
            .await?
            .ok_or(AssetsServiceError::AssetNotFound)?;
        let price = self.lookup_price(&slug, &detail.asset.symbol).await;

        Ok(AssetResponse::new(detail, price))
    }

    async fn lookup_price(&self, slug: &str, symbol: &str) -> LatestAssetPrice {
        let Some(client) = &self.price_indexer_client else {
            return LatestAssetPrice::unavailable();
        };

        info!(
            asset_slug = slug,
            symbol, "Price lookup attempted for asset detail"
        );

        match client.latest_by_symbol(symbol).await {
            Ok(price) => {
                info!(
                    asset_slug = slug,
                    symbol,
                    status = price.status.as_str(),
                    source_type = price.source_type.as_deref(),
                    is_fallback = price.is_fallback,
                    is_derived = price.is_derived,
                    "Price lookup succeeded for asset detail"
                );
                price
            }
            Err(error) => {
                log_price_lookup_error(slug, symbol, client, &error);
                LatestAssetPrice::unavailable()
            }
        }
    }
}

#[derive(Debug)]
pub enum AssetsServiceError {
    InvalidLimit,
    AssetNotFound,
    Repository(RepositoryError),
}

impl From<RepositoryError> for AssetsServiceError {
    fn from(error: RepositoryError) -> Self {
        Self::Repository(error)
    }
}

#[derive(Debug, Serialize)]
pub struct AssetResponse {
    ok: bool,
    #[serde(rename = "type")]
    response_type: &'static str,
    asset: AssetPayload,
    price: LatestAssetPrice,
    chain_maps: Vec<ChainMapPayload>,
}

impl AssetResponse {
    fn new(detail: GlobalAssetDetail, price: LatestAssetPrice) -> Self {
        Self {
            ok: true,
            response_type: "asset",
            asset: AssetPayload::from(detail.asset),
            price,
            chain_maps: detail
                .chain_maps
                .into_iter()
                .map(ChainMapPayload::from)
                .collect(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct AssetsResponse {
    ok: bool,
    #[serde(rename = "type")]
    response_type: &'static str,
    limit: u64,
    count: usize,
    assets: Vec<AssetPayload>,
}

impl AssetsResponse {
    fn new(limit: u64, assets: Vec<GlobalAsset>) -> Self {
        let assets = assets
            .into_iter()
            .map(AssetPayload::from)
            .collect::<Vec<_>>();

        Self {
            ok: true,
            response_type: "assets",
            limit,
            count: assets.len(),
            assets,
        }
    }
}

#[derive(Debug, Serialize)]
struct AssetPayload {
    asset_id: String,
    symbol: String,
    name: String,
    category: String,
    canonical_path: String,
}

impl From<GlobalAsset> for AssetPayload {
    fn from(asset: GlobalAsset) -> Self {
        Self {
            asset_id: asset.slug,
            symbol: asset.symbol,
            name: asset.name,
            category: asset.category,
            canonical_path: asset.canonical_path,
        }
    }
}

#[derive(Debug, Serialize)]
struct ChainMapPayload {
    network: NetworkPayload,
    is_native: bool,
    address: Option<String>,
}

impl From<AssetChainMap> for ChainMapPayload {
    fn from(chain_map: AssetChainMap) -> Self {
        Self {
            network: NetworkPayload::from(chain_map.network),
            is_native: chain_map.is_native,
            address: chain_map.address,
        }
    }
}

#[derive(Debug, Serialize)]
struct NetworkPayload {
    slug: String,
    name: String,
    caip2: Option<String>,
}

impl From<NetworkRef> for NetworkPayload {
    fn from(network: NetworkRef) -> Self {
        Self {
            slug: network.slug,
            name: network.name,
            caip2: network.caip2,
        }
    }
}

fn parse_limit(raw_limit: Option<&str>) -> Result<u64, AssetsServiceError> {
    let Some(raw_limit) = raw_limit else {
        return Ok(DEFAULT_LIMIT);
    };

    let limit = raw_limit
        .trim()
        .parse::<u64>()
        .map_err(|_| AssetsServiceError::InvalidLimit)?;

    if limit == 0 {
        return Err(AssetsServiceError::InvalidLimit);
    }

    Ok(limit.min(MAX_LIMIT))
}

fn log_price_lookup_error(
    slug: &str,
    symbol: &str,
    client: &PriceIndexerClient,
    error: &PriceLookupError,
) {
    match error {
        PriceLookupError::Disabled => {
            warn!(
                asset_slug = slug,
                symbol, "Price lookup disabled for asset detail"
            );
        }
        PriceLookupError::InvalidSymbol => {
            warn!(
                asset_slug = slug,
                symbol, "Price lookup skipped because asset symbol was invalid"
            );
        }
        PriceLookupError::Unavailable { status, code } => {
            warn!(
                asset_slug = slug,
                symbol,
                http_status = status,
                error_code = code.as_deref(),
                "Price lookup unavailable for asset detail"
            );
        }
        PriceLookupError::Unauthorized => {
            warn!(
                asset_slug = slug,
                symbol,
                price_indexer_host = client.base_host(),
                "Price lookup unauthorized for asset detail"
            );
        }
        PriceLookupError::Timeout => {
            warn!(
                asset_slug = slug,
                symbol,
                timeout_ms = client.timeout_ms(),
                "Price lookup timed out for asset detail"
            );
        }
        PriceLookupError::Transport => {
            warn!(
                asset_slug = slug,
                symbol, "Price lookup transport failure for asset detail"
            );
        }
        PriceLookupError::MalformedResponse => {
            warn!(
                asset_slug = slug,
                symbol, "Price lookup returned malformed response for asset detail"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repositories::global_assets::{demo_assets, GlobalAssetRepository};

    fn service() -> AssetsService {
        AssetsService::new(GlobalAssetRepository::in_memory(demo_assets()), None)
    }

    #[tokio::test]
    async fn defaults_limit_to_100() {
        let response = service().list_assets(None).await.unwrap();
        let json = serde_json::to_value(response).unwrap();

        assert_eq!(json["ok"], true);
        assert_eq!(json["type"], "assets");
        assert_eq!(json["limit"], 100);
        assert_eq!(json["count"], 21);
    }

    #[tokio::test]
    async fn honors_custom_limit() {
        let response = service().list_assets(Some("2")).await.unwrap();
        let json = serde_json::to_value(response).unwrap();

        assert_eq!(json["limit"], 2);
        assert_eq!(json["count"], 2);
        assert_eq!(json["assets"].as_array().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn clamps_limit_above_maximum() {
        let response = service().list_assets(Some("9999")).await.unwrap();
        let json = serde_json::to_value(response).unwrap();

        assert_eq!(json["limit"], 1000);
    }

    #[tokio::test]
    async fn rejects_invalid_limits() {
        for limit in ["0", "-1", "abc", ""] {
            let error = service().list_assets(Some(limit)).await.unwrap_err();

            assert!(matches!(error, AssetsServiceError::InvalidLimit));
        }
    }

    #[tokio::test]
    async fn returns_asset_detail() {
        let response = service().get_asset("bitcoin").await.unwrap();
        let json = serde_json::to_value(response).unwrap();

        assert_eq!(json["ok"], true);
        assert_eq!(json["type"], "asset");
        assert_eq!(json["asset"]["asset_id"], "bitcoin");
        assert_eq!(json["asset"]["canonical_path"], "/assets/bitcoin");
        assert_eq!(json["price"]["status"], "unavailable");
        assert!(json["price"]["price"].is_null());
        assert_eq!(json["chain_maps"][0]["network"]["slug"], "bitcoin-mainnet");
        assert_eq!(json["chain_maps"][0]["is_native"], true);
        assert!(json["chain_maps"][0]["address"].is_null());
    }

    #[tokio::test]
    async fn reports_unknown_asset_detail() {
        let error = service().get_asset("does-not-exist").await.unwrap_err();

        assert!(matches!(error, AssetsServiceError::AssetNotFound));
    }
}
