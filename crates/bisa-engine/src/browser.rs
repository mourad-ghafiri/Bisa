//! The browser bridge (ide/18): an agent's `browser_*` tool call parked here
//! until the desktop — the one thing that holds the embedded browser — performs
//! it and answers. The shape is a gate's: an in-memory entry with a `watch`,
//! put on the bus so an open desktop hears it, answered over HTTP, waited
//! on by the intake op. Nothing here touches a page; nothing is durable —
//! a request nobody answers times out with a sentence the agent can act on,
//! and a restart forgets every pending one.
//!
//! What the agent may ask is a closed list ([`BrowserAction`]) — a person's
//! browsing: open, read, find, snapshot the page's outline, click, type,
//! press, select, hover, scroll, wait, go back and forward, reload, read the
//! console, evaluate a script, screenshot, close. What comes back is bounded
//! text ([`MAX_BROWSER_TEXT`]), the dialogs the page asked meanwhile, and at
//! most one image **by reference** — a screenshot is uploaded as an
//! attachment by the desktop and answered as the path of its named copy,
//! never as bytes through here.
//!
//! Where a tab is at home is the engine's decision ([`home_of`]): the
//! checkout a conversation about a workstream runs in, the goal a thread is
//! about, the channel or the direct message a turn speaks in, the checkout a
//! work item was placed in — the desktop roots the tab there and shows it
//! beside the person. Who may ask is the workspace's word ([`Access`], from
//! the `browser.*` settings), checked at the op: the tool list is a menu, not
//! a permission.

use crate::conversation::ScopeFacts;
use crate::events::EnginePayload;
use crate::Inner;
use bisa_core::{AgentId, AttachmentRef, ChannelId, ChannelKind, GoalId, ProjectId, WorkItemId};
use bisa_core::{ConversationOrigin, GoalMode};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use std::time::{Duration, SystemTime};
use tokio::sync::watch;

/// The most text one answer carries — a page's text, a find's matches, a
/// snapshot, a script's value.
pub const MAX_BROWSER_TEXT: usize = 16 * 1024;
/// How long an op waits for the desktop before it says nobody is there.
pub const ANSWER_TIMEOUT: Duration = Duration::from_secs(60);
/// The most a `browser_wait` may wait — under [`ANSWER_TIMEOUT`], so the
/// desktop answers before the op gives up on it.
pub const MAX_WAIT_MS: u64 = 30_000;
/// What a `browser_wait` waits when the agent names no time.
pub const DEFAULT_WAIT_MS: u64 = 10_000;
/// How recently a desktop must have read the list or answered a request for
/// an op to park one at all: a desktop that is open reads the list this
/// often, so a stale stamp means nobody is home and the op says so at once
/// rather than after [`ANSWER_TIMEOUT`].
pub const DESKTOP_PRESENCE_TTL: Duration = Duration::from_secs(45);
/// The sentence an agent reads when no desktop has been heard from — said
/// at once, before a request is parked (`desktop_present`).
pub const NOBODY_HOME: &str =
    "the embedded browser is not available — the desktop app answers browser tools, and none is open";
/// The sentence an agent reads when a desktop was here — it is why the
/// request was parked at all — and this request went unanswered within
/// [`ANSWER_TIMEOUT`]: a different fact from nobody home, with a different
/// thing to do about it.
pub const DESKTOP_SILENT: &str =
    "the embedded browser was open but did not answer this request in \
time — try the same call once more; if it happens again, tell the person the browser is not \
responding and stop";
/// The catalog skill whose presence on an agent is the assignment
/// (`browser.agents = assigned`).
pub const BROWSER_SKILL: &str = "embedded-browser";
/// The sentences the op refuses with, by policy.
pub const OFF: &str = "the embedded browser is turned off in Settings › Capabilities › Browser";
pub const NOBODY_MAY: &str = "browser tools are set to nobody in Settings › Capabilities › Browser";
pub const NOT_ASSIGNED: &str = "this agent does not carry the Embedded Browser skill — attach it in Agents, or set browser.agents to everyone in Settings › Capabilities › Browser";
pub const LOCAL_ONLY: &str =
    "agents may open only pages served on this machine here (browser.agents.reach)";
pub const SCRIPTS_REFUSED: &str =
    "running a script in a page is refused here (browser.agents.scripts) — read the page with browser_read, browser_snapshot or browser_console instead";

/// What an agent may ask the embedded browser to do.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum BrowserAction {
    /// Open a URL — in the named tab, or a new one.
    Open,
    /// The open tabs: key, URL, title.
    #[default]
    Tabs,
    /// The page's URL, title and text — of the element `target` names, or the
    /// page; as text, or as HTML.
    Read,
    /// The matches of `query` in the page's text, with their surroundings.
    Find,
    /// The page's outline: headings, landmarks, links, buttons, fields —
    /// each with a ref the other tools take as a `target`.
    Snapshot,
    /// Click the element `target` names.
    Click,
    /// Set the value of the field `target` names in one go.
    Fill,
    /// Type `text` into the field `target` names, a keystroke at a time.
    Type,
    /// Press a key — on the element `target` names, else the focused one.
    Press,
    /// Choose an option of the `<select>` `target` names.
    Select,
    /// Move the pointer over the element `target` names.
    Hover,
    /// Scroll the page, or the element `target` names.
    Scroll,
    /// Wait for a load, an element, some text, its disappearance, or quiet.
    Wait,
    /// Go back one page.
    Back,
    /// Go forward one page.
    Forward,
    /// Load the page again.
    Reload,
    /// What the page wrote to the console, and its errors.
    Console,
    /// Evaluate `expression` in the page and answer its value.
    Eval,
    /// Close the tab.
    Close,
    /// A PNG of the tab as the person sees it, answered by path.
    Screenshot,
}

