//! Reviewing what an agent changed in a checkout (ide/20), proven without an
//! LLM: the tracker is driven the way the conversation pump drives it, the
//! "agent" is the test writing a file between the call being allowed and
//! the tool ending, and everybody else — a person's save, another
//! conversation — writes the same checkout in between.
//!
//! Every checkout is a temporary directory. The one disposal exercised is
//! `Disposal::Unlink`, on a file the test itself made there.

use crate::common;
use bisa_core::{
    AgentId, ChangeKind, ChangeState, ConversationId, ConversationMode, ConversationOrigin,
    FileScope, MessageBody, RelPath, ToolTier,
};
use bisa_engine::changes::asks::{self, AskAnswer, AskScope, AskSubject};
use bisa_engine::changes::settle::{restore, settle, Target, Verdict};
use bisa_engine::changes::tracker::ChangeTracker;
use bisa_engine::changes::{file_view, view, Checkout};
use bisa_engine::ide::files::{write_file, Disposal};
use bisa_engine::{Engine, EngineError};
use bisa_harness::mock::MockAdapter;
use bisa_harness::{InputAnswer, InputRequest};
use bisa_store::{NewConversation, PostOrigin};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

const TEN_LINES: &str = "1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n";

struct Bench {
    _dir: tempfile::TempDir,
    engine: Engine,
    root: PathBuf,
    workstream: bisa_core::WorkstreamId,
    project: bisa_core::ProjectId,
}

fn raw_git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("spawn git");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// An engine over a project whose primary checkout holds `file.txt` — ten
/// numbered lines — as a git repository with one commit when `git`.
fn bench(adapters: Vec<MockAdapter>, git: bool) -> Bench {
    let dir = tempfile::tempdir().unwrap();
    let ws = common::workspace(&dir);
    let project = ws
        .create_project(bisa_store::NewProject::managed("web-app").unwrap())
        .unwrap();
    let workstream = ws.primary_workstream(project.id).unwrap();
    let root = ws.checkout_in(&project, &workstream);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("file.txt"), TEN_LINES).unwrap();
    if git {
        if !root.join(".git").exists() {
            raw_git(&root, &["init", "--quiet"]);
        }
        raw_git(&root, &["config", "user.name", "Bisa Test"]);
        raw_git(&root, &["config", "user.email", "test@example.invalid"]);
        raw_git(&root, &["add", "-A"]);
        raw_git(&root, &["commit", "--quiet", "-m", "first"]);
    }
    for adapter in &adapters {
        let mut core = ws.get_agent(&AgentId::general()).unwrap();
        core.harness = adapter.id.clone();
        ws.update_agent(core).unwrap();
    }
    let engine = Engine::start(
        ws,
        common::catalog_with(adapters),
        common::design_off_config(),
    )
    .unwrap();
    Bench {
        _dir: dir,
        engine,
        root,
        workstream: workstream.id,
        project: project.id,
    }
}

impl Bench {
    fn conversation(&self, mode: ConversationMode) -> ConversationId {
        self.engine
            .workspace()
            .create_conversation(NewConversation {
                origin: ConversationOrigin::Workstream {
                    id: self.workstream,
                    project: self.project,
                },
                title: None,
                mode,
            })
            .unwrap()
            .id
    }

    fn tracker(&self, conversation: ConversationId, guarded: bool) -> ChangeTracker {
        ChangeTracker::new(
            self.engine.inner(),
            Checkout {
                conversation,
                workstream: self.workstream,
                root: self.root.clone(),
            },
            AgentId::general(),
            self.root.clone(),
            guarded,
        )
    }

    fn read(&self, path: &str) -> String {
        std::fs::read_to_string(self.root.join(path)).unwrap()
    }

    /// A person's save, through the editor's own write path.
    fn person_saves(&self, path: &str, text: &str) {
        let inner = self.engine.inner();
        let id = self.workstream.to_string();
        let hash = bisa_store::content_hash(self.read(path).as_bytes());
        write_file(inner, FileScope::Workstream, &id, path, text, Some(&hash)).unwrap();
    }
}

fn edit_of(path: &str) -> InputRequest {
    InputRequest::permission(
        "req-edit",
        "Edit",
        ToolTier::Write,
        path,
        json!({ "file_path": path }),
    )
}

fn command(id: &str, line: &str) -> InputRequest {
    InputRequest::permission(id, "Bash", ToolTier::Exec, line, json!({ "command": line }))
}

/// The agent edits a file: the call is allowed, the file is written, the
/// tool ends.
fn agent_edits(bench: &Bench, tracker: &ChangeTracker, path: &str, text: &str) {
    tracker.allowed(&edit_of(path));
    let at = bench.root.join(path);
    std::fs::create_dir_all(at.parent().unwrap()).unwrap();
    std::fs::write(at, text).unwrap();
    tracker.tool_ended(bench.engine.inner(), "Edit").unwrap();
}

fn rel(path: &str) -> RelPath {
    RelPath::new(path).unwrap()
}

fn line(n: usize, with: &str) -> String {
    TEN_LINES.replace(&format!("\n{n}\n"), &format!("\n{with}\n"))
}

#[tokio::test]
async fn a_file_edit_is_attributed_to_its_turn_and_waits_for_a_word() {
    let bench = bench(vec![], false);
    let inner = bench.engine.inner();
    let c = bench.conversation(ConversationMode::Manual);
    let tracker = bench.tracker(c, true);
    let turn = tracker
        .begin_turn(inner, Some("msg-1".into()), ConversationMode::Manual)
        .unwrap();
    agent_edits(&bench, &tracker, "file.txt", &line(2, "two"));
    tracker.end_turn(inner, Some("reply-1".into())).unwrap();

    let seen = view(inner, c).unwrap();
    assert_eq!((seen.pending, seen.owed), (1, true));
    assert_eq!(seen.turns.len(), 1);
    let card = &seen.turns[0];
    assert_eq!(card.turn, turn);
    assert_eq!(
        (card.prompt.as_deref(), card.reply.as_deref()),
        (Some("msg-1"), Some("reply-1"))
    );
    assert_eq!(card.files[0].path, "file.txt");
    assert_eq!(
        (card.files[0].kind, card.files[0].state),
        (ChangeKind::Modified, ChangeState::Pending)
    );
    assert_eq!((card.files[0].added, card.files[0].removed), (1, 1));

    let file = file_view(inner, c, &rel("file.txt")).unwrap().unwrap();
    assert_eq!(file.base_text, TEN_LINES);
    assert_eq!(file.hunks.len(), 1);
    assert_eq!(file.hunks[0].added, "two\n");

    // Kept: the file stays as the agent left it and nothing waits.
    let kept = settle(
        inner,
        c,
        Verdict::Keep,
        Target::All,
        Disposal::Unlink,
        false,
    )
    .unwrap();
    assert_eq!((kept.files, kept.pending), (1, 0));
    assert_eq!(bench.read("file.txt"), line(2, "two"));
    assert_eq!(
        view(inner, c).unwrap().turns[0].files[0].state,
        ChangeState::Kept
    );
    bench.engine.shutdown().await;
}

