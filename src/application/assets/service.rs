use std::collections::HashMap;

use serde::Serialize;
use tracing::{info, warn};

use crate::{
    adapters::postgres::global_assets::{
        AssetChainMap, GlobalAsset, GlobalAssetDetail, GlobalAssetRepository, RepositoryError,
    },
    adapters::price_indexer::{
        LatestAssetPrice, PriceIndexerClient, PriceLookupError, PriceSignalError,
        PriceSignalRequest, PriceStatus,
    },
};

const DEFAULT_LIMIT: u64 = 100;
const MAX_LIMIT: u64 = 1000;

#[derive(Clone, Debug)]
pub struct AssetEnrichmentQuery {
    pub include: Vec<AssetEnrichmentInclude>,
    pub params: Option<AssetEnrichmentParams>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AssetEnrichmentInclude {
    PriceStats,
    PriceTrend,
    PriceSeries,
}

impl AssetEnrichmentInclude {
    fn source(self) -> &'static str {
        match self {
            Self::PriceStats => "price_stats",
            Self::PriceTrend => "price_trend",
            Self::PriceSeries => "price_series",
        }
    }
}

#[derive(Clone, Debug)]
pub struct AssetEnrichmentParams {
    pub slug: String,
    pub quote_currency: String,
    pub window: String,
    pub granularity: Option<String>,
}