impl BrowserAction {
    /// The actions that may move the page — a load waits before they answer,
    /// and a navigation they start is waited for.
    pub fn may_navigate(self) -> bool {
        matches!(
            self,
            BrowserAction::Open
                | BrowserAction::Click
                | BrowserAction::Fill
                | BrowserAction::Type
                | BrowserAction::Press
                | BrowserAction::Select
                | BrowserAction::Back
                | BrowserAction::Forward
                | BrowserAction::Reload
        )
    }
}

/// How `browser_read` answers an element: its text, or its HTML.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ReadFormat {
    #[default]
    Text,
    Html,
}

/// What `browser_wait` waits for.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum WaitUntil {
    /// The page's load to finish.
    #[default]
    Load,
    /// The element `target` names to be in the page.
    Selector,
    /// `text` to be somewhere in the page's text.
    Text,
    /// The element `target` names to be gone from the page.
    Gone,
    /// The page to stop changing for half a second.
    Idle,
}

/// One request, as the tool sent it and the desktop reads it. `target` is a
/// CSS selector or a ref a snapshot answered (`e12`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct BrowserRequest {
    pub action: BrowserAction,
    /// The tab's key (`b1`); absent for `open` (a new tab) and `tabs`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// The element: a CSS selector, or a snapshot's ref.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// The words a find looks for, or a wait waits for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    /// What a fill or a type puts into the field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// For `read`: text, or HTML.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<ReadFormat>,
    /// For `snapshot`: every element with text, not only the interactive ones
    /// and the headings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub all: Option<bool>,
    /// For `wait`: what to wait for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub until: Option<WaitUntil>,
    /// For `wait`: how long, at most [`MAX_WAIT_MS`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    /// For `press`: the key — `Enter`, `Escape`, `Tab`, `ArrowDown`, a character.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// For `press`: `shift`, `alt`, `ctrl`, `meta`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modifiers: Vec<String>,
    /// For `type`: empty the field first.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clear: Option<bool>,
    /// For `type`: press Enter after the text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub submit: Option<bool>,
    /// For `select`: the option's value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// For `select`: the option's visible words, when `value` is not given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// For `scroll`: `top`, `bottom`, or `target` — the element to bring into view.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    /// For `scroll`: how far, in pixels, when `to` is not given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by_x: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by_y: Option<i64>,
    /// For `console`: forget what was read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clear_console: Option<bool>,
    /// For `eval`: the expression.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expression: Option<String>,
    /// For `open`, the agent's word on whether the tab is kept out of sight;
    /// absent, the workspace's policy decides (`headless_for`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub headless: Option<bool>,
}

impl BrowserRequest {
    /// A `wait`'s bound: what the agent asked within [`MAX_WAIT_MS`], else
    /// [`DEFAULT_WAIT_MS`].
    pub fn wait_ms(&self) -> u64 {
        self.timeout_ms
            .map(|t| t.clamp(1, MAX_WAIT_MS))
            .unwrap_or(DEFAULT_WAIT_MS)
    }
}

/// Where a tab is at home — the screens the desktop can show one beside,
/// word for word the desktop's `BROWSER_SCOPES`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BrowserHomeScope {
    Workstream,
    Goal,
    WorkItem,
    Workflow,
    Channel,
    Dm,
    Conversation,
}

/// The screen a tab is at home beside, and which one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct BrowserHome {
    pub scope: BrowserHomeScope,
    pub id: String,
}

impl BrowserHome {
    pub fn new(scope: BrowserHomeScope, id: impl ToString) -> Self {
        Self {
            scope,
            id: id.to_string(),
        }
    }
}

/// Where the asking session works, decided here ([`home_of`]) — so the
/// desktop roots a tab it opens beside that screen: the checkout's IDE, the
/// goal, the channel, the direct message, the conversation; with none, the
/// workspace's own Browser pane.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct BrowserScope {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home: Option<BrowserHome>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
}

