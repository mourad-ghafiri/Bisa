//! What a session is told about where it is: a chat session in a project
//! (ide/09), and a goal's run's work item about the goal it serves.
//!
//! Pure functions, tested here, because each carries a rule that is easy to
//! get wrong by a word:
//!
//! - [`project_frame`] states the placement. Attached to goals: it names them
//!   and **says no workflow step is judging the session**, so the agent does
//!   not infer a contract somewhere it cannot see. Standalone: it says the project is
//!   attached to no goal and *nothing more about goals* — inventing an absence
//!   to deny would be its own confusion.
//! - [`context_block`] renders the chips a person attached, in chip order,
//!   under one heading. Nothing is injected that is not a chip: if the agent
//!   sees it here, the person saw it above the composer and could remove it.
//! - [`goal_block`] states the goal a goal's run serves, once, in every first
//!   prompt of its work — so no step has to spell `{goal.statement}`, and a
//!   workflow's steps read the same in a run of the workspace, which has no
//!   goal and is told none.

use bisa_core::{ContextRef, ConversationOrigin, DiffScope};

/// The placement sentence(s) for a project thread.
pub fn project_frame(slug: &str, goals: &[String], workstream_branch: Option<&str>) -> String {
    let mut out = String::new();
    match workstream_branch {
        Some(b) => out.push_str(&format!(
            "You are working in project `{slug}`, in the checkout of branch `{b}`."
        )),
        None => out.push_str(&format!(
            "You are working in project `{slug}`, in its own tree."
        )),
    }
    out.push(' ');
    if goals.is_empty() {
        out.push_str("This project is not attached to any goal.");
    } else {
        let list = goals.join(", ");
        out.push_str(&format!(
            "This project is attached to {} goal{}: {list}. You are not running a work item for {}, so no workflow step is judging this session and no result is owed here.",
            goals.len(),
            if goals.len() == 1 { "" } else { "s" },
            if goals.len() == 1 { "it" } else { "any of them" }
        ));
    }
    out
}

/// The placement sentence for a conversation that runs in no checkout: what
/// it is about, said once. `subject` is the goal's title or the workflow's
/// name, when the origin names one. A project's or a workstream's
/// conversation says [`project_frame`] instead.
pub fn origin_frame(origin: &ConversationOrigin, subject: Option<&str>) -> String {
    // "the goal `Dark mode`" when the subject is known, "a goal" when not.
    let named = |what: &str| match subject {
        Some(name) => format!("the {what} `{name}`"),
        None => format!("a {what}"),
    };
    match origin {
        ConversationOrigin::Node => "This conversation is about this machine's node — its \
             harnesses, its setup and its settings, what is true here and nowhere else. It \
             is not about any goal, workflow or project."
            .to_string(),
        ConversationOrigin::Workspace => "This conversation is about the workspace as a \
             whole. It is not attached to any goal, workflow or project."
            .to_string(),
        ConversationOrigin::Goal { .. } => format!(
            "This conversation is about {}. You are not running a work item for \
             it, so no workflow step is judging this session and no result is owed here; \
             get_goal reads its statement, its workflow, its run and its projects.",
            named("goal")
        ),
        ConversationOrigin::Workflow { .. } => format!(
            "This conversation is about {}: its inputs, its steps and its \
             flows. It runs on no goal here and nothing is proposed: read it with \
             get_workflow, validate_workflow until clean, then save_workflow at the \
             revision you read — the person sees the change land on the canvas beside \
             this conversation. A change is written, never assumed. Leave `workflow` \
             out of every call: this workflow is chosen for you.",
            named("workflow")
        ),
        ConversationOrigin::Drawing { .. } => format!(
            "This conversation is about {}, open on the canvas beside the person. Draw with \
             the drawing tools — drawing_read first, then drawing_draw or drawing_mermaid — \
             and leave `drawing` out of every call: this drawing is chosen for you. The person \
             watches each change land; say in a sentence what the picture now shows.",
            named("drawing")
        ),
        ConversationOrigin::Note { .. } => format!(
            "This conversation is about {}, open beside the person in the notes overlay. Read \
             it with note_read before answering, and leave `note` out of every call: this note \
             is chosen for you. Your reply is the conversation's, not the note's — write into \
             the note only when asked to, with note_append, which adds to its end under your \
             name and never changes what they wrote.",
            named("note")
        ),
        ConversationOrigin::Project { .. } | ConversationOrigin::Workstream { .. } => String::new(),
    }
}

