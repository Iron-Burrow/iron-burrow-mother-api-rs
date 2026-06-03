use axum::{
    extract::{Path, State},
    Json,
};
use serde::Serialize;

use crate::{
    dis::{
        DisClientError, PolymarketCountrySummary, PolymarketSnapshotOdd, PolymarketSnapshotRequest,
        PolymarketSnapshotResponse,
    },
    error::ApiError,
    state::AppState,
};

const WINNER_EVENT_SLUG: &str = "fifa-world-cup-2026-winner";
const COUNTRY_EVENT_SLUG: &str = "fifa-world-cup-2026-country-probability";

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
    ensure_dis_success(response.ok)?;

    Ok(WinnerPredictionResponse {
        ok: true,
        event: response
            .event
            .ok_or_else(ApiError::prediction_resolver_unavailable)?,
        event_slug: WINNER_EVENT_SLUG.to_string(),
        odds: response
            .odds
            .ok_or_else(ApiError::prediction_resolver_unavailable)?
            .into_iter()
            .map(prediction_odd)
            .collect(),
        source: response.source,
        deterministic: response.deterministic,
        captured_at: response.captured_at,
    })
}

fn country_response(
    response: PolymarketSnapshotResponse,
) -> Result<CountryPredictionResponse, ApiError> {
    ensure_dis_success(response.ok)?;

    Ok(CountryPredictionResponse {
        ok: true,
        market: response
            .market
            .ok_or_else(ApiError::prediction_resolver_unavailable)?,
        country: response
            .country
            .map(prediction_country_summary)
            .ok_or_else(ApiError::prediction_resolver_unavailable)?,
        probability: response
            .probability
            .ok_or_else(ApiError::prediction_resolver_unavailable)?,
        price: response
            .price
            .ok_or_else(ApiError::prediction_resolver_unavailable)?,
        currency: response
            .currency
            .ok_or_else(ApiError::prediction_resolver_unavailable)?,
        source: response.source,
        deterministic: response.deterministic,
        captured_at: response.captured_at,
    })
}

fn ensure_dis_success(ok: bool) -> Result<(), ApiError> {
    if ok {
        Ok(())
    } else {
        Err(ApiError::prediction_resolver_unavailable())
    }
}

fn prediction_odd(odd: PolymarketSnapshotOdd) -> PredictionOdd {
    PredictionOdd {
        team: odd.team,
        probability: odd.probability,
        price: odd.price,
        currency: odd.currency,
    }
}

fn prediction_country_summary(country: PolymarketCountrySummary) -> PredictionCountrySummary {
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
        DisClientError::Transport
        | DisClientError::Timeout
        | DisClientError::ResolverUnavailable
        | DisClientError::MalformedResponse => ApiError::prediction_resolver_unavailable(),
    }
}
