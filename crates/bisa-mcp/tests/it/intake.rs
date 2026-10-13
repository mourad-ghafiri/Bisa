//! Tests against a fake intake server implementing the engine's wire protocol
//! verbatim (shapes copied from bisa-engine/src/intake.rs).

use bisa_core::Assignee;
use bisa_core::{Answer, AskOption};
use bisa_mcp::client::{Decision, Proposed, Saved};
use bisa_mcp::{
    AskHumanParams, AskOptionParam, GuardedWrite, IntakeClient, Raised, Scope, StandingGoal,
    SubmitOutcome, ToolCore, CORE_AGENT_ID, WORKFLOW_AGENT_ID,
};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;

const GOAL: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";

/// The work item of a run of the workspace, which has no goal.
const WORKSPACE_ITEM: &str = "wi-workspace";
const RUN: &str = "01ARZ3NDEKTSV4RRFFQ69G5RUN";

/// A plain approve/decline question — the common shape in these tests.
fn ask(question: &str) -> AskHumanParams {
    AskHumanParams {
        question: question.into(),
        expects: "decision".into(),
        options: vec![],
        multi: false,
    }
}

/// A question expecting an answer, with the options it offers.
fn ask_answer(question: &str, options: Vec<AskOptionParam>, multi: bool) -> AskHumanParams {
    AskHumanParams {
        question: question.into(),
        expects: "answer".into(),
        options,
        multi,
    }
}

/// Fake engine intake. Result schema is `{"answer": <number>}`; recall slug
/// "style" exists at hash "r1". Tracks submits.
struct FakeIntake {
    path: PathBuf,
    submits: Arc<AtomicU64>,
    _dir: tempfile::TempDir,
}

impl FakeIntake {
    fn start() -> Self {
        Self::start_with(false)
    }

    /// The fake, refusing every request when `refuse_all` — an engine that
    /// says no to the ops whose envelope names nothing but the agent.
    fn start_with(refuse_all: bool) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("intake.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let submits = Arc::new(AtomicU64::new(0));
        let submits2 = Arc::clone(&submits);
        tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                let submits = Arc::clone(&submits2);
                tokio::spawn(async move {
                    let (r, mut w) = stream.into_split();
                    let mut lines = BufReader::new(r).lines();
                    while let Ok(Some(line)) = lines.next_line().await {
                        let reply = if refuse_all {
                            json!({"ok": false, "errors": ["refused on purpose"]})
                        } else {
                            handle(&line, &submits)
                        };
                        let mut buf = reply.to_string();
                        buf.push('\n');
                        if w.write_all(buf.as_bytes()).await.is_err() {
                            break;
                        }
                    }
                });
            }
        });
        Self {
            path,
            submits,
            _dir: dir,
        }
    }
}

/// One of `work_item` / `goal` must scope the request (mirrors
/// `resolve_scope` in the engine's intake).
fn scoped(req: &Value) -> Result<(), Value> {
    // A conversation offers its own scope id as a candidate `goal`, because
    // a goal thread's messages are scoped by the goal's id. The engine
    // looks it up and refuses one that names no goal, so the fake refuses
    // any id but the one it knows — without that, every channel and DM here
    // would resolve to a goal and the scope tests would prove nothing.
    match req["goal"].as_str() {
        Some(id) if id != GOAL => return Err(json!({"ok": false, "errors": ["unknown goal"]})),
        Some(_) => return Ok(()),
        None => {}
    }
    if req["work_item"].is_string() {
        Ok(())
    } else {
        Err(json!({"ok": false, "errors": ["request needs `work_item` or `goal`"]}))
    }
}

