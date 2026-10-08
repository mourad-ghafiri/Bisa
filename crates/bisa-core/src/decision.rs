//! The decision contract: typed questions put against a state, and the
//! structured answers every provider of the Decision-Making Agent returns.
//!
//! The shape is a System One model's — Jev's, and any other model trained for
//! calibrated decisions: a request is a `state` and a map of typed
//! questions; a response is `{ model, answers, usage }`, one answer per
//! question, each a number software can compare, threshold and route on with no
//! prose to parse. A generative model behind a harness is held to the same
//! shape, so a caller never learns which provider answered.
//!
//! Three questions, and no fourth. A **noul** is a yes/no read as a probability
//! in `[0, 1]`. A **choice** picks one of the options the caller described and
//! carries the distribution across them. A **score** rates along ordered levels
//! and carries the distribution across those. A choice and a score carry a
//! `confidence`; a noul is its own.
//!
//! [`DecisionResponse::check`] is the one runtime validator. A response that
//! fails it is an error — never a guess, never a partial answer.
//!
//! A model's answer is a **judgement** wherever it is stored or shown: on the
//! wire a *decision* is a person's signed approval (kind 3401), and a judgement
//! never signs a gate.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The most options one choice may offer.
/// The most a request weighs, serialised: 64 KiB. A question, not a document.
pub const MAX_REQUEST_BYTES: usize = 64 * 1024;

pub const MAX_CHOICE_OPTIONS: usize = 255;
/// The fewest and the most levels a score's rubric may have.
pub const MIN_SCORE_LEVELS: usize = 2;
pub const MAX_SCORE_LEVELS: usize = 10;
/// How far a distribution's sum may stand from one and still be a distribution.
pub const DISTRIBUTION_TOLERANCE: f64 = 0.05;

/// What a noul's two ends mean, when the instructions alone do not say.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NoulCriteria {
    #[serde(rename = "true", default, skip_serializing_if = "Option::is_none")]
    pub yes: Option<String>,
    #[serde(rename = "false", default, skip_serializing_if = "Option::is_none")]
    pub no: Option<String>,
}

/// One typed question.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum DecisionQuestion {
    /// Is it so? Answered as a probability.
    Noul {
        instructions: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        criteria: Option<NoulCriteria>,
    },
    /// Which one? `criteria` is each option and what it means.
    Choice {
        instructions: String,
        criteria: BTreeMap<String, String>,
    },
    /// How much? `criteria` is the rubric's levels, lowest first.
    Score {
        instructions: String,
        criteria: Vec<String>,
    },
}

impl DecisionQuestion {
    pub fn noul(instructions: impl Into<String>) -> Self {
        Self::Noul {
            instructions: instructions.into(),
            criteria: None,
        }
    }

    pub fn choice<I, K, V>(instructions: impl Into<String>, options: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        Self::Choice {
            instructions: instructions.into(),
            criteria: options
                .into_iter()
                .map(|(k, v)| (k.into(), v.into()))
                .collect(),
        }
    }

    pub fn score<I, S>(instructions: impl Into<String>, levels: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self::Score {
            instructions: instructions.into(),
            criteria: levels.into_iter().map(Into::into).collect(),
        }
    }

    /// The wire word of the question's type, which its answer must repeat.
    pub fn type_str(&self) -> &'static str {
        match self {
            Self::Noul { .. } => "noul",
            Self::Choice { .. } => "choice",
            Self::Score { .. } => "score",
        }
    }

    fn instructions(&self) -> &str {
        match self {
            Self::Noul { instructions, .. }
            | Self::Choice { instructions, .. }
            | Self::Score { instructions, .. } => instructions,
        }
    }
}

/// A state and the questions put against it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DecisionRequest {
    /// What is being judged: text, or any JSON a caller composed.
    pub state: serde_json::Value,
    pub questions: BTreeMap<String, DecisionQuestion>,
}

impl DecisionRequest {
    /// One question against one state — the atomic shape most callers want.
    pub fn one(
        state: impl Into<serde_json::Value>,
        id: impl Into<String>,
        question: DecisionQuestion,
    ) -> Self {
        Self {
            state: state.into(),
            questions: BTreeMap::from([(id.into(), question)]),
        }
    }

    pub fn with(mut self, id: impl Into<String>, question: DecisionQuestion) -> Self {
        self.questions.insert(id.into(), question);
        self
    }

    /// Every way this request is not one a provider may be sent, in id order.
    pub fn validate(&self) -> Result<(), DecisionContractError> {
        if self.questions.is_empty() {
            return Err(DecisionContractError::NoQuestions);
        }
        // A question is asked, not a document handed over: the same bound at
        // every door — Settings' *Try it*, an agent's `decide`, a judge step.
        let bytes = serde_json::to_vec(self)
            .map(|b| b.len())
            .unwrap_or(usize::MAX);
        if bytes > MAX_REQUEST_BYTES {
            return Err(DecisionContractError::TooLarge { bytes });
        }
        for (id, question) in &self.questions {
            let id = id.as_str();
            if id.trim().is_empty() {
                return Err(DecisionContractError::EmptyQuestionId);
            }
            if question.instructions().trim().is_empty() {
                return Err(DecisionContractError::EmptyInstructions(id.to_string()));
            }
            match question {
                DecisionQuestion::Noul { .. } => {}
                DecisionQuestion::Choice { criteria, .. } => {
                    if criteria.len() < 2 || criteria.len() > MAX_CHOICE_OPTIONS {
                        return Err(DecisionContractError::OptionCount {
                            question: id.to_string(),
                            found: criteria.len(),
                        });
                    }
                    if criteria.keys().any(|k| k.trim().is_empty()) {
                        return Err(DecisionContractError::EmptyOption(id.to_string()));
                    }
                }
                DecisionQuestion::Score { criteria, .. } => {
                    if criteria.len() < MIN_SCORE_LEVELS || criteria.len() > MAX_SCORE_LEVELS {
                        return Err(DecisionContractError::LevelCount {
                            question: id.to_string(),
                            found: criteria.len(),
                        });
                    }
                }
            }
        }
        Ok(())
    }
}