/// Where a session's tab is at home. A conversation about a workstream or a
/// project is in that checkout, about a goal beside the goal, about a
/// workflow beside the workflow, about the workspace or the node beside the
/// conversation itself; a goal's thread beside the goal; a channel's or a
/// direct message's turn beside that channel or message; a work item in
/// the checkout it was placed in, else beside the item; about a drawing or a
/// note beside the conversation itself. A goal named by the session counts only when it exists — the MCP
/// scope defaults `goal` to the scope id, and a conversation's id is not a
/// goal.
pub(crate) fn home_of(
    inner: &Inner,
    facts: &ScopeFacts,
    scope: Option<&str>,
    work_item: Option<WorkItemId>,
    goal_named: Option<GoalId>,
) -> Option<BrowserHome> {
    if let Some(conversation) = &facts.conversation {
        return Some(match &conversation.origin {
            ConversationOrigin::Workstream { id, .. } => {
                BrowserHome::new(BrowserHomeScope::Workstream, id)
            }
            ConversationOrigin::Project { .. } => match facts.workstream() {
                Some(w) => BrowserHome::new(BrowserHomeScope::Workstream, w),
                None => BrowserHome::new(BrowserHomeScope::Conversation, conversation.id),
            },
            ConversationOrigin::Goal { id } => BrowserHome::new(BrowserHomeScope::Goal, id),
            ConversationOrigin::Workflow { id } => BrowserHome::new(BrowserHomeScope::Workflow, id),
            ConversationOrigin::Node
            | ConversationOrigin::Workspace
            | ConversationOrigin::Drawing { .. }
            | ConversationOrigin::Note { .. } => {
                BrowserHome::new(BrowserHomeScope::Conversation, conversation.id)
            }
        });
    }
    if let Some(goal) = facts.goal {
        return Some(BrowserHome::new(BrowserHomeScope::Goal, goal));
    }
    if let Some(scope) = scope {
        if let Some(channel) = ChannelId::from_str(scope)
            .ok()
            .and_then(|id| inner.ws.get_channel(&id).ok())
        {
            let kind = match channel.kind {
                ChannelKind::Direct => BrowserHomeScope::Dm,
                ChannelKind::Standing => BrowserHomeScope::Channel,
            };
            return Some(BrowserHome::new(kind, channel.id));
        }
    }
    if let Some(item) = work_item {
        return Some(match checkout_of_work_item(inner, item) {
            Some(w) => BrowserHome::new(BrowserHomeScope::Workstream, w),
            None => BrowserHome::new(BrowserHomeScope::WorkItem, item),
        });
    }
    goal_named
        .filter(|g| inner.ws.get_goal(*g).is_ok())
        .map(|g| BrowserHome::new(BrowserHomeScope::Goal, g))
}

/// The checkout a work item was placed in, when it was placed in one.
fn checkout_of_work_item(inner: &Inner, item: WorkItemId) -> Option<bisa_core::WorkstreamId> {
    inner
        .ws
        .list_workstreams(bisa_store::WorkstreamFilter::WorkItem(item))
        .ok()?
        .into_iter()
        .next()
        .map(|w| w.id)
}

/// The project whose `browser.*` settings bind the asking session: the
/// checkout's for a conversation about one, the work item's for a step
/// placed in a project, none elsewhere.
pub(crate) fn project_of(
    inner: &Inner,
    facts: &ScopeFacts,
    work_item: Option<WorkItemId>,
) -> Option<ProjectId> {
    facts.project().or_else(|| {
        work_item
            .and_then(|wi| crate::executor::find_work_item(inner, wi))
            .and_then(|spec| spec.project)
    })
}

/// A dialog the page asked while the agent's request ran — answered for it,
/// and reported.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BrowserDialog {
    /// `alert` · `confirm` · `prompt`.
    pub kind: String,
    pub message: String,
    /// What the page was answered: nothing for an alert, `true` for a
    /// confirm, the default text for a prompt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer: Option<String>,
}

/// One line the page wrote to its console, or an error it raised.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConsoleLine {
    /// `error` · `warn` · `info` · `log` · `uncaught` · `rejection` · `resource`.
    pub level: String,
    pub text: String,
    /// Milliseconds since the page loaded.
    pub at: u64,
}

/// Where a page is scrolled to, and how big it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScrollPosition {
    pub x: i64,
    pub y: i64,
    pub width: i64,
    pub height: i64,
}

/// What the desktop answers: `ok` with the facts, or a refusal in words.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BrowserResult {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// The page's or the element's text, a find's matches, a snapshot's
    /// outline — bounded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tabs: Vec<BrowserTab>,
    /// The act moved the page: the answer is the page it moved to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub navigated: Option<bool>,
    /// How many a find or a snapshot counted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<u32>,
    /// How long a wait waited.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub waited_ms: Option<u64>,
    /// Where the page stands after a scroll.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scroll: Option<ScrollPosition>,
    /// A script's value — JSON, bounded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<serde_json::Value>,
    /// The dialogs the page asked since the last answer, each answered.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dialogs: Vec<BrowserDialog>,
    /// What the page wrote to its console.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub console: Vec<ConsoleLine>,
    /// A screenshot's PNG, uploaded to the workspace's attachments by the
    /// desktop; the engine turns it into [`Self::path`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screenshot: Option<AttachmentRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    /// The screenshot's named copy on this machine — set by the engine, never
    /// by the desktop, which names no path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

impl BrowserResult {
    /// A refusal, in words.
    pub fn refused(why: impl Into<String>) -> Self {
        Self {
            ok: false,
            error: Some(why.into()),
            ..Self::default()
        }
    }

