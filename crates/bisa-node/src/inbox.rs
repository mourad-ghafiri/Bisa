//! `GET /inbox`, and the frames that keep a row current.
//!
//! The Inbox itself — which things earn a row, what each owes and what
//! happened to it, in which order — is the engine's (`bisa_engine::inbox`):
//! one builder, read here for the route, for the goal page's and the run
//! page's asks, for each `inbox` frame, and by the command line with no node
//! at all, so they cannot disagree. This module is the door: the two words a
//! request may narrow by — a bucket (`?filter=`) and a source (`?source=`) —
//! refused by name when they are neither, and the engine's view of what is
//! live (its open gates, its terminal harnesses) handed to the builder.

use crate::dto::*;
use crate::route_docs::RouteDoc;
use crate::Query;
use crate::{ApiError, Shared};
use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use bisa_core::{Goal, WorkflowRun};
use bisa_engine::inbox::{self, LiveView, Reader};
use serde::Deserialize;
use serde_json::json;

pub(crate) use bisa_engine::inbox::preview_of;

pub(crate) fn routes() -> Router<Shared> {
    Router::new().route("/inbox", get(inbox))
}

#[derive(Deserialize)]
struct InboxQuery {
    /// The primary bucket: all (default) | needs_you | unread.
    #[serde(default)]
    filter: Option<String>,
    /// The source, orthogonal to the bucket: goals | projects (workstreams
    /// and projects) | workflows (a workflow the Workflow Agent wrote, one
    /// put away, one whose listener failed) | messages (channels and direct
    /// channels) | people (persons hosted here); absent for every source.
    #[serde(default)]
    source: Option<String>,
}

/// The primary bucket of `GET /inbox`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Bucket {
    All,
    NeedsYou,
    Unread,
}

impl Bucket {
    const NAMES: &'static [&'static str] = &["all", "needs_you", "unread"];

    /// Absent is `all`; a word that is no bucket is refused by name — a
    /// misspelt filter that answered every row would read as a full Inbox.
    fn parse(raw: Option<&str>) -> Result<Self, ApiError> {
        match raw.unwrap_or("all") {
            "all" => Ok(Self::All),
            "needs_you" => Ok(Self::NeedsYou),
            "unread" => Ok(Self::Unread),
            s => Err(crate::bad_request(bisa_core::text!(
                "error-node-inbox-unknown-filter-use-one",
                s = format!("{s:?}"),
                a0 = (Self::NAMES.join(", ")).to_string()
            ))),
        }
    }

    fn holds(self, r: &InboxRow) -> bool {
        match self {
            Self::All => true,
            Self::NeedsYou => inbox::owes(r),
            Self::Unread => !r.read,
        }
    }
}

/// The sources a row can be asked by — `InboxSource::ALL`'s words, in the tabs' order.
const SOURCES: &[&str] = &["messages", "projects", "workflows", "goals", "people"];

/// Absent or empty is every source; a word that is no source is refused.
fn parse_source(raw: Option<&str>) -> Result<Option<&str>, ApiError> {
    match raw.unwrap_or("") {
        "" => Ok(None),
        s if SOURCES.contains(&s) => Ok(Some(s)),
        s => Err(crate::bad_request(bisa_core::text!(
            "error-node-inbox-unknown-source-use-one",
            s = format!("{s:?}"),
            a0 = (SOURCES.join(", ")).to_string()
        ))),
    }
}

