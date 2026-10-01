//! Pulse: the workspace-wide activity feed, one page at a time.
//!
//! The feed is the store's `activity` index — every journal fact, every
//! message and every engine fact with a concept, written as they happened —
//! paged by **keyset** on `(at, seq)`: a page costs the same at any depth,
//! two facts in one second never lose each other across a page edge, and
//! there is no horizon of goals a row could fall outside. A reader narrows
//! it to one of the platform's concepts (`?concept=`), and the page says
//! where the next one starts (`next`).
//!
//! **Nothing is rendered here.** A row carries the fact's payload verbatim,
//! its concept, what it is about and a title for that; the desktop's
//! `activity.ts` turns it into a line, with the same words a live frame gets
//! — so a row reads one way as it arrives and the same way after a reload.
//!
//! **One row this build cannot type costs that row, never the page.** The
//! feed is written by every version of the code that ever ran here; a row
//! whose concept, source or event no longer reads is skipped and said at
//! `warn` with its `seq`, and the cursor is the records' — so the page after
//! it is still reachable. A page that failed whole on one row was a feed a
//! person could never open again, since the newest rows are the first page.

use crate::dto::{EngineActivity, PulseCursor, PulseEvent, PulsePage, PulseRow, PulseSource};
use crate::route_docs::RouteDoc;
use crate::Query;
use crate::{bad_request, ApiError, Shared};
use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use bisa_core::event::JournalPayload;
use bisa_core::{ActivityConcept, ActivitySourceKind};
use bisa_store::{ActivityCursor, ActivityRecord, Workspace};
use serde::Deserialize;
use std::collections::HashMap;
use std::str::FromStr;

pub(crate) fn routes() -> Router<Shared> {
    Router::new().route("/pulse", get(pulse))
}

#[derive(Deserialize)]
struct PulseQuery {
    /// One concept, or `all` (the default) for every one.
    #[serde(default)]
    concept: Option<String>,
    /// The cursor the last page answered: strictly before this `(at, seq)`.
    #[serde(default)]
    before: Option<u64>,
    #[serde(default)]
    before_seq: Option<i64>,
    #[serde(default)]
    limit: Option<usize>,
}

const DEFAULT_LIMIT: usize = 60;
const MAX_LIMIT: usize = 200;

fn parse_concept(raw: Option<&str>) -> Result<Option<ActivityConcept>, ApiError> {
    match raw.unwrap_or("all") {
        "all" => Ok(None),
        s => ActivityConcept::from_str(s).map(Some).map_err(|_| {
            bad_request(bisa_core::text!(
                "error-node-pulse-unknown-concept-use-all-one",
                s = format!("{s:?}"),
                a0 = (ActivityConcept::ALL
                    .iter()
                    .map(|c| c.as_str())
                    .collect::<Vec<_>>()
                    .join(", "))
                .to_string()
            ))
        }),
    }
}

/// The titles a page's rows are about, looked up once per page from the
/// workspace's own lists: a goal's title, a channel's name, a project's
/// name, a workstream's label, a workflow's name, an agent's name. A source
/// nothing names any more is left untitled.
pub(crate) struct Titles<'a> {
    ws: &'a Workspace,
    cache: HashMap<(ActivitySourceKind, String), Option<String>>,
}

impl<'a> Titles<'a> {
    pub(crate) fn new(ws: &'a Workspace) -> Self {
        Self {
            ws,
            cache: HashMap::new(),
        }
    }

    fn of(&mut self, kind: ActivitySourceKind, id: &str) -> Option<String> {
        if let Some(hit) = self.cache.get(&(kind, id.to_string())) {
            return hit.clone();
        }
        let title = self.look_up(kind, id);
        self.cache.insert((kind, id.to_string()), title.clone());
        title
    }

    fn look_up(&self, kind: ActivitySourceKind, id: &str) -> Option<String> {
        let ws = self.ws;
        match kind {
            ActivitySourceKind::Goal => {
                let goal = ws.get_goal(id.parse().ok()?).ok()?;
                Some(goal.title.unwrap_or_else(|| trunc(&goal.statement, 40)))
            }
            ActivitySourceKind::Channel => ws
                .get_channel(&bisa_core::ChannelId::new(id).ok()?)
                .ok()
                .map(|c| c.name),
            ActivitySourceKind::Workstream => ws
                .get_workstream(id.parse().ok()?)
                .ok()
                .and_then(|w| w.name),
            ActivitySourceKind::Conversation => ws
                .conversation_row(id.parse().ok()?)
                .ok()
                .and_then(|c| c.title.or(c.first_line)),
            ActivitySourceKind::Project => ws.get_project(id.parse().ok()?).ok().map(|p| p.name),
            ActivitySourceKind::Workflow => ws.get_workflow(id.parse().ok()?).ok().map(|w| w.name),
            ActivitySourceKind::Agent => ws
                .get_agent(&bisa_core::AgentId::new(id).ok()?)
                .ok()
                .map(|a| a.name),
            ActivitySourceKind::Node | ActivitySourceKind::Workspace => None,
        }
    }
}