    /// The named copy the desktop's upload becomes, written beside the
    /// store: what the agent reads with its own tools. A result with no
    /// screenshot is left as it is; one the store cannot name is answered
    /// as a refusal, since a path that is not there is worse than none.
    pub fn with_named_copy(mut self, inner: &Inner) -> Self {
        let Some(shot) = self.screenshot.as_ref() else {
            return self;
        };
        match inner.ws.put_attachment_named(&shot.sha256, &shot.name) {
            Ok(path) => {
                self.path = Some(path.display().to_string());
                self
            }
            Err(e) => Self::refused(format!("the screenshot could not be kept: {e}")),
        }
    }

    /// The text and the value cut to [`MAX_BROWSER_TEXT`] on a character
    /// boundary, with a note when they were; the dialogs and the console
    /// lines held to their counts.
    pub fn bounded(mut self) -> Self {
        if let Some(text) = self.text.take() {
            self.text = Some(bound(text));
        }
        if let Some(value) = self.value.take() {
            let json = value.to_string();
            self.value = Some(if json.len() > MAX_BROWSER_TEXT {
                serde_json::Value::String(bound(json))
            } else {
                value
            });
        }
        self.dialogs.truncate(MAX_DIALOGS);
        self.console.truncate(MAX_CONSOLE_LINES);
        self
    }
}

/// How many dialogs and console lines one answer carries at most.
pub const MAX_DIALOGS: usize = 20;
pub const MAX_CONSOLE_LINES: usize = 100;

/// An open tab as the desktop lists it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BrowserTab {
    pub key: String,
    pub url: String,
    pub title: String,
}

fn bound(text: String) -> String {
    if text.len() <= MAX_BROWSER_TEXT {
        return text;
    }
    let mut end = MAX_BROWSER_TEXT;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n… (cut at {} bytes)", &text[..end], MAX_BROWSER_TEXT)
}

/// A request the desktop has not answered yet.
#[derive(Clone, Debug, Serialize, schemars::JsonSchema)]
pub struct PendingBrowserRequest {
    pub id: String,
    pub request: BrowserRequest,
    pub scope: BrowserScope,
    /// The engine's decision: a tab this request opens is kept out of sight
    /// — the policy's word, or the agent's (`headless_for`).
    pub headless: bool,
    pub asked_at: u64,
}

impl crate::parked::Parked for PendingBrowserRequest {
    type Answer = BrowserResult;
    fn id(&self) -> &str {
        &self.id
    }
    fn label(&self) -> String {
        format!("{:?}", self.request.action)
    }
}

/// The parked requests, by id — and when a desktop was last heard from: the
/// shared desk ([`crate::parked::Desk`]) in the browser's words.
pub struct BrowserRequests {
    desk: crate::parked::Desk<PendingBrowserRequest>,
}

impl Default for BrowserRequests {
    fn default() -> Self {
        Self::new()
    }
}

impl BrowserRequests {
    pub fn new() -> Self {
        Self {
            desk: crate::parked::Desk::new("browser", DESKTOP_PRESENCE_TTL),
        }
    }

    /// Whether a desktop was heard from within [`DESKTOP_PRESENCE_TTL`] —
    /// what decides between parking a request and saying nobody is home.
    pub fn desktop_present(&self) -> bool {
        self.desk.desktop_present()
    }

    /// Park a request and put it on the bus; the receiver resolves when the
    /// desktop answers. `headless` is the engine's decision on a tab the
    /// request opens.
    pub fn ask(
        &self,
        inner: &Inner,
        request: BrowserRequest,
        scope: BrowserScope,
        headless: bool,
    ) -> (String, watch::Receiver<Option<BrowserResult>>) {
        let id = crate::parked::Desk::<PendingBrowserRequest>::new_id();
        let pending = PendingBrowserRequest {
            id: id.clone(),
            request: request.clone(),
            scope: scope.clone(),
            headless,
            asked_at: now_secs(),
        };
        tracing::debug!(
            id = %id,
            action = ?request.action,
            agent = scope.agent.as_deref().unwrap_or("none"),
            home = ?scope.home,
            headless,
            "browser request parked"
        );
        let rx = self.desk.park(pending);
        inner.emit(crate::events::EngineEvent::global(
            EnginePayload::BrowserRequest {
                id: id.clone(),
                request,
                scope,
                headless,
            },
        ));
        (id, rx)
    }

    /// The desktop's answer, bounded; `false` when nothing waits under that id.
    pub fn answer(&self, id: &str, result: BrowserResult) -> bool {
        self.desk.answer(id, result.bounded())
    }

    /// Every request still waiting, oldest first — what a desktop that just
    /// opened reads to catch up.
    pub fn pending(&self) -> Vec<PendingBrowserRequest> {
        self.desk.pending()
    }

    /// Wait for the answer, or say the desktop was silent after
    /// [`ANSWER_TIMEOUT`]; a request that times out is forgotten.
    pub async fn wait(
        &self,
        id: &str,
        rx: watch::Receiver<Option<BrowserResult>>,
    ) -> BrowserResult {
        self.wait_for(id, rx, ANSWER_TIMEOUT).await
    }

