use axum::{extract::State, Json};
use serde::Serialize;
use sqlx::PgPool;

use crate::state::AppState;

const SERVICE_NAME: &str = "iron-burrow-mother-api";
const MASCOT_NAME: &str = "Capitan Sousa";
const STATUS_MESSAGE: &str = "Mother API is online.";

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DatabaseCheck {
    Skipped,
    Reachable,
    Unreachable,
}

impl DatabaseCheck {
    fn as_str(self) -> &'static str {
        match self {
            Self::Skipped => "skipped",
            Self::Reachable => "reachable",
            Self::Unreachable => "unreachable",
        }
    }

    fn allows_ok(self) -> bool {
        matches!(self, Self::Skipped | Self::Reachable)
    }
}

pub async fn status(State(state): State<AppState>) -> Json<StatusResponse> {
    let database = check_database(state.database_pool.as_ref()).await;

    Json(status_response(&state, database))
}

async fn check_database(pool: Option<&PgPool>) -> DatabaseCheck {
    let Some(pool) = pool else {
        return DatabaseCheck::Skipped;
    };

    match sqlx::query("select 1").execute(pool).await {
        Ok(_) => DatabaseCheck::Reachable,
        Err(_) => DatabaseCheck::Unreachable,
    }
}

fn status_response(state: &AppState, database: DatabaseCheck) -> StatusResponse {
    StatusResponse {
        ok: database.allows_ok(),
        service: SERVICE_NAME,
        version: state.version,
        environment: state.config.app_env.clone(),
        mascot: MASCOT_NAME,
        message: STATUS_MESSAGE,
        checks: StatusChecks {
            app: "ok",
            database: database.as_str(),
            price_indexer: "not_connected",
            evm_indexer: "not_connected",
        },
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::config::Config;

    #[test]
    fn status_response_reports_unreachable_database_without_live_postgres() {
        let response = status_response(
            &AppState::new(Config::default()),
            DatabaseCheck::Unreachable,
        );
        let json = serde_json::to_value(response).unwrap();

        assert_eq!(json["ok"], false);
        assert_eq!(json["checks"]["database"], "unreachable");
        assert_eq!(
            json,
            json!({
                "ok": false,
                "service": "iron-burrow-mother-api",
                "version": env!("CARGO_PKG_VERSION"),
                "environment": "development",
                "mascot": "Capitan Sousa",
                "message": "Mother API is online.",
                "checks": {
                    "app": "ok",
                    "database": "unreachable",
                    "price_indexer": "not_connected",
                    "evm_indexer": "not_connected"
                }
            })
        );
    }
}
