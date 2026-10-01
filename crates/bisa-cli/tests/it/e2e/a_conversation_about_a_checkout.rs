//! A conversation about a checkout, through the binary while a node runs:
//! how far its agent goes on its own in each of its three modes, what the
//! agent changed kept, undone and restored, and a call that is the person's
//! to decide asked — and answered — in the conversation itself.
//!
//! The agent is the scripted one, and its tools are its own: an edit of one
//! file, and a command that runs nothing and leaves a file changed. It tells
//! each call the way the protocol does — announced once, then by its id
//! alone — so what the guard judges and what the review brackets is what the
//! adapter kept of it.
//!
//! Nothing here removes a file: what is undone and restored are changes to
//! files that were there before the agent came, written back as they were.

use super::sealed::Sealed;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

const COMMITTER: &str = "A Journey <journey@example.test>";

const PLAN: &str = "measure\ncut\nsand\nfit\n";
const NOTES: &str = "oak, 18 mm\n";

/// One turn of the script: taken once, by the first prompt that holds
/// `when` — a prompt carries what was said before it, so a turn taken is a
/// turn the next prompt passes by.
fn turn(when: &str, tools: Value, says: &str) -> Value {
    json!({ "scope": "conversation", "when": when, "times": 1, "tools": tools, "say": [says] })
}

/// An edit of one file, asked about before it is made.
fn an_edit(path: &str, content: &str) -> Value {
    json!({
        "name": "Write", "own": "edit", "ask": {},
        "arguments": { "path": path, "content": content },
    })
}

/// A command, asked about before it runs — it runs nothing: `leaves` is the
/// file it is said to have changed.
fn a_command(line: &str, leaves: (&str, &str)) -> Value {
    json!({
        "name": "Bash", "own": "command", "ask": {},
        "arguments": { "command": line },
        "leaves": [{ "path": leaves.0, "content": leaves.1 }],
    })
}

fn the_script() -> Value {
    json!({ "turns": [
        turn("first pass", json!([an_edit("plan.md", "measure twice\ncut\nsand\nfit\n")]), "The plan says to measure twice."),
        turn("second pass", json!([
            an_edit("plan.md", "measure\ncut\nsand\nfit\noil\n"),
            a_command("fake-tool stamp", ("notes.md", "oak, 18 mm\nstamped\n")),
        ]), "Oiled and stamped."),
        turn("third pass", json!([a_command("fake-tool polish", ("notes.md", "polished\n"))]), "I was told not to."),
        turn("think it over", json!([an_edit("plan.md", "a plan changes nothing\n")]), "Here is the plan: oil last."),
        turn("go ahead", json!([
            an_edit("plan.md", "measure\ncut\nsand\nfit\noil\nwax\n"),
            a_command("fake-tool stamp", ("notes.md", "oak, 18 mm\nstamped\nwaxed\n")),
        ]), "Waxed."),
        turn("and then", json!([]), "Nothing more to do."),
    ]})
}

/// The checkout: a project that is a repository, with two files committed.
fn a_checkout(ws: &Sealed) -> (String, PathBuf) {
    let made = ws.json(&["project", "new", "shelf", "--committer", COMMITTER]);
    let project = made["project"]["id"]
        .as_str()
        .expect("the project")
        .to_string();
    let tree = PathBuf::from(made["path"].as_str().expect("its tree"));
    std::fs::write(tree.join("plan.md"), PLAN).expect("the plan");
    std::fs::write(tree.join("notes.md"), NOTES).expect("the notes");
    ws.ok(&[
        "project",
        "commit",
        &project,
        "-m",
        "as it was",
        "--path",
        "plan.md",
        "--path",
        "notes.md",
    ]);
    (project, tree)
}

