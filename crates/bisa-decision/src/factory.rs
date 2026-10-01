//! The one place a provider kind is matched. The engine reads the `decisions.*`
//! settings into [`DecisionSettings`], hands in its [`Ports`], and gets back
//! one provider already wrapped in the contract check, the retry budget and
//! the deadline. Switching provider is one setting; nothing else here moves.

use crate::prompted::{AskTarget, Asker, PromptedProvider};
use crate::provider::{DecisionProvider, ProviderError};
use crate::resilient::{Bounded, Checked, RetryBudget, Retrying};
use crate::system_one::{SystemOneProvider, JEV_ENDPOINT};
use bisa_connectors::{Clock, Entropy, HostJudge, HttpTransport, Secret};
use bisa_core::DecisionProviderKind;
use std::sync::Arc;

/// How an RLCD endpoint is signed in to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RlcdAuth {
    None,
    #[default]
    Bearer,
}

impl std::str::FromStr for RlcdAuth {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "none" => Ok(RlcdAuth::None),
            "bearer" => Ok(RlcdAuth::Bearer),
            other => Err(format!("unknown endpoint sign-in `{other}`")),
        }
    }
}

/// The `decisions.*` settings a provider is built from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecisionSettings {
    pub provider: DecisionProviderKind,
    pub harness: String,
    /// Empty is the harness's own default.
    pub harness_model: String,
    pub agent: String,
    pub jev_model: String,
    pub rlcd_endpoint: String,
    pub rlcd_model: String,
    pub rlcd_auth: RlcdAuth,
    pub retries: u8,
}

impl Default for DecisionSettings {
    fn default() -> Self {
        Self {
            provider: DecisionProviderKind::Harness,
            harness: "claude-code".into(),
            harness_model: "claude-sonnet-5-5[1m]".into(),
            agent: "general-agent".into(),
            jev_model: "jev-latest".into(),
            rlcd_endpoint: String::new(),
            rlcd_model: String::new(),
            rlcd_auth: RlcdAuth::Bearer,
            retries: 2,
        }
    }
}

/// Where a remote provider's key comes from: this machine's keystore.
pub trait KeySource: Send + Sync {
    fn key(&self, provider: DecisionProviderKind) -> Option<Secret>;
}

/// No key for anybody.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoKeys;

impl KeySource for NoKeys {
    fn key(&self, _provider: DecisionProviderKind) -> Option<Secret> {
        None
    }
}

/// What the engine lends a provider, for as long as one judgement takes.
#[derive(Clone)]
pub struct Ports<'a> {
    pub transport: Arc<dyn HttpTransport>,
    pub hosts: &'a dyn HostJudge,
    pub keys: &'a dyn KeySource,
    pub asker: &'a dyn Asker,
    pub clock: Arc<dyn Clock>,
    pub entropy: Arc<dyn Entropy>,
}

/// The provider `settings` names, ready to ask — or why it cannot be asked.
pub fn build<'a>(
    settings: &DecisionSettings,
    ports: &Ports<'a>,
) -> Result<Box<dyn DecisionProvider + 'a>, ProviderError> {
    let kind = settings.provider;
    let raw: Box<dyn DecisionProvider + 'a> = match kind {
        DecisionProviderKind::Harness => {
            let harness = settings.harness.trim();
            if harness.is_empty() {
                return Err(ProviderError::Misconfigured("no harness is named".into()));
            }
            let model = settings.harness_model.trim();
            Box::new(PromptedProvider::new(
                kind,
                AskTarget::Harness {
                    harness: harness.to_string(),
                    model: (!model.is_empty()).then(|| model.to_string()),
                },
                ports.asker,
            ))
        }
        DecisionProviderKind::Agent => {
            let agent = settings.agent.trim();
            if agent.is_empty() {
                return Err(ProviderError::Misconfigured("no agent is named".into()));
            }
            Box::new(PromptedProvider::new(
                kind,
                AskTarget::Agent(agent.to_string()),
                ports.asker,
            ))
        }
        DecisionProviderKind::Jev => {
            let key = ports.keys.key(kind).ok_or_else(|| {
                ProviderError::Misconfigured("no API key is stored for Jev".into())
            })?;
            Box::new(SystemOneProvider::new(
                kind,
                JEV_ENDPOINT,
                &settings.jev_model,
                Some(key),
                Arc::clone(&ports.transport),
                ports.hosts,
                Arc::clone(&ports.clock),
            )?)
        }
        DecisionProviderKind::Rlcd => {
            let key = match settings.rlcd_auth {
                RlcdAuth::None => None,
                RlcdAuth::Bearer => Some(ports.keys.key(kind).ok_or_else(|| {
                    ProviderError::Misconfigured("no API key is stored for the endpoint".into())
                })?),
            };
            Box::new(SystemOneProvider::new(
                kind,
                &settings.rlcd_endpoint,
                &settings.rlcd_model,
                key,
                Arc::clone(&ports.transport),
                ports.hosts,
                Arc::clone(&ports.clock),
            )?)
        }
    };
    Ok(Box::new(Bounded(Retrying::new(
        Checked(raw),
        RetryBudget::new(settings.retries),
        Arc::clone(&ports.entropy),
    ))))
}
