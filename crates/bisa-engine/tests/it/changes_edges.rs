//! Reviewing an agent's changes at the edges the first suite left: what
//! somebody else does to a file under review — removes it, puts it back,
//! writes bytes that are no text over it, takes the agent's change out
//! themselves, edits beside it — is folded away before the next read; a
//! word about a turn that collides with another turn's lines is left alone
//! and named; a turn's Keep or Undo that leaves nothing pending settles the
//! file; a hunk undone on an overlapped file waits for `force`; a file the
//! agent made is unmade by its one hunk; a restore skips a file that is no
//! text and keeps an earlier turn's image; the tracker ignores what is no
//! permission, a read, a tool that ended after its turn, a file grown past
//! the review's bound, and a snapshot git refuses.

use crate::common;
use bisa_core::{
    AgentId, ChangeState, ConversationId, ConversationMode, ConversationOrigin, FileScope, RelPath,
    ToolTier,
};
use bisa_engine::changes::settle::{restore, settle, Target, Verdict};
use bisa_engine::changes::tracker::ChangeTracker;
use bisa_engine::changes::{file_view, view, Checkout};
use bisa_engine::ide::files::{write_file, Disposal};
use bisa_engine::Engine;
use bisa_harness::{InputKind, InputRequest};
use bisa_store::NewConversation;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::process::Command;

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
fn bench(git: bool) -> Bench {
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
        raw_git(&root, &["init", "--quiet"]);
        raw_git(&root, &["config", "user.name", "Bisa Test"]);
        raw_git(&root, &["config", "user.email", "test@example.invalid"]);
        raw_git(&root, &["add", "-A"]);
        raw_git(&root, &["commit", "--quiet", "-m", "first"]);
    }
    let engine = Engine::start(
        ws,
        common::catalog_with(vec![]),
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

    /// One turn in which the agent writes `text` to `path`.
    fn turn(&self, tracker: &ChangeTracker, n: usize, path: &str, text: &str) -> bisa_core::TurnId {
        let inner = self.engine.inner();
        let turn = tracker
            .begin_turn(inner, Some(format!("msg-{n}")), ConversationMode::Manual)
            .unwrap();
        agent_edits(self, tracker, path, text);
        tracker.end_turn(inner, Some(format!("reply-{n}"))).unwrap();
        turn
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

/// The state one turn's record gives a file.
fn state_in(
    bench: &Bench,
    c: ConversationId,
    turn: bisa_core::TurnId,
    path: &str,
) -> Option<ChangeState> {
    view(bench.engine.inner(), c)
        .unwrap()
        .turns
        .iter()
        .find(|t| t.turn == turn)
        .and_then(|t| t.files.iter().find(|f| f.path == path).map(|f| f.state))
}

fn state_of(bench: &Bench, c: ConversationId, path: &str) -> Option<ChangeState> {
    view(bench.engine.inner(), c)
        .unwrap()
        .turns
        .iter()
        .flat_map(|t| t.files.iter())
        .filter(|f| f.path == path)
        .map(|f| f.state)
        .next_back()
}

/// What somebody else does to a file under review is folded away before
/// the next read: removed, it is gone; put back to where it started, gone;
/// the agent's change taken out by hand with another edit beside it, gone;
/// edited beside the agent's lines, the base moves and the change stays;
/// written over with bytes that are no text, it is opaque and overlapped;
/// grown past the review's bound, it is left as the ledger last saw it.
#[tokio::test]
async fn what_somebody_else_does_to_a_file_under_review_is_folded_before_the_next_read() {
    let bench = bench(false);
    let inner = bench.engine.inner();
    let c = bench.conversation(ConversationMode::Manual);
    let tracker = bench.tracker(c, true);
    for (n, path) in [
        "gone.txt",
        "back.txt",
        "taken.txt",
        "beside.txt",
        "bytes.txt",
        "grown.txt",
    ]
    .iter()
    .enumerate()
    {
        std::fs::write(bench.root.join(path), TEN_LINES).unwrap();
        bench.turn(&tracker, n, path, &line(2, "two"));
    }
    assert_eq!(view(inner, c).unwrap().pending, 6);

    std::fs::remove_file(bench.root.join("gone.txt")).unwrap();
    bench.person_saves("back.txt", TEN_LINES);
    bench.person_saves("taken.txt", &line(9, "nine"));
    bench.person_saves("beside.txt", &line(2, "two").replace("\n9\n", "\nnine\n"));
    std::fs::write(bench.root.join("bytes.txt"), [b'x', 0, b'y']).unwrap();
    std::fs::write(
        bench.root.join("grown.txt"),
        vec![b'g'; bisa_store::MAX_CHANGE_BLOB as usize + 1],
    )
    .unwrap();

    let seen = view(inner, c).unwrap();
    assert_eq!(seen.pending, 3, "{seen:?}");
    for path in ["gone.txt", "back.txt", "taken.txt"] {
        assert_eq!(state_of(&bench, c, path), Some(ChangeState::Gone), "{path}");
    }
    let beside = file_view(inner, c, &rel("beside.txt")).unwrap().unwrap();
    assert_eq!(
        beside.base_text,
        line(9, "nine"),
        "their edit moved into the base, the agent's change stays pending"
    );
    assert_eq!(beside.hunks.len(), 1);
    assert!(!beside.overlapped);
    let bytes = file_view(inner, c, &rel("bytes.txt")).unwrap().unwrap();
    assert!(bytes.opaque && bytes.overlapped, "{bytes:?}");
    assert_eq!(state_of(&bench, c, "grown.txt"), Some(ChangeState::Pending));

    // An undo of the opaque file waits for `force`; forced, the base is back.
    let asked = settle(
        inner,
        c,
        Verdict::Undo,
        Target::File {
            path: rel("bytes.txt"),
        },
        Disposal::Unlink,
        false,
    )
    .unwrap();
    assert_eq!(asked.skipped.len(), 1, "{asked:?}");
    let forced = settle(
        inner,
        c,
        Verdict::Undo,
        Target::File {
            path: rel("bytes.txt"),
        },
        Disposal::Unlink,
        true,
    )
    .unwrap();
    assert_eq!((forced.files, forced.skipped.len()), (1, 0), "{forced:?}");
    assert_eq!(bench.read("bytes.txt"), TEN_LINES);

    // A word about a file nothing waits on reaches nothing.
    let nothing = settle(
        inner,
        c,
        Verdict::Keep,
        Target::File {
            path: rel("nothing.txt"),
        },
        Disposal::Unlink,
        false,
    )
    .unwrap();
    assert_eq!(nothing.files, 0);
    bench.engine.shutdown().await;
}

/// Two turns on one file: a Keep or an Undo of one turn that collides with
/// the other's lines is left alone and named; one that leaves nothing of
/// the file pending settles it; a turn's Undo on an overlapped file waits
/// for `force`.
#[tokio::test]
async fn a_word_about_one_turn_folds_in_or_takes_out_and_names_what_it_cannot() {
    let bench = bench(false);
    let inner = bench.engine.inner();
    let c = bench.conversation(ConversationMode::Manual);
    let tracker = bench.tracker(c, true);
    // Two turns on different lines: keeping both, one at a time, settles the file.
    let first = bench.turn(&tracker, 1, "file.txt", &line(2, "two"));
    let second = bench.turn(
        &tracker,
        2,
        "file.txt",
        &line(2, "two").replace("\n9\n", "\nnine\n"),
    );
    let kept = settle(
        inner,
        c,
        Verdict::Keep,
        Target::Turn { turn: first },
        Disposal::Unlink,
        false,
    )
    .unwrap();
    assert_eq!((kept.files, kept.pending), (1, 1), "{kept:?}");
    let kept = settle(
        inner,
        c,
        Verdict::Keep,
        Target::Turn { turn: second },
        Disposal::Unlink,
        false,
    )
    .unwrap();
    assert_eq!((kept.files, kept.pending), (1, 0), "{kept:?}");
    assert_eq!(state_of(&bench, c, "file.txt"), Some(ChangeState::Kept));

    // Two turns on the same line: keeping the later one alone cannot fold
    // into a base that lacks the earlier; undoing the earlier one alone
    // cannot take it out from under the later. Each is left alone and named.
    std::fs::write(bench.root.join("same.txt"), TEN_LINES).unwrap();
    let a = bench.turn(&tracker, 3, "same.txt", &line(2, "two"));
    let b = bench.turn(&tracker, 4, "same.txt", &line(2, "deux"));
    for (verdict, turn) in [(Verdict::Keep, b), (Verdict::Undo, a)] {
        let said = settle(
            inner,
            c,
            verdict,
            Target::Turn { turn },
            Disposal::Unlink,
            false,
        )
        .unwrap();
        assert_eq!(said.skipped.len(), 1, "{verdict:?}: {said:?}");
        assert!(
            said.skipped[0].why.contains("changed on the same lines"),
            "{said:?}"
        );
    }
    assert_eq!(bench.read("same.txt"), line(2, "deux"));

    // Two turns on different lines, undone one at a time: the last leaves
    // nothing and settles the file.
    std::fs::write(bench.root.join("apart.txt"), TEN_LINES).unwrap();
    let x = bench.turn(&tracker, 5, "apart.txt", &line(2, "two"));
    let y = bench.turn(
        &tracker,
        6,
        "apart.txt",
        &line(2, "two").replace("\n9\n", "\nnine\n"),
    );
    let undone = settle(
        inner,
        c,
        Verdict::Undo,
        Target::Turn { turn: y },
        Disposal::Unlink,
        false,
    )
    .unwrap();
    assert_eq!((undone.files, undone.skipped.len()), (1, 0), "{undone:?}");
    assert_eq!(bench.read("apart.txt"), line(2, "two"));
    assert_eq!(
        state_in(&bench, c, y, "apart.txt"),
        Some(ChangeState::Undone)
    );
    assert_eq!(
        state_in(&bench, c, x, "apart.txt"),
        Some(ChangeState::Pending),
        "the first turn's change still waits"
    );
    let undone = settle(
        inner,
        c,
        Verdict::Undo,
        Target::Turn { turn: x },
        Disposal::Unlink,
        false,
    )
    .unwrap();
    assert_eq!((undone.files, undone.skipped.len()), (1, 0), "{undone:?}");
    assert_eq!(bench.read("apart.txt"), TEN_LINES);
    assert_eq!(state_of(&bench, c, "apart.txt"), Some(ChangeState::Undone));

    // Somebody reverted the earlier turn's line by hand: what the later
    // turn changed is all that stands between the base and the disk, so
    // keeping it settles the file — and undoing it settles it the other way.
    for (n, verdict, end) in [
        (9, Verdict::Keep, ChangeState::Kept),
        (11, Verdict::Undo, ChangeState::Undone),
    ] {
        let path = format!("reverted-{n}.txt");
        std::fs::write(bench.root.join(&path), TEN_LINES).unwrap();
        let early = bench.turn(&tracker, n, &path, &line(2, "two"));
        let late = bench.turn(
            &tracker,
            n + 1,
            &path,
            &line(2, "two").replace("\n9\n", "\nnine\n"),
        );
        bench.person_saves(&path, &line(9, "nine"));
        let said = settle(
            inner,
            c,
            verdict,
            Target::Turn { turn: late },
            Disposal::Unlink,
            true,
        )
        .unwrap();
        assert_eq!(
            (said.files, said.skipped.len()),
            (1, 0),
            "{verdict:?}: {said:?}"
        );
        assert_eq!(state_in(&bench, c, late, &path), Some(end));
        assert_eq!(
            view(inner, c)
                .unwrap()
                .turns
                .iter()
                .filter(|t| t.turn == early || t.turn == late)
                .flat_map(|t| t.files.iter())
                .filter(|f| f.path == path && f.state == ChangeState::Pending)
                .count(),
            0,
            "nothing of the file waits any more: {verdict:?}"
        );
        let expected = match verdict {
            Verdict::Keep => line(9, "nine"),
            Verdict::Undo => TEN_LINES.to_string(),
        };
        assert_eq!(bench.read(&path), expected, "{verdict:?}");
    }

    // A turn's Undo on a file somebody else edited on the agent's lines —
    // another turn's lines, so the undo itself would merge — waits for force.
    std::fs::write(bench.root.join("over.txt"), TEN_LINES).unwrap();
    let p = bench.turn(&tracker, 7, "over.txt", &line(2, "two"));
    let _q = bench.turn(
        &tracker,
        8,
        "over.txt",
        &line(2, "two").replace("\n9\n", "\nnine\n"),
    );
    bench.person_saves("over.txt", &line(2, "two").replace("\n9\n", "\nNINE\n"));
    let asked = settle(
        inner,
        c,
        Verdict::Undo,
        Target::Turn { turn: p },
        Disposal::Unlink,
        false,
    )
    .unwrap();
    assert_eq!(asked.skipped.len(), 1, "{asked:?}");
    assert!(asked.skipped[0].why.contains("somebody else"), "{asked:?}");
    let forced = settle(
        inner,
        c,
        Verdict::Undo,
        Target::Turn { turn: p },
        Disposal::Unlink,
        true,
    )
    .unwrap();
    assert_eq!((forced.files, forced.skipped.len()), (1, 0), "{forced:?}");
    assert_eq!(
        bench.read("over.txt"),
        TEN_LINES.replace("\n9\n", "\nNINE\n")
    );
    bench.engine.shutdown().await;
}

/// By hunk: an undo on an overlapped file waits for `force`; the one hunk
/// of a file the agent made undone unmakes it; the only hunk kept settles
/// the file.
#[tokio::test]
async fn a_hunk_undone_waits_where_somebody_else_edited_and_unmakes_a_file_the_agent_made() {
    let bench = bench(false);
    let inner = bench.engine.inner();
    let c = bench.conversation(ConversationMode::Manual);
    let tracker = bench.tracker(c, true);
    bench.turn(&tracker, 1, "file.txt", &line(2, "two"));
    bench.person_saves("file.txt", &line(2, "TWO"));
    let hunk_of = |path: &str| {
        let file = file_view(inner, c, &rel(path)).unwrap().unwrap();
        (file.hunks[0].id.clone(), file.disk_hash.clone())
    };
    let (hunk, hash) = hunk_of("file.txt");
    let asked = settle(
        inner,
        c,
        Verdict::Undo,
        Target::Hunk {
            path: rel("file.txt"),
            hunk,
            disk_hash: hash,
        },
        Disposal::Unlink,
        false,
    )
    .unwrap();
    assert_eq!(asked.skipped.len(), 1, "{asked:?}");
    assert_eq!(bench.read("file.txt"), line(2, "TWO"), "left as it is");

    bench.turn(&tracker, 2, "made.txt", "made by the agent\n");
    let (hunk, hash) = hunk_of("made.txt");
    let unmade = settle(
        inner,
        c,
        Verdict::Undo,
        Target::Hunk {
            path: rel("made.txt"),
            hunk,
            disk_hash: hash,
        },
        Disposal::Unlink,
        false,
    )
    .unwrap();
    assert_eq!((unmade.files, unmade.skipped.len()), (1, 0), "{unmade:?}");
    assert!(!bench.root.join("made.txt").exists(), "unmade");
    assert_eq!(state_of(&bench, c, "made.txt"), Some(ChangeState::Undone));

    // A file the agent made in a folder the disk will not let go of: the
    // undo halts at the disposal, says so, and the ledger is as it was.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        bench.turn(&tracker, 9, "locked/held.txt", "held\n");
        std::fs::set_permissions(
            bench.root.join("locked"),
            std::fs::Permissions::from_mode(0o555),
        )
        .unwrap();
        let halted = settle(
            inner,
            c,
            Verdict::Undo,
            Target::File {
                path: rel("locked/held.txt"),
            },
            Disposal::Unlink,
            false,
        );
        std::fs::set_permissions(
            bench.root.join("locked"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
        assert!(halted.is_err(), "{halted:?}");
        assert!(bench.root.join("locked/held.txt").is_file(), "it stayed");
        assert_eq!(
            state_of(&bench, c, "locked/held.txt"),
            Some(ChangeState::Pending)
        );
    }

    bench.turn(&tracker, 3, "one.txt", "one hunk\n");
    let (hunk, hash) = hunk_of("one.txt");
    let kept = settle(
        inner,
        c,
        Verdict::Keep,
        Target::Hunk {
            path: rel("one.txt"),
            hunk,
            disk_hash: hash,
        },
        Disposal::Unlink,
        false,
    )
    .unwrap();
    assert_eq!((kept.files, kept.skipped.len()), (1, 0), "{kept:?}");
    assert_eq!(state_of(&bench, c, "one.txt"), Some(ChangeState::Kept));
    bench.engine.shutdown().await;
}

/// A restore back before a turn skips a file that is no text now, and
/// keeps what an earlier turn left pending in a file it moved.
#[tokio::test]
async fn a_restore_skips_what_is_no_text_and_keeps_an_earlier_turns_image() {
    let bench = bench(false);
    let inner = bench.engine.inner();
    let c = bench.conversation(ConversationMode::Manual);
    let tracker = bench.tracker(c, true);
    let first = bench.turn(&tracker, 1, "file.txt", &line(2, "two"));
    let second = bench.turn(
        &tracker,
        2,
        "file.txt",
        &line(2, "two").replace("\n9\n", "\nnine\n"),
    );
    std::fs::write(bench.root.join("pic.bin"), b"text at first\n").unwrap();
    let third = bench.turn(&tracker, 3, "pic.bin", "then prose\n");
    std::fs::write(bench.root.join("pic.bin"), [b'p', 0, b'g']).unwrap();

    let back = restore(inner, c, third, Disposal::Unlink).unwrap();
    assert_eq!(back.skipped.len(), 1, "{back:?}");
    assert_eq!(back.skipped[0].path, "pic.bin");

    // Back before the second turn: the first turn's change stays pending,
    // and the picture — a later turn's — is skipped again.
    let back = restore(inner, c, second, Disposal::Unlink).unwrap();
    assert_eq!((back.files, back.skipped.len()), (1, 1), "{back:?}");
    assert_eq!(back.skipped[0].path, "pic.bin");
    assert_eq!(bench.read("file.txt"), line(2, "two"));
    assert_eq!(
        state_in(&bench, c, first, "file.txt"),
        Some(ChangeState::Pending),
        "the first turn still waits"
    );
    assert_eq!(
        state_in(&bench, c, second, "file.txt"),
        Some(ChangeState::Undone)
    );
    assert_eq!(
        state_in(&bench, c, third, "pic.bin"),
        Some(ChangeState::Pending),
        "left as it is"
    );

    // A file a turn made that somebody else removed since: going back
    // before that turn finds nothing to unmake, and counts the file.
    let made = bench.turn(&tracker, 4, "made.txt", "made\n");
    std::fs::remove_file(bench.root.join("made.txt")).unwrap();
    let back = restore(inner, c, made, Disposal::Unlink).unwrap();
    assert_eq!((back.files, back.skipped.len()), (1, 0), "{back:?}");
    assert!(!bench.root.join("made.txt").exists());
    bench.engine.shutdown().await;
}

/// The tracker's edges: what is no permission and a read are not tracked;
/// a tool that ends after its turn, and a command's sweep after it, record
/// nothing; a write still open when the turn ends is recorded; a file grown
/// past the bound between the call and its end is left out and said; an
/// outside write between the last look and this tool is folded first; and
/// a snapshot git refuses is a turn with no sweep, said in the log.
#[tokio::test]
async fn the_trackers_edges_what_is_not_tracked_and_what_is_folded_first() {
    let bench = bench(true);
    let inner = bench.engine.inner();
    let c = bench.conversation(ConversationMode::Manual);
    let tracker = bench.tracker(c, false);
    tracker.allowed(&InputRequest {
        id: "q".into(),
        kind: InputKind::Question {
            text: "which?".into(),
            options: vec![],
        },
        ..edit_of("file.txt")
    });
    tracker.allowed(&InputRequest::permission(
        "r",
        "Read",
        ToolTier::Read,
        "file.txt",
        json!({"file_path": "file.txt"}),
    ));
    // A tool ending with no turn open records nothing.
    tracker.tool_ended(inner, "Edit").unwrap();
    tracker.tool_ended(inner, "Bash").unwrap();

    // A write open when the turn ends is recorded by the turn's end — the
    // tool naming the file by its absolute path, as a harness does.
    tracker
        .begin_turn(inner, Some("msg-1".into()), ConversationMode::Manual)
        .unwrap();
    let absolute = bench.root.join("file.txt").display().to_string();
    tracker.allowed(&InputRequest::permission(
        "abs",
        "Edit",
        ToolTier::Write,
        &absolute,
        json!({"file_path": absolute}),
    ));
    std::fs::write(bench.root.join("file.txt"), line(2, "two")).unwrap();
    tracker.end_turn(inner, Some("reply-1".into())).unwrap();
    assert_eq!(view(inner, c).unwrap().pending, 1);

    // Grown past the bound between the call and its end: left out.
    tracker
        .begin_turn(inner, Some("msg-2".into()), ConversationMode::Manual)
        .unwrap();
    tracker.allowed(&edit_of("big.txt"));
    std::fs::write(
        bench.root.join("big.txt"),
        vec![b'b'; bisa_store::MAX_CHANGE_BLOB as usize + 1],
    )
    .unwrap();
    tracker.tool_ended(inner, "Edit").unwrap();
    // An outside write between the last look and this tool is folded first.
    tracker.allowed(&edit_of("file.txt"));
    bench.person_saves("file.txt", TEN_LINES);
    std::fs::write(bench.root.join("file.txt"), line(3, "three")).unwrap();
    tracker.tool_ended(inner, "Edit").unwrap();
    tracker.end_turn(inner, Some("reply-2".into())).unwrap();
    let seen = view(inner, c).unwrap();
    assert!(
        seen.turns
            .iter()
            .all(|t| t.files.iter().all(|f| f.path != "big.txt")),
        "{seen:?}"
    );
    let file = file_view(inner, c, &rel("file.txt")).unwrap().unwrap();
    assert_eq!(file.base_text, TEN_LINES, "their put-back is the base now");
    assert_eq!(file.hunks.len(), 1, "the agent's new change alone");

    // A command's window: a file the command made past the bound is left
    // out when it is read, and one already past the bound in the snapshot
    // is left out before it is read.
    std::fs::write(
        bench.root.join("heavy.bin"),
        vec![b'h'; bisa_store::MAX_CHANGE_BLOB as usize + 1],
    )
    .unwrap();
    raw_git(&bench.root, &["add", "-A"]);
    raw_git(&bench.root, &["commit", "--quiet", "-m", "heavy"]);
    tracker
        .begin_turn(inner, Some("msg-3".into()), ConversationMode::Manual)
        .unwrap();
    tracker.allowed(&InputRequest::permission(
        "cmd",
        "Bash",
        ToolTier::Exec,
        "make",
        json!({"command": "make"}),
    ));
    std::fs::write(
        bench.root.join("built.bin"),
        vec![b'z'; bisa_store::MAX_CHANGE_BLOB as usize + 1],
    )
    .unwrap();
    std::fs::write(bench.root.join("small.txt"), "built\n").unwrap();
    std::fs::write(
        bench.root.join("heavy.bin"),
        vec![b'H'; bisa_store::MAX_CHANGE_BLOB as usize + 2],
    )
    .unwrap();
    tracker.tool_ended(inner, "Bash").unwrap();
    tracker.end_turn(inner, Some("reply-3".into())).unwrap();
    let seen = view(inner, c).unwrap();
    let swept: Vec<&str> = seen
        .turns
        .iter()
        .flat_map(|t| t.files.iter().map(|f| f.path.as_str()))
        .filter(|p| p.ends_with(".bin") || *p == "small.txt")
        .collect();
    assert_eq!(swept, vec!["small.txt"], "{seen:?}");

    // A command still running when the turn ends: its window closes with
    // the turn, and what it moved is the turn's.
    tracker
        .begin_turn(inner, Some("msg-3b".into()), ConversationMode::Manual)
        .unwrap();
    tracker.allowed(&InputRequest::permission(
        "cmd-open",
        "Bash",
        ToolTier::Exec,
        "make slowly",
        json!({"command": "make slowly"}),
    ));
    std::fs::write(bench.root.join("late.txt"), "moved late\n").unwrap();
    tracker.end_turn(inner, Some("reply-3b".into())).unwrap();
    assert!(
        view(inner, c)
            .unwrap()
            .turns
            .iter()
            .any(|t| t.files.iter().any(|f| f.path == "late.txt")),
        "the window closed with the turn and swept what moved"
    );

    // A snapshot git refuses: the turn begins, and a command's window
    // attributes nothing.
    std::fs::write(bench.root.join(".git/config"), "[core]\n\tbare = maybe\n").unwrap();
    tracker
        .begin_turn(inner, Some("msg-4".into()), ConversationMode::Manual)
        .unwrap();
    tracker.allowed(&InputRequest::permission(
        "cmd2",
        "Bash",
        ToolTier::Exec,
        "make again",
        json!({"command": "make again"}),
    ));
    std::fs::write(bench.root.join("unseen.txt"), "moved\n").unwrap();
    tracker.tool_ended(inner, "Bash").unwrap();
    tracker.end_turn(inner, Some("reply-4".into())).unwrap();
    assert!(
        view(inner, c)
            .unwrap()
            .turns
            .iter()
            .all(|t| t.files.iter().all(|f| f.path != "unseen.txt")),
        "no snapshot, nothing attributed"
    );
    bench.engine.shutdown().await;
}
