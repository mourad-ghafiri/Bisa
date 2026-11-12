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

use crate::dto::{PulseCursor, PulsePage, PulseRow};
use crate::route_docs::RouteDoc;
use crate::Query;
use crate::{bad_request, ApiError, Shared};
use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use bisa_core::ActivityConcept;
use bisa_store::{ActivityCursor, ActivityNotice};
use serde::Deserialize;
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
    // Every record typed, and every engine fact that only repeats a journal
    // fact marked as its echo: the feed's one page builder.
    let rows: Vec<PulseRow> = bisa_engine::feed::page(ws, records);
    Ok(Json(PulsePage { rows, next }))
}

/// The feed's own frame on the event stream — `{"stream": "pulse", …}` —
/// for what the store says of its feed ([`ActivityNotice`]).
///
/// A row that landed is said with where it stands and what it is filed
/// under: `{seq, at, concept, kind, source}`. A reader holding that
/// concept's page reads its head again and finds the row, since the notice
/// is sent only once the row can be read — a journal fact's, a message's,
/// an engine fact's, and a peer's as much as this node's own.
///
/// `{"concept": null}` says the feed moved in a way no one row tells: the
/// index was rebuilt (every row may wear another `seq`), or this reader fell
/// behind and missed notices (`None`). Whatever a reader holds, it reads
/// again.
pub(crate) fn frame(notice: Option<&ActivityNotice>) -> serde_json::Value {
    let payload = match notice {
        Some(ActivityNotice::Row {
            seq,
            at,
            concept,
            kind,
            source,
        }) => serde_json::json!({
            "seq": seq,
            "at": at,
            "concept": concept,
            "kind": kind,
            "source": source,
        }),
        Some(ActivityNotice::Rebuilt) | None => serde_json::json!({"concept": null}),
    };
    serde_json::json!({"stream": "pulse", "payload": payload})
}

/// The routes this module mounts, for `bisa-node --bin api-docs` and the
/// route test. Kept beside `routes()` so a route added here is added here.
pub const ROUTES: &[RouteDoc] = &[RouteDoc {
    method: "GET",
    path: "/pulse",
    summary: "One page of the activity feed, newest first (`?concept=all|workspace|goals|workflows|projects|channels|agents|node&before=&before_seq=&limit=`): rows with the fact verbatim, its concept, what it is about and a title; `next` is the cursor of the page after, absent on the last. `limit` is 60 when absent, 200 at most and never less than one; `before` alone reads from the end of that second, `before_seq` alone is refused. What a listener heard, started or could not start is filed under its host — `workflows` for a library workflow, `goals` for a goal. One happening the journal and the engine both recorded is two rows, and the engine's carries `echo_of`, the `seq` of the journal's: a reader draws the journal's row and passes over the echo. A row this build cannot type is skipped and logged, never the reason a page fails — so a page may hold fewer rows than `limit`, none at all, and still carry `next`. Live: the event stream's `pulse` frame says each row as it lands.",
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

    /// A row that landed is said with its place and what it is filed under;
    /// a rebuild, and a reader that fell behind, are one word to every tab.
    #[test]
    fn a_pulse_frame_says_the_row_that_landed_or_that_everything_moved() {
        let row = ActivityNotice::Row {
            seq: 7,
            at: 42,
            concept: ActivityConcept::Workflows,
            kind: "step_changed".into(),
            source: bisa_core::ActivitySource::new(
                bisa_core::ActivitySourceKind::Workflow,
                "w1".to_string(),
            ),
        };
        assert_eq!(
            frame(Some(&row)),
            serde_json::json!({"stream": "pulse", "payload": {
                "seq": 7, "at": 42, "concept": "workflows", "kind": "step_changed",
                "source": {"kind": "workflow", "id": "w1"},
            }})
        );
        let moved = serde_json::json!({"stream": "pulse", "payload": {"concept": null}});
        assert_eq!(frame(Some(&ActivityNotice::Rebuilt)), moved);
        assert_eq!(frame(None), moved, "a reader that fell behind");
    }
}
