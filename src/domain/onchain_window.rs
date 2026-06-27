use std::fmt;

use crate::common::rfc3339::{compare_rfc3339, parse_rfc3339};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OnchainWindow {
    Block(BlockWindow),
    Timestamp(TimestampWindow),
    Lookback(LookbackWindow),
}

/// Block Window

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BlockWindow {
    pub from_block: u64,
    pub to_block: u64,
}

impl BlockWindow {
    pub fn new(from_block: u64, to_block: u64) -> Result<Self, OnchainWindowError> {
        if from_block > to_block {
            return Err(OnchainWindowError::InvalidBlockRange {
                from_block,
                to_block,
            });
        }

        Ok(Self {
            from_block,
            to_block,
        })
    }
}

/// Timestamp Window

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TimestampWindow {
    pub from_timestamp: String,
    pub to_timestamp: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimestampField {
    FromTimestamp,
    ToTimestamp,
}

impl TimestampWindow {
    pub fn new(from_timestamp: String, to_timestamp: String) -> Result<Self, OnchainWindowError> {
        let Some(from) = parse_rfc3339(&from_timestamp) else {
            return Err(OnchainWindowError::InvalidTimestamp {
                field: TimestampField::FromTimestamp,
                value: from_timestamp,
            });
        };

        let Some(to) = parse_rfc3339(&to_timestamp) else {
            return Err(OnchainWindowError::InvalidTimestamp {
                field: TimestampField::ToTimestamp,
                value: to_timestamp,
            });
        };

        if compare_rfc3339(&from, &to).is_gt() {
            return Err(OnchainWindowError::InvalidTimestampRange {
                from_timestamp,
                to_timestamp,
            });
        }

        Ok(Self {
            from_timestamp,
            to_timestamp,
        })
    }
}

/// Lookback Window

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LookbackWindow {
    pub lookback_seconds: u64,
    pub to: LookbackTarget,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LookbackTarget {
    Latest,
}

impl LookbackWindow {
    pub fn latest(lookback_seconds: u64) -> Result<Self, OnchainWindowError> {
        if lookback_seconds == 0 {
            return Err(OnchainWindowError::InvalidLookbackSeconds { lookback_seconds });
        }

        Ok(Self {
            lookback_seconds,
            to: LookbackTarget::Latest,
        })
    }
}

/// Errors

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum OnchainWindowError {
    #[error("from_block must be less than or equal to to_block")]
    InvalidBlockRange { from_block: u64, to_block: u64 },

    #[error("{field} must be a valid RFC3339 timestamp")]
    InvalidTimestamp {
        field: TimestampField,
        value: String,
    },

    #[error("from_timestamp must be less than or equal to to_timestamp")]
    InvalidTimestampRange {
        from_timestamp: String,
        to_timestamp: String,
    },

    #[error("lookback_seconds must be greater than zero")]
    InvalidLookbackSeconds { lookback_seconds: u64 },
}

impl fmt::Display for TimestampField {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FromTimestamp => write!(f, "from_timestamp"),
            Self::ToTimestamp => write!(f, "to_timestamp"),
        }
    }
}
