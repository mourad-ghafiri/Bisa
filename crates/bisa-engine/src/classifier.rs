//! The Auto Classifier: one bounded model call about one redacted subject — a
//! tool call, a message from another node, or content an agent is about to
//! read from outside (`crate::content`).
//!
//! Three readers, one verdict ([`ClassifierProvider`], chosen by
//! `security.classifier.provider`):
//!
//! - **an agent** — the classifier agent, on its own harness and models;
//! - **a harness** with one of its models, at the effort
//!   `security.classifier.effort` names, no agent in between;
//! - **the Decision-Making Agent** — asked which harm, if any, the subject is
//!   (`bisa_security::classify::TOOL_HARMS`), through [`crate::decider`], so its
//!   provider, its contract check, its retries and its record are the
//!   Decision-Making Agent's own.
//!
//! The first two ride [`crate::ask::ask_once`], which is what makes them safe
//! to put in the path of a permission: the session has no MCP door and a `Read`
//! ceiling, one deadline covers launch, prompt and turn, and every exit is an
//! `Err` rather than a plausible string the model did not say. Their answer is
//! read strictly — `SAFE`, or `HARMFUL: <why>` — and anything else is *no
//! verdict*. The Decision-Making Agent's is read as strictly: a choice it is
//! sure of to `decisions.confidence.security`, of a harm the question offered;
//! an unsure answer, a failed one and a word of its own are all no verdict.
//!
//! Whoever reads, the prompt goes out redacted, and the caller
//! ([`crate::security::decide_tool`]) turns no verdict into a question for the
//! person. **Nothing here ever allows.**
//!
//! Verdicts are cached per process by the digest of the redacted subject: the
//! same call asks the model once.

use bisa_core::{DecisionPoint, DecisionQuestion, DecisionRequest, Home};
use bisa_security::classify::{
    content_block, content_digest, content_prompt, message_block, message_digest, message_prompt,
    parse_verdict, prompt, subject_block, subject_digest, verdict_of_choice, MessageSubject,
    PageSubject, Subject, Verdict, CONTENT_HARMS, CONTENT_QUESTION, MESSAGE_HARMS,
    MESSAGE_QUESTION, TOOL_HARMS, TOOL_QUESTION,
};

use crate::ask::{ask_once, Whom};
use crate::decider::{self, Judged, Standing};
use crate::security::{ClassifierProvider, ClassifierSettings};
use crate::{EngineError, Inner};

/// The one question the Decision-Making Agent is asked about a subject.
const HARM_QUESTION: &str = "harm";

/// What one reading is of: the brief-and-subject a generative reader is
/// prompted with, and the question a decision model is asked instead.
struct Reading<'a> {
    prompt: String,
    state: String,
    question: &'static str,
    harms: &'static [(&'static str, &'static str)],
    point: DecisionPoint,
    home: Option<Home>,
    digest: &'a str,
}

/// Ask the classifier about one tool call, on its home — a goal, or a run of
/// the workspace — when it has one.
pub async fn classify(
    inner: &Inner,
    subject: &Subject,
    settings: &ClassifierSettings,
    home: Option<Home>,
) -> Result<Verdict, EngineError> {
    let digest = subject_digest(subject);
    read(
        inner,
        settings,
        Reading {
            prompt: prompt(subject),
            state: subject_block(subject),
            question: TOOL_QUESTION,
            harms: TOOL_HARMS,
            point: DecisionPoint::SecurityTool,
            home,
            digest: &digest,
        },
    )
    .await
}

/// Ask the classifier about one message from another node.
pub async fn classify_message(
    inner: &Inner,
    subject: &MessageSubject,
    settings: &ClassifierSettings,
) -> Result<Verdict, EngineError> {
    let digest = message_digest(subject);
    read(
        inner,
        settings,
        Reading {
            prompt: message_prompt(subject),
            state: message_block(subject),
            question: MESSAGE_QUESTION,
            harms: MESSAGE_HARMS,
            point: DecisionPoint::SecurityMessage,
            home: None,
            digest: &digest,
        },
    )
    .await
}

