//! The ask a conversation about a checkout answers in place (ide/20).
//!
//! A turn there has no goal, so there is no Inbox gate to open; before this
//! desk an `ask` was a refusal. Now the call parks here, the pane draws a
//! card at the foot of the timeline, and the person answers where they are
//! reading: *Allow once*, *Allow for this conversation*, or *Deny* — with a
//! note the agent hears as the reason. Two things are asked about
//! ([`AskSubject`]): a tool call the guard put to the person, and content
//! from outside the screen held — a page, a review — with its source, the
//! reason and an excerpt (11-security §What an agent reads from outside).
//!
//! An ask lives as long as its session: nothing is stored, a restart leaves
//! none behind, and a session that goes away answers its own asks *no*.
//! *For this conversation* covers the ceiling's asks for that tool, or the
//! content of that host — never a guard rule's: a rule that says `ask` asks
//! every time — and holds while the conversation stays in the mode it was
//! given in: a change of mode ends it.

use super::now_secs;
use crate::events::{EngineEvent, EnginePayload};
use crate::{EngineError, Inner};
use bisa_core::{ConversationId, ToolTier};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use tokio::sync::oneshot;

/// What an ask is about.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AskSubject {
    /// A tool call the guard put to the person.
    Tool { tool: String, tier: ToolTier },
    /// Content from outside the screen held before the agent read it: where
    /// it came from in words, its URL when it is a page, why it was held
    /// (the classifier's sentence, or *no verdict*), and an excerpt — all
    /// redacted already.
    Content {
        source: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        url: Option<String>,
        reason: String,
        excerpt: String,
    },
}

impl AskSubject {
    /// The key *for this conversation* remembers: the tool's name, or the
    /// content's host under `content:`, so a page never allows a tool.
    pub fn grant_key(&self) -> String {
        match self {
            AskSubject::Tool { tool, .. } => tool.clone(),
            AskSubject::Content { source, .. } => format!("content:{source}"),
        }
    }

    /// What is asked for, as the verb phrase after the agent's name — *to
    /// use `git push`*, *to read a page at example.org* — for a line of text
    /// (the CLI's activity feed) that has no card to lay the subject out on.
    pub fn words(&self) -> String {
        match self {
            AskSubject::Tool { tool, .. } => format!("to use {tool}"),
            AskSubject::Content {
                source,
                url: Some(url),
                ..
            } => format!("to read {source} at {url}"),
            AskSubject::Content {
                source, url: None, ..
            } => format!("to read {source}"),
        }
    }
}

/// An ask as the pane draws it. `question` is already redacted.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AskView {
    pub id: String,
    pub agent: String,
    pub subject: AskSubject,
    pub question: String,
    /// Whether *for this conversation* is on offer: only for the mode's own
    /// ask or the screen's, never a guard rule's.
    pub grantable: bool,
    pub opened_at: u64,
}

/// How far an *Allow* reaches.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum AskScope {
    #[default]
    Once,
    Conversation,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case", tag = "answer", deny_unknown_fields)]
pub enum AskAnswer {
    Allow {
        #[serde(default)]
        scope: AskScope,
    },
    Deny {
        /// What the agent should do instead — it hears this as the reason.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        note: Option<String>,
    },
}

struct Waiting {
    view: AskView,
    answer: oneshot::Sender<AskAnswer>,
}

#[derive(Default)]
pub struct AskDesk {
    waiting: DashMap<ConversationId, Vec<Waiting>>,
    /// What a person allowed for the rest of a conversation, by grant key —
    /// a tool's name, or `content:<host>`.
    granted: DashMap<ConversationId, HashSet<String>>,
}

impl AskDesk {
    /// The conversation moved to another mode: what was allowed *for this
    /// conversation* was allowed under the ceiling it was asked at, so it
    /// ends there. Asks still waiting stay — their question is unchanged.
    pub(crate) fn revoke_grants(&self, conversation: ConversationId) {
        if let Some((_, tools)) = self.granted.remove(&conversation) {
            tracing::info!(target: "bisa_engine::changes", %conversation, tools = tools.len(), "a mode change ended what was allowed for the conversation");
        }
    }

