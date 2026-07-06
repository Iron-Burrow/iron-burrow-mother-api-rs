use std::fmt;

use crate::common::rfc3339::{compare_rfc3339, parse_rfc3339};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum OnchainWindow {
    Block(BlockWindow),
    Timestamp(TimestampWindow),
    Lookback(LookbackWindow),
}

/// ****************
/// * Block Window *
/// ****************

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BlockWindow {
    pub(crate) from_block: u64,
    pub(crate) to_block: u64,
}

impl BlockWindow {
    pub(crate) fn new(from_block: u64, to_block: u64) -> Result<Self, InvalidOnchainWindowError> {
        if from_block > to_block {
            return Err(InvalidOnchainWindowError::BlockRange {
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

/// ********************
/// * Timestamp Window *
/// ********************

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TimestampWindow {
    pub(crate) from_timestamp: String,
    pub(crate) to_timestamp: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TimestampField {
    FromTimestamp,
    ToTimestamp,
}

impl TimestampWindow {
    pub(crate) fn new(
        from_timestamp: String,
        to_timestamp: String,
    ) -> Result<Self, InvalidOnchainWindowError> {
        let Some(from) = parse_rfc3339(&from_timestamp) else {
            return Err(InvalidOnchainWindowError::Timestamp {
                field: TimestampField::FromTimestamp,
                value: from_timestamp,
            });
        };

        let Some(to) = parse_rfc3339(&to_timestamp) else {
            return Err(InvalidOnchainWindowError::Timestamp {
                field: TimestampField::ToTimestamp,
                value: to_timestamp,
            });
        };

        if compare_rfc3339(&from, &to).is_gt() {
            return Err(InvalidOnchainWindowError::TimestampRange {
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

/// *******************
/// * Lookback Window *
/// *******************

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LookbackWindow {
    pub(crate) lookback_seconds: u64,
    pub(crate) to: LookbackTarget,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum LookbackTarget {
    Latest,
}

impl LookbackWindow {
    pub(crate) fn latest(lookback_seconds: u64) -> Result<Self, InvalidOnchainWindowError> {
        if lookback_seconds == 0 {
            return Err(InvalidOnchainWindowError::LookbackSeconds { lookback_seconds });
        }

        Ok(Self {
            lookback_seconds,
            to: LookbackTarget::Latest,
        })
    }
}

/// Errors

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub(crate) enum InvalidOnchainWindowError {
    #[error("from_block must be less than or equal to to_block")]
    BlockRange { from_block: u64, to_block: u64 },

    #[error("{field} must be a valid RFC3339 timestamp")]
    Timestamp {
        field: TimestampField,
        value: String,
    },

    #[error("from_timestamp must be less than or equal to to_timestamp")]
    TimestampRange {
        from_timestamp: String,
        to_timestamp: String,
    },

    #[error("lookback_seconds must be greater than zero")]
    LookbackSeconds { lookback_seconds: u64 },
}

impl fmt::Display for TimestampField {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FromTimestamp => write!(f, "from_timestamp"),
            Self::ToTimestamp => write!(f, "to_timestamp"),
        }
    }
}
