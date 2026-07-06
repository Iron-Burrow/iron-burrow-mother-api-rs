use std::net::SocketAddr;

use tokio::net::TcpListener;
use tracing::{info, warn};

use crate::adapters::http::router::build_router;
// use crate::cli::{Command, USAGE};
use crate::config::Config;
use crate::state::AppState;

pub(crate) async fn serve() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::from_env()?;
    let address = config.socket_addr()?;
    let state = AppState::try_new(config)?;
    let router = build_router(state.clone());
    let listener = TcpListener::bind(address).await?;

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
