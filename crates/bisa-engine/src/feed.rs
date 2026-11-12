//! The activity feed's row, typed back from the store.
//!
//! The Pulse pages these rows and the Inbox carries them as a row's notices
//! (`inbox::NoticeDto`): one typing of a stored fact — a journal payload, a
//! message, an engine fact as the JSON it was recorded as — so a notice and
//! a Pulse row are the same record and the desktop words both with one
//! function. A record this build cannot type is skipped and said once at
//! `warn`, never the reason a page or an Inbox fails. Beside it, `Titles`:
//! what each row is about, looked up once per page.

use bisa_core::event::JournalPayload;
use bisa_core::{ActivityConcept, ActivitySourceKind};
use bisa_store::{ActivityRecord, Workspace};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;

/// One thing that happened, carried as facts rather than as a finished
/// sentence.
///
/// The prose used to be composed here and shipped as `text`, which threw the
/// event's type away before the client ever saw it. A stored row could then
/// only guess its glyph from where it pointed, so a criterion failure and a
/// note drew the same symbol; a verification could say it failed but not what
/// the evidence was; and `TurnMetrics` — which both the CLI and the live activity timeline
/// render — produced no row at all. Live rows meanwhile *were* rendered from
/// their payload, so one event looked one way as it arrived and another way
/// after a reload. Shipping the payload leaves exactly one renderer, on the
/// client, and removes that divergence rather than papering over it.
#[derive(Serialize, JsonSchema)]
pub struct PulseRow {
    /// The row's place in the feed — the keyset's tiebreak within a second.
    pub seq: i64,
    pub at: u64,
    /// The concept the row belongs to — the filter it answers.
    pub concept: ActivityConcept,
    /// The payload's tag, so a client can key a glyph or a filter without opening `event`.
    pub kind: String,
    /// What the row is about: the record a click opens.
    pub source: PulseSource,
    /// What that record is called — a goal's title, a channel's name, a
    /// workstream's label. Its own field rather than interpolated into the
    /// line, because a title buried in a sentence cannot be styled, cannot be
    /// linked, and cannot be used to group consecutive rows under one heading.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// The signing principal, raw. Naming it is the client's job: the desktop
    /// already resolves a pubkey to a member or agent name, and a name
    /// resolved here would be a second, staler copy of that map.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    pub event: PulseEvent,
    /// Set on an engine's fact that only repeats a journal fact of the same
    /// happening — a run that finished is said by its journal and by the
    /// engine — to that journal row's `seq`. Both rows are in the feed; a
    /// reader that draws the feed draws the journal's and passes over the
    /// echo. Absent on every other row.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub echo_of: Option<i64>,
}

/// The record a row is about.
#[derive(Serialize, JsonSchema)]
pub struct PulseSource {
    pub kind: ActivitySourceKind,
    /// The record's id; empty for this node or the workspace itself.
    pub id: String,
}

/// What happened.
///
/// Untagged because every arm is already internally tagged on `type`, so the
/// union stays one flat discriminated shape on the wire. The journal arm
/// carries the payload **verbatim**: nothing here restates a variant's
/// fields, which is what makes it structurally impossible for a payload added
/// to the journal to reach the Pulse as anything less than itself. The engine
/// arm is the `EnginePayload` the frame carried, as recorded — the desktop's
/// hand-written mirror of that enum (`types.hand.ts`) is its type.
#[derive(Serialize, JsonSchema)]
#[serde(untagged)]
pub enum PulseEvent {
    Journal(JournalPayload),
    Message(PulseMessage),
    Engine(EngineActivity),
}

/// An engine fact as it was recorded — the frame's payload verbatim, tagged
/// on `type` like every other arm. Its schema says only that much: the
/// engine's enum has no schema of its own, and the desktop's hand-written
/// mirror (`types.hand.ts`, `EnginePayload`) is its type there.
#[derive(Serialize)]
#[serde(transparent)]
pub struct EngineActivity(pub serde_json::Value);

