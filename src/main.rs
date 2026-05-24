mod app;
mod config;
mod routes;
mod state;

use std::net::SocketAddr;

use tokio::net::TcpListener;
use tracing::{info, warn};

use crate::{app::create_app, config::Config, state::AppState};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_tracing();

    let config = Config::from_env()?;
    let address = config.socket_addr()?;
    let state = AppState::new(config);
    let app = create_app(state.clone());
    let listener = TcpListener::bind(address).await?;

    info!(
        service = "iron-burrow-mother-api",
        host = %state.config.http_host,
        port = state.config.http_port,
        environment = %state.config.app_env,
        version = %state.version,
        "Iron Burrow Mother API listening"
    );

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal(address))
        .await?;

    Ok(())
}

fn init_tracing() {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "iron_burrow_mother_api_rs=info,tower_http=info".into());

    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .json()
        .init();
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