/// How far the agent goes on its own in this conversation about a checkout
/// (ide/20), said on every turn: a person changes the mode between turns,
/// and a live session hears of it only here.
pub fn mode_note(mode: bisa_core::ConversationMode) -> &'static str {
    use bisa_core::ConversationMode as M;
    match mode {
        M::Manual => "Mode: manual. Make the changes asked for. Every file you change is shown to the person, who keeps or undoes each part; a command no rule allows is put to them first, so say what a command is for. Do not commit.",
        M::Auto => "Mode: auto. Make the changes asked for and run what you need without asking; the person reviews the changes afterwards. Do not commit.",
        M::Plan => "Mode: plan. Change nothing: an edit is refused. Read the code, ask what you need to know, and reply with the plan — what changes, in which files, in what order, and how it is verified. The person builds it, or asks for another.",
    }
}

fn scope_word(s: &DiffScope) -> String {
    match s {
        DiffScope::Unstaged => "unstaged".into(),
        DiffScope::Staged => "staged".into(),
        DiffScope::Branch { base } => format!("against {base}"),
    }
}
/// The sentence every work item reads about web pages (ide/18) — the one
/// text every session carries, `bisa_core::browser::browser_note`, with the
/// unattended sentence after it when the goal runs with nobody watching.
pub fn browser_note(unattended: bool) -> String {
    format!("\n\n{}", bisa_core::browser::browser_note(unattended))
}

/// The sentence a session reads about mobile development (ide/19) when
/// this machine develops for phones — `bisa_core::mobile_development::mobile_development_note` with
/// the platforms the workspace named — and nothing at all otherwise: a
/// session on a machine with no mobile tools is not told about tools that
/// would refuse it.
pub fn mobile_development_note(
    inner: &crate::Inner,
    project: Option<bisa_core::ProjectId>,
) -> String {
    let access = crate::mobile_development::access(inner, project);
    if !access.enabled {
        return String::new();
    }
    format!(
        "\n\n{}",
        bisa_core::mobile_development::mobile_development_note(access.platforms.words())
    )
}

/// What every first prompt of a goal's run's work reads after the step's
/// own instructions: the goal it serves ([`goal_block`]). `None` when the goal
/// cannot be read — the work goes on without the note rather than not at
/// all. A run of the workspace has no goal, and its callers ask for none.
pub fn goal_note(inner: &crate::Inner, goal: bisa_core::GoalId) -> Option<String> {
    let goal = inner.ws.get_goal(goal).ok()?;
    Some(goal_block(goal.title.as_deref(), &goal.statement))
}

/// The goal as a first prompt reads it: its title when it has one, then its
/// statement, under one heading.
pub fn goal_block(title: Option<&str>, statement: &str) -> String {
    let statement = statement.trim();
    match title.map(str::trim).filter(|t| !t.is_empty()) {
        Some(title) => {
            format!("\n\n---\n## The goal this work serves\n{title}\n\n{statement}")
        }
        None => format!("\n\n---\n## The goal this work serves\n{statement}"),
    }
}

/// The chips as a block, or an empty string when there are none.
pub fn context_block(refs: &[ContextRef]) -> String {
    if refs.is_empty() {
        return String::new();
    }
    let mut out = String::from("--- context the person attached ---\n");
    for r in refs {
        match r {
            ContextRef::Selection { path, range, text } => {
                out.push_str(&format!(
                    "[selection] {}:{}-{}\n```\n{}\n```\n",
                    path.as_str(),
                    range.start,
                    range.end,
                    text.trim_end()
                ));
            }
            ContextRef::File { path } => {
                out.push_str(&format!("[file] {}\n", path.as_str()));
            }
            ContextRef::DiffHunk {
                path,
                scope,
                hunk,
                patch,
            } => {
                out.push_str(&format!(
                    "[diff hunk] {} ({}) #{hunk}\n```diff\n{}\n```\n",
                    path.as_str(),
                    scope_word(scope),
                    patch.trim_end()
                ));
            }
            ContextRef::Terminal { session, tail } => {
                out.push_str(&format!(
                    "[terminal] {session}, last lines\n```\n{}\n```\n",
                    tail.trim_end()
                ));
            }
            ContextRef::WorkItem { id } => {
                out.push_str(&format!("[work item] {id}\n"));
            }
            ContextRef::Commit { id } => {
                out.push_str(&format!("[commit] {id}\n"));
            }
            ContextRef::Annotation {
                page,
                selector,
                excerpt,
                note,
            } => {
                out.push_str(&format!(
                    "[annotation] {} · {selector}\n```html\n{}\n```\n{}\n",
                    page.words(),
                    excerpt.trim_end(),
                    note.trim()
                ));
            }
            ContextRef::Capture {
                device,
                label,
                shot,
                mark,
                note,
            } => {
                let place = match mark {
                    Some(m) => m.words(),
                    None => "the whole screen".to_string(),
                };
                out.push_str(&format!(
                    "[capture] {label} · {device} · {place}\nfile: {} ({})\n{}\n",
                    shot.name,
                    shot.sha256,
                    note.trim()
                ));
            }
        }
    }
    out
}

