//! Deterministic, evidence-carrying portfolio simulation for the private Lab.
//! This is intentionally small: strategy definitions are compiled here and
//! historical sources are normalized before any strategy sees them.

use std::collections::HashMap;

use fastnum::{decimal::Context, D512};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::adapters::{
    aave_v3::AaveV3RealizedYieldAdapter,
    bigwig::BigwigClient,
    postgres::DefiProtocolRepository,
    price_indexer::{HistoricalPricePoint, PriceIndexerClient, PriceSignalError},
};

const REQUEST_SCHEMA_VERSION: u32 = 1;
const ENGINE_VERSION: &str = "v1";
const QUOTE_CURRENCY: &str = "USD";
const MAX_DAYS: i64 = 366;

#[derive(Clone, Debug)]
pub(crate) struct Command {
    pub(crate) initial_capital: String,
    pub(crate) quote_currency: String,
    pub(crate) start_date: String,
    pub(crate) end_date: String,
    pub(crate) strategy_slug: String,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct CompletedRun {
    pub(crate) outcome: String,
    pub(crate) strategy_slug: String,
    pub(crate) strategy_version: String,
    pub(crate) engine_version: &'static str,
    pub(crate) input: Value,
    pub(crate) evidence: Value,
    pub(crate) result: Value,
    pub(crate) evidence_digest: String,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error("initial capital must be a positive decimal")]
    InvalidInitialCapital,
    #[error("only USD quote currency is supported")]
    UnsupportedQuoteCurrency,
    #[error("date range is invalid or exceeds 366 days")]
    InvalidDateRange,
    #[error("strategy is unsupported")]
    UnsupportedStrategy,
    #[error("historical price evidence is unavailable")]
    HistoricalPricesUnavailable,
    #[error("historical price evidence was malformed")]
    HistoricalPricesMalformed,
    #[error("Aave historical evidence is unavailable")]
    AaveEvidenceUnavailable,
}

#[derive(Clone, Debug)]
pub(crate) struct Service {
    prices: Option<PriceIndexerClient>,
    protocols: Option<DefiProtocolRepository>,
    bigwig: Option<BigwigClient>,
    aave: AaveV3RealizedYieldAdapter,
}

impl Service {
    pub(crate) fn new(
        prices: Option<PriceIndexerClient>,
        protocols: Option<DefiProtocolRepository>,
        bigwig: Option<BigwigClient>,
    ) -> Self {
        Self {
            prices,
            protocols,
            bigwig,
            aave: AaveV3RealizedYieldAdapter,
        }
    }

    pub(crate) async fn run(&self, command: Command) -> Result<CompletedRun, Error> {
        let initial_capital = parse_positive_decimal(&command.initial_capital)?;
        if command.quote_currency.trim().to_ascii_uppercase() != QUOTE_CURRENCY {
            return Err(Error::UnsupportedQuoteCurrency);
        }
        let window = SimulationWindow::parse(&command.start_date, &command.end_date)?;
        let strategy = Strategy::parse(&command.strategy_slug)?;
        let asset_slug = strategy.price_asset();
        let prices = self.load_prices(asset_slug, &window).await?;

        let input = json!({
            "schema_version": REQUEST_SCHEMA_VERSION,
            "initial_capital": canonical(&initial_capital),
            "quote_currency": QUOTE_CURRENCY,
            "start_date": window.start_date,
            "end_date": window.end_date,
            "strategy_slug": strategy.slug(),
        });

        let mut evidence = json!({
            "schema_version": 1,
            "price_series": {
                "asset_slug": asset_slug,
                "quote_currency": QUOTE_CURRENCY,
                "from": window.start_timestamp(),
                "to": window.end_timestamp(),
                "points": prices,
            },
        });

        let factors = match strategy {
            Strategy::AaveUsdcSupply => {
                let observations = self.load_aave_factors(&window).await?;
                evidence["aave_income_indexes"] =
                    serde_json::to_value(&observations).expect("simulation evidence serializes");
                Some(observations)
            }
            _ => None,
        };

        let result = simulate(&window, strategy, initial_capital, prices, factors);
        let outcome = result
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("failed")
            .to_string();
        let digest_input = json!({"input": input, "evidence": evidence, "result": result});
        let evidence_digest = hex::encode(Sha256::digest(canonical_json(&digest_input).as_bytes()));
        Ok(CompletedRun {
            outcome,
            strategy_slug: strategy.slug().to_string(),
            strategy_version: strategy.version().to_string(),
            engine_version: ENGINE_VERSION,
            input,
            evidence,
            result,
            evidence_digest,
        })
    }

