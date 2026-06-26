use std::{cmp::Ordering, collections::HashSet};

use axum::{
    body::Bytes,
    extract::State,
    http::{header::CONTENT_TYPE, HeaderMap},
};
use serde_json::{Map, Value};
use tracing::warn;

use crate::{
    erc20_transfers::{
        Erc20TransferBlockWindow, Erc20TransferCommandDirection, Erc20TransferCommandTokenFilters,
        Erc20TransferCommandWindow, Erc20TransferDirection, Erc20TransferLookbackTarget,
        Erc20TransferLookbackWindow, Erc20TransferSearchCommand, Erc20TransferSearchRequest,
        Erc20TransferSearchWindow, Erc20TransferTimestampWindow, Erc20TransferTokenFilters,
    },
    error::ApiError,
    repositories::global_assets::{BalanceCatalogRow, GlobalAssetRepository},
    state::AppState,
};

const SUPPORTED_NETWORK_SLUG: &str = "eth-mainnet";
const TOP_LEVEL_FIELDS: [&str; 5] = ["network_slug", "address", "direction", "tokens", "window"];
const TOKEN_FIELDS: [&str; 2] = ["asset_slugs", "contract_addresses"];
const WINDOW_FIELDS: [&str; 5] = [
    "from_block",
    "to_block",
    "from_timestamp",
    "to_timestamp",
    "lookback_seconds",
];
const LOOKBACK_WINDOW_FIELDS: [&str; 2] = ["lookback_seconds", "to"];

type JsonObject = Map<String, Value>;