    /// [`wait`](Self::wait) with its patience given — the mechanism, so a
    /// test can run the timeout in milliseconds. A request parked here had a
    /// desktop to answer it (`desktop_present` was true), so a silence is
    /// [`DESKTOP_SILENT`], never [`NOBODY_HOME`].
    pub async fn wait_for(
        &self,
        id: &str,
        rx: watch::Receiver<Option<BrowserResult>>,
        patience: Duration,
    ) -> BrowserResult {
        self.desk
            .wait_for(id, rx, patience, || BrowserResult::refused(DESKTOP_SILENT))
            .await
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// The file name a screenshot of a tab is kept under: `browser-<tab>-<ulid>.png`.
pub fn screenshot_name(tab: &str) -> String {
    let tab: String = tab
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(16)
        .collect();
    format!(
        "browser-{}-{}.png",
        if tab.is_empty() { "tab" } else { &tab },
        ulid::Ulid::from_datetime(SystemTime::now())
    )
}

/// Whether a URL names a page served on this machine — where `local_only`
/// lets an agent go. Not a URL at all is not local.
pub fn is_local_url(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url) else {
        return false;
    };
    matches!(parsed.scheme(), "http" | "https")
        && matches!(
            parsed.host_str(),
            Some("localhost" | "127.0.0.1" | "[::1]" | "::1")
        )
}

// -- who may ask ----------------------------------------------------------

/// `browser.agents`: which agents the browser tools answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentsPolicy {
    /// Agents carrying the Embedded Browser skill; the core agents always.
    Assigned,
    Everyone,
    Nobody,
}

/// `browser.agents.reach`: where an agent may navigate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reach {
    Anywhere,
    LocalOnly,
}

/// `browser.agents.headless`: when a tab an agent opens is kept out of sight.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeadlessPolicy {
    /// In a goal in auto mode and its runs, where nobody is watching.
    Unattended,
    Always,
    Never,
}

/// `browser.agents.scripts`: whether an agent may evaluate a script in a page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScriptsPolicy {
    Allow,
    Refuse,
}

/// Whether a tab an agent opens is kept out of sight: the agent's word
/// when it gave one, else the policy's — *unattended* reading the goal's
/// mode, and no goal at all as attended, since a conversation has a person.
pub fn headless_for(policy: HeadlessPolicy, mode: Option<GoalMode>, asked: Option<bool>) -> bool {
    if let Some(asked) = asked {
        return asked;
    }
    match policy {
        HeadlessPolicy::Always => true,
        HeadlessPolicy::Never => false,
        HeadlessPolicy::Unattended => mode.is_some_and(GoalMode::unattended),
    }
}

/// The one question a tab's visibility asks the Decision-Making Agent.
const WATCH_QUESTION: &str = "needs_a_person";

/// Whether a tab the rule would keep out of sight should be shown after all:
/// how the Decision-Making Agent reads where the agent is going — a sign-in, a
/// consent screen, a payment, a captcha need a person at the page. Asked only
/// where the rule had a choice to make: the policy is *unattended*, the agent
/// gave no word, the rule said out of sight, and the request opens a URL.
/// `None` when it is not asked, is not sure, or does not answer — the rule's
/// word stands.
pub async fn shown_after_all(
    inner: &Inner,
    access: &Access,
    request: &BrowserRequest,
    rule_says_headless: bool,
    standing: crate::decider::Standing,
) -> Option<bool> {
    let asks = access.headless == HeadlessPolicy::Unattended
        && request.headless.is_none()
        && rule_says_headless
        && request.action == BrowserAction::Open;
    let url = request.url.as_deref().filter(|_| asks)?;
    let question = bisa_core::DecisionQuestion::Noul {
        instructions: "An agent working unattended is about to open this page in a browser tab \
                       nobody is watching. Does the page need a person at it?"
            .into(),
        criteria: Some(bisa_core::NoulCriteria {
            yes: Some(
                "a sign-in, a consent or permission screen, a payment, a captcha, or anything \
                 only the account's owner should do"
                    .into(),
            ),
            no: Some("a page an agent can read and use on its own".into()),
        }),
    };
    let request = bisa_core::DecisionRequest::one(
        serde_json::json!({ "url": url }),
        WATCH_QUESTION,
        question,
    );
    crate::decider::judge(
        inner,
        bisa_core::DecisionPoint::BrowserHeadless,
        &standing,
        request,
    )
    .await
    .answered()
    .and_then(|r| r.answer(WATCH_QUESTION)?.noul())
    .map(|needs_a_person| needs_a_person > 0.5)
}

/// The workspace's word on the browser, resolved for a project when the
/// asking session has one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Access {
    pub enabled: bool,
    pub agents: AgentsPolicy,
    pub reach: Reach,
    pub headless: HeadlessPolicy,
    pub scripts: ScriptsPolicy,
}

