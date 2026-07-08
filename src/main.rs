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

use crate::cli::{Command, USAGE};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    infra::telemetry::init_tracing();

    let command = match cli::parse_args(std::env::args().skip(1)) {
        Ok(command) => command,
        Err(error) => {
            eprintln!("{error}\n\n{USAGE}");
            std::process::exit(2);
        }
    };

    match command {
        Command::Serve => infra::server::serve().await?,
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
