use crate::support::*;
use bisa_core::DecisionProviderKind;
use bisa_decision::{build, DecisionSettings, NoKeys, ProviderError, RlcdAuth};
use std::time::Duration;

const DEADLINE: Duration = Duration::from_secs(20);
const GOOD: &str = r#"{"answers":{"model":{"type":"choice","choice":"large","probabilities":{"small":0.2,"large":0.8},"confidence":0.6}}}"#;

#[test]
fn the_default_is_claude_code_with_claude_sonnet_5_5() {
    let settings = DecisionSettings::default();
    assert_eq!(settings.provider, DecisionProviderKind::Harness);
    let asker = ScriptedAsker::with(vec![]);
    let ports = ports(ScriptedTransport::with(vec![]), asker.as_ref(), &NoKeys);
    let provider = build(&settings, &ports).unwrap();
    let descriptor = provider.descriptor();
    assert_eq!(descriptor.kind, DecisionProviderKind::Harness);
    assert_eq!(descriptor.model, "claude-code/claude-sonnet-5-5[1m]");
    assert!(!descriptor.calibrated());
}

#[tokio::test]
async fn switching_provider_is_one_field() {
    let transport = ScriptedTransport::with(vec![http(200, routed_answer("small"))]);
    let asker = ScriptedAsker::with(vec![Ok(GOOD), Ok(GOOD)]);
    let ports = ports(transport.clone(), asker.as_ref(), &FixtureKeys);
    let mut settings = DecisionSettings {
        rlcd_endpoint: "http://127.0.0.1:9000".into(),
        rlcd_model: "local".into(),
        ..DecisionSettings::default()
    };

    for (kind, model, chosen) in [
        (
            DecisionProviderKind::Harness,
            "claude-code/claude-sonnet-5-5[1m]",
            "large",
        ),
        (DecisionProviderKind::Agent, "general-agent", "large"),
        (DecisionProviderKind::Jev, "jev-1.13.0", "small"),
    ] {
        settings.provider = kind;
        let provider = build(&settings, &ports).unwrap();
        assert_eq!(provider.descriptor().kind, kind);
        let response = provider.decide(&routed(), DEADLINE).await.unwrap();
        assert_eq!(response.model, model);
        assert_eq!(response.answer("model").unwrap().chosen(), Some(chosen));
    }
    assert_eq!(asker.asked().len(), 2);
    assert_eq!(transport.sent().len(), 1);

    settings.provider = DecisionProviderKind::Rlcd;
    assert_eq!(
        build(&settings, &ports).unwrap().descriptor().model,
        "local"
    );
}

#[test]
fn a_remote_provider_with_no_key_cannot_be_asked() {
    let asker = ScriptedAsker::with(vec![]);
    let ports = ports(ScriptedTransport::with(vec![]), asker.as_ref(), &NoKeys);
    let mut settings = DecisionSettings {
        provider: DecisionProviderKind::Jev,
        rlcd_endpoint: "http://127.0.0.1:9000".into(),
        rlcd_model: "local".into(),
        ..DecisionSettings::default()
    };
    assert!(matches!(
        build(&settings, &ports).err(),
        Some(ProviderError::Misconfigured(_))
    ));
    settings.provider = DecisionProviderKind::Rlcd;
    assert!(build(&settings, &ports).is_err(), "bearer wants a key");
    // A model served on this machine may need none.
    settings.rlcd_auth = RlcdAuth::None;
    assert!(build(&settings, &ports).is_ok());
    settings.rlcd_endpoint.clear();
    assert!(
        build(&settings, &ports).is_err(),
        "and still wants an endpoint"
    );
    assert_eq!("none".parse::<RlcdAuth>(), Ok(RlcdAuth::None));
    assert!("basic".parse::<RlcdAuth>().is_err());
}

/// A prompted provider needs a harness, or an agent, by name.
#[test]
fn a_prompted_provider_needs_its_harness_or_its_agent_named() {
    let asker = ScriptedAsker::with(vec![]);
    let ports = ports(ScriptedTransport::with(vec![]), asker.as_ref(), &NoKeys);
    let settings = DecisionSettings {
        provider: DecisionProviderKind::Harness,
        harness: "  ".into(),
        ..DecisionSettings::default()
    };
    assert!(matches!(
        build(&settings, &ports).err(),
        Some(ProviderError::Misconfigured(why)) if why == "no harness is named"
    ));
    let settings = DecisionSettings {
        provider: DecisionProviderKind::Agent,
        agent: String::new(),
        ..DecisionSettings::default()
    };
    assert!(matches!(
        build(&settings, &ports).err(),
        Some(ProviderError::Misconfigured(why)) if why == "no agent is named"
    ));
}

/// The scripted provider: its kind is whatever a test says, a choice among
/// one option is certain, and a script that ran out is a provider that
/// cannot be reached.
#[tokio::test]
async fn the_scripted_provider_answers_from_its_script_and_no_further() {
    use bisa_decision::{DecisionProvider as _, ScriptedProvider};
    let provider = ScriptedProvider::new(vec![]).of_kind(DecisionProviderKind::Jev);
    assert_eq!(provider.descriptor().kind, DecisionProviderKind::Jev);
    match ScriptedProvider::choice("only", &["only"], 0.3) {
        bisa_core::DecisionAnswer::Choice { probabilities, .. } => {
            assert_eq!(probabilities.get("only").copied(), Some(1.0));
        }
        other => panic!("{other:?}"),
    }
    assert!(matches!(
        provider.decide(&routed(), DEADLINE).await.err(),
        Some(ProviderError::Unreachable(why)) if why == "the script ran out"
    ));
}