    /// Retains a sanitized failed outcome for a request whose user-controlled
    /// inputs were valid but whose required evidence could not be obtained.
    /// Invalid form input is deliberately not persisted.
    pub(crate) fn failed_run(&self, command: &Command, error: &Error) -> Option<CompletedRun> {
        if !matches!(
            error,
            Error::HistoricalPricesUnavailable
                | Error::HistoricalPricesMalformed
                | Error::AaveEvidenceUnavailable
        ) {
            return None;
        }
        let initial_capital = parse_positive_decimal(&command.initial_capital).ok()?;
        let window = SimulationWindow::parse(&command.start_date, &command.end_date).ok()?;
        let strategy = Strategy::parse(&command.strategy_slug).ok()?;
        if command.quote_currency.trim().to_ascii_uppercase() != QUOTE_CURRENCY {
            return None;
        }
        let input = json!({
            "schema_version": REQUEST_SCHEMA_VERSION,
            "initial_capital": canonical(&initial_capital),
            "quote_currency": QUOTE_CURRENCY,
            "start_date": window.start_date,
            "end_date": window.end_date,
            "strategy_slug": strategy.slug(),
        });
        let reason = match error {
            Error::HistoricalPricesUnavailable => "historical_price_source_unavailable",
            Error::HistoricalPricesMalformed => "historical_price_source_malformed",
            Error::AaveEvidenceUnavailable => "aave_evidence_source_unavailable",
            _ => return None,
        };
        let evidence = json!({"schema_version": 1, "status": "failed", "reason": reason});
        let result = json!({
            "status": "failed",
            "reason": reason,
            "strategy": {"slug": strategy.slug(), "version": strategy.version()},
            "metrics": Value::Null,
            "operations": [],
            "snapshots": [],
            "limitations": ["required historical evidence was unavailable; no performance result was calculated"]
        });
        let digest_input = json!({"input": input, "evidence": evidence, "result": result});
        Some(CompletedRun {
            outcome: "failed".to_string(),
            strategy_slug: strategy.slug().to_string(),
            strategy_version: strategy.version().to_string(),
            engine_version: ENGINE_VERSION,
            input,
            evidence,
            result,
            evidence_digest: hex::encode(Sha256::digest(canonical_json(&digest_input).as_bytes())),
        })
    }

    async fn load_prices(
        &self,
        asset_slug: &str,
        window: &SimulationWindow,
    ) -> Result<Vec<PriceObservation>, Error> {
        let client = self
            .prices
            .as_ref()
            .ok_or(Error::HistoricalPricesUnavailable)?;
        let series = client
            .historical_price_series(
                asset_slug,
                QUOTE_CURRENCY,
                &window.start_timestamp(),
                &window.end_timestamp(),
            )
            .await
            .map_err(map_price_error)?;
        if series.quote_currency.to_ascii_uppercase() != QUOTE_CURRENCY
            || series.granularity != "1d"
        {
            return Err(Error::HistoricalPricesMalformed);
        }
        if series.from != window.start_timestamp() || series.to != window.end_timestamp() {
            return Err(Error::HistoricalPricesMalformed);
        }
        let mut by_timestamp = HashMap::new();
        for point in series.points {
            if by_timestamp
                .insert(point.bucket_start.clone(), point)
                .is_some()
            {
                return Err(Error::HistoricalPricesMalformed);
            }
        }
        Ok(window
            .timestamps()
            .into_iter()
            .map(|timestamp| {
                PriceObservation::from_point(timestamp.clone(), by_timestamp.get(&timestamp))
            })
            .collect())
    }

