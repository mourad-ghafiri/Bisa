//! The three-tier session event vocabulary.
//!
//! Subscribers pick their tier (omp's channel-split lesson): an orchestrator
//! watches `Lifecycle`, a dashboard adds `Progress`, a debugger adds `Raw`.
//! Events are advisory; [`crate::SessionSnapshot`] is the authority.

use bisa_core::ToolTier;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A harness's own id for a sub-agent it spawned — Claude Code's `Task`
/// tool-use id, say. Opaque; only equality matters.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct SubagentId(pub String);

impl std::fmt::Display for SubagentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "tier", content = "event")]
pub enum SessionEvent {
    Lifecycle(LifecycleEvent),
    Progress(ProgressEvent),
    /// Harness-native passthrough for debugging; never parsed by the engine.
    Raw(serde_json::Value),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum LifecycleEvent {
    Started,
    /// The harness's process is up and the driver owns it; `pid` is its OS pid
    /// where the adapter runs one (`None` for an in-process or remote harness)
    /// — the root the desktop traces a listening port back to. Not
    /// a field on `Started`, which fires before any process exists.
    ProcessStarted {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pid: Option<u32>,
    },
    /// `is_terminal: false` means a retry/continuation is coming — treat as
    /// run completion only when `is_terminal` (the field pi and omp both had
    /// to retrofit; here from day one).
    Ended {
        outcome: Outcome,
        is_terminal: bool,
    },
    /// Session disposed but resumable via its resume token.
    Parked,
    Revived,
    /// The harness stopped and will not continue until
    /// [`crate::HarnessSession::answer`] is called with this request's id, or
    /// the session is aborted. Only adapters with
    /// [`bisa_core::HarnessCaps::INPUT_REQUESTS`] emit it, and they
    /// never decide it themselves: the engine does.
    InputRequested {
        request: InputRequest,
    },
    /// The answer reached the harness; the session is no longer blocked on
    /// `id`.
    InputResolved {
        id: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "outcome")]
pub enum Outcome {
    Completed,
    Aborted,
    Failed {
        error: String,
    },
    /// The session had already started and then hit a wall belonging to the
    /// **model**, not to the work: a quota exhausted mid-turn, a rate limit,
    /// a model retired under it.
    ///
    /// This is a *retry on the next model*, never a failure. It is the
    /// mid-turn twin of [`crate::HarnessError::ModelUnavailable`] — the same
    /// fact, arriving after `launch()` has already returned a session, so it
    /// has to come through the event stream rather than a `Result`.
    ///
    /// `retry_after` is seconds, present only when the harness said so.
    ModelUnavailable {
        model: String,
        reason: String,
        retry_after: Option<u64>,
    },
    /// Interrupted with a path back (crash with recoverable journal,
    /// deferred on a missing dependency).
    Suspended {
        reason: String,
    },
}

/// Something a harness asked for and stopped on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct InputRequest {
    /// Correlates the engine's answer back to the harness.
    pub id: String,
    #[serde(flatten)]
    pub kind: InputKind,
    /// The sub-agent that raised it, when the harness says so.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<SubagentId>,
}

/// What kind of input a harness stopped for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind")]
#[schemars(rename = "InputRequestKind")]
pub enum InputKind {
    /// May this tool run? Answered `Allow` or `Deny`.
    Permission {
        tool_name: String,
        tier: ToolTier,
        /// Short rendering of the arguments, already truncated by the adapter.
        args_summary: String,
        /// The tool's raw input as the harness carries it — what the guard
        /// reads a command and its paths from, and what an `Allow` may hand
        /// back rewritten. `Null` when the harness does not say.
        #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
        input: serde_json::Value,
    },
    /// The agent asked the person something. Answered `Text`.
    Question {
        text: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        options: Vec<String>,
    },
    /// The harness needs a sign-in it cannot do itself. Answered `Allow` once
    /// the person has done it, or `Deny`.
    Auth {
        provider: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        url: Option<String>,
    },
}

