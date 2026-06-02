use thiserror::Error;

pub type Result<T> = std::result::Result<T, ObjectiveError>;

#[derive(Debug, Error)]
pub enum ObjectiveError {
    #[error("configuration error: {0}")]
    Config(String),

    #[error("validation failed: {0}")]
    Validation(String),

    #[error("source error: {0}")]
    Source(String),

    #[error("storage error: {0}")]
    Storage(String),

    #[error("message bus error: {0}")]
    MessageBus(String),

    #[error("api error: {0}")]
    Api(String),
}