#[tokio::test]
async fn a_turn_that_changed_nothing_leaves_no_card() {
    let bench = bench(vec![], false);
    let inner = bench.engine.inner();
    let c = bench.conversation(ConversationMode::Manual);
    let tracker = bench.tracker(c, true);
    tracker
        .begin_turn(inner, None, ConversationMode::Manual)
        .unwrap();
    // Allowed, and the tool wrote back the very same bytes.
    agent_edits(&bench, &tracker, "file.txt", TEN_LINES);
    tracker.end_turn(inner, None).unwrap();
    // A second end — a `TurnEnded`, then an `Ended` — closes nothing twice.
    tracker.end_turn(inner, None).unwrap();
    let seen = view(inner, c).unwrap();
    assert_eq!((seen.pending, seen.turns.len()), (0, 0));
    bench.engine.shutdown().await;
}

#[tokio::test]
async fn somebody_elses_edit_elsewhere_in_the_file_survives_an_undo() {
    let bench = bench(vec![], false);
    let inner = bench.engine.inner();
    let c = bench.conversation(ConversationMode::Manual);
    let tracker = bench.tracker(c, true);
    tracker
        .begin_turn(inner, None, ConversationMode::Manual)
        .unwrap();
    agent_edits(&bench, &tracker, "file.txt", &line(2, "two"));
    tracker.end_turn(inner, None).unwrap();

    // A person edits line 9 of the same file while the change waits.
    bench.person_saves("file.txt", &line(2, "two").replace("\n9\n", "\nnine\n"));

    let file = file_view(inner, c, &rel("file.txt")).unwrap().unwrap();
    assert_eq!(file.hunks.len(), 1, "only the agent's change is pending");
    assert!(!file.overlapped);
    assert!(
        file.base_text.contains("\nnine\n"),
        "their edit is in the base now"
    );

    let undone = settle(
        inner,
        c,
        Verdict::Undo,
        Target::Hunk {
            path: rel("file.txt"),
            hunk: file.hunks[0].id.clone(),
            disk_hash: file.disk_hash.clone(),
        },
        Disposal::Unlink,
        false,
    )
    .unwrap();
    assert_eq!((undone.files, undone.pending), (1, 0));
    assert_eq!(
        bench.read("file.txt"),
        TEN_LINES.replace("\n9\n", "\nnine\n")
    );
    assert_eq!(
        view(inner, c).unwrap().turns[0].files[0].state,
        ChangeState::Undone
    );
    bench.engine.shutdown().await;
}

#[tokio::test]
async fn an_edit_on_the_agents_own_lines_is_overlapped_and_an_undo_asks_first() {
    let bench = bench(vec![], false);
    let inner = bench.engine.inner();
    let c = bench.conversation(ConversationMode::Manual);
    let tracker = bench.tracker(c, true);
    tracker
        .begin_turn(inner, None, ConversationMode::Manual)
        .unwrap();
    agent_edits(&bench, &tracker, "file.txt", &line(2, "two"));
    tracker.end_turn(inner, None).unwrap();
    bench.person_saves("file.txt", &line(2, "two, as I would say it"));

    let seen = view(inner, c).unwrap();
    assert!(seen.turns[0].files[0].overlapped);
    let asked = settle(
        inner,
        c,
        Verdict::Undo,
        Target::File {
            path: rel("file.txt"),
        },
        Disposal::Unlink,
        false,
    )
    .unwrap();
    assert_eq!((asked.files, asked.pending), (0, 1));
    assert_eq!(asked.skipped[0].path, "file.txt");
    assert_eq!(bench.read("file.txt"), line(2, "two, as I would say it"));

    // The person said yes: the base comes back, their edit with it.
    let forced = settle(
        inner,
        c,
        Verdict::Undo,
        Target::File {
            path: rel("file.txt"),
        },
        Disposal::Unlink,
        true,
    )
    .unwrap();
    assert_eq!((forced.files, forced.pending), (1, 0));
    assert_eq!(bench.read("file.txt"), TEN_LINES);
    bench.engine.shutdown().await;
}

#[tokio::test]
async fn a_hunk_read_from_a_file_that_moved_since_is_refused() {
    let bench = bench(vec![], false);
    let inner = bench.engine.inner();
    let c = bench.conversation(ConversationMode::Manual);
    let tracker = bench.tracker(c, true);
    tracker
        .begin_turn(inner, None, ConversationMode::Manual)
        .unwrap();
    agent_edits(&bench, &tracker, "file.txt", &line(2, "two"));
    tracker.end_turn(inner, None).unwrap();
    let file = file_view(inner, c, &rel("file.txt")).unwrap().unwrap();

    let refused = settle(
        inner,
        c,
        Verdict::Undo,
        Target::Hunk {
            path: rel("file.txt"),
            hunk: file.hunks[0].id.clone(),
            disk_hash: "0".repeat(64),
        },
        Disposal::Unlink,
        false,
    );
    assert!(matches!(refused, Err(EngineError::FileConflict { .. })));
    assert_eq!(
        bench.read("file.txt"),
        line(2, "two"),
        "nothing was written"
    );
    bench.engine.shutdown().await;
}

#[tokio::test]
async fn a_file_the_agent_made_is_unmade_by_an_undo() {
    let bench = bench(vec![], false);
    let inner = bench.engine.inner();
    let c = bench.conversation(ConversationMode::Manual);
    let tracker = bench.tracker(c, true);
    tracker
        .begin_turn(inner, None, ConversationMode::Manual)
        .unwrap();
    agent_edits(&bench, &tracker, "src/new.rs", "fn main() {}\n");
    tracker.end_turn(inner, None).unwrap();
    assert_eq!(
        view(inner, c).unwrap().turns[0].files[0].kind,
        ChangeKind::Created
    );

    settle(
        inner,
        c,
        Verdict::Undo,
        Target::File {
            path: rel("src/new.rs"),
        },
        Disposal::Unlink,
        false,
    )
    .unwrap();
    assert!(!bench.root.join("src/new.rs").exists());
    bench.engine.shutdown().await;
}

