#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AsOf {
    Latest,
    Timestamp { timestamp: String },
    BlockNumber { block_number: String },
}