pub async fn search_erc20_transfers(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<(), ApiError> {
    ensure_json_content_type(&headers)?;
    let request = parse_request_body(&body)?;
    let request = validate_request(&request)?;
    let command = build_command(
        request,
        state.asset_repository.clone(),
        state.config.erc20_transfers_max_token_filters,
    )
    .await?;

    extraction_unavailable_placeholder(command).await
}

fn ensure_json_content_type(headers: &HeaderMap) -> Result<(), ApiError> {
    if headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(is_json_content_type)
    {
        Ok(())
    } else {
        Err(ApiError::invalid_json())
    }
}

fn is_json_content_type(value: &str) -> bool {
    let mime = value
        .split(';')
        .next()
        .map(str::trim)
        .unwrap_or_default()
        .to_ascii_lowercase();

    mime.strip_prefix("application/")
        .is_some_and(|subtype| subtype == "json" || subtype.ends_with("+json"))
}

fn parse_request_body(body: &[u8]) -> Result<JsonObject, ApiError> {
    let value: Value = serde_json::from_slice(body).map_err(|_| ApiError::invalid_json())?;
    let Value::Object(object) = value else {
        return Err(ApiError::invalid_json());
    };

    Ok(object)
}

fn validate_request(request: &JsonObject) -> Result<Erc20TransferSearchRequest, ApiError> {
    reject_unknown_fields(request, &TOP_LEVEL_FIELDS)?;

    Ok(Erc20TransferSearchRequest {
        network_slug: validate_network_slug(request.get("network_slug"))?,
        address: validate_address(request.get("address"))?,
        direction: validate_direction(request.get("direction"))?,
        tokens: validate_tokens(request.get("tokens"))?,
        window: validate_window(request.get("window"))?,
    })
}

async fn extraction_unavailable_placeholder(
    _command: Erc20TransferSearchCommand,
) -> Result<(), ApiError> {
    Err(ApiError::extraction_unavailable())
}

async fn build_command(
    request: Erc20TransferSearchRequest,
    repository: Option<GlobalAssetRepository>,
    max_token_filters: u64,
) -> Result<Erc20TransferSearchCommand, ApiError> {
    let tokens = request.tokens.unwrap_or_default();
    let contract_addresses =
        resolve_token_filters(repository, &request.network_slug, tokens).await?;
    enforce_token_filter_limit(&contract_addresses, max_token_filters)?;

    Ok(Erc20TransferSearchCommand {
        network_slug: request.network_slug,
        address: request.address.to_ascii_lowercase(),
        direction: command_direction(request.direction),
        tokens: Erc20TransferCommandTokenFilters { contract_addresses },
        window: command_window(request.window),
    })
}

fn command_direction(direction: Erc20TransferDirection) -> Erc20TransferCommandDirection {
    match direction {
        Erc20TransferDirection::Any => Erc20TransferCommandDirection::Any,
        Erc20TransferDirection::From => Erc20TransferCommandDirection::From,
        Erc20TransferDirection::To => Erc20TransferCommandDirection::To,
    }
}

fn command_window(window: Erc20TransferSearchWindow) -> Erc20TransferCommandWindow {
    match window {
        Erc20TransferSearchWindow::Block(window) => Erc20TransferCommandWindow::Blocks {
            from_block: window.from_block,
            to_block: window.to_block,
        },
        Erc20TransferSearchWindow::Timestamp(window) => Erc20TransferCommandWindow::Timestamps {
            from_timestamp: window.from_timestamp,
            to_timestamp: window.to_timestamp,
        },
        Erc20TransferSearchWindow::Lookback(window) => Erc20TransferCommandWindow::Lookback {
            lookback_seconds: window.lookback_seconds,
        },
    }
}

async fn resolve_token_filters(
    repository: Option<GlobalAssetRepository>,
    network_slug: &str,
    tokens: Erc20TransferTokenFilters,
) -> Result<Vec<String>, ApiError> {
    let mut contract_addresses = Vec::new();
    let mut seen = HashSet::new();

    if !tokens.asset_slugs.is_empty() {
        let repository = repository.ok_or_else(ApiError::asset_contract_mapping_unavailable)?;
        let rows = repository
            .load_balance_catalog_rows(network_slug, &tokens.asset_slugs)
            .await
            .map_err(|error| {
                warn!(%error, "ERC-20 transfer asset catalog lookup failed");
                ApiError::asset_contract_mapping_unavailable()
            })?;
        let resolved_contracts =
            resolve_asset_slug_contracts(network_slug, &tokens.asset_slugs, &rows)?;

        for contract_address in resolved_contracts {
            push_unique_contract_address(&mut contract_addresses, &mut seen, contract_address);
        }
    }

    for contract_address in tokens.contract_addresses {
        push_unique_contract_address(&mut contract_addresses, &mut seen, contract_address);
    }

    Ok(contract_addresses)
}

fn push_unique_contract_address(
    contract_addresses: &mut Vec<String>,
    seen: &mut HashSet<String>,
    contract_address: String,
) {
    let contract_address = contract_address.to_ascii_lowercase();

    if seen.insert(contract_address.clone()) {
        contract_addresses.push(contract_address);
    }
}

fn resolve_asset_slug_contracts(
    requested_network_slug: &str,
    ordered_asset_slugs: &[String],
    rows: &[BalanceCatalogRow],
) -> Result<Vec<String>, ApiError> {
    let mut contract_addresses = Vec::with_capacity(ordered_asset_slugs.len());

    for (index, requested_asset_slug) in ordered_asset_slugs.iter().enumerate() {
        let ordinal = i64::try_from(index + 1).unwrap_or(i64::MAX);
        let matching_rows = rows
            .iter()
            .filter(|row| row.ordinal == ordinal)
            .collect::<Vec<_>>();

        let contract_address = resolve_asset_slug_contract(
            requested_network_slug,
            requested_asset_slug,
            ordinal,
            &matching_rows,
        )?;
        contract_addresses.push(contract_address);
    }

    if rows
        .iter()
        .any(|row| row.ordinal < 1 || row.ordinal > ordered_asset_slugs.len() as i64)
    {
        warn!(
            network_slug = requested_network_slug,
            "ERC-20 transfer catalog lookup returned an unexpected ordinal"
        );
        return Err(ApiError::internal_error());
    }

    Ok(contract_addresses)
}

fn resolve_asset_slug_contract(
    requested_network_slug: &str,
    requested_asset_slug: &str,
    ordinal: i64,
    matching_rows: &[&BalanceCatalogRow],
) -> Result<String, ApiError> {
    if matching_rows.is_empty() {
        warn!(
            network_slug = requested_network_slug,
            asset_slug = requested_asset_slug,
            ordinal,
            "ERC-20 transfer catalog lookup omitted a requested asset row"
        );
        return Err(ApiError::internal_error());
    }

    if matching_rows
        .iter()
        .any(|row| row.requested_asset_slug != requested_asset_slug)
    {
        warn!(
            network_slug = requested_network_slug,
            asset_slug = requested_asset_slug,
            ordinal,
            "ERC-20 transfer catalog lookup returned a mismatched requested asset"
        );
        return Err(ApiError::internal_error());
    }

    let first = matching_rows[0];
    let Some(asset_slug) = first.asset_slug.as_deref() else {
        return Err(ApiError::asset_not_found());
    };

    if asset_slug != requested_asset_slug
        || matching_rows
            .iter()
            .any(|row| row.asset_slug.as_deref() != Some(asset_slug))
    {
        warn!(
            network_slug = requested_network_slug,
            requested_asset_slug,
            resolved_asset_slug = asset_slug,
            "ERC-20 transfer catalog lookup returned a mismatched asset"
        );
        return Err(ApiError::internal_error());
    }

    if first.network_slug.as_deref() != Some(requested_network_slug) {
        return Err(ApiError::asset_not_available_on_network());
    }

    if first.network_family.as_deref() != Some("evm") {
        warn!(
            network_slug = requested_network_slug,
            network_family = first.network_family.as_deref().unwrap_or("<missing>"),
            "ERC-20 transfer catalog lookup returned a non-EVM admitted network"
        );
        return Err(ApiError::internal_error());
    }

    let concrete_rows = matching_rows
        .iter()
        .copied()
        .filter(|row| row.mapping_id.is_some())
        .collect::<Vec<_>>();

    if concrete_rows.is_empty() {
        return Err(ApiError::asset_not_available_on_network());
    }

    if concrete_rows.len() > 1 {
        warn!(
            network_slug = requested_network_slug,
            asset_slug = requested_asset_slug,
            "ERC-20 transfer catalog lookup returned ambiguous active mappings"
        );
        return Err(ApiError::internal_error());
    }

    let row = concrete_rows[0];
    let Some(is_native) = row.is_native else {
        return Err(ApiError::asset_contract_mapping_unavailable());
    };

    if is_native || row.token_standard.as_deref() != Some("erc20") {
        return Err(ApiError::asset_not_erc20_on_network());
    }

    if row.asset_symbol.is_none()
        || row.asset_name.is_none()
        || row
            .decimals
            .and_then(|decimals| u8::try_from(decimals).ok())
            .is_none()
    {
        return Err(ApiError::asset_contract_mapping_unavailable());
    }

    row.deployment_address
        .as_deref()
        .filter(|address| is_evm_address(address))
        .map(str::to_ascii_lowercase)
        .ok_or_else(ApiError::asset_contract_mapping_unavailable)
}

fn enforce_token_filter_limit(
    contract_addresses: &[String],
    max_token_filters: u64,
) -> Result<(), ApiError> {
    let token_filter_count = u64::try_from(contract_addresses.len()).unwrap_or(u64::MAX);

    if token_filter_count > max_token_filters {
        Err(ApiError::too_many_token_filters())
    } else {
        Ok(())
    }
}

fn validate_network_slug(value: Option<&Value>) -> Result<String, ApiError> {
    let Some(Value::String(network_slug)) = value else {
        return Err(ApiError::missing_network_slug());
    };

    if network_slug.trim().is_empty() {
        return Err(ApiError::missing_network_slug());
    }

    if network_slug == SUPPORTED_NETWORK_SLUG {
        Ok(network_slug.clone())
    } else {
        Err(ApiError::transfer_unsupported_network())
    }
}

fn validate_address(value: Option<&Value>) -> Result<String, ApiError> {
    let Some(Value::String(address)) = value else {
        return Err(ApiError::invalid_address());
    };

    if address.trim().is_empty() || !is_evm_address(address) {
        return Err(ApiError::invalid_address());
    }

    Ok(address.to_ascii_lowercase())
}

fn validate_direction(value: Option<&Value>) -> Result<Erc20TransferDirection, ApiError> {
    match value {
        Some(Value::String(direction)) => match direction.as_str() {
            "any" => Ok(Erc20TransferDirection::Any),
            "from" => Ok(Erc20TransferDirection::From),
            "to" => Ok(Erc20TransferDirection::To),
            _ => Err(ApiError::invalid_direction()),
        },
        _ => Err(ApiError::invalid_direction()),
    }
}

fn validate_tokens(value: Option<&Value>) -> Result<Option<Erc20TransferTokenFilters>, ApiError> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Object(tokens)) => {
            reject_unknown_fields(tokens, &TOKEN_FIELDS)?;

            Ok(Some(Erc20TransferTokenFilters {
                asset_slugs: validate_asset_slugs(tokens.get("asset_slugs"))?,
                contract_addresses: validate_contract_addresses(tokens.get("contract_addresses"))?,
            }))
        }
        Some(_) => Err(ApiError::invalid_json()),
    }
}

