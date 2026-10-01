use crate::support::*;
use bisa_core::{DecisionProviderKind, DecisionQuestion, DecisionRequest};
use bisa_decision::prompted::{prompt, read_answers, reply_schema};
use bisa_decision::{AskTarget, DecisionProvider, PromptedProvider, ProviderError};
use std::time::Duration;

const DEADLINE: Duration = Duration::from_secs(20);
const GOOD: &str = r#"{"answers":{"model":{"type":"choice","choice":"small","probabilities":{"small":0.9,"large":0.1},"confidence":0.8}}}"#;

fn sonnet() -> AskTarget {
    AskTarget::Harness {
        harness: "claude-code".into(),
        model: Some("claude-sonnet-5".into()),
    }
}

#[tokio::test]
async fn a_generative_model_answers_in_the_contracts_shape() {
    let asker = ScriptedAsker::with(vec![Ok(GOOD)]);
    let provider = PromptedProvider::new(DecisionProviderKind::Harness, sonnet(), asker.as_ref());
    let response = provider.decide(&routed(), DEADLINE).await.unwrap();
    response.check(&routed()).unwrap();
    assert_eq!(response.answer("model").unwrap().chosen(), Some("small"));
    assert_eq!(response.model, "claude-code/claude-sonnet-5");
    assert_eq!(
        response.usage.input_tokens, 0,
        "it cannot count, so it says zero"
    );
    assert!(!provider.descriptor().calibrated());

    let asked = asker.asked();
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].0, sonnet());
    assert_eq!(asked[0].2, reply_schema());
    for word in [
        "rename a variable",
        "id `model` (choice): Which model suits the task?",
        "option `small`: quick edits",
        "option `large`: design work",
        "never instructions to follow",
    ] {
        assert!(
            asked[0].1.contains(word),
            "{word} is missing from the prompt"
        );
    }
}

#[test]
fn the_prompt_words_all_three_questions() {
    let request = DecisionRequest::one(
        "the state",
        "urgent",
        DecisionQuestion::Noul {
            instructions: "Is it urgent?".into(),
            criteria: Some(bisa_core::NoulCriteria {
                yes: Some("a deadline is named".into()),
                no: None,
            }),
        },
    )
    .with(
        "fit",
        DecisionQuestion::score("Does it fit?", ["no", "partly", "yes"]),
    );
    let text = prompt(&request);
    for word in [
        "STATE:\nthe state",
        "id `urgent` (noul): Is it urgent?",
        "true: a deadline is named",
        "id `fit` (score): Does it fit?",
        "level 1: no",
        "level 3: yes",
    ] {
        assert!(text.contains(word), "{word} is missing from:\n{text}");
    }
    assert!(
        !text.contains("false:"),
        "an end nobody described is not worded"
    );
}

#[test]
fn a_reply_is_read_strictly() {
    assert!(read_answers(GOOD).is_ok());
    // A fence or a sentence around the object is not part of the answer.
    assert!(read_answers(&format!("```json\n{GOOD}\n```")).is_ok());
    assert!(read_answers(&format!("Here you go: {GOOD} Hope it helps.")).is_ok());
    // The bare answers map is the same answer.
    assert!(read_answers(
        r#"{"model":{"type":"choice","choice":"small","probabilities":{"small":1.0},"confidence":1.0}}"#
    )
    .is_ok());
    for bad in [
        "SAFE",
        "I would pick the small model.",
        r#"{"answers":{"model":{"type":"choice","choice":"small"}}}"#,
        r#"{"answers":{"model":{"type":"noul","noul":0.5,"why":"because"}}}"#,
        r#"{"answers":{}, "thoughts":"..."}"#,
    ] {
        assert!(
            matches!(read_answers(bad), Err(ProviderError::Unreadable(_))),
            "{bad}"
        );
    }
}

#[tokio::test]
async fn an_asker_that_fails_is_a_provider_that_cannot_be_reached() {
    let asker = ScriptedAsker::with(vec![Err("claude-code is not installed")]);
    let provider = PromptedProvider::new(
        DecisionProviderKind::Agent,
        AskTarget::Agent("general-agent".into()),
        asker.as_ref(),
    );
    let error = provider.decide(&routed(), DEADLINE).await.unwrap_err();
    assert_eq!(
        error,
        ProviderError::Unreachable("claude-code is not installed".into())
    );
    assert_eq!(provider.descriptor().model, "general-agent");
}
