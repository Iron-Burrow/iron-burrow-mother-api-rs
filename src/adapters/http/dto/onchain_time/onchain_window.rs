use std::cmp::Ordering;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

use crate::{
    adapters::http::{error::ApiError, types::JsonObject},
    common::rfc3339::{compare_rfc3339, parse_rfc3339},
    domain::onchain_time::onchain_window::{
        BlockWindow, LookbackTarget, LookbackWindow, OnchainWindow, TimestampWindow,
    },
};

const WINDOW_FIELDS: [&str; 5] = [
    "from_block",
    "to_block",
    "from_timestamp",
    "to_timestamp",
    "lookback_seconds",
];
const LOOKBACK_WINDOW_FIELDS: [&str; 2] = ["lookback_seconds", "to"];

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
#[serde(untagged)]
pub(crate) enum OnchainWindowDTO {
    Block(BlockWindowDTO),
    Timestamp(TimestampWindowDTO),
    Lookback(LookbackWindowDTO),
}

impl From<OnchainWindow> for OnchainWindowDTO {
    fn from(window: OnchainWindow) -> Self {
        match window {
            OnchainWindow::Block(window) => Self::Block(window.into()),
            OnchainWindow::Timestamp(window) => Self::Timestamp(window.into()),
            OnchainWindow::Lookback(window) => Self::Lookback(window.into()),
        }
    }
}

impl TryFrom<OnchainWindowDTO> for OnchainWindow {
    type Error = ApiError;

    fn try_from(dto: OnchainWindowDTO) -> Result<Self, Self::Error> {
        match dto {
            OnchainWindowDTO::Block(window) => Ok(Self::Block(BlockWindow::new(
                window.from_block,
                window.to_block,
            )?)),
            OnchainWindowDTO::Timestamp(window) => Ok(Self::Timestamp(TimestampWindow::new(
                window.from_timestamp,
                window.to_timestamp,
            )?)),
            OnchainWindowDTO::Lookback(window) => Ok(Self::Lookback(LookbackWindow::latest(
                window.lookback_seconds,
            )?)),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct BlockWindowDTO {
    pub from_block: u64,
    pub to_block: u64,
}

impl From<BlockWindow> for BlockWindowDTO {
    fn from(window: BlockWindow) -> Self {
        Self {
            from_block: window.from_block,
            to_block: window.to_block,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct TimestampWindowDTO {
    pub from_timestamp: String,
    pub to_timestamp: String,
}

impl From<TimestampWindow> for TimestampWindowDTO {
    fn from(window: TimestampWindow) -> Self {
        Self {
            from_timestamp: window.from_timestamp,
            to_timestamp: window.to_timestamp,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct LookbackWindowDTO {
    pub lookback_seconds: u64,
    pub to: LookbackTargetDTO,
}

impl From<LookbackWindow> for LookbackWindowDTO {
    fn from(window: LookbackWindow) -> Self {
        Self {
            lookback_seconds: window.lookback_seconds,
            to: window.to.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum LookbackTargetDTO {
    Latest,
}

impl From<LookbackTarget> for LookbackTargetDTO {
    fn from(_: LookbackTarget) -> Self {
        Self::Latest
    }
}

pub(crate) fn validate_window(value: Option<&Value>) -> Result<OnchainWindowDTO, ApiError> {
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

fn validate_block_window(window: &JsonObject) -> Result<OnchainWindowDTO, ApiError> {
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

    Ok(OnchainWindowDTO::Block(BlockWindowDTO {
        from_block,
        to_block,
    }))
}

fn validate_timestamp_window(window: &JsonObject) -> Result<OnchainWindowDTO, ApiError> {
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

    Ok(OnchainWindowDTO::Timestamp(TimestampWindowDTO {
        from_timestamp: from_timestamp.to_string(),
        to_timestamp: to_timestamp.to_string(),
    }))
}

fn validate_lookback_window(window: &JsonObject) -> Result<OnchainWindowDTO, ApiError> {
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

    Ok(OnchainWindowDTO::Lookback(LookbackWindowDTO {
        lookback_seconds,
        to: LookbackTargetDTO::Latest,
    }))
}
