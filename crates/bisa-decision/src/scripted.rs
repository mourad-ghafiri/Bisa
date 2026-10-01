//! The fake: a provider that answers from a script, for every test above this
//! crate. Nothing is asked of anybody; every request is kept to be read back.

use crate::provider::{DecisionProvider, ProviderDescriptor, ProviderError};
use async_trait::async_trait;
use bisa_core::{
    DecisionAnswer, DecisionProviderKind, DecisionRequest, DecisionResponse, DecisionUsage,
};
use std::collections::{BTreeMap, VecDeque};
use std::sync::Mutex;
use std::time::Duration;

/// One scripted turn: the answers to give, or the failure.
pub type Scripted = Result<BTreeMap<String, DecisionAnswer>, ProviderError>;

pub struct ScriptedProvider {
    descriptor: ProviderDescriptor,
    script: Mutex<VecDeque<Scripted>>,
    asked: Mutex<Vec<DecisionRequest>>,
}

impl ScriptedProvider {
    pub fn new(script: Vec<Scripted>) -> Self {
        Self {
            descriptor: ProviderDescriptor {
                kind: DecisionProviderKind::Rlcd,
                model: "scripted".into(),
            },
            script: Mutex::new(script.into_iter().collect()),
            asked: Mutex::new(Vec::new()),
        }
    }

    pub fn of_kind(mut self, kind: DecisionProviderKind) -> Self {
        self.descriptor.kind = kind;
        self
    }

    /// One answer to one question, as a turn of the script.
    pub fn answers(id: &str, answer: DecisionAnswer) -> Scripted {
        Ok(BTreeMap::from([(id.to_string(), answer)]))
    }

    /// A choice of `option` among `options`, with the rest of the probability
    /// spread evenly over the others.
    pub fn choice(option: &str, options: &[&str], confidence: f64) -> DecisionAnswer {
        let others = options.len().saturating_sub(1).max(1) as f64;
        let lead = if options.len() < 2 {
            1.0
        } else {
            0.5 + confidence / 2.0
        };
        DecisionAnswer::Choice {
            choice: option.to_string(),
            probabilities: options
                .iter()
                .map(|o| {
                    let p = if *o == option {
                        lead
                    } else {
                        (1.0 - lead) / others
                    };
                    (o.to_string(), p)
                })
                .collect(),
            confidence,
        }
    }

    /// Every request put to it, in order.
    pub fn asked(&self) -> Vec<DecisionRequest> {
        self.asked.lock().map(|a| a.clone()).unwrap_or_default()
    }
}

#[async_trait]
impl DecisionProvider for ScriptedProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        self.descriptor.clone()
    }

    async fn decide(
        &self,
        request: &DecisionRequest,
        _deadline: Duration,
    ) -> Result<DecisionResponse, ProviderError> {
        if let Ok(mut asked) = self.asked.lock() {
            asked.push(request.clone());
        }
        let turn = self
            .script
            .lock()
            .ok()
            .and_then(|mut s| s.pop_front())
            .unwrap_or_else(|| Err(ProviderError::Unreachable("the script ran out".into())));
        turn.map(|answers| DecisionResponse {
            model: self.descriptor.model.clone(),
            answers,
            usage: DecisionUsage::default(),
        })
    }
}