impl From<AssetEnrichmentParams> for PriceSignalRequest {
    fn from(params: AssetEnrichmentParams) -> Self {
        Self {
            slug: params.slug,
            quote_currency: params.quote_currency,
            window: params.window,
            granularity: params.granularity,
        }
    }
}

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
        let prices = self.lookup_list_prices(&assets).await;

        Ok(AssetsResponse::new(limit, assets, prices))
    }

    pub async fn get_asset(
        &self,
        raw_slug: &str,
        quote_currency: &str,
        enrichment_query: Option<AssetEnrichmentQuery>,
    ) -> Result<AssetResponse, AssetsServiceError> {
        let slug = raw_slug.trim().to_ascii_lowercase();
        let detail = self
            .repository
            .get_asset_detail_by_slug(&slug)
            .await?
            .ok_or(AssetsServiceError::AssetNotFound)?;
        let price = self
            .lookup_price(&slug, &detail.asset.symbol, quote_currency)
            .await;
        let enrichments = self.lookup_enrichments(enrichment_query, &slug).await;

        Ok(AssetResponse::new(detail, price, enrichments))
    }

    async fn lookup_price(
        &self,
        slug: &str,
        symbol: &str,
        quote_currency: &str,
    ) -> LatestAssetPrice {
        let Some(client) = &self.price_indexer_client else {
            return LatestAssetPrice::unavailable();
        };

        info!(
            asset_slug = slug,
            symbol, quote_currency, "Price lookup attempted for asset detail"
        );

        match client.latest_by_slug(slug, quote_currency).await {
            Ok(price) => {
                info!(
                    asset_slug = slug,
                    symbol,
                    quote_currency,
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

    async fn lookup_list_prices(
        &self,
        assets: &[GlobalAsset],
    ) -> HashMap<String, LatestAssetPrice> {
        let Some(client) = &self.price_indexer_client else {
            return HashMap::new();
        };

        let slugs = assets
            .iter()
            .map(|asset| asset.slug.clone())
            .collect::<Vec<_>>();

        if slugs.is_empty() {
            return HashMap::new();
        }

        info!(
            asset_count = slugs.len(),
            "Batch price lookup attempted for asset list"
        );

        let prices = client.latest_by_slugs(&slugs, "USD").await;
        let available_count = prices
            .values()
            .filter(|price| price.status != PriceStatus::Unavailable)
            .count();

        info!(
            asset_count = slugs.len(),
            available_count, "Batch price lookup completed for asset list"
        );

        prices
    }

    async fn lookup_enrichments(
        &self,
        enrichment_query: Option<AssetEnrichmentQuery>,
        slug: &str,
    ) -> Option<AssetEnrichments> {
        let enrichment_query = enrichment_query?;
        let mut enrichments = AssetEnrichments::from_include(&enrichment_query.include);

        let Some(mut params) = enrichment_query.params else {
            log_invalid_enrichment_request(slug, &enrichment_query.include);
            enrichments.fail_all_requested(EnrichmentErrorCode::InvalidRequest);
            return Some(enrichments);
        };
        params.slug = slug.to_string();

        let Some(client) = &self.price_indexer_client else {
            log_disabled_enrichment_request(&params, &enrichment_query.include);
            enrichments.fail_all_requested(EnrichmentErrorCode::PriceIndexerUnavailable);
            return Some(enrichments);
        };

        let request = PriceSignalRequest::from(params);

        for include in enrichment_query.include {
            let result = match include {
                AssetEnrichmentInclude::PriceStats => client.price_stats_raw(&request).await,
                AssetEnrichmentInclude::PriceTrend => client.price_trend_raw(&request).await,
                AssetEnrichmentInclude::PriceSeries => client.price_series_raw(&request).await,
            };

            match result {
                Ok(signal) => enrichments.set_signal(include, Some(signal)),
                Err(error) => {
                    log_enrichment_error(client, &request, include, &error);
                    enrichments.fail(include, EnrichmentErrorCode::from(error));
                }
            }
        }

        Some(enrichments)
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
    asset_network_maps: Vec<AssetNetworkMapPayload>,
    #[serde(skip_serializing_if = "Option::is_none")]
    signals: Option<AssetSignals>,
    #[serde(skip_serializing_if = "Option::is_none")]
    enrichment_errors: Option<Vec<EnrichmentErrorPayload>>,
}

impl AssetResponse {
    fn new(
        detail: GlobalAssetDetail,
        price: LatestAssetPrice,
        enrichments: Option<AssetEnrichments>,
    ) -> Self {
        let (signals, enrichment_errors) = enrichments
            .map(|enrichments| {
                (
                    Some(enrichments.signals),
                    Some(enrichments.enrichment_errors),
                )
            })
            .unwrap_or((None, None));

        Self {
            ok: true,
            response_type: "asset",
            asset: AssetPayload::from(detail.asset),
            price,
            asset_network_maps: detail
                .chain_maps
                .into_iter()
                .map(AssetNetworkMapPayload::from)
                .collect(),
            signals,
            enrichment_errors,
        }
    }
}

#[derive(Debug)]
struct AssetEnrichments {
    signals: AssetSignals,
    enrichment_errors: Vec<EnrichmentErrorPayload>,
}

impl AssetEnrichments {
    fn from_include(include: &[AssetEnrichmentInclude]) -> Self {
        let mut signals = AssetSignals::default();

        for include in include {
            signals.set_requested(*include);
        }

        Self {
            signals,
            enrichment_errors: Vec::new(),
        }
    }

    fn set_signal(&mut self, include: AssetEnrichmentInclude, signal: Option<serde_json::Value>) {
        match include {
            AssetEnrichmentInclude::PriceStats => self.signals.price_stats = Some(signal),
            AssetEnrichmentInclude::PriceTrend => self.signals.price_trend = Some(signal),
            AssetEnrichmentInclude::PriceSeries => self.signals.price_series = Some(signal),
        }
    }

    fn fail(&mut self, include: AssetEnrichmentInclude, code: EnrichmentErrorCode) {
        self.set_signal(include, None);
        self.enrichment_errors
            .push(EnrichmentErrorPayload::new(include.source(), code));
    }

    fn fail_all_requested(&mut self, code: EnrichmentErrorCode) {
        for include in self.signals.requested() {
            self.fail(include, code);
        }
    }
}

#[derive(Debug, Default, Serialize)]
struct AssetSignals {
    #[serde(skip_serializing_if = "Option::is_none")]
    price_stats: Option<Option<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    price_trend: Option<Option<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    price_series: Option<Option<serde_json::Value>>,
}

impl AssetSignals {
    fn set_requested(&mut self, include: AssetEnrichmentInclude) {
        match include {
            AssetEnrichmentInclude::PriceStats => {
                if self.price_stats.is_none() {
                    self.price_stats = Some(None);
                }
            }
            AssetEnrichmentInclude::PriceTrend => {
                if self.price_trend.is_none() {
                    self.price_trend = Some(None);
                }
            }
            AssetEnrichmentInclude::PriceSeries => {
                if self.price_series.is_none() {
                    self.price_series = Some(None);
                }
            }
        }
    }

    fn requested(&self) -> Vec<AssetEnrichmentInclude> {
        let mut requested = Vec::new();

        if self.price_stats.is_some() {
            requested.push(AssetEnrichmentInclude::PriceStats);
        }
        if self.price_trend.is_some() {
            requested.push(AssetEnrichmentInclude::PriceTrend);
        }
        if self.price_series.is_some() {
            requested.push(AssetEnrichmentInclude::PriceSeries);
        }

        requested
    }
}

#[derive(Clone, Copy, Debug)]
enum EnrichmentErrorCode {
    InvalidRequest,
    SignalNotAvailable,
    UpstreamAuthFailed,
    PriceIndexerError,
    PriceIndexerUnavailable,
    UpstreamInvalidResponse,
}

impl EnrichmentErrorCode {
    fn as_str(self) -> &'static str {
        match self {
            Self::InvalidRequest => "invalid_request",
            Self::SignalNotAvailable => "signal_not_available",
            Self::UpstreamAuthFailed => "upstream_auth_failed",
            Self::PriceIndexerError => "price_indexer_error",
            Self::PriceIndexerUnavailable => "price_indexer_unavailable",
            Self::UpstreamInvalidResponse => "upstream_invalid_response",
        }
    }

    fn message(self, source: &str) -> &'static str {
        match (source, self) {
            ("price_stats", Self::InvalidRequest) => "Price stats request parameters are invalid.",
            ("price_trend", Self::InvalidRequest) => "Price trend request parameters are invalid.",
            ("price_series", Self::InvalidRequest) => {
                "Price series request parameters are invalid."
            }
            ("price_stats", Self::SignalNotAvailable) => "Price stats are not available.",
            ("price_trend", Self::SignalNotAvailable) => "Price trend is not available.",
            ("price_series", Self::SignalNotAvailable) => "Price series is not available.",
            ("price_stats", Self::UpstreamAuthFailed) => "Price stats are temporarily unavailable.",
            ("price_trend", Self::UpstreamAuthFailed) => "Price trend is temporarily unavailable.",
            ("price_series", Self::UpstreamAuthFailed) => {
                "Price series is temporarily unavailable."
            }
            ("price_stats", Self::PriceIndexerError) => "Price stats are temporarily unavailable.",
            ("price_trend", Self::PriceIndexerError) => "Price trend is temporarily unavailable.",
            ("price_series", Self::PriceIndexerError) => "Price series is temporarily unavailable.",
            ("price_stats", Self::PriceIndexerUnavailable) => {
                "Price stats are temporarily unavailable."
            }
            ("price_trend", Self::PriceIndexerUnavailable) => {
                "Price trend is temporarily unavailable."
            }
            ("price_series", Self::PriceIndexerUnavailable) => {
                "Price series is temporarily unavailable."
            }
            ("price_stats", Self::UpstreamInvalidResponse) => {
                "Price stats are temporarily unavailable."
            }
            ("price_trend", Self::UpstreamInvalidResponse) => {
                "Price trend is temporarily unavailable."
            }
            ("price_series", Self::UpstreamInvalidResponse) => {
                "Price series is temporarily unavailable."
            }
            _ => "Price enrichment is temporarily unavailable.",
        }
    }
}

impl From<PriceSignalError> for EnrichmentErrorCode {
    fn from(error: PriceSignalError) -> Self {
        match error {
            PriceSignalError::InvalidRequest => Self::InvalidRequest,
            PriceSignalError::NotFound => Self::SignalNotAvailable,
            PriceSignalError::Unauthorized => Self::UpstreamAuthFailed,
            PriceSignalError::UpstreamInternal => Self::PriceIndexerError,
            PriceSignalError::Timeout | PriceSignalError::Transport => {
                Self::PriceIndexerUnavailable
            }
            PriceSignalError::MalformedResponse => Self::UpstreamInvalidResponse,
        }
    }
}

#[derive(Debug, Serialize)]
struct EnrichmentErrorPayload {
    source: &'static str,
    code: &'static str,
    message: &'static str,
}

impl EnrichmentErrorPayload {
    fn new(source: &'static str, code: EnrichmentErrorCode) -> Self {
        Self {
            source,
            code: code.as_str(),
            message: code.message(source),
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
    assets: Vec<AssetListItemPayload>,
}

impl AssetsResponse {
    fn new(
        limit: u64,
        assets: Vec<GlobalAsset>,
        prices: HashMap<String, LatestAssetPrice>,
    ) -> Self {
        let assets = assets
            .into_iter()
            .map(|asset| {
                let normalized_slug = asset.slug.trim().to_ascii_lowercase();
                let price = prices
                    .get(&normalized_slug)
                    .cloned()
                    .unwrap_or_else(LatestAssetPrice::unavailable);
                AssetListItemPayload::new(asset, price)
            })
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
struct AssetListItemPayload {
    asset_id: String,
    symbol: String,
    name: String,
    category: String,
    canonical_path: String,
    price: LatestAssetPrice,
}

impl AssetListItemPayload {
    fn new(asset: GlobalAsset, price: LatestAssetPrice) -> Self {
        Self {
            asset_id: asset.slug,
            symbol: asset.symbol,
            name: asset.name,
            category: asset.category,
            canonical_path: asset.canonical_path,
            price,
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
struct AssetNetworkMapPayload {
    network_slug: String,
    network_name: String,
    caip2: Option<String>,
    is_native: bool,
    address: Option<String>,
}

impl From<AssetChainMap> for AssetNetworkMapPayload {
    fn from(chain_map: AssetChainMap) -> Self {
        let network = chain_map.network;

        Self {
            network_slug: network.slug,
            network_name: network.name,
            caip2: network.caip2,
            is_native: chain_map.is_native,
            address: chain_map.address,
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
        PriceLookupError::InvalidSlug => {
            warn!(
                asset_slug = slug,
                symbol, "Price lookup skipped because asset slug was invalid"
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

fn log_enrichment_error(
    client: &PriceIndexerClient,
    request: &PriceSignalRequest,
    include: AssetEnrichmentInclude,
    error: &PriceSignalError,
) {
    warn!(
        ?error,
        source = include.source(),
        asset_slug = request.slug.as_str(),
        quote_currency = request.quote_currency.as_str(),
        window = request.window.as_str(),
        granularity = request.granularity.as_deref(),
        price_indexer_host = client.base_host(),
        timeout_ms = client.timeout_ms(),
        "Asset detail enrichment lookup failed"
    );
}

fn log_invalid_enrichment_request(slug: &str, include: &[AssetEnrichmentInclude]) {
    warn!(
        asset_slug = slug,
        requested_sources = ?enrichment_sources(include),
        "Asset detail enrichment request parameters are invalid"
    );
}

fn log_disabled_enrichment_request(
    params: &AssetEnrichmentParams,
    include: &[AssetEnrichmentInclude],
) {
    warn!(
        asset_slug = params.slug.as_str(),
        requested_sources = ?enrichment_sources(include),
        quote_currency = params.quote_currency.as_str(),
        window = params.window.as_str(),
        granularity = params.granularity.as_deref(),
        "Asset detail enrichment unavailable because price-indexer client is disabled"
    );
}

fn enrichment_sources(include: &[AssetEnrichmentInclude]) -> Vec<&'static str> {
    include.iter().map(|include| include.source()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::postgres::global_assets::{
        demo_assets, GlobalAsset, GlobalAssetRepository,
    };

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
        assert_eq!(json["assets"][0]["price"]["status"], "unavailable");
        assert!(json["assets"][0]["price"]["price"].is_null());
    }

    #[tokio::test]
    async fn honors_custom_limit() {
        let response = service().list_assets(Some("2")).await.unwrap();
        let json = serde_json::to_value(response).unwrap();

        assert_eq!(json["limit"], 2);
        assert_eq!(json["count"], 2);
        assert_eq!(json["assets"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn asset_list_prices_match_normalized_slugs_without_consuming_entries() {
        let price = LatestAssetPrice {
            status: PriceStatus::Available,
            price: Some("2500.123456".to_string()),
            quote_currency: Some("USD".to_string()),
            source_type: Some("chainlink".to_string()),
            confidence_label: Some("high".to_string()),
            is_fallback: false,
            is_derived: false,
            recorded_at: Some("2026-05-20T12:00:01.000Z".to_string()),
            warning: None,
        };
        let prices = HashMap::from([("ethereum".to_string(), price)]);

        let response = AssetsResponse::new(
            2,
            vec![
                test_asset(" Ethereum ", "ETH", 10),
                test_asset("ethereum", "ETH2", 20),
            ],
            prices,
        );
        let json = serde_json::to_value(response).unwrap();

        assert_eq!(json["assets"][0]["asset_id"], " Ethereum ");
        assert_eq!(json["assets"][0]["price"]["status"], "available");
        assert_eq!(json["assets"][0]["price"]["price"], "2500.123456");
        assert_eq!(json["assets"][1]["asset_id"], "ethereum");
        assert_eq!(json["assets"][1]["price"]["status"], "available");
        assert_eq!(json["assets"][1]["price"]["price"], "2500.123456");
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
        let response = service().get_asset("bitcoin", "USD", None).await.unwrap();
        let json = serde_json::to_value(response).unwrap();

        assert_eq!(json["ok"], true);
        assert_eq!(json["type"], "asset");
        assert_eq!(json["asset"]["asset_id"], "bitcoin");
        assert_eq!(json["asset"]["canonical_path"], "/assets/bitcoin");
        assert_eq!(json["price"]["status"], "unavailable");
        assert!(json["price"]["price"].is_null());
        assert!(json.get("chain_maps").is_none());
        assert_eq!(
            json["asset_network_maps"][0]["network_slug"],
            "bitcoin-mainnet"
        );
        assert_eq!(json["asset_network_maps"][0]["is_native"], true);
        assert!(json["asset_network_maps"][0]["address"].is_null());
    }

    #[tokio::test]
    async fn reports_unknown_asset_detail() {
        let error = service()
            .get_asset("does-not-exist", "USD", None)
            .await
            .unwrap_err();

        assert!(matches!(error, AssetsServiceError::AssetNotFound));
    }

    fn test_asset(slug: &str, symbol: &str, sort_order: i32) -> GlobalAsset {
        GlobalAsset {
            id: format!("test-{slug}"),
            slug: slug.to_string(),
            symbol: symbol.to_string(),
            name: symbol.to_string(),
            category: "crypto".to_string(),
            canonical_path: format!("/assets/{}", slug.trim().to_ascii_lowercase()),
            aliases: Vec::new(),
            sort_order,
        }
    }
}