/// One typed answer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum DecisionAnswer {
    Noul {
        noul: f64,
    },
    Choice {
        choice: String,
        probabilities: BTreeMap<String, f64>,
        confidence: f64,
    },
    Score {
        score: f64,
        legend: BTreeMap<String, String>,
        probabilities: BTreeMap<String, f64>,
        confidence: f64,
    },
}

impl DecisionAnswer {
    pub fn type_str(&self) -> &'static str {
        match self {
            Self::Noul { .. } => "noul",
            Self::Choice { .. } => "choice",
            Self::Score { .. } => "score",
        }
    }

    /// How sure the answer is, on one scale for all three: a choice's and a
    /// score's `confidence`; for a noul, how far it stands from a coin toss
    /// toward either end (`0.5` reads `0`, `0` and `1` read `1`).
    pub fn certainty(&self) -> f64 {
        match self {
            Self::Noul { noul } => (noul - 0.5).abs() * 2.0,
            Self::Choice { confidence, .. } | Self::Score { confidence, .. } => *confidence,
        }
    }

    /// Whether a caller gated at `threshold` may act on this answer.
    pub fn clears(&self, threshold: f64) -> bool {
        self.certainty() >= threshold
    }

    /// The option chosen, for a choice.
    pub fn chosen(&self) -> Option<&str> {
        match self {
            Self::Choice { choice, .. } => Some(choice),
            _ => None,
        }
    }

    /// The probability it is so, for a noul.
    pub fn noul(&self) -> Option<f64> {
        match self {
            Self::Noul { noul } => Some(*noul),
            _ => None,
        }
    }

    /// The rating, for a score.
    pub fn score(&self) -> Option<f64> {
        match self {
            Self::Score { score, .. } => Some(*score),
            _ => None,
        }
    }
}

/// What the call cost, as the provider counts it. A provider that cannot count
/// says zero.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(deny_unknown_fields)]
pub struct DecisionUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

/// The canonical response: what every provider returns, and nothing else.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DecisionResponse {
    /// The model that answered, as the provider names it.
    pub model: String,
    pub answers: BTreeMap<String, DecisionAnswer>,
    pub usage: DecisionUsage,
}

impl DecisionResponse {
    /// The answer to one question. Present for every question of a checked
    /// response.
    pub fn answer(&self, id: &str) -> Option<&DecisionAnswer> {
        self.answers.get(id)
    }

    /// The least certain answer's certainty — what a caller who needs *all* its
    /// questions answered gates on.
    pub fn certainty(&self) -> f64 {
        self.answers
            .values()
            .map(DecisionAnswer::certainty)
            .fold(1.0, f64::min)
    }

    /// Hold this response to the contract, against the request it answers.
    pub fn check(&self, request: &DecisionRequest) -> Result<(), DecisionContractError> {
        if self.model.trim().is_empty() {
            return Err(DecisionContractError::EmptyModel);
        }
        if let Some(extra) = self
            .answers
            .keys()
            .find(|id| !request.questions.contains_key(*id))
        {
            return Err(DecisionContractError::UnaskedAnswer(extra.clone()));
        }
        for (id, question) in &request.questions {
            let Some(answer) = self.answers.get(id) else {
                return Err(DecisionContractError::Unanswered(id.clone()));
            };
            check_answer(id, question, answer)?;
        }
        Ok(())
    }
}

fn check_answer(
    id: &str,
    question: &DecisionQuestion,
    answer: &DecisionAnswer,
) -> Result<(), DecisionContractError> {
    let q = || id.to_string();
    match (question, answer) {
        (DecisionQuestion::Noul { .. }, DecisionAnswer::Noul { noul }) => unit(id, "noul", *noul),
        (
            DecisionQuestion::Choice { criteria, .. },
            DecisionAnswer::Choice {
                choice,
                probabilities,
                confidence,
            },
        ) => {
            if !criteria.contains_key(choice) {
                return Err(DecisionContractError::UnknownChoice {
                    question: q(),
                    choice: choice.clone(),
                });
            }
            if let Some(stray) = probabilities.keys().find(|k| !criteria.contains_key(*k)) {
                return Err(DecisionContractError::UnknownOutcome {
                    question: q(),
                    outcome: stray.clone(),
                });
            }
            if !probabilities.contains_key(choice) {
                return Err(DecisionContractError::ChoiceWithoutProbability(q()));
            }
            distribution(id, probabilities)?;
            unit(id, "confidence", *confidence)
        }
        (
            DecisionQuestion::Score { criteria, .. },
            DecisionAnswer::Score {
                score,
                legend,
                probabilities,
                confidence,
            },
        ) => {
            if legend.len() != criteria.len() {
                return Err(DecisionContractError::LegendMismatch {
                    question: q(),
                    levels: criteria.len(),
                    found: legend.len(),
                });
            }
            if let Some(stray) = probabilities.keys().find(|k| !legend.contains_key(*k)) {
                return Err(DecisionContractError::UnknownOutcome {
                    question: q(),
                    outcome: stray.clone(),
                });
            }
            let marks: Option<Vec<f64>> = legend.keys().map(|k| k.parse::<f64>().ok()).collect();
            if let Some(marks) = marks {
                let low = marks.iter().copied().fold(f64::INFINITY, f64::min);
                let high = marks.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                if !score.is_finite() || *score < low || *score > high {
                    return Err(DecisionContractError::ScoreOffTheLegend {
                        question: q(),
                        score: *score,
                    });
                }
            } else if !score.is_finite() {
                return Err(DecisionContractError::ScoreOffTheLegend {
                    question: q(),
                    score: *score,
                });
            }
            distribution(id, probabilities)?;
            unit(id, "confidence", *confidence)
        }
        (question, answer) => Err(DecisionContractError::WrongType {
            question: q(),
            asked: question.type_str(),
            answered: answer.type_str(),
        }),
    }
}