/// The five settings, read once per op; a value the registry cannot give
/// falls to the default the registry declares.
pub fn access(inner: &Inner, project: Option<ProjectId>) -> Access {
    let word = |key: &str| {
        inner
            .ws
            .setting(key, project)
            .ok()
            .and_then(|r| r.value.as_str().map(str::to_string))
    };
    let enabled = inner
        .ws
        .setting("browser.enabled", project)
        .ok()
        .and_then(|r| r.value.as_bool())
        .unwrap_or(true);
    let agents = match word("browser.agents").as_deref() {
        Some("assigned") => AgentsPolicy::Assigned,
        Some("nobody") => AgentsPolicy::Nobody,
        _ => AgentsPolicy::Everyone,
    };
    let reach = match word("browser.agents.reach").as_deref() {
        Some("local_only") => Reach::LocalOnly,
        _ => Reach::Anywhere,
    };
    let headless = match word("browser.agents.headless").as_deref() {
        Some("always") => HeadlessPolicy::Always,
        Some("never") => HeadlessPolicy::Never,
        _ => HeadlessPolicy::Unattended,
    };
    let scripts = match word("browser.agents.scripts").as_deref() {
        Some("refuse") => ScriptsPolicy::Refuse,
        _ => ScriptsPolicy::Allow,
    };
    Access {
        enabled,
        agents,
        reach,
        headless,
        scripts,
    }
}

impl Access {
    /// Why this request is refused before it is parked, or `None` when it
    /// may go: the switch, the policy against the asking agent, the reach
    /// against an `open`'s URL, the scripts policy against an `eval`.
    /// `has_skill` is whether the agent carries [`BROWSER_SKILL`] — looked
    /// up by the caller, since a core agent needs no lookup at all.
    /// Whether this agent may use the browser tools at all: the switch and
    /// the agents policy — what every browser tool, `browser_serve`
    /// included, checks before anything else.
    pub fn may_use(
        &self,
        agent: &AgentId,
        has_skill: impl FnOnce() -> bool,
    ) -> Option<&'static str> {
        if !self.enabled {
            return Some(OFF);
        }
        match self.agents {
            AgentsPolicy::Nobody => Some(NOBODY_MAY),
            AgentsPolicy::Assigned if !agent.is_core_id() && !has_skill() => Some(NOT_ASSIGNED),
            _ => None,
        }
    }

    pub fn refusal(
        &self,
        agent: &AgentId,
        has_skill: impl FnOnce() -> bool,
        request: &BrowserRequest,
    ) -> Option<&'static str> {
        if let Some(why) = self.may_use(agent, has_skill) {
            return Some(why);
        }
        if self.reach == Reach::LocalOnly
            && request.action == BrowserAction::Open
            && !request.url.as_deref().is_some_and(is_local_url)
        {
            return Some(LOCAL_ONLY);
        }
        if self.scripts == ScriptsPolicy::Refuse && request.action == BrowserAction::Eval {
            return Some(SCRIPTS_REFUSED);
        }
        None
    }
}

/// A page on disk becomes a URL (ide/18 §Serving a folder): the node hosts
/// a static server for a folder of a checkout and lends the engine this
/// port, so a session can ask for one through `browser_serve` and
/// `browser_open` what it answers. The engine owns no HTTP, so the server
/// is the node's; an engine without a node — a one-shot CLI command — has
/// none, and the op says so ([`NO_SERVER`]).
#[async_trait::async_trait]
pub trait FolderServer: Send + Sync {
    /// Serve `folder` of `checkout` (the checkout itself when empty) on a
    /// loopback port, or say in a sentence why not.
    async fn serve(
        &self,
        workstream: bisa_core::WorkstreamId,
        checkout: &std::path::Path,
        folder: &str,
    ) -> Result<ServedPage, String>;
}

/// What `browser_serve` answers: the server's id, its root URL, the page
/// to open, and the folder it serves relative to the checkout.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ServedPage {
    pub id: String,
    pub url: String,
    pub page: String,
    pub folder: String,
}

/// `browser_serve` on an engine no node lent a server to.
pub const NO_SERVER: &str = "this engine hosts no server for a folder: run the project's run command in a terminal and browser_open the port it opens, or ask the person to serve the checkout from the IDE's Browser menu";
/// `browser_serve` from a session that stands in no checkout.
pub const NO_CHECKOUT: &str = "this session stands in no checkout, so there is no folder to serve: a page on disk lives in a project — create_project when the goal has none, write the page there, and serve that";

#[cfg(test)]
mod tests {
    use super::*;

    fn request(action: BrowserAction) -> BrowserRequest {
        BrowserRequest {
            action,
            ..BrowserRequest::default()
        }
    }

    fn open(url: &str) -> BrowserRequest {
        BrowserRequest {
            url: Some(url.into()),
            ..request(BrowserAction::Open)
        }
    }

    const ACCESS: Access = Access {
        enabled: true,
        agents: AgentsPolicy::Everyone,
        reach: Reach::Anywhere,
        headless: HeadlessPolicy::Unattended,
        scripts: ScriptsPolicy::Allow,
    };

    #[test]
    fn a_result_is_bounded_on_a_character_boundary_and_says_so() {
        let long = "é".repeat(MAX_BROWSER_TEXT);
        let r = BrowserResult {
            ok: true,
            text: Some(long),
            ..BrowserResult::default()
        }
        .bounded();
        let text = r.text.unwrap();
        assert!(text.ends_with(&format!("… (cut at {MAX_BROWSER_TEXT} bytes)")));
        assert!(text.is_char_boundary(text.find('\n').unwrap()));
        let short = BrowserResult::refused("no").bounded();
        assert_eq!(short.error.as_deref(), Some("no"));
        assert!(!short.ok);
    }

