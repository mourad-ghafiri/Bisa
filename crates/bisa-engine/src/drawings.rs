//! Drawings (19 — Drawings): the engine's half. The store keeps the records;
//! this module owns what happens around a write — the repository is touched
//! and the bus is told — and the **drawing bridge**: an agent's drawing tool
//! reaches the engine, which has no canvas; the desktop has one. What is
//! pure data — a list, a reading, a new empty drawing, an erasure — the
//! engine answers from the store itself. What needs the canvas's runtime —
//! turning a skeleton or a Mermaid text into laid-out elements, rendering a
//! PNG — is **parked** for the desktop on the shared desk
//! ([`crate::parked::Desk`]), exactly as a browser request is (ide/18): the
//! desktop hears `drawing_request`, performs it in the live canvas when the
//! drawing is open (the person watches it land) or in an offscreen one, saves
//! through the node, and answers. Nobody home is said at once.
//!
//! **Who may draw** is the workspace's word — `draw.enabled` (the machine's
//! switch) and `draw.agents` (*everyone* by default; *assigned* is carrying
//! the Drawing skill, the core agents always; *nobody*) — checked before
//! anything is read or parked, as the browser's is.

use crate::events::{EngineEvent, EnginePayload};
use crate::parked::{Desk, Parked};
use crate::{EngineError, Inner};
use bisa_core::draw::{self, DrawError};
use bisa_core::{
    AgentId, AttachmentRef, ConversationOrigin, Drawing, DrawingId, DrawingSummary, GoalId,
    OwnerScope, ProjectId, Scene,
};
use bisa_store::{scene_hash, DrawingPatch, NewDrawing, OwnerFilter};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::watch;

/// How long an op waits for the desktop before it says the canvas was silent.
pub const ANSWER_TIMEOUT: Duration = Duration::from_secs(60);
/// How recently a desktop must have read the list or answered a request for
/// an op to park one at all — the browser's window, for the same reason.
pub const DESKTOP_PRESENCE_TTL: Duration = Duration::from_secs(45);
/// The catalog skill whose presence on an agent is the assignment
/// (`draw.agents = assigned`).
pub const DRAW_SKILL: &str = "drawing";
/// Said at once, before a request is parked, when no desktop has been heard from.
pub const NOBODY_HOME: &str =
    "the canvas is not available — the desktop app draws, and none is open";
/// Said when a desktop was here and this request went unanswered within
/// [`ANSWER_TIMEOUT`]: a different fact from nobody home.
pub const DESKTOP_SILENT: &str = "the canvas was open but did not answer this request in \
time — try the same call once more; if it happens again, tell the person the canvas is not \
responding and stop";
/// The sentences the op refuses with, by policy.
pub const OFF: &str = "the canvas is turned off in Settings › You › Draw";
pub const NOBODY_MAY: &str = "the workspace lets no agent draw (draw.agents = nobody)";
pub const NOT_ASSIGNED: &str = "the workspace lets only assigned agents draw (draw.agents = \
assigned): this agent does not carry the Drawing skill — attach it, or set draw.agents to everyone";
/// Said when a call names no drawing and the conversation is not about one.
pub const NO_DRAWING: &str = "no drawing was named and this conversation is not about one: \
drawing_list to find one, drawing_create to make one";
/// Said when the words a call gave for a scope pair with nothing.
pub const BAD_SCOPE: &str = "not a scope: `scope` is one of workspace · goal · project · workflow \
· channel · node, with the record's `id` for the four that name one";

/// What a drawing tool asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DrawAction {
    List,
    Read,
    Create,
    Draw,
    Mermaid,
    Erase,
    Snapshot,
}

impl DrawAction {
    /// Whether the desktop performs it — the canvas's runtime lays out a
    /// skeleton, reads Mermaid and renders a picture; the rest is data.
    pub fn needs_canvas(self) -> bool {
        matches!(
            self,
            DrawAction::Draw | DrawAction::Mermaid | DrawAction::Snapshot
        )
    }
}

/// One drawing tool call, as the MCP server hands it over and the desktop
/// reads it when parked.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DrawRequest {
    pub action: DrawAction,
    /// The drawing — chosen for the call from its conversation when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drawing: Option<DrawingId>,
    /// A scope's kind word, for a list or a new drawing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    /// The scope's record id, where the kind takes one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope_id: Option<String>,
    /// A new drawing's title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Skeleton elements to draw.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elements: Option<Vec<Value>>,
    /// Mermaid text to draw.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Clear the drawing before drawing.
    #[serde(default)]
    pub replace: bool,
    /// The ids to erase.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ids: Vec<String>,
}

