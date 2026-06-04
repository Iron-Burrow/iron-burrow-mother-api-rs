use serde::Serialize;
use tracing::{info, warn};

use crate::{
    metering::{AlphaPricingCatalog, BillingPayload, MeteredOperation, UsageQuote},
    price_indexer::{InternalLatestPrice, PriceIndexerClient, PriceLookupError, PriceSeriesPoint},
    repositories::global_assets::{GlobalAssetDetail, GlobalAssetRepository, RepositoryError},
    signals::{
        calculations::{
            calculate_stats, calculate_trend, CalculationError, DecimalAmount, PricePoint,
            PriceStats, TrendEvidence, TrendModel,
        },
        range::{parse_utc_rfc3339_seconds, RangeValidationError, SignalRange},
    },
};

const DEFAULT_CURRENCY: &str = "USD";
const DEFAULT_GRANULARITY: &str = "1h";
const PRICE_STATS_RECIPE: &str = "price_stats_v1";
const PRICE_TREND_RECIPE: &str = "price_trend_evidence_v1";
const SOURCE_SERVICE: &str = "price-indexer";

#[derive(Clone, Debug)]
pub struct PriceSignalService {
    repository: GlobalAssetRepository,
    price_indexer_client: Option<PriceIndexerClient>,
}

impl PriceSignalService {
    pub fn new(
        repository: GlobalAssetRepository,
        price_indexer_client: Option<PriceIndexerClient>,
    ) -> Self {
        Self {
            repository,
            price_indexer_client,
        }
    }

    pub async fn latest_price(
        &self,
        raw_slug: &str,
    ) -> Result<LatestPriceResponse, PriceSignalServiceError> {
        let asset = self.lookup_asset(raw_slug).await?;
        let client = self
            .price_indexer_client
            .as_ref()
            .ok_or(PriceSignalServiceError::PriceIndexerUnavailable)?;

        info!(
            asset_slug = asset.slug,
            "Internal price-indexer latest lookup attempted"
        );

        let latest = client
            .internal_latest_by_slug(&asset.slug, DEFAULT_CURRENCY)
            .await
            .map_err(|error| {
                log_price_indexer_error(&asset.slug, client, &error);
                PriceSignalServiceError::PriceIndexer(error)
            })?;
        let quote = AlphaPricingCatalog::quote_usd(MeteredOperation::PriceLatest, None);

        Ok(LatestPriceResponse {
            ok: true,
            response_type: "asset_price_latest",
            asset,
            price: LatestPricePayload::from(latest),
            billing: BillingPayload::from(quote),
        })
    }

    pub async fn price_stats(
        &self,
        raw_slug: &str,
        raw_query: Option<&str>,
    ) -> Result<PriceStatsResponse, PriceSignalServiceError> {
        let range = SignalRange::parse(raw_query).map_err(PriceSignalServiceError::InvalidRange)?;
        let asset = self.lookup_asset(raw_slug).await?;
        let points = self.fetch_points(&asset, &range).await?;

        if points.is_empty() {
            return Ok(PriceStatsResponse::new_insufficient(asset, range, 0));
        }

        let stats = calculate_stats(&points).ok_or(PriceSignalServiceError::CalculationFailed)?;
        let quote = AlphaPricingCatalog::quote_usd(
            MeteredOperation::SignalPriceStats,
            Some(range.range_days()),
        );

        Ok(PriceStatsResponse::new_found(
            asset,
            range,
            points.len(),
            stats,
            quote,
        ))
    }

    pub async fn price_trend(
        &self,
        raw_slug: &str,
        raw_query: Option<&str>,
    ) -> Result<PriceTrendResponse, PriceSignalServiceError> {
        let range = SignalRange::parse(raw_query).map_err(PriceSignalServiceError::InvalidRange)?;
        let asset = self.lookup_asset(raw_slug).await?;
        let points = self.fetch_points(&asset, &range).await?;
        let calculation = calculate_trend(&points);

        if calculation.evidence.agreement == "insufficient_data" {
            return Ok(PriceTrendResponse::new_insufficient(
                asset,
                range,
                points.len(),
            ));
        }

        let quote = AlphaPricingCatalog::quote_usd(
            MeteredOperation::SignalPriceTrend,
            Some(range.range_days()),
        );

        Ok(PriceTrendResponse::new_found(
            asset,
            range,
            points.len(),
            calculation.stats,
            calculation.models,
            calculation.evidence,
            quote,
        ))
    }