fn validate_asset_slugs(value: Option<&Value>) -> Result<Vec<String>, ApiError> {
    match value {
        None => Ok(Vec::new()),
        Some(Value::Array(asset_slugs)) => asset_slugs
            .iter()
            .map(|value| match value {
                Value::String(asset_slug) if is_asset_slug(asset_slug) => Ok(asset_slug.clone()),
                _ => Err(ApiError::invalid_asset_slug()),
            })
            .collect(),
        Some(_) => Err(ApiError::invalid_asset_slug()),
    }
}

fn validate_contract_addresses(value: Option<&Value>) -> Result<Vec<String>, ApiError> {
    match value {
        None => Ok(Vec::new()),
        Some(Value::Array(contract_addresses)) => contract_addresses
            .iter()
            .map(|value| match value {
                Value::String(contract_address) if is_evm_address(contract_address) => {
                    Ok(contract_address.to_ascii_lowercase())
                }
                _ => Err(ApiError::invalid_contract_address()),
            })
            .collect(),
        Some(_) => Err(ApiError::invalid_contract_address()),
    }
}

fn validate_window(value: Option<&Value>) -> Result<Erc20TransferSearchWindow, ApiError> {
    let Some(Value::Object(window)) = value else {
        return Err(ApiError::invalid_window());
    };

    reject_unknown_window_fields(window)?;

    let block_fields_present = window.contains_key("from_block") || window.contains_key("to_block");
    let timestamp_fields_present =
        window.contains_key("from_timestamp") || window.contains_key("to_timestamp");
    let lookback_fields_present =
        window.contains_key("lookback_seconds") || window.contains_key("to");
    let window_shape_count = usize::from(block_fields_present)
        + usize::from(timestamp_fields_present)
        + usize::from(lookback_fields_present);

    if window_shape_count != 1 {
        return Err(ApiError::invalid_window());
    }

    if block_fields_present {
        validate_block_window(window)
    } else if timestamp_fields_present {
        validate_timestamp_window(window)
    } else {
        validate_lookback_window(window)
    }
}

fn reject_unknown_window_fields(window: &JsonObject) -> Result<(), ApiError> {
    for field in window.keys() {
        let field = field.as_str();
        if WINDOW_FIELDS.contains(&field) || field == "to" {
            continue;
        }

        return Err(ApiError::unknown_field());
    }

    Ok(())
}

fn validate_block_window(window: &JsonObject) -> Result<Erc20TransferSearchWindow, ApiError> {
    if window.len() != 2 {
        return Err(ApiError::invalid_window());
    }

    let Some(from_block) = window.get("from_block").and_then(Value::as_u64) else {
        return Err(ApiError::invalid_window());
    };
    let Some(to_block) = window.get("to_block").and_then(Value::as_u64) else {
        return Err(ApiError::invalid_window());
    };

    if from_block > to_block {
        return Err(ApiError::invalid_window());
    }

    Ok(Erc20TransferSearchWindow::Block(Erc20TransferBlockWindow {
        from_block,
        to_block,
    }))
}

fn validate_timestamp_window(window: &JsonObject) -> Result<Erc20TransferSearchWindow, ApiError> {
    if window.len() != 2 {
        return Err(ApiError::invalid_window());
    }

    let Some(from_timestamp) = window.get("from_timestamp").and_then(Value::as_str) else {
        return Err(ApiError::invalid_window());
    };
    let Some(to_timestamp) = window.get("to_timestamp").and_then(Value::as_str) else {
        return Err(ApiError::invalid_window());
    };
    let Some(from_parsed) = parse_rfc3339(from_timestamp) else {
        return Err(ApiError::invalid_window());
    };
    let Some(to_parsed) = parse_rfc3339(to_timestamp) else {
        return Err(ApiError::invalid_window());
    };

    if compare_rfc3339(&from_parsed, &to_parsed) == Ordering::Greater {
        return Err(ApiError::invalid_window());
    }

    Ok(Erc20TransferSearchWindow::Timestamp(
        Erc20TransferTimestampWindow {
            from_timestamp: from_timestamp.to_string(),
            to_timestamp: to_timestamp.to_string(),
        },
    ))
}

fn validate_lookback_window(window: &JsonObject) -> Result<Erc20TransferSearchWindow, ApiError> {
    if window.len() != LOOKBACK_WINDOW_FIELDS.len() {
        return Err(ApiError::invalid_window());
    }

    let Some(lookback_seconds) = window.get("lookback_seconds").and_then(Value::as_u64) else {
        return Err(ApiError::invalid_window());
    };
    let Some(to) = window.get("to").and_then(Value::as_str) else {
        return Err(ApiError::invalid_window());
    };

    if lookback_seconds == 0 || to != "latest" {
        return Err(ApiError::invalid_window());
    }

    Ok(Erc20TransferSearchWindow::Lookback(
        Erc20TransferLookbackWindow {
            lookback_seconds,
            to: Erc20TransferLookbackTarget::Latest,
        },
    ))
}

fn reject_unknown_fields(object: &JsonObject, allowed_fields: &[&str]) -> Result<(), ApiError> {
    if object
        .keys()
        .any(|field| !allowed_fields.contains(&field.as_str()))
    {
        return Err(ApiError::unknown_field());
    }

    Ok(())
}

fn is_asset_slug(asset_slug: &str) -> bool {
    if asset_slug.is_empty()
        || asset_slug.starts_with('-')
        || asset_slug.ends_with('-')
        || asset_slug.contains("--")
    {
        return false;
    }

    asset_slug.bytes().all(|character| {
        character.is_ascii_lowercase() || character.is_ascii_digit() || character == b'-'
    })
}

