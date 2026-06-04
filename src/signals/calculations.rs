const DECIMAL_SCALE: i128 = 1_000_000;
const SECONDS_PER_DAY: f64 = 86_400.0;

// This epsilon is only a numerical-noise threshold aligned to six-decimal
// response precision. It is not a claim of material price movement.
const SLOPE_DIRECTION_EPSILON: f64 = 0.0000005;

#[derive(Clone, Debug, PartialEq)]
pub struct PricePoint {
    pub timestamp: String,
    pub unix_seconds: i64,
    pub price: DecimalAmount,
    pub source: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct DecimalAmount {
    micros: i128,
}

impl DecimalAmount {
    pub fn parse(raw: &str) -> Result<Self, CalculationError> {
        let trimmed = raw.trim();

        if trimmed.is_empty() {
            return Err(CalculationError::InvalidDecimal);
        }

        let (sign, unsigned) = match trimmed.strip_prefix('-') {
            Some(unsigned) => (-1_i128, unsigned),
            None => (1_i128, trimmed.strip_prefix('+').unwrap_or(trimmed)),
        };

        if unsigned.is_empty() {
            return Err(CalculationError::InvalidDecimal);
        }

        let (integral, fractional) = unsigned.split_once('.').unwrap_or((unsigned, ""));

        if integral.is_empty() || !integral.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(CalculationError::InvalidDecimal);
        }

        if !fractional.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(CalculationError::InvalidDecimal);
        }

        let integral = integral
            .parse::<i128>()
            .map_err(|_| CalculationError::InvalidDecimal)?;
        let mut fractional_digits = fractional.chars().take(6).collect::<String>();

        while fractional_digits.len() < 6 {
            fractional_digits.push('0');
        }

        let mut fractional_micros = if fractional_digits.is_empty() {
            0
        } else {
            fractional_digits
                .parse::<i128>()
                .map_err(|_| CalculationError::InvalidDecimal)?
        };

        if fractional
            .chars()
            .nth(6)
            .and_then(|digit| digit.to_digit(10))
            .is_some_and(|digit| digit >= 5)
        {
            fractional_micros += 1;
        }

        let carry = fractional_micros / DECIMAL_SCALE;
        fractional_micros %= DECIMAL_SCALE;

