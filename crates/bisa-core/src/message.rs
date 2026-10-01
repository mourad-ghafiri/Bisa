//! Messages: one kind (3407) for every conversation surface, with a typed body.
//!
//! A [`MessageBody::Post`] may carry **context**: the chips a person attached
//! above the composer. Nothing is injected that is not a chip — so what the
//! agent sees is exactly this list, and the list is on the wire.
//!
//! It may also carry **artifacts** ([`crate::artifact`]): what an agent made
//! for the person to look at, each a titled, kinded descriptor of bytes the
//! attachment store holds. The descriptor is in the body so a peer knows what
//! it is looking at before it holds a byte.

use crate::artifact::{ArtifactRef, MAX_ARTIFACTS_PER_MESSAGE};
use crate::channel::MembershipEvent;
use crate::id::{CommitIdStr, WorkItemId};
use crate::path::RelPath;
use serde::{Deserialize, Serialize};

/// Hard cap on a serialised `context` list.
pub const MAX_CONTEXT_BYTES: usize = 64 * 1024;

/// Which message stream a message belongs to: a channel (standing or
/// direct), a goal's thread, or a conversation
/// ([`crate::conversation::Conversation`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ScopeKind {
    Channel,
    Goal,
    /// A saved exchange a person started with agents, with an origin of its
    /// own — where the IDE's Agent pane lives.
    Conversation,
}

impl ScopeKind {
    pub const ALL: [ScopeKind; 3] = [ScopeKind::Channel, ScopeKind::Goal, ScopeKind::Conversation];

    pub fn as_str(self) -> &'static str {
        match self {
            ScopeKind::Channel => "channel",
            ScopeKind::Goal => "goal",
            ScopeKind::Conversation => "conversation",
        }
    }
}

/// Longest thinking a reply keeps, in bytes: the tail of what the model
/// reasoned before it answered — the end is what led to the words.
pub const MAX_THINKING_BYTES: usize = 64 * 1024;

/// Longest text one message carries, in bytes — a long reply with a diff
/// pasted in fits; anything that is really a file goes as an attachment.
/// Judged before the node's body limit is, so an oversized post is a refusal
/// in words like every other, never a bare 413.
pub const MAX_TEXT_BYTES: usize = 256 * 1024;

impl std::str::FromStr for ScopeKind {
    type Err = crate::CoreError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        ScopeKind::ALL
            .into_iter()
            .find(|k| k.as_str() == s)
            .ok_or_else(|| crate::CoreError::UnknownScopeKind(s.to_string()))
    }
}

/// The content of a kind-3407 event.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "snake_case", tag = "body")]
pub enum MessageBody {
    Post {
        text: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        context: Vec<ContextRef>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        artifacts: Vec<ArtifactRef>,
        /// An agent's reasoning before its words, when its harness said it
        /// — kept beside the reply for a person who wants to read it; never
        /// on a person's own post.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        thinking: Option<String>,
        /// The platform's own sentence, when the post is one — the Workflow
        /// Agent's standing note — as data: the reader renders it in their
        /// language, `text` being its English ([17](../../../docs/architecture/17-internationalisation.md)).
        /// Never on a person's or an agent's own words.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        said: Option<crate::text::Text>,
    },
    Membership(MembershipEvent),
}

impl MessageBody {
    pub fn post(text: impl Into<String>) -> Self {
        MessageBody::Post {
            text: text.into(),
            context: vec![],
            artifacts: vec![],
            thinking: None,
            said: None,
        }
    }

    /// A sentence the platform authors, posted as a note: its English as the
    /// text, itself as `said` for a reader in another language.
    pub fn said(text: crate::text::Text) -> Self {
        MessageBody::Post {
            text: text.to_string(),
            context: vec![],
            artifacts: vec![],
            thinking: None,
            said: Some(text),
        }
    }

    /// An agent's reply with the thinking that led to it; blank thinking is
    /// none.
    pub fn post_with_thinking(text: impl Into<String>, thinking: impl Into<String>) -> Self {
        let thinking: String = thinking.into();
        MessageBody::Post {
            text: text.into(),
            context: vec![],
            artifacts: vec![],
            thinking: (!thinking.trim().is_empty()).then_some(thinking),
            said: None,
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            MessageBody::Post { .. } => "post",
            MessageBody::Membership(_) => "membership",
        }
    }