#[tokio::test]
async fn a_command_is_swept_from_a_snapshot_and_what_moved_before_it_is_not_its() {
    let bench = bench(vec![], true);
    let inner = bench.engine.inner();
    let c = bench.conversation(ConversationMode::Auto);
    let tracker = bench.tracker(c, true);
    tracker
        .begin_turn(inner, None, ConversationMode::Auto)
        .unwrap();

    // Somebody else writes a file before the command's window opens.
    std::fs::write(bench.root.join("theirs.txt"), "not the agent's\n").unwrap();

    tracker.allowed(&command(
        "req-1",
        "sed -i s/2/two/ file.txt && touch made.txt",
    ));
    std::fs::write(bench.root.join("file.txt"), line(2, "two")).unwrap();
    std::fs::write(bench.root.join("made.txt"), "made by the command\n").unwrap();
    tracker.tool_ended(inner, "Bash").unwrap();
    tracker.end_turn(inner, None).unwrap();

    let seen = view(inner, c).unwrap();
    let mut paths: Vec<&str> = seen.turns[0]
        .files
        .iter()
        .map(|f| f.path.as_str())
        .collect();
    paths.sort();
    assert_eq!(paths, vec!["file.txt", "made.txt"]);
    assert!(!seen.owed, "auto never owes a review");
    let file = file_view(inner, c, &rel("file.txt")).unwrap().unwrap();
    assert_eq!(file.base_text, TEN_LINES, "read out of the snapshot");
    bench.engine.shutdown().await;
}

#[tokio::test]
async fn a_harness_the_guard_cannot_stop_is_one_window_per_turn() {
    let bench = bench(vec![], true);
    let inner = bench.engine.inner();
    let c = bench.conversation(ConversationMode::Manual);
    let tracker = bench.tracker(c, false);
    tracker
        .begin_turn(inner, None, ConversationMode::Manual)
        .unwrap();
    // No call is raised: the harness just writes.
    std::fs::write(bench.root.join("file.txt"), line(5, "five")).unwrap();
    tracker.end_turn(inner, None).unwrap();
    let seen = view(inner, c).unwrap();
    assert_eq!(seen.pending, 1);
    assert_eq!(seen.turns[0].files[0].path, "file.txt");
    bench.engine.shutdown().await;
}

#[tokio::test]
async fn two_turns_on_one_file_are_undone_apart_and_a_restore_goes_back_before_both() {
    let bench = bench(vec![], false);
    let inner = bench.engine.inner();
    let c = bench.conversation(ConversationMode::Manual);
    let tracker = bench.tracker(c, true);
    let first = tracker
        .begin_turn(inner, None, ConversationMode::Manual)
        .unwrap();
    agent_edits(&bench, &tracker, "file.txt", &line(2, "two"));
    tracker.end_turn(inner, None).unwrap();
    let second = tracker
        .begin_turn(inner, None, ConversationMode::Manual)
        .unwrap();
    let both = line(2, "two").replace("\n8\n", "\neight\n");
    agent_edits(&bench, &tracker, "file.txt", &both);
    tracker.end_turn(inner, None).unwrap();

    // The first turn alone is taken back out from under the second.
    let undone = settle(
        inner,
        c,
        Verdict::Undo,
        Target::Turn { turn: first },
        Disposal::Unlink,
        false,
    )
    .unwrap();
    assert_eq!((undone.files, undone.pending), (1, 1));
    assert_eq!(
        bench.read("file.txt"),
        TEN_LINES.replace("\n8\n", "\neight\n")
    );
    let cards = view(inner, c).unwrap().turns;
    assert_eq!(cards[0].files[0].state, ChangeState::Undone);
    assert_eq!(cards[1].files[0].state, ChangeState::Pending);

    // A person edits line 5, then goes back to before the second message.
    bench.person_saves(
        "file.txt",
        &TEN_LINES
            .replace("\n8\n", "\neight\n")
            .replace("\n5\n", "\nfive\n"),
    );
    let restored = restore(inner, c, second, Disposal::Unlink).unwrap();
    assert_eq!((restored.files, restored.pending), (1, 0));
    assert_eq!(
        bench.read("file.txt"),
        TEN_LINES.replace("\n5\n", "\nfive\n")
    );
    bench.engine.shutdown().await;
}

#[tokio::test]
async fn a_restore_leaves_a_file_alone_when_it_would_lose_somebody_elses_edit() {
    let bench = bench(vec![], false);
    let inner = bench.engine.inner();
    let c = bench.conversation(ConversationMode::Manual);
    let tracker = bench.tracker(c, true);
    let turn = tracker
        .begin_turn(inner, None, ConversationMode::Manual)
        .unwrap();
    agent_edits(&bench, &tracker, "file.txt", &line(2, "two"));
    tracker.end_turn(inner, None).unwrap();
    settle(
        inner,
        c,
        Verdict::Keep,
        Target::All,
        Disposal::Unlink,
        false,
    )
    .unwrap();
    bench.person_saves("file.txt", &line(2, "two!"));

    let restored = restore(inner, c, turn, Disposal::Unlink).unwrap();
    assert_eq!(restored.files, 0);
    assert_eq!(restored.skipped[0].path, "file.txt");
    assert_eq!(bench.read("file.txt"), line(2, "two!"));
    bench.engine.shutdown().await;
}

#[tokio::test]
async fn auto_keeps_what_earlier_turns_left_when_the_next_message_begins_a_turn() {
    let bench = bench(vec![], false);
    let inner = bench.engine.inner();
    let c = bench.conversation(ConversationMode::Auto);
    let tracker = bench.tracker(c, true);
    let first = tracker
        .begin_turn(inner, None, ConversationMode::Auto)
        .unwrap();
    agent_edits(&bench, &tracker, "file.txt", &line(2, "two"));
    tracker.end_turn(inner, None).unwrap();
    assert_eq!(view(inner, c).unwrap().pending, 1);

    tracker
        .begin_turn(inner, None, ConversationMode::Auto)
        .unwrap();
    let seen = view(inner, c).unwrap();
    assert_eq!(seen.pending, 0);
    assert_eq!(seen.turns[0].files[0].state, ChangeState::Kept);
    // Kept is not gone: the message can still be gone back to before.
    tracker.end_turn(inner, None).unwrap();
    restore(inner, c, first, Disposal::Unlink).unwrap();
    assert_eq!(bench.read("file.txt"), TEN_LINES);
    bench.engine.shutdown().await;
}