impl JsonSchema for EngineActivity {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("EngineActivity")
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "object",
            "description": "An engine fact as recorded — the frame's payload verbatim; the desktop's hand-written `EnginePayload` is its type.",
            "properties": { "type": { "type": "string" } },
            "required": ["type"],
            "additionalProperties": true
        })
    }
}

/// A message in a conversation — a post or a membership change. A snippet,
/// not the body: the row is a pointer into the conversation, not a copy of
/// it. It carries `type: "message"` as an explicit field, so the untagged
/// union stays one flat shape discriminated on `type` in the schema as well
/// as on the wire.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct PulseMessage {
    #[serde(rename = "type")]
    pub kind: PulseMessageTag,
    pub message_id: String,
    /// `post` or `membership`.
    pub body_kind: String,
    pub snippet: String,
}

/// The one value `PulseMessage.type` takes.
#[derive(Serialize, Deserialize, JsonSchema, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum PulseMessageTag {
    Message,
}

/// The titles a page's rows are about, looked up once per page from the
/// workspace's own lists: a goal's title, a channel's name, a project's
/// name, a workstream's label, a workflow's name, an agent's name. A source
/// nothing names any more is left untitled.
pub struct Titles<'a> {
    ws: &'a Workspace,
    cache: HashMap<(ActivitySourceKind, String), Option<String>>,
}