/// The engine's answer to an [`InputRequest`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "answer")]
pub enum InputAnswer {
    /// Run it — with this input instead of the one asked about, when given
    /// (a placeholder restored right before execution). A harness without
    /// `INPUT_REWRITE` runs the original.
    Allow {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        input: Option<serde_json::Value>,
    },
    Deny {
        reason: String,
    },
    Text {
        text: String,
    },
}

impl InputAnswer {
    /// An allow that runs the call as asked.
    pub const ALLOW: InputAnswer = InputAnswer::Allow { input: None };
}

impl InputRequest {
    pub fn permission(
        id: impl Into<String>,
        tool_name: impl Into<String>,
        tier: ToolTier,
        args_summary: impl Into<String>,
        input: serde_json::Value,
    ) -> Self {
        Self {
            id: id.into(),
            kind: InputKind::Permission {
                tool_name: tool_name.into(),
                tier,
                args_summary: args_summary.into(),
                input,
            },
            parent: None,
        }
    }

    pub fn question(id: impl Into<String>, text: impl Into<String>, options: Vec<String>) -> Self {
        Self {
            id: id.into(),
            kind: InputKind::Question {
                text: text.into(),
                options,
            },
            parent: None,
        }
    }

    pub fn auth(id: impl Into<String>, provider: impl Into<String>, url: Option<String>) -> Self {
        Self {
            id: id.into(),
            kind: InputKind::Auth {
                provider: provider.into(),
                url,
            },
            parent: None,
        }
    }

    pub fn raised_by(mut self, parent: Option<SubagentId>) -> Self {
        self.parent = parent;
        self
    }