#[tokio::test]
async fn two_conversations_on_one_checkout_are_each_others_outside_writers() {
    let bench = bench(vec![], false);
    let inner = bench.engine.inner();
    let (a, b) = (
        bench.conversation(ConversationMode::Manual),
        bench.conversation(ConversationMode::Manual),
    );
    let (ta, tb) = (bench.tracker(a, true), bench.tracker(b, true));
    ta.begin_turn(inner, None, ConversationMode::Manual)
        .unwrap();
    agent_edits(&bench, &ta, "file.txt", &line(2, "two"));
    ta.end_turn(inner, None).unwrap();
    tb.begin_turn(inner, None, ConversationMode::Manual)
        .unwrap();
    let both = line(2, "two").replace("\n8\n", "\neight\n");
    agent_edits(&bench, &tb, "file.txt", &both);
    tb.end_turn(inner, None).unwrap();

    let of_a = file_view(inner, a, &rel("file.txt")).unwrap().unwrap();
    let of_b = file_view(inner, b, &rel("file.txt")).unwrap().unwrap();
    assert_eq!(of_a.hunks.len(), 1);
    assert_eq!(of_a.hunks[0].added, "two\n");
    assert_eq!(of_b.hunks.len(), 1);
    assert_eq!(of_b.hunks[0].added, "eight\n");

    // Undoing one conversation's change leaves the other's standing.
    settle(
        inner,
        a,
        Verdict::Undo,
        Target::All,
        Disposal::Unlink,
        false,
    )
    .unwrap();
    assert_eq!(
        bench.read("file.txt"),
        TEN_LINES.replace("\n8\n", "\neight\n")
    );
    assert_eq!(view(inner, b).unwrap().pending, 1);
    bench.engine.shutdown().await;
}

#[tokio::test]
async fn a_conversation_about_anything_but_a_checkout_has_no_changes_and_no_mode() {
    let bench = bench(vec![], false);
    let inner = bench.engine.inner();
    let elsewhere = bench
        .engine
        .workspace()
        .create_conversation(NewConversation {
            origin: ConversationOrigin::Workspace,
            title: None,
            mode: ConversationMode::Manual,
        })
        .unwrap();
    assert!(matches!(
        view(inner, elsewhere.id),
        Err(EngineError::Invalid(_))
    ));
    assert!(matches!(
        bisa_engine::conversations::set_mode(inner, elsewhere.id, ConversationMode::Auto),
        Err(EngineError::Invalid(_))
    ));
    bench.engine.shutdown().await;
}

// ---------------------------------------------------------------------------
// Through a session: the modes and the ask in the conversation
// ---------------------------------------------------------------------------

fn guarded(id: &str, request: InputRequest) -> MockAdapter {
    let base = MockAdapter::default();
    MockAdapter {
        id: id.into(),
        caps: base.caps | bisa_core::HarnessCaps::TOOL_GUARD,
        input_request: Some(request),
        ..base
    }
}

fn say(bench: &Bench, c: ConversationId, what: &str) {
    bench
        .engine
        .workspace()
        .post_message(
            &c.to_string(),
            MessageBody::post(what),
            None,
            &[],
            &[],
            None,
            PostOrigin::Asked,
        )
        .unwrap();
}

#[tokio::test]
async fn manual_asks_a_command_in_the_conversation_and_the_person_answers_there() {
    let adapter = guarded("asking", command("req-1", "cargo test"));
    let answered = Arc::clone(&adapter.answered);
    let bench = bench(vec![adapter], false);
    let inner = bench.engine.inner();
    let c = bench.conversation(ConversationMode::Manual);
    say(&bench, c, "run the tests");

    let ask = common::until("the command is asked in the conversation", || {
        asks::open(inner, c).into_iter().next()
    })
    .await;
    assert_eq!(
        ask.subject,
        AskSubject::Tool {
            tool: "Bash".into(),
            tier: ToolTier::Exec
        }
    );
    assert!(
        ask.grantable,
        "the mode's own ask may be allowed for the conversation"
    );
    assert!(answered.lock().unwrap().is_empty(), "nothing ran yet");

    asks::answer(
        inner,
        c,
        &ask.id,
        AskAnswer::Allow {
            scope: AskScope::Conversation,
        },
    )
    .unwrap();
    common::until("the harness hears the allow", || {
        answered
            .lock()
            .unwrap()
            .iter()
            .any(|(_, a)| matches!(a, InputAnswer::Allow { .. }))
            .then_some(())
    })
    .await;
    assert!(asks::open(inner, c).is_empty());
    // Answered twice is an error in a sentence, not a second run.
    assert!(asks::answer(inner, c, &ask.id, AskAnswer::Deny { note: None }).is_err());
    bench.engine.shutdown().await;
}

#[tokio::test]
async fn an_answer_nobody_is_left_to_hear_is_refused_in_a_sentence() {
    let bench = bench(vec![guarded("idle", command("req-1", "true"))], false);
    let inner = Arc::clone(bench.engine.inner());
    let c = bench.conversation(ConversationMode::Manual);

    // A turn asks, and is stopped while the person reads the question.
    let asking = {
        let inner = Arc::clone(&inner);
        tokio::spawn(async move {
            asks::ask(
                &inner,
                c,
                "coder",
                AskSubject::Tool {
                    tool: "Bash".into(),
                    tier: ToolTier::Exec,
                },
                "Run `cargo test`?".into(),
                true,
            )
            .await
        })
    };
    let ask = common::until("the question is open", || {
        asks::open(&inner, c).into_iter().next()
    })
    .await;
    asking.abort();
    assert!(asking.await.is_err_and(|e| e.is_cancelled()));

    let refused = asks::answer(
        &inner,
        c,
        &ask.id,
        AskAnswer::Allow {
            scope: AskScope::Once,
        },
    )
    .unwrap_err();
    assert!(
        refused.to_string().contains("its turn ended"),
        "an allow that reaches nobody is not reported as taken: {refused}"
    );
    assert!(
        asks::open(&inner, c).is_empty(),
        "and the question does not stay on screen"
    );
    // A name that was never asked reads the same as one already answered.
    assert!(asks::answer(&inner, c, "01J0NOSUCHASK", AskAnswer::Deny { note: None }).is_err());
    bench.engine.shutdown().await;
}