    async fn load_aave_factors(
        &self,
        window: &SimulationWindow,
    ) -> Result<Vec<AaveIndexObservation>, Error> {
        let (Some(protocols), Some(bigwig)) = (&self.protocols, &self.bigwig) else {
            return Err(Error::AaveEvidenceUnavailable);
        };
        let protocol = protocols
            .load_realized_yield_protocol("aave-v3")
            .await
            .map_err(|_| Error::AaveEvidenceUnavailable)?
            .filter(|protocol| protocol.adapter_kind == "aave_v3_realized_yield")
            .ok_or(Error::AaveEvidenceUnavailable)?;
        let mut resolver = BlockResolver::new(bigwig);
        let mut observations = Vec::new();
        for timestamp in window.timestamps() {
            let block = resolver.at_or_before(&timestamp).await?;
            let index = self
                .aave
                .income_index_at(&protocol, "usdc", block.number, bigwig)
                .await
                .map_err(|_| Error::AaveEvidenceUnavailable)?;
            observations.push(AaveIndexObservation {
                timestamp,
                block_number: block.number.to_string(),
                block_hash: block.hash,
                block_timestamp: block.timestamp,
                income_index: index.income_index,
                asset_symbol: index.asset_symbol,
                underlying_asset_address: index.underlying_asset_address,
            });
        }
        Ok(observations)
    }
}

fn map_price_error(error: PriceSignalError) -> Error {
    match error {
        PriceSignalError::MalformedResponse => Error::HistoricalPricesMalformed,
        _ => Error::HistoricalPricesUnavailable,
    }
}

#[derive(Clone, Copy, Debug)]
enum Strategy {
    BtcHold,
    EthHold,
    AaveUsdcSupply,
}

impl Strategy {
    fn parse(value: &str) -> Result<Self, Error> {
        match value.trim().to_ascii_lowercase().as_str() {
            "btc-hold" => Ok(Self::BtcHold),
            "eth-hold" => Ok(Self::EthHold),
            "aave-usdc-supply" => Ok(Self::AaveUsdcSupply),
            _ => Err(Error::UnsupportedStrategy),
        }
    }
    fn slug(self) -> &'static str {
        match self {
            Self::BtcHold => "btc-hold",
            Self::EthHold => "eth-hold",
            Self::AaveUsdcSupply => "aave-usdc-supply",
        }
    }
    fn version(self) -> &'static str {
        "v1"
    }
    fn price_asset(self) -> &'static str {
        match self {
            Self::BtcHold => "bitcoin",
            Self::EthHold => "ethereum",
            Self::AaveUsdcSupply => "usdc",
        }
    }
    fn position_kind(self) -> &'static str {
        match self {
            Self::BtcHold | Self::EthHold => "spot_asset",
            Self::AaveUsdcSupply => "aave_usdc_supply",
        }
    }
}

#[derive(Clone, Debug, Serialize)]
struct PriceObservation {
    timestamp: String,
    price: Option<String>,
    status: String,
    source_published_at: Option<String>,
    source_type: Option<String>,
    is_derived: Option<bool>,
    derivation_path: Option<Vec<String>>,
}

impl PriceObservation {
    fn from_point(timestamp: String, point: Option<&HistoricalPricePoint>) -> Self {
        match point {
            Some(point) => Self {
                timestamp,
                price: point.price.clone(),
                status: point.status.clone(),
                source_published_at: point.source_published_at.clone(),
                source_type: point.source_type.clone(),
                is_derived: point.is_derived,
                derivation_path: point.derivation_path.clone(),
            },
            None => Self {
                timestamp,
                price: None,
                status: "missing".to_string(),
                source_published_at: None,
                source_type: None,
                is_derived: None,
                derivation_path: None,
            },
        }
    }
}