impl<'a> Titles<'a> {
    pub fn new(ws: &'a Workspace) -> Self {
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
pub fn typed(record: ActivityRecord, titles: &mut Titles<'_>) -> Option<PulseRow> {
    match typed_row(record, titles) {
        Ok(row) => Some(row),
        Err((seq, why)) => {
            tracing::warn!(target: "bisa_engine", seq, "skipping an activity row the feed cannot type: {why}");
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
        echo_of: None,
    })
}

/// One page of the feed as its readers take it — the Pulse's route and the
/// command line with no node: every record typed ([`typed`]), and every
/// engine fact that only repeats a journal fact marked as its echo
/// ([`Echoes`]). The one place a page is built, so the two readers cannot
/// draw one happening differently.
pub fn page(ws: &Workspace, records: Vec<ActivityRecord>) -> Vec<PulseRow> {
    let mut titles = Titles::new(ws);
    let mut echoes = Echoes::new(ws);
    records
        .into_iter()
        .filter_map(|record| {
            let echo_of = echoes.of(&record);
            let mut row = typed(record, &mut titles)?;
            row.echo_of = echo_of;
            Some(row)
        })
        .collect()
}

// ---------------------------------------------------------------------
// Echoes: one happening, said twice
// ---------------------------------------------------------------------

/// How far apart, in seconds, a journal fact and the engine's fact of the
/// same happening can be stamped: each takes the clock on its own.
const ECHO_SPAN_SECS: u64 = 2;

/// How many rows around a second are read to look for a twin — far more
/// than one happening writes; a burst past it leaves its echoes unmarked.
const ECHO_NEIGHBOURS: usize = 2000;

/// Which side of a happening a row is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Side {
    Journal,
    Engine,
}

/// What a row says happened, as a key its twin shares: a journal fact and
/// the engine's fact of the same happening answer the same key, and nothing
/// else does. `None` for a row that has no twin by rule.
///
/// The pairs — journal fact · engine fact:
///
/// - `run` queued, started, finished, cancelled · `run_queued`,
///   `run_started`, `run_finished`, `run_cancelled`, by the run. An amended
///   run is the journal's alone.
/// - `step` · `step_changed`, by the run, the step and the state it reached
///   — *started* is the engine's `pending` or `running`; *answered* and an
///   approval are its `done`, a refusal its `failed`.
/// - `step` boundary · `boundary_fired`, by the run, the step and the
///   boundary.
/// - `question` · `gate_opened` and `question_asked`, by the gate.
/// - `decision` · `gate_decided`, by the gate's kind and the verdict.
/// - `guidance` · `guided`, `guard` · `guard_decided`, by every field.
/// - `judgement` · `judged`, by the judgement and where it was made.
/// - `document` · `document_added`, by the file's digest.
///
/// Not twins, though they sound alike: an attachment (the journal files it
/// under the goal, the engine under the project), a signal and what a
/// listener did with it, a result and its acceptance, a claim and a
/// schedule.
fn echo_key(kind: &str, event: &serde_json::Value) -> Option<(Side, String)> {
    use serde_json::json;
    let text = |key: &str| event.get(key).and_then(|v| v.as_str());
    let field = |key: &str| event.get(key).cloned().unwrap_or(serde_json::Value::Null);
    let fact = || {
        event
            .get("event")
            .and_then(|e| e.get("fact"))
            .and_then(|f| f.as_str())
    };
    let run_key = |fact: &str| Some(format!("run/{}/{fact}", text("run")?));
    let step_key = |state: &str| Some(format!("step/{}/{}/{state}", text("run")?, text("step")?));
    let boundary_key = |boundary: &serde_json::Value| {
        Some(format!(
            "boundary/{}/{}/{boundary}",
            text("run")?,
            text("step")?
        ))
    };
    let journal = |key: Option<String>| key.map(|key| (Side::Journal, key));
    let engine = |key: Option<String>| key.map(|key| (Side::Engine, key));
    match kind {
        "run" => match fact()? {
            fact @ ("queued" | "started" | "finished" | "cancelled") => journal(run_key(fact)),
            _ => None,
        },
        "run_queued" => engine(run_key("queued")),
        "run_started" => engine(run_key("started")),
        "run_finished" => engine(run_key("finished")),
        "run_cancelled" => engine(run_key("cancelled")),
        "step" => {
            let said = event.get("event")?;
            match fact()? {
                "boundary" => journal(boundary_key(said.get("boundary")?)),
                "started" => journal(step_key("started")),
                "done" | "answered" => journal(step_key("done")),
                "decided" => journal(step_key(
                    if said.get("approve").and_then(|a| a.as_bool())? {
                        "done"
                    } else {
                        "failed"
                    },
                )),
                state => journal(step_key(state)),
            }
        }
        "step_changed" => match text("state")? {
            "pending" | "running" => engine(step_key("started")),
            state => engine(step_key(state)),
        },
        "boundary_fired" => engine(boundary_key(event.get("boundary")?)),
        "question" => journal(Some(format!("gate/{}", text("gate")?))),
        "gate_opened" | "question_asked" => engine(Some(format!("gate/{}", text("gate_id")?))),
        "decision" => journal(Some(format!(
            "decision/{}/{}",
            text("gate")?,
            field("approve")
        ))),
        "gate_decided" => engine(Some(format!(
            "decision/{}/{}",
            text("gate")?,
            field("approve")
        ))),
        "guidance" | "guided" => {
            let key = format!(
                "guided/{}",
                json!([
                    field("phase"),
                    field("status"),
                    field("detail"),
                    field("session")
                ])
            );
            Some((
                if kind == "guidance" {
                    Side::Journal
                } else {
                    Side::Engine
                },
                key,
            ))
        }
        "guard" | "guard_decided" => {
            let key = format!(
                "guard/{}",
                json!([
                    field("tool"),
                    field("subject"),
                    field("verdict"),
                    field("by"),
                    field("rule"),
                    field("reason")
                ])
            );
            Some((
                if kind == "guard" {
                    Side::Journal
                } else {
                    Side::Engine
                },
                key,
            ))
        }
        "judgement" | "judged" => {
            let key = format!(
                "judged/{}",
                json!([field("judgement"), field("run"), field("step")])
            );
            Some((
                if kind == "judgement" {
                    Side::Journal
                } else {
                    Side::Engine
                },
                key,
            ))
        }
        "document" => journal(Some(format!(
            "document/{}",
            event.get("file")?.get("sha256")?.as_str()?
        ))),
        "document_added" => engine(Some(format!(
            "document/{}",
            event.get("file")?.get("sha256")?.as_str()?
        ))),
        _ => None,
    }
}

/// One row as the pairing reads it.
#[derive(Clone, Debug, PartialEq)]
struct Said {
    seq: i64,
    at: u64,
    kind: String,
    side: Side,
}

/// The journal row an engine row repeats, among the rows that say the same
/// thing about the same source (`said`: both sides, in any order).
///
/// One to one, by the order they were written in: the engine's rows of one
/// kind and the journal's are each read oldest first, and each engine row
/// takes the oldest journal row still free that stands within
/// [`ECHO_SPAN_SECS`] of it. A step that passed three times in two seconds
/// is three pairs — three rows drawn, never one; a fact that has no twin in
/// reach — a peer's, whose engine ran elsewhere — is drawn as itself.
fn paired(seq: i64, said: &[Said]) -> Option<i64> {
    let mine = said.iter().find(|row| row.seq == seq)?;
    if mine.side != Side::Engine {
        return None;
    }
    let ordered = |side: Side, kind: Option<&str>| {
        let mut rows: Vec<&Said> = said
            .iter()
            .filter(|row| row.side == side && kind.is_none_or(|kind| row.kind == kind))
            .collect();
        rows.sort_by_key(|row| (row.at, row.seq));
        rows
    };
    let journal = ordered(Side::Journal, None);
    let mut taken = vec![false; journal.len()];
    for row in ordered(Side::Engine, Some(&mine.kind)) {
        let free = journal
            .iter()
            .enumerate()
            .position(|(i, j)| !taken[i] && j.at.abs_diff(row.at) <= ECHO_SPAN_SECS);
        if let Some(i) = free {
            taken[i] = true;
        }
        if row.seq == seq {
            return free.map(|i| journal[i].seq);
        }
    }
    None // LCOV_EXCL_LINE: `mine` is among the engine rows of its own kind, so the loop above answers
}

/// The rows that stand beside a second — a span either side, and a span
/// further, where the twin of a row's rival may stand. A read that fails
/// leaves the row unpaired and the page answered.
fn neighbours(ws: &Workspace, at: u64) -> Vec<ActivityRecord> {
    let reach = 2 * ECHO_SPAN_SECS;
    match ws.activity_between(
        at.saturating_sub(reach),
        at.saturating_add(reach),
        ECHO_NEIGHBOURS,
    ) {
        Ok(near) => near,
        // LCOV_EXCL_START: the index's disk arm; the row is drawn unpaired and the page still answers
        Err(e) => {
            tracing::warn!(target: "bisa_engine", at, "the rows beside an activity row could not be read: {e}");
            Vec::new()
        } // LCOV_EXCL_STOP
    }
}

/// The echoes of one page, found against the store: the rows beside each
/// engine fact are read once per second it stands in, whatever the page —
/// a twin on the page before or after is found the same, so a pair split by
/// a page's edge is still one happening.
pub struct Echoes<'a> {
    ws: &'a Workspace,
    /// The rows around a second, read once.
    around: HashMap<u64, Vec<ActivityRecord>>,
}