#[tokio::test]
async fn a_deny_with_a_note_is_what_the_agent_hears() {
    let adapter = guarded("noted", command("req-1", "npm publish --dry-run"));
    let answered = Arc::clone(&adapter.answered);
    let bench = bench(vec![adapter], false);
    let inner = bench.engine.inner();
    let c = bench.conversation(ConversationMode::Manual);
    say(&bench, c, "check the package");
    let ask = common::until("the command is asked", || {
        asks::open(inner, c).into_iter().next()
    })
    .await;
    asks::answer(
        inner,
        c,
        &ask.id,
        AskAnswer::Deny {
            note: Some("use the lockfile check instead".into()),
        },
    )
    .unwrap();
    // The mock asks the same thing in every session it runs, and one that is
    // about no conversation is refused at once; the refusal under test is the
    // one the person wrote.
    let reason = common::until("the harness hears the person's refusal", || {
        answered.lock().unwrap().iter().find_map(|(_, a)| match a {
            InputAnswer::Deny { reason } if reason.contains("lockfile") => Some(reason.clone()),
            _ => None,
        })
    })
    .await;
    assert!(
        reason.contains("use the lockfile check instead"),
        "{reason}"
    );
    bench.engine.shutdown().await;
}

#[tokio::test]
async fn plan_refuses_an_edit_outright_and_auto_lets_it_land() {
    for (mode, allowed) in [
        (ConversationMode::Plan, false),
        (ConversationMode::Auto, true),
        (ConversationMode::Manual, true),
    ] {
        let adapter = guarded("editing", edit_of("file.txt"));
        let answered = Arc::clone(&adapter.answered);
        let bench = bench(vec![adapter], false);
        let c = bench.conversation(mode);
        say(&bench, c, "rename the function");
        let answer = common::until("the edit is answered", || {
            answered.lock().unwrap().first().map(|(_, a)| a.clone())
        })
        .await;
        match answer {
            InputAnswer::Allow { .. } => assert!(allowed, "{mode:?} allowed an edit"),
            InputAnswer::Deny { reason } => {
                assert!(!allowed, "{mode:?} refused an edit: {reason}");
                assert_eq!(reason, bisa_engine::inputs::PLAN_REFUSAL);
            }
            other => panic!("{other:?}"),
        }
        assert!(
            asks::open(bench.engine.inner(), c).is_empty(),
            "an edit is never a question"
        );
        bench.engine.shutdown().await;
    }
}

#[tokio::test]
async fn a_rule_that_refuses_still_refuses_in_auto() {
    let adapter = guarded("reckless", command("req-1", "sudo ls"));
    let answered = Arc::clone(&adapter.answered);
    let bench = bench(vec![adapter], false);
    let c = bench.conversation(ConversationMode::Auto);
    say(&bench, c, "clean up");
    // The refusal is the rule's own — never the mode's, and never the one a
    // session about nothing gets for having nobody to ask.
    let reason = common::until("the rule refuses the command", || {
        answered.lock().unwrap().iter().find_map(|(_, a)| match a {
            InputAnswer::Deny { reason } if reason.contains("sudo, doas, su") => {
                Some(reason.clone())
            }
            _ => None,
        })
    })
    .await;
    assert!(!reason.contains("outside any goal"), "{reason}");
    assert!(
        asks::open(bench.engine.inner(), c).is_empty(),
        "a rule that refuses asks nobody, in any mode"
    );
    bench.engine.shutdown().await;
}

#[tokio::test]
async fn a_plan_on_a_harness_the_guard_cannot_stop_is_refused_in_words() {
    let adapter = MockAdapter {
        id: "unguarded".into(),
        ..Default::default()
    };
    let launches = Arc::clone(&adapter.launches);
    let bench = bench(vec![adapter], false);
    let c = bench.conversation(ConversationMode::Plan);
    say(&bench, c, "plan the migration");
    let said = common::until("the agent says why it will not plan", || {
        bench
            .engine
            .workspace()
            .messages(&c.to_string(), None, 10)
            .unwrap()
            .into_iter()
            .map(|m| m.content)
            .find(|m| m.contains("plan mode"))
    })
    .await;
    assert!(said.contains("unguarded"), "{said}");
    assert!(
        launches.lock().unwrap().is_empty(),
        "no session was launched"
    );
    bench.engine.shutdown().await;
}

// ---------------------------------------------------------------------------
// The edges of a word: said twice, said about nothing, said about bytes.
// ---------------------------------------------------------------------------

/// One turn that edits `file.txt` on line `n`, ended.
fn one_edit(bench: &Bench, c: ConversationId, n: usize, with: &str) -> bisa_core::TurnId {
    let inner = bench.engine.inner();
    let tracker = bench.tracker(c, true);
    let turn = tracker
        .begin_turn(inner, None, ConversationMode::Manual)
        .unwrap();
    let text = bench
        .read("file.txt")
        .replace(&format!("\n{n}\n"), &format!("\n{with}\n"));
    agent_edits(bench, &tracker, "file.txt", &text);
    tracker.end_turn(inner, None).unwrap();
    turn
}

fn sentence<T>(result: Result<T, EngineError>) -> String {
    match result {
        Err(EngineError::Invalid(said)) => said.to_string(),
        Err(other) => panic!("expected a sentence, got {other}"),
        Ok(_) => panic!("expected a refusal"),
    }
}

/// How many turns stay to go back to is a setting, read when a turn begins:
/// past it the oldest turn that owes nothing is forgotten, and a restore to
/// before it is refused in a sentence — while a turn with a change still
/// waiting is kept however old it is.
#[tokio::test]
async fn the_turns_to_go_back_to_are_as_many_as_the_setting_says() {
    let bench = bench(vec![], false);
    let inner = bench.engine.inner();
    bench
        .engine
        .set_setting(
            bisa_core::SettingScope::Workspace,
            None,
            "agents.review.checkpoints",
            json!(2),
        )
        .unwrap();
    let c = bench.conversation(ConversationMode::Manual);
    let turns_of = || -> Vec<bisa_core::TurnId> {
        view(inner, c)
            .unwrap()
            .turns
            .iter()
            .map(|t| t.turn)
            .collect()
    };
    let keep_all = || {
        settle(
            inner,
            c,
            Verdict::Keep,
            Target::All,
            Disposal::Unlink,
            false,
        )
        .unwrap()
    };

    // The first turn's change is left waiting; the three after it are kept.
    let waiting = one_edit(&bench, c, 2, "two");
    let mut kept = Vec::new();
    for (n, with) in [(4, "four"), (6, "six"), (8, "eight")] {
        let turn = one_edit(&bench, c, n, with);
        settle(
            inner,
            c,
            Verdict::Keep,
            Target::Turn { turn },
            Disposal::Unlink,
            false,
        )
        .unwrap();
        kept.push(turn);
    }
    let left = turns_of();
    assert!(
        left.contains(&waiting),
        "a turn that is owed a word is never forgotten: {left:?}"
    );
    assert!(
        !left.contains(&kept[0]),
        "the oldest settled turn went: {left:?}"
    );
    assert!(left.contains(&kept[2]), "the newest stays: {left:?}");

    let gone = sentence(restore(inner, c, kept[0], Disposal::Unlink));
    assert!(!gone.is_empty(), "a turn nobody kept is a refusal in words");
    assert_eq!(
        bench.read("file.txt"),
        TEN_LINES
            .replace("\n2\n", "\ntwo\n")
            .replace("\n4\n", "\nfour\n")
            .replace("\n6\n", "\nsix\n")
            .replace("\n8\n", "\neight\n"),
        "and the refusal wrote nothing"
    );
    keep_all();
    bench.engine.shutdown().await;
}