#[derive(Clone, Debug, Serialize)]
struct AaveIndexObservation {
    timestamp: String,
    block_number: String,
    block_hash: String,
    block_timestamp: String,
    income_index: String,
    asset_symbol: String,
    underlying_asset_address: String,
}

fn simulate(
    window: &SimulationWindow,
    strategy: Strategy,
    initial: D512,
    prices: Vec<PriceObservation>,
    indexes: Option<Vec<AaveIndexObservation>>,
) -> Value {
    let start_price = prices
        .first()
        .and_then(|point| point.price.as_deref())
        .and_then(decimal);
    let final_price = prices
        .last()
        .and_then(|point| point.price.as_deref())
        .and_then(decimal);
    if start_price.is_none() || final_price.is_none() {
        return json!({
            "status": "unsupported",
            "reason": "required_start_or_final_price_evidence_unavailable",
            "strategy": {"slug": strategy.slug(), "version": strategy.version()},
            "metrics": Value::Null,
            "snapshots": []
        });
    }
    let start_price = start_price.expect("checked");
    let indexes = indexes.unwrap_or_default();
    let start_index = indexes
        .first()
        .and_then(|index| decimal(&index.income_index));
    if matches!(strategy, Strategy::AaveUsdcSupply) && start_index.is_none() {
        return json!({"status":"unsupported", "reason":"required_aave_index_evidence_unavailable", "metrics": Value::Null, "snapshots": []});
    }
    let mut partial = false;
    let mut snapshots = Vec::new();
    let mut values = Vec::new();
    for (offset, point) in prices.iter().enumerate() {
        let value = point.price.as_deref().and_then(decimal).and_then(|price| {
            let mut value = initial * price / start_price;
            if matches!(strategy, Strategy::AaveUsdcSupply) {
                let factor = indexes
                    .get(offset)
                    .and_then(|index| decimal(&index.income_index))?;
                value = value * factor / start_index.expect("checked");
            }
            Some(value)
        });
        if value.is_none() {
            partial = true;
        }
        if point.status.contains("carry") || point.status == "missing" {
            partial = true;
        }
        values.push(value);
        snapshots.push(json!({
            "timestamp": point.timestamp,
            "portfolio": {"position_kind": strategy.position_kind(), "asset_slug": strategy.price_asset()},
            "value": value.as_ref().map(canonical),
            "quote_currency": QUOTE_CURRENCY,
            "price_status": point.status,
            "income_index": indexes.get(offset).map(|index| index.income_index.clone()),
        }));
    }
    let final_value = values
        .last()
        .and_then(Clone::clone)
        .expect("final price checked");
    let absolute = final_value - initial;
    let percent = absolute / initial;
    let max_drawdown = (!partial).then(|| max_drawdown(&values));
    let annualized_return = (!partial && window.days >= 30)
        .then(|| annualized_return(&final_value, &initial, window.days));
    let price_component = match strategy {
        Strategy::AaveUsdcSupply => {
            let principal_at_final_price = initial * final_price.expect("checked") / start_price;
            let gross = final_value - initial;
            let yield_component = final_value - principal_at_final_price;
            json!({"price_appreciation": canonical(&(principal_at_final_price - initial)), "yield": canonical(&yield_component), "gross_total": canonical(&gross)})
        }
        _ => {
            json!({"price_appreciation": canonical(&absolute), "yield": "0", "gross_total": canonical(&absolute)})
        }
    };
    let operations = match strategy {
        Strategy::AaveUsdcSupply => vec![
            json!({"timestamp": window.start_timestamp(), "operation_type":"buy", "from_position":"usd_cash", "to_position":"usdc", "amount": canonical(&initial), "fees":"unmodeled", "reason":"aave-usdc-supply@v1 initial allocation"}),
            json!({"timestamp": window.start_timestamp(), "operation_type":"deposit", "from_position":"usdc", "to_position":"aave_usdc_supply", "amount": canonical(&initial), "fees":"unmodeled", "reason":"aave-usdc-supply@v1 initial allocation"}),
        ],
        _ => vec![
            json!({"timestamp": window.start_timestamp(), "operation_type":"buy", "from_position":"usd_cash", "to_position": strategy.price_asset(), "amount": canonical(&initial), "fees":"unmodeled", "reason": format!("{}@v1 initial allocation", strategy.slug())}),
        ],
    };
    json!({
        "status": if partial { "partial" } else { "complete" },
        "strategy": {"slug": strategy.slug(), "version": strategy.version()},
        "initial_portfolio": {"value": canonical(&initial), "quote_currency": QUOTE_CURRENCY},
        "final_portfolio": {"value": canonical(&final_value), "quote_currency": QUOTE_CURRENCY},
        "metrics": {
            "initial_value": canonical(&initial), "final_value": canonical(&final_value),
            "absolute_return": canonical(&absolute), "percentage_return": canonical(&percent),
            "annualized_return": annualized_return.map(|value| canonical(&value)),
            "maximum_drawdown": max_drawdown.map(|value| canonical(&value)),
            "components": {"capital_gains_and_price_appreciation": price_component, "staking_rewards":"unsupported", "protocol_rewards":"unsupported", "fees":"not_separately_attributable", "transaction_costs":"unmodeled"}
        },
        "operations": operations,
        "snapshots": snapshots,
        "limitations": ["gross simulated return; transaction costs, gas, separately attributable protocol fees, and reward tokens are not modeled"]
    })
}