fn is_evm_address(address: &str) -> bool {
    address.len() == 42
        && address.starts_with("0x")
        && address.as_bytes()[2..]
            .iter()
            .all(|character| character.is_ascii_hexdigit())
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ParsedRfc3339 {
    epoch_seconds: i64,
    fraction: String,
}

fn parse_rfc3339(value: &str) -> Option<ParsedRfc3339> {
    let (date, time_and_offset) = value.split_once('T')?;
    if time_and_offset.contains('T') {
        return None;
    }

    let (year, month, day) = parse_date(date)?;
    let (time, offset_seconds) = parse_time_and_offset(time_and_offset)?;
    let days = days_from_civil(year, month, day);
    let epoch_seconds = days
        .checked_mul(86_400)?
        .checked_add(i64::from(time.hour) * 3_600)?
        .checked_add(i64::from(time.minute) * 60)?
        .checked_add(i64::from(time.second))?
        .checked_sub(offset_seconds)?;

    Some(ParsedRfc3339 {
        epoch_seconds,
        fraction: time.fraction,
    })
}

fn compare_rfc3339(left: &ParsedRfc3339, right: &ParsedRfc3339) -> Ordering {
    left.epoch_seconds
        .cmp(&right.epoch_seconds)
        .then_with(|| compare_fractional_seconds(&left.fraction, &right.fraction))
}

fn compare_fractional_seconds(left: &str, right: &str) -> Ordering {
    let len = left.len().max(right.len());

    for index in 0..len {
        let left_digit = left.as_bytes().get(index).copied().unwrap_or(b'0');
        let right_digit = right.as_bytes().get(index).copied().unwrap_or(b'0');
        match left_digit.cmp(&right_digit) {
            Ordering::Equal => continue,
            ordering => return ordering,
        }
    }

    Ordering::Equal
}

fn parse_date(value: &str) -> Option<(i32, u32, u32)> {
    let bytes = value.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }

    let year = parse_ascii_i32(&bytes[0..4])?;
    let month = parse_ascii_u32(&bytes[5..7])?;
    let day = parse_ascii_u32(&bytes[8..10])?;
    if !(1..=12).contains(&month) {
        return None;
    }

    let max_day = days_in_month(year, month);
    if day == 0 || day > max_day {
        return None;
    }

    Some((year, month, day))
}

fn parse_time_and_offset(value: &str) -> Option<(ParsedTime, i64)> {
    let (time, offset_seconds) = if let Some(time) = value.strip_suffix('Z') {
        (time, 0)
    } else {
        let offset_start = value.rfind(|character| character == '+' || character == '-')?;
        let time = &value[..offset_start];
        let offset = &value[offset_start..];
        let offset_seconds = parse_offset(offset)?;
        (time, offset_seconds)
    };

    let parsed_time = parse_time(time)?;

    Some((parsed_time, offset_seconds))
}

fn parse_offset(value: &str) -> Option<i64> {
    let bytes = value.as_bytes();
    if bytes.len() != 6 || bytes[3] != b':' {
        return None;
    }

    let sign = match bytes[0] {
        b'+' => 1_i64,
        b'-' => -1_i64,
        _ => return None,
    };
    let hours = parse_ascii_u32(&bytes[1..3])?;
    let minutes = parse_ascii_u32(&bytes[4..6])?;
    if hours > 23 || minutes > 59 {
        return None;
    }

    Some(sign * (i64::from(hours) * 3_600 + i64::from(minutes) * 60))
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ParsedTime {
    hour: u32,
    minute: u32,
    second: u32,
    fraction: String,
}

fn parse_time(value: &str) -> Option<ParsedTime> {
    let bytes = value.as_bytes();
    if bytes.len() < 8 || bytes[2] != b':' || bytes[5] != b':' {
        return None;
    }

    let hour = parse_ascii_u32(&bytes[0..2])?;
    let minute = parse_ascii_u32(&bytes[3..5])?;
    let seconds_and_fraction = &bytes[6..];
    let (second, fraction) = if let Some(dot_index) = seconds_and_fraction
        .iter()
        .position(|character| *character == b'.')
    {
        let second = &seconds_and_fraction[..dot_index];
        let fraction = &seconds_and_fraction[dot_index + 1..];
        if fraction.is_empty() || !fraction.iter().all(|character| character.is_ascii_digit()) {
            return None;
        }

        (
            parse_ascii_u32(second)?,
            String::from_utf8(fraction.to_vec()).ok()?,
        )
    } else {
        (parse_ascii_u32(seconds_and_fraction)?, String::new())
    };

    if hour > 23 || minute > 59 || second > 59 {
        return None;
    }

    Some(ParsedTime {
        hour,
        minute,
        second,
        fraction,
    })
}

fn parse_ascii_u32(value: &[u8]) -> Option<u32> {
    if value.is_empty() || !value.iter().all(|character| character.is_ascii_digit()) {
        return None;
    }

    std::str::from_utf8(value).ok()?.parse::<u32>().ok()
}

fn parse_ascii_i32(value: &[u8]) -> Option<i32> {
    if value.is_empty() || !value.iter().all(|character| character.is_ascii_digit()) {
        return None;
    }

    std::str::from_utf8(value).ok()?.parse::<i32>().ok()
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let adjusted_year = year - i32::from(month <= 2);
    let era = if adjusted_year >= 0 {
        adjusted_year / 400
    } else {
        (adjusted_year - 399) / 400
    };
    let year_of_era = adjusted_year - era * 400;
    let month_prime = i32::try_from(month).unwrap() + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * month_prime + 2) / 5 + i32::try_from(day).unwrap() - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;

    i64::from(era) * 146_097 + i64::from(day_of_era) - 719_468
}

#[cfg(test)]
mod tests {
    use axum::{
        body::Body,
        http::{Request, StatusCode},
        response::IntoResponse,
        Router,
    };
    use serde_json::json;
    use tower::ServiceExt;

    use super::*;
    use crate::{
        app::create_app,
        config::Config,
        repositories::global_assets::{demo_assets, GlobalAssetRepository},
    };

    const TEST_MAX_TOKEN_FILTERS: u64 = 20;

