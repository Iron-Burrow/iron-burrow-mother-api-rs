use axum::Router;

use crate::{
    adapters::http::state::HttpStateTestBuilder,
    adapters::{
        bigwig::BigwigClient, http::router::build_router, price_indexer::PriceIndexerClient,
    },
    config::Config,
};

pub(crate) fn balance_router(
    config: Config,
    bigwig_client: Option<BigwigClient>,
    price_indexer_client: Option<PriceIndexerClient>,
) -> Router {
    let mut builder = HttpStateTestBuilder::new(config);
    if let Some(bigwig_client) = bigwig_client {
        builder = builder.with_bigwig_client(bigwig_client);
    }
    if let Some(price_indexer_client) = price_indexer_client {
        builder = builder.with_price_indexer_client(price_indexer_client);
    }
    build_router(builder.build())
}

pub(crate) fn transfers_router(config: Config) -> Router {
    build_router(HttpStateTestBuilder::new(config).build())
}

pub(crate) fn transfers_router_without_repository(config: Config) -> Router {
    build_router(HttpStateTestBuilder::new(config).build())
}

pub(crate) fn transfers_router_with_bigwig_client(
    config: Config,
    bigwig_client: BigwigClient,
) -> Router {
    build_router(
        HttpStateTestBuilder::new(config)
            .with_bigwig_client(bigwig_client)
            .build(),
    )
}