/// Ask the classifier about content an agent is about to read from outside
/// — a page, a review (`crate::content`), on its home when it has one.
pub async fn classify_content(
    inner: &Inner,
    subject: &PageSubject,
    settings: &ClassifierSettings,
    home: Option<Home>,
) -> Result<Verdict, EngineError> {
    let digest = content_digest(subject);
    read(
        inner,
        settings,
        Reading {
            prompt: content_prompt(subject),
            state: content_block(subject),
            question: CONTENT_QUESTION,
            harms: CONTENT_HARMS,
            point: DecisionPoint::SecurityContent,
            home,
            digest: &digest,
        },
    )
    .await
}

async fn read(
    inner: &Inner,
    settings: &ClassifierSettings,
    reading: Reading<'_>,
) -> Result<Verdict, EngineError> {
    if let Some(cached) = inner.security.cached_verdict(reading.digest) {
        return Ok(cached);
    }
    let verdict = match &settings.provider {
        ClassifierProvider::Agent => {
            prompted(
                inner,
                settings,
                Whom::Agent(settings.agent.clone()),
                &reading,
            )
            .await?
        }
        ClassifierProvider::Harness => {
            let model = settings.model.trim();
            // The classifier's model works as hard as its setting says: a
            // verdict's session asks nobody.
            let whom = Whom::Harness {
                harness: settings.harness.clone(),
                model: (!model.is_empty()).then(|| model.to_string()),
                effort: Some(settings.effort),
            };
            prompted(inner, settings, whom, &reading).await?
        }
        ClassifierProvider::DecisionMakingAgent => judged(inner, &reading).await?,
    };
    inner
        .security
        .cache_verdict(reading.digest.to_string(), verdict.clone());
    Ok(verdict)
}

/// A generative reader: one prompt, one line back.
async fn prompted(
    inner: &Inner,
    settings: &ClassifierSettings,
    whom: Whom,
    reading: &Reading<'_>,
) -> Result<Verdict, EngineError> {
    // Boxed: the classifier's own session answers permissions through the
    // same funnel that called us, and the compiler needs the cycle cut here.
    // The cycle never runs, because a classifier session is judged with the
    // classifier off (`Judge::classifier`).
    let asked: Result<crate::ask::Asked, EngineError> = Box::pin(ask_once(
        inner,
        &whom,
        crate::ask::Asking::of(bisa_core::AskPurpose::Classifier).on(reading.home),
        &reading.prompt,
        None,
        settings.deadline(),
    ))
    .await;
    let asked = asked.map_err(|e| EngineError::Security(e.to_string()))?;
    parse_verdict(&asked.text).map_err(|e| EngineError::Security(e.to_string()))
}

/// The Decision-Making Agent: which harm, if any — sure, or no verdict.
async fn judged(inner: &Inner, reading: &Reading<'_>) -> Result<Verdict, EngineError> {
    let request = DecisionRequest::one(
        reading.state.clone(),
        HARM_QUESTION,
        DecisionQuestion::choice(reading.question, reading.harms.iter().copied()),
    );
    let standing = Standing::on(reading.home);
    // Boxed for the reason `prompted` is: answered by a harness, the
    // Decision-Making Agent answers its own permissions through the funnel
    // that called us.
    let judged: Judged = Box::pin(decider::judge(inner, reading.point, &standing, request)).await;
    let no_verdict = |why: String| EngineError::Security(why);
    match judged {
        Judged::Answered(response) => {
            let choice = response
                .answer(HARM_QUESTION)
                .and_then(|a| a.chosen())
                .ok_or_else(|| no_verdict("the Decision-Making Agent chose nothing".into()))?;
            verdict_of_choice(reading.harms, choice).map_err(|e| no_verdict(e.to_string()))
        }
        Judged::Unsure(response) => Err(no_verdict(format!(
            "the Decision-Making Agent was sure only to {:.2}",
            response.certainty()
        ))),
        Judged::Failed(why) => Err(no_verdict(why)),
        Judged::Off => Err(no_verdict("the Decision-Making Agent was not asked".into())),
    }
}
