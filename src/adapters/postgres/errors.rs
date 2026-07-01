#[derive(Debug)]
pub struct RepositoryError {
    source: sqlx::Error,
}

impl RepositoryError {
    pub(super) fn new(source: sqlx::Error) -> Self {
        Self { source }
    }

    pub(super) fn protocol(message: impl Into<String>) -> Self {
        Self {
            source: sqlx::Error::Protocol(message.into()),
        }
    }

    #[cfg(test)]
    pub(crate) fn test() -> Self {
        Self {
            source: sqlx::Error::PoolClosed,
        }
    }
}

impl std::fmt::Display for RepositoryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "postgres repository error: {}", self.source)
    }
}

impl std::error::Error for RepositoryError {}