fn max_drawdown(values: &[Option<D512>]) -> D512 {
    let mut peak = values[0].clone().expect("complete series");
    let mut drawdown = D512::from_str("0", Context::default()).expect("zero is valid");
    for value in values.iter().flatten() {
        if *value > peak {
            peak = value.clone();
        }
        let current = (value.clone() - peak.clone()) / peak.clone();
        if current < drawdown {
            drawdown = current;
        }
    }
    drawdown
}

fn annualized_return(final_value: &D512, initial: &D512, days: i64) -> D512 {
    let year = D512::from_str("365.2425", Context::default()).expect("constant is valid");
    let days = D512::from_str(&days.to_string(), Context::default()).expect("positive days");
    (final_value.clone() / initial.clone()).pow(year / days)
        - D512::from_str("1", Context::default()).expect("one is valid")
}

fn parse_positive_decimal(value: &str) -> Result<D512, Error> {
    if value.is_empty()
        || value
            .bytes()
            .any(|byte| !(byte.is_ascii_digit() || byte == b'.'))
        || value.matches('.').count() > 1
    {
        return Err(Error::InvalidInitialCapital);
    }
    let parsed = decimal(value).ok_or(Error::InvalidInitialCapital)?;
    (parsed > D512::from_str("0", Context::default()).expect("zero is valid"))
        .then_some(parsed)
        .ok_or(Error::InvalidInitialCapital)
}

fn decimal(value: &str) -> Option<D512> {
    D512::from_str(value, Context::default()).ok()
}

fn canonical(value: &D512) -> String {
    let value = value.to_string();
    let value = match value.split_once(['E', 'e']) {
        Some((coefficient, exponent)) => expand_exponent(coefficient, exponent).unwrap_or(value),
        None => value,
    };
    if value.contains('.') {
        value
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    } else {
        value
    }
}