    async fn lookup_asset(
        &self,
        raw_slug: &str,
    ) -> Result<SignalAssetPayload, PriceSignalServiceError> {
        let slug = raw_slug.trim().to_ascii_lowercase();
        let detail = self
            .repository
            .get_asset_detail_by_slug(&slug)
            .await?
            .ok_or(PriceSignalServiceError::AssetNotFound)?;

        Ok(SignalAssetPayload::from(detail))
    }

    async fn fetch_points(
        &self,
        asset: &SignalAssetPayload,
        range: &SignalRange,
    ) -> Result<Vec<PricePoint>, PriceSignalServiceError> {
        let client = self
            .price_indexer_client
            .as_ref()
            .ok_or(PriceSignalServiceError::PriceIndexerUnavailable)?;

        info!(
            asset_slug = asset.slug,
            from = range.from(),
            to = range.to(),
            "Internal price-indexer series lookup attempted"
        );

        let series = client
            .internal_price_series(
                &asset.slug,
                DEFAULT_CURRENCY,
                range.from(),
                range.to(),
                DEFAULT_GRANULARITY,
            )
            .await
            .map_err(|error| {
                log_price_indexer_error(&asset.slug, client, &error);
                PriceSignalServiceError::PriceIndexer(error)
            })?;
        let mut points = series
            .points
            .into_iter()
            .map(PricePoint::try_from)
            .collect::<Result<Vec<_>, _>>()?;

        points.sort_by(|left, right| left.timestamp.cmp(&right.timestamp));

        Ok(points)
    }
}

#[derive(Debug)]
pub enum PriceSignalServiceError {
    InvalidRange(RangeValidationError),
    AssetNotFound,
    Repository(RepositoryError),
    PriceIndexerUnavailable,
    PriceIndexer(PriceLookupError),
    CalculationFailed,
    MalformedPricePoint,
}

impl From<RepositoryError> for PriceSignalServiceError {
    fn from(error: RepositoryError) -> Self {
        Self::Repository(error)
    }
}

impl From<CalculationError> for PriceSignalServiceError {
    fn from(_: CalculationError) -> Self {
        Self::MalformedPricePoint
    }
}

impl TryFrom<PriceSeriesPoint> for PricePoint {
    type Error = PriceSignalServiceError;

