//! The port: one question shape in, one answer shape out.

use async_trait::async_trait;
use bisa_core::{DecisionContractError, DecisionProviderKind, DecisionRequest, DecisionResponse};
use std::time::Duration;

/// Who answers, as a record and a settings screen name it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderDescriptor {
    pub kind: DecisionProviderKind,
    /// The model asked — a harness's model id, an agent's id, a System One
    /// model's name. What a record says when no answer came back to name one.
    pub model: String,
}

impl ProviderDescriptor {
    /// Whether the probabilities it reports are a calibrated model's.
    pub fn calibrated(&self) -> bool {
        self.kind.is_calibrated()
    }
}

/// A model that answers typed questions against a state.
#[async_trait]
pub trait DecisionProvider: Send + Sync {
    fn descriptor(&self) -> ProviderDescriptor;

    /// Answer every question of `request`, within `deadline`.
    async fn decide(
        &self,
        request: &DecisionRequest,
        deadline: Duration,
    ) -> Result<DecisionResponse, ProviderError>;
}

/// Why no answer came back. Never carries a credential.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum ProviderError {
    /// The provider cannot be asked as it is set up: no endpoint, no key, a
    /// host the node refuses. Asking again changes nothing.
    #[error("the decision provider is not set up: {0}")]
    Misconfigured(String),
    /// The service refused the request itself — the key, or the body.
    #[error("the decision provider refused the request ({status}): {message}")]
    Refused { status: u16, message: String },
    /// The service is there and cannot answer now.
    #[error("the decision provider is busy ({status})")]
    Busy {
        status: u16,
        retry_after: Option<Duration>,
    },
    #[error("the decision provider cannot be reached: {0}")]
    Unreachable(String),
    /// An answer came back and is not the contract's shape.
    #[error("the decision provider's answer cannot be read: {0}")]
    Unreadable(String),
    #[error("the decision provider's answer breaks the contract: {0}")]
    Contract(#[from] DecisionContractError),
    #[error("the decision provider did not answer in time")]
    TimedOut,
}

impl ProviderError {
    /// Whether asking again may get an answer.
    pub fn is_transient(&self) -> bool {
        match self {
            ProviderError::Busy { .. }
            | ProviderError::Unreachable(_)
            | ProviderError::Unreadable(_)
            | ProviderError::Contract(_) => true,
            ProviderError::Misconfigured(_)
            | ProviderError::Refused { .. }
            | ProviderError::TimedOut => false,
        }
    }

    /// How long the service asked to be left alone, when it said.
    pub fn retry_after(&self) -> Option<Duration> {
        match self {
            ProviderError::Busy { retry_after, .. } => *retry_after,
            _ => None,
        }
    }
}