fn handle(line: &str, submits: &AtomicU64) -> Value {
    let req: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => return json!({"ok": false, "errors": [format!("bad request: {e}")]}),
    };
    // A request carrying these words anywhere — in its scope, its text, its
    // gate — is refused on purpose: how a test reaches every tool's refusal
    // arm through one door.
    if line.contains("refuse me") {
        return json!({"ok": false, "errors": ["refused on purpose"]});
    }
    match req["op"].as_str() {
        Some("result_submit") => {
            submits.fetch_add(1, Ordering::SeqCst);
            if req["output"]["answer"].is_number() {
                json!({"ok": true, "result_event": "deadbeef"})
            } else {
                json!({"ok": false, "errors": ["\"answer\" is required and must be a number"], "attempts_left": 2})
            }
        }
        Some("progress") => {
            if req["verb"].as_str().unwrap_or("").is_empty() {
                json!({"ok": false, "errors": ["unknown work item"]})
            } else {
                json!({"ok": true})
            }
        }
        Some("ask_human") => {
            if let Err(e) = scoped(&req) {
                return e;
            }
            match req["expects"].as_str() {
                None | Some("decision") => json!({"ok": true, "gate": "gate-1"}),
                // The option list rides the same op; the gate id encodes which
                // canned resolution this test wants back.
                Some("answer") if req["question"].as_str() == Some("which?") => {
                    assert_eq!(req["options"][0]["id"], "a", "options reach the wire");
                    assert_eq!(req["multi"], true);
                    json!({"ok": true, "gate": "gate-options"})
                }
                Some("answer") if req["question"].as_str() == Some("unsure?") => {
                    json!({"ok": true, "gate": "gate-unsure"})
                }
                Some("answer") if req["question"].as_str() == Some("spent?") => {
                    json!({"ok": true, "gate": "gate-spent"})
                }
                Some("answer") => json!({"ok": true, "gate": "gate-text"}),
                Some(other) => {
                    json!({"ok": false, "errors": [format!("expects must be decision|answer, got {other:?}")]})
                }
            }
        }
        Some("await_decision") => match req["gate"].as_str() {
            Some("gate-1") => json!({"ok": true, "approve": true, "answer": null}),
            Some("gate-text") => {
                json!({"ok": true, "approve": true,
                       "answer": {"selected": [], "text": "ship it tuesday", "unsure": false}})
            }
            Some("gate-options") => {
                json!({"ok": true, "approve": true,
                       "answer": {"selected": ["a", "b"], "text": "but check the export",
                                  "unsure": false}})
            }
            Some("gate-unsure") => {
                json!({"ok": true, "approve": true,
                       "answer": {"selected": [], "text": null, "unsure": true},
                       "clarify_rounds_left": 2})
            }
            Some("gate-spent") => {
                json!({"ok": true, "approve": true,
                       "answer": {"selected": [], "text": null, "unsure": true},
                       "clarify_rounds_left": 0})
            }
            Some("gate-declined") => {
                json!({"ok": true, "approve": false,
                       "answer": {"selected": [], "text": "not like this", "unsure": false}})
            }
            Some("gate-denied") => json!({"ok": true, "approve": false, "answer": null}),
            _ => json!({"ok": false, "errors": ["unknown gate"]}),
        },
        Some("get_goal") => {
            if let Err(e) = scoped(&req) {
                return e;
            }
            if req["work_item"] == WORKSPACE_ITEM {
                return json!({"ok": false, "errors": [
                    "this work item belongs to a run of the workspace, which has no goal — orient with get_run"]});
            }
            json!({"ok": true, "goal": {"id": GOAL, "statement": "s", "status": "draft"},
                   "run": null, "work_items": [], "journal_tail": []})
        }
        // A worker's run: the engine reads it off the work item — a goal's
        // for any item but the one of a run of the workspace.
        Some("get_run") => match req["work_item"].as_str() {
            None => json!({"ok": false, "errors": ["request needs `work_item` or `run`"]}),
            Some("unreadable") => json!({"ok": false, "errors": ["unknown work item"]}),
            Some(WORKSPACE_ITEM) => json!({"ok": true,
                "run": {"id": RUN, "scope": "workspace", "status": "running", "steps": []},
                "home": format!("run:{RUN}"), "goal": null,
                "budget": {"max_tokens": 1000}, "spent": null,
                "work_items": [], "journal_tail": []}),
            Some(_) => json!({"ok": true,
                "run": {"id": RUN, "scope": "goal", "status": "running", "steps": []},
                "home": format!("goal:{GOAL}"), "goal": GOAL,
                "budget": {}, "spent": null, "work_items": [], "journal_tail": []}),
        },
        Some("revise_statement") => {
            assert!(req["goal"].is_string(), "revise_statement is goal-scoped");
            json!({"ok": true})
        }
        // -- the Workflow Agent's ops. The engine gates them by agent id; the
        // fake repeats the gate so a scope test here proves the id travels.
        Some("propose_workflow") => {
            assert!(req["goal"].is_string(), "propose_workflow is goal-scoped");
            if req["agent"] != WORKFLOW_AGENT_ID {
                return json!({"ok": false, "errors": [format!(
                    "only {WORKFLOW_AGENT_ID} may call this op; this session is {}", req["agent"])]});
            }
            assert!(
                req["workflow"]["steps"].is_array(),
                "the definition travels whole: {req}"
            );
            if req["workflow"]["name"] == "Broken" {
                json!({"ok": false, "errors": ["step a: flows to an unknown step \"nowhere\""],
                       "problems": [{"step": "a", "kind": "unknown_step",
                                     "message": "flows to an unknown step \"nowhere\""}]})
            } else {
                if req["workflow"]["name"] == json!("ungated") {
                    json!({"ok": true, "workflow": "01WORKFLOW", "revision": 1, "gate": null})
                } else {
                    json!({"ok": true, "workflow": "01WORKFLOW", "revision": 1, "gate": "01ADOPT"})
                }
            }
        }
        Some("amend_workflow") => {
            assert!(req["goal"].is_string());
            assert_eq!(req["agent"], WORKFLOW_AGENT_ID);
            if req["workflow"]["name"] == json!("ungated") {
                json!({"ok": true, "gate": null})
            } else {
                json!({"ok": true, "gate": "01AMEND"})
            }
        }
        Some("list_workflow_templates") => {
            assert_eq!(req["agent"], WORKFLOW_AGENT_ID);
            json!({"ok": true,
                   "templates": [{"kind": "workflow", "slug": "software-feature",
                                  "name": "Software feature", "description": "Design, build, check, ship.",
                                  "tags": ["engineering"], "requires": ["developer"], "installed": false}],
                   "workflows": [{"id": "01WORKFLOW", "name": "Ask first", "description": "one question",
                                  "steps": 1, "origin": "local", "tags": []}]})
        }
        Some("get_workflow") => {
            assert_eq!(req["agent"], WORKFLOW_AGENT_ID);
            json!({"ok": true, "installed": false,
                   "workflow": {"id": "01WORKFLOW", "name": "Software feature", "steps": []},
                   "problems": [{"step": "implement", "kind": "unknown_assignee",
                                 "message": "agent:developer is not installed"}],
                   "note": "a catalog template that is not installed"})
        }
        Some("validate_workflow") => {
            assert_eq!(req["agent"], WORKFLOW_AGENT_ID);
            // `goal` rides only when the session serves one, and is a goal id then.
            assert!(req.get("goal").is_none_or(Value::is_string), "{req}");
            if req["workflow"]["name"] == "Whose roster" {
                json!({"ok": true, "problems": [
                    {"kind": "staff", "message": format!("judged against {}", req["goal"].as_str().unwrap_or("the whole staff"))}
                ]})
            } else if req["workflow"]["name"] == "Broken" {
                json!({"ok": true, "problems": [
                    {"step": "a", "kind": "unknown_step", "message": "flows to an unknown step \"nowhere\""},
                    {"kind": "no_start", "message": "no step is without an incoming flow"}
                ]})
            } else {
                json!({"ok": true, "problems": []})
            }
        }
        // The one write to a library workflow: the scope names the
        // conversation, the definition travels whole, the revision is the one
        // read. The engine's rules (the conversation's workflow only, a
        // conflict when it moved) are the engine's tests'; the fake proves
        // the wire.
        Some("save_workflow") => {
            assert_eq!(req["agent"], WORKFLOW_AGENT_ID);
            assert!(req["scope"].is_string(), "the scope rides: {req}");
            assert!(
                req.get("workflow").is_none(),
                "no id: the conversation chooses: {req}"
            );
            assert!(
                req["definition"]["steps"].is_array(),
                "the definition travels whole: {req}"
            );
            if req["revision"] != json!(3) {
                json!({"ok": false, "errors": ["workflow 01WORKFLOW moved to revision 3 since you read revision 2 — read it again with get_workflow and save at that revision"]})
            } else if req["definition"]["name"] == "Broken" {
                json!({"ok": false, "errors": ["step a: flows to an unknown step \"nowhere\""],
                       "problems": [{"step": "a", "kind": "unknown_step",
                                     "message": "flows to an unknown step \"nowhere\""}]})
            } else {
                json!({"ok": true, "workflow": "01WORKFLOW", "revision": 4})
            }
        }
        Some("add_note") => {
            if let Err(e) = scoped(&req) {
                return e;
            }
            json!({"ok": true})
        }
        Some("spawn_sub_goal") => {
            if let Err(e) = scoped(&req) {
                return e;
            }
            json!({"ok": true, "child": "01CHILD"})
        }
        Some("emit_signal") => {
            // A signal is named and carries a payload; the session's scope
            // rides the envelope, the signal's own is said apart.
            assert!(req["name"].is_string(), "a signal is named: {req}");
            assert!(req.get("topic").is_none(), "{req}");
            match req["name"].as_str() {
                Some("Not A Name") => {
                    json!({"ok": false, "errors": ["`Not A Name` is not a signal name"]})
                }
                Some("nobody.listens") => {
                    json!({"ok": true, "signal": "01RECORD", "listeners": []})
                }
                _ => {
                    assert_eq!(req["payload"]["status"], "green", "{req}");
                    assert_eq!(req["signal_scope"], format!("goal:{GOAL}"), "{req}");
                    json!({"ok": true, "signal": "01RECORD",
                           "listeners": ["01HEARD", "01ALSO"]})
                }
            }
        }
        Some("post_message") => {
            assert!(req["scope"].is_string(), "post_message needs a scope ULID");
            // Who speaks rides with the post: the agent, else the work item
            // the engine resolves one from — never nobody.
            assert!(
                req["agent"].is_string() || req["work_item"].is_string(),
                "a post says who speaks: {req}"
            );
            if req["content"] == "who speaks?" {
                return json!({"ok": true, "message": format!(
                    "spoken-by-{}",
                    req["agent"].as_str().or(req["work_item"].as_str()).unwrap_or_default()
                )});
            }
            // A worker of a run of the workspace, naming no scope, speaks in
            // `general`; any other worker in its goal's thread.
            match req["content"].as_str() {
                Some("from the run") => assert_eq!(req["scope"], "general", "{req}"),
                Some("from the goal's run") => assert_eq!(req["scope"], GOAL, "{req}"),
                _ => {}
            }
            // An artifact rides as `{path, title}`; the field is absent when
            // none was given, never an empty list.
            if let Some(arts) = req.get("artifacts") {
                let arts = arts.as_array().expect("artifacts is a list");
                assert!(!arts.is_empty(), "an empty list is never sent");
                for a in arts {
                    assert!(a["path"].is_string(), "an artifact names a path: {a}");
                }
                return json!({"ok": true, "message": format!("msg-with-{}-artifacts", arts.len())});
            }
            json!({"ok": true, "message": "msg-1"})
        }
        Some("recall_store") => {
            if req["agent"].is_null() && req["work_item"].is_null() {
                return json!({"ok": false, "errors": ["recall requires an agent identity (pass `agent` or a work item run by one)"]});
            }
            if req["slug"] == "style" && req["base_hash"] != "r1" {
                json!({"ok": false,
                       "errors": ["the memory changed since you read it — merge and retry with base_hash"],
                       "current_value": "tabs, not spaces", "current_hash": "r1"})
            } else {
                json!({"ok": true, "hash": "r2"})
            }
        }
        Some("recall_get") => {
            if req["slug"] == "style" {
                json!({"ok": true, "slug": "style", "value": "tabs, not spaces",
                       "hash": "r1", "links": ["conventions"]})
            } else {
                json!({"ok": true, "value": null})
            }
        }
        Some("recall_list") => json!({"ok": true, "records": [
            {"slug": "style", "hash": "r1", "updated_at": 1, "links": []}
        ]}),
        // Every session's to read: the fake says who it heard asking, so a
        // test reads what the request carried.
        Some("list_connectors") => {
            let who = req["agent"]
                .as_str()
                .map(|agent| format!("agent {agent}"))
                .or_else(|| req["work_item"].as_str().map(|item| format!("item {item}")))
                .unwrap_or_else(|| "nobody named".to_string());
            json!({"ok": true, "connectors": [], "text": format!("CONNECTORS — asked by {who}")})
        }
        // -- platform ops (M9). Shapes copied from the wire contract; the
        // asserts are the half of that contract this crate is responsible for.
        // Who may be named: the goal's roster when the session serves one.
        Some("list_staff") => {
            assert!(req.get("goal").is_none_or(Value::is_string), "{req}");
            let whose = req["goal"].as_str().unwrap_or("the whole staff");
            json!({"ok": true, "agents": [], "teams": [], "scoped": req.get("goal").is_some(),
                   "text": format!("STAFF for {whose}")})
        }
        Some("workspace_overview") => {
            assert!(
                [CORE_AGENT_ID, WORKFLOW_AGENT_ID].contains(&req["agent"].as_str().unwrap_or("")),
                "core ops name their signer: {req}"
            );
            json!({"ok": true,
                   "agents": {"total": 4, "enabled": 3, "core": [CORE_AGENT_ID, WORKFLOW_AGENT_ID],
                              "installed": [CORE_AGENT_ID, WORKFLOW_AGENT_ID, "developer", "reviewer"]},
                   "teams": [{"id": "engineering", "name": "Engineering",
                              "agents": 2, "humans": 1}],
                   "channels": [{"id": "01CHANNEL", "name": "Standup",
                                "roster": "listed", "members": 2}],
                   "skills": 4,
                   "mcp_servers": {"total": 2, "enabled": 1},
                   "listening": {"workflows": 1, "goals": 2, "paused": 1, "listeners": 4,
                                 "next_due": 1_700_000_000u64},
                   "projects": [{"id": "01PROJ", "slug": "web",
                                 "goals": [GOAL], "vcs": "git"}],
                   "goals": {"by_status": {"draft": 2, "running": 1},
                               "waiting_on_human": [GOAL]},
                   "workflows": {"total": 3, "catalog": 2},
                   "running": [{"agent": "developer", "kind": "work_item",
                                "work_item": "01WI"}],
                   "catalog": {"agent": {"total": 29, "installed": 3},
                               "skill": {"total": 40, "installed": 4},
                               "team": {"total": 9, "installed": 1},
                               "channel": {"total": 8, "installed": 1},
                               "workflow": {"total": 12, "installed": 2}}})
        }
        Some("list_catalog") => {
            assert!(
                [CORE_AGENT_ID, WORKFLOW_AGENT_ID].contains(&req["agent"].as_str().unwrap_or(""))
            );
            if req["kind"] == "team" {
                return json!({"ok": true, "entries": []});
            }
            json!({"ok": true, "entries": [
                {"kind": "agent", "slug": "developer", "name": "Developer",
                 "description": "Writes code.", "tags": ["engineering"],
                 "requires": ["rust"], "installed": true},
                {"kind": "agent", "slug": "assayer", "name": "Assayer",
                 "description": "Judges results.", "tags": [], "requires": [],
                 "installed": false}
            ]})
        }
        Some("install_catalog_entry") => {
            assert_eq!(req["agent"], CORE_AGENT_ID);
            if req["slug"] == "already-here" {
                json!({"ok": true, "installed": {"agents": [], "skills": [],
                                                 "teams": [], "channels": [], "workflows": []}})
            } else {
                json!({"ok": true, "installed": {"agents": ["developer", "reviewer"],
                                                 "skills": ["rust"],
                                                 "teams": ["engineering"], "channels": [],
                                                 "workflows": []}})
            }
        }
        Some("assign") => {
            assert_eq!(req["agent"], CORE_AGENT_ID);
            assert!(req["goal"].is_string(), "assign names the goal");
            json!({"ok": true, "assignees": ["agent:developer", "team:engineering"]})
        }
        Some("capture_goal") => {
            assert_eq!(req["agent"], CORE_AGENT_ID);
            // The statement as the caller wrote it, trimmed; a title or null.
            assert_eq!(
                req["statement"], "Every Monday at 09:00, post the digest",
                "{req}"
            );
            assert!(req["title"].is_string() || req["title"].is_null(), "{req}");
            json!({"ok": true, "goal": "01STANDING"})
        }
        Some("create_project") => {
            assert_eq!(req["agent"], CORE_AGENT_ID);
            // A project belongs to the workspace; `goal` is the optional
            // attachment, and null is how "none" travels.
            assert!(req["goal"].is_string() || req["goal"].is_null(), "{req}");
            json!({"ok": true, "project": "01PROJECT", "slug": req["slug"],
                   "path": "/tmp/ws/storefront", "attached_to": req["goal"]})
        }
        // -- the ops the real server answers and a session reads back as
        // words: the shapes are the engine's (`intake.rs`), the words the
        // server's.
        Some("note_read") => json!({"ok": true, "title": "Plan", "body": "# Plan\n", "hash": "n1"}),
        Some("note_append") => json!({"ok": true, "title": "Plan"}),
        Some("note_write") => {
            if req["base_hash"] == "same" {
                json!({"ok": true, "title": "Plan", "hash": "n1", "changed": false})
            } else {
                json!({"ok": true, "title": "Plan", "hash": "n2", "changed": true})
            }
        }
        Some("review_notes_list") => {
            if req["include_resolved"] == true {
                json!({"ok": true, "notes": [
                    {"id": "01NOTE", "path": "src/a.rs", "range": {"start": 3, "end": 3},
                     "scope": {"scope": "branch", "base": "main"}, "body": "rename this",
                     "hunk": "-a\n+b"},
                    {"id": "01OLD", "path": "src/b.rs", "range": {"start": 1, "end": 4},
                     "scope": {"scope": "working"}, "body": "done", "resolved_at": 7},
                    {"id": "01BARE", "path": "src/c.rs", "body": "?"}
                ]})
            } else {
                json!({"ok": true, "notes": []})
            }
        }
        Some("review_note_resolve") => {
            json!({"ok": true, "note": {"id": req["note"], "path": "src/a.rs"}})
        }
        Some("pr_reviews_list") => json!({"ok": true, "reviews": [], "threads": []}),
        Some("pr_review_submit") | Some("pr_thread_reply") | Some("pr_thread_resolve") => {
            json!({"ok": true})
        }
        Some("browser_serve") => json!({"ok": true, "result": {
            "folder": req["folder"], "url": "http://127.0.0.1:1", "page": "http://127.0.0.1:1/index.html"}}),
        Some("call_connector") => {
            assert!(
                req["account"].is_string() || req["account"].is_null(),
                "{req}"
            );
            json!({"ok": true, "output": {"issues": 1}, "truncated": false})
        }
        Some("decide") => {
            let sure = req["request"]["state"]["sure"] != false;
            json!({"ok": true, "result": {"sure": sure,
                   "response": {"answers": {"q": {"type": "noul", "noul": 0.9}}}}})
        }
        Some("mobile_development") => match req["request"]["action"].as_str() {
            Some("status") => json!({"ok": true, "result": {"enabled": false, "platforms": "both",
                "toolchain": {"flutter": {"installed": true}, "xcode": {"version": "16.0"},
                              "android": {}, "doctor": [{"state": "ok", "name": "Flutter"}]}}}),
            Some("devices") => json!({"ok": true, "result": {"devices": []}}),
            Some("boot") => json!({"ok": true, "result": {"device": {"id": "A-1", "name": "iPhone",
                "platform": "ios", "kind": "simulator", "state": "booted"}}}),
            Some("screenshot") => {
                json!({"ok": true, "result": {"path": "/shots/a.png", "width": 1, "height": 2}})
            }
            _ => json!({"ok": false, "errors": ["unknown action"]}),
        },
        Some("draw") => match req["request"]["action"].as_str() {
            Some("snapshot") => {
                json!({"ok": true, "result": {"ok": true, "path": "/snaps/d.png", "width": 3, "height": 4}})
            }
            Some("erase") => json!({"ok": true, "result": {"ok": true}}),
            Some("list") if req["request"]["scope_id"].is_string() => {
                json!({"ok": true, "result": {"ok": true, "drawings": [
                {"id": "01DRAWING", "scope": {"kind": "goal", "id": GOAL}, "title": "Orders", "pinned": false,
                 "hash": "abc", "element_count": 3, "created_at": 1, "updated_at": 2}]}})
            }
            Some("list") => json!({"ok": true, "result": {"ok": true, "drawings": [
                {"id": "01DRAWING", "scope": {"kind": "workspace"}, "title": "Orders", "pinned": false,
                 "hash": "abc", "element_count": 3, "created_at": 1, "updated_at": 2}]}}),
            Some("read") => {
                json!({"ok": true, "result": {"ok": true, "drawing": "01DRAWING", "title": "Orders",
                "hash": "abc", "element_count": 1, "description": "a rectangle \"API\" at 0,0 160×80\n"}})
            }
            _ => {
                json!({"ok": true, "result": {"ok": false, "error": "the canvas is not available — the desktop app draws, and none is open"}})
            }
        },
        Some("browser") => {
            // The client's contract: a conversation's scope id rides as the
            // candidate `goal`; the engine, not this fake, says what it is.
            if let Some(scope) = req["scope"].as_str() {
                assert_eq!(req["goal"], json!(scope), "{req}");
            }
            match req["request"]["action"].as_str() {
                Some("boom") => json!({"ok": false, "errors": [format!(
                    "{}the engine could not read this `browser` request (missing field `request`) — not a policy refusal; report it to the person and continue the rest of the task without this tool",
                    bisa_core::browser::PLATFORM_FAULT
                )]}),
                Some("tabs") => {
                    json!({"ok": true, "result": {"ok": true, "tabs": [{"key": "b1", "url": "https://example.test/", "title": "Example"}]}})
                }
                _ => {
                    json!({"ok": true, "result": {"ok": false, "error": "the page did not answer — it may have navigated, or it is still loading; try again"}})
                }
            }
        }
        _ => json!({"ok": false, "errors": ["bad request: unknown op"]}),
    }
}

