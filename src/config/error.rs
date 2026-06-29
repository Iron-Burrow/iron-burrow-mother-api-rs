#[derive(Debug, Eq, PartialEq, thiserror::Error)]
pub(crate) enum ConfigError {
    #[error("HTTP_PORT must be a valid u16, got {0:?}")]
    InvalidHttpPort(String),
    #[error("PRICE_INDEXER_TIMEOUT_MS must be a valid u64, got {0:?}")]
    InvalidPriceIndexerTimeout(String),
    #[error("DIS_REQUEST_TIMEOUT_MS must be a valid u64, got {0:?}")]
    InvalidDisRequestTimeout(String),
    #[error("DIS_RETRY_MAX_ATTEMPTS must be a positive u64, got {0:?}")]
    InvalidDisRetryMaxAttempts(String),
    #[error("BIGWIG_REQUEST_TIMEOUT_MS must be a positive u64, got {0:?}")]
    InvalidBigwigRequestTimeout(String),
    #[error("ERC20_TRANSFERS_ENABLED must be a boolean, got {0:?}")]
    InvalidErc20TransfersEnabled(String),
    #[error("ERC20_TRANSFERS_MAX_TOKEN_FILTERS must be a positive u64, got {0:?}")]
    InvalidErc20TransfersMaxTokenFilters(String),
    #[error("BIGWIG_MAX_CONTRACT_ADDRESSES must be a positive u64, got {0:?}")]
    InvalidBigwigMaxContractAddresses(String),
    #[error("ERC20_TRANSFERS_MAX_TOKEN_FILTERS ({erc20_transfers_max_token_filters}) must not exceed BIGWIG_MAX_CONTRACT_ADDRESSES ({bigwig_max_contract_addresses})")]
    Erc20TransfersPublicLimitExceedsBigwig {
        erc20_transfers_max_token_filters: u64,
        bigwig_max_contract_addresses: u64,
    },
    #[error("HTTP_HOST and HTTP_PORT must form a valid socket address, got {host}:{port}")]
    InvalidSocketAddress { host: String, port: u16 },
}
