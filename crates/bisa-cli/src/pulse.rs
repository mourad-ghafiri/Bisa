//! `bisa pulse` — the activity feed in a terminal: the same rows the node
//! serves at `GET /pulse` and the desktop's Pulse draws, newest first.
//!
//! One page, read from the node when one runs and from the workspace's own
//! index when none does — through the engine's one page builder
//! (`bisa_engine::feed::page`) either way, so the two cannot show different
//! feeds: the same rows, order, titles, concepts and `--json` shape. One
//! printer words them, with the lines the live timeline says
//! (`activity.rs`); a happening the journal and the engine both recorded is
//! printed once (the engine's row is its echo, `echo_of`) and both rows stay
//! in `--json`.

use crate::ctx::Ctx;
use crate::output::Out;
use anyhow::{bail, Result};
use bisa_core::ActivityConcept;
use bisa_store::ActivityCursor;
use serde_json::{json, Value};
use std::str::FromStr;

/// The most rows one read takes — the node's own ceiling for a page.
const MAX_LIMIT: usize = 200;

/// How many pages in a row may hold no row the feed can type before the
/// read stops and says where it stands: every one of them was read past.
const EMPTY_PAGES: usize = 20;

/// What a person asked for: how much, of which concept, from where.
pub struct Ask {
    pub limit: usize,
    pub concept: String,
    pub before: Option<u64>,
    pub before_seq: Option<i64>,
}

/// Where the feed is read from: the node when one runs, else the workspace
/// itself.
enum Door {
    Node(crate::client::NodeClient),
    Workspace(Box<bisa_store::Workspace>),
}

/// One page of the feed as either door answers it.
#[derive(Debug, PartialEq)]
struct Page {
    rows: Vec<Value>,
    /// Where the page after starts; nothing on the last.
    next: Option<Cursor>,
}

/// Where a page stopped: the `(at, seq)` the next read is asked with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Cursor {
    at: u64,
    seq: i64,
}

/// The concept a word names: every concept for `all`, one of the seven
/// otherwise — and a word that is neither is refused here, before anything
/// is read, with a node or without one.
fn concept_of(word: &str) -> Result<Option<ActivityConcept>> {
    if word == "all" {
        return Ok(None);
    }
    match ActivityConcept::from_str(word) {
        Ok(concept) => Ok(Some(concept)),
        Err(_) => bail!(bisa_core::text!(
            "cli-pulse-no-such-concept",
            word = word.to_string()
        )),
    }
}

/// The address of one page at the node. Every part is a number or one of
/// the seven words, so nothing a person typed reaches the query unread.
fn address(limit: usize, concept: Option<ActivityConcept>, before: Option<Cursor>) -> String {
    let mut path = format!("/pulse?limit={limit}");
    if let Some(concept) = concept {
        path.push_str(&format!("&concept={}", concept.as_str()));
    }
    if let Some(Cursor { at, seq }) = before {
        path.push_str(&format!("&before={at}&before_seq={seq}"));
    }
    path
}

/// The node's answer as a page.
fn page_of(answer: &Value) -> Page {
    let next = answer.get("next").and_then(|next| {
        Some(Cursor {
            at: next.get("at")?.as_u64()?,
            seq: next.get("seq")?.as_i64()?,
        })
    });
    Page {
        rows: answer["rows"].as_array().cloned().unwrap_or_default(),
        next,
    }
}

/// One page from the workspace itself, as the node builds its own: the
/// index's records, a cursor when the page is full, the rows typed and
/// their echoes marked by the engine's builder.
fn page_offline(
    ws: &bisa_store::Workspace,
    limit: usize,
    concept: Option<ActivityConcept>,
    before: Option<Cursor>,
) -> Result<Page> {
    let cursor = before.map(|Cursor { at, seq }| ActivityCursor { at, seq });
    let records = ws.activity_page(concept, cursor, limit)?;
    let next = (records.len() == limit)
        .then(|| {
            records.last().map(|r| Cursor {
                at: r.at,
                seq: r.seq,
            })
        })
        .flatten();
    let rows = bisa_engine::feed::page(ws, records)
        .iter()
        .map(serde_json::to_value)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Page { rows, next })
}

