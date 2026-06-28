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
    dis: &'static str,
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
            price_indexer: price_indexer_check(state),
            dis: dis_check(state),
            evm_indexer: "not_connected",
        },
    }
}

fn price_indexer_check(state: &AppState) -> &'static str {
    match (
        state.config.price_indexer_url.as_ref(),
        state.config.price_ql_internal_token.as_ref(),
        state.price_indexer_client.as_ref(),
    ) {
        (_, _, Some(_)) => "configured",
        (None, None, None) => "not_configured",
        _ => "invalid_config",
    }
}

fn dis_check(state: &AppState) -> &'static str {
    match (
        state.config.dis_base_url.as_ref(),
        state.dis_client.as_ref(),
    ) {
        (None, _) => "not_configured",
        (Some(_), Some(_)) => "configured",
        (Some(_), None) => "invalid_config",
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::{
        config::Config,
        test_utils::constants::{DIS_BASE_URL, PRICE_INDEXER_URL},
    };

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
                    "price_indexer": "not_configured",
                    "dis": "not_configured",
                    "evm_indexer": "not_connected"
                }
            })
        );
    }

    #[test]
    fn status_response_reports_missing_dis_config() {
        let response = status_response(&AppState::new(Config::default()), DatabaseCheck::Skipped);
        let json = serde_json::to_value(response).unwrap();

        assert_eq!(json["ok"], true);
        assert_eq!(json["checks"]["dis"], "not_configured");
    }

    #[test]
    fn status_response_reports_missing_price_indexer_config() {
        let response = status_response(&AppState::new(Config::default()), DatabaseCheck::Skipped);
        let json = serde_json::to_value(response).unwrap();

        assert_eq!(json["ok"], true);
        assert_eq!(json["checks"]["price_indexer"], "not_configured");
    }

    #[test]
    fn status_response_reports_valid_price_indexer_config() {
        let response = status_response(
            &AppState::new(Config {
                price_indexer_url: Some(PRICE_INDEXER_URL.to_string()),
                price_ql_internal_token: Some("test-token".to_string()),
                ..Config::default()
            }),
            DatabaseCheck::Skipped,
        );
        let json = serde_json::to_value(response).unwrap();

        assert_eq!(json["ok"], true);
        assert_eq!(json["checks"]["price_indexer"], "configured");
    }

    #[test]
    fn status_response_reports_invalid_price_indexer_config_without_failing_ok() {
        let response = status_response(
            &AppState::new(Config {
                price_indexer_url: Some("not a url".to_string()),
                price_ql_internal_token: Some("test-token".to_string()),
                ..Config::default()
            }),
            DatabaseCheck::Skipped,
        );
        let json = serde_json::to_value(response).unwrap();

        assert_eq!(json["ok"], true);
        assert_eq!(json["checks"]["price_indexer"], "invalid_config");
    }

    #[test]
    fn status_response_reports_incomplete_price_indexer_config_without_failing_ok() {
        let response = status_response(
            &AppState::new(Config {
                price_indexer_url: Some(PRICE_INDEXER_URL.to_string()),
                ..Config::default()
            }),
            DatabaseCheck::Skipped,
        );
        let json = serde_json::to_value(response).unwrap();

        assert_eq!(json["ok"], true);
        assert_eq!(json["checks"]["price_indexer"], "invalid_config");
    }

    #[test]
    fn status_response_reports_valid_dis_config() {
        let response = status_response(
            &AppState::new(Config {
                dis_base_url: Some(DIS_BASE_URL.to_string()),
                ..Config::default()
            }),
            DatabaseCheck::Skipped,
        );
        let json = serde_json::to_value(response).unwrap();

        assert_eq!(json["ok"], true);
        assert_eq!(json["checks"]["dis"], "configured");
    }

    #[test]
    fn status_response_reports_invalid_dis_config_without_failing_ok() {
        let response = status_response(
            &AppState::new(Config {
                dis_base_url: Some("not a url".to_string()),
                ..Config::default()
            }),
            DatabaseCheck::Skipped,
        );
        let json = serde_json::to_value(response).unwrap();

        assert_eq!(json["ok"], true);
        assert_eq!(json["checks"]["dis"], "invalid_config");
    }
}