fn expand_exponent(coefficient: &str, exponent: &str) -> Option<String> {
    let exponent = exponent.parse::<i32>().ok()?;
    let negative = coefficient.strip_prefix('-').is_some();
    let coefficient = coefficient.trim_start_matches('-');
    let digits = coefficient.replace('.', "");
    let decimal_index = coefficient.find('.').unwrap_or(coefficient.len()) as i32;
    let index = decimal_index + exponent;
    let body = if index <= 0 {
        format!("0.{}{}", "0".repeat((-index) as usize), digits)
    } else if index as usize >= digits.len() {
        format!("{}{}", digits, "0".repeat(index as usize - digits.len()))
    } else {
        format!(
            "{}.{}",
            &digits[..index as usize],
            &digits[index as usize..]
        )
    };
    Some(format!("{}{}", if negative { "-" } else { "" }, body))
}

fn canonical_json(value: &Value) -> String {
    match value {
        Value::Object(map) => {
            let mut keys = map.keys().collect::<Vec<_>>();
            keys.sort_unstable();
            format!(
                "{{{}}}",
                keys.into_iter()
                    .map(|key| format!(
                        "{}:{}",
                        serde_json::to_string(key).expect("key serializes"),
                        canonical_json(&map[key])
                    ))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        }
        Value::Array(values) => format!(
            "[{}]",
            values
                .iter()
                .map(canonical_json)
                .collect::<Vec<_>>()
                .join(",")
        ),
        _ => serde_json::to_string(value).expect("JSON value serializes"),
    }
}

#[derive(Clone, Debug)]
struct SimulationWindow {
    start_date: String,
    end_date: String,
    start_day: i64,
    days: i64,
}

impl SimulationWindow {
    fn parse(start: &str, end: &str) -> Result<Self, Error> {
        let start_day = parse_date(start).ok_or(Error::InvalidDateRange)?;
        let end_day = parse_date(end).ok_or(Error::InvalidDateRange)?;
        let days = end_day - start_day + 1;
        if !(1..=MAX_DAYS).contains(&days) {
            return Err(Error::InvalidDateRange);
        }
        Ok(Self {
            start_date: start.to_string(),
            end_date: end.to_string(),
            start_day,
            days,
        })
    }
    fn start_timestamp(&self) -> String {
        timestamp_for_day(self.start_day)
    }
    fn end_timestamp(&self) -> String {
        timestamp_for_day(self.start_day + self.days)
    }
    fn timestamps(&self) -> Vec<String> {
        (0..=self.days)
            .map(|offset| timestamp_for_day(self.start_day + offset))
            .collect()
    }
}

fn parse_date(value: &str) -> Option<i64> {
    let bytes = value.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    let year = value[0..4].parse::<i32>().ok()?;
    let month = value[5..7].parse::<u32>().ok()?;
    let day = value[8..10].parse::<u32>().ok()?;
    if !(1..=12).contains(&month) || day == 0 || day > days_in_month(year, month) {
        return None;
    }
    Some(days_from_civil(year, month, day))
}
fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 => 29,
        2 => 28,
        _ => 0,
    }
}
fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let year = year - i32::from(month <= 2);
    let era = if year >= 0 {
        year / 400
    } else {
        (year - 399) / 400
    };
    let yoe = year - era * 400;
    let mp = month as i32 + if month > 2 { -3 } else { 9 };
    let doy = (153 * mp + 2) / 5 + day as i32 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era as i64 * 146097 + doe as i64 - 719468
}
fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let year = yoe as i32 + era as i32 * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    (
        year + i32::from(mp >= 10),
        (mp + if mp < 10 { 3 } else { -9 }) as u32,
        (doy - (153 * mp + 2) / 5 + 1) as u32,
    )
}
fn timestamp_for_day(days: i64) -> String {
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}T00:00:00Z")
}