fn unit(id: &str, field: &'static str, value: f64) -> Result<(), DecisionContractError> {
    if value.is_finite() && (0.0..=1.0).contains(&value) {
        Ok(())
    } else {
        Err(DecisionContractError::OutOfRange {
            question: id.to_string(),
            field,
            value,
        })
    }
}

fn distribution(
    id: &str,
    probabilities: &BTreeMap<String, f64>,
) -> Result<(), DecisionContractError> {
    for p in probabilities.values() {
        unit(id, "probabilities", *p)?;
    }
    let sum: f64 = probabilities.values().sum();
    if (sum - 1.0).abs() > DISTRIBUTION_TOLERANCE {
        return Err(DecisionContractError::NotADistribution {
            question: id.to_string(),
            sum,
        });
    }
    Ok(())
}

/// Every way a request or a response breaks the contract, by name.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum DecisionContractError {
    #[error("a request asks at least one question")]
    NoQuestions,
    #[error("a request of {bytes} bytes is too large; the most is {max} — a question, not a document", max = MAX_REQUEST_BYTES)]
    TooLarge { bytes: usize },
    #[error("a question's id is never empty")]
    EmptyQuestionId,
    #[error("question `{0}` has no instructions")]
    EmptyInstructions(String),
    #[error("choice `{question}` offers {found} options; it takes 2 to 255")]
    OptionCount { question: String, found: usize },
    #[error("choice `{0}` has an option with no name")]
    EmptyOption(String),
    #[error("score `{question}` has {found} levels; it takes 2 to 10")]
    LevelCount { question: String, found: usize },
    #[error("the response names no model")]
    EmptyModel,
    #[error("question `{0}` was not answered")]
    Unanswered(String),
    #[error("the response answers `{0}`, which was not asked")]
    UnaskedAnswer(String),
    #[error("question `{question}` is a {asked} and was answered as a {answered}")]
    WrongType {
        question: String,
        asked: &'static str,
        answered: &'static str,
    },
    #[error("choice `{question}` answered `{choice}`, which is not one of its options")]
    UnknownChoice { question: String, choice: String },
    #[error("question `{question}` gives a probability to `{outcome}`, which it does not have")]
    UnknownOutcome { question: String, outcome: String },
    #[error("choice `{0}` gives its own choice no probability")]
    ChoiceWithoutProbability(String),
    #[error("score `{question}` has a legend of {found} for {levels} levels")]
    LegendMismatch {
        question: String,
        levels: usize,
        found: usize,
    },
    #[error("score `{question}` answered {score}, which is off its legend")]
    ScoreOffTheLegend { question: String, score: f64 },
    #[error("question `{question}`: {field} is {value}, outside 0 to 1")]
    OutOfRange {
        question: String,
        field: &'static str,
        value: f64,
    },
    #[error("question `{question}`: its probabilities sum to {sum}, not to one")]
    NotADistribution { question: String, sum: f64 },
}

/// The JSON Schema of a response's `answers` — what a generative provider is
/// handed so its harness can hold the model to the shape.
pub fn answers_schema() -> serde_json::Value {
    let schema = schemars::schema_for!(BTreeMap<String, DecisionAnswer>);
    serde_json::to_value(schema).unwrap_or(serde_json::Value::Null)
}

/// A place in the platform where a judgement may stand in for a fixed rule.
/// Every other decision the platform makes is rule-only by design — see
/// `docs/architecture/15-decision-making-agent.md`.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
)]
pub enum DecisionPoint {
    /// Which of an agent's models leads a launch (`auto_route`).
    #[serde(rename = "model.route")]
    ModelRoute,
    /// How hard a model works on a task (an effort of `auto`).
    #[serde(rename = "model.effort")]
    ModelEffort,
    /// Whether a tool call is harmful (the Auto-classifier).
    #[serde(rename = "security.tool")]
    SecurityTool,
    /// Whether a message from another node is harmful.
    #[serde(rename = "security.message")]
    SecurityMessage,
    /// Whether content an agent reads from outside — a page, a review — is
    /// harmful: instructions dressed as content.
    #[serde(rename = "security.content")]
    SecurityContent,
    /// Which member of the eligible pool takes a work item.
    #[serde(rename = "assign.pick")]
    AssignPick,
    /// Which agent an unaddressed message wakes.
    #[serde(rename = "dispatch.triage")]
    DispatchTriage,
    /// Whether an auto goal adopts the workflow designed for it alone.
    #[serde(rename = "goal.adopt")]
    GoalAdopt,
    /// Whether a browser tab an agent opens needs a person watching.
    #[serde(rename = "browser.headless")]
    BrowserHeadless,
    /// A workflow's `judge` step.
    #[serde(rename = "workflow.judge")]
    WorkflowJudge,
    /// An agent's own question, through the `decide` tool.
    #[serde(rename = "agent.decide")]
    AgentDecide,
}

