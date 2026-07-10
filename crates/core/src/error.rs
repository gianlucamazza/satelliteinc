use thiserror::Error;

#[derive(Debug, Error)]
pub enum SimError {
    #[error("invalid action: {0}")]
    InvalidAction(String),
    #[error("content error: {0}")]
    Content(String),
    #[error("insufficient funds: need ${0:.0}")]
    InsufficientFunds(f64),
    #[error("spacecraft not in correct phase")]
    WrongPhase,
}