impl<'a> Echoes<'a> {
    pub fn new(ws: &'a Workspace) -> Self {
        Self {
            ws,
            around: HashMap::new(),
        }
    }

    /// The `seq` of the journal row `record` only repeats, or nothing.
    pub fn of(&mut self, record: &ActivityRecord) -> Option<i64> {
        let event: serde_json::Value = serde_json::from_str(&record.event).ok()?;
        let (side, key) = echo_key(&record.kind, &event)?;
        if side != Side::Engine {
            return None;
        }
        let ws = self.ws;
        let near = self
            .around
            .entry(record.at)
            .or_insert_with(|| neighbours(ws, record.at));
        let said: Vec<Said> = near
            .iter()
            .filter(|other| {
                other.source_kind == record.source_kind && other.source_id == record.source_id
            })
            .filter_map(|other| {
                let event: serde_json::Value = serde_json::from_str(&other.event).ok()?;
                let (side, theirs) = echo_key(&other.kind, &event)?;
                (theirs == key).then(|| Said {
                    seq: other.seq,
                    at: other.at,
                    kind: other.kind.clone(),
                    side,
                })
            })
            .collect();
        paired(record.seq, &said)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn key(event: serde_json::Value) -> Option<(Side, String)> {
        let kind = event["type"].as_str().unwrap().to_string();
        echo_key(&kind, &event)
    }

    /// The same key on both sides, journal first.
    fn twins(journal: serde_json::Value, engine: serde_json::Value) {
        let (j, e) = (key(journal.clone()), key(engine.clone()));
        let (Some((Side::Journal, j)), Some((Side::Engine, e))) = (j, e) else {
            panic!("a journal fact and an engine fact: {journal} · {engine}");
        };
        assert_eq!(j, e, "{journal} · {engine}");
    }

    /// Every pair the rule names answers one key from both sides.
    #[test]
    fn a_journal_fact_and_the_engines_fact_of_one_happening_share_a_key() {
        for fact in ["queued", "started", "finished", "cancelled"] {
            twins(
                json!({"type": "run", "run": "R", "event": {"fact": fact}}),
                json!({"type": format!("run_{fact}"), "run": "R", "workflow": "W"}),
            );
        }
        let step = |fact: serde_json::Value| json!({"type": "step", "run": "R", "step": "s", "event": fact});
        let changed = |state: &str| json!({"type": "step_changed", "run": "R", "step": "s", "state": state, "kind": "agent"});
        for (fact, state) in [
            (json!({"fact": "started"}), "pending"),
            (json!({"fact": "started", "work_item": "I"}), "running"),
            (json!({"fact": "waiting"}), "waiting"),
            (json!({"fact": "done"}), "done"),
            (
                json!({"fact": "answered", "answer": {"text": "yes"}}),
                "done",
            ),
            (
                json!({"fact": "decided", "approve": true, "approval": "a"}),
                "done",
            ),
            (
                json!({"fact": "decided", "approve": false, "approval": "a"}),
                "failed",
            ),
            (json!({"fact": "failed", "error": "no"}), "failed"),
            (json!({"fact": "diverted", "by": "timeout"}), "diverted"),
            (json!({"fact": "skipped"}), "skipped"),
            (json!({"fact": "cancelled"}), "cancelled"),
        ] {
            twins(step(fact), changed(state));
        }
        twins(
            step(json!({"fact": "boundary", "boundary": "timeout"})),
            json!({"type": "boundary_fired", "run": "R", "step": "s", "boundary": "timeout", "diverts": false}),
        );
        for engine in ["gate_opened", "question_asked"] {
            twins(
                json!({"type": "question", "gate": "G", "text": "?", "expects": {"kind": "answer"}}),
                json!({"type": engine, "gate_id": "G"}),
            );
        }
        twins(
            json!({"type": "decision", "gate": "approval", "approve": true, "subject": "s"}),
            json!({"type": "gate_decided", "gate_id": "G", "gate": "approval", "approve": true}),
        );
        twins(
            json!({"type": "guidance", "phase": "design", "status": "started", "session": "S"}),
            json!({"type": "guided", "phase": "design", "status": "started", "session": "S"}),
        );
        twins(
            json!({"type": "guard", "tool": "Bash", "subject": "x", "verdict": "denied", "by": "rule", "rule": "r"}),
            json!({"type": "guard_decided", "session": "L", "tool": "Bash", "subject": "x", "verdict": "denied", "by": "rule", "rule": "r"}),
        );
        twins(
            json!({"type": "judgement", "judgement": {"point": "goal_adopt"}, "run": "R"}),
            json!({"type": "judged", "judgement": {"point": "goal_adopt"}, "agent": "a", "run": "R"}),
        );
        twins(
            json!({"type": "document", "file": {"sha256": "ab", "name": "n"}}),
            json!({"type": "document_added", "goal": "G", "file": {"sha256": "ab", "name": "n"}}),
        );
    }

    /// What sounds alike and is not the same happening has no key, or
    /// another: each is drawn.
    #[test]
    fn what_only_sounds_alike_shares_no_key() {
        for alone in [
            json!({"type": "run", "run": "R", "event": {"fact": "amended", "revision": 2}}),
            json!({"type": "attachment", "project": "P", "attached": true}),
            json!({"type": "attachment_changed", "project": "P", "attached": true}),
            json!({"type": "signal", "signal": "S"}),
            json!({"type": "signal_received", "signal": "S"}),
            json!({"type": "listener_fired", "signal": "S"}),
            json!({"type": "result", "work_item": "I"}),
            json!({"type": "result_accepted"}),
            json!({"type": "claim", "work_item": "I"}),
            json!({"type": "scheduled", "harness": "mock"}),
            json!({"type": "note", "text": "goal captured"}),
            json!({"type": "goal_created", "goal": "G"}),
            json!({"type": "message", "message_id": "m"}),
            // A fact cut short names nothing to pair by.
            json!({"type": "run_started"}),
            json!({"type": "step", "run": "R", "step": "s", "event": {"fact": "decided"}}),
        ] {
            assert_eq!(key(alone.clone()), None, "{alone}");
        }
        let different = |a: serde_json::Value, b: serde_json::Value| {
            assert_ne!(
                key(a.clone()).unwrap().1,
                key(b.clone()).unwrap().1,
                "{a} · {b}"
            )
        };
        different(
            json!({"type": "run", "run": "R", "event": {"fact": "started"}}),
            json!({"type": "run_finished", "run": "R"}),
        );
        different(
            json!({"type": "run", "run": "R", "event": {"fact": "started"}}),
            json!({"type": "run_started", "run": "another"}),
        );
        different(
            json!({"type": "step", "run": "R", "step": "s", "event": {"fact": "failed", "error": "e"}}),
            json!({"type": "step_changed", "run": "R", "step": "s", "state": "blocked"}),
        );
        different(
            json!({"type": "decision", "gate": "approval", "approve": true, "subject": "s"}),
            json!({"type": "gate_decided", "gate_id": "G", "gate": "approval", "approve": false}),
        );
    }

    fn said(seq: i64, at: u64, kind: &str, side: Side) -> Said {
        Said {
            seq,
            at,
            kind: kind.to_string(),
            side,
        }
    }

    /// One to one, in the order written, within the span — whichever side
    /// was written first.
    #[test]
    fn an_engine_row_takes_the_oldest_free_journal_row_within_reach() {
        use Side::{Engine as E, Journal as J};
        // Either order of writing.
        assert_eq!(
            paired(2, &[said(1, 10, "run", J), said(2, 10, "run_started", E)]),
            Some(1)
        );
        assert_eq!(
            paired(1, &[said(1, 10, "run_started", E), said(2, 11, "run", J)]),
            Some(2)
        );
        // A journal row is nobody's echo, and an engine row alone is itself.
        assert_eq!(
            paired(1, &[said(1, 10, "run", J), said(2, 10, "run_started", E)]),
            None
        );
        assert_eq!(paired(1, &[said(1, 10, "run_started", E)]), None);
        assert_eq!(
            paired(9, &[said(1, 10, "run", J)]),
            None,
            "a row not among them"
        );
        // Out of reach: three seconds apart are two happenings.
        assert_eq!(
            paired(2, &[said(1, 10, "run", J), said(2, 13, "run_started", E)]),
            None
        );
        // A step that passed three times in two seconds: three pairs.
        let looped = [
            said(1, 10, "step", J),
            said(2, 10, "step_changed", E),
            said(3, 11, "step", J),
            said(4, 11, "step_changed", E),
            said(5, 12, "step", J),
            said(6, 12, "step_changed", E),
        ];
        assert_eq!(
            [paired(2, &looped), paired(4, &looped), paired(6, &looped)],
            [Some(1), Some(3), Some(5)]
        );
        // Two engine rows of one kind and one journal row: the older takes it.
        let spare = [
            said(1, 10, "step", J),
            said(2, 10, "step_changed", E),
            said(3, 10, "step_changed", E),
        ];
        assert_eq!([paired(2, &spare), paired(3, &spare)], [Some(1), None]);
        // A question is said by two engine facts: each echoes the journal's.
        let asked = [
            said(1, 10, "question", J),
            said(2, 10, "gate_opened", E),
            said(3, 10, "question_asked", E),
        ];
        assert_eq!([paired(2, &asked), paired(3, &asked)], [Some(1), Some(1)]);
    }

    fn ws() -> (tempfile::TempDir, Workspace) {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::open_with_keystore(
            dir.path(),
            Box::new(bisa_store::MemoryKeyStore::default()),
        )
        .unwrap();
        (dir, ws)
    }

    fn fact(
        at: u64,
        source: bisa_core::ActivitySource,
        event: serde_json::Value,
    ) -> bisa_core::ActivityFact {
        bisa_core::ActivityFact {
            at,
            concept: ActivityConcept::Goals,
            kind: bisa_core::tag_of(&event),
            source,
            author: None,
            event,
        }
    }

    /// A page marks the engine's echo wherever its journal twin stands — on
    /// the page or off it — and only for the same source.
    #[test]
    fn a_page_marks_an_echo_whose_twin_is_on_another_page_and_never_across_sources() {
        let (_dir, ws) = ws();
        let goal =
            |id: &str| bisa_core::ActivitySource::new(ActivitySourceKind::Goal, id.to_string());
        let started = json!({"type": "run", "run": "R", "event": {"fact": "started", "workflow": "W", "revision": 1}});
        let echo = json!({"type": "run_started", "run": "R", "workflow": "W"});
        let journal = ws
            .record_activity(&fact(10, goal("a"), started.clone()), false)
            .unwrap();
        let engine = ws
            .record_activity(&fact(11, goal("a"), echo.clone()), false)
            .unwrap();
        // The same words about another goal are another happening.
        let elsewhere = ws
            .record_activity(&fact(11, goal("b"), echo), false)
            .unwrap();

        let all = page(&ws, ws.activity_page(None, None, 10).unwrap());
        let echo_of = |rows: &[PulseRow], seq: i64| {
            rows.iter()
                .find(|row| row.seq == seq)
                .and_then(|row| row.echo_of)
        };
        assert_eq!(echo_of(&all, engine), Some(journal));
        assert_eq!(echo_of(&all, journal), None);
        assert_eq!(echo_of(&all, elsewhere), None);
        // A page of one row each: the pair is split, and still a pair.
        let newest = page(&ws, ws.activity_page(None, None, 2).unwrap());
        assert_eq!(newest.len(), 2);
        assert!(
            newest.iter().all(|row| row.seq != journal),
            "the twin is off this page"
        );
        assert_eq!(echo_of(&newest, engine), Some(journal));
        // On the wire the mark is a field of the echo alone.
        let wire = serde_json::to_value(&all).unwrap();
        let fields: Vec<bool> = wire
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row.get("echo_of").is_some())
            .collect();
        assert_eq!(fields.iter().filter(|has| **has).count(), 1, "{wire}");
    }

