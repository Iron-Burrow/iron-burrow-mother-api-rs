use std::sync::Arc;

use serde::Serialize;

use crate::domain::canonical_registry::{CanonicalAsset, CanonicalAssetMatch, CanonicalRegistry};

use super::query::NormalizedQuery;

const UNKNOWN_MESSAGE: &str =
    "Iron Burrow does not know this query publicly yet. Showing related recommendations instead.";

#[derive(Clone, Debug)]
pub struct ResolveService {
    registry: Arc<CanonicalRegistry>,
}

impl ResolveService {
    pub fn new(registry: Arc<CanonicalRegistry>) -> Self {
        Self { registry }
    }

    pub fn resolve(&self, query: NormalizedQuery) -> ResolveResponse {
        if let Some(asset_match) = self.registry.find_confident_asset(&query.normalized) {
            return ResolveResponse::resolved(query, asset_match);
        }

        let recommendations = self
            .registry
            .recommendations(&query.normalized, 3)
            .into_iter()
            .map(Recommendation::from)
            .collect();

        ResolveResponse::unknown(query, recommendations)
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
    fn resolved(query: NormalizedQuery, asset_match: CanonicalAssetMatch<'_>) -> Self {
        Self {
            ok: true,
            response_type: "resolve",
            resolved: true,
            query: QueryPayload::from(query),
            result: ResolveResult::Asset {
                resource_url: asset_resource_url(asset_match.asset),
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
        resource_url: String,
        canonical_path: String,
        confidence: &'static str,
        asset: AssetPayload,
    },
    Unknown {
        message: &'static str,
        recommendations: Vec<Recommendation>,
    },
}

fn asset_resource_url(asset: &CanonicalAsset) -> String {
    format!("/v1/assets/{}", asset.slug)
}

#[derive(Serialize)]
struct AssetPayload {
    asset_id: String,
    symbol: String,
    name: String,
    category: String,
}

impl From<&CanonicalAsset> for AssetPayload {
    fn from(asset: &CanonicalAsset) -> Self {
        Self {
            asset_id: asset.slug.clone(),
            symbol: asset.symbol.clone(),
            name: asset.name.clone(),
            category: asset
                .category
                .clone()
                .unwrap_or_else(|| asset.asset_kind.clone()),
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

impl From<&CanonicalAsset> for Recommendation {
    fn from(asset: &CanonicalAsset) -> Self {
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
        application::assets::resolve::query::parse_query,
        test_utils::fixtures::registry::embedded_canonical_registry,
    };

    fn service() -> ResolveService {
        ResolveService::new(embedded_canonical_registry())
    }

    #[test]
    fn resolves_usdc_alias() {
        let response = service().resolve(parse_query(Some("usdc coin usd")).unwrap());
        let json = serde_json::to_value(response).unwrap();

        assert_eq!(json["resolved"], true);
        assert_eq!(json["result"]["canonical_path"], "/assets/usdc");
        assert_eq!(json["result"]["resource_url"], "/v1/assets/usdc");
        assert_eq!(json["result"]["confidence"], "alias_exact");
    }

    #[test]
    fn resolves_gold_aliases() {
        for query in ["oro de ley", "oro", "gold", "xau"] {
            let response = service().resolve(parse_query(Some(query)).unwrap());
            let json = serde_json::to_value(response).unwrap();

            assert_eq!(json["resolved"], true);
            assert_eq!(json["result"]["canonical_path"], "/assets/gold");
        }
    }

    #[test]
    fn resolves_wrapped_bitcoin_aliases() {
        for query in ["wbtc", "wrapped bitcoin", "wrapped btc"] {
            let response = service().resolve(parse_query(Some(query)).unwrap());
            let json = serde_json::to_value(response).unwrap();

            assert_eq!(json["resolved"], true);
            assert_eq!(json["result"]["canonical_path"], "/assets/wrapped-bitcoin");
        }
    }

    #[test]
    fn leaves_network_only_aliases_unresolved() {
        for query in ["base", "base mainnet", "coinbase base"] {
            let response = service().resolve(parse_query(Some(query)).unwrap());
            let json = serde_json::to_value(response).unwrap();

            assert_eq!(json["resolved"], false);
            assert_eq!(json["result"]["kind"], "unknown");
        }
    }

    #[test]
    fn returns_unknown_with_recommendations() {
        let response = service().resolve(parse_query(Some("some unknown thing")).unwrap());
        let json = serde_json::to_value(response).unwrap();

        assert_eq!(json["resolved"], false);
        assert_eq!(json["result"]["kind"], "unknown");
        assert!(json["result"]["resource_url"].is_null());
        assert!(!json["result"]["recommendations"]
            .as_array()
            .unwrap()
            .is_empty());
    }
}