    /// One short clause a person reads: *run Bash*, the question, *sign in to X*.
    pub fn summary(&self) -> String {
        match &self.kind {
            InputKind::Permission { tool_name, .. } => format!("run {tool_name}"),
            InputKind::Question { text, .. } => text.clone(),
            InputKind::Auth { provider, .. } => format!("sign in to {provider}"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum ProgressEvent {
    TurnStarted,
    TurnEnded,
    /// The harness spawned a sub-agent of its own (Claude Code's `Task`).
    /// Everything the sub-agent does arrives wrapped in [`Self::Nested`].
    SubagentStarted {
        id: SubagentId,
        /// The harness's name for the kind of sub-agent (`explore`, `plan`…).
        name: String,
        /// What it was asked to do, already truncated by the adapter.
        description: String,
    },
    SubagentEnded {
        id: SubagentId,
        ok: bool,
    },
    /// A progress event raised inside a sub-agent. Kept as a wrapper rather
    /// than a field on every variant so a consumer that reads the parent's
    /// stream — the chat reply pump, the work-item transcript — never mistakes
    /// a sub-agent's text or tools for the agent's own.
    Nested {
        parent: SubagentId,
        event: Box<ProgressEvent>,
    },
    ToolStarted {
        name: String,
        /// Short rendering of the arguments, already truncated by the adapter.
        args_summary: String,
        tier: ToolTier,
    },
    ToolEnded {
        name: String,
        ok: bool,
    },
    /// Streaming assistant text.
    TextDelta {
        text: String,
    },
    /// The model's reasoning as it streams — what a harness shows as
    /// *thinking*. Never an instruction and never the reply: the engine
    /// keeps it beside the words, for a person who wants to read how the
    /// answer was reached.
    ThinkingDelta {
        text: String,
    },
    CostDelta {
        input_tokens: u64,
        output_tokens: u64,
        usd_cents: u64,
    },
    /// The harness now runs on this model — said at start, when the harness
    /// names it, and again whenever it switches (Claude Code's own fallback,
    /// a person's `/model` in a terminal). The harness-native id, as the
    /// harness spells it. The one way a roster row's model moves after the
    /// launch that registered it.
    ModelChanged {
        model: String,
    },
}

impl ProgressEvent {
    /// Wrap this event as raised by `parent` — or leave it as is when there is no parent.
    pub fn raised_by(self, parent: Option<SubagentId>) -> Self {
        match parent {
            Some(parent) => ProgressEvent::Nested {
                parent,
                event: Box::new(self),
            },
            None => self,
        }
    }

    /// The sub-agent this event was raised in, when it was.
    pub fn parent(&self) -> Option<&SubagentId> {
        match self {
            ProgressEvent::Nested { parent, .. } => Some(parent),
            _ => None,
        }
    }
}

impl SessionEvent {
    /// Convenience: is this a terminal end-of-session event?
    pub fn is_terminal_end(&self) -> bool {
        matches!(
            self,
            SessionEvent::Lifecycle(LifecycleEvent::Ended {
                is_terminal: true,
                ..
            })
        )
    }

    /// The sub-agent this event was raised in, when it was.
    pub fn parent(&self) -> Option<&SubagentId> {
        match self {
            SessionEvent::Progress(p) => p.parent(),
            SessionEvent::Lifecycle(LifecycleEvent::InputRequested { request }) => {
                request.parent.as_ref()
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_input_request_flattens_its_kind_on_the_wire() {
        let req = InputRequest::permission(
            "r1",
            "Bash",
            ToolTier::Exec,
            "ls -la",
            serde_json::json!({ "command": "ls -la" }),
        );
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["id"], "r1");
        assert_eq!(json["kind"], "permission");
        assert_eq!(json["tool_name"], "Bash");
        assert_eq!(json["input"]["command"], "ls -la");
        assert!(
            json.get("parent").is_none(),
            "an absent parent is not on the wire"
        );
        let bare = InputRequest::permission(
            "r2",
            "approval",
            ToolTier::Exec,
            "",
            serde_json::Value::Null,
        );
        assert!(
            serde_json::to_value(&bare).unwrap().get("input").is_none(),
            "a harness that carries no input puts none on the wire"
        );
        let back: InputRequest = serde_json::from_value(json).unwrap();
        assert_eq!(back, req);
        assert_eq!(req.summary(), "run Bash");
        assert_eq!(
            InputRequest::question("q", "Which tone?", vec![]).summary(),
            "Which tone?"
        );
        assert_eq!(
            InputRequest::auth("a", "github", None).summary(),
            "sign in to github"
        );
    }

    #[test]
    fn a_nested_event_names_its_parent_and_a_plain_one_has_none() {
        let inner = ProgressEvent::ToolStarted {
            name: "Read".into(),
            args_summary: "x".into(),
            tier: ToolTier::Read,
        };
        let plain = SessionEvent::Progress(inner.clone().raised_by(None));
        assert!(plain.parent().is_none());
        let nested = SessionEvent::Progress(inner.raised_by(Some(SubagentId("toolu_1".into()))));
        assert_eq!(nested.parent().map(|p| p.0.as_str()), Some("toolu_1"));
        let json = serde_json::to_value(&nested).unwrap();
        assert_eq!(json["event"]["type"], "nested");
        assert_eq!(json["event"]["event"]["type"], "tool_started");
    }

    #[test]
    fn answers_are_tagged() {
        let allow = serde_json::to_value(InputAnswer::ALLOW).unwrap();
        assert_eq!(allow["answer"], "allow");
        assert!(
            allow.get("input").is_none(),
            "an allow as asked carries no input"
        );
        let rewritten = serde_json::to_value(InputAnswer::Allow {
            input: Some(serde_json::json!({ "command": "echo x" })),
        })
        .unwrap();
        assert_eq!(rewritten["input"]["command"], "echo x");
        assert_eq!(
            serde_json::to_value(InputAnswer::Deny {
                reason: "no".into()
            })
            .unwrap()["reason"],
            "no"
        );
    }
}