/// What comes back — from the store, or from the desktop.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DrawResult {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drawing: Option<DrawingId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// The scene's hash after the act — what the next edit passes back.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub element_count: Option<usize>,
    /// The reading of the scene ([`Scene::describe`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drawings: Option<Vec<DrawingSummary>>,
    /// A snapshot's PNG, uploaded to the workspace's attachments by the
    /// desktop — a reference, never bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<AttachmentRef>,
    /// The snapshot's named copy on this machine — set by the engine.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
}

impl DrawResult {
    /// A refusal, in words.
    pub fn refused(why: impl Into<String>) -> Self {
        Self {
            ok: false,
            error: Some(why.into()),
            ..Self::default()
        }
    }

    /// The record's facts after an act.
    fn of(drawing: &Drawing) -> Self {
        Self {
            ok: true,
            drawing: Some(drawing.id),
            title: Some(drawing.title.clone()),
            hash: Some(scene_hash(&drawing.scene)),
            element_count: Some(drawing.scene.element_count()),
            ..Self::default()
        }
    }

    /// The description cut to its cap on a character boundary.
    pub fn bounded(mut self) -> Self {
        if let Some(text) = self.description.take() {
            self.description = Some(if text.len() > draw::MAX_DESCRIBE_BYTES {
                let mut cut = draw::MAX_DESCRIBE_BYTES;
                while !text.is_char_boundary(cut) {
                    cut -= 1;
                }
                format!(
                    "{}\n… cut at {} bytes",
                    &text[..cut],
                    draw::MAX_DESCRIBE_BYTES
                )
            } else {
                text
            });
        }
        self
    }

    /// The named copy the desktop's upload becomes, written beside the
    /// store: what the agent reads with its own tools. A result with no
    /// snapshot is left as it is; one the store cannot name is a refusal.
    pub fn with_named_copy(mut self, inner: &Inner) -> Self {
        let Some(shot) = self.snapshot.as_ref() else {
            return self;
        };
        match inner.ws.put_attachment_named(&shot.sha256, &shot.name) {
            Ok(path) => {
                self.path = Some(path.display().to_string());
                self
            }
            Err(e) => Self::refused(format!("the snapshot could not be kept: {e}")),
        }
    }
}

/// Who asked, and from where — what the desktop reads to show the working
/// dot and root the request.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DrawScope {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversation: Option<String>,
}

/// A request the desktop has not answered yet.
#[derive(Clone, Debug, Serialize, schemars::JsonSchema)]
pub struct PendingDrawRequest {
    pub id: String,
    pub request: DrawRequest,
    pub scope: DrawScope,
    pub asked_at: u64,
}

impl Parked for PendingDrawRequest {
    type Answer = DrawResult;
    fn id(&self) -> &str {
        &self.id
    }
    fn label(&self) -> String {
        format!("{:?}", self.request.action)
    }
}

/// The desk the drawing bridge parks on.
pub fn desk() -> Desk<PendingDrawRequest> {
    Desk::new("drawing", DESKTOP_PRESENCE_TTL)
}

/// The file name a snapshot of a drawing is kept under: `drawing-<id>-<ulid>.png`.
pub fn snapshot_name(drawing: DrawingId) -> String {
    format!(
        "drawing-{drawing}-{}.png",
        ulid::Ulid::from_datetime(SystemTime::now())
    )
}

// ---------------------------------------------------------------------------
// Who may draw
// ---------------------------------------------------------------------------

/// The two settings, read once per op.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Access {
    pub enabled: bool,
    pub agents: crate::browser::AgentsPolicy,
}

pub fn access(inner: &Inner, project: Option<ProjectId>) -> Access {
    use crate::browser::AgentsPolicy;
    let enabled = inner
        .ws
        .setting("draw.enabled", project)
        .ok()
        .and_then(|r| r.value.as_bool())
        .unwrap_or(true);
    let agents = match inner
        .ws
        .setting("draw.agents", project)
        .ok()
        .and_then(|r| r.value.as_str().map(str::to_string))
        .as_deref()
    {
        Some("assigned") => AgentsPolicy::Assigned,
        Some("nobody") => AgentsPolicy::Nobody,
        _ => AgentsPolicy::Everyone,
    };
    Access { enabled, agents }
}

