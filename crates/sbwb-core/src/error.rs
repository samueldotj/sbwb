use thiserror::Error;

#[derive(Debug, Error)]
pub enum SbwbError {
    #[error("{0}")]
    InvalidInput(String),
    #[error("source integrity check failed: {0}")]
    Integrity(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("cancelled")]
    Cancelled,
    #[error("timed out after {0:?}")]
    Timeout(std::time::Duration),
    #[error("resource limit: {0}")]
    ResourceLimit(String),
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, SbwbError>;

impl SbwbError {
    pub fn other(msg: impl std::fmt::Display) -> Self {
        SbwbError::Other(msg.to_string())
    }
    pub fn invalid(msg: impl std::fmt::Display) -> Self {
        SbwbError::InvalidInput(msg.to_string())
    }
}