/// The first page from `before` that holds a row, or the last one read.
///
/// A page may hold no row and still say there are more: every record on it
/// was one this build cannot type, and the node counted before it skipped.
/// Such a page is read past, never taken for the end of the feed — up to
/// [`EMPTY_PAGES`] of them, after which the page says where it stands and a
/// person reads on from there.
async fn read_past<F, Fut>(mut before: Option<Cursor>, mut fetch: F) -> Result<Page>
where
    F: FnMut(Option<Cursor>) -> Fut,
    Fut: std::future::Future<Output = Result<Page>>,
{
    let mut page = fetch(before).await?;
    for _ in 1..EMPTY_PAGES {
        if !page.rows.is_empty() || page.next.is_none() {
            break;
        }
        before = page.next;
        page = fetch(before).await?;
    }
    Ok(page)
}

/// One `/pulse` row → the words for it.
///
/// The route ships the event, not a sentence, so the same prose the journal
/// and the live timeline print is reached by reading the payload back — the
/// alternative is a second vocabulary for the same facts. A fact this build
/// cannot read back — one a newer build recorded — is said by its kind.
fn pulse_line(row: &Value) -> String {
    let title = row["title"].as_str().unwrap_or_default();
    let event = &row["event"];
    let body = match serde_json::from_value::<bisa_core::event::JournalPayload>(event.clone()) {
        Ok(payload) => crate::activity::payload_line(&payload),
        Err(_) if event["type"].as_str() == Some("message") => {
            format!("“{}”", event["snippet"].as_str().unwrap_or_default()) // content, never translated
        }
        Err(_) => crate::activity::fact_line(event).unwrap_or_else(|| {
            let kind = row["kind"]
                .as_str()
                .or(event["type"].as_str())
                .unwrap_or("?");
            bisa_i18n::say(&bisa_core::text!(
                "cli-pulse-a-fact-of-kind",
                kind = kind.replace('_', " ")
            ))
        }),
    };
    if title.is_empty() {
        body
    } else {
        format!("{title}: {body}") // content, never translated
    }
}

/// A row as one line of a terminal: when, under which concept, what.
fn row_line(row: &Value) -> String {
    format!(
        "{:>10}  {:<10} {}",
        row["at"].as_u64().unwrap_or(0),
        row["concept"].as_str().unwrap_or(""),
        pulse_line(row)
    )
}

/// Whether a row is drawn: an engine's fact that only repeats a journal
/// fact of the same happening is not — the journal's row says it.
fn drawn(row: &Value) -> bool {
    row.get("echo_of").is_none_or(Value::is_null)
}

/// How to read on from a cursor, as the command a person types.
fn read_on(ask: &Ask, next: Cursor) -> String {
    let concept = if ask.concept == "all" {
        String::new()
    } else {
        format!(" --concept {}", ask.concept)
    };
    // for the machine: a command to type, the same in every language
    format!(
        "bisa pulse --limit {}{concept} --before {} --before-seq {}",
        ask.limit, next.at, next.seq
    )
}