    /// The platform's own sentence a post carries — the message of the
    /// catalog behind its English — when it authored the post
    /// ([`MessageBody::said`] made it).
    pub fn sentence(&self) -> Option<&crate::text::Text> {
        match self {
            MessageBody::Post { said, .. } => said.as_ref(),
            MessageBody::Membership(_) => None,
        }
    }

    /// The artifacts a post carries; none on a membership event.
    pub fn artifacts(&self) -> &[ArtifactRef] {
        match self {
            MessageBody::Post { artifacts, .. } => artifacts,
            MessageBody::Membership(_) => &[],
        }
    }

    /// The thinking a post carries, when its author's harness said any.
    pub fn thinking(&self) -> Option<&str> {
        match self {
            MessageBody::Post { thinking, .. } => thinking.as_deref(),
            MessageBody::Membership(_) => None,
        }
    }

    /// The text within [`MAX_TEXT_BYTES`]; the `context` list under
    /// [`MAX_CONTEXT_BYTES`] serialised; at most [`MAX_ARTIFACTS_PER_MESSAGE`]
    /// artifacts, each well-formed; the thinking within [`MAX_THINKING_BYTES`].
    pub fn validate(&self) -> Result<(), crate::CoreError> {
        if let MessageBody::Post {
            text,
            context,
            artifacts,
            thinking,
            ..
        } = self
        {
            if text.len() > MAX_TEXT_BYTES {
                return Err(crate::CoreError::TextTooLarge { bytes: text.len() });
            }
            if let Some(t) = thinking {
                if t.len() > MAX_THINKING_BYTES {
                    return Err(crate::CoreError::ThinkingTooLarge { bytes: t.len() });
                }
            }
            if artifacts.len() > MAX_ARTIFACTS_PER_MESSAGE {
                return Err(crate::CoreError::TooManyArtifacts(artifacts.len()));
            }
            for a in artifacts {
                a.validate()?;
            }
            if context.is_empty() {
                return Ok(());
            }
            let bytes = serde_json::to_vec(context)
                .map(|v| v.len())
                .unwrap_or(usize::MAX);
            if bytes > MAX_CONTEXT_BYTES {
                return Err(crate::CoreError::ContextTooLarge { bytes });
            }
        }
        Ok(())
    }
}

/// A code fence's mark, and the most of an opening line that is said again
/// where a part is cut inside the fence.
const FENCE: &str = "```";
const MAX_FENCE_LINE: usize = 32;

/// The largest index at or below `at` that stands between two characters.
fn boundary_at(text: &str, at: usize) -> usize {
    let mut at = at.min(text.len());
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

/// The fence open at the end of `text`, as the line that opened it.
fn open_fence(text: &str) -> Option<String> {
    let mut open: Option<String> = None;
    for line in text.lines() {
        let line = line.trim();
        if !line.starts_with(FENCE) {
            continue;
        }
        open = match open {
            Some(_) => None,
            None => Some(line[..boundary_at(line, MAX_FENCE_LINE)].to_string()),
        };
    }
    open
}

/// Where a part of at most `room` bytes ends and where the next begins: at
/// the last paragraph break that fits, else the last line break, else
/// between two characters. A part is never empty.
fn cut_of(text: &str, room: usize) -> (usize, usize) {
    let window = &text[..boundary_at(text, room)];
    if let Some(at) = window.rfind("\n\n").filter(|at| *at > 0) {
        return (at, at + 2);
    }
    if let Some(at) = window.rfind('\n').filter(|at| *at > 0) {
        return (at, at + 1);
    }
    if !window.is_empty() {
        return (window.len(), window.len());
    }
    // Not one character fits: the part is that character, whole.
    let first = text.chars().next().map_or(0, char::len_utf8);
    (first, first)
}

/// A text too long for one message, as the parts it is said in, in order:
/// each within `max` bytes, cut at the last paragraph break that fits, else
/// at the last line break, else between two characters. A code fence open
/// where a part is cut is closed there and opened again, under its own
/// word, at the head of the next — so every part reads by itself. A text
/// that fits is one part, and nothing is no part.
pub fn split_text(text: &str, max: usize) -> Vec<String> {
    let mut parts = Vec::new();
    let mut rest = text.trim();
    let mut fence: Option<String> = None;
    while !rest.is_empty() {
        let head = fence
            .take()
            .map(|line| format!("{line}\n"))
            .unwrap_or_default();
        let room = max.saturating_sub(head.len());
        if rest.len() <= room {
            parts.push(format!("{head}{rest}"));
            break;
        }
        // Room kept for the fence this part may have to close.
        let (end, next) = cut_of(rest, room.saturating_sub(FENCE.len() + 1));
        let mut part = format!("{head}{}", rest[..end].trim_end());
        fence = open_fence(&part);
        if fence.is_some() {
            part.push('\n');
            part.push_str(FENCE);
        }
        parts.push(part);
        rest = &rest[next..];
        if fence.is_none() {
            rest = rest.trim_start();
        }
    }
    parts
}

/// An inclusive 1-based line range. One line is a 1-length range.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LineRange {
    pub start: u32,
    pub end: u32,
}