/// The text of a refusal. Every refusal is a tool *error* (B5): a harness
/// renders it as one, and the model cannot mistake it for a result.
fn refusal(r: Result<String, rmcp::ErrorData>) -> String {
    r.expect_err("a refusal is an error, not a result")
        .message
        .to_string()
}

fn wi_scope() -> Scope {
    Scope::WorkItem("wi".to_string())
}

fn goal_scope() -> Scope {
    Scope::Goal {
        goal: GOAL.to_string(),
        agent: None,
    }
}

/// The General Agent driving a goal — the scope the platform tools run in.
fn core_agent_scope() -> Scope {
    Scope::Goal {
        goal: GOAL.to_string(),
        agent: Some(CORE_AGENT_ID.to_string()),
    }
}

/// The Workflow Agent designing a goal's workflow.
fn workflow_agent_scope() -> Scope {
    Scope::Goal {
        goal: GOAL.to_string(),
        agent: Some(WORKFLOW_AGENT_ID.to_string()),
    }
}

/// A definition that validates; the fake knows it by name.
fn good_workflow() -> Value {
    json!({"name": "Ask first", "steps": [
        {"id": "ask", "name": "Ask", "kind": "human", "prompt": "which?"}
    ]})
}

/// A definition the fake records without a gate — an auto or a manual goal.
fn ungated_workflow() -> Value {
    json!({"name": "ungated", "steps": [
        {"id": "ask", "name": "Ask", "kind": "human", "prompt": "which?"}
    ]})
}

/// A definition the fake answers with problems.
fn broken_workflow() -> Value {
    json!({"name": "Broken", "steps": [
        {"id": "a", "name": "A", "kind": "human", "prompt": "?", "then": ["nowhere"]}
    ]})
}

/// A session serving a goal reads the goal's roster: `list_staff` and
/// `validate_workflow` carry the session's goal to the engine, with no
/// argument the agent has to remember. A caller with no goal sends none, and
/// reads the whole staff.
#[tokio::test]
async fn reading_the_staff_carries_the_sessions_goal_and_only_then() {
    let fake = FakeIntake::start();
    let designer = ToolCore::new(fake.path.clone(), workflow_agent_scope());
    let staff = designer.list_staff().await.unwrap();
    assert_eq!(staff, format!("STAFF for {GOAL}"));
    let whose = json!({"name": "Whose roster", "steps": []});
    let judged = designer.validate_workflow(whose.clone()).await.unwrap();
    assert!(
        judged.contains(&format!("judged against {GOAL}")),
        "{judged}"
    );

    let client = IntakeClient::new(fake.path.clone());
    let whole = client
        .list_staff(CORE_AGENT_ID, None)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(whole["scoped"], json!(false));
    assert_eq!(whole["text"], json!("STAFF for the whole staff"));
    let problems = client
        .validate_workflow(WORKFLOW_AGENT_ID, whose, None)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        problems[0]["message"],
        json!("judged against the whole staff")
    );
}