/// Read the Inbox with the engine's own view of what is live, in the
/// node's words (English today, as every row always was).
fn read<T>(state: &Shared, with: impl FnOnce(&Reader<'_>) -> T) -> T {
    let live = LiveView::of(state.engine.inner());
    with(&Reader {
        ws: state.engine.workspace(),
        live: &live,
        say: &bisa_i18n::english,
    })
}

/// Everything waiting on a person for one goal — what the goal page shows,
/// from the builder the Inbox rows come from.
pub(crate) fn needs_actions_for(
    state: &Shared,
    goal: &Goal,
    run: Option<&WorkflowRun>,
) -> Vec<NeedsAction> {
    read(state, |r| inbox::needs_actions_for(r, goal, run))
}

/// Everything one run of the workspace owes a person — what `GET
/// /runs/{rid}` shows, from the same builder.
pub(crate) fn needs_actions_for_run(state: &Shared, run: &WorkflowRun) -> Vec<NeedsAction> {
    read(state, |r| inbox::needs_actions_for_run(r, run))
}

/// One row's state for the `inbox` frame: a projection of the row the list
/// holds for the key (`bisa_engine::inbox::delta`).
pub(crate) fn delta(state: &Shared, key: &str) -> Option<serde_json::Value> {
    read(state, |r| inbox::delta(r, key))
}

async fn inbox(
    State(state): State<Shared>,
    Query(q): Query<InboxQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Refused before anything is read.
    let bucket = Bucket::parse(q.filter.as_deref())?;
    let source = parse_source(q.source.as_deref())?;
    // A read that fails is this node's fault, said as one: never the
    // store's *not found*, which told a client there was no Inbox.
    let mut rows = read(&state, inbox::rows).map_err(|e| ApiError::internal(&e))?;
    rows.retain(|r| bucket.holds(r) && source.is_none_or(|s| r.source.as_str() == s));
    Ok(Json(json!({"rows": rows})))
}

/// The routes this module mounts, for `bisa-node --bin api-docs` and the
/// route test. Kept beside `routes()` so a route added here is added here.
pub const ROUTES: &[RouteDoc] = &[RouteDoc {
    method: "GET",
    path: "/inbox",
    summary: "What concerns you, one row per thing, of nine kinds (`kind`) — a goal, a channel, a direct channel, a conversation, a workstream, a project (a project and its primary workstream are one row, the project's), a workflow, a person, a terminal session: its asks (`needs_action` — gates and questions, a held `wait` step as a `release:` ask; each says its `subject`, which `POST …/decide` takes to decide exactly that question, and its `gate_id` while a gate is live), its notices (`notices` — what happened to it, from the activity index: a run done or failed, a step blocked, a budget spent, a design stalled, a listener that could not be armed or could not start its run (`listener_failed`, on its host's row — the workflow's, or the goal's), a committer wanted, a script that failed, a pull request opened or merged, a command refused, a folder a step made, a workflow the Workflow Agent designed or proposed, a workflow put away), `unread_notices`, unread messages, `read` (your watermark, covering notices), `handled`, `mentioned`. `?filter=needs_you|unread|all`, `?source=goals|projects|workflows|messages|people` — a word that is neither is `400` naming the ones that are, never every row (every row says its `source`; `messages` covers channels, direct channels and a conversation about the node or the workspace — one about a goal, a workflow, a project or a workstream sits under that thing's source and says what it is about in `origin`; `workflows` a workflow's row — the Workflow Agent's design or proposal, an archiving, a listener of it that failed, and its runs of the workspace: their asks, decided through the run's home, and what happened to them; a person's own save earns none; `people` the persons hosted here — a claim waiting to be admitted (`join`), a person who joined or left, a message the classifier held). A row is earned by an ask, a decision, a mention, a direct channel, an unread message, a join request or a notice, and kept — and a harness a person opened in the IDE's terminal has a `session` row (`?source=projects`, `waiting`: the harness, where it stands, what it waits on in words) while it waits on them at its own prompt, gone when the prompt is answered or the tab closes; its door is that terminal. Rows are ordered: what is owed, then what is unread, then the rest — the newest first within each, by key where two are of one second. A record this node cannot read costs its own row, never the list; a read that fails outright is a `500` in words.",
}];

#[cfg(test)]
mod tests {
    use super::*;
    use bisa_core::GoalId;
    use std::str::FromStr;

    /// The refusal of a result that had to be one — `ApiError` is not `Debug`.
    fn refusal<T>(result: Result<T, ApiError>) -> ApiError {
        match result {
            Err(e) => e,
            Ok(_) => panic!("taken, where a refusal was due"), // LCOV_EXCL_LINE: a test's failure words; by construction
        }
    }

    #[test]
    fn a_filter_is_one_of_three_words_and_anything_else_is_refused_by_name() {
        assert_eq!(Bucket::parse(None).ok(), Some(Bucket::All));
        for (word, bucket) in
            Bucket::NAMES
                .iter()
                .zip([Bucket::All, Bucket::NeedsYou, Bucket::Unread])
        {
            assert_eq!(Bucket::parse(Some(word)).ok(), Some(bucket));
        }
        // The name the bucket had before it was `needs_you`: it answered
        // every row, and a test passed on it.
        for wrong in ["needs_action", "", "ALL", "unread "] {
            let refused = refusal(Bucket::parse(Some(wrong)));
            assert_eq!(refused.status, axum::http::StatusCode::BAD_REQUEST);
            assert!(
                refused.text.to_string().contains("needs_you"),
                "the refusal names what may be asked: {}",
                refused.text
            );
        }
    }

    #[test]
    fn a_source_is_every_source_when_absent_and_refused_when_unknown() {
        assert_eq!(parse_source(None).ok(), Some(None));
        assert_eq!(parse_source(Some("")).ok(), Some(None));
        assert_eq!(parse_source(Some("goals")).ok(), Some(Some("goals")));
        let refused = refusal(parse_source(Some("goal")));
        assert_eq!(refused.status, axum::http::StatusCode::BAD_REQUEST);
        assert!(
            refused.text.to_string().contains("workflows"),
            "{}",
            refused.text
        );
        assert_eq!(SOURCES.len(), 5);
        assert_eq!(
            SOURCES.to_vec(),
            InboxSource::ALL
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>(),
            "one order, the tabs'"
        );
        // Every kind a row can have is asked for by a source the route takes.
        for kind in InboxKind::ALL {
            assert!(
                SOURCES.contains(&InboxSource::of(*kind, None).as_str()),
                "{kind:?}"
            );
        }
        // A conversation sits under what it is about; one about nothing in
        // particular is a message like any other.
        use bisa_core::ConversationOrigin as O;
        let goal = GoalId::from_str("01ARZ3NDEKTSV4RRFFQ69G5FAV").unwrap();
        let at = |origin: O| InboxSource::of(InboxKind::Conversation, Some(&origin));
        assert_eq!(at(O::Goal { id: goal }), InboxSource::Goals);
        assert_eq!(at(O::Workspace), InboxSource::Messages);
        assert_eq!(at(O::Node), InboxSource::Messages);
        assert_eq!(
            InboxSource::of(InboxKind::Conversation, None),
            InboxSource::Messages,
            "no origin known: a message"
        );
        for source in SOURCES {
            assert!(
                matches!(parse_source(Some(source)), Ok(Some(_))),
                "{source} is taken"
            );
        }
    }
}
