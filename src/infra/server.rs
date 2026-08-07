use std::{error::Error, future::Future, io, net::SocketAddr};

use tokio::net::TcpListener;
use tracing::{info, warn};

use crate::adapters::http::router::build_router;
use crate::config::Config;
use crate::state::{AppState, AppStateError};

pub(crate) async fn serve() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::from_env()?;
    let address = config.socket_addr()?;
    serve_with(config, address, AppState::try_new, TcpListener::bind).await
}

async fn serve_with<BuildState, BindListener, BindFuture>(
    config: Config,
    address: SocketAddr,
    build_state: BuildState,
    bind_listener: BindListener,
) -> Result<(), Box<dyn Error>>
where
    BuildState: FnOnce(Config) -> Result<AppState, AppStateError>,
    BindListener: FnOnce(SocketAddr) -> BindFuture,
    BindFuture: Future<Output = Result<TcpListener, io::Error>>,
{
    let state = build_state(config)?;
    let router = build_router(state.clone());
    let listener = bind_listener(address).await?;

    info!(
        service = "iron-burrow-mother-api",
        host = %state.config.http_host,
        port = state.config.http_port,
        environment = %state.config.app_env,
        version = %state.version,
        "Iron Burrow Mother API listening"
    );

    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal(address))
        .await?;

    Ok(())
}

async fn shutdown_signal(address: SocketAddr) {
    let ctrl_c = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            warn!(%error, "failed to listen for shutdown signal");
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(error) => warn!(%error, "failed to listen for terminate signal"),
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    info!(%address, "shutdown signal received");
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };

    use super::*;
    use crate::domain::canonical_registry::CanonicalRegistryError;

    #[tokio::test]
    async fn registry_failure_stops_before_listener_binding() {
        let listener_was_called = Arc::new(AtomicBool::new(false));
        let listener_was_called_by_binder = Arc::clone(&listener_was_called);
        let address = "127.0.0.1:0".parse().unwrap();

        let error = serve_with(
            Config::default(),
            address,
            |_| {
                AppState::try_new_with_registry(Config::default(), || {
                    Err(CanonicalRegistryError::Invalid(
                        "invalid fixture catalog".to_string(),
                    ))
                })
            },
            move |_| {
                listener_was_called_by_binder.store(true, Ordering::SeqCst);
                async { Err(io::Error::other("listener must not be called")) }
            },
        )
        .await
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            "canonical registry initialization failed"
        );
        assert!(!listener_was_called.load(Ordering::SeqCst));
    }
}