    /// A stored row is typed back as what it was — a journal fact, a
    /// message, an engine fact — and one this build cannot type is left out
    /// of the page, which is read all the same.
    #[test]
    fn a_row_is_typed_back_and_one_nobody_can_type_is_left_out() {
        let (_dir, ws) = ws();
        let node = bisa_core::ActivitySource::node;
        ws.record_activity(
            &fact(1, node(), json!({"type": "note", "text": "kept"})),
            false,
        )
        .unwrap();
        ws.record_activity(
            &fact(2, node(), json!({"type": "message", "message_id": "m", "body_kind": "post", "snippet": "hello"})),
            false,
        )
        .unwrap();
        ws.record_activity(&fact(3, node(), json!({"type": "paused"})), false)
            .unwrap();
        // A message whose event is not one: the row cannot be typed.
        ws.record_activity(&fact(4, node(), json!({"type": "message"})), false)
            .unwrap();
        let rows = page(&ws, ws.activity_page(None, None, 10).unwrap());
        let shapes: Vec<&str> = rows
            .iter()
            .map(|row| match &row.event {
                PulseEvent::Journal(_) => "journal",
                PulseEvent::Message(_) => "message",
                PulseEvent::Engine(_) => "engine",
            })
            .collect();
        assert_eq!(
            shapes,
            ["engine", "message", "journal"],
            "newest first, the bad row skipped"
        );
        assert!(
            rows.iter().all(|row| row.title.is_none()),
            "this node has no title"
        );

        let record = |concept: &str, source_kind: &str, event: &str| ActivityRecord {
            seq: 9,
            at: 1,
            concept: concept.to_string(),
            kind: "note".to_string(),
            source_kind: source_kind.to_string(),
            source_id: String::new(),
            author: None,
            event: event.to_string(),
        };
        let mut titles = Titles::new(&ws);
        let note = r#"{"type":"note","text":"x"}"#;
        assert!(typed(record("goals", "node", note), &mut titles).is_some());
        assert!(typed(record("triggers", "node", note), &mut titles).is_none());
        assert!(typed(record("goals", "trigger", note), &mut titles).is_none());
        assert!(typed(record("goals", "node", "not json"), &mut titles).is_none());
    }