impl LineRange {
    pub fn new(start: u32, end: u32) -> Result<Self, crate::CoreError> {
        if start == 0 || end < start {
            return Err(crate::CoreError::InvalidRange { start, end });
        }
        Ok(Self { start, end })
    }

    pub fn single(line: u32) -> Result<Self, crate::CoreError> {
        Self::new(line, line)
    }

    /// A range is never empty by construction, so there is no `is_empty`.
    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> u32 {
        self.end - self.start + 1
    }
}

/// Which diff a note or a hunk belongs to. Three different patches for one
/// path, so the side is always part of the reference.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "snake_case", tag = "scope")]
pub enum DiffScope {
    /// Worktree against the index.
    Unstaged,
    /// Index against HEAD.
    Staged,
    /// HEAD against a base ref.
    Branch { base: String },
}

/// A piece of context a person attached to a message. Every variant renders
/// as a chip; the bytes that matter are captured at send so a peer with the
/// same project can act on it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "snake_case", tag = "kind")]
pub enum ContextRef {
    Selection {
        path: RelPath,
        range: LineRange,
        text: String,
    },
    File {
        path: RelPath,
    },
    DiffHunk {
        path: RelPath,
        scope: DiffScope,
        hunk: String,
        patch: String,
    },
    Terminal {
        session: String,
        tail: String,
    },
    WorkItem {
        id: WorkItemId,
    },
    Commit {
        id: CommitIdStr,
    },
    /// An element of a rendered page the person pointed at, and the change
    /// they want for it (ide/03 §Annotate, ide/18). `page` is where the
    /// element lives — a file of the project, or a URL the IDE's browser
    /// showed; `excerpt` is the element as it was — the head of its HTML,
    /// captured at send — and is the identity; `selector` is a locator hint
    /// the page may have outgrown.
    Annotation {
        page: PageRef,
        selector: String,
        excerpt: String,
        note: String,
    },
    /// A device's screen the person captured and marked (ide/19): the
    /// simulator, emulator or phone by id and name, the PNG as an attachment
    /// the node holds, the rectangle they drew in device pixels — none for
    /// the whole screen — and the change they want.
    Capture {
        device: String,
        label: String,
        shot: crate::AttachmentRef,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mark: Option<Mark>,
        note: String,
    },
}

/// A rectangle on a captured screen, in device pixels from the top left.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Mark {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Mark {
    /// The rectangle as a sentence names it: `x 120, y 340 · 200×48`.
    pub fn words(&self) -> String {
        format!(
            "x {}, y {} · {}×{}",
            self.x, self.y, self.width, self.height
        )
    }
}

/// Where an annotated element lives (ide/18): a file of the project, when
/// the page is one the IDE renders or serves from the checkout, or a URL —
/// a page a dev server or the web put in the IDE's browser, whose source is
/// not a file the agent can name.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "snake_case", tag = "kind")]
pub enum PageRef {
    File { path: RelPath },
    Url { url: String },
}