#[tokio::test]
async fn submit_ok_and_rejected() {
    let fake = FakeIntake::start();
    let client = IntakeClient::new(fake.path.clone());

    let ok = client
        .result_submit("wi", json!({"answer": 42}))
        .await
        .unwrap();
    assert_eq!(
        ok,
        SubmitOutcome::Accepted {
            result_event: "deadbeef".into()
        }
    );

    let rejected = client
        .result_submit("wi", json!({"wrong": true}))
        .await
        .unwrap();
    match rejected {
        SubmitOutcome::Rejected {
            errors,
            attempts_left,
        } => {
            assert_eq!(attempts_left, 2);
            assert!(errors[0].contains("must be a number"));
        }
        other => panic!("expected rejection, got {other:?}"),
    }
    assert_eq!(fake.submits.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn ask_and_await_with_answers() {
    let fake = FakeIntake::start();
    let client = IntakeClient::new(fake.path.clone());

    // Decision gate (worker scope on the wire).
    let gate = client
        .ask_human(&wi_scope(), "deploy to prod?", None, &[], false)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(gate, "gate-1");
    assert_eq!(
        client.await_decision(&gate).await.unwrap(),
        Ok(Decision {
            approve: true,
            answer: None,
            clarify_rounds_left: None,
        })
    );

    // Answer gate with no options (goal scope on the wire) carries the text.
    let gate = client
        .ask_human(
            &goal_scope(),
            "when should we ship?",
            Some("answer"),
            &[],
            false,
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(gate, "gate-text");
    assert_eq!(
        client.await_decision(&gate).await.unwrap(),
        Ok(Decision {
            approve: true,
            answer: Some(Answer::text("ship it tuesday")),
            clarify_rounds_left: None,
        })
    );

    // Options go out; a multi-select and free text come back together.
    let gate = client
        .ask_human(
            &goal_scope(),
            "which?",
            Some("answer"),
            &[AskOption::new("a", "A"), AskOption::new("b", "B")],
            true,
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        client.await_decision(&gate).await.unwrap(),
        Ok(Decision {
            approve: true,
            answer: Some(Answer::selecting(["a", "b"]).with_text("but check the export")),
            clarify_rounds_left: None,
        })
    );

    // "I'm not sure" comes back as an answer with a clarify budget, never as
    // a decline.
    let gate = client
        .ask_human(&goal_scope(), "unsure?", Some("answer"), &[], false)
        .await
        .unwrap()
        .unwrap();
    let d = client.await_decision(&gate).await.unwrap().unwrap();
    assert!(d.is_unsure());
    assert!(d.approve, "not-sure is not a decline");
    assert_eq!(d.clarify_rounds_left, Some(2));

    assert_eq!(
        client.await_decision("nope").await.unwrap(),
        Err(vec!["unknown gate".to_string()])
    );
}

#[tokio::test]
async fn goal_ops_roundtrip() {
    let fake = FakeIntake::start();
    let client = IntakeClient::new(fake.path.clone());

    let v = client.get_goal(&wi_scope()).await.unwrap().unwrap();
    assert_eq!(v["goal"]["id"], GOAL);

    client
        .revise_statement(GOAL, "sharper", Some("clearer scope"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        client
            .propose_workflow(GOAL, WORKFLOW_AGENT_ID, good_workflow())
            .await
            .unwrap(),
        Ok(Proposed {
            workflow: "01WORKFLOW".to_string(),
            revision: 1,
            gate: Some("01ADOPT".to_string()),
        })
    );
    // A refused proposal carries the problems beside the errors.
    let refused = client
        .propose_workflow(GOAL, WORKFLOW_AGENT_ID, broken_workflow())
        .await
        .unwrap()
        .unwrap_err();
    assert!(refused[0].contains("unknown step"), "{refused:?}");
    assert_eq!(
        client
            .amend_workflow(GOAL, WORKFLOW_AGENT_ID, good_workflow())
            .await
            .unwrap(),
        Ok(Some("01AMEND".to_string()))
    );
    let templates = client
        .list_workflow_templates(WORKFLOW_AGENT_ID)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(templates["templates"][0]["slug"], "software-feature");
    assert_eq!(
        client
            .validate_workflow(WORKFLOW_AGENT_ID, broken_workflow(), None)
            .await
            .unwrap()
            .unwrap()
            .len(),
        2
    );
    let about_workflow = Scope::Conversation {
        scope: "01WORKFLOWCONVERSATION".to_string(),
        agent: WORKFLOW_AGENT_ID.to_string(),
        goal: None,
    };
    assert_eq!(
        client
            .save_workflow(&about_workflow, 3, good_workflow())
            .await
            .unwrap(),
        Ok(Saved {
            workflow: "01WORKFLOW".to_string(),
            revision: 4,
        })
    );
    let moved = client
        .save_workflow(&about_workflow, 2, good_workflow())
        .await
        .unwrap()
        .unwrap_err();
    assert!(moved[0].contains("moved to revision 3"), "{moved:?}");
    client
        .add_note(&goal_scope(), "context")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        client
            .spawn_sub_goal(&wi_scope(), "child work", None)
            .await
            .unwrap(),
        Ok("01CHILD".to_string())
    );
}

#[tokio::test]
async fn recall_guarded_writes() {
    let fake = FakeIntake::start();
    let client = IntakeClient::new(fake.path.clone());

    // Recall conflict carries the current value for merging.
    match client
        .recall_store(Some("builder"), None, "style", "spaces", None)
        .await
        .unwrap()
    {
        Ok(GuardedWrite::Stale {
            current_value,
            current_hash,
            ..
        }) => {
            assert_eq!(current_value.as_deref(), Some("tabs, not spaces"));
            assert_eq!(current_hash, "r1");
        }
        other => panic!("expected conflict, got {other:?}"),
    }
    assert_eq!(
        client
            .recall_store(Some("builder"), None, "style", "merged", Some("r1"))
            .await
            .unwrap(),
        Ok(GuardedWrite::Written { hash: "r2".into() })
    );
    let got = client
        .recall_get(Some("builder"), None, "style")
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(got.0, "tabs, not spaces");
    assert_eq!(got.2, vec!["conventions".to_string()]);
    assert_eq!(
        client
            .recall_list(Some("builder"), None)
            .await
            .unwrap()
            .unwrap()
            .len(),
        1
    );

    // No identity at all → intake's explanatory error: a refusal, not a
    // conflict — there is nothing to merge, and the tool says so as an error.
    let err = client
        .recall_store(None, None, "style", "x", None)
        .await
        .unwrap();
    match err {
        Err(errors) => assert!(errors[0].contains("agent identity"), "{errors:?}"),
        other => panic!("expected identity error, got {other:?}"),
    }
    let core = ToolCore::new(fake.path.clone(), goal_scope());
    let refused = refusal(core.recall_store("style", "x", None).await);
    assert!(refused.contains("agent identity"), "{refused}");
}

#[tokio::test]
async fn connect_failure_is_typed() {
    let client = IntakeClient::new(PathBuf::from("/nonexistent/bisa-test.sock"));
    let err = client.result_submit("wi", json!({})).await.unwrap_err();
    assert!(err.to_string().contains("intake socket unavailable"));
}

#[tokio::test]
async fn tool_core_texts() {
    let fake = FakeIntake::start();
    let core = ToolCore::new(fake.path.clone(), wi_scope());

    let accepted = core.yield_result(json!({"answer": 1})).await.unwrap();
    assert!(accepted.contains("Result accepted"));

    let rejected = refusal(core.yield_result(json!({})).await);
    assert!(rejected.contains("REJECTED"), "{rejected}");
    assert!(rejected.contains("2 attempts left"), "{rejected}");

    let progress = core.report_progress("fixed", "parser", None).await.unwrap();
    assert!(progress.contains("recorded"));

    let combined = core.ask_human_and_wait(&ask("ship it?")).await.unwrap();
    assert!(combined.contains("APPROVED"), "{combined}");

    // Worker scope: goal-mutating tools refuse, and the error says who may
    // shape a goal — a session driving one.
    let refused = refusal(core.revise_statement("x", None).await);
    assert!(refused.contains("shapes a goal"), "{refused}");
    assert!(refused.contains("session driving one"), "{refused}");

    // post_message defaults to the session's goal (lazily resolved).
    let posted = core
        .post_message(None, "hello", None, &[], &[], &[])
        .await
        .unwrap();
    assert!(posted.contains("Message posted"), "{posted}");
    assert!(
        posted.contains("msg-1"),
        "no artifacts field when none: {posted}"
    );

    // An artifact goes on the wire as {path, title}, title optional.
    let posted = core
        .post_message(
            None,
            "the chart",
            None,
            &[],
            &[],
            &[
                bisa_mcp::ArtifactParam {
                    path: "out/chart.svg".into(),
                    title: Some("Chart".into()),
                },
                bisa_mcp::ArtifactParam {
                    path: "notes.md".into(),
                    title: None,
                },
            ],
        )
        .await
        .unwrap();
    assert!(posted.contains("msg-with-2-artifacts"), "{posted}");
}

#[tokio::test]
async fn goal_scoped_core_texts() {
    let fake = FakeIntake::start();
    let core = ToolCore::new(fake.path.clone(), goal_scope());

    let answered = core
        .ask_human_and_wait(&ask_answer("when?", vec![], false))
        .await
        .unwrap();
    assert!(answered.contains("ship it tuesday"), "{answered}");

    // The escape hatch that used to read as a denial. Both arms must steer:
    // one to a narrower question, the other to proceeding on a recorded
    // assumption. Neither may tell the agent to stop.
    let unsure = core
        .ask_human_and_wait(&ask_answer("unsure?", vec![], false))
        .await
        .unwrap();
    assert!(unsure.contains("NOT SURE"), "{unsure}");
    assert!(unsure.contains("not a refusal"), "{unsure}");
    assert!(unsure.contains("narrower"), "{unsure}");
    assert!(unsure.contains("2 clarification round"), "{unsure}");
    assert!(
        !unsure.contains("Do not proceed"),
        "\"I'm not sure\" must never read as a denial: {unsure}"
    );

    let spent = core
        .ask_human_and_wait(&ask_answer("spent?", vec![], false))
        .await
        .unwrap();
    assert!(spent.contains("no clarification rounds"), "{spent}");
    assert!(spent.contains("add_note"), "{spent}");
    assert!(!spent.contains("Do not proceed"), "{spent}");

    // A selection and free text are both reported — dropping either half
    // loses the part that changes what the agent does next.
    let picked = core
        .ask_human_and_wait(&ask_answer(
            "which?",
            vec![
                AskOptionParam {
                    id: "a".into(),
                    label: "A".into(),
                    detail: None,
                    recommended: false,
                },
                AskOptionParam {
                    id: "b".into(),
                    label: "B".into(),
                    detail: None,
                    recommended: false,
                },
            ],
            true,
        ))
        .await
        .unwrap();
    assert!(picked.contains("They chose: a, b."), "{picked}");
    assert!(picked.contains("but check the export"), "{picked}");

    // A goal session that is not the Workflow Agent is refused the proposal
    // by the engine, and the refusal names who may.
    let refused = refusal(core.propose_workflow(good_workflow()).await);
    assert!(refused.contains("no agent identity"), "{refused}");
}

/// The Workflow Agent's texts: a proposal says who adopts it, a refusal lists
/// problems by step, the templates list is readable and a definition comes
/// back as JSON with its problems.
#[tokio::test]
async fn workflow_agent_texts_render_problems_by_step() {
    let fake = FakeIntake::start();
    let core = ToolCore::new(fake.path.clone(), workflow_agent_scope());

    let proposed = core.propose_workflow(good_workflow()).await.unwrap();
    assert!(proposed.contains("01WORKFLOW"), "{proposed}");
    assert!(proposed.contains("Adopt gate (01ADOPT)"), "{proposed}");
    assert!(
        proposed.contains("you do not adopt it yourself"),
        "{proposed}"
    );
    // On an auto or a manual goal no gate opens, and the reply says so
    // instead of naming one that does not exist.
    let ungated = core.propose_workflow(ungated_workflow()).await.unwrap();
    assert!(ungated.contains("no gate opened"), "{ungated}");
    assert!(!ungated.contains("Adopt gate"), "{ungated}");

    let refused = refusal(core.propose_workflow(broken_workflow()).await);
    assert!(refused.contains("Not done"), "{refused}");
    assert!(refused.contains("unknown step"), "{refused}");

    let amended = core.amend_workflow(good_workflow()).await.unwrap();
    assert!(amended.contains("01AMEND"), "{amended}");

    let listed = core.list_workflow_templates().await.unwrap();
    assert!(listed.contains("CATALOG TEMPLATES (1)"), "{listed}");
    assert!(listed.contains("[available] software-feature"), "{listed}");
    assert!(listed.contains("(tags: engineering)"), "{listed}");
    assert!(
        listed.contains("THIS WORKSPACE'S WORKFLOWS (1)"),
        "{listed}"
    );
    assert!(
        listed.contains("01WORKFLOW \"Ask first\" (1 steps, local)"),
        "{listed}"
    );

    let read = core.get_workflow("software-feature").await.unwrap();
    assert!(read.contains("\"name\": \"Software feature\""), "{read}");
    assert!(
        read.contains("Note: a catalog template that is not installed"),
        "{read}"
    );
    assert!(read.contains("1 problem(s):"), "{read}");
    assert!(
        read.contains("- step implement: unknown_assignee — agent:developer is not installed"),
        "{read}"
    );
    let blank = refusal(core.get_workflow("  ").await);
    assert!(blank.contains("Name the workflow"), "{blank}");

    let valid = core.validate_workflow(good_workflow()).await.unwrap();
    assert!(valid.contains("Valid: no problems"), "{valid}");
    let invalid = core.validate_workflow(broken_workflow()).await.unwrap();
    assert!(invalid.contains("2 problem(s):"), "{invalid}");
    assert!(invalid.contains("- step a: unknown_step"), "{invalid}");
    assert!(
        invalid.contains("- no_start — no step is without an incoming flow"),
        "a problem with no step is about the whole definition: {invalid}"
    );

    // The one write, from the conversation about the workflow: the reply
    // names the revision now stored; a definition with problems and a
    // workflow that moved are refused in the engine's words.
    let about_workflow = ToolCore::new(
        fake.path.clone(),
        Scope::Conversation {
            scope: "01WORKFLOWCONVERSATION".to_string(),
            agent: WORKFLOW_AGENT_ID.to_string(),
            goal: None,
        },
    );
    let saved = about_workflow
        .save_workflow(good_workflow(), 3)
        .await
        .unwrap();
    assert!(
        saved.contains("Saved workflow 01WORKFLOW as revision 4"),
        "{saved}"
    );
    let refused = refusal(about_workflow.save_workflow(broken_workflow(), 3).await);
    assert!(
        refused.contains("Not done") && refused.contains("unknown step"),
        "{refused}"
    );
    let moved = refusal(about_workflow.save_workflow(good_workflow(), 2).await);
    assert!(moved.contains("moved to revision 3"), "{moved}");

    // The shared tools answer the Workflow Agent too.
    let overview = core.workspace_overview().await.unwrap();
    assert!(
        overview.contains("WORKFLOWS: 3 installed (2 from the catalog)"),
        "{overview}"
    );
    let catalog = core.list_catalog(Some("workflow")).await.unwrap();
    assert!(catalog.contains("[installed] agent/developer"), "{catalog}");
}

#[tokio::test]
async fn platform_tools_summarise_rather_than_dump() {
    let fake = FakeIntake::start();
    let core = ToolCore::new(fake.path.clone(), core_agent_scope());

    // Every section of the overview has to survive the rendering: an agent
    // that read a summary with a section missing stops looking for it.
    let overview = core.workspace_overview().await.unwrap();
    for expected in [
        "4 installed, 3 enabled",
        &format!("core agents {CORE_AGENT_ID}, {WORKFLOW_AGENT_ID}"),
        "engineering",
        "2 agents, 1 humans",
        "Standup",
        "SKILLS: 4",
        "MCP SERVERS: 2 (1 enabled)",
        "LISTENING: 1 workflow on, 2 goals (1 paused); 4 starts armed",
        "next due at epoch 1700000000",
        "attached to goals: ",
        "draft 2",
        "Waiting on a human",
        "WORKFLOWS: 3 installed (2 from the catalog)",
        "developer on work_item 01WI",
        "agent 29/3",
        "channel 8/1",
        "workflow 12/2",
    ] {
        assert!(
            overview.contains(expected),
            "missing {expected:?}:\n{overview}"
        );
    }

    let catalog = core.list_catalog(Some("agent")).await.unwrap();
    assert!(catalog.contains("[installed] agent/developer"), "{catalog}");
    assert!(catalog.contains("[available] agent/assayer"), "{catalog}");
    assert!(catalog.contains("(tags: engineering)"), "{catalog}");

    // The install names everything it created, not just the entry asked for.
    let installed = core
        .install_catalog_entry("team", "engineering", None)
        .await
        .unwrap();
    assert!(
        installed.contains("agents developer, reviewer"),
        "{installed}"
    );
    assert!(installed.contains("skills rust"), "{installed}");
    let already = core
        .install_catalog_entry("team", "already-here", None)
        .await
        .unwrap();
    assert!(already.contains("already installed"), "{already}");

    let assigned = core
        .assign(
            None,
            None,
            &[
                "agent:developer".to_string(),
                "team:engineering".to_string(),
            ],
            false,
        )
        .await
        .unwrap();
    assert!(assigned.contains(GOAL), "{assigned}");
    assert!(
        assigned.contains("agent:developer, team:engineering"),
        "{assigned}"
    );

    let captured = core
        .capture_goal(StandingGoal {
            statement: "  Every Monday at 09:00, post the digest  ".to_string(),
            title: Some("  ".to_string()),
        })
        .await
        .unwrap();
    assert!(captured.contains("01STANDING"), "{captured}");
    assert!(
        captured.contains("The Workflow Agent designs"),
        "{captured}"
    );
    assert!(captured.contains("listens once"), "{captured}");

    let project = core
        .create_project(None, "storefront", None, &[])
        .await
        .unwrap();
    assert!(project.contains("01PROJECT"), "{project}");
    assert!(project.contains("storefront"), "{project}");
    assert!(project.contains("git repository"), "{project}");
}

/// `create_project` in a conversation with no goal still creates the
/// project — attached to nothing, and the result says so.
///
/// A project belongs to the workspace; a goal is what it gets
/// attached to when the session has one. The core agent also answers in
/// channels and DMs, where there is none, and refusing it somewhere to put
/// files there would send it back to `git init` in a scratch folder.
#[tokio::test]
async fn create_project_in_a_conversation_attaches_to_nothing_and_says_so() {
    let fake = FakeIntake::start();
    let core = ToolCore::new(
        fake.path.clone(),
        Scope::Conversation {
            scope: "01CHANNEL".to_string(),
            agent: CORE_AGENT_ID.to_string(),
            goal: None,
        },
    );

    let unattached = core
        .create_project(None, "storefront", None, &[])
        .await
        .expect("a project needs no goal");
    assert!(unattached.contains("01PROJECT"), "{unattached}");
    assert!(
        unattached.contains("attached to no goal"),
        "the result says the project hangs off nothing: {unattached}"
    );

    // Naming a goal from the same scope attaches it, and the result names it.
    let attached = core
        .create_project(Some(GOAL), "storefront", None, &[])
        .await
        .unwrap();
    assert!(attached.contains(GOAL), "{attached}");
    assert!(attached.contains("attached to goal"), "{attached}");
}

/// Bad input is refused as an error whose message says what was wrong. The
/// sentence is the same one the old string result carried; what changed is
/// that a harness now renders it as a failure.
#[tokio::test]
async fn platform_tools_refuse_bad_input_with_an_error_that_explains() {
    let fake = FakeIntake::start();
    let core = ToolCore::new(fake.path.clone(), core_agent_scope());

    // A word that is no kind, and a kind of the catalog these tools do not
    // take, are refused alike — by what the tool takes, never by saying the
    // catalog has no such kind: it has seven, and lists them all unasked.
    for word in ["plugin", "connector", "addon"] {
        let kind = refusal(core.list_catalog(Some(word)).await);
        assert!(
            kind.contains("not a kind this tool narrows to")
                && kind.contains("omit kind to see the whole catalog"),
            "{kind}"
        );
        let install = refusal(core.install_catalog_entry(word, "anything", None).await);
        assert!(
            install.contains("not a kind this tool installs"),
            "{install}"
        );
    }

    let slug = refusal(core.install_catalog_entry("agent", "   ", None).await);
    assert!(slug.contains("catalog slug"), "{slug}");

    let empty = refusal(core.assign(None, None, &[], false).await);
    assert!(empty.contains("at least one assignee"), "{empty}");

    // A bare id is ambiguous between the three kinds, and the sentence names
    // the offender rather than rejecting the whole list.
    let bad = refusal(
        core.assign(
            None,
            None,
            &["agent:developer".to_string(), "developer".to_string()],
            false,
        )
        .await,
    );
    assert!(bad.contains("\"developer\" is not an assignee"), "{bad}");
    assert!("agent:developer".parse::<Assignee>().is_ok());

    let no_slug = refusal(core.create_project(None, " ", None, &[]).await);
    assert!(no_slug.contains("needs a slug"), "{no_slug}");

    let unsaid = refusal(
        core.capture_goal(StandingGoal {
            statement: "  ".to_string(),
            title: None,
        })
        .await,
    );
    assert!(unsaid.contains("needs a statement"), "{unsaid}");
}

/// `emit_signal`: a named signal goes out with its payload and its own
/// scope, and the answer says who heard it — or that nobody starts on it.
#[tokio::test]
async fn emit_signal_raises_a_named_signal_and_says_who_heard_it() {
    let fake = FakeIntake::start();
    let core = ToolCore::new(fake.path.clone(), core_agent_scope());

    let heard = core
        .emit_signal(
            "  deploy.finished  ",
            json!({"status": "green"}),
            Some(&format!("goal:{GOAL}")),
        )
        .await
        .unwrap();
    assert!(heard.contains("deploy.finished"), "{heard}");
    assert!(heard.contains("01RECORD"), "{heard}");
    assert!(heard.contains("2 starts heard it"), "{heard}");
    assert!(heard.contains("01HEARD, 01ALSO"), "{heard}");
    assert!(heard.contains("do not wait"), "{heard}");

    let quiet = core
        .emit_signal("nobody.listens", json!({}), None)
        .await
        .unwrap();
    assert!(quiet.contains("No workflow starts on it"), "{quiet}");

    let unnamed = refusal(core.emit_signal("  ", json!({}), None).await);
    assert!(unnamed.contains("needs a name"), "{unnamed}");
    let bad = refusal(core.emit_signal("Not A Name", json!({}), None).await);
    assert!(bad.contains("is not a signal name"), "{bad}");

    // The client's own answer, for a caller that wants the ids.
    let client = IntakeClient::new(fake.path.clone());
    let raised = client
        .emit_signal(
            &core_agent_scope(),
            "deploy.finished",
            json!({"status": "green"}),
            Some(&format!("goal:{GOAL}")),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        raised,
        Raised {
            signal: "01RECORD".into(),
            listeners: vec!["01HEARD".into(), "01ALSO".into()],
        }
    );
}

/// The General Agent is refused the Workflow Agent's ops by the engine, by
/// id — the router already keeps the tools apart, and this is the second
/// line, the one that holds if a scope is ever counterfeited.
#[tokio::test]
async fn the_general_agent_cannot_propose_a_workflow() {
    let fake = FakeIntake::start();
    let core = ToolCore::new(fake.path.clone(), core_agent_scope());
    let refused = refusal(core.propose_workflow(good_workflow()).await);
    assert!(refused.contains(WORKFLOW_AGENT_ID), "{refused}");
    assert!(
        refused.contains(CORE_AGENT_ID),
        "the refusal names who asked: {refused}"
    );
}

/// A post through the tool says who speaks — the scope's agent, or the work
/// item the engine resolves one from — so it is signed as that agent and may
/// carry what the agent made. A goal a person drives by hand speaks as
/// nobody, which the engine signs as the General Agent.
#[tokio::test]
async fn a_post_through_the_tool_says_who_speaks() {
    let fake = FakeIntake::start();
    for (scope, spoken_by) in [
        (channel_scope(), "content-strategist"),
        (core_agent_scope(), CORE_AGENT_ID),
        (wi_scope(), "wi"),
    ] {
        let core = ToolCore::new(fake.path.clone(), scope);
        let posted = core
            .post_message(Some("general"), "who speaks?", None, &[], &[], &[])
            .await
            .unwrap();
        assert!(
            posted.contains(&format!("spoken-by-{spoken_by}")),
            "{posted}"
        );
    }
}

/// The connectors installed here are every session's to read — a worker's
/// too, whose agent the engine resolves from its item: a tool on every menu
/// that refused the one session with a step to do left that step unable to
/// find what it may read through.
#[tokio::test]
async fn the_connectors_are_every_sessions_to_read() {
    let fake = FakeIntake::start();
    let worker = ToolCore::new(fake.path.clone(), wi_scope());
    assert_eq!(
        worker.list_connectors().await.unwrap(),
        "CONNECTORS — asked by item wi"
    );
    let turn = ToolCore::new(
        fake.path.clone(),
        Scope::Conversation {
            scope: "01SCOPE".into(),
            agent: "researcher".into(),
            goal: None,
        },
    );
    assert_eq!(
        turn.list_connectors().await.unwrap(),
        "CONNECTORS — asked by agent researcher"
    );
    // A cycle a person drives by hand names nobody, and reads it all the same.
    let by_hand = ToolCore::new(fake.path.clone(), goal_scope());
    assert_eq!(
        by_hand.list_connectors().await.unwrap(),
        "CONNECTORS — asked by nobody named"
    );
}

#[tokio::test]
async fn platform_tools_need_an_agent_identity() {
    let fake = FakeIntake::start();
    // A guided cycle a human is driving by hand: the tools are not in its
    // router at all, but the core refuses with an error rather than panicking
    // if one is ever reached another way.
    let core = ToolCore::new(fake.path.clone(), goal_scope());
    let refused = refusal(core.workspace_overview().await);
    assert!(refused.contains("no agent identity"), "{refused}");
}

/// An engine that answers once and hangs up, or hangs up without a word —
/// what a restart mid-session looks like from the tool's side.
struct FlakyIntake {
    path: PathBuf,
    _dir: tempfile::TempDir,
}

impl FlakyIntake {
    /// `answer_first`: every connection answers its first line, then closes;
    /// otherwise every connection closes at once, saying nothing.
    fn start(answer_first: bool) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("flaky.sock");
        let listener = UnixListener::bind(&path).unwrap();
        tokio::spawn(async move {
            let submits = Arc::new(AtomicU64::new(0));
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                let submits = Arc::clone(&submits);
                tokio::spawn(async move {
                    let (r, mut w) = stream.into_split();
                    let mut lines = BufReader::new(r).lines();
                    if answer_first {
                        if let Ok(Some(line)) = lines.next_line().await {
                            let mut buf = handle(&line, &submits).to_string();
                            buf.push('\n');
                            // The client may have hung up before the reply.
                            let _client_gone = w.write_all(buf.as_bytes()).await;
                        }
                    }
                    // Dropping both halves closes the connection.
                });
            }
        });
        Self { path, _dir: dir }
    }
}

/// The cached connection died between two calls — the engine restarted —
/// and the client reconnects once, so the second call is answered too; a
/// third and a fourth as well, each on a fresh connection.
#[tokio::test]
async fn a_connection_the_engine_dropped_is_made_again_once_per_call() {
    let fake = FlakyIntake::start(true);
    let client = IntakeClient::new(fake.path.clone());
    for n in 0..4 {
        let ok = client
            .result_submit("wi", json!({"answer": n}))
            .await
            .unwrap_or_else(|e| panic!("call {n}: {e}"));
        assert!(
            matches!(ok, SubmitOutcome::Accepted { .. }),
            "call {n}: {ok:?}"
        );
    }
}

/// An engine that takes the connection and says nothing is an error in a
/// sentence, at once — never a tool call that hangs for its session.
#[tokio::test]
async fn an_engine_that_hangs_up_without_a_word_is_an_error_not_a_hang() {
    let fake = FlakyIntake::start(false);
    let client = IntakeClient::new(fake.path.clone());
    let started = std::time::Instant::now();
    let err = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        client.result_submit("wi", json!({"answer": 1})),
    )
    .await
    .expect("answered in time")
    .expect_err("no reply is an error");
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
    let said = err.to_string();
    assert!(!said.is_empty(), "{said}");
}

