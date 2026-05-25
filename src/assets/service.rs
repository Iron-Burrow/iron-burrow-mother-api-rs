use serde::Serialize;

use crate::repositories::global_assets::{GlobalAsset, GlobalAssetRepository, RepositoryError};

const DEFAULT_LIMIT: u64 = 100;
const MAX_LIMIT: u64 = 1000;

#[derive(Clone, Debug)]
pub struct AssetsService {
    repository: GlobalAssetRepository,
}

impl AssetsService {
    pub fn new(repository: GlobalAssetRepository) -> Self {
        Self { repository }
    }

    pub async fn list_assets(
        &self,
        raw_limit: Option<&str>,
    ) -> Result<AssetsResponse, AssetsServiceError> {
        let limit = parse_limit(raw_limit)?;
        let assets = self.repository.list_assets(limit).await?;

        Ok(AssetsResponse::new(limit, assets))
    }
}

#[derive(Debug)]
pub enum AssetsServiceError {
    InvalidLimit,
    Repository(RepositoryError),
}

impl From<RepositoryError> for AssetsServiceError {
    fn from(error: RepositoryError) -> Self {
        Self::Repository(error)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repositories::global_assets::{demo_assets, GlobalAssetRepository};

    fn service() -> AssetsService {
        AssetsService::new(GlobalAssetRepository::in_memory(demo_assets()))
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
}
