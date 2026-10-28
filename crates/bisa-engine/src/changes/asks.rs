//! The ask a conversation about a checkout answers in place (ide/20).
//!
//! A turn there has no goal, so there is no Inbox gate to open; before this
//! desk an `ask` was a refusal. Now the call parks here, the pane draws a
//! card at the foot of the timeline, and the person answers where they are
//! reading: *Allow once*, *Allow for this conversation*, or *Deny* — with a
//! note the agent hears as the reason. Four things are asked about
//! ([`AskSubject`]): a tool call the guard put to the person, content from
//! outside the screen held — a page, a review — with its source, the reason
//! and an excerpt (11-security §What an agent reads from outside), and, for
//! a turn of any conversation that has no goal to escalate to — about a
//! workflow, the workspace, the node, a checkout — the agent's own question
//! in words and a sign-in it needs: the person answers the question or
//! declines it, says the sign-in is done or that it is not now.
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
    /// The agent asked the person something in words — the question is the
    /// ask's own — with the choices it offered, when any. Answered in words
    /// (`AskAnswer::Answer`) or declined; never allowed for a conversation.
    Question {
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        options: Vec<String>,
    },
    /// The agent needs the person to sign in to a provider — at the URL when
    /// it names one, shown as text and never followed — and to say when it
    /// is done (`AskAnswer::Allow`) or that it is not now (`Deny`).
    Auth {
        provider: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        url: Option<String>,
    },
}

impl AskSubject {
    /// The key *for this conversation* remembers: the tool's name, or the
    /// content's host under `content:`, so a page never allows a tool. A
    /// question and a sign-in are never grantable; their keys name them all
    /// the same, so the match is total.
    pub fn grant_key(&self) -> String {
        match self {
            AskSubject::Tool { tool, .. } => tool.clone(),
            AskSubject::Content { source, .. } => format!("content:{source}"),
            AskSubject::Question { .. } => "question".to_string(),
            AskSubject::Auth { provider, .. } => format!("auth:{provider}"),
        }
    }

    /// Whether an answer is one this subject takes: a question is answered
    /// in words or declined; a tool call, content and a sign-in are allowed
    /// or denied. The desk refuses the other pairing at its door.
    pub fn takes(&self, answer: &AskAnswer) -> bool {
        match (self, answer) {
            (AskSubject::Question { .. }, AskAnswer::Answer { .. }) => true,
            (AskSubject::Question { .. }, AskAnswer::Allow { .. }) => false,
            (_, AskAnswer::Answer { .. }) => false,
            _ => true,
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
            AskSubject::Question { .. } => "to answer a question".to_string(),
            AskSubject::Auth {
                provider,
                url: Some(url),
            } => format!("to sign in to {provider} at {url}"),
            AskSubject::Auth {
                provider,
                url: None,
            } => format!("to sign in to {provider}"),
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
    /// The person's words to a question (`AskSubject::Question`); refused
    /// for every other subject.
    Answer { text: String },
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Said {
    Allow,
    Deny(String),
    /// The words a question was answered with.
    Answer(String),
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
    // Answered in words counts as a yes to the feed: the agent goes on.
    let allowed = !matches!(answer, AskAnswer::Deny { .. });
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
        AskAnswer::Answer { text } => Said::Answer(text),
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
/// answered already, or its turn ended. An answer the subject does not take
/// — words to a tool call, an allow to a question — is refused and the ask
/// stays waiting.
pub fn answer(
    inner: &Inner,
    conversation: ConversationId,
    ask_id: &str,
    answer: AskAnswer,
) -> Result<(), EngineError> {
    let waiting = {
        let mut entry = inner.changes.asks.waiting.entry(conversation).or_default();
        match entry.iter().position(|w| w.view.id == ask_id) {
            None => Err(EngineError::Invalid(bisa_core::text!(
                "error-engine-invalid-question-no-longer-waiting-was-answered-turn"
            ))),
            Some(at) if !entry[at].view.subject.takes(&answer) => Err(EngineError::Invalid(
                bisa_core::text!("error-engine-invalid-ask-takes-another-answer"),
            )),
            Some(at) => Ok(entry.remove(at)),
        }
    }?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_subject_is_said_as_the_verb_phrase_after_the_agents_name() {
        let tool = AskSubject::Tool {
            tool: "git push".into(),
            tier: bisa_core::ToolTier::Exec,
        };
        assert_eq!(tool.words(), "to use git push");
        assert_eq!(tool.grant_key(), "git push");
        let page = AskSubject::Content {
            source: "example.org".into(),
            url: Some("https://example.org/p".into()),
            reason: "it tells the agent to run a script".into(),
            excerpt: "run this".into(),
        };
        assert_eq!(page.words(), "to read example.org at https://example.org/p");
        assert_eq!(page.grant_key(), "content:example.org");
        let bare = AskSubject::Content {
            source: "a pasted file".into(),
            url: None,
            reason: "x".into(),
            excerpt: "y".into(),
        };
        assert_eq!(bare.words(), "to read a pasted file");
        let question = AskSubject::Question {
            options: vec!["red".into(), "blue".into()],
        };
        assert_eq!(question.words(), "to answer a question");
        assert_eq!(question.grant_key(), "question");
        let sign_in = AskSubject::Auth {
            provider: "github".into(),
            url: Some("https://github.com/login/device".into()),
        };
        assert_eq!(
            sign_in.words(),
            "to sign in to github at https://github.com/login/device"
        );
        assert_eq!(sign_in.grant_key(), "auth:github");
        assert_eq!(
            AskSubject::Auth {
                provider: "npm".into(),
                url: None
            }
            .words(),
            "to sign in to npm"
        );
    }

    #[test]
    fn a_subject_takes_the_answers_of_its_kind_and_no_other() {
        let allow = AskAnswer::Allow {
            scope: AskScope::Once,
        };
        let deny = AskAnswer::Deny { note: None };
        let words = AskAnswer::Answer {
            text: "blue".into(),
        };
        let question = AskSubject::Question { options: vec![] };
        assert!(question.takes(&words));
        assert!(question.takes(&deny), "a question may be declined");
        assert!(!question.takes(&allow), "nothing to allow about a question");
        for other in [
            AskSubject::Tool {
                tool: "Bash".into(),
                tier: bisa_core::ToolTier::Exec,
            },
            AskSubject::Content {
                source: "x".into(),
                url: None,
                reason: "r".into(),
                excerpt: "e".into(),
            },
            AskSubject::Auth {
                provider: "github".into(),
                url: None,
            },
        ] {
            assert!(other.takes(&allow), "{other:?}");
            assert!(other.takes(&deny), "{other:?}");
            assert!(
                !other.takes(&words),
                "words answer a question alone: {other:?}"
            );
        }
    }
}