    #[test]
    fn validation_accepts_supported_window_shapes() {
        let block = validate_request(&json_object(valid_request_body())).unwrap();
        assert!(matches!(
            block.window,
            Erc20TransferSearchWindow::Block(Erc20TransferBlockWindow {
                from_block: 18_600_000,
                to_block: 18_600_500,
            })
        ));

        let mut timestamp_body = valid_request_body();
        timestamp_body["window"] = json!({
            "from_timestamp": "2026-06-25T00:00:00Z",
            "to_timestamp": "2026-06-25T01:00:00Z"
        });
        let timestamp = validate_request(&json_object(timestamp_body)).unwrap();
        assert!(matches!(
            timestamp.window,
            Erc20TransferSearchWindow::Timestamp(_)
        ));

        let mut lookback_body = valid_request_body();
        lookback_body["window"] = json!({
            "lookback_seconds": 600,
            "to": "latest"
        });
        let lookback = validate_request(&json_object(lookback_body)).unwrap();
        assert!(matches!(
            lookback.window,
            Erc20TransferSearchWindow::Lookback(_)
        ));
    }

    #[test]
    fn validation_accepts_omitted_null_and_empty_tokens() {
        let omitted_tokens = validate_request(&json_object(body_without_tokens())).unwrap();
        assert_eq!(
            omitted_tokens.tokens.unwrap_or_default(),
            Erc20TransferTokenFilters::default()
        );

        let mut null_tokens_body = valid_request_body();
        null_tokens_body["tokens"] = Value::Null;
        let null_tokens = validate_request(&json_object(null_tokens_body)).unwrap();
        assert_eq!(
            null_tokens.tokens.unwrap_or_default(),
            Erc20TransferTokenFilters::default()
        );

        let mut empty_tokens_body = valid_request_body();
        empty_tokens_body["tokens"] = json!({});
        let empty_tokens = validate_request(&json_object(empty_tokens_body)).unwrap();
        assert_eq!(
            empty_tokens.tokens.unwrap_or_default(),
            Erc20TransferTokenFilters::default()
        );
    }

    #[test]
    fn validation_normalizes_explicit_contract_addresses_to_lowercase() {
        let mut body = valid_request_body();
        body["tokens"]["contract_addresses"] =
            json!(["0xA0B86991C6218B36C1D19D4A2E9EB0CE3606EB48"]);

        let request = validate_request(&json_object(body)).unwrap();

        assert_eq!(
            request.tokens.unwrap().contract_addresses,
            ["0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"]
        );
    }

    #[tokio::test]
    async fn command_resolves_usdc_and_dedupes_duplicate_explicit_address() {
        let mut body = valid_request_body();
        body["address"] = json!("0xABC0000000000000000000000000000000000000");
        body["tokens"]["contract_addresses"] =
            json!(["0xA0B86991C6218B36C1D19D4A2E9EB0CE3606EB48"]);

        let command = command_from_body(body, TEST_MAX_TOKEN_FILTERS).await;

        assert_eq!(
            command,
            Erc20TransferSearchCommand {
                network_slug: "eth-mainnet".to_string(),
                address: "0xabc0000000000000000000000000000000000000".to_string(),
                direction: Erc20TransferCommandDirection::Any,
                tokens: Erc20TransferCommandTokenFilters {
                    contract_addresses: vec![
                        "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48".to_string()
                    ],
                },
                window: Erc20TransferCommandWindow::Blocks {
                    from_block: 18_600_000,
                    to_block: 18_600_500,
                },
            }
        );
    }

    #[test]
    fn validation_accepts_minimal_asset_contract_and_mixed_token_filter_shapes() {
        let cases = [
            (body_without_tokens(), Erc20TransferTokenFilters::default()),
            (
                request_with_tokens(json!({
                    "asset_slugs": ["usdc", "wrapped-ether"]
                })),
                Erc20TransferTokenFilters {
                    asset_slugs: vec!["usdc".to_string(), "wrapped-ether".to_string()],
                    contract_addresses: Vec::new(),
                },
            ),
            (
                request_with_tokens(json!({
                    "contract_addresses": [
                        "0x1111111111111111111111111111111111111111",
                        "0xA0B86991C6218B36C1D19D4A2E9EB0CE3606EB48"
                    ]
                })),
                Erc20TransferTokenFilters {
                    asset_slugs: Vec::new(),
                    contract_addresses: vec![
                        "0x1111111111111111111111111111111111111111".to_string(),
                        "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48".to_string(),
                    ],
                },
            ),
            (
                valid_request_body(),
                Erc20TransferTokenFilters {
                    asset_slugs: vec!["usdc".to_string()],
                    contract_addresses: vec![
                        "0x1111111111111111111111111111111111111111".to_string()
                    ],
                },
            ),
        ];

        for (body, expected_tokens) in cases {
            let request = validate_request(&json_object(body)).unwrap();

            assert_eq!(request.network_slug, "eth-mainnet");
            assert_eq!(
                request.address,
                "0xabc0000000000000000000000000000000000000"
            );
            assert_eq!(request.direction, Erc20TransferDirection::Any);
            assert_eq!(request.tokens.unwrap_or_default(), expected_tokens);
            assert!(matches!(
                request.window,
                Erc20TransferSearchWindow::Block(Erc20TransferBlockWindow {
                    from_block: 18_600_000,
                    to_block: 18_600_500,
                })
            ));
        }
    }

    #[tokio::test]
    async fn command_enforces_final_token_filter_limit_after_dedupe() {
        let body = request_with_tokens(json!({
            "asset_slugs": ["usdc"],
            "contract_addresses": ["0xA0B86991C6218B36C1D19D4A2E9EB0CE3606EB48"]
        }));
        let command = command_from_body(body, 1).await;
        assert_eq!(
            command.tokens.contract_addresses,
            ["0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"]
        );

        let body = request_with_tokens(json!({
            "asset_slugs": ["usdc"],
            "contract_addresses": ["0x1111111111111111111111111111111111111111"]
        }));
        let request = validate_request(&json_object(body)).unwrap();
        let error = build_command(request, Some(repository()), 1)
            .await
            .unwrap_err();

        assert_api_error(
            error,
            StatusCode::UNPROCESSABLE_ENTITY,
            "too_many_token_filters",
        )
        .await;
    }

