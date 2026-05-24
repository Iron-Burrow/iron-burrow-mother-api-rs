use serde::Serialize;

use crate::repositories::global_assets::{
    AssetMatch, GlobalAsset, GlobalAssetRepository, RepositoryError,
};

use super::query::NormalizedQuery;

const UNKNOWN_MESSAGE: &str =
    "Iron Burrow does not know this query publicly yet. Showing related recommendations instead.";

#[derive(Clone, Debug)]
pub struct ResolveService {
    repository: GlobalAssetRepository,
}

impl ResolveService {
    pub fn new(repository: GlobalAssetRepository) -> Self {
        Self { repository }
    }

    pub async fn resolve(
        &self,
        query: NormalizedQuery,
    ) -> Result<ResolveResponse, RepositoryError> {
        if let Some(asset_match) = self
            .repository
            .find_confident_match(&query.normalized)
            .await?
        {
            return Ok(ResolveResponse::resolved(query, asset_match));
        }

        let recommendations = self
            .repository
            .list_recommendations(&query.normalized, 3)
            .await?
            .into_iter()
            .map(Recommendation::from)
            .collect();

        Ok(ResolveResponse::unknown(query, recommendations))
    }
}

#[derive(Serialize)]
pub struct ResolveResponse {
    ok: bool,
    #[serde(rename = "type")]
    response_type: &'static str,
    resolved: bool,
    query: QueryPayload,
    result: ResolveResult,
}

impl ResolveResponse {
    fn resolved(query: NormalizedQuery, asset_match: AssetMatch) -> Self {
        Self {
            ok: true,
            response_type: "resolve",
            resolved: true,
            query: QueryPayload::from(query),
            result: ResolveResult::Asset {
                canonical_path: asset_match.asset.canonical_path.clone(),
                confidence: asset_match.confidence.as_str(),
                asset: AssetPayload::from(asset_match.asset),
            },
        }
    }

    fn unknown(query: NormalizedQuery, recommendations: Vec<Recommendation>) -> Self {
        Self {
            ok: true,
            response_type: "resolve",
            resolved: false,
            query: QueryPayload::from(query),
            result: ResolveResult::Unknown {
                message: UNKNOWN_MESSAGE,
                recommendations,
            },
        }
    }
}

#[derive(Serialize)]
struct QueryPayload {
    raw: String,
    normalized: String,
}

impl From<NormalizedQuery> for QueryPayload {
    fn from(query: NormalizedQuery) -> Self {
        Self {
            raw: query.raw,
            normalized: query.normalized,
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum ResolveResult {
    Asset {
        canonical_path: String,
        confidence: &'static str,
        asset: AssetPayload,
    },
    Unknown {
        message: &'static str,
        recommendations: Vec<Recommendation>,
    },
}

#[derive(Serialize)]
struct AssetPayload {
    asset_id: String,
    symbol: String,
    name: String,
    category: String,
}

impl From<GlobalAsset> for AssetPayload {
    fn from(asset: GlobalAsset) -> Self {
        Self {
            asset_id: asset.slug,
            symbol: asset.symbol,
            name: asset.name,
            category: asset.category,
        }
    }
}

#[derive(Serialize)]
struct Recommendation {
    kind: &'static str,
    canonical_path: String,
    asset: AssetPayload,
    reason: &'static str,
}

impl From<GlobalAsset> for Recommendation {
    fn from(asset: GlobalAsset) -> Self {
        Self {
            kind: "asset",
            canonical_path: asset.canonical_path.clone(),
            asset: AssetPayload::from(asset),
            reason: "related_public_asset",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        repositories::global_assets::{demo_assets, GlobalAssetRepository},
        resolve::query::parse_query,
    };

    fn service() -> ResolveService {
        ResolveService::new(GlobalAssetRepository::in_memory(demo_assets()))
    }

    #[tokio::test]
    async fn resolves_usdc_alias() {
        let response = service()
            .resolve(parse_query(Some("usdc coin usd")).unwrap())
            .await
            .unwrap();
        let json = serde_json::to_value(response).unwrap();

        assert_eq!(json["resolved"], true);
        assert_eq!(json["result"]["canonical_path"], "/assets/usdc");
        assert_eq!(json["result"]["confidence"], "alias_exact");
    }

    #[tokio::test]
    async fn resolves_gold_aliases() {
        for query in ["oro de ley", "oro", "gold", "xau"] {
            let response = service()
                .resolve(parse_query(Some(query)).unwrap())
                .await
                .unwrap();
            let json = serde_json::to_value(response).unwrap();

            assert_eq!(json["resolved"], true);
            assert_eq!(json["result"]["canonical_path"], "/assets/gold");
        }
    }

    #[tokio::test]
    async fn returns_unknown_with_recommendations() {
        let response = service()
            .resolve(parse_query(Some("some unknown thing")).unwrap())
            .await
            .unwrap();
        let json = serde_json::to_value(response).unwrap();

        assert_eq!(json["resolved"], false);
        assert_eq!(json["result"]["kind"], "unknown");
        assert!(json["result"]["recommendations"].as_array().unwrap().len() > 0);
    }
}