    #[test]
    fn a_scripts_value_is_bounded_and_the_lists_held_to_their_counts() {
        let big = serde_json::Value::String("x".repeat(MAX_BROWSER_TEXT * 2));
        let r = BrowserResult {
            ok: true,
            value: Some(big),
            dialogs: (0..MAX_DIALOGS + 5)
                .map(|i| BrowserDialog {
                    kind: "alert".into(),
                    message: format!("m{i}"),
                    answer: None,
                })
                .collect(),
            console: (0..MAX_CONSOLE_LINES + 5)
                .map(|i| ConsoleLine {
                    level: "log".into(),
                    text: format!("l{i}"),
                    at: i as u64,
                })
                .collect(),
            ..BrowserResult::default()
        }
        .bounded();
        let value = r.value.unwrap();
        assert!(value
            .as_str()
            .unwrap()
            .ends_with(&format!("… (cut at {MAX_BROWSER_TEXT} bytes)")));
        assert_eq!(r.dialogs.len(), MAX_DIALOGS);
        assert_eq!(r.console.len(), MAX_CONSOLE_LINES);
        let small = BrowserResult {
            ok: true,
            value: Some(serde_json::json!({"n": 1})),
            ..BrowserResult::default()
        }
        .bounded();
        assert_eq!(
            small.value,
            Some(serde_json::json!({"n": 1})),
            "a small value stays JSON"
        );
    }

    #[test]
    fn access_refuses_by_switch_policy_skill_reach_and_scripts_in_that_order() {
        let read = BrowserRequest {
            tab: Some("b1".into()),
            ..request(BrowserAction::Read)
        };
        let dev = AgentId::new("developer").unwrap();
        let general = AgentId::general();
        let off = Access {
            enabled: false,
            ..ACCESS
        };
        assert_eq!(off.refusal(&general, || true, &read), Some(OFF));
        let nobody = Access {
            agents: AgentsPolicy::Nobody,
            ..ACCESS
        };
        assert_eq!(
            nobody.refusal(&general, || true, &read),
            Some(NOBODY_MAY),
            "nobody means the core agents too"
        );
        let assigned = Access {
            agents: AgentsPolicy::Assigned,
            ..ACCESS
        };
        assert_eq!(assigned.refusal(&dev, || false, &read), Some(NOT_ASSIGNED));
        assert_eq!(
            assigned.refusal(&dev, || true, &read),
            None,
            "the skill is the assignment"
        );
        assert_eq!(
            assigned.refusal(&general, || false, &read),
            None,
            "a core agent needs no skill"
        );
        let local = Access {
            reach: Reach::LocalOnly,
            ..ACCESS
        };
        assert_eq!(
            local.refusal(&dev, || false, &open("https://example.com/")),
            Some(LOCAL_ONLY)
        );
        assert_eq!(
            local.refusal(&dev, || false, &open("http://localhost:5173/app")),
            None
        );
        assert_eq!(
            local.refusal(&dev, || false, &read),
            None,
            "reach is about where a tab goes, not about reading one"
        );
        assert_eq!(
            local.refusal(&dev, || false, &request(BrowserAction::Reload)),
            None,
            "a reload moves within a history the reach admitted"
        );
        let no_scripts = Access {
            scripts: ScriptsPolicy::Refuse,
            ..ACCESS
        };
        let eval = BrowserRequest {
            tab: Some("b1".into()),
            expression: Some("1 + 1".into()),
            ..request(BrowserAction::Eval)
        };
        assert_eq!(
            no_scripts.refusal(&dev, || false, &eval),
            Some(SCRIPTS_REFUSED)
        );
        assert_eq!(
            no_scripts.refusal(&general, || false, &eval),
            Some(SCRIPTS_REFUSED),
            "the core agents too"
        );
        assert_eq!(
            no_scripts.refusal(&dev, || false, &read),
            None,
            "reading is not a script"
        );
        assert_eq!(
            ACCESS.refusal(&dev, || false, &eval),
            None,
            "allowed by default"
        );
    }

    #[test]
    fn a_tab_is_headless_by_the_agents_word_else_the_policy_else_the_goals_mode() {
        use HeadlessPolicy::*;
        assert!(
            headless_for(Unattended, Some(GoalMode::Auto), None),
            "an auto goal runs with nobody watching"
        );
        assert!(!headless_for(Unattended, Some(GoalMode::Guided), None));
        assert!(!headless_for(Unattended, Some(GoalMode::Manual), None));
        assert!(
            !headless_for(Unattended, None, None),
            "no goal: a conversation, with a person in it"
        );
        assert!(headless_for(Always, None, None));
        assert!(headless_for(Always, Some(GoalMode::Guided), None));
        assert!(!headless_for(Never, Some(GoalMode::Auto), None));
        assert!(
            !headless_for(Unattended, Some(GoalMode::Auto), Some(false)),
            "the agent's word wins: a person should watch"
        );
        assert!(
            headless_for(Never, None, Some(true)),
            "… and out of sight when it asks so"
        );
    }