fn read(tree: &Path, name: &str) -> String {
    std::fs::read_to_string(tree.join(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// A conversation as the journey follows it.
struct Talk {
    id: String,
}

impl Talk {
    fn about(ws: &Sealed, project: &str) -> Self {
        let made = ws.json(&[
            "conversation",
            "new",
            "project",
            project,
            "--title",
            "The shelf",
        ]);
        Self {
            id: made["conversation"]["id"]
                .as_str()
                .expect("the conversation")
                .to_string(),
        }
    }

    fn route(&self, rest: &str) -> String {
        format!("/conversations/{}{rest}", self.id)
    }

    /// Everything said in it — the person's words and the agent's — each
    /// with the id it is known by.
    fn said(&self, ws: &Sealed) -> Vec<(String, String)> {
        ws.json(&["msgs", &self.id, "--limit", "200"])["messages"]
            .as_array()
            .expect("the messages")
            .iter()
            .map(|m| {
                (
                    m["id"].as_str().unwrap_or_default().to_string(),
                    m["content"].as_str().unwrap_or_default().to_string(),
                )
            })
            .collect()
    }

    fn replies(&self, ws: &Sealed) -> Vec<String> {
        self.said(ws).into_iter().map(|(_, words)| words).collect()
    }

    /// The id of the message that says `words`.
    fn message(&self, ws: &Sealed, words: &str) -> String {
        self.said(ws)
            .into_iter()
            .find(|(_, said)| said == words)
            .map(|(id, _)| id)
            .unwrap_or_else(|| panic!("nobody said {words:?}"))
    }

    /// The person says something, and nobody waits for what comes of it.
    fn says(&self, ws: &Sealed, words: &str) {
        ws.ok(&["conversation", "post", &self.id, words]);
    }

    /// The person says something, and the agent's reply to it lands.
    fn says_and_hears(&self, ws: &Sealed, words: &str, reply: &str) {
        self.says(ws, words);
        self.hears(ws, reply);
    }

    fn hears(&self, ws: &Sealed, reply: &str) {
        ws.until(&format!("the reply {reply:?}"), || {
            self.replies(ws)
                .iter()
                .any(|r| r.contains(reply))
                .then_some(())
        });
        ws.until("the turn to be over", || {
            let (_, live) = ws.call("GET", &self.route("/live"), &[], None);
            (live["turns"] == json!([])).then_some(())
        });
    }

    fn changes(&self, ws: &Sealed) -> Value {
        ws.json(&["conversation", "changes", &self.id])
    }

    /// Each file of each turn that changed something: `path state`.
    fn files(&self, ws: &Sealed) -> Vec<Vec<String>> {
        self.changes(ws)["turns"]
            .as_array()
            .expect("the turns")
            .iter()
            .map(|turn| {
                let mut files: Vec<String> = turn["files"]
                    .as_array()
                    .expect("the files")
                    .iter()
                    .map(|f| {
                        format!(
                            "{} {} {}",
                            f["path"].as_str().unwrap_or_default(),
                            f["kind"].as_str().unwrap_or_default(),
                            f["state"].as_str().unwrap_or_default()
                        )
                    })
                    .collect();
                files.sort();
                files
            })
            .collect()
    }

    /// The asks a turn waits on, once there is one.
    fn asked(&self, ws: &Sealed) -> Value {
        ws.until("an ask in the conversation", || {
            let (status, open) = ws.call("GET", &self.route("/asks"), &[], None);
            assert_eq!(status, 200, "{open}");
            open["asks"]
                .as_array()
                .and_then(|asks| asks.first().cloned())
        })
    }

    fn answers(&self, ws: &Sealed, ask: &Value, answer: Value) -> Value {
        let id = ask["id"].as_str().expect("the ask's id");
        let (status, left) = ws.call(
            "POST",
            &self.route(&format!("/asks/{id}")),
            &[],
            Some(&answer),
        );
        assert_eq!(status, 200, "{left}");
        left
    }

    fn mode(&self, ws: &Sealed, mode: &str) {
        let set = ws.json(&["conversation", "mode", &self.id, mode]);
        assert_eq!(set["conversation"]["mode"], mode, "{set}");
    }
}

/// What the scripted agent was answered when it asked before a tool.
fn answered(ws: &Sealed) -> Vec<String> {
    ws.recorded("permission")
        .iter()
        .map(|fact| {
            format!(
                "{} {}",
                fact["tool"].as_str().unwrap_or_default(),
                fact["outcome"].as_str().unwrap_or_default()
            )
        })
        .collect()
}

#[test]
fn an_agents_changes_are_reviewed_in_each_mode_and_a_call_is_asked_where_the_person_reads() {
    let mut ws = Sealed::with_script(&the_script());
    ws.start();
    let (project, tree) = a_checkout(&ws);
    let talk = Talk::about(&ws, &project);
    let view = ws.json(&["conversation", "mode", &talk.id]);
    assert_eq!(
        view["conversation"]["mode"], "manual",
        "as a conversation begins"
    );
    assert_eq!(talk.files(&ws), Vec::<Vec<String>>::new());

    // --- manual: an edit lands and waits for a word -----------------------------
    talk.says_and_hears(&ws, "a first pass, please", "measure twice");
    assert_eq!(read(&tree, "plan.md"), "measure twice\ncut\nsand\nfit\n");
    assert_eq!(
        answered(&ws),
        ["Write allowed"],
        "within the mode's reach: nobody was asked"
    );
    let changes = talk.changes(&ws);
    assert_eq!(changes["mode"], "manual");
    assert_eq!(changes["pending"], 1, "{changes}");
    assert_eq!(changes["owed"], true, "{changes}");
    assert_eq!(talk.files(&ws), [["plan.md modified pending"]]);
    let first = changes["turns"][0].clone();
    // The turn stands between the message that woke it and the reply it posted.
    assert_eq!(
        first["prompt"],
        talk.message(&ws, "a first pass, please").as_str(),
        "{first}"
    );
    assert_eq!(
        first["reply"],
        talk.message(&ws, "The plan says to measure twice.")
            .as_str(),
        "{first}"
    );
    assert_eq!(
        (
            first["files"][0]["added"].as_u64(),
            first["files"][0]["removed"].as_u64()
        ),
        (Some(1), Some(1))
    );
    // The file as the review reads it: what it was, and one hunk.
    let (status, file) = ws.call("GET", &talk.route("/changes/file?path=plan.md"), &[], None);
    assert_eq!(status, 200, "{file}");
    assert_eq!(file["file"]["base_text"], PLAN);
    assert_eq!(
        file["file"]["hunks"].as_array().map(Vec::len),
        Some(1),
        "{file}"
    );

    // Undone: the file is what it was, and nothing waits.
    let undone = ws.json(&["conversation", "undo", &talk.id, "--path", "plan.md"]);
    assert_eq!(
        (undone["files"].as_u64(), undone["pending"].as_u64()),
        (Some(1), Some(0)),
        "{undone}"
    );
    assert_eq!(undone["skipped"], json!([]));
    assert_eq!(read(&tree, "plan.md"), PLAN);
    assert_eq!(talk.files(&ws), [["plan.md modified undone"]]);

    // --- manual: a command is the person's to allow -----------------------------
    talk.says(&ws, "a second pass, please");
    let ask = talk.asked(&ws);
    assert_eq!(
        ask["subject"],
        json!({"kind": "tool", "tool": "Bash", "tier": "exec"}),
        "{ask}"
    );
    assert_eq!(ask["agent"], "general-agent", "{ask}");
    assert_eq!(ask["grantable"], true, "the mode's own ask: {ask}");
    assert!(
        ask["question"]
            .as_str()
            .is_some_and(|q| q.contains("fake-tool stamp")),
        "{ask}"
    );
    assert_eq!(
        read(&tree, "plan.md"),
        "measure\ncut\nsand\nfit\noil\n",
        "the edit before it ran"
    );
    assert_eq!(read(&tree, "notes.md"), NOTES, "and the command waits");
    let left = talk.answers(&ws, &ask, json!({"answer": "allow", "scope": "once"}));
    assert_eq!(left["asks"], json!([]));
    talk.hears(&ws, "Oiled and stamped.");
    assert_eq!(read(&tree, "notes.md"), "oak, 18 mm\nstamped\n");
    assert_eq!(
        talk.files(&ws),
        [
            vec!["plan.md modified undone"],
            vec!["notes.md modified pending", "plan.md modified pending"],
        ],
        "what the command left is the turn's, as its edit is"
    );
    // Answered twice is an ask that no longer waits.
    let (status, refused) = ws.call(
        "POST",
        &talk.route(&format!("/asks/{}", ask["id"].as_str().unwrap_or_default())),
        &[],
        Some(&json!({"answer": "allow", "scope": "once"})),
    );
    assert_eq!(status, 400, "{refused}");

    // Kept: the turn's changes are the tree's, and nothing waits.
    let second = talk.changes(&ws)["turns"][1]["turn"]
        .as_str()
        .expect("the turn")
        .to_string();
    let kept = ws.json(&["conversation", "keep", &talk.id, "--turn", &second]);
    assert_eq!(
        (kept["files"].as_u64(), kept["pending"].as_u64()),
        (Some(2), Some(0)),
        "{kept}"
    );
    assert_eq!(read(&tree, "plan.md"), "measure\ncut\nsand\nfit\noil\n");

    // --- manual: a command refused, with why -------------------------------------
    talk.says(&ws, "a third pass, please");
    let ask = talk.asked(&ws);
    talk.answers(
        &ws,
        &ask,
        json!({"answer": "deny", "note": "not on a Sunday"}),
    );
    talk.hears(&ws, "I was told not to.");
    assert_eq!(
        read(&tree, "notes.md"),
        "oak, 18 mm\nstamped\n",
        "nothing ran"
    );
    assert_eq!(
        answered(&ws).last().map(String::as_str),
        Some("Bash refused")
    );

    // --- plan: reading, and nothing else -----------------------------------------
    talk.mode(&ws, "plan");
    talk.says_and_hears(&ws, "think it over first", "Here is the plan");
    assert_eq!(
        read(&tree, "plan.md"),
        "measure\ncut\nsand\nfit\noil\n",
        "a plan changes nothing"
    );
    assert_eq!(
        answered(&ws).last().map(String::as_str),
        Some("Write refused")
    );
    let (_, open) = ws.call("GET", &talk.route("/asks"), &[], None);
    assert_eq!(open["asks"], json!([]), "refused outright, never asked");

    // --- the node goes and comes back: the review is on disk ---------------------
    let before = talk.files(&ws);
    ws.stop();
    ws.start();
    assert_eq!(talk.files(&ws), before);
    let view = ws.json(&["conversation", "mode", &talk.id]);
    assert_eq!(view["conversation"]["mode"], "plan");

    // --- auto: the agent goes on, and the next message is the person's word ------
    talk.mode(&ws, "auto");
    talk.says_and_hears(&ws, "go ahead", "Waxed.");
    assert_eq!(
        read(&tree, "plan.md"),
        "measure\ncut\nsand\nfit\noil\nwax\n"
    );
    assert_eq!(read(&tree, "notes.md"), "oak, 18 mm\nstamped\nwaxed\n");
    let (_, open) = ws.call("GET", &talk.route("/asks"), &[], None);
    assert_eq!(
        open["asks"],
        json!([]),
        "within auto's reach: nobody was asked"
    );
    let changes = talk.changes(&ws);
    assert_eq!(
        (changes["pending"].as_u64(), changes["owed"].as_bool()),
        (Some(2), Some(false)),
        "{changes}"
    );
    let waxed = changes["turns"]
        .as_array()
        .and_then(|turns| turns.last())
        .and_then(|turn| turn["turn"].as_str())
        .expect("the turn")
        .to_string();
    talk.says_and_hears(&ws, "and then?", "Nothing more to do.");
    let changes = talk.changes(&ws);
    assert_eq!(
        changes["pending"], 0,
        "kept by the message that followed: {changes}"
    );
    assert_eq!(
        talk.files(&ws).last().cloned(),
        Some(vec![
            "notes.md modified kept".to_string(),
            "plan.md modified kept".to_string()
        ])
    );

    // --- restored: back to before the message that woke the turn -----------------
    let restored = ws.json(&["conversation", "restore", &talk.id, &waxed]);
    assert_eq!(restored["skipped"], json!([]), "{restored}");
    assert_eq!(restored["files"], 2, "{restored}");
    assert_eq!(read(&tree, "plan.md"), "measure\ncut\nsand\nfit\noil\n");
    assert_eq!(read(&tree, "notes.md"), "oak, 18 mm\nstamped\n");
    assert!(
        talk.replies(&ws).iter().any(|r| r == "Waxed."),
        "what was said stays said"
    );
    ws.stop();
}

/// What changes a conversation goes through the node when one runs: every
/// window is told, and what the node does beside the record is done.
#[test]
fn what_changes_a_conversation_is_heard_by_the_node_that_runs() {
    let mut ws = Sealed::with_script(&json!({ "turns": [
        { "scope": "conversation", "when": "wait here", "say": ["Waiting."], "hold": true },
    ]}));
    ws.start();
    let (project, _tree) = a_checkout(&ws);
    let listening = ws.listen();
    let talk = Talk::about(&ws, &project);

    let renamed = ws.json(&["conversation", "rename", &talk.id, "The oak shelf"]);
    assert_eq!(renamed["conversation"]["title"], "The oak shelf");
    assert_eq!(
        renamed["conversation"]["origin"]["kind"], "project",
        "the wire's shape: {renamed}"
    );
    talk.mode(&ws, "auto");
    // Refused where it means nothing, by the node as by the workspace.
    let about_the_workspace = ws.json(&["conversation", "new", "workspace"]);
    let elsewhere = about_the_workspace["conversation"]["id"]
        .as_str()
        .expect("an id");
    let refused = ws.bisa(&["conversation", "mode", elsewhere, "plan"]);
    assert!(!refused.status.success());

    // A turn is running: putting the conversation away stops it.
    talk.says(&ws, "wait here");
    ws.until("the turn to hold", || {
        (!ws.recorded("holding").is_empty()).then_some(())
    });
    let away = ws.json(&["conversation", "archive", &talk.id]);
    assert_eq!(away["conversation"]["archived"], true);
    ws.until("the turn to be stopped", || {
        (!ws.recorded("cancel").is_empty() || !ws.recorded("ended").is_empty()).then_some(())
    });
    let post = ws.bisa(&["conversation", "post", &talk.id, "anybody?"]);
    assert!(!post.status.success(), "put away, it takes no word");
    let back = ws.json(&["conversation", "unarchive", &talk.id]);
    assert_eq!(back["conversation"]["archived"], false);

    ws.ok(&["conversation", "delete", &talk.id]);
    let (status, _) = ws.call("GET", &talk.route(""), &[], None);
    assert_eq!(status, 404);
    assert!(
        !ws.data()
            .join("ide")
            .join("changes")
            .join(&talk.id)
            .exists(),
        "its review went with it"
    );

    // Every change was said on the bus, in order: what a window redraws on.
    let said_of_it = || -> Vec<String> {
        listening
            .heard()
            .frames
            .iter()
            .filter(|frame| frame["stream"] == "engine")
            .map(|frame| &frame["payload"]["payload"])
            .filter(|said| said["id"] == talk.id.as_str())
            .filter_map(|said| match said["type"].as_str()? {
                "conversation_created" => Some("created".to_string()),
                "conversation_changed" => said["change"].as_str().map(str::to_string),
                _ => None,
            })
            .collect()
    };
    let heard = ws.until("the deletion to be heard", || {
        let said = said_of_it();
        said.iter()
            .any(|change| change == "deleted")
            .then_some(said)
    });
    assert_eq!(
        heard,
        [
            "created",
            "renamed",
            "mode",
            "archived",
            "unarchived",
            "deleted"
        ]
    );
    ws.stop();
}