        Ok(Self {
            micros: sign * ((integral + carry) * DECIMAL_SCALE + fractional_micros),
        })
    }

    pub fn is_non_positive(self) -> bool {
        self.micros <= 0
    }

    pub fn to_f64(self) -> f64 {
        self.micros as f64 / DECIMAL_SCALE as f64
    }

    pub fn to_fixed_6(self) -> String {
        format_micro_decimal(self.micros)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PriceStats {
    pub first_price: String,
    pub last_price: String,
    pub min_price: String,
    pub max_price: String,
    pub avg_price: String,
    pub change_abs: String,
    pub change_pct: String,
    pub observations: usize,
}

pub fn calculate_stats(points: &[PricePoint]) -> Option<PriceStats> {
    let first = points.first()?.price;
    let last = points.last()?.price;
    let mut min = first;
    let mut max = first;
    let mut sum = 0_i128;

    for point in points {
        min = min.min(point.price);
        max = max.max(point.price);
        sum += point.price.micros;
    }

    let observations = points.len();
    let avg = div_round_i128(sum, observations as i128);
    let change_abs = last.micros - first.micros;
    let change_pct = if first.micros == 0 {
        0
    } else {
        div_round_i128(change_abs * 100 * DECIMAL_SCALE, first.micros)
    };

    Some(PriceStats {
        first_price: first.to_fixed_6(),
        last_price: last.to_fixed_6(),
        min_price: min.to_fixed_6(),
        max_price: max.to_fixed_6(),
        avg_price: format_micro_decimal(avg),
        change_abs: format_micro_decimal(change_abs),
        change_pct: format_micro_decimal(change_pct),
        observations,
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrendEvidence {
    pub positive_models: usize,
    pub negative_models: usize,
    pub flat_models: usize,
    pub skipped_models: usize,
    pub total_models: usize,
    pub agreement: &'static str,
}

impl TrendEvidence {
    pub fn insufficient_data() -> Self {
        Self {
            positive_models: 0,
            negative_models: 0,
            flat_models: 0,
            skipped_models: 0,
            total_models: 0,
            agreement: "insufficient_data",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TrendModel {
    pub name: &'static str,
    pub transform: &'static str,
    pub status: &'static str,
    pub direction: Option<&'static str>,
    pub slope_per_day: Option<String>,
    pub slope_index_points_per_day: Option<String>,
    pub approx_pct_change_per_day: Option<String>,
    pub r_squared: Option<String>,
    pub reason: Option<&'static str>,
}

pub fn calculate_trend(points: &[PricePoint]) -> TrendCalculation {
    if points.len() < 2 {
        return TrendCalculation {
            stats: None,
            models: Vec::new(),
            evidence: TrendEvidence::insufficient_data(),
        };
    }

    let stats = calculate_stats(points);
    let mut models = Vec::new();

    models.push(included_model(
        "linear_raw_price",
        "price",
        "slope_per_day",
        y_values(points, |point| point.price.to_f64()),
    ));

    if points.iter().any(|point| point.price.is_non_positive()) {
        models.push(skipped_model(
            "log_linear_price",
            "ln(price)",
            "non_positive_price",
        ));
    } else {
        models.push(included_model(
            "log_linear_price",
            "ln(price)",
            "slope_per_day",
            y_values(points, |point| point.price.to_f64().ln()),
        ));
    }

    let first_price = points[0].price;

    if first_price.is_non_positive() {
        models.push(skipped_model(
            "indexed_linear_price",
            "price_index_100",
            "non_positive_first_price",
        ));
    } else {
        let first_price = first_price.to_f64();
        models.push(included_model(
            "indexed_linear_price",
            "price_index_100",
            "slope_index_points_per_day",
            y_values(points, |point| point.price.to_f64() / first_price * 100.0),
        ));
    }

    let evidence = aggregate_evidence(&models);

    TrendCalculation {
        stats,
        models,
        evidence,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TrendCalculation {
    pub stats: Option<PriceStats>,
    pub models: Vec<TrendModel>,
    pub evidence: TrendEvidence,
}

fn included_model(
    name: &'static str,
    transform: &'static str,
    slope_field: &'static str,
    values: Vec<(f64, f64)>,
) -> TrendModel {
    let Some(regression) = ordinary_least_squares(&values) else {
        return skipped_model(name, transform, "not_enough_distinct_timestamps");
    };
    let direction = classify_direction(regression.slope_per_day);
    let slope = format_f64_6(regression.slope_per_day);
    let approx_pct_change_per_day = if name == "log_linear_price" {
        Some(format_f64_6((regression.slope_per_day.exp() - 1.0) * 100.0))
    } else {
        None
    };

    TrendModel {
        name,
        transform,
        status: "included",
        direction: Some(direction),
        slope_per_day: (slope_field == "slope_per_day").then(|| slope.clone()),
        slope_index_points_per_day: (slope_field == "slope_index_points_per_day").then_some(slope),
        approx_pct_change_per_day,
        r_squared: Some(format_f64_6(regression.r_squared)),
        reason: None,
    }
}

fn skipped_model(name: &'static str, transform: &'static str, reason: &'static str) -> TrendModel {
    TrendModel {
        name,
        transform,
        status: "skipped",
        direction: None,
        slope_per_day: None,
        slope_index_points_per_day: None,
        approx_pct_change_per_day: None,
        r_squared: None,
        reason: Some(reason),
    }
}

fn aggregate_evidence(models: &[TrendModel]) -> TrendEvidence {
    let positive_models = models
        .iter()
        .filter(|model| model.direction == Some("positive"))
        .count();
    let negative_models = models
        .iter()
        .filter(|model| model.direction == Some("negative"))
        .count();
    let flat_models = models
        .iter()
        .filter(|model| model.direction == Some("flat"))
        .count();
    let skipped_models = models
        .iter()
        .filter(|model| model.status == "skipped")
        .count();
    let included_models = positive_models + negative_models + flat_models;
    let agreement = if included_models == 0 {
        "insufficient_data"
    } else if positive_models > 0 && negative_models == 0 && flat_models == 0 {
        "positive"
    } else if negative_models > 0 && positive_models == 0 && flat_models == 0 {
        "negative"
    } else if flat_models > 0 && positive_models == 0 && negative_models == 0 {
        "flat"
    } else {
        "mixed"
    };

    TrendEvidence {
        positive_models,
        negative_models,
        flat_models,
        skipped_models,
        total_models: models.len(),
        agreement,
    }
}

fn y_values(points: &[PricePoint], transform: impl Fn(&PricePoint) -> f64) -> Vec<(f64, f64)> {
    let first_timestamp = points[0].unix_seconds;

    points
        .iter()
        .map(|point| {
            (
                (point.unix_seconds - first_timestamp) as f64 / SECONDS_PER_DAY,
                transform(point),
            )
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Regression {
    slope_per_day: f64,
    r_squared: f64,
}

fn ordinary_least_squares(values: &[(f64, f64)]) -> Option<Regression> {
    if values.len() < 2 {
        return None;
    }

    let count = values.len() as f64;
    let mean_x = values.iter().map(|(x, _)| x).sum::<f64>() / count;
    let mean_y = values.iter().map(|(_, y)| y).sum::<f64>() / count;
    let mut sum_xx = 0.0;
    let mut sum_xy = 0.0;
    let mut total_y_variance = 0.0;

    for (x, y) in values {
        let dx = x - mean_x;
        let dy = y - mean_y;
        sum_xx += dx * dx;
        sum_xy += dx * dy;
        total_y_variance += dy * dy;
    }

    if sum_xx.abs() <= f64::EPSILON {
        return None;
    }

    let slope_per_day = sum_xy / sum_xx;
    let intercept = mean_y - slope_per_day * mean_x;
    let mut residual_sum_squares = 0.0;

    for (x, y) in values {
        let predicted = intercept + slope_per_day * x;
        residual_sum_squares += (y - predicted).powi(2);
    }

    let r_squared = if total_y_variance.abs() <= f64::EPSILON {
        1.0
    } else {
        (1.0 - residual_sum_squares / total_y_variance).clamp(0.0, 1.0)
    };

    Some(Regression {
        slope_per_day,
        r_squared,
    })
}

fn classify_direction(slope_per_day: f64) -> &'static str {
    if slope_per_day > SLOPE_DIRECTION_EPSILON {
        "positive"
    } else if slope_per_day < -SLOPE_DIRECTION_EPSILON {
        "negative"
    } else {
        "flat"
    }
}

fn format_micro_decimal(micros: i128) -> String {
    let sign = if micros < 0 { "-" } else { "" };
    let absolute = micros.abs();
    let integral = absolute / DECIMAL_SCALE;
    let fractional = absolute % DECIMAL_SCALE;

    format!("{sign}{integral}.{fractional:06}")
}

fn format_f64_6(value: f64) -> String {
    if value.abs() < 0.0000005 {
        "0.000000".to_string()
    } else {
        format!("{value:.6}")
    }
}

fn div_round_i128(numerator: i128, denominator: i128) -> i128 {
    if denominator == 0 {
        return 0;
    }

    let quotient = numerator / denominator;
    let remainder = numerator % denominator;

    if remainder == 0 {
        return quotient;
    }

    let double_remainder = remainder.abs() * 2;

    if double_remainder >= denominator.abs() {
        quotient
            + if (numerator > 0) == (denominator > 0) {
                1
            } else {
                -1
            }
    } else {
        quotient
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CalculationError {
    InvalidDecimal,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(day: i64, price: &str) -> PricePoint {
        PricePoint {
            timestamp: format!("2026-05-{:02}T00:00:00Z", 20 + day),
            unix_seconds: day * 86_400,
            price: DecimalAmount::parse(price).unwrap(),
            source: Some("chainlink".to_string()),
        }
    }

    #[test]
    fn parses_decimal_strings_to_six_decimal_precision() {
        assert_eq!(
            DecimalAmount::parse("3811.4567894").unwrap().to_fixed_6(),
            "3811.456789"
        );
        assert_eq!(
            DecimalAmount::parse("3811.4567895").unwrap().to_fixed_6(),
            "3811.456790"
        );
        assert_eq!(
            DecimalAmount::parse("-1.5").unwrap().to_fixed_6(),
            "-1.500000"
        );
        assert_eq!(
            DecimalAmount::parse("nope"),
            Err(CalculationError::InvalidDecimal)
        );
    }

    #[test]
    fn calculates_basic_price_stats() {
        let points = vec![
            point(0, "100.000000"),
            point(1, "120.000000"),
            point(2, "80.000000"),
            point(3, "130.000000"),
        ];

        let stats = calculate_stats(&points).unwrap();

        assert_eq!(stats.first_price, "100.000000");
        assert_eq!(stats.last_price, "130.000000");
        assert_eq!(stats.min_price, "80.000000");
        assert_eq!(stats.max_price, "130.000000");
        assert_eq!(stats.avg_price, "107.500000");
        assert_eq!(stats.change_abs, "30.000000");
        assert_eq!(stats.change_pct, "30.000000");
        assert_eq!(stats.observations, 4);
    }

    #[test]
    fn raw_linear_model_returns_expected_slope_and_direction() {
        let points = vec![
            point(0, "100.000000"),
            point(1, "110.000000"),
            point(2, "120.000000"),
        ];
        let trend = calculate_trend(&points);

        let raw = &trend.models[0];

        assert_eq!(raw.name, "linear_raw_price");
        assert_eq!(raw.status, "included");
        assert_eq!(raw.direction, Some("positive"));
        assert_eq!(raw.slope_per_day.as_deref(), Some("10.000000"));
        assert_eq!(raw.r_squared.as_deref(), Some("1.000000"));
    }

    #[test]
    fn log_model_is_skipped_for_non_positive_prices() {
        let points = vec![point(0, "100.000000"), point(1, "0.000000")];
        let trend = calculate_trend(&points);
        let log = &trend.models[1];

        assert_eq!(log.name, "log_linear_price");
        assert_eq!(log.status, "skipped");
        assert_eq!(log.reason, Some("non_positive_price"));
        assert_eq!(trend.evidence.skipped_models, 1);
    }

    #[test]
    fn indexed_model_normalizes_first_price_to_100() {
        let points = vec![point(0, "50.000000"), point(1, "55.000000")];
        let trend = calculate_trend(&points);
        let indexed = &trend.models[2];

        assert_eq!(indexed.name, "indexed_linear_price");
        assert_eq!(indexed.direction, Some("positive"));
        assert_eq!(
            indexed.slope_index_points_per_day.as_deref(),
            Some("10.000000")
        );
    }

    #[test]
    fn evidence_aggregation_classifies_agreement() {
        let positive = calculate_trend(&[
            point(0, "100.000000"),
            point(1, "110.000000"),
            point(2, "120.000000"),
        ]);
        let negative = calculate_trend(&[
            point(0, "120.000000"),
            point(1, "110.000000"),
            point(2, "100.000000"),
        ]);
        let flat = calculate_trend(&[
            point(0, "100.000000"),
            point(1, "100.000000"),
            point(2, "100.000000"),
        ]);

        assert_eq!(positive.evidence.agreement, "positive");
        assert_eq!(positive.evidence.positive_models, 3);
        assert_eq!(negative.evidence.agreement, "negative");
        assert_eq!(negative.evidence.negative_models, 3);
        assert_eq!(flat.evidence.agreement, "flat");
        assert_eq!(flat.evidence.flat_models, 3);
    }

    #[test]
    fn insufficient_data_returns_stable_empty_evidence() {
        let trend = calculate_trend(&[point(0, "100.000000")]);

        assert_eq!(trend.stats, None);
        assert!(trend.models.is_empty());
        assert_eq!(trend.evidence, TrendEvidence::insufficient_data());
    }
}