/// A chat instance in a channel — the scope every browser call failed in.
fn channel_scope() -> Scope {
    Scope::Conversation {
        scope: "general".to_string(),
        agent: "content-strategist".to_string(),
        goal: None,
    }
}

#[tokio::test]
async fn a_browser_call_from_a_channel_carries_the_scope_id_as_its_candidate_goal_and_reads_the_tabs(
) {
    let fake = FakeIntake::start();
    let core = ToolCore::new(fake.path.clone(), channel_scope());
    let words = core
        .browser(json!({"action": "tabs"}))
        .await
        .expect("the tabs are read");
    assert!(
        words.contains("b1") && words.contains("https://example.test/"),
        "{words}"
    );
}

#[tokio::test]
async fn a_refusal_of_the_result_and_a_refusal_of_the_envelope_wear_the_same_not_done() {
    let fake = FakeIntake::start();
    let core = ToolCore::new(fake.path.clone(), channel_scope());
    let silent = core
        .browser(json!({"action": "read", "tab": "b1"}))
        .await
        .expect_err("a silent page is an error");
    assert_eq!(silent.code, rmcp::model::ErrorCode::INVALID_PARAMS);
    assert!(
        silent
            .message
            .starts_with("Not done: the page did not answer"),
        "{}",
        silent.message
    );
    assert!(
        silent.message.contains("try again"),
        "the sentence says what to do"
    );
}

