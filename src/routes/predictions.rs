use axum::{
    extract::{Path, State},
    http::{HeaderName, HeaderValue},
    response::Response,
    Json,
};
use serde::Serialize;

use crate::{
    dis::{
        DisClientError, PolymarketCountrySnapshot, PolymarketCountrySubject,
        PolymarketSnapshotRequest, PolymarketSnapshotResponse, PolymarketWinnerOutcome,
        PolymarketWinnerSnapshot,
    },
    error::ApiError,
    state::AppState,
};

const WINNER_EVENT_SLUG: &str = "fifa-world-cup-2026-winner";
const COUNTRY_EVENT_SLUG: &str = "fifa-world-cup-2026-country-probability";
pub const DEPRECATION_HEADER_VALUE: &str = "@1781740800";

pub async fn add_deprecation_header(mut response: Response) -> Response {
    response.headers_mut().insert(
        HeaderName::from_static("deprecation"),
        HeaderValue::from_static(DEPRECATION_HEADER_VALUE),
    );
    response
}

#[derive(Serialize)]
pub struct WinnerPredictionResponse {
    ok: bool,
    event: String,
    event_slug: String,
    odds: Vec<PredictionOdd>,
    source: String,
    deterministic: bool,
    captured_at: String,
}

#[derive(Serialize)]
pub struct CountryPredictionResponse {
    ok: bool,
    market: String,
    country: PredictionCountrySummary,
    probability: String,
    price: String,
    currency: String,
    source: String,
    deterministic: bool,
    captured_at: String,
}

#[derive(Serialize)]
struct PredictionOdd {
    team: String,
    probability: String,
    price: String,
    currency: String,
}

#[derive(Serialize)]
struct PredictionCountrySummary {
    slug: String,
    name: String,
}

pub async fn get_world_cup_winner_prediction(
    State(state): State<AppState>,
) -> Result<Json<WinnerPredictionResponse>, ApiError> {
    let response = prediction_snapshot(
        state,
        PolymarketSnapshotRequest {
            event_slug: WINNER_EVENT_SLUG.to_string(),
            country: None,
        },
    )
    .await?;

    Ok(Json(winner_response(response)?))
}

pub async fn get_world_cup_country_prediction(
    State(state): State<AppState>,
    Path(country): Path<String>,
) -> Result<Json<CountryPredictionResponse>, ApiError> {
    let country = country.trim().to_ascii_lowercase();

    if country.is_empty() {
        return Err(ApiError::unsupported_prediction_subject());
    }

    let response = prediction_snapshot(
        state,
        PolymarketSnapshotRequest {
            event_slug: COUNTRY_EVENT_SLUG.to_string(),
            country: Some(country),
        },
    )
    .await?;

    Ok(Json(country_response(response)?))
}

async fn prediction_snapshot(
    state: AppState,
    request: PolymarketSnapshotRequest,
) -> Result<PolymarketSnapshotResponse, ApiError> {
    let client = state
        .dis_client
        .as_ref()
        .ok_or_else(ApiError::prediction_resolver_unavailable)?;

    client
        .get_polymarket_prediction_snapshot(request)
        .await
        .map_err(dis_error_to_api_error)
}

fn winner_response(
    response: PolymarketSnapshotResponse,
) -> Result<WinnerPredictionResponse, ApiError> {
    let PolymarketSnapshotResponse::Winner(response) = response else {
        return Err(ApiError::prediction_resolver_schema_mismatch());
    };

    Ok(winner_prediction_response(response))
}

fn country_response(
    response: PolymarketSnapshotResponse,
) -> Result<CountryPredictionResponse, ApiError> {
    let PolymarketSnapshotResponse::Country(response) = response else {
        return Err(ApiError::prediction_resolver_schema_mismatch());
    };

    Ok(country_prediction_response(response))
}

fn winner_prediction_response(response: PolymarketWinnerSnapshot) -> WinnerPredictionResponse {
    WinnerPredictionResponse {
        ok: true,
        event: response.event_title,
        event_slug: WINNER_EVENT_SLUG.to_string(),
        odds: response.outcomes.into_iter().map(prediction_odd).collect(),
        source: response.source,
        deterministic: response.deterministic,
        captured_at: response.captured_at,
    }
}

fn country_prediction_response(response: PolymarketCountrySnapshot) -> CountryPredictionResponse {
    CountryPredictionResponse {
        ok: true,
        market: response.market,
        country: prediction_country_summary(response.subject),
        probability: response.probability,
        price: response.price,
        currency: response.currency,
        source: response.source,
        deterministic: response.deterministic,
        captured_at: response.captured_at,
    }
}

fn prediction_odd(odd: PolymarketWinnerOutcome) -> PredictionOdd {
    PredictionOdd {
        team: odd.name,
        probability: odd.probability,
        price: odd.price,
        currency: odd.currency,
    }
}

fn prediction_country_summary(country: PolymarketCountrySubject) -> PredictionCountrySummary {
    PredictionCountrySummary {
        slug: country.slug,
        name: country.name,
    }
}

fn dis_error_to_api_error(error: DisClientError) -> ApiError {
    match error {
        DisClientError::UnsupportedSubject => ApiError::unsupported_prediction_subject(),
        DisClientError::ProviderUnavailable => ApiError::prediction_provider_unavailable(),
        DisClientError::ProviderTimeout => ApiError::prediction_provider_timeout(),
        DisClientError::UnsupportedResponseSchema => {
            ApiError::prediction_resolver_schema_mismatch()
        }
        DisClientError::MalformedErrorResponse => {
            ApiError::prediction_resolver_malformed_response()
        }
        DisClientError::ResolverError | DisClientError::UnknownResolverErrorCode(_) => {
            ApiError::prediction_resolver_error()
        }
        DisClientError::Timeout => ApiError::prediction_resolver_timeout(),
        DisClientError::Transport | DisClientError::ResolverUnavailable => {
            ApiError::prediction_resolver_unavailable()
        }
    }
}