impl Access {
    /// Why this agent may not draw, or `None` when it may: the switch, then
    /// the policy against the asking agent. `has_skill` is whether the agent
    /// carries [`DRAW_SKILL`] — looked up by the caller, since a core agent
    /// needs no lookup at all.
    pub fn refusal(
        &self,
        agent: &AgentId,
        has_skill: impl FnOnce() -> bool,
    ) -> Option<&'static str> {
        use crate::browser::AgentsPolicy;
        if !self.enabled {
            return Some(OFF);
        }
        match self.agents {
            AgentsPolicy::Nobody => Some(NOBODY_MAY),
            AgentsPolicy::Assigned if !agent.is_core_id() && !has_skill() => Some(NOT_ASSIGNED),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// The person's writes: the repository touched, the bus told
// ---------------------------------------------------------------------------

/// Create a drawing and announce it.
pub fn create(inner: &Arc<Inner>, new: NewDrawing) -> Result<Drawing, EngineError> {
    inner.drawings_git.touched(inner);
    let drawing = inner.ws.create_drawing(new)?;
    inner.drawings_git.touched(inner);
    emit_changed(inner, &drawing);
    Ok(drawing)
}

/// Edit a drawing against the hash the canvas last saw. Silent on the bus
/// for the reason a note's edit is: the writer holds the answer, and telling
/// it would make it re-read what it just wrote.
pub fn update(
    inner: &Arc<Inner>,
    id: DrawingId,
    patch: DrawingPatch,
    base_hash: Option<&str>,
) -> Result<Drawing, EngineError> {
    let drawing = inner.ws.update_drawing(id, patch, base_hash)?;
    inner.drawings_git.touched(inner);
    Ok(drawing)
}

/// Delete a drawing and announce it, so an overlay showing it lets go.
pub fn delete(inner: &Arc<Inner>, id: DrawingId) -> Result<(), EngineError> {
    let drawing = inner.ws.get_drawing(id)?;
    inner.ws.delete_drawing(id)?;
    inner.drawings_git.touched(inner);
    emit_changed(inner, &drawing);
    Ok(())
}

/// Say on the bus that a drawing changed — with the scene's hash, so an
/// open canvas can tell its own save from somebody else's.
pub fn emit_changed(inner: &Inner, drawing: &Drawing) {
    inner.emit(EngineEvent::global(EnginePayload::DrawingChanged {
        drawing: drawing.id.to_string(),
        scope: drawing.scope.kind().to_string(),
        scope_id: drawing.scope.id(),
        hash: scene_hash(&drawing.scene),
    }));
}

// ---------------------------------------------------------------------------
// The agent's tool: the op
// ---------------------------------------------------------------------------

/// Where a call stands: the conversation it speaks in, if any, and the goal
/// a goal-scoped session serves — or the workflow a run of the workspace's
/// session runs — what chooses a drawing and a scope for a call that names
/// none.
pub struct Standing {
    pub conversation: Option<bisa_core::Conversation>,
    pub goal: Option<GoalId>,
    pub workflow: Option<bisa_core::WorkflowId>,
}

impl Standing {
    /// The drawing a call means when it names none: the conversation's.
    fn drawing(&self) -> Option<DrawingId> {
        self.conversation.as_ref().and_then(|c| c.origin.drawing())
    }

    /// Where a new drawing is filed when the call says nothing: what the
    /// conversation is about, the goal the session serves, the workflow of
    /// the run of the workspace it serves, else the workspace.
    fn default_scope(&self, inner: &Inner) -> OwnerScope {
        if let Some(c) = &self.conversation {
            match &c.origin {
                ConversationOrigin::Goal { id } => return OwnerScope::Goal { id: *id },
                ConversationOrigin::Project { id } => return OwnerScope::Project { id: *id },
                ConversationOrigin::Workflow { id } => return OwnerScope::Workflow { id: *id },
                ConversationOrigin::Workstream { project, .. } => {
                    return OwnerScope::Project { id: *project }
                }
                ConversationOrigin::Drawing { id } => {
                    if let Ok(d) = inner.ws.get_drawing(*id) {
                        return d.scope;
                    }
                }
                ConversationOrigin::Note { id } => {
                    if let Ok(n) = inner.ws.get_note(*id) {
                        return n.scope;
                    }
                }
                ConversationOrigin::Node => return OwnerScope::Node,
                ConversationOrigin::Workspace => return OwnerScope::Workspace,
            }
        }
        match (self.goal, self.workflow) {
            (Some(id), _) => OwnerScope::Goal { id },
            (None, Some(id)) => OwnerScope::Workflow { id },
            (None, None) => OwnerScope::Workspace,
        }
    }
}

/// Answer a drawing tool call whose agent may draw: from the store when it
/// is data, from the desktop when it needs the canvas. Every successful
/// change is announced.
pub async fn perform(
    inner: &Arc<Inner>,
    request: DrawRequest,
    standing: &Standing,
    scope: DrawScope,
) -> DrawResult {
    let named = || request.drawing.or_else(|| standing.drawing());
    match request.action {
        DrawAction::List => {
            let filter = match (&request.scope, &request.scope_id) {
                (None, _) => OwnerFilter::All,
                (Some(kind), None) if !OwnerScope::kind_takes_id(kind) => {
                    match OwnerScope::from_parts(kind, None) {
                        Some(s) => OwnerFilter::Scope(s),
                        None => return DrawResult::refused(BAD_SCOPE),
                    }
                }
                (Some(kind), None) => match OwnerScope::KINDS.iter().find(|k| *k == kind) {
                    Some(k) => OwnerFilter::Kind(k),
                    None => return DrawResult::refused(BAD_SCOPE),
                },
                (Some(kind), Some(id)) => match OwnerScope::from_parts(kind, Some(id)) {
                    Some(s) => OwnerFilter::Scope(s),
                    None => return DrawResult::refused(BAD_SCOPE),
                },
            };
            match inner.ws.list_drawings(filter) {
                Ok(rows) => DrawResult {
                    ok: true,
                    drawings: Some(rows),
                    ..DrawResult::default()
                },
                Err(e) => DrawResult::refused(e.to_string()),
            }
        }
        DrawAction::Read => {
            let Some(id) = named() else {
                return DrawResult::refused(NO_DRAWING);
            };
            match inner.ws.get_drawing(id) {
                Ok(d) => DrawResult {
                    description: Some(d.scene.describe()),
                    ..DrawResult::of(&d)
                }
                .bounded(),
                Err(e) => DrawResult::refused(e.to_string()),
            }
        }
        DrawAction::Create => {
            let title = request.title.clone().unwrap_or_default();
            let scope = match (&request.scope, &request.scope_id) {
                (None, _) => standing.default_scope(inner),
                (Some(kind), id) => match OwnerScope::from_parts(kind, id.as_deref()) {
                    Some(s) => s,
                    None => return DrawResult::refused(BAD_SCOPE),
                },
            };
            match create(
                inner,
                NewDrawing {
                    scope,
                    title,
                    scene: None,
                },
            ) {
                Ok(d) => DrawResult::of(&d),
                Err(e) => DrawResult::refused(e.to_string()),
            }
        }
        DrawAction::Erase => {
            let Some(id) = named() else {
                return DrawResult::refused(NO_DRAWING);
            };
            if request.ids.is_empty() {
                return DrawResult::refused("drawing_erase needs the ids to remove");
            }
            let current = match inner.ws.get_drawing(id) {
                Ok(d) => d,
                Err(e) => return DrawResult::refused(e.to_string()),
            };
            let mut scene: Scene = current.scene.clone();
            let removed = scene.erase(&request.ids);
            if removed == 0 {
                return DrawResult::refused(format!(
                    "none of {:?} is an element of this drawing — drawing_read for the ids",
                    request.ids
                ));
            }
            let base = scene_hash(&current.scene);
            match update(
                inner,
                id,
                DrawingPatch {
                    scene: Some(scene),
                    ..Default::default()
                },
                Some(&base),
            ) {
                Ok(d) => {
                    emit_changed(inner, &d);
                    DrawResult::of(&d)
                }
                Err(e) => DrawResult::refused(e.to_string()),
            }
        }
        DrawAction::Draw | DrawAction::Mermaid | DrawAction::Snapshot => {
            let Some(id) = named() else {
                return DrawResult::refused(NO_DRAWING);
            };
            if let Err(why) = check_canvas_request(&request) {
                return DrawResult::refused(why.to_string());
            }
            if let Err(e) = inner.ws.get_drawing(id) {
                return DrawResult::refused(e.to_string());
            }
            // Nobody home is said at once, never after a wait.
            if !inner.draw.desktop_present() {
                return DrawResult::refused(NOBODY_HOME);
            }
            let request = DrawRequest {
                drawing: Some(id),
                ..request
            };
            let (park_id, rx) = ask(inner, request, scope);
            let result = inner
                .draw
                .wait_for(&park_id, rx, ANSWER_TIMEOUT, || {
                    DrawResult::refused(DESKTOP_SILENT)
                })
                .await
                .with_named_copy(inner);
            if result.ok {
                if let Ok(d) = inner.ws.get_drawing(id) {
                    inner.drawings_git.touched(inner);
                    emit_changed(inner, &d);
                }
            }
            result.bounded()
        }
    }
}

/// What the core promises about what the desktop is handed: a skeleton of
/// vector types within its caps, a Mermaid text that is there.
fn check_canvas_request(request: &DrawRequest) -> Result<(), DrawError> {
    match request.action {
        DrawAction::Draw => {
            let Some(elements) = &request.elements else {
                return Err(DrawError::BadElement {
                    why: "drawing_draw needs `elements`".into(),
                });
            };
            draw::validate_skeleton(elements)
        }
        DrawAction::Mermaid => match request.text.as_deref().map(str::trim) {
            Some(t) if !t.is_empty() && t.len() <= draw::MAX_SCENE_BYTES => Ok(()),
            _ => Err(DrawError::BadElement {
                why: "drawing_mermaid needs `text`, a Mermaid definition".into(),
            }),
        },
        _ => Ok(()),
    }
}

/// Park a request and put it on the bus; the receiver resolves when the
/// desktop answers.
pub fn ask(
    inner: &Inner,
    request: DrawRequest,
    scope: DrawScope,
) -> (String, watch::Receiver<Option<DrawResult>>) {
    let id = Desk::<PendingDrawRequest>::new_id();
    let pending = PendingDrawRequest {
        id: id.clone(),
        request: request.clone(),
        scope: scope.clone(),
        asked_at: now_secs(),
    };
    tracing::debug!(
        id = %id,
        action = ?request.action,
        drawing = ?request.drawing,
        agent = scope.agent.as_deref().unwrap_or("none"),
        "drawing request parked"
    );
    let rx = inner.draw.park(pending);
    inner.emit(EngineEvent::global(EnginePayload::DrawingRequest {
        id: id.clone(),
        request,
        scope,
    }));
    (id, rx)
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_canvas_actions_are_the_three_that_need_its_runtime() {
        for a in [DrawAction::Draw, DrawAction::Mermaid, DrawAction::Snapshot] {
            assert!(a.needs_canvas());
        }
        for a in [
            DrawAction::List,
            DrawAction::Read,
            DrawAction::Create,
            DrawAction::Erase,
        ] {
            assert!(!a.needs_canvas());
        }
    }

    #[test]
    fn a_result_is_bounded_and_a_refusal_carries_its_words() {
        let r = DrawResult::refused("no");
        assert!(!r.ok);
        assert_eq!(r.error.as_deref(), Some("no"));
        let long = DrawResult {
            ok: true,
            description: Some("é".repeat(draw::MAX_DESCRIBE_BYTES)),
            ..DrawResult::default()
        }
        .bounded();
        let text = long.description.unwrap();
        assert!(text.len() < draw::MAX_DESCRIBE_BYTES + 64);
        assert!(text.contains("cut at"));
    }

    #[test]
    fn a_request_says_what_it_lacks_before_the_desktop_is_asked() {
        let mut r = DrawRequest {
            action: DrawAction::Draw,
            drawing: None,
            scope: None,
            scope_id: None,
            title: None,
            elements: None,
            text: None,
            replace: false,
            ids: vec![],
        };
        assert!(check_canvas_request(&r).is_err());
        r.elements = Some(vec![serde_json::json!({"type": "image", "x": 0, "y": 0})]);
        assert!(matches!(
            check_canvas_request(&r),
            Err(DrawError::ElementRefused { .. })
        ));
        r.action = DrawAction::Mermaid;
        assert!(check_canvas_request(&r).is_err());
        r.text = Some("flowchart LR\n a --> b".into());
        assert!(check_canvas_request(&r).is_ok());
        r.action = DrawAction::Snapshot;
        assert!(check_canvas_request(&r).is_ok());
    }

    #[test]
    fn a_snapshot_name_is_a_png_named_after_the_drawing() {
        let id = DrawingId::from_ulid(ulid::Ulid::from_parts(7, 1));
        let name = snapshot_name(id);
        assert!(name.starts_with(&format!("drawing-{id}-")));
        assert!(name.ends_with(".png"));
    }
}