pub async fn pulse(ctx: &Ctx, out: &Out, ask: Ask) -> Result<()> {
    let concept = concept_of(&ask.concept)?;
    let limit = ask.limit.clamp(1, MAX_LIMIT);
    let before = ask.before.map(|at| Cursor {
        at,
        // A second alone reads from its end, as the route reads it.
        seq: ask.before_seq.unwrap_or(i64::MAX),
    });
    let door = match ctx.node_client().await {
        Some(client) => Door::Node(client),
        None => Door::Workspace(Box::new(ctx.workspace()?)),
    };
    let page = read_past(before, |before| {
        let door = &door;
        async move {
            match door {
                Door::Node(client) => Ok(page_of(
                    &client.get(&address(limit, concept, before)).await?,
                )),
                Door::Workspace(ws) => page_offline(ws, limit, concept, before),
            }
        }
    })
    .await?;
    let lines: Vec<String> = page
        .rows
        .iter()
        .filter(|row| drawn(row))
        .map(row_line)
        .collect();
    if lines.is_empty() && page.next.is_none() {
        out.say(&bisa_core::text!("cli-studio-nothing-has-happened-yet"));
    }
    for line in &lines {
        out.human(line);
    }
    if let Some(next) = page.next {
        out.say(&bisa_core::text!(
            "cli-pulse-read-on",
            command = read_on(&ask, next)
        ));
    }
    out.json_value(json!({
        "rows": page.rows,
        "next": page.next.map(|next| json!({"at": next.at, "seq": next.seq})),
    }));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bisa_engine::{EngineEvent, EnginePayload};

    fn ask(concept: &str) -> Ask {
        Ask {
            limit: 40,
            concept: concept.to_string(),
            before: None,
            before_seq: None,
        }
    }

    /// `all` is every concept, each of the seven is itself, and any other
    /// word is refused in a sentence that names the seven.
    #[test]
    fn a_concept_is_all_or_one_of_the_seven_and_any_other_word_is_refused() {
        assert_eq!(concept_of("all").unwrap(), None);
        for concept in ActivityConcept::ALL {
            assert_eq!(concept_of(concept.as_str()).unwrap(), Some(*concept));
        }
        for word in ["runs", "", "goals&limit=500", "All"] {
            let refused = concept_of(word).unwrap_err().to_string();
            assert!(
                refused.contains("cli-pulse-no-such-concept") || refused.contains("workflows"),
                "{refused}"
            );
        }
    }

    /// The address is built from numbers and the seven words alone.
    #[test]
    fn a_pages_address_names_its_limit_its_concept_and_its_cursor() {
        assert_eq!(address(40, None, None), "/pulse?limit=40");
        assert_eq!(
            address(
                3,
                Some(ActivityConcept::Workflows),
                Some(Cursor { at: 17, seq: 4 })
            ),
            "/pulse?limit=3&concept=workflows&before=17&before_seq=4"
        );
    }

    /// The node's answer: its rows, and a cursor only when it sent a whole one.
    #[test]
    fn a_page_is_the_nodes_rows_and_its_cursor_when_there_is_one() {
        let last = page_of(&json!({"rows": [{"seq": 1}]}));
        assert_eq!((last.rows.len(), last.next), (1, None));
        let more = page_of(&json!({"rows": [], "next": {"at": 9, "seq": 2}}));
        assert_eq!(more.next, Some(Cursor { at: 9, seq: 2 }));
        for odd in [
            json!({}),
            json!({"rows": "none", "next": {"at": 9}}),
            json!({"next": null}),
        ] {
            assert_eq!(
                page_of(&odd),
                Page {
                    rows: vec![],
                    next: None
                },
                "{odd}"
            );
        }
    }

    /// A page with no row and a cursor is read past; the feed ends where a
    /// page has no cursor, and a run of empty pages stops where it stands.
    #[tokio::test]
    async fn an_empty_page_with_a_cursor_is_read_past() {
        let at = |n: u64| Some(Cursor { at: n, seq: 1 });
        // Two pages of rows nobody can type, then one with a row.
        let asked = std::cell::RefCell::new(Vec::new());
        let page = read_past(None, |before| {
            asked.borrow_mut().push(before);
            async move {
                Ok(match before {
                    None => Page {
                        rows: vec![],
                        next: at(9),
                    },
                    Some(Cursor { at: 9, .. }) => Page {
                        rows: vec![],
                        next: at(8),
                    },
                    _ => Page {
                        rows: vec![json!({"seq": 1})],
                        next: at(7),
                    },
                })
            }
        })
        .await
        .unwrap();
        assert_eq!(page.rows.len(), 1);
        assert_eq!(*asked.borrow(), [None, at(9), at(8)]);
        // The end of the feed is an empty page with no cursor: read once.
        let end = read_past(at(3), |_| async {
            Ok(Page {
                rows: vec![],
                next: None,
            })
        })
        .await
        .unwrap();
        assert_eq!(
            end,
            Page {
                rows: vec![],
                next: None
            }
        );
        // Nothing but empty pages: the read stops and keeps its place.
        let reads = std::cell::Cell::new(0usize);
        let stuck = read_past(None, |_| {
            reads.set(reads.get() + 1);
            async {
                Ok(Page {
                    rows: vec![],
                    next: Some(Cursor { at: 5, seq: 5 }),
                })
            }
        })
        .await
        .unwrap();
        assert_eq!(
            (reads.get(), stuck.next),
            (EMPTY_PAGES, Some(Cursor { at: 5, seq: 5 }))
        );
        // A read that fails is the command's failure.
        let failed = read_past(None, |_| async {
            Err::<Page, _>(anyhow::anyhow!("no node"))
        })
        .await;
        assert!(failed.is_err());
    }

    /// `/pulse` ships the event, so the row is read back rather than printed
    /// as a sentence the node composed: a journal fact through the journal's
    /// own words, a message as its excerpt, an engine fact through the live
    /// timeline's line — never a bare tag — and a fact this build cannot
    /// read back by its kind.
    #[test]
    fn a_row_is_worded_from_its_event() {
        assert_eq!(
            pulse_line(&json!({
                "at": 1, "title": "ship it",
                "event": {"type": "note", "text": "the total is wrong"}
            })),
            "ship it: noted “the total is wrong”"
        );
        assert_eq!(
            pulse_line(&json!({
                "at": 1, "title": "watercooler",
                "event": {"type": "message", "snippet": "hello everyone"}
            })),
            "watercooler: “hello everyone”"
        );
        // An engine's fact, as the feed stored it.
        let stored = serde_json::to_value(EnginePayload::Scheduled {
            harness: "mock".into(),
        })
        .unwrap();
        let line =
            pulse_line(&json!({"at": 1, "title": "Nightly", "kind": "scheduled", "event": stored}));
        let live = crate::activity::engine_line(
            &crate::activity::Style::plain(),
            &EngineEvent::global(EnginePayload::Scheduled {
                harness: "mock".into(),
            }),
        )
        .unwrap();
        assert_eq!(line, format!("Nightly: {live}"));
        // A row with no title is still a row, not a line beginning ": ", and
        // a fact from a newer build is named, never dropped.
        let newer = pulse_line(
            &json!({"at": 1, "kind": "invented_later", "event": {"type": "invented_later"}}),
        );
        assert!(
            newer.starts_with("invented later") && !newer.contains("cli-"),
            "{newer}"
        );
        let nameless = pulse_line(&json!({"at": 1, "event": {}}));
        assert!(nameless.starts_with('?'), "{nameless}");
    }

    /// A row is one line — when, under which concept, what — and an echo is
    /// not drawn.
    #[test]
    fn a_row_is_a_line_and_an_echo_is_not_drawn() {
        let row = json!({
            "at": 42, "concept": "goals", "title": "ship it",
            "event": {"type": "note", "text": "kept"}
        });
        assert_eq!(
            row_line(&row),
            "        42  goals      ship it: noted “kept”"
        );
        assert!(drawn(&row));
        assert!(drawn(&json!({"echo_of": null})));
        assert!(!drawn(&json!({"echo_of": 7})));
    }

    /// The line that says there is more is the command that reads it.
    #[test]
    fn reading_on_is_a_command_that_keeps_what_was_asked() {
        let next = Cursor { at: 17, seq: 4 };
        assert_eq!(
            read_on(&ask("all"), next),
            "bisa pulse --limit 40 --before 17 --before-seq 4"
        );
        assert_eq!(
            read_on(&ask("workflows"), next),
            "bisa pulse --limit 40 --concept workflows --before 17 --before-seq 4"
        );
    }
}