    fn try_from(point: PriceSeriesPoint) -> Result<Self, Self::Error> {
        Ok(Self {
            unix_seconds: parse_utc_rfc3339_seconds(&point.timestamp)
                .map_err(|_| PriceSignalServiceError::MalformedPricePoint)?,
            timestamp: point.timestamp,
            price: DecimalAmount::parse(&point.price)?,
            source: point.source,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SignalAssetPayload {
    slug: String,
    symbol: String,
}

impl From<GlobalAssetDetail> for SignalAssetPayload {
    fn from(detail: GlobalAssetDetail) -> Self {
        Self {
            slug: detail.asset.slug,
            symbol: detail.asset.symbol,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct LatestPriceResponse {
    ok: bool,
    #[serde(rename = "type")]
    response_type: &'static str,
    asset: SignalAssetPayload,
    price: LatestPricePayload,
    billing: BillingPayload,
}

#[derive(Debug, Serialize)]
struct LatestPricePayload {
    currency: String,
    value: String,
    published_at: String,
    source: String,
}

impl From<InternalLatestPrice> for LatestPricePayload {
    fn from(price: InternalLatestPrice) -> Self {
        Self {
            currency: price.currency,
            value: price.value,
            published_at: price.published_at,
            source: price.source,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PriceStatsResponse {
    ok: bool,
    #[serde(rename = "type")]
    response_type: &'static str,
    asset: SignalAssetPayload,
    signal: PriceStatsSignalPayload,
}

impl PriceStatsResponse {
    fn new_found(
        asset: SignalAssetPayload,
        range: SignalRange,
        observations: usize,
        stats: PriceStats,
        quote: UsageQuote,
    ) -> Self {
        Self {
            ok: true,
            response_type: "price_stats_signal",
            asset,
            signal: PriceStatsSignalPayload {
                signal_type: "price_stats",
                recipe: PRICE_STATS_RECIPE,
                status: "found",
                range: RangePayload::from(range),
                input: SignalInputPayload::new(observations),
                stats: Some(StatsPayload::from(stats)),
                billing: BillingPayload::from(quote),
                source: SignalSourcePayload::historical(),
            },
        }
    }

    fn new_insufficient(
        asset: SignalAssetPayload,
        range: SignalRange,
        observations: usize,
    ) -> Self {
        Self {
            ok: true,
            response_type: "price_stats_signal",
            asset,
            signal: PriceStatsSignalPayload {
                signal_type: "price_stats",
                recipe: PRICE_STATS_RECIPE,
                status: "insufficient_data",
                range: RangePayload::from(range),
                input: SignalInputPayload::new(observations),
                stats: None,
                billing: BillingPayload::from(UsageQuote::not_billable(
                    MeteredOperation::SignalPriceStats,
                    "insufficient_data",
                )),
                source: SignalSourcePayload::not_enough_price_points(),
            },
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PriceTrendResponse {
    ok: bool,
    #[serde(rename = "type")]
    response_type: &'static str,
    asset: SignalAssetPayload,
    signal: PriceTrendSignalPayload,
}

impl PriceTrendResponse {
    fn new_found(
        asset: SignalAssetPayload,
        range: SignalRange,
        observations: usize,
        stats: Option<PriceStats>,
        models: Vec<TrendModel>,
        evidence: TrendEvidence,
        quote: UsageQuote,
    ) -> Self {
        Self {
            ok: true,
            response_type: "price_trend_signal",
            asset,
            signal: PriceTrendSignalPayload {
                signal_type: "price_trend_evidence",
                recipe: PRICE_TREND_RECIPE,
                status: "found",
                range: RangePayload::from(range),
                input: SignalInputPayload::new(observations),
                stats: stats.map(StatsPayload::from),
                models: models.into_iter().map(TrendModelPayload::from).collect(),
                evidence: EvidencePayload::from(evidence),
                billing: BillingPayload::from(quote),
                source: SignalSourcePayload::historical(),
            },
        }
    }

    fn new_insufficient(
        asset: SignalAssetPayload,
        range: SignalRange,
        observations: usize,
    ) -> Self {
        Self {
            ok: true,
            response_type: "price_trend_signal",
            asset,
            signal: PriceTrendSignalPayload {
                signal_type: "price_trend_evidence",
                recipe: PRICE_TREND_RECIPE,
                status: "insufficient_data",
                range: RangePayload::from(range),
                input: SignalInputPayload::new(observations),
                stats: None,
                models: Vec::new(),
                evidence: EvidencePayload::from(TrendEvidence::insufficient_data()),
                billing: BillingPayload::from(UsageQuote::not_billable(
                    MeteredOperation::SignalPriceTrend,
                    "insufficient_data",
                )),
                source: SignalSourcePayload::not_enough_price_points(),
            },
        }
    }
}

#[derive(Debug, Serialize)]
struct PriceStatsSignalPayload {
    #[serde(rename = "type")]
    signal_type: &'static str,
    recipe: &'static str,
    status: &'static str,
    range: RangePayload,
    input: SignalInputPayload,
    stats: Option<StatsPayload>,
    billing: BillingPayload,
    source: SignalSourcePayload,
}

#[derive(Debug, Serialize)]
struct PriceTrendSignalPayload {
    #[serde(rename = "type")]
    signal_type: &'static str,
    recipe: &'static str,
    status: &'static str,
    range: RangePayload,
    input: SignalInputPayload,
    stats: Option<StatsPayload>,
    models: Vec<TrendModelPayload>,
    evidence: EvidencePayload,
    billing: BillingPayload,
    source: SignalSourcePayload,
}

#[derive(Debug, Serialize)]
struct RangePayload {
    mode: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    window: Option<&'static str>,
    from: String,
    to: String,
}

impl From<SignalRange> for RangePayload {
    fn from(range: SignalRange) -> Self {
        Self {
            mode: range.mode(),
            window: range.window_value(),
            from: range.from().to_string(),
            to: range.to().to_string(),
        }
    }
}

#[derive(Debug, Serialize)]
struct SignalInputPayload {
    currency: &'static str,
    granularity: &'static str,
    observations: usize,
    source_service: &'static str,
}

impl SignalInputPayload {
    fn new(observations: usize) -> Self {
        Self {
            currency: DEFAULT_CURRENCY,
            granularity: DEFAULT_GRANULARITY,
            observations,
            source_service: SOURCE_SERVICE,
        }
    }
}

#[derive(Debug, Serialize)]
struct StatsPayload {
    first_price: String,
    last_price: String,
    min_price: String,
    max_price: String,
    avg_price: String,
    change_abs: String,
    change_pct: String,
    observations: usize,
}

impl From<PriceStats> for StatsPayload {
    fn from(stats: PriceStats) -> Self {
        Self {
            first_price: stats.first_price,
            last_price: stats.last_price,
            min_price: stats.min_price,
            max_price: stats.max_price,
            avg_price: stats.avg_price,
            change_abs: stats.change_abs,
            change_pct: stats.change_pct,
            observations: stats.observations,
        }
    }
}

#[derive(Debug, Serialize)]
struct TrendModelPayload {
    name: &'static str,
    transform: &'static str,
    status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    direction: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    slope_per_day: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    slope_index_points_per_day: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    approx_pct_change_per_day: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    r_squared: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<&'static str>,
}

impl From<TrendModel> for TrendModelPayload {
    fn from(model: TrendModel) -> Self {
        Self {
            name: model.name,
            transform: model.transform,
            status: model.status,
            direction: model.direction,
            slope_per_day: model.slope_per_day,
            slope_index_points_per_day: model.slope_index_points_per_day,
            approx_pct_change_per_day: model.approx_pct_change_per_day,
            r_squared: model.r_squared,
            reason: model.reason,
        }
    }
}

#[derive(Debug, Serialize)]
struct EvidencePayload {
    positive_models: usize,
    negative_models: usize,
    flat_models: usize,
    skipped_models: usize,
    total_models: usize,
    agreement: &'static str,
}

impl From<TrendEvidence> for EvidencePayload {
    fn from(evidence: TrendEvidence) -> Self {
        Self {
            positive_models: evidence.positive_models,
            negative_models: evidence.negative_models,
            flat_models: evidence.flat_models,
            skipped_models: evidence.skipped_models,
            total_models: evidence.total_models,
            agreement: evidence.agreement,
        }
    }
}

#[derive(Debug, Serialize)]
struct SignalSourcePayload {
    service: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    freshness: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<&'static str>,
}

impl SignalSourcePayload {
    fn historical() -> Self {
        Self {
            service: SOURCE_SERVICE,
            freshness: Some("historical"),
            reason: None,
        }
    }

    fn not_enough_price_points() -> Self {
        Self {
            service: SOURCE_SERVICE,
            freshness: None,
            reason: Some("not_enough_price_points"),
        }
    }
}

fn log_price_indexer_error(slug: &str, client: &PriceIndexerClient, error: &PriceLookupError) {
    warn!(
        asset_slug = slug,
        price_indexer_host = client.base_host(),
        ?error,
        "Internal price-indexer lookup failed for signal endpoint"
    );
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::*;
    use crate::{
        repositories::global_assets::{demo_assets, GlobalAssetRepository},
        signals::range::SignalRange,
    };

    fn asset() -> SignalAssetPayload {
        SignalAssetPayload {
            slug: "ethereum".to_string(),
            symbol: "ETH".to_string(),
        }
    }

    fn range() -> SignalRange {
        SignalRange::parse_with_now(Some("window=7d"), 1_780_066_800).unwrap()
    }

    #[test]
    fn stats_found_response_includes_stable_contract_fields() {
        let response = PriceStatsResponse::new_found(
            asset(),
            range(),
            2,
            PriceStats {
                first_price: "100.000000".to_string(),
                last_price: "110.000000".to_string(),
                min_price: "100.000000".to_string(),
                max_price: "110.000000".to_string(),
                avg_price: "105.000000".to_string(),
                change_abs: "10.000000".to_string(),
                change_pct: "10.000000".to_string(),
                observations: 2,
            },
            AlphaPricingCatalog::quote_usd(MeteredOperation::SignalPriceStats, Some(7)),
        );
        let json: Value = serde_json::to_value(response).unwrap();

        assert_eq!(json["ok"], true);
        assert_eq!(json["asset"]["slug"], "ethereum");
        assert_eq!(json["signal"]["recipe"], PRICE_STATS_RECIPE);
        assert_eq!(json["signal"]["source"]["service"], SOURCE_SERVICE);
        assert_eq!(json["signal"]["billing"]["amount"], "0.000500");
        assert_eq!(json["signal"]["stats"]["observations"], 2);
    }

    #[test]
    fn trend_insufficient_response_is_not_billable() {
        let response = PriceTrendResponse::new_insufficient(asset(), range(), 1);
        let json: Value = serde_json::to_value(response).unwrap();

        assert_eq!(json["signal"]["status"], "insufficient_data");
        assert_eq!(json["signal"]["stats"], serde_json::Value::Null);
        assert_eq!(json["signal"]["models"].as_array().unwrap().len(), 0);
        assert_eq!(json["signal"]["billing"]["billable"], false);
        assert_eq!(
            json["signal"]["source"]["reason"],
            "not_enough_price_points"
        );
    }

    #[tokio::test]
    async fn unknown_asset_uses_existing_asset_behavior() {
        let service =
            PriceSignalService::new(GlobalAssetRepository::in_memory(demo_assets()), None);
        let error = service.latest_price("missing").await.unwrap_err();

        assert!(matches!(error, PriceSignalServiceError::AssetNotFound));
    }
}