#[tokio::test]
async fn a_word_said_twice_is_said_once() {
    let bench = bench(vec![], false);
    let inner = bench.engine.inner();
    let c = bench.conversation(ConversationMode::Manual);
    one_edit(&bench, c, 2, "two");
    let file = file_view(inner, c, &rel("file.txt")).unwrap().unwrap();
    let hunk = Target::Hunk {
        path: rel("file.txt"),
        hunk: file.hunks[0].id.clone(),
        disk_hash: file.disk_hash.clone(),
    };

    let kept = settle(
        inner,
        c,
        Verdict::Keep,
        hunk.clone(),
        Disposal::Unlink,
        false,
    )
    .unwrap();
    assert_eq!(kept.pending, 0, "the file's only hunk was kept");
    // The same hunk undone afterwards: nothing of the file waits any more.
    let late = sentence(settle(
        inner,
        c,
        Verdict::Undo,
        hunk,
        Disposal::Unlink,
        false,
    ));
    assert!(late.contains("nothing of file.txt is waiting"), "{late}");
    assert_eq!(bench.read("file.txt"), line(2, "two"), "a kept hunk stays");

    // A whole-file Undo after the Keep reaches nothing and harms nothing.
    let again = settle(
        inner,
        c,
        Verdict::Undo,
        Target::All,
        Disposal::Unlink,
        false,
    )
    .unwrap();
    assert_eq!((again.files, again.pending), (0, 0));
    assert_eq!(bench.read("file.txt"), line(2, "two"));
    bench.engine.shutdown().await;
}

#[tokio::test]
async fn an_undo_said_twice_undoes_once() {
    let bench = bench(vec![], false);
    let inner = bench.engine.inner();
    let c = bench.conversation(ConversationMode::Manual);
    let turn = one_edit(&bench, c, 2, "two");

    let first = settle(
        inner,
        c,
        Verdict::Undo,
        Target::Turn { turn },
        Disposal::Unlink,
        false,
    )
    .unwrap();
    assert_eq!((first.files, first.pending), (1, 0));
    assert_eq!(bench.read("file.txt"), TEN_LINES);

    // A person writes the file afterwards; the second Undo must not reach it.
    bench.person_saves("file.txt", &line(9, "nine"));
    let second = settle(
        inner,
        c,
        Verdict::Undo,
        Target::Turn { turn },
        Disposal::Unlink,
        false,
    )
    .unwrap();
    assert_eq!((second.files, second.pending), (0, 0));
    assert_eq!(bench.read("file.txt"), line(9, "nine"));
    bench.engine.shutdown().await;
}

#[tokio::test]
async fn a_word_about_nothing_is_a_sentence_and_writes_nothing() {
    let bench = bench(vec![], false);
    let inner = bench.engine.inner();
    let c = bench.conversation(ConversationMode::Manual);
    one_edit(&bench, c, 2, "two");
    let file = file_view(inner, c, &rel("file.txt")).unwrap().unwrap();

    // A hunk the file does not hold, read from the disk as it stands.
    let unknown = settle(
        inner,
        c,
        Verdict::Undo,
        Target::Hunk {
            path: rel("file.txt"),
            hunk: "no-such-hunk".into(),
            disk_hash: file.disk_hash.clone(),
        },
        Disposal::Unlink,
        false,
    );
    assert!(
        matches!(unknown, Err(EngineError::FileConflict { .. })),
        "a hunk that is not there reads as a file that moved: the lens reloads"
    );
    // A turn that never was.
    let ghost =
        bisa_core::TurnId::from_ulid(ulid::Ulid::from_datetime(std::time::SystemTime::now()));
    let said = sentence(settle(
        inner,
        c,
        Verdict::Keep,
        Target::Turn { turn: ghost },
        Disposal::Unlink,
        false,
    ));
    assert!(said.contains("no such turn"), "{said}");
    let said = sentence(restore(inner, c, ghost, Disposal::Unlink));
    assert!(said.contains("no such turn"), "{said}");
    // A file nobody touched.
    let said = sentence(settle(
        inner,
        c,
        Verdict::Undo,
        Target::Hunk {
            path: rel("other.txt"),
            hunk: "h".into(),
            disk_hash: String::new(),
        },
        Disposal::Unlink,
        false,
    ));
    assert!(said.contains("nothing of other.txt"), "{said}");

    assert_eq!(bench.read("file.txt"), line(2, "two"));
    assert_eq!(
        view(inner, c).unwrap().pending,
        1,
        "and the change still waits"
    );
    bench.engine.shutdown().await;
}

#[tokio::test]
async fn bytes_are_kept_or_undone_whole_and_never_by_the_hunk() {
    let bench = bench(vec![], false);
    let inner = bench.engine.inner();
    let c = bench.conversation(ConversationMode::Manual);
    let before: Vec<u8> = vec![0x89, b'P', b'N', b'G', 0, 1, 2, 3];
    std::fs::write(bench.root.join("logo.png"), &before).unwrap();

    let tracker = bench.tracker(c, true);
    tracker
        .begin_turn(inner, None, ConversationMode::Manual)
        .unwrap();
    tracker.allowed(&edit_of("logo.png"));
    std::fs::write(bench.root.join("logo.png"), [0u8, 9, 9, 9, 0xff]).unwrap();
    tracker.tool_ended(inner, "Edit").unwrap();
    tracker.end_turn(inner, None).unwrap();

    let card = &view(inner, c).unwrap().turns[0].files[0];
    assert!(card.opaque);
    assert_eq!((card.added, card.removed), (0, 0), "bytes have no lines");
    let lens = file_view(inner, c, &rel("logo.png")).unwrap().unwrap();
    assert!(lens.opaque && lens.hunks.is_empty());

    let said = sentence(settle(
        inner,
        c,
        Verdict::Undo,
        Target::Hunk {
            path: rel("logo.png"),
            hunk: "h".into(),
            disk_hash: lens.disk_hash,
        },
        Disposal::Unlink,
        false,
    ));
    assert!(said.contains("is not text"), "{said}");

    let undone = settle(
        inner,
        c,
        Verdict::Undo,
        Target::File {
            path: rel("logo.png"),
        },
        Disposal::Unlink,
        false,
    )
    .unwrap();
    assert_eq!((undone.files, undone.pending), (1, 0));
    assert_eq!(std::fs::read(bench.root.join("logo.png")).unwrap(), before);
    bench.engine.shutdown().await;
}

