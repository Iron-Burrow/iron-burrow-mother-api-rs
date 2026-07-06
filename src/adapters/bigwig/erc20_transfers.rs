use serde::{Deserialize, Serialize};

use crate::adapters::bigwig::{client::BigwigClient, error::BigwigError};
use crate::application::erc20_transfers::service::{
    Erc20TransferExtractionError, Erc20TransferExtractionRequest, Erc20TransferExtractionResult,
    Erc20TransferExtractionRow, Erc20TransferExtractor,
};
use crate::domain::{
    onchain_time::onchain_window::OnchainWindow, transfers::transfer_direction::TransferDirection,
};

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct BigwigErc20TransferRequest {
    pub network_slug: String,
    pub address: String,
    pub direction: BigwigErc20TransferDirection,
    pub contract_addresses: Vec<String>,
    pub window: BigwigErc20TransferWindow,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum BigwigErc20TransferDirection {
    Any,
    From,
    To,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(untagged)]
pub(crate) enum BigwigErc20TransferWindow {
    Block {
        from_block: u64,
        to_block: u64,
    },
    Timestamp {
        from_timestamp: String,
        to_timestamp: String,
    },
    Lookback {
        lookback_seconds: u64,
        to: BigwigErc20TransferLookbackTarget,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum BigwigErc20TransferLookbackTarget {
    Latest,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub(crate) struct BigwigErc20TransferResponse {
    pub extractor: BigwigErc20TransferExtractor,
    pub network_slug: String,
    pub address: String,
    pub direction: BigwigErc20TransferDirection,
    pub window_kind: BigwigErc20TransferWindowKind,
    #[serde(default)]
    pub from_block: Option<u64>,
    #[serde(default)]
    pub to_block: Option<u64>,
    #[serde(default)]
    pub from_timestamp: Option<String>,
    #[serde(default)]
    pub to_timestamp: Option<String>,
    #[serde(default)]
    pub lookback_seconds: Option<u64>,
    #[serde(default)]
    pub truncated: bool,
    pub rows_extracted: u64,
    pub results: Vec<BigwigErc20TransferRow>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
pub(crate) enum BigwigErc20TransferExtractor {
    #[serde(rename = "evm_erc20_transfers_by_address")]
    EvmErc20TransfersByAddress,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum BigwigErc20TransferWindowKind {
    Block,
    Timestamp,
    Lookback,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub(crate) struct BigwigErc20TransferRow {
    pub block_number: u64,
    pub tx_hash: String,
    pub log_index: u64,
    pub token: String,
    pub from: String,
    pub to: String,
    pub value: String,
}

impl From<Erc20TransferExtractionRequest> for BigwigErc20TransferRequest {
    fn from(request: Erc20TransferExtractionRequest) -> Self {
        Self {
            network_slug: request.network_slug,
            address: request.address.to_ascii_lowercase(),
            direction: BigwigErc20TransferDirection::from(request.direction),
            contract_addresses: request
                .contract_addresses
                .into_iter()
                .map(|contract_address| contract_address.to_ascii_lowercase())
                .collect(),
            window: BigwigErc20TransferWindow::from(request.window),
        }
    }
}

impl From<TransferDirection> for BigwigErc20TransferDirection {
    fn from(direction: TransferDirection) -> Self {
        match direction {
            TransferDirection::Any => Self::Any,
            TransferDirection::From => Self::From,
            TransferDirection::To => Self::To,
        }
    }
}

impl From<OnchainWindow> for BigwigErc20TransferWindow {
    fn from(window: OnchainWindow) -> Self {
        match window {
            OnchainWindow::Block(window) => Self::Block {
                from_block: window.from_block,
                to_block: window.to_block,
            },
            OnchainWindow::Timestamp(window) => Self::Timestamp {
                from_timestamp: window.from_timestamp,
                to_timestamp: window.to_timestamp,
            },
            OnchainWindow::Lookback(window) => Self::Lookback {
                lookback_seconds: window.lookback_seconds,
                to: BigwigErc20TransferLookbackTarget::Latest,
            },
        }
    }
}

impl TryFrom<BigwigErc20TransferResponse> for Erc20TransferExtractionResult {
    type Error = Erc20TransferExtractionError;

    fn try_from(response: BigwigErc20TransferResponse) -> Result<Self, Self::Error> {
        if response.rows_extracted != u64::try_from(response.results.len()).unwrap_or(u64::MAX) {
            return Err(Erc20TransferExtractionError::InternalError);
        }

        Ok(Self {
            truncated: response.truncated,
            rows: response
                .results
                .into_iter()
                .map(|row| Erc20TransferExtractionRow {
                    block_number: row.block_number,
                    tx_hash: row.tx_hash,
                    log_index: row.log_index,
                    token: row.token.to_ascii_lowercase(),
                    from: row.from.to_ascii_lowercase(),
                    to: row.to.to_ascii_lowercase(),
                    value: row.value,
                })
                .collect(),
        })
    }
}

impl Erc20TransferExtractor for BigwigClient {
    fn search_erc20_transfers(
        &self,
        request: Erc20TransferExtractionRequest,
    ) -> impl std::future::Future<
        Output = Result<Erc20TransferExtractionResult, Erc20TransferExtractionError>,
    > + Send {
        let request = BigwigErc20TransferRequest::from(request);

        async move {
            let response = BigwigClient::search_erc20_transfers(self, &request)
                .await
                .map_err(map_bigwig_transfer_error)?;

            Erc20TransferExtractionResult::try_from(response)
        }
    }
}

pub(crate) fn map_bigwig_transfer_error(error: BigwigError) -> Erc20TransferExtractionError {
    match error {
        BigwigError::ReversedBlockRange
        | BigwigError::BlockOutOfRange
        | BigwigError::ReversedTimestampRange
        | BigwigError::TimestampOutOfRange => Erc20TransferExtractionError::InvalidWindow,
        BigwigError::LookbackTooLarge | BigwigError::RangeTooLarge => {
            Erc20TransferExtractionError::WindowTooLarge
        }
        BigwigError::RpcError => Erc20TransferExtractionError::UpstreamProviderError,
        BigwigError::ProviderTimeout => Erc20TransferExtractionError::UpstreamProviderTimeout,
        BigwigError::Timeout | BigwigError::ExtractionTimeout => {
            Erc20TransferExtractionError::ExtractionTimeout
        }
        BigwigError::Transport
        | BigwigError::Unauthorized
        | BigwigError::UnsupportedNetwork
        | BigwigError::NetworkNotEnabledForOperation
        | BigwigError::NoRouteSatisfiesOperation
        | BigwigError::RateLimited { .. }
        | BigwigError::ProviderUnavailable { .. }
        | BigwigError::InternalError => Erc20TransferExtractionError::ExtractionUnavailable,
        BigwigError::InvalidExtractionRequest
        | BigwigError::InvalidAddress
        | BigwigError::InvalidContractAddress
        | BigwigError::InvalidDirection
        | BigwigError::InvalidWindowShape
        | BigwigError::TooManyContractAddresses
        | BigwigError::RequestValidation(_)
        | BigwigError::MalformedSuccessResponse
        | BigwigError::MalformedErrorResponse
        | BigwigError::UnexpectedSuccessStatus(_)
        | BigwigError::UnexpectedErrorResponse { .. } => {
            Erc20TransferExtractionError::InternalError
        }
    }
}

#[cfg(test)]
mod tests;
