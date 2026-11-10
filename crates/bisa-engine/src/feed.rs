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
    })
}