impl DecisionPoint {
    pub const ALL: [DecisionPoint; 11] = [
        DecisionPoint::ModelRoute,
        DecisionPoint::ModelEffort,
        DecisionPoint::SecurityTool,
        DecisionPoint::SecurityMessage,
        DecisionPoint::SecurityContent,
        DecisionPoint::AssignPick,
        DecisionPoint::DispatchTriage,
        DecisionPoint::GoalAdopt,
        DecisionPoint::BrowserHeadless,
        DecisionPoint::WorkflowJudge,
        DecisionPoint::AgentDecide,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            DecisionPoint::ModelRoute => "model.route",
            DecisionPoint::ModelEffort => "model.effort",
            DecisionPoint::SecurityTool => "security.tool",
            DecisionPoint::SecurityMessage => "security.message",
            DecisionPoint::SecurityContent => "security.content",
            DecisionPoint::AssignPick => "assign.pick",
            DecisionPoint::DispatchTriage => "dispatch.triage",
            DecisionPoint::GoalAdopt => "goal.adopt",
            DecisionPoint::BrowserHeadless => "browser.headless",
            DecisionPoint::WorkflowJudge => "workflow.judge",
            DecisionPoint::AgentDecide => "agent.decide",
        }
    }

    /// A security point fails closed and is gated at the security threshold.
    pub fn is_security(self) -> bool {
        matches!(
            self,
            DecisionPoint::SecurityTool
                | DecisionPoint::SecurityMessage
                | DecisionPoint::SecurityContent
        )
    }

    /// A point somebody selected by name — the `auto_route` strategy, an
    /// effort of `auto`, the classifier's provider, a `judge` step — is on
    /// whatever the switches say: selecting it *is* the switch.
    pub fn is_selected_explicitly(self) -> bool {
        matches!(
            self,
            DecisionPoint::ModelRoute
                | DecisionPoint::ModelEffort
                | DecisionPoint::SecurityTool
                | DecisionPoint::SecurityMessage
                | DecisionPoint::SecurityContent
                | DecisionPoint::WorkflowJudge
        )
    }
}

impl std::str::FromStr for DecisionPoint {
    type Err = crate::CoreError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        DecisionPoint::ALL
            .into_iter()
            .find(|p| p.as_str() == s)
            .ok_or_else(|| crate::CoreError::UnknownDecisionWord {
                what: "decision point",
                value: s.to_string(),
            })
    }
}

impl std::fmt::Display for DecisionPoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What became of one judgement.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum JudgementOutcome {
    /// The answer cleared its threshold and the caller acted on it.
    Applied,
    /// The provider answered, below the threshold; the caller's own rule ran.
    Unsure,
    /// No answer that holds to the contract; the caller's own rule ran.
    Failed,
}

impl JudgementOutcome {
    pub const ALL: [JudgementOutcome; 3] = [
        JudgementOutcome::Applied,
        JudgementOutcome::Unsure,
        JudgementOutcome::Failed,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            JudgementOutcome::Applied => "applied",
            JudgementOutcome::Unsure => "unsure",
            JudgementOutcome::Failed => "failed",
        }
    }
}

impl std::str::FromStr for JudgementOutcome {
    type Err = crate::CoreError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        JudgementOutcome::ALL
            .into_iter()
            .find(|o| o.as_str() == s)
            .ok_or_else(|| crate::CoreError::UnknownDecisionWord {
                what: "judgement outcome",
                value: s.to_string(),
            })
    }
}

/// Which kind of provider answers.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum DecisionProviderKind {
    /// A harness on this node, with one of its models.
    #[default]
    Harness,
    /// An agent, with its own harness and model plan.
    Agent,
    /// Jev, TypeSafe AI's System One model.
    Jev,
    /// Any other model that speaks the System One wire at an endpoint.
    Rlcd,
}

impl DecisionProviderKind {
    pub const ALL: [DecisionProviderKind; 4] = [
        DecisionProviderKind::Harness,
        DecisionProviderKind::Agent,
        DecisionProviderKind::Jev,
        DecisionProviderKind::Rlcd,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            DecisionProviderKind::Harness => "harness",
            DecisionProviderKind::Agent => "agent",
            DecisionProviderKind::Jev => "jev",
            DecisionProviderKind::Rlcd => "rlcd",
        }
    }

    /// A model trained for calibrated decisions reports probabilities that mean
    /// how often it is right. A generative model reports its own estimate.
    pub fn is_calibrated(self) -> bool {
        matches!(self, DecisionProviderKind::Jev | DecisionProviderKind::Rlcd)
    }

    /// Whether the provider is an outside service reached over HTTP.
    pub fn is_remote(self) -> bool {
        self.is_calibrated()
    }
}

impl std::str::FromStr for DecisionProviderKind {
    type Err = crate::CoreError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        DecisionProviderKind::ALL
            .into_iter()
            .find(|k| k.as_str() == s)
            .ok_or_else(|| crate::CoreError::UnknownDecisionWord {
                what: "decision provider",
                value: s.to_string(),
            })
    }
}