#[tokio::test]
async fn a_file_the_agent_removed_comes_back_with_an_undo() {
    let bench = bench(vec![], false);
    let inner = bench.engine.inner();
    let c = bench.conversation(ConversationMode::Manual);
    let tracker = bench.tracker(c, true);
    tracker
        .begin_turn(inner, None, ConversationMode::Manual)
        .unwrap();
    tracker.allowed(&edit_of("file.txt"));
    std::fs::remove_file(bench.root.join("file.txt")).unwrap();
    tracker.tool_ended(inner, "Edit").unwrap();
    tracker.end_turn(inner, None).unwrap();

    let card = &view(inner, c).unwrap().turns[0].files[0];
    assert_eq!((card.kind, card.removed), (ChangeKind::Removed, 10));

    settle(
        inner,
        c,
        Verdict::Undo,
        Target::All,
        Disposal::Unlink,
        false,
    )
    .unwrap();
    assert_eq!(bench.read("file.txt"), TEN_LINES);
    bench.engine.shutdown().await;
}

#[tokio::test]
async fn what_cannot_be_reviewed_is_not_tracked_and_never_written_back() {
    let bench = bench(vec![], false);
    let inner = bench.engine.inner();
    let c = bench.conversation(ConversationMode::Manual);
    // A file past the review's bound — sparse, so it costs no disk — a path
    // outside the checkout, and a link that leaves it.
    let huge = std::fs::File::create(bench.root.join("huge.bin")).unwrap();
    huge.set_len(bisa_store::MAX_CHANGE_BLOB + 1).unwrap();
    let outside = bench.root.parent().unwrap().join("outside.txt");
    std::fs::write(&outside, "theirs\n").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, bench.root.join("link.txt")).unwrap();

    let tracker = bench.tracker(c, true);
    tracker
        .begin_turn(inner, None, ConversationMode::Manual)
        .unwrap();
    for path in ["huge.bin", "../outside.txt", "link.txt"] {
        tracker.allowed(&edit_of(path));
        tracker.tool_ended(inner, "Edit").unwrap();
    }
    std::fs::write(&outside, "the agent's\n").unwrap();
    tracker.end_turn(inner, None).unwrap();

    let seen = view(inner, c).unwrap();
    assert_eq!((seen.pending, seen.turns.len()), (0, 0));
    let all = settle(
        inner,
        c,
        Verdict::Undo,
        Target::All,
        Disposal::Unlink,
        false,
    )
    .unwrap();
    assert_eq!(all.files, 0);
    assert_eq!(
        std::fs::read_to_string(&outside).unwrap(),
        "the agent's\n",
        "an Undo never reaches outside the checkout"
    );
    bench.engine.shutdown().await;
}

#[tokio::test]
async fn a_restore_said_twice_goes_back_once() {
    let bench = bench(vec![], false);
    let inner = bench.engine.inner();
    let c = bench.conversation(ConversationMode::Manual);
    let first = one_edit(&bench, c, 2, "two");
    one_edit(&bench, c, 8, "eight");

    let back = restore(inner, c, first, Disposal::Unlink).unwrap();
    assert_eq!((back.files, back.pending), (1, 0));
    assert_eq!(bench.read("file.txt"), TEN_LINES);

    bench.person_saves("file.txt", &line(5, "five"));
    let again = restore(inner, c, first, Disposal::Unlink).unwrap();
    assert!(again.skipped.is_empty(), "{:?}", again.skipped);
    assert_eq!(
        bench.read("file.txt"),
        line(5, "five"),
        "what a person wrote after the restore is not taken back by a second one"
    );
    bench.engine.shutdown().await;
}

#[tokio::test]
async fn many_words_at_once_settle_every_file_exactly_once() {
    let bench = bench(vec![], false);
    let inner = Arc::clone(bench.engine.inner());
    let c = bench.conversation(ConversationMode::Manual);
    let tracker = bench.tracker(c, true);
    tracker
        .begin_turn(&inner, None, ConversationMode::Manual)
        .unwrap();
    for n in 0..12 {
        agent_edits(&bench, &tracker, &format!("src/f{n}.txt"), "made\n");
    }
    tracker.end_turn(&inner, None).unwrap();
    assert_eq!(view(&inner, c).unwrap().pending, 12);

    let reached: usize = std::thread::scope(|scope| {
        let hands: Vec<_> = (0..8)
            .map(|n| {
                let inner = &inner;
                scope.spawn(move || {
                    let verdict = if n % 2 == 0 {
                        Verdict::Keep
                    } else {
                        Verdict::Undo
                    };
                    settle(inner, c, verdict, Target::All, Disposal::Unlink, false)
                        .unwrap()
                        .files
                })
            })
            .collect();
        hands.into_iter().map(|h| h.join().unwrap()).sum()
    });
    assert_eq!(
        reached, 12,
        "each file heard one word, from whoever came first"
    );
    let seen = view(&inner, c).unwrap();
    assert_eq!(seen.pending, 0);
    for file in &seen.turns[0].files {
        let stands = bench.root.join(&file.path).exists();
        assert_eq!(
            stands,
            file.state == ChangeState::Kept,
            "{} is {:?}",
            file.path,
            file.state
        );
    }
    bench.engine.shutdown().await;
}

// ---------------------------------------------------------------------------
// The ledger outlives the node, and an unreadable one does not stop the pane.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn pending_changes_outlive_a_restart_and_are_still_undone() {
    let bench = bench(vec![], false);
    let c = bench.conversation(ConversationMode::Manual);
    one_edit(&bench, c, 2, "two");
    let Bench {
        _dir: dir,
        engine,
        root,
        ..
    } = bench;
    engine.shutdown().await;

    let engine = common::engine_with(&dir, vec![]);
    let inner = engine.inner();
    let seen = view(inner, c).unwrap();
    assert_eq!((seen.pending, seen.turns.len()), (1, 1));
    assert!(
        asks::open(inner, c).is_empty(),
        "no ask outlives its session"
    );
    settle(
        inner,
        c,
        Verdict::Undo,
        Target::All,
        Disposal::Unlink,
        false,
    )
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(root.join("file.txt")).unwrap(),
        TEN_LINES
    );
    engine.shutdown().await;
}