    #[test]
    fn nobody_is_home_until_a_desktop_reads_the_list_or_answers() {
        let requests = BrowserRequests::new();
        assert!(
            !requests.desktop_present(),
            "no desktop has been heard from"
        );
        requests.pending();
        assert!(
            requests.desktop_present(),
            "a read of the list is a desktop"
        );
        let again = BrowserRequests::new();
        again.answer("nothing", BrowserResult::refused("x"));
        assert!(
            again.desktop_present(),
            "an answer is a desktop too, even for nothing"
        );
        assert!(
            DESKTOP_PRESENCE_TTL < ANSWER_TIMEOUT,
            "presence is judged before a request waits"
        );
        assert!(
            Duration::from_millis(MAX_WAIT_MS) < ANSWER_TIMEOUT,
            "the longest wait answers before the op gives up"
        );
    }

    #[test]
    fn local_urls_are_loopback_hosts_over_http() {
        assert!(is_local_url("http://localhost:5173/"));
        assert!(is_local_url("http://127.0.0.1:3000/x?y=1"));
        assert!(is_local_url("http://[::1]:8080/"));
        assert!(!is_local_url("https://example.com/"));
        assert!(!is_local_url("http://localhost.evil.com/"));
        assert!(!is_local_url("file:///etc/hosts"));
        assert!(!is_local_url("not a url"));
    }

    #[test]
    fn a_screenshot_name_is_a_png_named_after_the_tab() {
        let name = screenshot_name("b7");
        assert!(name.starts_with("browser-b7-"), "{name}");
        assert!(name.ends_with(".png"));
        assert!(
            screenshot_name("../x").starts_with("browser-x-"),
            "only word characters of the tab survive"
        );
        assert!(screenshot_name("").starts_with("browser-tab-"));
    }

    #[test]
    fn a_request_serialises_by_action_and_omits_what_it_lacks() {
        let r = BrowserRequest {
            tab: Some("b1".into()),
            target: Some("main".into()),
            format: Some(ReadFormat::Html),
            ..request(BrowserAction::Read)
        };
        let json = serde_json::to_value(&r).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"action": "read", "tab": "b1", "target": "main", "format": "html"})
        );
        assert_eq!(serde_json::from_value::<BrowserRequest>(json).unwrap(), r);
        let w = BrowserRequest {
            tab: Some("b1".into()),
            until: Some(WaitUntil::Text),
            query: Some("Signed in".into()),
            timeout_ms: Some(90_000),
            ..request(BrowserAction::Wait)
        };
        assert_eq!(
            serde_json::to_value(&w).unwrap(),
            serde_json::json!({"action": "wait", "tab": "b1", "query": "Signed in", "until": "text", "timeout_ms": 90000})
        );
        assert_eq!(w.wait_ms(), MAX_WAIT_MS, "a wait is held to the most");
        assert_eq!(request(BrowserAction::Wait).wait_ms(), DEFAULT_WAIT_MS);
        let p = BrowserRequest {
            tab: Some("b1".into()),
            key: Some("Enter".into()),
            modifiers: vec!["shift".into()],
            ..request(BrowserAction::Press)
        };
        assert_eq!(
            serde_json::to_value(&p).unwrap(),
            serde_json::json!({"action": "press", "tab": "b1", "key": "Enter", "modifiers": ["shift"]})
        );
    }

    #[test]
    fn the_acts_that_may_move_the_page_are_named() {
        for a in [
            BrowserAction::Open,
            BrowserAction::Click,
            BrowserAction::Fill,
            BrowserAction::Type,
            BrowserAction::Press,
            BrowserAction::Select,
            BrowserAction::Back,
            BrowserAction::Forward,
            BrowserAction::Reload,
        ] {
            assert!(a.may_navigate(), "{a:?}");
        }
        for a in [
            BrowserAction::Tabs,
            BrowserAction::Read,
            BrowserAction::Find,
            BrowserAction::Snapshot,
            BrowserAction::Hover,
            BrowserAction::Scroll,
            BrowserAction::Wait,
            BrowserAction::Console,
            BrowserAction::Eval,
            BrowserAction::Close,
            BrowserAction::Screenshot,
        ] {
            assert!(!a.may_navigate(), "{a:?}");
        }
    }

    #[test]
    fn a_home_serialises_as_the_desktops_scope_words() {
        let scope = BrowserScope {
            home: Some(BrowserHome::new(BrowserHomeScope::Dm, "dm-01abc")),
            agent: Some("general-agent".into()),
        };
        assert_eq!(
            serde_json::to_value(&scope).unwrap(),
            serde_json::json!({"home": {"scope": "dm", "id": "dm-01abc"}, "agent": "general-agent"})
        );
        for (scope, word) in [
            (BrowserHomeScope::Workstream, "workstream"),
            (BrowserHomeScope::Goal, "goal"),
            (BrowserHomeScope::WorkItem, "work_item"),
            (BrowserHomeScope::Workflow, "workflow"),
            (BrowserHomeScope::Channel, "channel"),
            (BrowserHomeScope::Dm, "dm"),
            (BrowserHomeScope::Conversation, "conversation"),
        ] {
            assert_eq!(
                serde_json::to_value(scope).unwrap(),
                serde_json::json!(word)
            );
        }
    }
}