/// One judgement, as it is recorded: on a goal's journal, and in the node's
/// own log of every judgement it asked for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Judgement {
    pub point: DecisionPoint,
    pub provider: DecisionProviderKind,
    /// The model that answered, or the one that was asked when none did.
    pub model: String,
    /// Whether the probabilities are a calibrated model's.
    pub calibrated: bool,
    /// The questions as they were asked — redacted, like the state they were
    /// put against, which is not kept.
    pub questions: BTreeMap<String, DecisionQuestion>,
    /// Empty when the provider gave no answer that holds to the contract.
    #[serde(default)]
    pub answers: BTreeMap<String, DecisionAnswer>,
    pub outcome: JudgementOutcome,
    /// Why the outcome is not `applied`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default)]
    pub latency_ms: u64,
    #[serde(default)]
    pub usage: DecisionUsage,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn urgent() -> DecisionRequest {
        DecisionRequest::one(
            "Help! My payouts have been failing for 3 days.",
            "is_urgent",
            DecisionQuestion::Noul {
                instructions: "Does this convey urgency?".into(),
                criteria: Some(NoulCriteria {
                    yes: Some("Explicitly time-sensitive".into()),
                    no: Some("No urgency expressed".into()),
                }),
            },
        )
    }

    fn routed() -> DecisionRequest {
        DecisionRequest::one(
            json!({ "task": "rename a variable" }),
            "model",
            DecisionQuestion::choice(
                "Which model suits the task?",
                [("small", "quick edits"), ("large", "design work")],
            ),
        )
    }

    fn rated() -> DecisionRequest {
        DecisionRequest::one(
            "a workflow",
            "fit",
            DecisionQuestion::score("Does it reach the goal?", ["no", "partly", "yes"]),
        )
    }

    fn choice(choice: &str, small: f64, large: f64, confidence: f64) -> DecisionResponse {
        DecisionResponse {
            model: "jev-1.13.0".into(),
            answers: BTreeMap::from([(
                "model".to_string(),
                DecisionAnswer::Choice {
                    choice: choice.into(),
                    probabilities: BTreeMap::from([
                        ("small".to_string(), small),
                        ("large".to_string(), large),
                    ]),
                    confidence,
                },
            )]),
            usage: DecisionUsage::default(),
        }
    }

    #[test]
    fn the_documented_request_and_response_read_and_write_unchanged() {
        let request = json!({
            "state": "Help! My payouts have been failing for 3 days.",
            "questions": { "is_urgent": {
                "type": "noul",
                "instructions": "Does this convey urgency?",
                "criteria": { "true": "Explicitly time-sensitive", "false": "No urgency expressed" }
            } }
        });
        let parsed: DecisionRequest = serde_json::from_value(request.clone()).unwrap();
        assert_eq!(parsed, urgent());
        assert_eq!(serde_json::to_value(&parsed).unwrap(), request);

        let response = json!({
            "model": "jev-1.13.0",
            "answers": { "is_urgent": { "type": "noul", "noul": 0.95 } },
            "usage": { "input_tokens": 426, "output_tokens": 73 }
        });
        let parsed: DecisionResponse = serde_json::from_value(response.clone()).unwrap();
        parsed.check(&urgent()).unwrap();
        assert_eq!(parsed.answer("is_urgent").unwrap().noul(), Some(0.95));
        assert_eq!(serde_json::to_value(&parsed).unwrap(), response);
    }

    #[test]
    fn a_response_with_a_field_the_contract_does_not_name_is_not_read() {
        for stray in [
            json!({ "model": "m", "answers": {}, "usage": { "input_tokens": 0, "output_tokens": 0 }, "note": "x" }),
            json!({ "model": "m", "answers": { "q": { "type": "noul", "noul": 0.5, "why": "x" } },
                    "usage": { "input_tokens": 0, "output_tokens": 0 } }),
            json!({ "model": "m", "answers": { "q": { "type": "guess", "noul": 0.5 } },
                    "usage": { "input_tokens": 0, "output_tokens": 0 } }),
            json!({ "model": "m", "answers": {} }),
        ] {
            assert!(serde_json::from_value::<DecisionResponse>(stray).is_err());
        }
    }

    #[test]
    fn a_request_is_refused_by_name() {
        let none = DecisionRequest {
            state: json!(null),
            questions: BTreeMap::new(),
        };
        assert_eq!(none.validate(), Err(DecisionContractError::NoQuestions));

        let blank = DecisionRequest::one("s", "q", DecisionQuestion::noul("  "));
        assert_eq!(
            blank.validate(),
            Err(DecisionContractError::EmptyInstructions("q".into()))
        );

        let one_option = DecisionRequest::one(
            "s",
            "q",
            DecisionQuestion::choice("which?", [("only", "the one")]),
        );
        assert_eq!(
            one_option.validate(),
            Err(DecisionContractError::OptionCount {
                question: "q".into(),
                found: 1
            })
        );

        let many: Vec<(String, String)> = (0..=MAX_CHOICE_OPTIONS)
            .map(|i| (format!("o{i}"), String::new()))
            .collect();
        let too_many = DecisionRequest::one("s", "q", DecisionQuestion::choice("which?", many));
        assert!(matches!(
            too_many.validate(),
            Err(DecisionContractError::OptionCount { found: 256, .. })
        ));

        let flat = DecisionRequest::one("s", "q", DecisionQuestion::score("how?", ["only"]));
        assert_eq!(
            flat.validate(),
            Err(DecisionContractError::LevelCount {
                question: "q".into(),
                found: 1
            })
        );
        let eleven = DecisionRequest::one(
            "s",
            "q",
            DecisionQuestion::score("how?", (0..11).map(|i| i.to_string())),
        );
        assert!(matches!(
            eleven.validate(),
            Err(DecisionContractError::LevelCount { found: 11, .. })
        ));

        urgent().validate().unwrap();
        routed().validate().unwrap();
        rated().validate().unwrap();
    }

    #[test]
    fn a_response_is_held_to_the_request_it_answers() {
        let request = routed();
        choice("small", 0.8, 0.2, 0.7).check(&request).unwrap();

        assert_eq!(
            choice("medium", 0.8, 0.2, 0.7).check(&request),
            Err(DecisionContractError::UnknownChoice {
                question: "model".into(),
                choice: "medium".into()
            })
        );
        assert!(matches!(
            choice("small", 0.8, 0.8, 0.7).check(&request),
            Err(DecisionContractError::NotADistribution { .. })
        ));
        assert!(matches!(
            choice("small", 1.2, -0.2, 0.7).check(&request),
            Err(DecisionContractError::OutOfRange {
                field: "probabilities",
                ..
            })
        ));
        assert!(matches!(
            choice("small", 0.8, 0.2, 1.5).check(&request),
            Err(DecisionContractError::OutOfRange {
                field: "confidence",
                ..
            })
        ));
        assert!(matches!(
            choice("small", 0.8, 0.2, f64::NAN).check(&request),
            Err(DecisionContractError::OutOfRange { .. })
        ));

        let mut stray = choice("small", 0.7, 0.2, 0.7);
        if let Some(DecisionAnswer::Choice { probabilities, .. }) = stray.answers.get_mut("model") {
            probabilities.insert("medium".into(), 0.1);
        }
        assert_eq!(
            stray.check(&request),
            Err(DecisionContractError::UnknownOutcome {
                question: "model".into(),
                outcome: "medium".into()
            })
        );

        let mut unanswered = choice("small", 0.8, 0.2, 0.7);
        unanswered.answers.clear();
        assert_eq!(
            unanswered.check(&request),
            Err(DecisionContractError::Unanswered("model".into()))
        );

        let mut extra = choice("small", 0.8, 0.2, 0.7);
        extra
            .answers
            .insert("other".into(), DecisionAnswer::Noul { noul: 0.5 });
        assert_eq!(
            extra.check(&request),
            Err(DecisionContractError::UnaskedAnswer("other".into()))
        );

        let mut wrong = choice("small", 0.8, 0.2, 0.7);
        wrong
            .answers
            .insert("model".into(), DecisionAnswer::Noul { noul: 0.5 });
        assert_eq!(
            wrong.check(&request),
            Err(DecisionContractError::WrongType {
                question: "model".into(),
                asked: "choice",
                answered: "noul"
            })
        );

        let mut nameless = choice("small", 0.8, 0.2, 0.7);
        nameless.model = " ".into();
        assert_eq!(
            nameless.check(&request),
            Err(DecisionContractError::EmptyModel)
        );
    }

    #[test]
    fn a_score_stays_on_its_legend() {
        let request = rated();
        let answer = |score: f64, legend: &[&str]| DecisionResponse {
            model: "m".into(),
            answers: BTreeMap::from([(
                "fit".to_string(),
                DecisionAnswer::Score {
                    score,
                    legend: legend
                        .iter()
                        .enumerate()
                        .map(|(i, l)| ((i + 1).to_string(), l.to_string()))
                        .collect(),
                    probabilities: BTreeMap::from([
                        ("1".to_string(), 0.1),
                        ("2".to_string(), 0.3),
                        ("3".to_string(), 0.6),
                    ]),
                    confidence: 0.5,
                },
            )]),
            usage: DecisionUsage::default(),
        };
        answer(2.5, &["no", "partly", "yes"])
            .check(&request)
            .unwrap();
        assert!(matches!(
            answer(3.5, &["no", "partly", "yes"]).check(&request),
            Err(DecisionContractError::ScoreOffTheLegend { .. })
        ));
        assert!(matches!(
            answer(2.0, &["no", "yes"]).check(&request),
            Err(DecisionContractError::LegendMismatch {
                levels: 3,
                found: 2,
                ..
            })
        ));
    }

    #[test]
    fn certainty_reads_all_three_answers_on_one_scale() {
        assert_eq!(DecisionAnswer::Noul { noul: 0.5 }.certainty(), 0.0);
        assert_eq!(DecisionAnswer::Noul { noul: 1.0 }.certainty(), 1.0);
        assert_eq!(DecisionAnswer::Noul { noul: 0.0 }.certainty(), 1.0);
        assert!(DecisionAnswer::Noul { noul: 0.97 }.clears(0.9));
        assert!(!DecisionAnswer::Noul { noul: 0.9 }.clears(0.9));

        let response = choice("small", 0.8, 0.2, 0.7);
        assert_eq!(response.certainty(), 0.7);
        assert!(response.answer("model").unwrap().clears(0.7));
        assert!(!response.answer("model").unwrap().clears(0.71));
        assert_eq!(response.answer("model").unwrap().chosen(), Some("small"));
    }

    #[test]
    fn every_point_outcome_and_provider_reads_back_from_its_word() {
        for point in DecisionPoint::ALL {
            assert_eq!(point.as_str().parse::<DecisionPoint>().unwrap(), point);
            assert_eq!(
                serde_json::to_value(point).unwrap(),
                json!(point.as_str()),
                "the wire word is the point's id"
            );
        }
        assert!("gate.sign".parse::<DecisionPoint>().is_err());
        for outcome in JudgementOutcome::ALL {
            assert_eq!(
                outcome.as_str().parse::<JudgementOutcome>().unwrap(),
                outcome
            );
        }
        for kind in DecisionProviderKind::ALL {
            assert_eq!(kind.as_str().parse::<DecisionProviderKind>().unwrap(), kind);
            assert_eq!(kind.is_calibrated(), kind.is_remote());
        }
        assert_eq!(
            DecisionProviderKind::default(),
            DecisionProviderKind::Harness
        );
    }

    #[test]
    fn the_security_points_are_the_ones_that_fail_closed() {
        let security: Vec<_> = DecisionPoint::ALL
            .into_iter()
            .filter(|p| p.is_security())
            .collect();
        assert_eq!(
            security,
            [
                DecisionPoint::SecurityTool,
                DecisionPoint::SecurityMessage,
                DecisionPoint::SecurityContent
            ]
        );
        assert_eq!(DecisionPoint::ALL.len(), 11);
        assert_eq!(
            "security.content".parse::<DecisionPoint>().unwrap(),
            DecisionPoint::SecurityContent
        );
        assert!(security.iter().all(|p| p.is_selected_explicitly()));
        assert!(!DecisionPoint::AssignPick.is_selected_explicitly());
        assert!(!DecisionPoint::AgentDecide.is_selected_explicitly());
    }

    /// The help of `decisions.points_off` names every point by its id: the
    /// ones the switch reaches in its first sentence — what may be listed
    /// there — and the ones selected by name in its second. A point added to
    /// the enum is a point a person must be able to read about.
    #[test]
    fn the_help_of_the_points_setting_names_every_point_where_it_belongs() {
        let catalog = include_str!("../../../locales/en/settings.ftl");
        let help = catalog
            .lines()
            .skip_while(|line| !line.starts_with("setting-decisions-points_off ="))
            .nth(1)
            .and_then(|line| line.trim().strip_prefix(".help = "))
            .expect("the setting's help");
        let (reached, selected) = help
            .split_once(". ")
            .expect("two sentences: what the switch reaches, what is selected by name");
        let named = |sentence: &str| -> Vec<String> {
            sentence
                .split('`')
                .skip(1)
                .step_by(2)
                .map(str::to_string)
                .collect()
        };
        let of = |explicit: bool| -> Vec<String> {
            DecisionPoint::ALL
                .into_iter()
                .filter(|p| p.is_selected_explicitly() == explicit)
                .map(|p| p.as_str().to_string())
                .collect()
        };
        assert_eq!(named(reached), of(false), "{reached}");
        assert_eq!(named(selected), of(true), "{selected}");
    }

    #[test]
    fn the_effort_point_is_the_eleventh_second_in_the_list_and_selected_by_name() {
        assert_eq!(DecisionPoint::ALL.len(), 11);
        assert_eq!(DecisionPoint::ALL[0], DecisionPoint::ModelRoute);
        assert_eq!(DecisionPoint::ALL[1], DecisionPoint::ModelEffort);
        assert_eq!(DecisionPoint::ALL[2], DecisionPoint::SecurityTool);
        assert_eq!(DecisionPoint::ModelEffort.as_str(), "model.effort");
        assert_eq!(
            "model.effort".parse::<DecisionPoint>().unwrap(),
            DecisionPoint::ModelEffort
        );
        assert_eq!(
            serde_json::to_value(DecisionPoint::ModelEffort).unwrap(),
            json!("model.effort")
        );
        // Naming `auto` is the switch; a wrong level is no harm, so the point
        // does not fail closed.
        assert!(DecisionPoint::ModelEffort.is_selected_explicitly());
        assert!(!DecisionPoint::ModelEffort.is_security());
        // The list is in the order the enum declares, which is the order the
        // points sort in.
        let mut sorted = DecisionPoint::ALL;
        sorted.sort();
        assert_eq!(sorted, DecisionPoint::ALL);
        let selected: Vec<_> = DecisionPoint::ALL
            .into_iter()
            .filter(|p| p.is_selected_explicitly())
            .collect();
        assert_eq!(
            selected,
            [
                DecisionPoint::ModelRoute,
                DecisionPoint::ModelEffort,
                DecisionPoint::SecurityTool,
                DecisionPoint::SecurityMessage,
                DecisionPoint::SecurityContent,
                DecisionPoint::WorkflowJudge,
            ]
        );
    }

    #[test]
    fn the_answers_schema_names_the_three_answer_types() {
        let schema = answers_schema().to_string();
        for word in ["noul", "choice", "score", "probabilities", "confidence"] {
            assert!(schema.contains(word), "{word} is missing from {schema}");
        }
    }

    // added by the coverage pass: decision.rs

    #[test]
    fn a_question_and_an_answer_say_their_type_and_an_answer_yields_its_own_kind_alone() {
        assert_eq!(DecisionQuestion::noul("x").type_str(), "noul");
        assert_eq!(DecisionQuestion::score("x", ["a", "b"]).type_str(), "score");
        let noul = DecisionAnswer::Noul { noul: 0.9 };
        let choice = DecisionAnswer::Choice {
            choice: "a".into(),
            probabilities: BTreeMap::from([("a".to_string(), 1.0)]),
            confidence: 1.0,
        };
        let score = DecisionAnswer::Score {
            score: 2.0,
            legend: BTreeMap::new(),
            probabilities: BTreeMap::new(),
            confidence: 1.0,
        };
        assert_eq!(
            (noul.type_str(), choice.type_str(), score.type_str()),
            ("noul", "choice", "score")
        );
        assert_eq!(noul.noul(), Some(0.9));
        assert_eq!((choice.noul(), score.noul()), (None, None));
        assert_eq!(choice.chosen(), Some("a"));
        assert_eq!((noul.chosen(), score.chosen()), (None, None));
        assert_eq!(score.score(), Some(2.0));
        assert_eq!((noul.score(), choice.score()), (None, None));
    }

    #[test]
    fn a_point_prints_as_its_key_and_an_outcome_refuses_a_word_it_does_not_know() {
        for point in DecisionPoint::ALL {
            assert_eq!(point.to_string(), point.as_str());
        }
        assert!(matches!(
            "nope".parse::<JudgementOutcome>(),
            Err(crate::CoreError::UnknownDecisionWord {
                what: "judgement outcome",
                ..
            })
        ));
    }

    #[test]
    fn a_request_refuses_a_blank_question_id_and_a_blank_option() {
        let blank_id = DecisionRequest::one("s", " ", DecisionQuestion::noul("why"));
        assert!(matches!(
            blank_id.validate(),
            Err(DecisionContractError::EmptyQuestionId)
        ));
        let blank_option = DecisionRequest::one(
            "s",
            "pick",
            DecisionQuestion::choice("which", [(" ", "blank"), ("b", "fine")]),
        );
        assert!(matches!(
            blank_option.validate(),
            Err(DecisionContractError::EmptyOption(id)) if id == "pick"
        ));
    }

    #[test]
    fn an_answer_is_refused_when_its_choice_has_no_probability_or_a_score_names_a_stray_outcome_or_is_no_number(
    ) {
        let request = routed();
        let mut response = choice("small", 0.5, 0.5, 0.9);
        if let Some(DecisionAnswer::Choice { probabilities, .. }) =
            response.answers.get_mut("model")
        {
            probabilities.remove("small");
        }
        assert!(matches!(
            response.check(&request),
            Err(DecisionContractError::ChoiceWithoutProbability(id)) if id == "model"
        ));

        let rated = rated();
        let score =
            |score: f64, legend: &[(&str, &str)], probabilities: &[(&str, f64)]| DecisionResponse {
                model: "jev".into(),
                answers: BTreeMap::from([(
                    "fit".to_string(),
                    DecisionAnswer::Score {
                        score,
                        legend: legend
                            .iter()
                            .map(|(k, v)| (k.to_string(), v.to_string()))
                            .collect(),
                        probabilities: probabilities
                            .iter()
                            .map(|(k, v)| (k.to_string(), *v))
                            .collect(),
                        confidence: 0.8,
                    },
                )]),
                usage: DecisionUsage::default(),
            };
        let numbered = [("1", "no"), ("2", "partly"), ("3", "yes")];
        assert!(matches!(
            score(2.0, &numbered, &[("9", 1.0)]).check(&rated),
            Err(DecisionContractError::UnknownOutcome { outcome, .. }) if outcome == "9"
        ));
        let worded = [("low", "no"), ("mid", "partly"), ("high", "yes")];
        assert!(matches!(
            score(f64::NAN, &worded, &[("low", 1.0)]).check(&rated),
            Err(DecisionContractError::ScoreOffTheLegend { .. })
        ));
    }

    // added by the coverage pass: b5-decision.rs
    #[test]
    fn a_request_grows_by_a_question_is_bounded_in_bytes_and_an_unknown_provider_word_is_refused() {
        let two = urgent().with(
            "model",
            DecisionQuestion::choice("Which?", [("small", "quick"), ("large", "deep")]),
        );
        assert_eq!(two.questions.len(), 2);
        let big = DecisionRequest::one(
            "x".repeat(MAX_REQUEST_BYTES + 1),
            "q",
            DecisionQuestion::choice("Which?", [("a", "x"), ("b", "y")]),
        );
        assert!(matches!(
            big.validate(),
            Err(DecisionContractError::TooLarge { bytes }) if bytes > MAX_REQUEST_BYTES
        ));
        assert!(matches!(
            "nope".parse::<DecisionProviderKind>(),
            Err(crate::CoreError::UnknownDecisionWord { what: "decision provider", value }) if value == "nope"
        ));
    }

    // added by the coverage pass: b7-decision.rs
    #[test]
    fn a_score_whose_legend_is_not_numbered_is_judged_by_its_probabilities_alone() {
        let worded = DecisionResponse {
            model: "m".into(),
            answers: BTreeMap::from([(
                "fit".to_string(),
                DecisionAnswer::Score {
                    score: 2.0,
                    legend: BTreeMap::from([
                        ("no".to_string(), "not at all".to_string()),
                        ("partly".to_string(), "in part".to_string()),
                        ("yes".to_string(), "wholly".to_string()),
                    ]),
                    probabilities: BTreeMap::from([
                        ("no".to_string(), 0.2),
                        ("partly".to_string(), 0.3),
                        ("yes".to_string(), 0.5),
                    ]),
                    confidence: 0.5,
                },
            )]),
            usage: DecisionUsage::default(),
        };
        worded.check(&rated()).unwrap();
    }
}
