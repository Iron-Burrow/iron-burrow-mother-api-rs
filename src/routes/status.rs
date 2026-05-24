use axum::{extract::State, Json};
use serde::Serialize;

use crate::{routes::health, state::AppState};

#[derive(Serialize)]
pub struct StatusResponse {
    ok: bool,
    service: &'static str,
    version: &'static str,
    environment: String,
    mascot: &'static str,
    message: &'static str,
    checks: StatusChecks,
}

#[derive(Serialize)]
pub struct StatusChecks {
    app: &'static str,
    database: &'static str,
    price_indexer: &'static str,
    evm_indexer: &'static str,
}

pub async fn status(State(state): State<AppState>) -> Json<StatusResponse> {
    Json(StatusResponse {
        ok: true,
        service: health::service_name(),
        version: state.version,
        environment: state.config.app_env,
        mascot: health::mascot_name(),
        message: "Mother API is online.",
        checks: StatusChecks {
            app: "ok",
            database: "skipped",
            price_indexer: "not_connected",
            evm_indexer: "not_connected",
        },
    })
}
