use std::collections::HashMap;

use tracing::warn;

use crate::price_indexer::{PriceIndexerClient, StrictLatestQuote, StrictPriceBatchError};

#[derive(Clone, Debug)]
pub struct PriceQuoteClient {
    client: PriceIndexerClient,
}

impl PriceQuoteClient {
    pub fn new(client: PriceIndexerClient) -> Self {
        Self { client }
    }

    pub async fn latest_quotes(
        &self,
        pricing_asset_slugs: &[String],
        quote_currency: &str,
    ) -> Result<HashMap<String, PriceQuoteResolution>, PriceQuoteClientError> {
        match self
            .client
            .latest_quotes_strict(pricing_asset_slugs, quote_currency)
            .await
        {
            Ok(quotes) => Ok(quotes
                .into_iter()
                .map(|(slug, quote)| {
                    let quote = match quote {
                        StrictLatestQuote::Available {
                            unit_price,
                            quote_currency,
                            recorded_at,
                        } => PriceQuoteResolution::Available {
                            unit_price,
                            quote_currency,
                            price_as_of: recorded_at,
                        },
                        StrictLatestQuote::Unavailable => PriceQuoteResolution::Unavailable,
                        StrictLatestQuote::Unsupported => PriceQuoteResolution::Unsupported,
                    };
                    (slug, quote)
                })
                .collect()),
            Err(error) => {
                log_quote_error(&self.client, pricing_asset_slugs, quote_currency, &error);
                Err(PriceQuoteClientError::from(error))
            }
        }
    }
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
pub enum PriceQuoteClientError {
    ProviderUnavailable,
    InternalError,
}

impl From<StrictPriceBatchError> for PriceQuoteClientError {
    fn from(error: StrictPriceBatchError) -> Self {
        match error {
            StrictPriceBatchError::Unauthorized
            | StrictPriceBatchError::ProviderUnavailable { .. }
            | StrictPriceBatchError::Timeout
            | StrictPriceBatchError::Transport => Self::ProviderUnavailable,
            StrictPriceBatchError::InvalidRequest | StrictPriceBatchError::MalformedResponse => {
                Self::InternalError
            }
        }
    }
}

fn log_quote_error(
    client: &PriceIndexerClient,
    pricing_asset_slugs: &[String],
    quote_currency: &str,
    error: &StrictPriceBatchError,
) {
    match error {
        StrictPriceBatchError::ProviderUnavailable { status, code } => warn!(
            price_indexer_host = client.base_host(),
            pricing_asset_slugs = ?pricing_asset_slugs,
            quote_currency,
            ?status,
            ?code,
            "Balance quote batch failed"
        ),
        _ => warn!(
            price_indexer_host = client.base_host(),
            pricing_asset_slugs = ?pricing_asset_slugs,
            quote_currency,
            ?error,
            "Balance quote batch failed"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_strict_client_errors_to_balance_quote_classes() {
        for error in [
            StrictPriceBatchError::Unauthorized,
            StrictPriceBatchError::ProviderUnavailable {
                status: Some(503),
                code: Some("UNAVAILABLE".to_string()),
            },
            StrictPriceBatchError::Timeout,
            StrictPriceBatchError::Transport,
        ] {
            assert_eq!(
                PriceQuoteClientError::from(error),
                PriceQuoteClientError::ProviderUnavailable
            );
        }

        for error in [
            StrictPriceBatchError::InvalidRequest,
            StrictPriceBatchError::MalformedResponse,
        ] {
            assert_eq!(
                PriceQuoteClientError::from(error),
                PriceQuoteClientError::InternalError
            );
        }
    }
}