    #[tokio::test]
    async fn route_is_absent_when_disabled() {
        let response = transfers_app(Config::default())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/erc20-transfers/search")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&valid_request_body()).unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn route_is_present_when_enabled() {
        let response = transfers_app(enabled_config())
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/erc20-transfers/search")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    }

    #[tokio::test]
    async fn valid_request_returns_extraction_unavailable_placeholder() {
        let (status, response) = post_json(
            transfers_app(enabled_config()),
            "/v1/erc20-transfers/search",
            valid_request_body(),
        )
        .await;

        assert_public_error(
            status,
            &response,
            StatusCode::SERVICE_UNAVAILABLE,
            "extraction_unavailable",
        );
    }

    #[tokio::test]
    async fn request_without_asset_slugs_does_not_require_catalog_or_bigwig_to_exist() {
        let (status, response) = post_json(
            transfers_app_without_repository(enabled_config()),
            "/v1/erc20-transfers/search",
            request_with_tokens(json!({
                "contract_addresses": ["0x1111111111111111111111111111111111111111"]
            })),
        )
        .await;

        assert_public_error(
            status,
            &response,
            StatusCode::SERVICE_UNAVAILABLE,
            "extraction_unavailable",
        );
    }

    #[tokio::test]
    async fn request_with_asset_slugs_requires_catalog() {
        let (status, response) = post_json(
            transfers_app_without_repository(enabled_config()),
            "/v1/erc20-transfers/search",
            request_with_tokens(json!({"asset_slugs": ["usdc"]})),
        )
        .await;

        assert_public_error(
            status,
            &response,
            StatusCode::SERVICE_UNAVAILABLE,
            "asset_contract_mapping_unavailable",
        );
        assert_ne!(response["error"]["code"], "extraction_unavailable");
    }

    #[tokio::test]
    async fn native_asset_slug_rejects_whole_request_before_extraction() {
        let (status, response) = post_json(
            transfers_app(enabled_config()),
            "/v1/erc20-transfers/search",
            request_with_tokens(json!({"asset_slugs": ["ethereum"]})),
        )
        .await;

        assert_public_error(
            status,
            &response,
            StatusCode::UNPROCESSABLE_ENTITY,
            "asset_not_erc20_on_network",
        );
        assert_ne!(response["error"]["code"], "extraction_unavailable");
    }

    #[tokio::test]
    async fn unknown_asset_slug_rejects_whole_request_before_extraction() {
        let (status, response) = post_json(
            transfers_app(enabled_config()),
            "/v1/erc20-transfers/search",
            request_with_tokens(json!({"asset_slugs": ["missing-but-syntactically-valid"]})),
        )
        .await;

        assert_public_error(status, &response, StatusCode::NOT_FOUND, "asset_not_found");
        assert_ne!(response["error"]["code"], "extraction_unavailable");
    }

    #[tokio::test]
    async fn globally_known_asset_unavailable_on_network_rejects_whole_request() {
        let (status, response) = post_json(
            transfers_app(enabled_config()),
            "/v1/erc20-transfers/search",
            request_with_tokens(json!({"asset_slugs": ["mantle"]})),
        )
        .await;

        assert_public_error(
            status,
            &response,
            StatusCode::UNPROCESSABLE_ENTITY,
            "asset_not_available_on_network",
        );
        assert_ne!(response["error"]["code"], "extraction_unavailable");
    }

    #[tokio::test]
    async fn mixed_valid_and_invalid_asset_slug_rejects_whole_request() {
        let (status, response) = post_json(
            transfers_app(enabled_config()),
            "/v1/erc20-transfers/search",
            request_with_tokens(json!({
                "asset_slugs": ["usdc", "ethereum"],
                "contract_addresses": ["0x1111111111111111111111111111111111111111"]
            })),
        )
        .await;

        assert_public_error(
            status,
            &response,
            StatusCode::UNPROCESSABLE_ENTITY,
            "asset_not_erc20_on_network",
        );
        assert_ne!(response["error"]["code"], "extraction_unavailable");
    }

    #[tokio::test]
    async fn duplicate_explicit_and_resolved_address_dedupes_before_limit() {
        let (status, response) = post_json(
            transfers_app(Config {
                erc20_transfers_enabled: true,
                erc20_transfers_max_token_filters: 1,
                bigwig_max_contract_addresses: 20,
                ..Config::default()
            }),
            "/v1/erc20-transfers/search",
            request_with_tokens(json!({
                "asset_slugs": ["usdc"],
                "contract_addresses": ["0xA0B86991C6218B36C1D19D4A2E9EB0CE3606EB48"]
            })),
        )
        .await;

        assert_public_error(
            status,
            &response,
            StatusCode::SERVICE_UNAVAILABLE,
            "extraction_unavailable",
        );
    }

    #[tokio::test]
    async fn duplicate_explicit_contract_addresses_dedupe_before_limit() {
        let (status, response) = post_json(
            transfers_app_without_repository(Config {
                erc20_transfers_enabled: true,
                erc20_transfers_max_token_filters: 1,
                bigwig_max_contract_addresses: 20,
                ..Config::default()
            }),
            "/v1/erc20-transfers/search",
            request_with_tokens(json!({
                "contract_addresses": [
                    "0xA0B86991C6218B36C1D19D4A2E9EB0CE3606EB48",
                    "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
                ]
            })),
        )
        .await;

        assert_public_error(
            status,
            &response,
            StatusCode::SERVICE_UNAVAILABLE,
            "extraction_unavailable",
        );
    }

    #[tokio::test]
    async fn validation_failures_do_not_require_catalog_or_bigwig_to_exist() {
        let mut body = valid_request_body();
        body["tokens"]["asset_slugs"] = json!(["USDC"]);

        let (status, response) = post_json(
            transfers_app_without_repository(enabled_config()),
            "/v1/erc20-transfers/search",
            body,
        )
        .await;

        assert_public_error(
            status,
            &response,
            StatusCode::BAD_REQUEST,
            "invalid_asset_slug",
        );
    }

    #[tokio::test]
    async fn malformed_json_raw_body_returns_invalid_json() {
        let (status, response) = post_raw(
            transfers_app(enabled_config()),
            "/v1/erc20-transfers/search",
            Some("application/json"),
            br#"{"network_slug":"eth-mainnet""#.to_vec(),
        )
        .await;

        assert_public_error(status, &response, StatusCode::BAD_REQUEST, "invalid_json");
    }

    #[tokio::test]
    async fn missing_or_non_json_content_type_returns_invalid_json() {
        for content_type in [None, Some("text/plain")] {
            let (status, response) = post_raw(
                transfers_app(enabled_config()),
                "/v1/erc20-transfers/search",
                content_type,
                serde_json::to_vec(&valid_request_body()).unwrap(),
            )
            .await;

            assert_public_error(status, &response, StatusCode::BAD_REQUEST, "invalid_json");
        }
    }

    #[tokio::test]
    async fn invalid_requests_return_stable_public_codes() {
        let app = transfers_app(enabled_config());
        let cases = [
            (
                Some("application/json"),
                serde_json::to_vec(&json!([])).unwrap(),
                StatusCode::BAD_REQUEST,
                "invalid_json",
            ),
            (
                Some("application/json"),
                serde_json::to_vec(&{
                    let mut body = valid_request_body();
                    body["future"] = json!(true);
                    body
                })
                .unwrap(),
                StatusCode::BAD_REQUEST,
                "unknown_field",
            ),
            (
                Some("application/json"),
                serde_json::to_vec(&{
                    let mut body = valid_request_body();
                    body["tokens"]["symbol"] = json!("USDC");
                    body
                })
                .unwrap(),
                StatusCode::BAD_REQUEST,
                "unknown_field",
            ),
            (
                Some("application/json"),
                serde_json::to_vec(&{
                    let mut body = valid_request_body();
                    body["window"]["cursor"] = json!("next");
                    body
                })
                .unwrap(),
                StatusCode::BAD_REQUEST,
                "unknown_field",
            ),
            (
                Some("application/json"),
                serde_json::to_vec(&{
                    let mut body = valid_request_body();
                    body.as_object_mut().unwrap().remove("network_slug");
                    body
                })
                .unwrap(),
                StatusCode::BAD_REQUEST,
                "missing_network_slug",
            ),
            (
                Some("application/json"),
                serde_json::to_vec(&{
                    let mut body = valid_request_body();
                    body["network_slug"] = json!("");
                    body
                })
                .unwrap(),
                StatusCode::BAD_REQUEST,
                "missing_network_slug",
            ),
            (
                Some("application/json"),
                serde_json::to_vec(&{
                    let mut body = valid_request_body();
                    body["network_slug"] = json!(null);
                    body
                })
                .unwrap(),
                StatusCode::BAD_REQUEST,
                "missing_network_slug",
            ),
            (
                Some("application/json"),
                serde_json::to_vec(&{
                    let mut body = valid_request_body();
                    body["network_slug"] = json!("ETH-MAINNET");
                    body
                })
                .unwrap(),
                StatusCode::NOT_FOUND,
                "unsupported_network",
            ),
            (
                Some("application/json"),
                serde_json::to_vec(&{
                    let mut body = valid_request_body();
                    body["network_slug"] = json!("base-mainnet");
                    body
                })
                .unwrap(),
                StatusCode::NOT_FOUND,
                "unsupported_network",
            ),
            (
                Some("application/json"),
                serde_json::to_vec(&{
                    let mut body = valid_request_body();
                    body["address"] = json!("0x1234");
                    body
                })
                .unwrap(),
                StatusCode::BAD_REQUEST,
                "invalid_address",
            ),
            (
                Some("application/json"),
                serde_json::to_vec(&{
                    let mut body = valid_request_body();
                    body["direction"] = json!("ANY");
                    body
                })
                .unwrap(),
                StatusCode::BAD_REQUEST,
                "invalid_direction",
            ),
            (
                Some("application/json"),
                serde_json::to_vec(&{
                    let mut body = valid_request_body();
                    body["direction"] = json!("sideways");
                    body
                })
                .unwrap(),
                StatusCode::BAD_REQUEST,
                "invalid_direction",
            ),
            (
                Some("application/json"),
                serde_json::to_vec(&{
                    let mut body = valid_request_body();
                    body.as_object_mut().unwrap().remove("window");
                    body
                })
                .unwrap(),
                StatusCode::BAD_REQUEST,
                "invalid_window",
            ),
            (
                Some("application/json"),
                serde_json::to_vec(&{
                    let mut body = valid_request_body();
                    body["window"] = json!({});
                    body
                })
                .unwrap(),
                StatusCode::BAD_REQUEST,
                "invalid_window",
            ),
            (
                Some("application/json"),
                serde_json::to_vec(&{
                    let mut body = valid_request_body();
                    body["window"] = json!({
                        "from_block": "18600000",
                        "to_block": 18_600_500
                    });
                    body
                })
                .unwrap(),
                StatusCode::BAD_REQUEST,
                "invalid_window",
            ),
            (
                Some("application/json"),
                serde_json::to_vec(&{
                    let mut body = valid_request_body();
                    body["window"] = json!({
                        "from_block": 18_600_500,
                        "to_block": 18_600_000
                    });
                    body
                })
                .unwrap(),
                StatusCode::BAD_REQUEST,
                "invalid_window",
            ),
            (
                Some("application/json"),
                serde_json::to_vec(&{
                    let mut body = valid_request_body();
                    body["window"] = json!({
                        "from_timestamp": "not-a-timestamp",
                        "to_timestamp": "2026-06-25T01:00:00Z"
                    });
                    body
                })
                .unwrap(),
                StatusCode::BAD_REQUEST,
                "invalid_window",
            ),
            (
                Some("application/json"),
                serde_json::to_vec(&{
                    let mut body = valid_request_body();
                    body["window"] = json!({
                        "from_timestamp": "2026-06-25T02:00:00Z",
                        "to_timestamp": "2026-06-25T01:00:00Z"
                    });
                    body
                })
                .unwrap(),
                StatusCode::BAD_REQUEST,
                "invalid_window",
            ),
            (
                Some("application/json"),
                serde_json::to_vec(&{
                    let mut body = valid_request_body();
                    body["window"] = json!({
                        "from_block": 18_600_000,
                        "to_block": 18_600_500,
                        "from_timestamp": "2026-06-25T00:00:00Z",
                        "to_timestamp": "2026-06-25T01:00:00Z"
                    });
                    body
                })
                .unwrap(),
                StatusCode::BAD_REQUEST,
                "invalid_window",
            ),
            (
                Some("application/json"),
                serde_json::to_vec(&{
                    let mut body = valid_request_body();
                    body["window"] = json!({
                        "lookback_seconds": 0,
                        "to": "latest"
                    });
                    body
                })
                .unwrap(),
                StatusCode::BAD_REQUEST,
                "invalid_window",
            ),
            (
                Some("application/json"),
                serde_json::to_vec(&{
                    let mut body = valid_request_body();
                    body["tokens"]["asset_slugs"] = json!(["USDC"]);
                    body
                })
                .unwrap(),
                StatusCode::BAD_REQUEST,
                "invalid_asset_slug",
            ),
            (
                Some("application/json"),
                serde_json::to_vec(&{
                    let mut body = valid_request_body();
                    body["tokens"]["asset_slugs"] = json!([""]);
                    body
                })
                .unwrap(),
                StatusCode::BAD_REQUEST,
                "invalid_asset_slug",
            ),
            (
                Some("application/json"),
                serde_json::to_vec(&{
                    let mut body = valid_request_body();
                    body["tokens"]["asset_slugs"] = json!(null);
                    body
                })
                .unwrap(),
                StatusCode::BAD_REQUEST,
                "invalid_asset_slug",
            ),
            (
                Some("application/json"),
                serde_json::to_vec(&{
                    let mut body = valid_request_body();
                    body["tokens"]["contract_addresses"] = json!(["0x1234"]);
                    body
                })
                .unwrap(),
                StatusCode::BAD_REQUEST,
                "invalid_contract_address",
            ),
            (
                Some("application/json"),
                serde_json::to_vec(&{
                    let mut body = valid_request_body();
                    body["tokens"]["contract_addresses"] = json!([""]);
                    body
                })
                .unwrap(),
                StatusCode::BAD_REQUEST,
                "invalid_contract_address",
            ),
            (
                Some("application/json"),
                serde_json::to_vec(&{
                    let mut body = valid_request_body();
                    body["tokens"]["contract_addresses"] = json!(null);
                    body
                })
                .unwrap(),
                StatusCode::BAD_REQUEST,
                "invalid_contract_address",
            ),
        ];

        for (content_type, body, expected_status, expected_code) in cases {
            let (status, response) = post_raw(
                app.clone(),
                "/v1/erc20-transfers/search",
                content_type,
                body,
            )
            .await;

            assert_public_error(status, &response, expected_status, expected_code);
            assert_ne!(response["error"]["code"], "extraction_unavailable");
        }
    }

    #[tokio::test]
    async fn too_many_token_filters_uses_configured_public_limit() {
        let (status, response) = post_json(
            transfers_app(Config {
                erc20_transfers_enabled: true,
                erc20_transfers_max_token_filters: 1,
                bigwig_max_contract_addresses: 20,
                ..Config::default()
            }),
            "/v1/erc20-transfers/search",
            request_with_tokens(json!({
                "asset_slugs": ["usdc"],
                "contract_addresses": ["0x1111111111111111111111111111111111111111"]
            })),
        )
        .await;

        assert_public_error(
            status,
            &response,
            StatusCode::UNPROCESSABLE_ENTITY,
            "too_many_token_filters",
        );
    }

    #[tokio::test]
    async fn command_token_filters_have_no_asset_slug_field() {
        let command = command_from_body(valid_request_body(), TEST_MAX_TOKEN_FILTERS).await;
        let debug = format!("{:?}", command.tokens);

        assert!(!debug.contains("asset_slugs"));
        assert_eq!(
            command.tokens.contract_addresses,
            [
                "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
                "0x1111111111111111111111111111111111111111",
            ]
        );
    }

    #[test]
    fn rfc3339_parser_accepts_offsets_and_rejects_invalid_values() {
        assert_eq!(
            parse_rfc3339("2026-06-25T00:00:00Z"),
            parse_rfc3339("2026-06-24T19:00:00-05:00")
        );
        assert!(parse_rfc3339("2026-02-29T00:00:00Z").is_none());
        assert!(parse_rfc3339("2026-06-25 00:00:00Z").is_none());
    }

    fn enabled_config() -> Config {
        Config {
            erc20_transfers_enabled: true,
            ..Config::default()
        }
    }

    fn transfers_app(config: Config) -> Router {
        create_app(AppState::with_asset_repository(config, repository()))
    }

    fn transfers_app_without_repository(config: Config) -> Router {
        create_app(AppState::new(config))
    }

    fn valid_request_body() -> Value {
        json!({
            "network_slug": "eth-mainnet",
            "address": "0xabc0000000000000000000000000000000000000",
            "direction": "any",
            "tokens": {
                "asset_slugs": ["usdc"],
                "contract_addresses": ["0x1111111111111111111111111111111111111111"]
            },
            "window": {
                "from_block": 18600000,
                "to_block": 18600500
            }
        })
    }

    fn body_without_tokens() -> Value {
        let mut body = valid_request_body();
        body.as_object_mut().unwrap().remove("tokens");
        body
    }

    fn request_with_tokens(tokens: Value) -> Value {
        let mut body = body_without_tokens();
        body["tokens"] = tokens;
        body
    }

    fn repository() -> GlobalAssetRepository {
        GlobalAssetRepository::in_memory(demo_assets())
    }

    async fn command_from_body(body: Value, max_token_filters: u64) -> Erc20TransferSearchCommand {
        let request = validate_request(&json_object(body)).unwrap();
        build_command(request, Some(repository()), max_token_filters)
            .await
            .unwrap()
    }

    fn json_object(value: Value) -> JsonObject {
        match value {
            Value::Object(object) => object,
            other => panic!("expected JSON object, got {other:?}"),
        }
    }

    async fn post_json(app: Router, uri: &str, body: Value) -> (StatusCode, Value) {
        post_raw(
            app,
            uri,
            Some("application/json"),
            serde_json::to_vec(&body).unwrap(),
        )
        .await
    }

    async fn post_raw(
        app: Router,
        uri: &str,
        content_type: Option<&str>,
        body: Vec<u8>,
    ) -> (StatusCode, Value) {
        let mut request = Request::builder().method("POST").uri(uri);
        if let Some(content_type) = content_type {
            request = request.header("content-type", content_type);
        }

        let response = app
            .oneshot(request.body(Body::from(body)).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json = serde_json::from_slice(&body).unwrap();

        (status, json)
    }

    fn assert_public_error(
        status: StatusCode,
        response: &Value,
        expected_status: StatusCode,
        expected_code: &str,
    ) {
        assert_eq!(status, expected_status);
        assert_eq!(response["ok"], false);
        assert_eq!(response["error"]["code"], expected_code);
        assert!(response["error"]["message"]
            .as_str()
            .is_some_and(|message| !message.is_empty()));
    }

    async fn assert_api_error(error: ApiError, expected_status: StatusCode, expected_code: &str) {
        let response = error.into_response();
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();

        assert_public_error(status, &json, expected_status, expected_code);
    }

    #[tokio::test]
    async fn transfer_unsupported_network_uses_not_found_status() {
        let response = ApiError::transfer_unsupported_network().into_response();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
