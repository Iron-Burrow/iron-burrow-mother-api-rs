use serde::{Deserialize, Serialize};

use crate::domain::onchain_time::as_of::AsOf;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum BigwigAsOfDTO {
    Latest,
    Timestamp { timestamp: String },
    BlockNumber { block_number: String },
}

impl From<&AsOf> for BigwigAsOfDTO {
    fn from(as_of: &AsOf) -> Self {
        match as_of {
            AsOf::Latest => Self::Latest,
            AsOf::Timestamp { timestamp } => Self::Timestamp {
                timestamp: timestamp.clone(),
            },
            AsOf::BlockNumber { block_number } => Self::BlockNumber {
                block_number: block_number.clone(),
            },
        }
    }
}
