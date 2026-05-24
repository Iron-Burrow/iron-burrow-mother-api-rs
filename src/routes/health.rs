use axum::Json;
use serde::Serialize;

const SERVICE_NAME: &str = "iron-burrow-mother-api";
const MASCOT_NAME: &str = "Capitan Sousa";
const HEALTH_MESSAGE: &str = "Happy squirrel, systems nominal.";

#[derive(Serialize)]
pub struct HealthResponse {
    ok: bool,
    service: &'static str,
    mascot: &'static str,
    message: &'static str,
}

pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        ok: true,
        service: SERVICE_NAME,
        mascot: MASCOT_NAME,
        message: HEALTH_MESSAGE,
    })
}

pub(crate) fn service_name() -> &'static str {
    SERVICE_NAME
}

pub(crate) fn mascot_name() -> &'static str {
    MASCOT_NAME
}