struct BlockResolver<'a> {
    bigwig: &'a BigwigClient,
    head: Option<BlockEvidence>,
    genesis: Option<BlockEvidence>,
    last_resolved: Option<BlockEvidence>,
    cache: HashMap<u64, BlockEvidence>,
}
#[derive(Clone, Debug)]
struct BlockEvidence {
    number: u64,
    hash: String,
    timestamp: String,
    epoch_seconds: i64,
}
impl<'a> BlockResolver<'a> {
    fn new(bigwig: &'a BigwigClient) -> Self {
        Self {
            bigwig,
            head: None,
            genesis: None,
            last_resolved: None,
            cache: HashMap::new(),
        }
    }
    async fn at_or_before(&mut self, timestamp: &str) -> Result<BlockEvidence, Error> {
        let target = parse_rfc3339_epoch(timestamp).ok_or(Error::AaveEvidenceUnavailable)?;
        let head = match self.head.clone() {
            Some(head) => head,
            None => {
                let value = self
                    .bigwig
                    .archive_rpc("eth_blockNumber", json!([]))
                    .await
                    .map_err(|_| Error::AaveEvidenceUnavailable)?;
                let head = parse_hex(value.as_str()).ok_or(Error::AaveEvidenceUnavailable)?;
                let head = self.block(head).await?;
                self.head = Some(head.clone());
                head
            }
        };

        if target >= head.epoch_seconds {
            self.last_resolved = Some(head.clone());
            return Ok(head);
        }

        let genesis = match self.genesis.clone() {
            Some(genesis) => genesis,
            None => {
                let genesis = self.block(0).await?;
                self.genesis = Some(genesis.clone());
                genesis
            }
        };
        if target < genesis.epoch_seconds {
            return Err(Error::AaveEvidenceUnavailable);
        }

        // Simulation boundaries are chronological. Reuse all previously probed
        // blocks and interpolate within the tightest known time bracket before
        // falling back to an exact binary refinement. This avoids restarting a
        // whole-chain binary search for every daily boundary.
        let mut low = self
            .last_resolved
            .as_ref()
            .filter(|block| block.epoch_seconds <= target)
            .cloned()
            .unwrap_or(genesis);
        let mut high = head;
        for block in self.cache.values() {
            if block.epoch_seconds <= target && block.number > low.number {
                low = block.clone();
            }
            if block.epoch_seconds > target && block.number < high.number {
                high = block.clone();
            }
        }

        while low.number.saturating_add(1) < high.number {
            let middle = interpolated_block_number(target, &low, &high);
            let block = self.block(middle).await?;
            if block.epoch_seconds <= target {
                low = block;
            } else {
                high = block;
            }
        }
        self.last_resolved = Some(low.clone());
        Ok(low)
    }
    async fn block(&mut self, number: u64) -> Result<BlockEvidence, Error> {
        if let Some(block) = self.cache.get(&number) {
            return Ok(block.clone());
        }
        let value = self
            .bigwig
            .archive_rpc(
                "eth_getBlockByNumber",
                json!([format!("0x{number:x}"), false]),
            )
            .await
            .map_err(|_| Error::AaveEvidenceUnavailable)?;
        let timestamp_raw = value
            .get("timestamp")
            .and_then(Value::as_str)
            .and_then(|value| parse_hex(Some(value)))
            .ok_or(Error::AaveEvidenceUnavailable)?;
        let hash = value
            .get("hash")
            .and_then(Value::as_str)
            .filter(|value| value.starts_with("0x"))
            .ok_or(Error::AaveEvidenceUnavailable)?
            .to_string();
        let block = BlockEvidence {
            number,
            hash,
            timestamp: timestamp_for_epoch(timestamp_raw as i64),
            epoch_seconds: timestamp_raw as i64,
        };
        self.cache.insert(number, block.clone());
        Ok(block)
    }
}

