use crate::support::*;
use bisa_core::DecisionAnswer;
use bisa_decision::{
    Bounded, Checked, DecisionProvider, ProviderError, RetryBudget, Retrying, ScriptedProvider,
};
use std::sync::Arc;
use std::time::Duration;

const DEADLINE: Duration = Duration::from_secs(20);

fn good() -> bisa_decision::scripted::Scripted {
    ScriptedProvider::answers(
        "model",
        ScriptedProvider::choice("small", &["small", "large"], 0.8),
    )
}

fn busy(retry_after: Option<u64>) -> bisa_decision::scripted::Scripted {
    Err(ProviderError::Busy {
        status: 429,
        retry_after: retry_after.map(Duration::from_secs),
    })
}

fn retrying(
    script: Vec<bisa_decision::scripted::Scripted>,
    retries: u8,
) -> Retrying<Checked<ScriptedProvider>> {
    Retrying::new(
        Checked(ScriptedProvider::new(script)),
        RetryBudget::new(retries),
        Arc::new(ZeroEntropy),
    )
}

#[tokio::test]
async fn a_response_that_breaks_the_contract_is_an_error_never_a_guess() {
    let off_list = ScriptedProvider::answers(
        "model",
        ScriptedProvider::choice("medium", &["medium", "large"], 0.9),
    );
    let error = Checked(ScriptedProvider::new(vec![off_list]))
        .decide(&routed(), DEADLINE)
        .await
        .unwrap_err();
    assert!(matches!(error, ProviderError::Contract(_)), "{error:?}");

    let unasked = ScriptedProvider::answers("other", DecisionAnswer::Noul { noul: 0.5 });
    let error = Checked(ScriptedProvider::new(vec![unasked]))
        .decide(&routed(), DEADLINE)
        .await
        .unwrap_err();
    assert!(matches!(error, ProviderError::Contract(_)), "{error:?}");
}

#[tokio::test]
async fn a_request_that_breaks_the_contract_is_never_sent() {
    let provider = ScriptedProvider::new(vec![good()]);
    let mut request = routed();
    request.questions.clear();
    let checked = Checked(provider);
    let error = checked.decide(&request, DEADLINE).await.unwrap_err();
    assert!(matches!(error, ProviderError::Misconfigured(_)));
    assert!(checked.0.asked().is_empty());
}

#[tokio::test(start_paused = true)]
async fn a_transient_failure_is_asked_again_within_the_budget() {
    let provider = retrying(vec![busy(None), busy(Some(2)), good()], 2);
    let started = tokio::time::Instant::now();
    let response = provider.decide(&routed(), DEADLINE).await.unwrap();
    assert_eq!(response.answer("model").unwrap().chosen(), Some("small"));
    // 0.5 s of back-off, then the two seconds the service asked for.
    assert_eq!(started.elapsed(), Duration::from_millis(2_500));
}

#[tokio::test(start_paused = true)]
async fn the_budget_is_the_budget() {
    let provider = retrying(vec![busy(None), busy(None), busy(None), good()], 2);
    let error = provider.decide(&routed(), DEADLINE).await.unwrap_err();
    assert!(matches!(error, ProviderError::Busy { .. }));

    let none = retrying(vec![busy(None), good()], 0);
    assert!(none.decide(&routed(), DEADLINE).await.is_err());
}

#[tokio::test(start_paused = true)]
async fn a_refusal_and_a_bad_setup_are_never_asked_again() {
    for error in [
        ProviderError::Refused {
            status: 401,
            message: "no".into(),
        },
        ProviderError::Misconfigured("no key".into()),
    ] {
        let inner = ScriptedProvider::new(vec![Err(error.clone()), good()]);
        let provider = Retrying::new(inner, RetryBudget::new(3), Arc::new(ZeroEntropy));
        assert_eq!(
            provider.decide(&routed(), DEADLINE).await.unwrap_err(),
            error
        );
    }
}

#[tokio::test(start_paused = true)]
async fn a_contract_violation_is_asked_again() {
    let off_list = ScriptedProvider::answers(
        "model",
        ScriptedProvider::choice("medium", &["medium", "large"], 0.9),
    );
    let provider = retrying(vec![off_list, good()], 1);
    assert!(provider.decide(&routed(), DEADLINE).await.is_ok());
}

#[tokio::test(start_paused = true)]
async fn a_wait_that_would_outlive_the_deadline_is_not_taken() {
    let provider = retrying(vec![busy(Some(4)), good()], 2);
    let started = tokio::time::Instant::now();
    let error = provider
        .decide(&routed(), Duration::from_secs(3))
        .await
        .unwrap_err();
    assert!(matches!(error, ProviderError::Busy { .. }));
    assert_eq!(
        started.elapsed(),
        Duration::ZERO,
        "it did not sleep to find out"
    );
}

/// A provider that never answers.
struct Silent;

#[async_trait::async_trait]
impl DecisionProvider for Silent {
    fn descriptor(&self) -> bisa_decision::ProviderDescriptor {
        bisa_decision::ProviderDescriptor {
            kind: bisa_core::DecisionProviderKind::Harness,
            model: "silent".into(),
        }
    }

    async fn decide(
        &self,
        _request: &bisa_core::DecisionRequest,
        _deadline: Duration,
    ) -> Result<bisa_core::DecisionResponse, ProviderError> {
        std::future::pending().await
    }
}

#[tokio::test(start_paused = true)]
async fn one_deadline_ends_the_call_whatever_the_provider_is_doing() {
    let started = tokio::time::Instant::now();
    let error = Bounded(Silent)
        .decide(&routed(), Duration::from_secs(7))
        .await
        .unwrap_err();
    assert_eq!(error, ProviderError::TimedOut);
    assert_eq!(started.elapsed(), Duration::from_secs(7));
    assert!(!error.is_transient());
}