#[tokio::test]
async fn an_envelope_the_engine_could_not_read_is_an_internal_error_never_not_done() {
    let fake = FakeIntake::start();
    let core = ToolCore::new(fake.path.clone(), channel_scope());
    let fault = core
        .browser(json!({"action": "boom"}))
        .await
        .expect_err("a platform fault is an error");
    assert_eq!(
        fault.code,
        rmcp::model::ErrorCode::INTERNAL_ERROR,
        "{}",
        fault.message
    );
    assert!(
        fault
            .message
            .starts_with(bisa_core::browser::PLATFORM_FAULT),
        "{}",
        fault.message
    );
    assert!(
        !fault.message.contains("Not done"),
        "a fault of the platform is not a refusal: {}",
        fault.message
    );
    assert!(
        fault.message.contains("continue the rest of the task"),
        "{}",
        fault.message
    );
}

/// A worker of a run of the workspace orients with `get_run` — whose `goal`
/// is null — is refused `get_goal` in the engine's words, and a message it
/// posts naming no scope goes to `general`; a goal's worker posts in its
/// goal's thread. `get_run` is a worker's alone.
#[tokio::test]
async fn a_worker_of_a_workspace_run_orients_with_get_run_and_speaks_in_general() {
    let fake = FakeIntake::start();
    let core = ToolCore::new(
        fake.path.clone(),
        Scope::WorkItem(WORKSPACE_ITEM.to_string()),
    );

    let run = core.get_run().await.unwrap();
    assert!(run.contains(RUN), "{run}");
    assert!(run.contains("\"goal\": null"), "{run}");
    let refused = refusal(core.get_goal().await);
    assert!(refused.contains("orient with get_run"), "{refused}");

    let posted = core
        .post_message(None, "from the run", None, &[], &[], &[])
        .await
        .unwrap();
    assert!(posted.contains("Message posted"), "{posted}");

    let goals_worker = ToolCore::new(fake.path.clone(), wi_scope());
    let run = goals_worker.get_run().await.unwrap();
    assert!(run.contains(GOAL), "{run}");
    let posted = goals_worker
        .post_message(None, "from the goal's run", None, &[], &[], &[])
        .await
        .unwrap();
    assert!(posted.contains("Message posted"), "{posted}");

    let not_a_worker = ToolCore::new(fake.path.clone(), goal_scope());
    let refused = refusal(not_a_worker.get_run().await);
    assert!(refused.contains("work-item session"), "{refused}");
}

// ---------------------------------------------------------------------------
// The server over the wire: an MCP client in the same process, on a pipe,
// calls every tool of every scope against the fake intake.

use rmcp::model::CallToolRequestParams;
use rmcp::ServiceExt as _;

/// The server for `scope`, served on one end of a pipe, and a client on the
/// other — what a harness session is, minus the process boundary.
async fn connected(
    fake: &FakeIntake,
    scope: Scope,
) -> (
    rmcp::service::RunningService<rmcp::RoleClient, ()>,
    tokio::task::JoinHandle<()>,
) {
    let (ours, theirs) = tokio::io::duplex(1 << 16);
    let (client_read, client_write) = tokio::io::split(ours);
    let (server_read, server_write) = tokio::io::split(theirs);
    let server = bisa_mcp::BisaServer::new(fake.path.clone(), scope);
    let serving = tokio::spawn(async move {
        let running = server
            .serve((server_read, server_write))
            .await
            .expect("the server serves");
        let _ended_by_the_client = running.waiting().await;
    });
    let client = ().serve((client_read, client_write)).await.expect("the client connects");
    (client, serving)
}

/// What one tool call came to: the words of its result, or of its refusal.
enum Said {
    Result(String),
    Refused(String),
}

async fn call(
    client: &rmcp::service::RunningService<rmcp::RoleClient, ()>,
    name: &str,
    args: Value,
) -> Said {
    let arguments = args
        .as_object()
        .cloned()
        .expect("a tool's arguments are an object");
    match client
        .call_tool(CallToolRequestParams::new(name.to_string()).with_arguments(arguments))
        .await
    {
        Ok(result) if result.is_error == Some(true) => Said::Refused(format!("{result:?}")),
        Ok(result) => Said::Result(format!("{result:?}")),
        Err(e) => Said::Refused(e.to_string()),
    }
}

/// The arguments every tool takes, as a session would hand them — `None`
/// for a tool this scope's fake would refuse to hear. `core` says whether
/// the scope's agent is the General Agent; `conversation` whether the scope
/// is a conversation.
fn arguments_for(name: &str, core: bool, conversation: bool) -> Option<Value> {
    arguments_for_in(name, core, conversation, false)
}

/// [`arguments_for`], with every word the fake refuses on sight when
/// `refusing`: each top-level string, and a workflow's name.
fn arguments_for_in(name: &str, core: bool, conversation: bool, refusing: bool) -> Option<Value> {
    let mut args = plain_arguments_for(name, core, conversation)?;
    if refusing {
        if let Some(fields) = args.as_object_mut() {
            for value in fields.values_mut() {
                if value.is_string() {
                    *value = json!("refuse me");
                } else if value.get("name").is_some_and(Value::is_string) {
                    value["name"] = json!("refuse me");
                }
            }
        }
    }
    Some(args)
}