    pub(crate) fn forget(&self, conversation: ConversationId) {
        self.waiting.remove(&conversation);
        self.granted.remove(&conversation);
    }
}

/// What the person said, as the caller acts on it.
pub enum Said {
    Allow,
    Deny(String),
}

const REFUSED: &str = "refused by the person in the conversation";

/// Ask the person in the conversation and wait for their word. `grantable`
/// asks answer at once when the subject's key was allowed for the
/// conversation (`AskSubject::grant_key`).
pub async fn ask(
    inner: &Inner,
    conversation: ConversationId,
    agent: &str,
    subject: AskSubject,
    question: String,
    grantable: bool,
) -> Said {
    let desk = &inner.changes.asks;
    let grant_key = subject.grant_key();
    if grantable
        && desk
            .granted
            .get(&conversation)
            .is_some_and(|keys| keys.contains(&grant_key))
    {
        return Said::Allow;
    }
    let (tx, rx) = oneshot::channel();
    let view = AskView {
        id: ulid::Ulid::from_datetime(std::time::SystemTime::now()).to_string(),
        agent: agent.to_string(),
        subject,
        question,
        grantable,
        opened_at: now_secs(),
    };
    desk.waiting.entry(conversation).or_default().push(Waiting {
        view: view.clone(),
        answer: tx,
    });
    inner.emit(EngineEvent::global(EnginePayload::AskOpened {
        conversation: conversation.to_string(),
        ask: view.clone(),
    }));
    // A dropped sender — the session went away, the conversation was
    // deleted — is a no.
    let answer = rx.await.unwrap_or(AskAnswer::Deny { note: None });
    let allowed = matches!(answer, AskAnswer::Allow { .. });
    inner.emit(EngineEvent::global(EnginePayload::AskSettled {
        conversation: conversation.to_string(),
        ask_id: view.id,
        allowed,
    }));
    match answer {
        AskAnswer::Allow { scope } => {
            if grantable && scope == AskScope::Conversation {
                desk.granted
                    .entry(conversation)
                    .or_default()
                    .insert(grant_key);
            }
            Said::Allow
        }
        AskAnswer::Deny { note } => Said::Deny(
            match note.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
                Some(note) => format!("{REFUSED}: {note}"),
                None => REFUSED.to_string(),
            },
        ),
    }
}

/// The asks of a conversation still waiting, oldest first.
pub fn open(inner: &Inner, conversation: ConversationId) -> Vec<AskView> {
    inner
        .changes
        .asks
        .waiting
        .get(&conversation)
        .map(|w| w.iter().map(|w| w.view.clone()).collect())
        .unwrap_or_default()
}

/// Answer one. An id that is not waiting is an error in a sentence: it was
/// answered already, or its turn ended.
pub fn answer(
    inner: &Inner,
    conversation: ConversationId,
    ask_id: &str,
    answer: AskAnswer,
) -> Result<(), EngineError> {
    let waiting = {
        let mut entry = inner.changes.asks.waiting.entry(conversation).or_default();
        entry
            .iter()
            .position(|w| w.view.id == ask_id)
            .map(|at| entry.remove(at))
    };
    let Some(waiting) = waiting else {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-question-no-longer-waiting-was-answered-turn"
        )));
    };
    // The turn may have been stopped between the read and the answer: then
    // nobody hears it, and saying it was taken would be a lie.
    if waiting.answer.send(answer).is_err() {
        tracing::debug!(target: "bisa_engine::changes", %conversation, ask = ask_id, "an ask was answered after its turn ended");
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-question-no-longer-waiting-turn-ended-before"
        )));
    }
    Ok(())
}

/// The session of `agent` in the conversation went away: its asks are
/// answered *no* by their senders dropping.
pub(crate) fn drop_asks_of(inner: &Inner, conversation: ConversationId, agent: &str) {
    if let Some(mut entry) = inner.changes.asks.waiting.get_mut(&conversation) {
        entry.retain(|w| w.view.agent != agent);
    }
}
