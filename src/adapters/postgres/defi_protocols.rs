use sqlx::{FromRow, PgPool};

use crate::{
    adapters::postgres::errors::RepositoryError,
    domain::defi::{RealizedYieldProtocol, RealizedYieldReserve},
};

#[derive(Clone, Debug)]
pub(crate) struct DefiProtocolRepository {
    pool: PgPool,
}

#[derive(FromRow)]
struct ProtocolRow {
    slug: String,
    network_slug: String,
    chain_id: i64,
    adapter_kind: String,
    adapter_version: String,
    pool_address: String,
}

#[derive(FromRow)]
struct ReserveRow {
    asset_slug: String,
    asset_symbol: String,
    underlying_asset_address: String,
}

impl DefiProtocolRepository {
    pub(crate) fn database(pool: PgPool) -> Self {
        Self { pool }
    }

    pub(crate) async fn load_realized_yield_protocol(
        &self,
        slug: &str,
    ) -> Result<Option<RealizedYieldProtocol>, RepositoryError> {
        let protocol = sqlx::query_as::<_, ProtocolRow>(
            r#"
            select protocol.slug, network.slug as network_slug, network.chain_id,
                   protocol.adapter_kind, protocol.adapter_version, pool.address as pool_address
            from mother_api.defi_protocol protocol
            join mother_api.network network on network.id = protocol.network_id
            join mother_api.defi_protocol_target pool
              on pool.defi_protocol_id = protocol.id
             and pool.target_kind = 'pool'
             and pool.target_key = 'pool'
             and pool.enabled and pool.verified
            where protocol.slug = $1
              and protocol.enabled and protocol.verified
              and network.status = 'active'
            limit 1
            "#,
        )
        .bind(slug)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::new)?;

        let Some(protocol) = protocol else {
            return Ok(None);
        };
        let reserves = sqlx::query_as::<_, ReserveRow>(
            r#"
            select asset.slug as asset_slug, asset.symbol as asset_symbol,
                   target.address as underlying_asset_address
            from mother_api.defi_protocol_target target
            join mother_api.defi_protocol protocol on protocol.id = target.defi_protocol_id
            join mother_api.asset_chain_map chain_map on chain_map.id = target.asset_chain_map_id
            join mother_api.global_asset asset on asset.id = chain_map.asset_id
            where protocol.slug = $1
              and target.target_kind = 'reserve'
              and target.enabled and target.verified
              and chain_map.network_id = protocol.network_id
              and chain_map.status = 'active' and asset.status = 'active'
            order by asset.slug
            "#,
        )
        .bind(slug)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::new)?;

        Ok(Some(RealizedYieldProtocol {
            slug: protocol.slug,
            network_slug: protocol.network_slug,
            chain_id: protocol.chain_id,
            adapter_kind: protocol.adapter_kind,
            adapter_version: protocol.adapter_version,
            pool_address: protocol.pool_address,
            reserves: reserves
                .into_iter()
                .map(|reserve| RealizedYieldReserve {
                    asset_slug: reserve.asset_slug,
                    asset_symbol: reserve.asset_symbol,
                    underlying_asset_address: reserve.underlying_asset_address,
                })
                .collect(),
        }))
    }
}