fn plain_arguments_for(name: &str, core: bool, conversation: bool) -> Option<Value> {
    let tab = json!("b1");
    Some(match name {
        "list_connectors"
        | "get_goal"
        | "browser_tabs"
        | "recall_list"
        | "_Stop"
        | "get_run"
        | "workspace_overview"
        | "list_staff"
        | "list_workflow_templates"
        | "pr_reviews_list"
        | "mobile_development_devices" => json!({}),
        "call_connector" => {
            json!({"connector": "jira", "operation": "list", "account": "acct-1", "params": {"q": "x"}})
        }
        "browser_open" => json!({"url": "https://example.test/", "tab": tab, "headless": true}),
        "decide" => {
            json!({"state": {"page": 1}, "questions": {"q": {"type": "noul", "instructions": "?"}}})
        }
        "browser_serve" => json!({"folder": "dist"}),
        "browser_snapshot" => json!({"tab": tab, "target": "main", "all": true}),
        "browser_read" => json!({"tab": tab, "target": "main", "format": "text"}),
        "browser_find" => json!({"tab": tab, "query": "login"}),
        "browser_click" | "browser_hover" => json!({"tab": tab, "target": "#go"}),
        "browser_type" => {
            json!({"tab": tab, "target": "#q", "text": "hi", "clear": true, "submit": false})
        }
        "browser_fill" => json!({"tab": tab, "target": "#q", "text": "hi"}),
        "browser_press" => {
            json!({"tab": tab, "key": "Enter", "target": "#q", "modifiers": ["Shift"]})
        }
        "browser_select" => json!({"tab": tab, "target": "#s", "value": "a", "label": "A"}),
        "browser_scroll" => json!({"tab": tab, "to": "bottom", "by_x": 0, "by_y": 10}),
        "browser_wait" => {
            json!({"tab": tab, "until": "text", "target": "#x", "query": "done", "timeout_ms": 10})
        }
        "browser_back" | "browser_forward" | "browser_reload" | "browser_close"
        | "browser_screenshot" => json!({"tab": tab}),
        "browser_console" => json!({"tab": tab, "clear": true}),
        "browser_eval" => json!({"tab": tab, "expression": "1+1"}),
        "drawing_list" => json!({"scope": "goal", "id": GOAL}),
        "drawing_read" | "drawing_snapshot" => json!({"drawing": "01DRAWING"}),
        "drawing_create" => json!({"title": "Orders", "scope": "workspace"}),
        "drawing_draw" => {
            json!({"drawing": "01DRAWING", "elements": [{"type": "rectangle"}], "replace": false})
        }
        "drawing_mermaid" => {
            json!({"drawing": "01DRAWING", "text": "graph TD; a-->b", "replace": true})
        }
        "drawing_erase" => json!({"drawing": "01DRAWING", "ids": ["e1"]}),
        "mobile_development_status" => json!({"fresh": true}),
        "mobile_development_boot" | "mobile_development_screenshot" => json!({"device": "A-1"}),
        "ask_human" => json!({"question": "ship it?"}),
        "await_human" => json!({"gate": "gate-1"}),
        "ask_human_and_wait" => json!({"question": "ship it?", "expects": "decision"}),
        "add_note" => json!({"text": "noted"}),
        "spawn_sub_goal" => json!({"statement": "child", "title": "T"}),
        "emit_signal" => json!({"name": "nobody.listens", "payload": {"status": "green"}}),
        "post_message" => {
            json!({"content": "who speaks?", "reply_to": "m0", "mentions": ["@dev"], "attachments": ["notes.md"]})
        }
        "recall_store" => json!({"slug": "style", "value": "v", "base_hash": "r1"}),
        "recall_get" => json!({"slug": "style"}),
        "create_project" => {
            if !core {
                return None;
            }
            json!({"slug": "web", "name": "Web", "assignees": ["agent:developer"]})
        }
        "note_read" => json!({"note": "01NOTE"}),
        "note_append" => json!({"text": "more", "note": "01NOTE"}),
        "note_write" => json!({"text": "all", "base_hash": "n1", "note": "01NOTE"}),
        "review_notes_list" => json!({"project": "01P", "include_resolved": true}),
        "review_note_resolve" => json!({"note": "01NOTE", "project": "01P"}),
        "yield_result" => json!({"output": {"answer": 1}}),
        "report_progress" => json!({"verb": "fixed", "object": "parser", "outcome": "green"}),
        "revise_statement" => json!({"statement": "sharper", "why": "clearer"}),
        "propose_workflow" | "amend_workflow" | "validate_workflow" => {
            json!({"workflow": good_workflow()})
        }
        "list_catalog" => json!({"kind": "agent"}),
        "get_workflow" => json!({"workflow": "software-feature"}),
        "save_workflow" => {
            if !conversation {
                return None;
            }
            json!({"workflow": good_workflow(), "revision": 3})
        }
        "install_catalog_entry" => json!({"kind": "agent", "slug": "developer", "goal": GOAL}),
        "assign" => json!({"assignees": ["agent:developer"], "goal": GOAL, "replace": true}),
        "capture_goal" => {
            json!({"statement": "Every Monday at 09:00, post the digest", "title": "Digest"})
        }
        "pr_review_submit" => {
            json!({"event": "comment", "body": "fine", "comments": [{"path": "a.rs", "line": 1, "body": "x"}]})
        }
        "pr_thread_reply" => json!({"thread": "t1", "body": "ok", "resolve": true}),
        "pr_thread_resolve" => json!({"thread": "t1", "resolved": false}),
        other => panic!("no arguments known for the tool {other}; add a row"),
    })
}

/// A goal's thread, as the Workflow Agent is launched into it.
fn goal_thread_scope(agent: &str) -> Scope {
    Scope::Conversation {
        scope: GOAL.to_string(),
        agent: agent.to_string(),
        goal: Some(GOAL.to_string()),
    }
}

/// Every tool of every scope answers over the wire — with its words when
/// the engine agrees, as a tool error when it refuses — and the manifest a
/// client lists is the router's: a tool nobody here knows the arguments of
/// fails this test, and every tool is called at least once across the scopes.
#[tokio::test(flavor = "multi_thread")]
async fn every_tool_of_every_scope_answers_over_the_wire() {
    let fake = FakeIntake::start();
    let scopes = [
        ("a worker", wi_scope(), false, false),
        ("a goal with no agent", goal_scope(), false, false),
        (
            "the General Agent in a goal",
            core_agent_scope(),
            true,
            false,
        ),
        (
            "the Workflow Agent in a goal",
            workflow_agent_scope(),
            false,
            false,
        ),
        ("another agent in a channel", channel_scope(), false, true),
        (
            "the General Agent in a channel",
            Scope::Conversation {
                scope: "general".into(),
                agent: CORE_AGENT_ID.into(),
                goal: None,
            },
            true,
            true,
        ),
        (
            "the General Agent in a goal's thread, launched without the goal",
            Scope::Conversation {
                scope: GOAL.into(),
                agent: CORE_AGENT_ID.into(),
                goal: None,
            },
            true,
            true,
        ),
        (
            "the Workflow Agent in a goal's thread",
            goal_thread_scope(WORKFLOW_AGENT_ID),
            false,
            true,
        ),
    ];
    let mut listed = std::collections::BTreeSet::new();
    let mut called = std::collections::BTreeSet::new();
    for (who, scope, core, conversation) in scopes {
        let (client, serving) = connected(&fake, scope).await;
        let info = client.peer_info().expect("the server introduced itself");
        let instructions = info.instructions.clone().unwrap_or_default();
        assert!(
            instructions.contains("Bisa"),
            "{who}: the instructions name the platform: {instructions}"
        );
        if who == "the Workflow Agent in a goal's thread" {
            assert!(
                instructions.contains("propose_workflow proposes for it"),
                "{who}: {instructions}"
            );
        }
        if who == "the General Agent in a channel" {
            assert!(
                instructions.contains("addressed nobody was routed to you"),
                "{who}: {instructions}"
            );
        }
        let tools = client.list_all_tools().await.expect("the tools are listed");
        assert!(tools.len() > 40, "{who}: {} tools", tools.len());
        for tool in &tools {
            listed.insert(tool.name.to_string());
            let Some(args) = arguments_for(&tool.name, core, conversation) else {
                continue;
            };
            let said = call(&client, &tool.name, args).await;
            called.insert(tool.name.to_string());
            if let Said::Refused(words) = &said {
                assert!(
                    !words.contains("unknown op") && !words.contains("bad request"),
                    "{who}: {} met a fake that did not know it: {words}",
                    tool.name
                );
            }
        }
        client.cancel().await.expect("the client leaves");
        let _server_ended = tokio::time::timeout(std::time::Duration::from_secs(5), serving).await;
    }
    assert_eq!(listed, called, "every listed tool was called in some scope");
    assert!(called.contains("_Stop") && called.contains("save_workflow"));
}

/// Every refusal of the engine reaches the session as a tool error, whatever
/// the tool: a scope the fake refuses on sight makes every call a refusal,
/// and none of them is dressed as a result.
#[tokio::test(flavor = "multi_thread")]
async fn every_refusal_of_the_engine_is_a_tool_error_whatever_the_tool() {
    let fake = FakeIntake::start();
    let refusing = [
        (
            "a worker",
            Scope::WorkItem("refuse me".into()),
            false,
            false,
        ),
        (
            "the General Agent in a goal",
            Scope::Goal {
                goal: "refuse me".into(),
                agent: Some(CORE_AGENT_ID.into()),
            },
            true,
            false,
        ),
        (
            "the Workflow Agent in a goal",
            Scope::Goal {
                goal: "refuse me".into(),
                agent: Some(WORKFLOW_AGENT_ID.into()),
            },
            false,
            false,
        ),
        (
            "the Workflow Agent in a thread",
            Scope::Conversation {
                scope: "refuse me".into(),
                agent: WORKFLOW_AGENT_ID.into(),
                goal: Some("refuse me".into()),
            },
            false,
            true,
        ),
        (
            "the General Agent in a channel",
            Scope::Conversation {
                scope: "refuse me".into(),
                agent: CORE_AGENT_ID.into(),
                goal: None,
            },
            true,
            true,
        ),
    ];
    for (who, scope, core, conversation) in refusing {
        let worker = who == "a worker";
        let (client, serving) = connected(&fake, scope).await;
        let tools = client.list_all_tools().await.expect("the tools are listed");
        for tool in &tools {
            let Some(args) = arguments_for_in(&tool.name, core, conversation, true) else {
                continue;
            };
            match call(&client, &tool.name, args).await {
                Said::Refused(words) => {
                    // A refusal in the engine's own words; a tool that stops
                    // before asking says its own.
                    let _ = words;
                }
                Said::Result(words) => assert!(
                    // The hook asks nothing; a worker whose run cannot be
                    // read has nowhere to post and says so; an op that
                    // carries nothing but the agent's own id cannot be
                    // refused by the scope or an argument outside a worker.
                    tool.name == "_Stop"
                        || words.contains("No scope given")
                        || (!worker
                            && [
                                "list_connectors",
                                "recall_list",
                                "workspace_overview",
                                "list_workflow_templates",
                                "list_staff",
                            ]
                            .contains(&tool.name.as_ref())),
                    "{who}: {} answered a refusing engine with a result: {words}",
                    tool.name
                ),
            }
        }
        client.cancel().await.expect("the client leaves");
        let _server_ended = tokio::time::timeout(std::time::Duration::from_secs(5), serving).await;
    }
}

