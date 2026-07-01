mod adapters;
mod admin;
mod application;
mod cli;
mod common;
#[allow(dead_code)]
mod config;
mod db_lifecycle;
mod domain;
mod infra;
#[allow(dead_code)]
mod openapi;
mod reference_data;
mod state;

#[cfg(test)]
mod test_utils;

use std::net::SocketAddr;
use tokio::net::TcpListener;
use tracing::{info, warn};

use crate::adapters::http::router::build_router;
use crate::cli::{Command, USAGE};
use crate::config::Config;
use crate::state::AppState;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_tracing();

    let command = match cli::parse_args(std::env::args().skip(1)) {
        Ok(command) => command,
        Err(error) => {
            eprintln!("{error}\n\n{USAGE}");
            std::process::exit(2);
        }
    };

    match command {
        Command::Serve => serve().await?,
        Command::Help => println!("{USAGE}"),
        Command::Db(command) => {
            if let Err(error) = db_lifecycle::run(command).await {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
        Command::Admin(command) => {
            if let Err(error) = admin::run(command).await {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
    }

    Ok(())
}

async fn serve() -> Result<(), Box<dyn std::error::Error>> {
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
