use std::collections::HashMap;

pub trait LatestPriceQuotes {
    async fn latest_quotes(
        &self,
        pricing_asset_slugs: &[String],
        quote_currency: &str,
    ) -> Result<HashMap<String, PriceQuoteResolution>, PriceQuoteError>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PriceQuoteResolution {
    Available {
        unit_price: String,
        quote_currency: String,
        price_as_of: String,
    },
    Unavailable,
    Unsupported,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PriceQuoteError {
    ProviderUnavailable,
    InternalError,
}