fn trunc(s: &str, n: usize) -> String {
    let mut out: String = s.chars().take(n).collect();
    if s.chars().count() > n {
        out.push('…');
    }
    out
}

/// The wire's event for a stored row: a journal fact typed back, a message
/// as itself, an engine fact as the JSON it was recorded as — or why the
/// row cannot be typed.
fn event_of(record: &ActivityRecord) -> Result<PulseEvent, String> {
    let value: serde_json::Value =
        serde_json::from_str(&record.event).map_err(|e| format!("the event is not JSON: {e}"))?; // for the log
    if record.kind == "message" {
        return serde_json::from_value(value)
            .map(PulseEvent::Message)
            .map_err(|e| format!("the event is not a message: {e}")); // for the log
    }
    if let Ok(payload) = serde_json::from_value::<JournalPayload>(value.clone()) {
        return Ok(PulseEvent::Journal(payload));
    }
    Ok(PulseEvent::Engine(EngineActivity(value)))
}

/// One row as the wire carries it, or nothing: a row this build cannot type
/// is said once at `warn` with its `seq` and left out of the page.
pub(crate) fn row_of(record: ActivityRecord, titles: &mut Titles<'_>) -> Option<PulseRow> {
    match typed_row(record, titles) {
        Ok(row) => Some(row),
        Err((seq, why)) => {
            tracing::warn!(target: "bisa_node", seq, "skipping an activity row the feed cannot type: {why}");
            None
        }
    }
}

fn typed_row(record: ActivityRecord, titles: &mut Titles<'_>) -> Result<PulseRow, (i64, String)> {
    let seq = record.seq;
    let concept = ActivityConcept::from_str(&record.concept)
        .map_err(|e| (seq, format!("concept {:?}: {e}", record.concept)))?;
    let source_kind = ActivitySourceKind::from_str(&record.source_kind)
        .map_err(|e| (seq, format!("source kind {:?}: {e}", record.source_kind)))?; // for the log
    let title = titles.of(source_kind, &record.source_id);
    let event = event_of(&record).map_err(|why| (seq, why))?;
    Ok(PulseRow {
        seq: record.seq,
        at: record.at,
        concept,
        kind: record.kind,
        source: PulseSource {
            kind: source_kind,
            id: record.source_id,
        },
        title,
        author: record.author,
        event,
    })
}

async fn pulse(
    State(state): State<Shared>,
    Query(q): Query<PulseQuery>,
) -> Result<Json<PulsePage>, ApiError> {
    let concept = parse_concept(q.concept.as_deref())?;
    let before = match (q.before, q.before_seq) {
        (Some(at), Some(seq)) => Some(ActivityCursor { at, seq }),
        (Some(at), None) => Some(ActivityCursor { at, seq: i64::MAX }),
        (None, Some(_)) => {
            return Err(bad_request(bisa_core::text!(
                "error-node-pulse-before-seq-needs-before"
            )))
        }
        (None, None) => None,
    };
    let limit = q.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let ws = state.engine.workspace();
    let records = ws.activity_page(concept, before, limit)?;
    // The cursor is the records' last, not the rows': a page whose last row
    // was skipped is still paged past.
    let next = (records.len() == limit)
        .then(|| {
            records.last().map(|r| PulseCursor {
                at: r.at,
                seq: r.seq,
            })
        })
        .flatten();
    let mut titles = Titles::new(ws);
    let rows: Vec<PulseRow> = records
        .into_iter()
        .filter_map(|r| row_of(r, &mut titles))
        .collect();
    Ok(Json(PulsePage { rows, next }))
}

/// The routes this module mounts, for `bisa-node --bin api-docs` and the
/// route test. Kept beside `routes()` so a route added here is added here.
pub const ROUTES: &[RouteDoc] = &[RouteDoc {
    method: "GET",
    path: "/pulse",
    summary: "One page of the activity feed, newest first (`?concept=all|workspace|goals|workflows|projects|channels|agents|node&before=&before_seq=&limit=`): rows with the fact verbatim, its concept, what it is about and a title; `next` is the cursor of the page after, null on the last. What a listener heard, started or could not start is filed under its host — `workflows` for a library workflow, `goals` for a goal. A row this build cannot type is skipped and logged, never the reason a page fails.",
}];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_concept_is_all_or_one_of_the_seven() {
        assert!(matches!(parse_concept(None), Ok(None)));
        assert!(matches!(parse_concept(Some("all")), Ok(None)));
        assert_eq!(ActivityConcept::ALL.len(), 7);
        for concept in ActivityConcept::ALL {
            assert!(
                matches!(parse_concept(Some(concept.as_str())), Ok(Some(c)) if c == *concept),
                "{}",
                concept.as_str()
            );
        }
        assert!(parse_concept(Some("everything")).is_err());
    }
}