impl PageRef {
    /// The page as a sentence names it: the path, or the URL.
    pub fn words(&self) -> String {
        match self {
            PageRef::File { path } => path.to_string(),
            PageRef::Url { url } => url.clone(),
        }
    }
}

impl ContextRef {
    /// The chip's one-line label.
    pub fn label(&self) -> String {
        match self {
            ContextRef::Selection { path, range, .. } => {
                format!("{path}:{}-{}", range.start, range.end)
            }
            ContextRef::File { path } => path.to_string(),
            ContextRef::DiffHunk { path, hunk, .. } => format!("{path} @ {hunk}"),
            ContextRef::Terminal { session, .. } => format!("terminal {session}"),
            ContextRef::WorkItem { id } => format!("work item {id}"),
            ContextRef::Commit { id } => format!("commit {}", &id.0[..id.0.len().min(7)]),
            ContextRef::Annotation { page, selector, .. } => {
                format!("{} · {selector}", page.words())
            }
            ContextRef::Capture { label, mark, .. } => match mark {
                Some(m) => format!("{label} · {}", m.words()),
                None => format!("{label} · the screen"),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_post_with_no_context_serialises_without_the_field() {
        let json = serde_json::to_value(MessageBody::post("hi")).unwrap();
        assert_eq!(json, serde_json::json!({"body": "post", "text": "hi"}));
    }

    #[test]
    fn a_text_that_fits_is_one_part_and_nothing_is_none() {
        assert_eq!(split_text("hello", 64), vec!["hello".to_string()]);
        assert_eq!(split_text("  hello\n", 64), vec!["hello".to_string()]);
        assert_eq!(split_text("x".repeat(64).as_str(), 64).len(), 1);
        assert!(split_text("", 64).is_empty());
        assert!(split_text(" \n\t ", 64).is_empty());
    }

    #[test]
    fn a_long_text_is_cut_where_a_paragraph_ends_and_nothing_is_lost() {
        let paragraphs: Vec<String> = ["a", "b", "c", "d"]
            .iter()
            .map(|letter| letter.repeat(40))
            .collect();
        let text = paragraphs.join("\n\n");
        let parts = split_text(&text, 64);
        assert_eq!(parts, paragraphs, "one paragraph a part");
        assert_eq!(parts.join("\n\n"), text);
        // Two fit where there is room for two.
        let parts = split_text(&text, 100);
        assert_eq!(parts.len(), 2);
        assert_eq!(parts.join("\n\n"), text);
    }

    #[test]
    fn with_no_paragraph_to_cut_at_a_line_is_and_then_a_character() {
        let lines = format!("{}\n{}\n{}", "a".repeat(30), "b".repeat(30), "c".repeat(30));
        let parts = split_text(&lines, 64);
        assert_eq!(parts.join("\n"), lines);
        assert!(parts.iter().all(|p| p.len() <= 64), "{parts:?}");

        // No break at all, and characters of several bytes: never cut inside one.
        let word = "né€𝄞".repeat(40);
        for max in [8, 9, 10, 11, 64, 65, 66, 67] {
            let parts = split_text(&word, max);
            assert!(parts.iter().all(|p| p.len() <= max), "{max}: {parts:?}");
            assert_eq!(parts.concat(), word, "{max}");
        }
    }

    #[test]
    fn a_fence_open_where_a_part_is_cut_is_closed_and_opened_again() {
        let code: Vec<String> = (0..40).map(|n| format!("let line_{n} = {n};")).collect();
        let text = format!(
            "Here it is.\n\n```rust\n{}\n```\n\nThat is all.",
            code.join("\n")
        );
        for max in [96, 128, 200, 512] {
            let parts = split_text(&text, max);
            assert!(parts.len() > 1, "{max}");
            for part in &parts {
                assert!(part.len() <= max, "{max}: a part of {} bytes", part.len());
                let marks = part.lines().filter(|l| l.trim().starts_with(FENCE)).count();
                assert_eq!(marks % 2, 0, "{max}: a part reads by itself:\n{part}");
            }
            let inside: Vec<&String> = parts.iter().filter(|p| p.contains("let line_")).collect();
            assert!(
                inside
                    .iter()
                    .all(|p| p.starts_with("```rust\n") || p.starts_with("Here it is.")),
                "{max}: the fence opens again under its own word"
            );
            let said: Vec<&str> = parts
                .iter()
                .flat_map(|p| p.lines())
                .filter(|l| l.starts_with("let line_"))
                .collect();
            assert_eq!(said, code, "{max}: every line of the code, once, in order");
            assert!(parts[0].starts_with("Here it is."), "{max}");
            assert!(parts[parts.len() - 1].ends_with("That is all."), "{max}");
        }
    }

    #[test]
    fn a_bound_smaller_than_a_character_still_ends() {
        let parts = split_text("𝄞𝄞𝄞", 1);
        assert_eq!(parts.concat(), "𝄞𝄞𝄞");
        assert_eq!(parts.len(), 3);
    }

    #[test]
    fn a_membership_body_flattens_its_event() {
        let body = MessageBody::Membership(MembershipEvent {
            channel: crate::id::ChannelId::general(),
            member: crate::channel::Member::Agent(crate::id::AgentId::new("developer").unwrap()),
            change: crate::channel::MembershipChange::Joined,
            cause: crate::channel::MembershipCause::Enabled,
            at: 1,
        });
        let json = serde_json::to_value(&body).unwrap();
        assert_eq!(json["body"], "membership");
        assert_eq!(json["channel"], "general");
        assert_eq!(serde_json::from_value::<MessageBody>(json).unwrap(), body);
    }

    #[test]
    fn context_is_bounded() {
        let big = ContextRef::Terminal {
            session: "t1".into(),
            tail: "x".repeat(MAX_CONTEXT_BYTES + 1),
        };
        let body = MessageBody::Post {
            text: "look".into(),
            context: vec![big],
            artifacts: vec![],
            thinking: None,
            said: None,
        };
        assert!(matches!(
            body.validate(),
            Err(crate::CoreError::ContextTooLarge { .. })
        ));
        assert!(MessageBody::post("fine").validate().is_ok());
    }

    #[test]
    fn a_capture_round_trips_as_kind_capture_with_or_without_its_mark() {
        let shot = crate::AttachmentRef {
            sha256: "ab".repeat(32),
            name: "mobile-A-01J.png".into(),
            mime: "image/png".into(),
            size: 1234,
        };
        let marked = ContextRef::Capture {
            device: "AAAA-1".into(),
            label: "iPhone 16".into(),
            shot: shot.clone(),
            mark: Some(Mark {
                x: 120,
                y: 340,
                width: 200,
                height: 48,
            }),
            note: "make the button blue".into(),
        };
        let json = serde_json::to_value(&marked).unwrap();
        assert_eq!(json["kind"], "capture");
        assert_eq!(json["mark"]["width"], 200);
        assert_eq!(serde_json::from_value::<ContextRef>(json).unwrap(), marked);
        assert_eq!(marked.label(), "iPhone 16 · x 120, y 340 · 200×48");
        let whole = ContextRef::Capture {
            device: "AAAA-1".into(),
            label: "iPhone 16".into(),
            shot,
            mark: None,
            note: "the whole screen is too dark".into(),
        };
        let json = serde_json::to_value(&whole).unwrap();
        assert!(json.get("mark").is_none(), "no mark, no field");
        assert_eq!(whole.label(), "iPhone 16 · the screen");
    }

    #[test]
    fn line_ranges_are_inclusive_and_one_based() {
        assert!(LineRange::new(0, 1).is_err());
        assert!(LineRange::new(5, 4).is_err());
        assert_eq!(LineRange::single(7).unwrap().len(), 1);
        assert_eq!(LineRange::new(3, 9).unwrap().len(), 7);
    }

    #[test]
    fn chips_have_labels_and_roundtrip() {
        let sel = ContextRef::Selection {
            path: RelPath::new("src/main.rs").unwrap(),
            range: LineRange::new(40, 52).unwrap(),
            text: "fn main() {}".into(),
        };
        assert_eq!(sel.label(), "src/main.rs:40-52");
        let json = serde_json::to_value(&sel).unwrap();
        assert_eq!(json["kind"], "selection");
        assert_eq!(serde_json::from_value::<ContextRef>(json).unwrap(), sel);
        // An element of a rendered page: the path and the locator label it,
        // the excerpt and the note ride along, and the wire word is `annotation`.
        let ann = ContextRef::Annotation {
            page: PageRef::File {
                path: RelPath::new("www/index.html").unwrap(),
            },
            selector: "body > main > button:nth-of-type(2)".into(),
            excerpt: "<button class=\"cta\">Buy</button>".into(),
            note: "make it blue".into(),
        };
        assert_eq!(
            ann.label(),
            "www/index.html · body > main > button:nth-of-type(2)"
        );
        let json = serde_json::to_value(&ann).unwrap();
        assert_eq!(json["kind"], "annotation");
        assert_eq!(json["page"]["kind"], "file");
        assert_eq!(json["page"]["path"], "www/index.html");
        assert_eq!(json["note"], "make it blue");
        assert_eq!(serde_json::from_value::<ContextRef>(json).unwrap(), ann);
        // A page the IDE's browser showed from a dev server: the URL is the
        // page, since no file of the project is its source.
        let served = ContextRef::Annotation {
            page: PageRef::Url {
                url: "http://localhost:5173/pricing".into(),
            },
            selector: "#cta".into(),
            excerpt: "<a id=\"cta\">Buy</a>".into(),
            note: "bigger".into(),
        };
        assert_eq!(served.label(), "http://localhost:5173/pricing · #cta");
        let json = serde_json::to_value(&served).unwrap();
        assert_eq!(json["page"]["kind"], "url");
        assert_eq!(serde_json::from_value::<ContextRef>(json).unwrap(), served);
        assert_eq!("goal".parse::<ScopeKind>().unwrap(), ScopeKind::Goal);
        assert!("room".parse::<ScopeKind>().is_err());
    }

    #[test]
    fn the_scopes_are_a_channel_a_goal_and_a_conversation_and_a_workstream_is_none() {
        assert_eq!(ScopeKind::ALL.len(), 3);
        for kind in ScopeKind::ALL {
            assert_eq!(kind.as_str().parse::<ScopeKind>().unwrap(), kind);
        }
        assert_eq!(
            "conversation".parse::<ScopeKind>().unwrap(),
            ScopeKind::Conversation
        );
        assert!(matches!(
            "workstream".parse::<ScopeKind>(),
            Err(crate::CoreError::UnknownScopeKind(_))
        ));
    }

    #[test]
    fn a_reply_keeps_its_thinking_beside_the_words_within_its_bound() {
        let plain = MessageBody::post("done");
        assert_eq!(plain.thinking(), None);
        assert_eq!(
            serde_json::to_value(&plain).unwrap().get("thinking"),
            None,
            "no key when there is none"
        );
        let thought =
            MessageBody::post_with_thinking("done", "first check the tests, then the docs");
        assert_eq!(thought.kind(), "post");
        assert_eq!(
            thought.thinking(),
            Some("first check the tests, then the docs")
        );
        assert!(thought.validate().is_ok());
        let json = serde_json::to_value(&thought).unwrap();
        assert_eq!(json["thinking"], "first check the tests, then the docs");
        assert_eq!(
            serde_json::from_value::<MessageBody>(json).unwrap(),
            thought
        );
        assert_eq!(
            MessageBody::post_with_thinking("done", "   ").thinking(),
            None,
            "blank thinking is none"
        );
        let big = MessageBody::post_with_thinking("done", "x".repeat(MAX_THINKING_BYTES + 1));
        assert!(matches!(
            big.validate(),
            Err(crate::CoreError::ThinkingTooLarge { bytes }) if bytes == MAX_THINKING_BYTES + 1
        ));
        // Two kinds, and only two, on the wire.
        assert!(serde_json::from_str::<MessageBody>(
            r#"{"body":"summary","text":"x","through":"y"}"#
        )
        .is_err());
    }
}
