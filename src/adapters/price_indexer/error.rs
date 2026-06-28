#[derive(Debug, thiserror::Error)]
pub enum PriceIndexerClientInitError {
    #[error("invalid PRICE_INDEXER_URL: {0}")]
    InvalidBaseUrl(String),
}
