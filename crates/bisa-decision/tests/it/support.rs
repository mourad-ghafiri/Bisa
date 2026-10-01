//! Fakes: a transport and an asker that answer from a script and keep what
//! they were sent, a fixed clock, entropy that is always zero, a key source.

use async_trait::async_trait;
use bisa_connectors::{
    Clock, Entropy, HostJudge, HttpTransport, Request, Response, Secret, TransportError,
};
use bisa_core::{DecisionProviderKind, DecisionQuestion, DecisionRequest};
use bisa_decision::{AskTarget, Asked, Asker, KeySource, Ports};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Default)]
pub struct ScriptedTransport {
    answers: Mutex<VecDeque<Result<Response, TransportError>>>,
    sent: Mutex<Vec<Request>>,
}

impl ScriptedTransport {
    pub fn with(answers: Vec<Result<Response, TransportError>>) -> Arc<Self> {
        Arc::new(Self {
            answers: Mutex::new(answers.into_iter().collect()),
            sent: Mutex::new(Vec::new()),
        })
    }

    pub fn sent(&self) -> Vec<Request> {
        self.sent.lock().unwrap().clone()
    }
}

#[async_trait]
impl HttpTransport for ScriptedTransport {
    async fn send(
        &self,
        req: &Request,
        _insecure_loopback: bool,
    ) -> Result<Response, TransportError> {
        self.sent.lock().unwrap().push(req.clone());
        self.answers
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| Err(TransportError::Connect("the script ran out".into())))
    }
}

pub fn http(status: u16, body: serde_json::Value) -> Result<Response, TransportError> {
    Ok(Response {
        status,
        headers: vec![],
        body: serde_json::to_vec(&body).unwrap(),
    })
}

pub fn http_with(
    status: u16,
    headers: &[(&str, &str)],
    body: serde_json::Value,
) -> Result<Response, TransportError> {
    Ok(Response {
        status,
        headers: headers
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        body: serde_json::to_vec(&body).unwrap(),
    })
}

#[derive(Default)]
pub struct ScriptedAsker {
    replies: Mutex<VecDeque<Result<Asked, String>>>,
    asked: Mutex<Vec<(AskTarget, String, serde_json::Value)>>,
}

impl ScriptedAsker {
    pub fn with(replies: Vec<Result<&str, &str>>) -> Arc<Self> {
        Arc::new(Self {
            replies: Mutex::new(
                replies
                    .into_iter()
                    .map(|r| {
                        r.map(|text| Asked {
                            text: text.to_string(),
                            model: None,
                        })
                        .map_err(str::to_string)
                    })
                    .collect(),
            ),
            asked: Mutex::new(Vec::new()),
        })
    }

    pub fn asked(&self) -> Vec<(AskTarget, String, serde_json::Value)> {
        self.asked.lock().unwrap().clone()
    }
}

#[async_trait]
impl Asker for ScriptedAsker {
    async fn ask(
        &self,
        target: &AskTarget,
        prompt: &str,
        output_schema: &serde_json::Value,
        _deadline: Duration,
    ) -> Result<Asked, String> {
        self.asked.lock().unwrap().push((
            target.clone(),
            prompt.to_string(),
            output_schema.clone(),
        ));
        self.replies
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| Err("the script ran out".into()))
    }
}

pub struct FixedClock(pub u64);

impl Clock for FixedClock {
    fn now(&self) -> u64 {
        self.0
    }
}

pub struct ZeroEntropy;

impl Entropy for ZeroEntropy {
    fn fill(&self, out: &mut [u8]) {
        out.fill(0);
    }
}

pub struct Refusing(pub &'static str);

impl HostJudge for Refusing {
    fn judge(&self, host: &str) -> Result<(), String> {
        if host == self.0 {
            Err(format!("{host} is on this node's deny list"))
        } else {
            Ok(())
        }
    }
}

/// A key for every remote provider. The value is a fixture and opens nothing.
pub struct FixtureKeys;

pub const FIXTURE_KEY: &str = "fixture-key-opens-nothing";

impl KeySource for FixtureKeys {
    fn key(&self, _provider: DecisionProviderKind) -> Option<Secret> {
        Secret::some(FIXTURE_KEY)
    }
}

pub fn ports<'a>(
    transport: Arc<dyn HttpTransport>,
    asker: &'a dyn Asker,
    keys: &'a dyn KeySource,
) -> Ports<'a> {
    Ports {
        transport,
        hosts: &bisa_connectors::AllowAll,
        keys,
        asker,
        clock: Arc::new(FixedClock(1_000)),
        entropy: Arc::new(ZeroEntropy),
    }
}

/// "Which model suits the task?" over `small` and `large`.
pub fn routed() -> DecisionRequest {
    DecisionRequest::one(
        serde_json::json!({ "task": "rename a variable" }),
        "model",
        DecisionQuestion::choice(
            "Which model suits the task?",
            [("small", "quick edits"), ("large", "design work")],
        ),
    )
}

/// A contract-true answer to [`routed`], as a System One service words it.
pub fn routed_answer(choice: &str) -> serde_json::Value {
    serde_json::json!({
        "model": "jev-1.13.0",
        "answers": { "model": {
            "type": "choice", "choice": choice,
            "probabilities": { "small": 0.85, "large": 0.15 }, "confidence": 0.7
        } },
        "usage": { "input_tokens": 40, "output_tokens": 9 }
    })
}