/// What a **live** session is handed for a follow-up turn: the message's
/// transcript line, then the chips in full.
///
/// A first turn gets the chips through [`context_block`] in its prompt. A
/// follow-up used to get only the transcript's one-line labels — so a file
/// attached to the second message reached the agent as `[context] file
/// src/x.rs` and nothing more, and a selection lost its text entirely. The
/// rule is one rule: the agent sees the bytes of every chip the person saw
/// above the composer, on every turn.
pub fn follow_up_text(line: String, refs: &[ContextRef]) -> String {
    if refs.is_empty() {
        return line;
    }
    format!("{line}\n{}", context_block(refs))
}

/// One line per chip for the transcript, so a reader — and a session handed
/// the transcript — sees what each message carried.
pub fn context_lines(refs: &[ContextRef]) -> Vec<String> {
    refs.iter()
        .map(|r| match r {
            ContextRef::Selection { path, range, .. } => {
                format!(
                    "  [context] selection {}:{}-{}",
                    path.as_str(),
                    range.start,
                    range.end
                )
            }
            ContextRef::File { path } => format!("  [context] file {}", path.as_str()),
            ContextRef::DiffHunk { path, scope, .. } => {
                format!(
                    "  [context] diff hunk {} ({})",
                    path.as_str(),
                    scope_word(scope)
                )
            }
            ContextRef::Terminal { session, .. } => format!("  [context] terminal {session}"),
            ContextRef::WorkItem { id } => format!("  [context] work item {id}"),
            ContextRef::Commit { id } => format!("  [context] commit {id}"),
            ContextRef::Annotation { page, selector, .. } => {
                format!("  [context] annotation {} · {selector}", page.words())
            }
            ContextRef::Capture { label, device, .. } => {
                format!("  [context] capture {label} · {device}")
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bisa_core::{LineRange, RelPath};

    #[test]
    fn an_origin_frame_names_its_subject_once_and_says_a_when_it_has_none() {
        let goal = ConversationOrigin::Goal {
            id: bisa_core::GoalId::from_ulid(ulid::Ulid::nil()),
        };
        assert!(
            origin_frame(&goal, Some("Dark mode"))
                .starts_with("This conversation is about the goal `Dark mode`. "),
            "{}",
            origin_frame(&goal, Some("Dark mode"))
        );
        assert!(origin_frame(&goal, None).starts_with("This conversation is about a goal. "));
        let workflow = ConversationOrigin::Workflow {
            id: bisa_core::WorkflowId::from_ulid(ulid::Ulid::nil()),
        };
        assert!(origin_frame(&workflow, Some("Release"))
            .starts_with("This conversation is about the workflow `Release`: "));
        assert!(
            origin_frame(&workflow, None).starts_with("This conversation is about a workflow: ")
        );
    }

    #[test]
    fn a_goal_block_names_the_title_and_the_statement_under_one_heading() {
        let named = goal_block(Some(" Checkout "), " Ship the new checkout by Friday. ");
        assert_eq!(
            named,
            "\n\n---\n## The goal this work serves\nCheckout\n\nShip the new checkout by Friday."
        );
        let bare = goal_block(Some("  "), "Ship it.");
        assert_eq!(bare, "\n\n---\n## The goal this work serves\nShip it.");
        assert_eq!(
            goal_block(None, "Ship it."),
            bare,
            "a blank title is no title"
        );
    }

    #[test]
    fn a_follow_up_turn_carries_the_chips_bytes_not_only_their_labels() {
        let refs = vec![
            ContextRef::File {
                path: RelPath::new("src/x.rs").unwrap(),
            },
            ContextRef::Selection {
                path: RelPath::new("src/y.rs").unwrap(),
                range: LineRange { start: 3, end: 4 },
                text: "let a = 1;\nlet b = 2;".into(),
            },
        ];
        let text = follow_up_text("The owner: look at these".into(), &refs);
        assert!(text.starts_with("The owner: look at these\n"), "{text}");
        assert!(
            text.contains("--- context the person attached ---"),
            "{text}"
        );
        assert!(text.contains("[file] src/x.rs"), "{text}");
        assert!(
            text.contains("let b = 2;"),
            "the selection's text travels: {text}"
        );
        assert_eq!(
            follow_up_text("The owner: hi".into(), &[]),
            "The owner: hi",
            "no chips, no block"
        );
    }

    #[test]
    fn an_attached_project_names_its_goals_and_the_absence_of_a_step() {
        let f = project_frame("web-app", &["Dark mode".into(), "SSO rollout".into()], None);
        assert!(
            f.contains("attached to 2 goals: Dark mode, SSO rollout"),
            "{f}"
        );
        assert!(f.contains("no workflow step is judging"), "{f}");
        assert!(f.contains("no result is owed"), "{f}");
        assert!(f.contains("not running a work item for any of them"), "{f}");
        let one = project_frame("web-app", &["Dark mode".into()], Some("feature/x"));
        assert!(one.contains("attached to 1 goal: Dark mode"), "{one}");
        assert!(one.contains("checkout of branch `feature/x`"), "{one}");
    }

    #[test]
    fn a_standalone_project_gets_no_goal_vocabulary_beyond_the_one_sentence() {
        let f = project_frame("web-app", &[], None);
        assert!(f.contains("not attached to any goal"), "{f}");
        assert!(!f.contains("workflow step"), "{f}");
        assert!(!f.contains("result is owed"), "{f}");
        assert!(!f.contains("work item"), "{f}");
    }

    #[test]
    fn the_context_block_renders_every_chip_in_order_and_nothing_when_empty() {
        assert_eq!(context_block(&[]), "");
        let refs = vec![
            ContextRef::File {
                path: RelPath::new("src/lib.rs").unwrap(),
            },
            ContextRef::Selection {
                path: RelPath::new("src/main.rs").unwrap(),
                range: LineRange::new(3, 4).unwrap(),
                text: "let x = 1;\nlet y = 2;\n".into(),
            },
            ContextRef::DiffHunk {
                path: RelPath::new("a.txt").unwrap(),
                scope: DiffScope::Staged,
                hunk: "h1".into(),
                patch: "@@ -1 +1 @@\n-a\n+b\n".into(),
            },
            ContextRef::Terminal {
                session: "t3".into(),
                tail: "error: boom\n".into(),
            },
            ContextRef::Annotation {
                page: bisa_core::PageRef::File {
                    path: RelPath::new("www/index.html").unwrap(),
                },
                selector: "#save".into(),
                excerpt: "<button id=\"save\">Save</button>\n".into(),
                note: " make it blue ".into(),
            },
        ];
        let b = context_block(&refs);
        let file = b.find("[file] src/lib.rs").unwrap();
        let sel = b.find("[selection] src/main.rs:3-4").unwrap();
        let hunk = b.find("[diff hunk] a.txt (staged) #h1").unwrap();
        let term = b.find("[terminal] t3").unwrap();
        let ann = b.find("[annotation] www/index.html · #save").unwrap();
        assert!(
            file < sel && sel < hunk && hunk < term && term < ann,
            "chip order is kept"
        );
        assert!(b.starts_with("--- context the person attached ---"));
        assert!(b.contains("```diff\n@@ -1 +1 @@"));
        assert!(
            b.contains("```html\n<button id=\"save\">Save</button>\n```\nmake it blue\n"),
            "the element as it was, then the change wanted: {b}"
        );
        assert_eq!(context_lines(&refs).len(), 5);
        assert_eq!(context_lines(&refs)[0], "  [context] file src/lib.rs");
        assert_eq!(
            context_lines(&refs)[4],
            "  [context] annotation www/index.html · #save"
        );
        // A page the browser showed: the URL names it, in the block and the line.
        let served = vec![ContextRef::Annotation {
            page: bisa_core::PageRef::Url {
                url: "http://localhost:5173/".into(),
            },
            selector: "body > h1".into(),
            excerpt: "<h1>Hi</h1>".into(),
            note: "shorter".into(),
        }];
        assert!(
            context_block(&served).contains("[annotation] http://localhost:5173/ · body > h1\n")
        );
        assert_eq!(
            context_lines(&served)[0],
            "  [context] annotation http://localhost:5173/ · body > h1"
        );
    }

    #[test]
    fn a_capture_names_the_device_the_mark_and_the_file_to_read() {
        let shot = bisa_core::AttachmentRef {
            sha256: "cd".repeat(32),
            name: "mobile-AAAA1-01J.png".into(),
            mime: "image/png".into(),
            size: 99,
        };
        let refs = vec![
            ContextRef::Capture {
                device: "AAAA-1".into(),
                label: "iPhone 16".into(),
                shot: shot.clone(),
                mark: Some(bisa_core::Mark {
                    x: 120,
                    y: 340,
                    width: 200,
                    height: 48,
                }),
                note: " make the button blue ".into(),
            },
            ContextRef::Capture {
                device: "emulator-5554".into(),
                label: "Pixel 8".into(),
                shot,
                mark: None,
                note: "too dark".into(),
            },
        ];
        let b = context_block(&refs);
        assert!(
            b.contains("[capture] iPhone 16 · AAAA-1 · x 120, y 340 · 200×48\nfile: mobile-AAAA1-01J.png ("),
            "{b}"
        );
        assert!(b.contains("\nmake the button blue\n"), "{b}");
        assert!(
            b.contains("[capture] Pixel 8 · emulator-5554 · the whole screen\n"),
            "{b}"
        );
        assert_eq!(
            context_lines(&refs)[0],
            "  [context] capture iPhone 16 · AAAA-1"
        );
    }
}
