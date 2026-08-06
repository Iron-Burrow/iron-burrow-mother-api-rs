use crate::{
    adapters::{
        aave_v3::{
            AaveV3RealizedYieldAdapter, Error as AaveError, Request as AaveRequest,
            Result as AaveResult,
        },
        bigwig::BigwigClient,
        postgres::DefiProtocolRepository,
    },
    domain::defi::RealizedYieldProtocol,
};

#[derive(Clone, Debug)]
pub(crate) struct Command {
    pub(crate) protocol_slug: String,
    pub(crate) asset_slug: String,
    pub(crate) from_block: u64,
    pub(crate) to_block: u64,
    pub(crate) include_annualized_apy_estimate: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct Result {
    pub(crate) protocol: RealizedYieldProtocol,
    pub(crate) resolved: AaveResult,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error("protocol is unavailable")]
    ProtocolUnavailable,
    #[error("protocol operation is unsupported")]
    OperationUnsupported,
    #[error("registry is unavailable")]
    Registry,
    #[error(transparent)]
    Aave(#[from] AaveError),
}

pub(crate) trait RealizedYieldProtocolAdapter {
    fn adapter_kind(&self) -> &'static str;
}

impl RealizedYieldProtocolAdapter for AaveV3RealizedYieldAdapter {
    fn adapter_kind(&self) -> &'static str {
        "aave_v3_realized_yield"
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Service {
    protocols: DefiProtocolRepository,
    bigwig: BigwigClient,
    min_confirmations: u64,
    aave_v3: AaveV3RealizedYieldAdapter,
}

impl Service {
    pub(crate) fn new(
        protocols: DefiProtocolRepository,
        bigwig: BigwigClient,
        min_confirmations: u64,
    ) -> Self {
        Self {
            protocols,
            bigwig,
            min_confirmations,
            aave_v3: AaveV3RealizedYieldAdapter,
        }
    }

    pub(crate) async fn resolve(&self, command: Command) -> std::result::Result<Result, Error> {
        let protocol = self
            .protocols
            .load_realized_yield_protocol(&command.protocol_slug)
            .await
            .map_err(|_| Error::Registry)?
            .ok_or(Error::ProtocolUnavailable)?;
        if protocol.adapter_kind != self.aave_v3.adapter_kind() {
            return Err(Error::OperationUnsupported);
        }
        let resolved = self
            .aave_v3
            .resolve(
                &protocol,
                AaveRequest {
                    asset_slug: command.asset_slug,
                    from_block: command.from_block,
                    to_block: command.to_block,
                    include_annualized_apy_estimate: command.include_annualized_apy_estimate,
                },
                &self.bigwig,
                self.min_confirmations,
            )
            .await?;
        Ok(Result { protocol, resolved })
    }
}
