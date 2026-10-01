//! What every provider is wrapped in: the contract check, the retry budget,
//! the one deadline. Each is a [`DecisionProvider`] over another, so the stack
//! is built once and a caller sees one provider.

use crate::provider::{DecisionProvider, ProviderDescriptor, ProviderError};
use async_trait::async_trait;
use bisa_connectors::Entropy;
use bisa_core::{DecisionRequest, DecisionResponse};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::Instant;

/// Holds a request and its response to the decision contract. A request that
/// breaks it is never sent; a response that breaks it is an error — transient,
/// since a generative model asked again may hold to the shape.
pub struct Checked<P>(pub P);

#[async_trait]
impl<P: DecisionProvider> DecisionProvider for Checked<P> {
    fn descriptor(&self) -> ProviderDescriptor {
        self.0.descriptor()
    }

    async fn decide(
        &self,
        request: &DecisionRequest,
        deadline: Duration,
    ) -> Result<DecisionResponse, ProviderError> {
        request
            .validate()
            .map_err(|e| ProviderError::Misconfigured(e.to_string()))?;
        let response = self.0.decide(request, deadline).await?;
        response.check(request)?;
        Ok(response)
    }
}

/// How often, and how patiently, a judgement is asked again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RetryBudget {
    /// Attempts after the first.
    pub retries: u8,
    /// The first back-off, doubled per attempt.
    pub base: Duration,
    /// The longest back-off, computed or asked for.
    pub cap: Duration,
}

impl RetryBudget {
    pub fn new(retries: u8) -> Self {
        Self {
            retries,
            base: Duration::from_millis(500),
            cap: Duration::from_secs(5),
        }
    }

    /// The wait before attempt `attempt + 1` (zero-based): the service's own
    /// wish when it sent one, else the doubled base less up to a quarter of
    /// jitter — both under the cap. `jitter` is a fraction in `0..1`.
    pub fn wait(&self, attempt: u8, retry_after: Option<Duration>, jitter: f64) -> Duration {
        let wait = match retry_after {
            Some(asked) => asked,
            None => {
                let doubled = self.base.saturating_mul(1u32 << attempt.min(16));
                doubled.mul_f64(1.0 - 0.25 * jitter.clamp(0.0, 1.0))
            }
        };
        wait.min(self.cap)
    }
}

/// Asks again after a transient failure, inside the deadline it was given: a
/// wait that would outlive the deadline is not taken, and the last error is
/// the answer.
pub struct Retrying<P> {
    inner: P,
    budget: RetryBudget,
    entropy: Arc<dyn Entropy>,
}

impl<P> Retrying<P> {
    pub fn new(inner: P, budget: RetryBudget, entropy: Arc<dyn Entropy>) -> Self {
        Self {
            inner,
            budget,
            entropy,
        }
    }

    fn jitter(&self) -> f64 {
        let mut byte = [0u8; 1];
        self.entropy.fill(&mut byte);
        f64::from(byte[0]) / 255.0
    }
}

#[async_trait]
impl<P: DecisionProvider> DecisionProvider for Retrying<P> {
    fn descriptor(&self) -> ProviderDescriptor {
        self.inner.descriptor()
    }

    async fn decide(
        &self,
        request: &DecisionRequest,
        deadline: Duration,
    ) -> Result<DecisionResponse, ProviderError> {
        let started = Instant::now();
        let mut attempt: u8 = 0;
        loop {
            let left = deadline.saturating_sub(started.elapsed());
            if left.is_zero() {
                return Err(ProviderError::TimedOut);
            }
            let error = match self.inner.decide(request, left).await {
                Ok(response) => return Ok(response),
                Err(error) => error,
            };
            if !error.is_transient() || attempt >= self.budget.retries {
                return Err(error);
            }
            let wait = self
                .budget
                .wait(attempt, error.retry_after(), self.jitter());
            if wait >= deadline.saturating_sub(started.elapsed()) {
                return Err(error);
            }
            tracing::debug!(
                attempt,
                ?wait,
                "asking the decision provider again: {error}"
            );
            tokio::time::sleep(wait).await;
            attempt += 1;
        }
    }
}

/// One deadline over the whole call — every attempt and every wait. Past it
/// the call is over, whatever the provider is doing.
pub struct Bounded<P>(pub P);

#[async_trait]
impl<P: DecisionProvider> DecisionProvider for Bounded<P> {
    fn descriptor(&self) -> ProviderDescriptor {
        self.0.descriptor()
    }

    async fn decide(
        &self,
        request: &DecisionRequest,
        deadline: Duration,
    ) -> Result<DecisionResponse, ProviderError> {
        match tokio::time::timeout(deadline, self.0.decide(request, deadline)).await {
            Ok(result) => result,
            Err(_) => Err(ProviderError::TimedOut),
        }
    }
}

#[async_trait]
impl DecisionProvider for Arc<dyn DecisionProvider> {
    fn descriptor(&self) -> ProviderDescriptor {
        (**self).descriptor()
    }

    async fn decide(
        &self,
        request: &DecisionRequest,
        deadline: Duration,
    ) -> Result<DecisionResponse, ProviderError> {
        (**self).decide(request, deadline).await
    }
}

#[async_trait]
impl DecisionProvider for Box<dyn DecisionProvider + '_> {
    fn descriptor(&self) -> ProviderDescriptor {
        (**self).descriptor()
    }

    async fn decide(
        &self,
        request: &DecisionRequest,
        deadline: Duration,
    ) -> Result<DecisionResponse, ProviderError> {
        (**self).decide(request, deadline).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wait_doubles_under_the_cap_and_obeys_the_service_under_it_too() {
        let budget = RetryBudget::new(2);
        assert_eq!(budget.wait(0, None, 0.0), Duration::from_millis(500));
        assert_eq!(budget.wait(1, None, 0.0), Duration::from_secs(1));
        assert_eq!(budget.wait(2, None, 0.0), Duration::from_secs(2));
        assert_eq!(budget.wait(6, None, 0.0), Duration::from_secs(5), "capped");
        assert_eq!(budget.wait(0, None, 1.0), Duration::from_millis(375));
        assert_eq!(
            budget.wait(0, Some(Duration::from_secs(2)), 0.5),
            Duration::from_secs(2)
        );
        assert_eq!(
            budget.wait(0, Some(Duration::from_secs(90)), 0.5),
            Duration::from_secs(5),
            "a long Retry-After is capped, not obeyed"
        );
    }
}