    /// A row is titled by what it is about, once per page, and a thing that
    /// is gone — or an id that is none — leaves the row untitled.
    #[test]
    fn a_title_is_what_the_row_is_about_and_a_thing_that_is_gone_has_none() {
        let (_dir, ws) = ws();
        let goal = ws
            .create_goal(bisa_store::NewGoal::captured("Ship the feed"))
            .unwrap();
        let mut titles = Titles::new(&ws);
        assert_eq!(
            titles
                .of(ActivitySourceKind::Goal, &goal.id.to_string())
                .as_deref(),
            Some("Ship the feed")
        );
        assert_eq!(
            titles.of(ActivitySourceKind::Channel, "general").as_deref(),
            Some("general")
        );
        for (kind, id) in [
            (ActivitySourceKind::Goal, "01ARZ3NDEKTSV4RRFFQ69G5FAV"),
            (ActivitySourceKind::Goal, "no id"),
            (ActivitySourceKind::Channel, "nowhere"),
            (ActivitySourceKind::Workstream, "no id"),
            (ActivitySourceKind::Conversation, "general"),
            (ActivitySourceKind::Project, "no id"),
            (ActivitySourceKind::Workflow, "no id"),
            (ActivitySourceKind::Agent, "nobody"),
            (ActivitySourceKind::Node, ""),
            (ActivitySourceKind::Workspace, "servers"),
        ] {
            assert_eq!(titles.of(kind, id), None, "{kind:?} {id}");
        }
        // Asked again, a title is the one already looked up.
        assert_eq!(titles.cache.len(), 12);
        assert_eq!(trunc("a statement that is long", 11), "a statement…");
    }
}