/// The tool core's own refusals, said before the engine is asked, and the
/// words it gives what the engine answers at its edges.
#[tokio::test]
async fn the_tool_core_refuses_at_its_own_edges_and_words_what_it_reads() {
    let fake = FakeIntake::start();

    // A goal session holds no work item: the worker tools refuse.
    let goal = ToolCore::new(fake.path.clone(), core_agent_scope());
    assert!(refusal(goal.yield_result(json!({"answer": 1})).await).contains("work-item session"));
    assert!(refusal(goal.report_progress("v", "o", None).await).contains("work-item session"));
    // A goal with no agent cannot sign an amendment, nor the platform tools.
    let unsigned = ToolCore::new(fake.path.clone(), goal_scope());
    assert!(refusal(unsigned.amend_workflow(good_workflow()).await).contains("agent"));
    assert!(refusal(unsigned.propose_workflow(good_workflow()).await).contains("agent"));
    assert!(refusal(unsigned.list_catalog(None).await).contains("agent"));
    assert!(refusal(
        unsigned
            .install_catalog_entry("agent", "developer", None)
            .await
    )
    .contains("agent"));
    assert!(refusal(
        unsigned
            .capture_goal(StandingGoal {
                statement: "x".into(),
                title: None
            })
            .await
    )
    .contains("agent"));
    assert!(refusal(unsigned.list_workflow_templates().await).contains("agent"));
    assert!(refusal(unsigned.get_workflow("x").await).contains("agent"));
    assert!(refusal(unsigned.validate_workflow(good_workflow()).await).contains("agent"));
    assert!(refusal(unsigned.workspace_overview().await).contains("agent"));
    assert!(refusal(unsigned.list_staff().await).contains("agent"));
    assert!(refusal(
        unsigned
            .assign(None, None, &["agent:developer".into()], false)
            .await
    )
    .contains("agent"));

    // A question is said with what it expects; a gate declined or denied
    // tells the agent not to proceed, with the person's words when they gave any.
    let worker_asking = ToolCore::new(fake.path.clone(), wi_scope());
    let sent = worker_asking.ask_human(&ask("ship it?")).await.unwrap();
    assert!(sent.contains("wait for the decision"), "{sent}");
    let declined = worker_asking.await_human("gate-declined").await.unwrap();
    assert!(
        declined.contains("DECLINED") && declined.contains("not like this"),
        "{declined}"
    );
    let denied = worker_asking.await_human("gate-denied").await.unwrap();
    assert!(denied.contains("DENIED"), "{denied}");

    // A worker shapes no goal.
    assert!(
        refusal(worker_asking.propose_workflow(good_workflow()).await).contains("shapes a goal")
    );
    assert!(refusal(worker_asking.amend_workflow(good_workflow()).await).contains("shapes a goal"));

    // The General Agent: an assignee that is no assignee, a project named,
    // no goal to assign under from a channel, and a catalog page with nothing on it.
    let general = ToolCore::new(fake.path.clone(), core_agent_scope());
    let empty = general.list_catalog(Some("team")).await.unwrap();
    assert!(empty.contains("no entries"), "{empty}");
    assert!(refusal(
        general
            .create_project(None, "web", None, &["nobody:x".into()])
            .await
    )
    .contains("nobody:x"));
    let in_a_channel = ToolCore::new(
        fake.path.clone(),
        Scope::Conversation {
            scope: "general".into(),
            agent: CORE_AGENT_ID.into(),
            goal: None,
        },
    );
    assert!(refusal(
        in_a_channel
            .assign(None, None, &["agent:developer".into()], false)
            .await
    )
    .contains("No goal given"));
    let manifest = bisa_mcp::BisaServer::new(fake.path.clone(), core_agent_scope()).tool_manifest();
    assert!(
        manifest.iter().any(|(name, _)| name == "assign"),
        "{manifest:?}"
    );
    assert!(
        manifest.windows(2).all(|w| w[0].0 <= w[1].0),
        "sorted by name"
    );
    assert!(refusal(
        general
            .assign(None, None, &["nobody:x".into()], false)
            .await
    )
    .contains("nobody:x"));
    let on_project = general
        .assign(None, Some("01PROJ"), &["agent:developer".into()], true)
        .await
        .unwrap();
    assert!(
        on_project.contains("project 01PROJ of goal"),
        "{on_project}"
    );
    // A worker of a run of the workspace: no goal to attach a catalog entry to.
    let workspace_worker = ToolCore::new(fake.path.clone(), Scope::WorkItem(WORKSPACE_ITEM.into()));
    let posted = workspace_worker
        .post_message(None, "from the run", None, &[], &[], &[])
        .await
        .unwrap();
    assert!(posted.contains("Message posted"), "{posted}");

    // A worker whose run cannot be read has nowhere to post without a scope.
    let lost = ToolCore::new(fake.path.clone(), Scope::WorkItem("unreadable".into()));
    let said = lost
        .post_message(None, "hello", None, &[], &[], &[])
        .await
        .unwrap();
    assert!(said.contains("could not be read"), "{said}");

    // Notes: nothing to append or write is refused before the engine; a
    // write that changes nothing says so; a hash is required.
    let worker = ToolCore::new(fake.path.clone(), wi_scope());
    assert!(refusal(worker.note_append(None, "  ").await).contains("empty"));
    assert!(refusal(worker.note_write(None, " ", "n1").await).contains("empty"));
    assert!(refusal(worker.note_write(None, "x", " ").await).contains("base_hash"));
    let unchanged = worker.note_write(None, "x", "same").await.unwrap();
    assert!(unchanged.contains("Nothing changed"), "{unchanged}");
    let read = worker.note_read(Some("01NOTE")).await.unwrap();
    assert!(read.starts_with("# Plan\nhash: n1"), "{read}");
    let appended = worker.note_append(Some("01NOTE"), "more").await.unwrap();
    assert!(appended.contains("Appended"), "{appended}");
    let none = worker.review_notes_list(None, false).await.unwrap();
    assert!(none.starts_with("No open review notes"), "{none}");
    let some = worker.review_notes_list(Some("01P"), true).await.unwrap();
    assert!(some.contains("3 review note(s)"), "{some}");
    assert!(
        some.contains("`01NOTE` on `src/a.rs:3` (vs main)"),
        "{some}"
    );
    assert!(
        some.contains("`01OLD` on `src/b.rs:1-4` (working) (resolved)"),
        "{some}"
    );
    assert!(some.contains("```diff\n  -a\n  +b"), "{some}");
    let resolved = worker
        .review_note_resolve("01NOTE", Some("01P"))
        .await
        .unwrap();
    assert_eq!(resolved, "Resolved review note 01NOTE on src/a.rs.");

    // Recall: a stale write names the current value; the list names each memory.
    let stale = worker
        .recall_store("style", "spaces", Some("r0"))
        .await
        .unwrap();
    assert!(
        stale.contains("NOT stored") && stale.contains("tabs, not spaces"),
        "{stale}"
    );
    let got = worker.recall_get("style").await.unwrap();
    assert!(got.starts_with("hash: r1\nlinks: conventions"), "{got}");
    let missing = worker.recall_get("nothing").await.unwrap();
    assert!(missing.contains("No memory stored"), "{missing}");
    let list = worker.recall_list().await.unwrap();
    assert!(list.contains("style (hash r1, links: 0)"), "{list}");

    // The judge's words: sure, and not sure enough.
    let sure = worker
        .decide(json!({"state": {"sure": true}, "questions": {}}))
        .await
        .unwrap();
    assert!(
        sure.starts_with("The Decision-Making Agent is sure"),
        "{sure}"
    );
    let unsure = worker
        .decide(json!({"state": {"sure": false}, "questions": {}}))
        .await
        .unwrap();
    assert!(unsure.contains("NOT sure enough"), "{unsure}");

    // What the mobile tools and a served folder read as.
    let status = worker
        .mobile_development(json!({"action": "status"}))
        .await
        .unwrap();
    assert!(
        status.contains("mobile development: off") && status.contains("flutter: installed"),
        "{status}"
    );
    assert!(
        status.contains("xcode: 16.0") && status.contains("- [ok] Flutter"),
        "{status}"
    );
    let none = worker
        .mobile_development(json!({"action": "devices"}))
        .await
        .unwrap();
    assert!(none.starts_with("no device"), "{none}");
    let booted = worker
        .mobile_development(json!({"action": "boot"}))
        .await
        .unwrap();
    assert!(booted.contains("run: flutter run -d A-1"), "{booted}");
    let shot = worker
        .mobile_development(json!({"action": "screenshot"}))
        .await
        .unwrap();
    assert!(shot.starts_with("screenshot: /shots/a.png (1×2)"), "{shot}");
    let served = worker.browser_serve(Some("dist".into())).await.unwrap();
    assert!(
        served.starts_with("serving `dist` at http://127.0.0.1:1"),
        "{served}"
    );
    let snap = worker.draw(json!({"action": "snapshot"})).await.unwrap();
    assert!(snap.contains("snapshot: /snaps/d.png (3×4)"), "{snap}");
    let erased = worker
        .draw(json!({"action": "erase", "ids": ["e1"]}))
        .await
        .unwrap();
    assert_eq!(erased, "done");
    let listed = worker
        .draw(json!({"action": "list", "scope_id": GOAL}))
        .await
        .unwrap();
    assert!(listed.contains(&format!("goal {GOAL}")), "{listed}");

    // The PR review tools, on a conversation.
    let thread = ToolCore::new(fake.path.clone(), goal_thread_scope(CORE_AGENT_ID));
    assert!(thread.pr_reviews_list().await.unwrap().contains("reviews"));
    assert!(thread
        .pr_review_submit("approve", "", json!([]))
        .await
        .unwrap()
        .contains("approve review"));
    assert_eq!(
        thread.pr_thread_reply("t1", "ok", true).await.unwrap(),
        "Replied on the thread and resolved it."
    );
    assert_eq!(
        thread.pr_thread_reply("t1", "ok", false).await.unwrap(),
        "Replied on the thread."
    );
    assert_eq!(
        thread.pr_thread_resolve("t1", true).await.unwrap(),
        "Resolved the thread."
    );
    assert_eq!(
        thread.pr_thread_resolve("t1", false).await.unwrap(),
        "Reopened the thread."
    );
    // The Workflow Agent in a goal's thread shapes the goal; the General
    // Agent there creates a project under it.
    let shaping = ToolCore::new(fake.path.clone(), goal_thread_scope(WORKFLOW_AGENT_ID));
    assert!(shaping
        .revise_statement("sharper", None)
        .await
        .unwrap()
        .contains("revised"));
    let ungated = shaping.amend_workflow(ungated_workflow()).await.unwrap();
    assert!(ungated.contains("takes its repair at once"), "{ungated}");
    let created = thread.create_project(None, "web", None, &[]).await.unwrap();
    assert!(
        created.contains(GOAL),
        "attached to the thread's goal: {created}"
    );

    // The client's own words.
    assert_eq!(goal_scope().goal(), Some(GOAL));
    assert_eq!(wi_scope().goal(), None);
    assert_eq!(goal_thread_scope(CORE_AGENT_ID).goal(), Some(GOAL));
    assert_eq!(
        IntakeClient::new(fake.path.clone()).socket_path(),
        &fake.path
    );
}

/// An engine that refuses the core agents' own reads — the ops whose
/// envelope names nothing but the agent — is a tool error like any other.
#[tokio::test]
async fn an_engine_that_refuses_the_core_agents_own_reads_is_a_tool_error() {
    let fake = FakeIntake::start_with(true);
    let general = ToolCore::new(fake.path.clone(), core_agent_scope());
    for refused in [
        general.workspace_overview().await,
        general.list_staff().await,
        general.list_catalog(None).await,
        general
            .install_catalog_entry("agent", "developer", None)
            .await,
        general.list_connectors().await,
        general.recall_list().await,
    ] {
        assert!(refusal(refused).contains("refused on purpose"));
    }
    let workflow = ToolCore::new(fake.path.clone(), workflow_agent_scope());
    assert!(refusal(workflow.list_workflow_templates().await).contains("refused on purpose"));
}
