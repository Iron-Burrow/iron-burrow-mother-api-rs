use serde::Serialize;

const USD_MICRO_PER_USD: i64 = 1_000_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(dead_code)]
pub enum BillingCurrency {
    UsdMicro,
    BtcSats,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(dead_code)]
pub enum ApiKeyType {
    DemoLike,
    OneTimeApi,
    ShrewSubscription,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MeteredOperation {
    PriceLatest,
    SignalPriceStats,
    SignalPriceTrend,
}

impl MeteredOperation {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PriceLatest => "price.latest",
            Self::SignalPriceStats => "signal.price_stats",
            Self::SignalPriceTrend => "signal.price_trend",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UsageQuote {
    pub operation: String,
    pub currency: BillingCurrency,
    pub amount_minor: i64,
    pub billable: bool,
    pub reason: Option<String>,
}

impl UsageQuote {
    pub fn not_billable(operation: MeteredOperation, reason: &str) -> Self {
        Self {
            operation: operation.as_str().to_string(),
            currency: BillingCurrency::UsdMicro,
            amount_minor: 0,
            billable: false,
            reason: Some(reason.to_string()),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BillingPayload {
    billable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    currency: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    amount: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    amount_sats: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
}

impl From<UsageQuote> for BillingPayload {
    fn from(quote: UsageQuote) -> Self {
        if !quote.billable {
            return Self {
                billable: false,
                currency: None,
                amount: None,
                amount_sats: None,
                reason: quote.reason,
            };
        }

        match quote.currency {
            BillingCurrency::UsdMicro => Self {
                billable: true,
                currency: Some("USD"),
                amount: Some(format_usd_micro(quote.amount_minor)),
                amount_sats: None,
                reason: None,
            },
            BillingCurrency::BtcSats => Self {
                billable: true,
                currency: Some("BTC"),
                amount: None,
                amount_sats: Some(quote.amount_minor),
                reason: None,
            },
        }
    }
}

pub struct AlphaPricingCatalog;

impl AlphaPricingCatalog {
    pub fn quote_usd(operation: MeteredOperation, range_days: Option<i64>) -> UsageQuote {
        Self::quote(operation, range_days, BillingCurrency::UsdMicro)
    }

    #[allow(dead_code)]
    pub fn quote_btc(operation: MeteredOperation, range_days: Option<i64>) -> UsageQuote {
        Self::quote(operation, range_days, BillingCurrency::BtcSats)
    }

    fn quote(
        operation: MeteredOperation,
        range_days: Option<i64>,
        currency: BillingCurrency,
    ) -> UsageQuote {
        let amount_minor = match (operation, range_days.unwrap_or(0), currency) {
            (MeteredOperation::PriceLatest, _, BillingCurrency::UsdMicro) => 100,
            (MeteredOperation::PriceLatest, _, BillingCurrency::BtcSats) => 1,
            (MeteredOperation::SignalPriceStats, days, BillingCurrency::UsdMicro) if days <= 7 => {
                500
            }
            (MeteredOperation::SignalPriceStats, _, BillingCurrency::UsdMicro) => 1_500,
            (MeteredOperation::SignalPriceStats, days, BillingCurrency::BtcSats) if days <= 7 => 1,
            (MeteredOperation::SignalPriceStats, _, BillingCurrency::BtcSats) => 5,
            (MeteredOperation::SignalPriceTrend, days, BillingCurrency::UsdMicro) if days <= 7 => {
                1_000
            }
            (MeteredOperation::SignalPriceTrend, _, BillingCurrency::UsdMicro) => 3_000,
            (MeteredOperation::SignalPriceTrend, days, BillingCurrency::BtcSats) if days <= 7 => 3,
            (MeteredOperation::SignalPriceTrend, _, BillingCurrency::BtcSats) => 10,
        };

        UsageQuote {
            operation: operation.as_str().to_string(),
            currency,
            amount_minor,
            billable: true,
            reason: None,
        }
    }
}

fn format_usd_micro(amount_minor: i64) -> String {
    let sign = if amount_minor < 0 { "-" } else { "" };
    let absolute = amount_minor.abs();
    let major = absolute / USD_MICRO_PER_USD;
    let minor = absolute % USD_MICRO_PER_USD;

    format!("{sign}{major}.{minor:06}")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn formats_usd_micro_as_six_decimal_amounts() {
        assert_eq!(format_usd_micro(100), "0.000100");
        assert_eq!(format_usd_micro(500), "0.000500");
        assert_eq!(format_usd_micro(1_500), "0.001500");
        assert_eq!(format_usd_micro(1_000_000), "1.000000");
    }

    #[test]
    fn quotes_alpha_usd_prices_by_operation_and_range() {
        assert_eq!(
            AlphaPricingCatalog::quote_usd(MeteredOperation::PriceLatest, None).amount_minor,
            100
        );
        assert_eq!(
            AlphaPricingCatalog::quote_usd(MeteredOperation::SignalPriceStats, Some(7))
                .amount_minor,
            500
        );
        assert_eq!(
            AlphaPricingCatalog::quote_usd(MeteredOperation::SignalPriceStats, Some(31))
                .amount_minor,
            1_500
        );
        assert_eq!(
            AlphaPricingCatalog::quote_usd(MeteredOperation::SignalPriceTrend, Some(7))
                .amount_minor,
            1_000
        );
        assert_eq!(
            AlphaPricingCatalog::quote_usd(MeteredOperation::SignalPriceTrend, Some(31))
                .amount_minor,
            3_000
        );
    }

    #[test]
    fn quotes_alpha_btc_prices_by_operation_and_range() {
        assert_eq!(
            AlphaPricingCatalog::quote_btc(MeteredOperation::PriceLatest, None).amount_minor,
            1
        );
        assert_eq!(
            AlphaPricingCatalog::quote_btc(MeteredOperation::SignalPriceStats, Some(7))
                .amount_minor,
            1
        );
        assert_eq!(
            AlphaPricingCatalog::quote_btc(MeteredOperation::SignalPriceStats, Some(31))
                .amount_minor,
            5
        );
        assert_eq!(
            AlphaPricingCatalog::quote_btc(MeteredOperation::SignalPriceTrend, Some(7))
                .amount_minor,
            3
        );
        assert_eq!(
            AlphaPricingCatalog::quote_btc(MeteredOperation::SignalPriceTrend, Some(31))
                .amount_minor,
            10
        );
    }

    #[test]
    fn renders_public_billing_payloads() {
        let usd = BillingPayload::from(AlphaPricingCatalog::quote_usd(
            MeteredOperation::SignalPriceStats,
            Some(7),
        ));
        let sats = BillingPayload::from(AlphaPricingCatalog::quote_btc(
            MeteredOperation::SignalPriceTrend,
            Some(31),
        ));
        let free = BillingPayload::from(UsageQuote::not_billable(
            MeteredOperation::SignalPriceTrend,
            "insufficient_data",
        ));

        assert_eq!(
            serde_json::to_value(usd).unwrap(),
            json!({"billable": true, "currency": "USD", "amount": "0.000500"})
        );
        assert_eq!(
            serde_json::to_value(sats).unwrap(),
            json!({"billable": true, "currency": "BTC", "amount_sats": 10})
        );
        assert_eq!(
            serde_json::to_value(free).unwrap(),
            json!({"billable": false, "reason": "insufficient_data"})
        );
    }

    #[test]
    fn api_key_types_are_distinct_from_balance_currency() {
        assert_ne!(ApiKeyType::DemoLike, ApiKeyType::OneTimeApi);
        assert_ne!(BillingCurrency::UsdMicro, BillingCurrency::BtcSats);
    }
}