#[tokio::test]
async fn a_ledger_that_does_not_parse_is_set_aside_and_the_conversation_goes_on() {
    let bench = bench(vec![], false);
    let inner = bench.engine.inner();
    let c = bench.conversation(ConversationMode::Manual);
    one_edit(&bench, c, 2, "two");
    let ledger = bench
        .engine
        .workspace()
        .change_index_file(c)
        .with_file_name("ledger.json");
    let blobs = ledger.with_file_name("blobs");
    let held = std::fs::read_dir(&blobs).unwrap().count();
    std::fs::write(&ledger, "{ \"turns\": [ tr").unwrap();

    let seen = view(inner, c).unwrap();
    assert_eq!((seen.pending, seen.turns.len()), (0, 0));
    let aside = ledger.with_file_name(bisa_store::UNREADABLE_LEDGER);
    assert!(aside.exists(), "the unreadable ledger is kept, not deleted");
    assert_eq!(
        bench.read("file.txt"),
        line(2, "two"),
        "the checkout is as it was"
    );

    // The next turn is recorded as any other, and the blobs the ledger set
    // aside names are still there for whoever reads it.
    one_edit(&bench, c, 8, "eight");
    assert_eq!(view(inner, c).unwrap().pending, 1);
    assert!(std::fs::read_dir(&blobs).unwrap().count() >= held);
    bench.engine.shutdown().await;
}

#[tokio::test]
async fn a_deleted_conversation_answers_its_asks_no_and_leaves_the_checkout_alone() {
    let bench = bench(vec![], false);
    let inner = Arc::clone(bench.engine.inner());
    let c = bench.conversation(ConversationMode::Manual);
    one_edit(&bench, c, 2, "two");
    let asking = {
        let inner = Arc::clone(&inner);
        tokio::spawn(async move {
            asks::ask(
                &inner,
                c,
                "coder",
                AskSubject::Tool {
                    tool: "Bash".into(),
                    tier: ToolTier::Exec,
                },
                "Run `cargo test`?".into(),
                true,
            )
            .await
        })
    };
    common::until("the question is open", || {
        asks::open(&inner, c).into_iter().next()
    })
    .await;
    let changes_dir = bench
        .engine
        .workspace()
        .change_index_file(c)
        .parent()
        .unwrap()
        .to_path_buf();
    assert!(changes_dir.exists());

    bisa_engine::conversations::delete(&inner, c).await.unwrap();
    let said = asking.await.unwrap();
    assert!(
        matches!(said, asks::Said::Deny(_)),
        "a turn waiting on a deleted conversation hears no"
    );
    assert!(
        !changes_dir.exists(),
        "the record of its changes went with it"
    );
    assert_eq!(bench.read("file.txt"), line(2, "two"), "the files did not");
    assert!(view(&inner, c).is_err());
    bench.engine.shutdown().await;
}

#[tokio::test]
async fn allowed_for_the_conversation_ends_when_the_mode_changes() {
    let bench = bench(vec![], false);
    let inner = Arc::clone(bench.engine.inner());
    let c = bench.conversation(ConversationMode::Manual);
    let ask_bash = |grantable: bool| {
        let inner = Arc::clone(&inner);
        tokio::spawn(async move {
            asks::ask(
                &inner,
                c,
                "coder",
                AskSubject::Tool {
                    tool: "Bash".into(),
                    tier: ToolTier::Exec,
                },
                "Run `cargo test`?".into(),
                grantable,
            )
            .await
        })
    };

    let first = ask_bash(true);
    let open = common::until("the first ask", || asks::open(&inner, c).into_iter().next()).await;
    asks::answer(
        &inner,
        c,
        &open.id,
        AskAnswer::Allow {
            scope: AskScope::Conversation,
        },
    )
    .unwrap();
    assert!(matches!(first.await.unwrap(), asks::Said::Allow));

    // The same tool again: allowed without a card.
    assert!(matches!(ask_bash(true).await.unwrap(), asks::Said::Allow));
    assert!(asks::open(&inner, c).is_empty());
    // A guard rule's ask is never covered by it.
    let ruled = ask_bash(false);
    let open = common::until("a rule's ask", || asks::open(&inner, c).into_iter().next()).await;
    assert!(!open.grantable);
    asks::answer(&inner, c, &open.id, AskAnswer::Deny { note: None }).unwrap();
    assert!(matches!(ruled.await.unwrap(), asks::Said::Deny(_)));

    // Setting the mode it already has is not a change, and ends nothing.
    bisa_engine::conversations::set_mode(&inner, c, ConversationMode::Manual).unwrap();
    assert!(matches!(ask_bash(true).await.unwrap(), asks::Said::Allow));

    // A plan: what was allowed in manual does not carry over — nor back.
    bisa_engine::conversations::set_mode(&inner, c, ConversationMode::Plan).unwrap();
    bisa_engine::conversations::set_mode(&inner, c, ConversationMode::Manual).unwrap();
    let after = ask_bash(true);
    let open = common::until("asked again", || asks::open(&inner, c).into_iter().next()).await;
    asks::answer(
        &inner,
        c,
        &open.id,
        AskAnswer::Deny {
            note: Some("  ".into()),
        },
    )
    .unwrap();
    match after.await.unwrap() {
        asks::Said::Deny(reason) => assert_eq!(
            reason, "refused by the person in the conversation",
            "a blank note adds nothing"
        ),
        asks::Said::Allow => panic!("the grant outlived the mode it was given in"),
    }
    bench.engine.shutdown().await;
}

#[tokio::test]
async fn a_mode_changed_between_two_messages_holds_from_the_next_call() {
    let adapter = guarded("editing", edit_of("file.txt"));
    let answered = Arc::clone(&adapter.answered);
    let bench = bench(vec![adapter], false);
    let inner = bench.engine.inner();
    let c = bench.conversation(ConversationMode::Manual);

    say(&bench, c, "rename the function");
    let first = common::until("the first edit is answered", || {
        answered.lock().unwrap().first().map(|(_, a)| a.clone())
    })
    .await;
    assert!(matches!(first, InputAnswer::Allow { .. }), "{first:?}");

    // The same conversation, the same agent, now planning: the next edit is
    // the mode's sentence, whether or not the session is the one that ran.
    bisa_engine::conversations::set_mode(inner, c, ConversationMode::Plan).unwrap();
    say(&bench, c, "and the callers");
    let second = common::until("the second edit is answered", || {
        answered.lock().unwrap().get(1).map(|(_, a)| a.clone())
    })
    .await;
    match second {
        InputAnswer::Deny { reason } => assert_eq!(reason, bisa_engine::inputs::PLAN_REFUSAL),
        other => panic!("a plan let an edit land: {other:?}"),
    }
    bench.engine.shutdown().await;
}