fn interpolated_block_number(target: i64, low: &BlockEvidence, high: &BlockEvidence) -> u64 {
    let block_span = high.number - low.number;
    let time_span = high.epoch_seconds - low.epoch_seconds;
    if time_span <= 0 {
        return low.number + block_span / 2;
    }
    let target_offset = (target - low.epoch_seconds).clamp(0, time_span) as u128;
    let estimated_offset = (target_offset * u128::from(block_span) / time_span as u128) as u64;
    (low.number + estimated_offset).clamp(low.number + 1, high.number - 1)
}
fn parse_hex(value: Option<&str>) -> Option<u64> {
    u64::from_str_radix(value?.strip_prefix("0x")?, 16).ok()
}
fn parse_rfc3339_epoch(value: &str) -> Option<i64> {
    let day = parse_date(value.get(0..10)?);
    let time = value.get(11..19)?;
    if value.get(19..) != Some("Z") {
        return None;
    }
    let hours = time.get(0..2)?.parse::<i64>().ok()?;
    let minutes = time.get(3..5)?.parse::<i64>().ok()?;
    let seconds = time.get(6..8)?.parse::<i64>().ok()?;
    (hours < 24 && minutes < 60 && seconds < 60)
        .then_some(day? * 86400 + hours * 3600 + minutes * 60 + seconds)
}
fn timestamp_for_epoch(epoch: i64) -> String {
    let days = epoch.div_euclid(86_400);
    let seconds = epoch.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hours = seconds / 3_600;
    let minutes = (seconds % 3_600) / 60;
    let seconds = seconds % 60;
    format!("{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}Z")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dates_are_utc_inclusive_and_bounded() {
        let window = SimulationWindow::parse("2025-01-01", "2025-01-02").unwrap();
        assert_eq!(
            window.timestamps(),
            vec![
                "2025-01-01T00:00:00Z",
                "2025-01-02T00:00:00Z",
                "2025-01-03T00:00:00Z"
            ]
        );
        assert!(SimulationWindow::parse("2025-01-02", "2025-01-01").is_err());
        assert!(SimulationWindow::parse("2025-01-01", "2026-01-02").is_err());
    }
    #[test]
    fn spot_simulation_marks_missing_interior_evidence_partial() {
        let window = SimulationWindow::parse("2025-01-01", "2025-01-02").unwrap();
        let prices = vec![
            PriceObservation::from_point("2025-01-01T00:00:00Z".into(), Some(&point("100"))),
            PriceObservation::from_point("2025-01-02T00:00:00Z".into(), None),
            PriceObservation::from_point("2025-01-03T00:00:00Z".into(), Some(&point("110"))),
        ];
        let result = simulate(
            &window,
            Strategy::BtcHold,
            decimal("10000").unwrap(),
            prices,
            None,
        );
        assert_eq!(result["status"], "partial");
        assert_eq!(result["metrics"]["final_value"], "11000");
        assert!(result["metrics"]["maximum_drawdown"].is_null());
    }
    #[test]
    fn block_interpolation_stays_strictly_inside_its_known_bracket() {
        let low = BlockEvidence {
            number: 1_000,
            hash: "0xlow".to_string(),
            timestamp: "2025-01-01T00:00:00Z".to_string(),
            epoch_seconds: 1_000,
        };
        let high = BlockEvidence {
            number: 2_000,
            hash: "0xhigh".to_string(),
            timestamp: "2025-01-01T00:16:40Z".to_string(),
            epoch_seconds: 2_000,
        };

        assert_eq!(interpolated_block_number(1_500, &low, &high), 1_500);
        assert_eq!(interpolated_block_number(1_000, &low, &high), 1_001);
        assert_eq!(interpolated_block_number(2_000, &low, &high), 1_999);
    }
    fn point(price: &str) -> HistoricalPricePoint {
        HistoricalPricePoint {
            bucket_start: String::new(),
            price: Some(price.into()),
            status: "observed".into(),
            source_published_at: Some("2025-01-01T00:00:00Z".into()),
            source_type: Some("fixture".into()),
            is_derived: Some(false),
            derivation_path: None,
        }
    }
}
