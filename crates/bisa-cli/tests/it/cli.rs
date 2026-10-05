//! End-to-end CLI tests driving the compiled binary against a tempdir
//! workspace with file-based keys (never the OS keyring).

use bisa_store::CATALOG;
use std::path::Path;
use std::process::{Command, Output};

/// A one-`human`-step workflow definition, written where a test can point
/// `--from` at it. The smallest run that waits on a person.
fn human_workflow_file(dir: &Path) -> String {
    let path = dir.join("ask.json");
    std::fs::write(
        &path,
        serde_json::json!({
            "name": "Ask first",
            "description": "one question, then done",
            "steps": [{
                "id": "ask", "name": "Which database?", "kind": "human",
                "prompt": "Which database should we use?",
                "options": [
                    {"id": "sqlite", "label": "SQLite"},
                    {"id": "postgres", "label": "Postgres", "recommended": true}
                ]
            }]
        })
        .to_string(),
    )
    .unwrap();
    path.to_string_lossy().to_string()
}

/// A one-`approval`-step workflow: a signed decision, declined is a failure.
fn approval_workflow_file(dir: &Path) -> String {
    let path = dir.join("ship.json");
    std::fs::write(
        &path,
        serde_json::json!({
            "name": "Ship it",
            "steps": [{
                "id": "ship", "name": "Ship?", "kind": "approval",
                "prompt": "Ship the release?"
            }]
        })
        .to_string(),
    )
    .unwrap();
    path.to_string_lossy().to_string()
}

/// Record a workflow from a file; returns its id.
fn workflow_new(dir: &Path, from: &str) -> String {
    let out = bisa(dir, &["--json", "workflow", "new", "--from", from]);
    assert_ok(&out, "workflow new");
    json(&out)["workflow"]["id"].as_str().unwrap().to_string()
}

pub(crate) fn bisa(data_dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_bisa"))
        // A token in the shell that runs the tests is nobody's business here.
        .env_remove("BISA_API_TOKEN")
        .arg("--data-dir")
        .arg(data_dir)
        .arg("--file-keys")
        .args(args)
        .output()
        .expect("binary runs")
}

/// `bisa init`, then the General Agent and the Workflow Agent on a stub harness. A test never
/// reaches a real harness on this machine: the stub answers every prompt with
/// words and no proposal, so a capture in auto or guided mode wakes the
/// Workflow Agent, sees the wake settle, and returns — and no token is ever
/// spent by a test.
pub(crate) fn init(dir: &Path) {
    assert_ok(&bisa(dir, &["init"]), "init");
    stub_harness(dir);
}

/// A tier-3 custom harness under `<data-dir>/harnesses`, speaking the
/// custom-JSON wire, backed by a shell script in the data dir.
fn stub_harness(dir: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let script = dir.join("stub-harness.sh");
    std::fs::write(
        &script,
        "#!/bin/sh\n\
         # The tests' harness: words and no proposal for every prompt, until EOF.\n\
         while read -r _line; do\n\
           echo '{\"type\":\"text\",\"text\":\"the stub harness has nothing to propose\"}'\n\
           echo '{\"type\":\"ended\",\"outcome\":\"completed\"}'\n\
         done\n",
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let harnesses = dir.join("harnesses");
    std::fs::create_dir_all(&harnesses).unwrap();
    std::fs::write(
        harnesses.join("stub.json"),
        serde_json::json!({
            "id": "stub",
            "label": "Stub harness",
            "command": script.to_string_lossy(),
        })
        .to_string(),
    )
    .unwrap();
    for agent in ["general-agent", "workflow-agent"] {
        assert_ok(
            &bisa(dir, &["agent", "edit", agent, "--harness", "custom:stub"]),
            "agent edit --harness custom:stub",
        );
    }
}

pub(crate) fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).to_string()
}

pub(crate) fn assert_ok(out: &Output, ctx: &str) {
    assert!(
        out.status.success(),
        "{ctx} failed:\nstdout: {}\nstderr: {}",
        stdout(out),
        String::from_utf8_lossy(&out.stderr)
    );
}

pub(crate) fn json(out: &Output) -> serde_json::Value {
    serde_json::from_str(stdout(out).trim()).unwrap_or_else(|e| {
        panic!("stdout is not JSON ({e}): {:?}", stdout(out));
    })
}

#[test]
fn init_is_idempotent_and_stable() {
    let dir = tempfile::tempdir().unwrap();
    let first = bisa(dir.path(), &["--json", "init"]);
    assert_ok(&first, "init");
    let a = json(&first);
    let second = bisa(dir.path(), &["--json", "init"]);
    assert_ok(&second, "second init");
    let b = json(&second);
    assert_eq!(
        a["pubkey"], b["pubkey"],
        "identity must be stable across runs"
    );
    assert!(a["npub"].as_str().unwrap().starts_with("npub1"));
}

#[test]
fn full_solo_flow_from_workflow_to_done() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());

    // The workflow first: recorded, listed, shown with no problems.
    let ask = human_workflow_file(dir.path());
    let wf = workflow_new(dir.path(), &ask);
    let listed = bisa(dir.path(), &["--json", "workflow", "list"]);
    assert_ok(&listed, "workflow list");
    assert!(
        json(&listed)["workflows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w["workflow"]["id"] == wf.as_str()),
        "the library lists it"
    );
    let shown = bisa(dir.path(), &["--json", "workflow", "show", &wf]);
    assert_ok(&shown, "workflow show");
    assert!(json(&shown)["problems"].as_array().unwrap().is_empty());

    // Capture with the workflow, holding the start: a draft pointing at it.
    let new = bisa(
        dir.path(),
        &[
            "--json",
            "new",
            "Build a bookmark sync CLI",
            "--title",
            "Bookmark sync",
            "--workflow",
            &wf,
            "--no-start",
        ],
    );
    assert_ok(&new, "new");
    let created = json(&new);
    let id = created["goal"].as_str().unwrap().to_string();
    assert_eq!(
        created["mode"], "auto",
        "--workflow names the workflow yourself on any mode; --no-start only holds the start"
    );
    assert_eq!(created["workflow"], wf.as_str());
    assert!(created["run"].is_null(), "--no-start holds the start");

    let status = bisa(dir.path(), &["--json", "status", &id]);
    assert_ok(&status, "status");
    assert_eq!(json(&status)["status"], "draft");

    // Start the run: the one human step waits on us, and the run settles
    // there — `run` hands off rather than waiting forever headless.
    let run = bisa(dir.path(), &["--json", "run", &id]);
    assert_ok(&run, "run");
    let r = json(&run);
    assert_eq!(r["status"], "waiting");
    assert_eq!(r["run"]["steps"]["ask"]["state"]["state"], "waiting");

    // The inbox shows the goal as a conversation wanting an answer.
    let inbox = bisa(dir.path(), &["--json", "inbox"]);
    assert_ok(&inbox, "inbox");
    let rows = json(&inbox);
    assert!(
        rows["rows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["key"] == id.as_str()),
        "inbox should carry the waiting goal: {rows}"
    );

    // Answer the step: an option it offered, refused when it is not one.
    let bogus = bisa(
        dir.path(),
        &["--json", "step", "answer", &id, "ask", "-o", "mysql"],
    );
    assert!(
        !bogus.status.success(),
        "an unoffered option id must refuse"
    );
    let nothing = bisa(dir.path(), &["--json", "step", "answer", &id, "ask", ""]);
    assert!(!nothing.status.success(), "an empty answer must refuse");
    let answered = bisa(
        dir.path(),
        &[
            "--json",
            "step",
            "answer",
            &id,
            "ask",
            "for full-text search",
            "-o",
            "postgres",
        ],
    );
    assert_ok(&answered, "step answer");
    let a = json(&answered);
    assert_eq!(a["status"], "done");
    assert_eq!(a["run"]["steps"]["ask"]["state"]["state"], "done");

    // Status renders the run and its step.
    let status = bisa(dir.path(), &["--json", "status", &id]);
    assert_ok(&status, "status");
    let st = json(&status);
    assert_eq!(st["status"], "done");
    assert_eq!(st["run"]["workflow"]["id"], wf.as_str());

    // Log shows journal lines: the run starting, the step's facts, the run
    // finishing — and the answer whole.
    let log = bisa(dir.path(), &["--json", "log", &id]);
    assert_ok(&log, "log");
    let events = json(&log)["events"].as_array().unwrap().clone();
    assert!(events.len() >= 4, "{events:?}");
    let text = stdout(&bisa(dir.path(), &["log", &id]));
    assert!(
        text.contains("postgres") && text.contains("for full-text search"),
        "the answer must be journaled whole: {text}"
    );
    assert!(
        text.contains("run ") && text.contains("finished done"),
        "{text}"
    );

    // Search finds it, with its status.
    let found = bisa(dir.path(), &["--json", "search", "bookmark"]);
    assert_ok(&found, "search");
    let m = json(&found)["matches"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["goal"] == id.as_str())
        .cloned()
        .expect("search should find the goal");
    assert_eq!(m["status"], "done");

    // Human (non-json) status prints prose with the step marked done.
    let human = bisa(dir.path(), &["status", &id]);
    assert_ok(&human, "human status");
    let text = stdout(&human);
    assert!(text.contains("Bookmark sync"), "{text}");
    assert!(text.contains("✓ ask"), "{text}");
}

/// `workflow use` points a goal at a workflow after the fact, `run` starts it
/// with inputs, and a second start while the first is unfinished is refused.
#[test]
fn workflow_use_then_run_with_inputs() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());
    let path = dir.path().join("greet.json");
    std::fs::write(
        &path,
        serde_json::json!({
            "name": "Greet",
            "inputs": [{"name": "who", "label": "Who", "kind": "text", "required": true}],
            "steps": [{
                "id": "ask", "name": "Greet {inputs.who}", "kind": "human",
                "prompt": "Say hello to {inputs.who}?"
            }]
        })
        .to_string(),
    )
    .unwrap();
    let wf = workflow_new(dir.path(), path.to_str().unwrap());
    let goal = json(&bisa(
        dir.path(),
        &["--json", "new", "Greet somebody", "--mode", "manual"],
    ))["goal"]
        .as_str()
        .unwrap()
        .to_string();

    // No workflow yet: nothing to run, and the message says what to do.
    let idle = bisa(dir.path(), &["--json", "run", &goal]);
    assert_ok(&idle, "run without a workflow");
    assert_eq!(json(&idle)["started"], false);

    let used = bisa(dir.path(), &["--json", "workflow", "use", &goal, &wf]);
    assert_ok(&used, "workflow use");
    assert_eq!(json(&used)["workflow"], wf.as_str());

    // A required input left out is refused at start.
    let missing = bisa(dir.path(), &["run", &goal]);
    assert!(!missing.status.success(), "{}", stdout(&missing));
    assert!(
        String::from_utf8_lossy(&missing.stderr).contains("who"),
        "the refusal names the input"
    );

    let run = bisa(
        dir.path(),
        &["--json", "run", &goal, "--input", "who=the team"],
    );
    assert_ok(&run, "run --input");
    let r = json(&run);
    assert_eq!(r["status"], "waiting");
    assert_eq!(r["run"]["inputs"]["who"], "the team");

    // Unfinished: a second start is refused, and `status` shows the run.
    let again = bisa(dir.path(), &["--json", "run", &goal]);
    assert_ok(&again, "run picks up the unfinished run");
    assert_eq!(json(&again)["run"]["id"], r["run"]["id"]);
    let status = json(&bisa(dir.path(), &["--json", "status", &goal]));
    assert_eq!(status["status"], "waiting");

    // A workflow in use cannot be removed; the refusal names the holder.
    let rm = bisa(dir.path(), &["workflow", "rm", &wf]);
    assert!(!rm.status.success());
}

/// `workflow validate` names problems by step and exits non-zero; `workflow
/// new` keeps a definition with problems as a draft and prints them — and a
/// run of it is what refuses.
#[test]
fn workflow_validate_reports_problems_and_new_keeps_a_draft_with_them() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());
    let path = dir.path().join("broken.json");
    std::fs::write(
        &path,
        serde_json::json!({
            "name": "Broken",
            "steps": [{
                "id": "a", "name": "A", "kind": "human", "prompt": "?", "then": ["nowhere"]
            }]
        })
        .to_string(),
    )
    .unwrap();
    let from = path.to_str().unwrap();

    let validated = bisa(
        dir.path(),
        &["--json", "workflow", "validate", "--from", from],
    );
    assert!(!validated.status.success(), "problems are a non-zero exit");
    let v = json(&validated);
    assert_eq!(v["valid"], false);
    assert!(
        v["problems"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["kind"] == "unknown_step" && p["step"] == "a"),
        "{v}"
    );
    let prose = bisa(dir.path(), &["workflow", "validate", "--from", from]);
    assert!(stdout(&prose).contains("step a:"), "{}", stdout(&prose));

    let kept = bisa(dir.path(), &["--json", "workflow", "new", "--from", from]);
    assert_ok(&kept, "workflow new keeps a draft");
    let v = json(&kept);
    let id = v["workflow"]["id"].as_str().unwrap().to_string();
    assert_eq!(v["workflow"]["revision"], 1);
    assert!(
        v["problems"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["kind"] == "unknown_step"),
        "the problems are printed with the draft: {v}"
    );
    let prose = bisa(dir.path(), &["workflow", "new", "--from", from]);
    assert_ok(&prose, "workflow new in prose");
    assert!(stdout(&prose).contains("step a:"), "{}", stdout(&prose));
    let listed = json(&bisa(dir.path(), &["--json", "workflow", "list"]));
    let row = listed["workflows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["workflow"]["id"] == id)
        .expect("the draft is listed");
    assert!(!row["problems"].as_array().unwrap().is_empty());

    // Starting it is what refuses.
    let created = bisa(
        dir.path(),
        &[
            "--json",
            "new",
            "draft goal",
            "--workflow",
            &id,
            "--no-start",
        ],
    );
    assert_ok(&created, "new --no-start on a draft");
    let gid = json(&created)["goal"].as_str().unwrap().to_string();
    let run = bisa(dir.path(), &["run", &gid]);
    assert!(
        !run.status.success(),
        "a draft with problems does not start"
    );
    let err = String::from_utf8_lossy(&run.stderr);
    assert!(err.contains("nowhere"), "{err}");
}

/// `workflow edit` names the revision it edited; a workflow that moved past
/// it is a conflict naming both revisions, and nothing is written.
#[test]
fn workflow_edit_offline_reports_stale_revision() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());
    let from = human_workflow_file(dir.path());
    let id = workflow_new(dir.path(), &from);

    // Edited from revision 1: lands as revision 2.
    let first = bisa(
        dir.path(),
        &[
            "--json",
            "workflow",
            "edit",
            &id,
            "--from",
            &from,
            "--revision",
            "1",
        ],
    );
    assert_ok(&first, "workflow edit");
    assert_eq!(json(&first)["workflow"]["revision"], 2);

    // Edited from revision 1 again: the stored copy is at 2.
    let stale = bisa(
        dir.path(),
        &["workflow", "edit", &id, "--from", &from, "--revision", "1"],
    );
    assert!(!stale.status.success(), "a stale revision is refused");
    let err = String::from_utf8_lossy(&stale.stderr);
    assert!(err.contains("revision 2"), "{err}");
    assert!(err.contains("revision 1"), "{err}");
    assert!(err.contains("reload and merge"), "{err}");
    let shown = json(&bisa(dir.path(), &["--json", "workflow", "show", &id]));
    assert_eq!(shown["workflow"]["revision"], 2, "nothing was written");

    // Without `--revision`, the stored revision is the one edited.
    let third = bisa(
        dir.path(),
        &["--json", "workflow", "edit", &id, "--from", &from],
    );
    assert_ok(&third, "workflow edit at the stored revision");
    assert_eq!(json(&third)["workflow"]["revision"], 3);
}

/// A catalog-shaped TOML — the definition under `[workflow]`, a `spawn` step
/// naming another template by slug — reads the way the installer reads it:
/// the slug resolves to the installed id, and a misspelled key is refused.
#[test]
fn workflow_new_from_catalog_toml_resolves_spawn_slugs() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());
    assert_ok(
        &bisa(
            dir.path(),
            &["catalog", "install", "workflow", "incident-response"],
        ),
        "install incident-response",
    );
    let installed = json(&bisa(
        dir.path(),
        &["--json", "workflow", "show", "incident-response"],
    ))["workflow"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let path = dir.path().join("escalate.toml");
    std::fs::write(
        &path,
        r#"[workflow]
name = "Escalate"
description = "spawns an incident"

[[workflow.steps]]
id = "open"
name = "Open an incident"
kind = "spawn"
statement_template = "Incident: {goal.statement}"
workflow = "incident-response"
wait = false
"#,
    )
    .unwrap();
    let from = path.to_str().unwrap();
    // The slug resolves; and a spawn that gives its workflow nothing of what
    // it requires is kept as a draft with its problems, each by name.
    let made = bisa(dir.path(), &["--json", "workflow", "new", "--from", from]);
    assert_ok(&made, "workflow new from TOML");
    let v = json(&made);
    assert_eq!(v["workflow"]["steps"][0]["workflow"], installed, "{v}");
    let mut left_out: Vec<(&str, &str)> = v["problems"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["kind"].as_str().unwrap_or_default(),
                p["text"]["args"]["name"].as_str().unwrap_or_default(),
            )
        })
        .collect();
    left_out.sort_unstable();
    assert_eq!(
        left_out,
        vec![("spawn_input", "project"), ("spawn_input", "symptom")],
        "{v}"
    );

    // Given what the child requires, it has none.
    std::fs::write(
        &path,
        r#"[workflow]
name = "Escalate whole"
description = "spawns an incident, with what it needs"

[[workflow.inputs]]
name = "project"
label = "The project the cause is fixed in"
kind = "project"
required = true

[[workflow.steps]]
id = "open"
name = "Open an incident"
kind = "spawn"
statement_template = "Incident: {goal.statement}"
workflow = "incident-response"
inputs = { symptom = "{goal.statement}", project = "{inputs.project}" }
wait = false
"#,
    )
    .unwrap();
    let made = bisa(dir.path(), &["--json", "workflow", "new", "--from", from]);
    assert_ok(&made, "workflow new from TOML, whole");
    let v = json(&made);
    assert_eq!(v["workflow"]["steps"][0]["workflow"], installed, "{v}");
    assert_eq!(
        v["workflow"]["steps"][0]["inputs"],
        serde_json::json!({ "symptom": "{goal.statement}", "project": "{inputs.project}" }),
        "{v}"
    );
    assert!(v["problems"].as_array().unwrap().is_empty(), "{v}");

    // A slug nothing installed is refused by name.
    std::fs::write(
        &path,
        std::fs::read_to_string(&path)
            .unwrap()
            .replace("incident-response", "no-such-template"),
    )
    .unwrap();
    let refused = bisa(dir.path(), &["workflow", "new", "--from", from]);
    assert!(!refused.status.success());
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("no-such-template"),
        "{}",
        String::from_utf8_lossy(&refused.stderr)
    );

    // A misspelled key is refused, never dropped.
    let typo = dir.path().join("typo.toml");
    std::fs::write(
        &typo,
        r#"[workflow]
name = "Typo"
description = "d"

[[workflow.steps]]
id = "a"
name = "A"
kind = "end"
finsh = "done"
"#,
    )
    .unwrap();
    let refused = bisa(
        dir.path(),
        &["workflow", "new", "--from", typo.to_str().unwrap()],
    );
    assert!(!refused.status.success());
    let err = String::from_utf8_lossy(&refused.stderr);
    assert!(err.contains("finsh"), "{err}");
}

/// The catalog's templates are listed without installing anything; `show`
/// reads one by slug, and `use` installs it on the way.
#[test]
fn workflow_templates_are_browsable_and_installed_on_use() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());

    let templates = bisa(dir.path(), &["--json", "workflow", "list", "--templates"]);
    assert_ok(&templates, "workflow list --templates");
    let rows = json(&templates)["templates"].as_array().unwrap().clone();
    assert_eq!(
        rows.len(),
        CATALOG.workflows.len(),
        "every template the build ships, none installed"
    );
    assert!(rows.iter().all(|r| r["installed"] == false));
    assert!(
        json(&bisa(dir.path(), &["--json", "workflow", "list"]))["workflows"]
            .as_array()
            .unwrap()
            .is_empty(),
        "listing installs nothing"
    );

    let shown = bisa(dir.path(), &["--json", "workflow", "show", "weekly-review"]);
    assert_ok(&shown, "workflow show <slug>");
    let shown = json(&shown);
    assert!(
        shown["workflow"]["name"]
            .as_str()
            .is_some_and(|n| !n.is_empty()),
        "a template reads as a workflow: {shown}"
    );
    assert!(shown["problems"].is_array(), "{shown}");

    let goal = json(&bisa(
        dir.path(),
        &["--json", "new", "Review the week", "--mode", "manual"],
    ))["goal"]
        .as_str()
        .unwrap()
        .to_string();
    let used = bisa(
        dir.path(),
        &["--json", "workflow", "use", &goal, "weekly-review"],
    );
    assert_ok(&used, "workflow use <slug>");
    let installed = json(&bisa(dir.path(), &["--json", "workflow", "list"]));
    let rows = installed["workflows"].as_array().unwrap();
    assert_eq!(
        rows.len(),
        1,
        "the template and what it brought: {installed}"
    );
    assert_eq!(
        rows[0]["workflow"]["origin"],
        serde_json::json!({"origin": "catalog", "slug": "weekly-review"})
    );
}

/// Declining an `approval` step fails it, and with `on_fail` at its default
/// the run: `approve --no` is a real outcome, not a pause.
#[test]
fn declining_an_approval_step_fails_the_run() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());
    let wf = workflow_new(dir.path(), &approval_workflow_file(dir.path()));
    let new = bisa(
        dir.path(),
        &["--json", "new", "Throwaway release", "--workflow", &wf],
    );
    assert_ok(&new, "new --workflow starts the run");
    let created = json(&new);
    let id = created["goal"].as_str().unwrap().to_string();
    assert_eq!(
        created["status"], "waiting",
        "the approval step waits: {created}"
    );

    let rejected = bisa(
        dir.path(),
        &["--json", "approve", &id, "--no", "--rationale", "not now"],
    );
    assert_ok(&rejected, "reject");
    let r = json(&rejected);
    assert_eq!(r["approve"], false);
    assert_eq!(r["gate"], "approval");
    assert_eq!(r["status"], "failed");

    let status = json(&bisa(dir.path(), &["--json", "status", &id]));
    assert_eq!(status["run"]["steps"]["ship"]["state"]["state"], "failed");
    assert_eq!(status["run"]["outcome"], "failed");

    // A failed goal runs again on the same workflow, and this time it is
    // approved: the second run is its own record.
    let run = bisa(dir.path(), &["--json", "run", &id]);
    assert_ok(&run, "run again");
    let approved = bisa(dir.path(), &["--json", "approve", &id]);
    assert_ok(&approved, "approve");
    assert_eq!(json(&approved)["status"], "done");
    let status = json(&bisa(dir.path(), &["--json", "status", &id]));
    assert_eq!(status["status"], "done");
}

/// `close` is the one move outside the run: it cancels an unfinished run
/// and the goal is closed for good.
#[test]
fn closing_a_goal_cancels_its_run() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());
    let wf = workflow_new(dir.path(), &human_workflow_file(dir.path()));
    let id = json(&bisa(
        dir.path(),
        &["--json", "new", "Give up on me", "--workflow", &wf],
    ))["goal"]
        .as_str()
        .unwrap()
        .to_string();

    let closed = bisa(
        dir.path(),
        &["--json", "close", &id, "--rationale", "not worth it"],
    );
    assert_ok(&closed, "close");
    assert_eq!(json(&closed)["closed"]["reason"], "abandoned");
    let status = json(&bisa(dir.path(), &["--json", "status", &id]));
    assert_eq!(status["status"], "closed");
    assert!(status["run"]["outcome"].is_null(), "{status}");
    assert_eq!(status["run"]["cancelled"]["cause"], "closed", "{status}");

    // Nothing starts on a closed goal.
    let run = bisa(dir.path(), &["run", &id]);
    assert!(!run.status.success());
}

/// `run --new` queues behind the live run, `runs` lists newest first with
/// the queued one's place, `stop` cancels the live run and withdraws the
/// queue, and `restart` starts the last run again.
#[test]
fn a_second_run_queues_and_stop_restart_and_runs_read_back() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());
    let wf = workflow_new(dir.path(), &human_workflow_file(dir.path()));
    let id = json(&bisa(
        dir.path(),
        &["--json", "new", "Run me twice", "--workflow", &wf],
    ))["goal"]
        .as_str()
        .unwrap()
        .to_string();
    let first = json(&bisa(dir.path(), &["--json", "status", &id]))["run"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // A plain `run` follows the live run; `--new` makes another, queued.
    let again = json(&bisa(dir.path(), &["--json", "run", &id]));
    assert_eq!(again["run"]["id"], first.as_str());
    let queued = bisa(dir.path(), &["--json", "run", &id, "--new"]);
    assert_ok(&queued, "run --new");
    assert!(
        String::from_utf8_lossy(&queued.stderr).contains("queued behind run"),
        "{}",
        String::from_utf8_lossy(&queued.stderr)
    );
    assert_eq!(
        json(&queued)["run"]["id"],
        first.as_str(),
        "the live run is followed"
    );

    let runs = bisa(dir.path(), &["--json", "runs", &id]);
    assert_ok(&runs, "runs");
    let listed = json(&runs)["runs"].as_array().unwrap().clone();
    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0]["status"], "queued", "newest first");
    assert_eq!(listed[0]["position"], 1);
    assert_eq!(listed[1]["id"], first.as_str());
    assert_eq!(listed[1]["status"], "waiting");
    let queued_id = listed[0]["id"].as_str().unwrap().to_string();

    // Stop: the live run stopped with the word, the queue withdrawn.
    let stopped = bisa(
        dir.path(),
        &["--json", "stop", &id, "--rationale", "enough"],
    );
    assert_ok(&stopped, "stop");
    let s = json(&stopped);
    assert_eq!(s["stopped"], first.as_str());
    assert_eq!(s["withdrawn"], serde_json::json!([queued_id]));
    let status = json(&bisa(dir.path(), &["--json", "status", &id]));
    assert_eq!(status["status"], "draft");
    // A cancelled run has no outcome; its `cancelled` cause says why.
    assert!(status["run"]["outcome"].is_null(), "{status}");
    assert_eq!(status["run"]["cancelled"]["cause"], "stopped");
    assert_eq!(status["run"]["cancelled"]["rationale"], "enough");
    let listed = json(&bisa(dir.path(), &["--json", "runs", &id]))["runs"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(listed[0]["cause"]["cause"], "withdrawn");
    assert_eq!(listed[1]["cause"]["cause"], "stopped");
    // Nothing left: said, still a success.
    let nothing = bisa(dir.path(), &["--json", "stop", &id]);
    assert_ok(&nothing, "stop with nothing to stop");
    assert!(json(&nothing)["stopped"].is_null());

    // Restart: a new run, live, of the same workflow.
    let restarted = bisa(dir.path(), &["--json", "restart", &id]);
    assert_ok(&restarted, "restart");
    let r = json(&restarted);
    assert_eq!(r["status"], "waiting");
    assert_ne!(r["run"]["id"], first.as_str());
    assert_eq!(r["run"]["workflow"]["id"], wf.as_str());
    assert_eq!(
        json(&bisa(dir.path(), &["--json", "runs", &id]))["runs"]
            .as_array()
            .unwrap()
            .len(),
        3
    );

    // The workflow's verbs act on its runs of the workspace alone: the
    // goal's live run of it is its goal's.
    let stopped = bisa(dir.path(), &["--json", "workflow", "stop", &wf]);
    assert_ok(&stopped, "workflow stop");
    assert_eq!(json(&stopped)["runs"], serde_json::json!([]));
    let status = json(&bisa(dir.path(), &["--json", "status", &id]));
    assert_eq!(status["status"], "waiting", "the goal's run goes on");
}

/// `workflow run` starts a run of the workspace — no goal — and follows it
/// until it waits on a person; `workflow runs` lists it; its step is
/// answered and its status read by the run's own id; `workflow stop` and
/// `restart --run` act on it; a goal is never asked for.
#[test]
fn a_workflow_runs_in_the_workspace_from_the_cli() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());
    let wf = workflow_new(dir.path(), &human_workflow_file(dir.path()));

    let started = bisa(dir.path(), &["--json", "workflow", "run", &wf]);
    assert_ok(&started, "workflow run");
    let s = json(&started);
    assert_eq!(s["scope"], "workspace", "{s}");
    assert!(s["goal"].is_null(), "{s}");
    assert_eq!(s["status"], "waiting", "the human step waits: {s}");
    let run = s["run"].as_str().unwrap().to_string();
    assert!(
        String::from_utf8_lossy(&started.stderr).contains(&format!("bisa step answer {run} ask")),
        "the waiting line names the run: {}",
        String::from_utf8_lossy(&started.stderr)
    );
    let goals = json(&bisa(dir.path(), &["--json", "search"]));
    assert_eq!(
        goals["matches"],
        serde_json::json!([]),
        "no goal was captured: {goals}"
    );

    let runs = json(&bisa(dir.path(), &["--json", "workflow", "runs", &wf]));
    let listed = runs["runs"].as_array().unwrap();
    assert_eq!(listed.len(), 1, "{runs}");
    assert_eq!(listed[0]["id"], run.as_str());
    assert_eq!(listed[0]["number"], 1);

    // The run's status, by its own id.
    let status = json(&bisa(dir.path(), &["--json", "status", &run]));
    assert_eq!(status["run"]["id"], run.as_str(), "{status}");
    assert_eq!(status["run"]["scope"]["scope"], "workspace");
    assert_eq!(
        status["status"], "waiting",
        "a run says where it stands under the key a goal does: {status}"
    );

    // Its step is answered by the run's id.
    let answered = bisa(
        dir.path(),
        &["--json", "step", "answer", &run, "ask", "-o", "sqlite"],
    );
    assert_ok(&answered, "step answer by run");
    assert_eq!(json(&answered)["status"], "done");

    // Restarted by id: a new run of it, followed until it waits.
    let restarted = bisa(
        dir.path(),
        &["--json", "workflow", "restart", &wf, "--run", &run],
    );
    assert_ok(&restarted, "workflow restart --run");
    let again = json(&restarted)["run"].as_str().unwrap().to_string();
    assert_ne!(again, run);
    // Stopped with every run of it that goes.
    let stopped = bisa(dir.path(), &["--json", "workflow", "stop", &wf]);
    assert_ok(&stopped, "workflow stop");
    assert_eq!(json(&stopped)["runs"], serde_json::json!([again]));
    let none = bisa(dir.path(), &["--json", "workflow", "stop", &wf]);
    assert_eq!(json(&none)["runs"], serde_json::json!([]));

    // A run of another workflow — or no run at all — is refused by name.
    let other = bisa(
        dir.path(),
        &[
            "workflow",
            "stop",
            &wf,
            "--run",
            "01ARZ3NDEKTSV4RRFFQ69G5FAV",
        ],
    );
    assert!(!other.status.success());
    // The goal flags are gone: a run of the workspace asks for none.
    let goal_flag = bisa(dir.path(), &["workflow", "run", &wf, "--statement", "x"]);
    assert!(!goal_flag.status.success());
}

#[test]
fn harness_list_reports_probes() {
    let dir = tempfile::tempdir().unwrap();
    let list = bisa(dir.path(), &["--json", "harness", "list"]);
    assert_ok(&list, "harness list");
    let listings = json(&list);
    let rows = listings["harnesses"].as_array().unwrap();
    // All builtin adapters + presets are always listed regardless of what's
    // installed; probe availability varies per machine.
    let ids: Vec<_> = rows
        .iter()
        .map(|r| r["id"].as_str().unwrap().to_string())
        .collect();
    for expected in [
        "claude-code",
        "codex",
        "pi",
        "omp",
        "opencode",
        "copilot",
        "grok",
        "gemini",
        "preset:goose",
    ] {
        assert!(
            ids.iter().any(|i| i == expected),
            "missing {expected} in {ids:?}"
        );
    }
    for r in rows {
        assert!(r["probe"]["available"].is_boolean());
    }

    // One row per id. `preset:omp` and `preset:opencode` used to shadow the
    // compiled-in adapters of the same name, so this list offered omp three
    // times and two of them could not run.
    let mut sorted = ids.clone();
    sorted.sort();
    let unique = sorted.len();
    sorted.dedup();
    assert_eq!(sorted.len(), unique, "a harness is listed twice in {ids:?}");
    for shadow in ["preset:omp", "preset:opencode"] {
        assert!(
            !ids.iter().any(|i| i == shadow),
            "{shadow} shadows a compiled-in adapter"
        );
    }

    // Every CLI harness says how to run it in a terminal; an ACP target says
    // nothing, because its command speaks a protocol rather than a session.
    let launch_of = |id: &str| {
        rows.iter()
            .find(|r| r["id"].as_str() == Some(id))
            .map(|r| r["launch"].clone())
    };
    assert_eq!(
        launch_of("claude-code").and_then(|l| l["program"].as_str().map(String::from)),
        Some("claude".to_string())
    );
    if let Some(acp) = launch_of("acp:goose") {
        assert!(acp.is_null(), "an ACP target has no interactive form");
    }
    // The three harnesses that speak ACP under an id of their own are rows of
    // their own — never also a generic target — and a person opens each by
    // its bare command, resumed with its own word.
    for (id, program, resume) in [
        ("copilot", "copilot", serde_json::json!(["--continue"])),
        ("grok", "grok", serde_json::json!(["--continue"])),
        (
            "gemini",
            "gemini",
            serde_json::json!(["--resume", "latest"]),
        ),
    ] {
        let launch = launch_of(id).unwrap_or_else(|| panic!("no row for {id}"));
        assert_eq!(launch["program"], program, "{id}");
        assert_eq!(launch["resume_args"], resume, "{id}");
        assert!(launch.get("args").is_none(), "{id}: no protocol word");
        assert!(
            !ids.iter().any(|i| *i == format!("acp:{id}")),
            "{id} is listed twice"
        );
        let row = rows.iter().find(|r| r["id"].as_str() == Some(id)).unwrap();
        assert_eq!(row["tier"], "builtin", "{id}");
        assert_eq!(row["tool_guard"], true, "{id}: the guard can veto a call");
        assert!(
            row["install"]["verify"]
                .as_str()
                .is_some_and(|v| v == format!("{program} --version")),
            "{id} says how it is installed: {row}"
        );
    }
}

/// `status` reads a goal or a run: an id that names neither is refused in
/// one sentence that repeats what was typed.
#[test]
fn an_id_that_names_no_goal_and_no_run_is_a_clear_error() {
    let dir = tempfile::tempdir().unwrap();
    let bad = bisa(dir.path(), &["status", "not-a-ulid"]);
    assert!(!bad.status.success());
    let said = String::from_utf8_lossy(&bad.stderr);
    assert!(
        said.contains("\"not-a-ulid\" names no goal and no run"),
        "{said}"
    );
}

/// Daemon round-trip: start `bisa node` as a child, then drive the same
/// CLI commands — they must route through the socket and behave identically
/// to embedded mode. Where the daemon cannot bind its socket the test fails,
/// saying what the daemon said.
#[test]
fn cli_routes_through_running_node() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());
    let wf = workflow_new(dir.path(), &human_workflow_file(dir.path()));
    let new = bisa(
        dir.path(),
        &[
            "--json",
            "new",
            "node routing test",
            "--title",
            "Node demo",
            "--workflow",
            &wf,
            "--no-start",
        ],
    );
    assert_ok(&new, "new");
    let id = json(&new)["goal"].as_str().unwrap().to_string();
    let node = spawn_daemon(dir.path());

    // Start the run through the daemon: the step's gate must live where the
    // work will run, not in a CLI process that exits.
    let run = bisa(dir.path(), &["--json", "run", &id]);
    assert_ok(&run, "run via node");
    assert_eq!(json(&run)["status"], "waiting");

    // status via node == embedded shape.
    let via_node = bisa(dir.path(), &["--json", "status", &id]);
    assert_ok(&via_node, "status via node");
    let embedded = bisa(dir.path(), &["--json", "--no-node", "status", &id]);
    assert_ok(&embedded, "status embedded");
    let a = json(&via_node);
    let b = json(&embedded);
    assert_eq!(a["status"], b["status"]);
    assert_eq!(a["run"]["id"], b["run"]["id"]);

    // inbox via node: a conversation row carrying the live question, bound
    // to its step.
    let inbox = bisa(dir.path(), &["--json", "inbox"]);
    assert_ok(&inbox, "inbox via node");
    let rows = json(&inbox)["rows"].as_array().cloned().unwrap();
    let row = rows
        .iter()
        .find(|r| r["key"] == serde_json::json!(id))
        .unwrap_or_else(|| panic!("goal row missing via node: {rows:?}"));
    assert!(
        row["needs_action"].as_array().is_some_and(|a| a
            .iter()
            .any(|n| n["gate_kind"] == "escalation" && n["step"] == "ask")),
        "the live question should surface via the daemon: {row}"
    );

    // answer via node: durable effects identical to the embedded answer.
    let answered = bisa(
        dir.path(),
        &["--json", "step", "answer", &id, "ask", "-o", "sqlite"],
    );
    assert_ok(&answered, "step answer via node");
    let a = json(&answered);
    assert_eq!(a["status"], serde_json::json!("done"));

    // log via node renders journal events.
    let log = bisa(dir.path(), &["--json", "log", &id]);
    assert_ok(&log, "log via node");
    assert!(json(&log)["events"].as_array().unwrap().len() >= 3);

    drop(node);
}

/// `bisa node` as a child over `dir`, once its socket is bound — either path
/// form. A socket that never binds **fails the test, in the daemon's own
/// words**: a run that could not start the daemon proved nothing about it,
/// and a test that returned there would read as one that passed.
fn spawn_daemon(dir: &Path) -> Daemon {
    let mut node = Command::new(env!("CARGO_BIN_EXE_bisa"))
        .arg("--data-dir")
        .arg(dir)
        .arg("--file-keys")
        .arg("node")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn node");
    for _ in 0..100 {
        if daemon_socket(dir).exists() {
            return Daemon(node);
        }
        if let Ok(Some(_)) = node.try_wait() {
            break; // the daemon ended early — the bind failed
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    end_daemon(&mut node);
    let mut said = String::new();
    if let Some(mut stderr) = node.stderr.take() {
        let _read_what_there_is = std::io::Read::read_to_string(&mut stderr, &mut said);
    }
    panic!(
        "the daemon's socket never bound under {}; the daemon said:\n{said}",
        dir.display()
    );
}

/// Where the daemon's socket lands: the preferred path, or — on a long
/// tempdir — the short one its pointer file names.
fn daemon_socket(dir: &Path) -> std::path::PathBuf {
    let preferred = bisa_store::Paths::new(dir).node_socket();
    if preferred.exists() {
        return preferred;
    }
    std::fs::read_to_string(bisa_node::pointer_path(&preferred))
        .map(|p| std::path::PathBuf::from(p.trim()))
        .unwrap_or(preferred)
}

/// A daemon a test started, ended when the test lets go of it — at its
/// close, or when an assertion failed on the way there. A test that failed
/// used to leave its daemon running, under a folder that was already gone,
/// for as long as the machine stayed up.
struct Daemon(std::process::Child);

impl Drop for Daemon {
    fn drop(&mut self) {
        end_daemon(&mut self.0);
    }
}

/// End a daemon a test started, and reap it — an end that fails is a daemon
/// already gone.
fn end_daemon(node: &mut std::process::Child) {
    let _already_gone = node.kill();
    let _reaped = node.wait();
}

/// An install through the running daemon lands on its bus: the desktop,
/// listening on `/events`, draws the addon without being asked to look
/// again — which a write straight into the store could never make happen.
#[test]
fn a_catalog_install_with_a_running_node_goes_through_it_and_is_announced() {
    use http_body_util::BodyExt as _;
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());
    let node = spawn_daemon(dir.path());
    let rt = tokio::runtime::Runtime::new().unwrap();
    let announced = rt.block_on(async {
        let socket = daemon_socket(dir.path());
        let token = std::fs::read_to_string(bisa_store::Paths::new(dir.path()).token_file())
            .expect("the daemon wrote its token");
        let stream = tokio::net::UnixStream::connect(&socket).await.unwrap();
        let (mut sender, conn) =
            hyper::client::conn::http1::handshake(hyper_util::rt::TokioIo::new(stream))
                .await
                .unwrap();
        tokio::spawn(conn);
        let req = hyper::Request::builder()
            .method("GET")
            .uri("/events")
            .header(hyper::header::HOST, "localhost")
            .header(
                hyper::header::AUTHORIZATION,
                format!("Bearer {}", token.trim()),
            )
            .body(http_body_util::Empty::<hyper::body::Bytes>::new())
            .unwrap();
        let resp = sender.send_request(req).await.unwrap();
        assert!(resp.status().is_success(), "{}", resp.status());
        // The stream is open: now the install, from another thread, as a
        // person's shell would run it.
        let data_dir = dir.path().to_path_buf();
        let install = tokio::task::spawn_blocking(move || {
            bisa(
                &data_dir,
                &["--json", "catalog", "install", "addon", "clock"],
            )
        });
        let mut body = resp.into_body();
        let mut frames = bisa_http::SseFrames::default();
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(20);
        let mut announced = false;
        while !announced {
            let Ok(Some(Ok(frame))) = tokio::time::timeout_at(deadline, body.frame()).await else {
                break;
            };
            if let Some(data) = frame.data_ref() {
                frames
                    .push(data)
                    .expect("the node's stream is one the platform reads");
            }
            while let Some(event) = frames.next_event() {
                let v: serde_json::Value = serde_json::from_str(&event.data).unwrap_or_default();
                // The envelope carries the engine event whole: its own
                // `payload` is where the tag lives.
                if v["stream"] == "engine" && v["payload"]["payload"]["type"] == "addons_changed" {
                    announced = true;
                }
            }
        }
        let out = install.await.unwrap();
        assert_ok(&out, "catalog install via node");
        let installed = json(&out)["installed"].clone();
        assert!(
            installed["addons"].as_array().is_some_and(|a| a
                .iter()
                .any(|s| s.as_str().is_some_and(|s| s.contains("clock")))),
            "the CLI reports what landed, in the store's shape: {installed}"
        );
        announced
    });
    drop(node);
    assert!(announced, "the daemon's bus never said addons_changed");
}

// ---------------------------------------------------------------------------
// M5: the guided cycle, agents/teams, and the Studio surface
// ---------------------------------------------------------------------------

/// Auto is the default: the goal is handed to the Workflow Agent — one of
/// the two core agents a workspace always holds a record of — to design its workflow, and the
/// platform runs it. `--mode guided` has the person adopt, `--mode manual`
/// hands the design to them; a word that is not a mode is refused. (No
/// harness runs here — we assert the durable field and the words only.)
#[test]
fn new_is_guided_unless_manual() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());

    let auto = bisa(dir.path(), &["--json", "new", "Ship the pricing page"]);
    assert_ok(&auto, "new auto");
    let g = json(&auto);
    assert_eq!(g["mode"], "auto", "auto is the default experience");

    let guided = bisa(
        dir.path(),
        &["--json", "new", "Walk me through it", "--mode", "guided"],
    );
    assert_ok(&guided, "new guided");
    assert_eq!(json(&guided)["mode"], "guided");

    let manual = bisa(
        dir.path(),
        &["--json", "new", "Do it myself", "--mode", "manual"],
    );
    assert_ok(&manual, "new manual");
    assert_eq!(json(&manual)["mode"], "manual");

    let bogus = bisa(
        dir.path(),
        &["--json", "new", "Nope", "--mode", "interactive"],
    );
    assert!(
        !bogus.status.success(),
        "a word that is not a mode is refused"
    );

    // The human-facing text points at the next step rather than a command list.
    let prose = bisa(dir.path(), &["new", "Another one"]);
    assert_ok(&prose, "new prose");
    assert!(
        stdout(&prose).contains("Workflow Agent"),
        "guided capture should name who takes it from here: {}",
        stdout(&prose)
    );
    let manual_prose = bisa(dir.path(), &["new", "By hand", "--mode", "manual"]);
    assert_ok(&manual_prose, "new prose manual");
    assert!(
        stdout(&manual_prose).contains("Workflow tab")
            && stdout(&manual_prose).contains("workflow use"),
        "a manual draft says how to design or pick a workflow: {}",
        stdout(&manual_prose)
    );
}

/// A question reaches the inbox and `bisa answer` resolves it by goal
/// id, carrying the option the human picked and the sentence they added —
/// and refusing an option the question never offered. `answer` is the
/// inbox's verb; `step answer` names the step, and they meet at the same
/// decision.
#[test]
fn answering_a_question_carries_the_selection_and_the_text() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());
    let wf = workflow_new(dir.path(), &human_workflow_file(dir.path()));
    let new = bisa(
        dir.path(),
        &["--json", "new", "Question me", "--workflow", &wf],
    );
    assert_ok(&new, "new");
    let id = json(&new)["goal"].as_str().unwrap().to_string();

    // The inbox surfaces the waiting conversation.
    let inbox = bisa(dir.path(), &["--json", "inbox"]);
    assert_ok(&inbox, "inbox");
    assert!(
        json(&inbox)["rows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["key"] == id.as_str()),
        "a waiting goal belongs in the inbox"
    );

    // An option id the question never offered is refused, not dropped: a
    // selection that quietly resolves to nothing is an answer nobody gave.
    let bogus = bisa(dir.path(), &["--json", "answer", &id, "-o", "mysql"]);
    assert!(
        !bogus.status.success(),
        "an unoffered option id must refuse"
    );

    // Nor is an empty answer an answer. `answer <id> ""` used to journal
    // `Some("")`.
    let nothing = bisa(dir.path(), &["--json", "answer", &id, ""]);
    assert!(!nothing.status.success(), "an empty answer must refuse");

    // Picking an option and qualifying it is one answer, not two.
    let answered = bisa(
        dir.path(),
        &[
            "--json",
            "answer",
            &id,
            "for full-text search",
            "-o",
            "postgres",
        ],
    );
    assert_ok(&answered, "answer");
    assert_eq!(json(&answered)["answered"], true);
    assert_eq!(json(&answered)["status"], "done");

    // The answer is durable, in the goal's own journal — both halves, and on
    // the step's own record.
    let log = bisa(dir.path(), &["--json", "log", &id]);
    assert_ok(&log, "log");
    let text = stdout(&log);
    assert!(
        text.contains("postgres") && text.contains("for full-text search"),
        "the answer must be journaled whole: {text}"
    );
    let status = json(&bisa(dir.path(), &["--json", "status", &id]));
    assert_eq!(
        status["run"]["steps"]["ask"]["answer"]["selected"],
        serde_json::json!(["postgres"])
    );
}

/// "I'm not sure" is a real answer on the command line: it journals, it does
/// not decline, and the step keeps waiting for a real answer.
#[test]
fn answering_a_question_with_not_sure_is_not_a_decline() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());
    let wf = workflow_new(dir.path(), &human_workflow_file(dir.path()));
    let new = bisa(
        dir.path(),
        &["--json", "new", "ship the thing", "--workflow", &wf],
    );
    assert_ok(&new, "new");
    let id = json(&new)["goal"].as_str().unwrap().to_string();

    let answered = bisa(dir.path(), &["--json", "answer", &id, "--unsure"]);
    assert_ok(&answered, "answer --unsure");
    assert_eq!(json(&answered)["answered"], true);

    let ws = open_ws(dir.path());
    let recorded = ws
        .journal(&bisa_core::Home::Goal {
            goal: id.parse().unwrap(),
        })
        .unwrap()
        .iter()
        .rev()
        .find_map(|e| match &e.payload {
            bisa_core::event::JournalPayload::Decision {
                answer, approve, ..
            } => Some((answer.clone(), *approve)),
            _ => None,
        })
        .expect("the decision is journaled");
    assert!(recorded.0.unwrap().unsure, "\"not sure\" is what was said");
    assert!(recorded.1, "and it is not a decline");
    // The step is still the person's to answer: not-sure re-opens the
    // question rather than failing the run.
    let status = json(&bisa(dir.path(), &["--json", "status", &id]));
    assert_eq!(status["status"], "waiting", "{status}");
}

fn open_ws(dir: &Path) -> bisa_store::Workspace {
    bisa_store::Workspace::open_with_keystore(
        dir,
        Box::new(bisa_store::FileKeyStore::new(
            bisa_store::Paths::new(dir).identity_dir(),
        )),
    )
    .expect("open workspace")
}

/// An agent is a definition: prompt + harness + model plan, plus **references**
/// into the skill library and the MCP registry. The bodies live in the
/// library; the agent carries ids.
#[test]
fn agent_definition_roundtrip_with_a_skill() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());
    let procedure = dir.path().join("review.md");
    std::fs::write(&procedure, "# Review\nAlways check the tests first.").unwrap();
    assert_ok(
        &bisa(
            dir.path(),
            &[
                "skill",
                "add",
                "--id",
                "review",
                "--name",
                "Review",
                "--description",
                "Use when reading someone else's diff.",
                "--file",
                procedure.to_str().unwrap(),
            ],
        ),
        "skill add",
    );

    let added = bisa(
        dir.path(),
        &[
            "--json",
            "agent",
            "add",
            "--name",
            "Reviewer",
            "--prompt",
            "You review code changes.",
            "--harness",
            "claude-code",
            "--model",
            "some-model",
            "--model",
            "spare-model=3",
            "--strategy",
            "weighted",
            "--skill",
            "review",
        ],
    );
    assert_ok(&added, "agent add");
    let a = json(&added)["agent"].clone();
    let agent_id = a["id"].as_str().unwrap().to_string();
    assert_eq!(a["harness"], "claude-code");
    assert_eq!(a["models"]["strategy"], "weighted");
    assert_eq!(a["models"]["models"][0]["model"], "some-model");
    assert_eq!(a["models"]["models"][1]["model"], "spare-model");
    assert_eq!(
        a["models"]["models"][1]["weight"], 3,
        "`--model name=weight` carries the weight: {a}"
    );
    assert_eq!(
        a["skills"],
        serde_json::json!(["review"]),
        "an agent carries the skill's id, not its markdown: {a}"
    );
    assert!(
        a["pubkey"].as_str().is_some_and(|k| k.len() == 64),
        "every agent gets its own key: {a}"
    );

    let listed = bisa(dir.path(), &["--json", "agent", "list"]);
    assert_ok(&listed, "agent list");
    assert!(json(&listed)["agents"]
        .as_array()
        .unwrap()
        .iter()
        .any(|x| x["id"] == agent_id.as_str()));

    let skills = bisa(dir.path(), &["--json", "agent", "skill", "list", &agent_id]);
    assert_ok(&skills, "agent skill list");
    assert_eq!(json(&skills)["skills"], serde_json::json!(["review"]));

    assert_ok(&bisa(dir.path(), &["agent", "rm", &agent_id]), "agent rm");
    let after = bisa(dir.path(), &["--json", "agent", "list"]);
    assert!(!json(&after)["agents"]
        .as_array()
        .unwrap()
        .iter()
        .any(|x| x["id"] == agent_id.as_str()));
}

/// `--decision-making` is the agent's own switch: a flag on `add`, and on
/// `edit` a word that says on or off.
#[test]
fn the_decision_making_switch_is_a_flag_on_add_and_a_word_on_edit() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());

    let plain = bisa(
        dir.path(),
        &["--json", "agent", "add", "--name", "Plain", "--prompt", "p"],
    );
    assert_ok(&plain, "agent add");
    assert_eq!(
        json(&plain)["agent"]["decision_making"],
        false,
        "off unless it is asked for"
    );

    let added = bisa(
        dir.path(),
        &[
            "--json",
            "agent",
            "add",
            "--name",
            "Picker",
            "--prompt",
            "You pick.",
            "--decision-making",
        ],
    );
    assert_ok(&added, "agent add --decision-making");
    let a = json(&added)["agent"].clone();
    assert_eq!(a["decision_making"], true, "{a}");
    let id = a["id"].as_str().unwrap().to_string();

    let edited = bisa(
        dir.path(),
        &["--json", "agent", "edit", &id, "--decision-making", "false"],
    );
    assert_ok(&edited, "agent edit --decision-making false");
    assert_eq!(json(&edited)["agent"]["decision_making"], false);

    // One of the three things a core agent may change.
    let core = bisa(
        dir.path(),
        &[
            "--json",
            "agent",
            "edit",
            "general-agent",
            "--decision-making",
            "true",
        ],
    );
    assert_ok(&core, "agent edit general-agent --decision-making true");
    assert_eq!(json(&core)["agent"]["decision_making"], true);
}

/// The retired spelling of the flag is a flag the command does not know.
#[test]
fn the_retired_flag_is_a_usage_error() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());
    let retired = bisa(
        dir.path(),
        &["agent", "edit", "general-agent", "--decision-maker", "true"], // terminology-lint-ignore: decision-maker - proves the retired word is refused
    );
    assert!(!retired.status.success(), "a flag nobody declared");
    let agents = json(&bisa(dir.path(), &["--json", "agent", "list"]));
    assert!(agents["agents"]
        .as_array()
        .unwrap()
        .iter()
        .all(|a| a["decision_making"] == false));
}

/// Teams mix agents and humans.
#[test]
fn team_gathers_agents_and_humans() {
    let dir = tempfile::tempdir().unwrap();
    let init = bisa(dir.path(), &["--json", "init"]);
    assert_ok(&init, "init");
    let me = json(&init)["pubkey"].as_str().unwrap().to_string();

    let agent = bisa(
        dir.path(),
        &[
            "--json",
            "agent",
            "add",
            "--name",
            "Builder",
            "--prompt",
            "You build things.",
        ],
    );
    assert_ok(&agent, "agent add");
    let agent_id = json(&agent)["agent"]["id"].as_str().unwrap().to_string();

    let team = bisa(
        dir.path(),
        &[
            "--json",
            "team",
            "create",
            "Launch crew",
            "--purpose",
            "Ship v1",
            "--agent",
            &agent_id,
        ],
    );
    assert_ok(&team, "team create");
    let team_id = json(&team)["team"]["id"].as_str().unwrap().to_string();

    let with_human = bisa(
        dir.path(),
        &["--json", "team", "add-member", &team_id, "--human", &me],
    );
    assert_ok(&with_human, "team add-member");
    let members = json(&with_human)["team"]["members"].clone();
    assert_eq!(
        members.as_array().unwrap().len(),
        2,
        "a team is agents *and* humans: {members}"
    );

    let listed = bisa(dir.path(), &["--json", "team", "list"]);
    assert_ok(&listed, "team list");
    assert!(json(&listed)["teams"]
        .as_array()
        .unwrap()
        .iter()
        .any(|t| t["id"] == team_id.as_str()));

    // A team is only real once it can carry a goal.
    let goal = bisa(
        dir.path(),
        &["--json", "new", "ship the thing", "--mode", "manual"],
    );
    assert_ok(&goal, "new");
    let goal_id = json(&goal)["goal"].as_str().unwrap().to_string();

    let assigned = bisa(
        dir.path(),
        &["--json", "assign", &goal_id, &format!("team:{team_id}")],
    );
    assert_ok(&assigned, "assign");
    assert_eq!(
        json(&assigned)["assignees"],
        serde_json::json!([format!("team:{team_id}")])
    );

    let shown = bisa(dir.path(), &["--json", "team", "show", &team_id]);
    assert_ok(&shown, "team show");
    let carried = json(&shown)["goals"].clone();
    assert_eq!(
        carried.as_array().unwrap().len(),
        1,
        "team show lists what it carries: {carried}"
    );

    let status = bisa(dir.path(), &["--json", "status", &goal_id]);
    assert_ok(&status, "status");
    assert_eq!(
        json(&status)["goal"]["assignees"],
        serde_json::json!([{"team": team_id}])
    );

    // A goal is carried only by somebody the workspace has — with no node
    // running too: the engine's rule, not a screen's.
    let ghost = bisa(
        dir.path(),
        &["--json", "assign", &goal_id, "agent:nobody-here"],
    );
    assert_eq!(ghost.status.code(), Some(1), "{}", stderr(&ghost));
    assert!(ghost.stdout.is_empty());
    assert!(stderr(&ghost).contains("nobody-here"), "{}", stderr(&ghost));
    let status = bisa(dir.path(), &["--json", "status", &goal_id]);
    assert_eq!(
        json(&status)["goal"]["assignees"],
        serde_json::json!([{"team": team_id}]),
        "a refusal changes nothing"
    );

    let cleared = bisa(dir.path(), &["--json", "unassign", &goal_id]);
    assert_ok(&cleared, "unassign");
    let shown = bisa(dir.path(), &["--json", "team", "show", &team_id]);
    assert!(json(&shown)["goals"].as_array().unwrap().is_empty());
}

/// Channels carry messages; reading marks them read.
#[test]
fn channel_messages_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());

    let created = bisa(dir.path(), &["--json", "channels", "create", "design"]);
    assert_ok(&created, "channels create");
    let channel = json(&created)["channel"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    assert_ok(
        &bisa(dir.path(), &["msg", &channel, "first thoughts"]),
        "msg",
    );
    assert_ok(
        &bisa(dir.path(), &["msg", &channel, "second pass"]),
        "msg 2",
    );

    let msgs = bisa(dir.path(), &["--json", "msgs", &channel]);
    assert_ok(&msgs, "msgs");
    let rows = json(&msgs)["messages"].as_array().cloned().unwrap();
    assert_eq!(rows.len(), 2, "both messages: {rows:?}");
    assert!(rows.iter().any(|m| m["content"] == "first thoughts"));

    let listed = bisa(dir.path(), &["--json", "channels", "list"]);
    assert_ok(&listed, "channels list");
    assert!(json(&listed)["channels"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c["id"] == channel.as_str()));
}

/// `channels list` names the rooms you can join and `dm list` names the
/// conversations you are in, and neither shows the other's rows.
///
/// M12 narrowed `channels list` to standing channels — a DM used to print with a
/// literal `Dm` in the kind column — and pointed at `bisa dm list` in a
/// comment. That command did not exist until M13, so for a milestone the CLI
/// had no way to list a DM at all: this asserts both halves, because a
/// listing that excludes something is only correct if something else includes
/// it.
#[test]
fn a_dm_is_listed_by_dm_list_and_never_by_the_channel_list() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());

    let created = bisa(dir.path(), &["--json", "channels", "create", "design"]);
    assert_ok(&created, "channels create");
    let channel = json(&created)["channel"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Any principal that is not the owner opens a real DM; an agent's own key
    // is the one a workspace hands out without a second machine.
    let agent = bisa(
        dir.path(),
        &[
            "--json",
            "agent",
            "add",
            "--name",
            "Scout",
            "--prompt",
            "You scout.",
        ],
    );
    assert_ok(&agent, "agent add");
    let peer = json(&agent)["agent"]["pubkey"]
        .as_str()
        .unwrap()
        .to_string();
    let sent = bisa(
        dir.path(),
        &["--json", "dm", "send", &peer, "--text", "just us"],
    );
    assert_ok(&sent, "dm send");
    let dm_id = json(&sent)["channel"].as_str().unwrap().to_string();

    let listed = bisa(dir.path(), &["--json", "channels", "list"]);
    assert_ok(&listed, "channels list");
    let rows = json(&listed)["channels"].as_array().cloned().unwrap();
    // `general` is the workspace's own room and the one made here; the DM
    // is neither.
    assert_eq!(rows.len(), 2, "the DM is not a channel: {rows:?}");
    assert!(rows.iter().any(|c| c["id"] == channel.as_str()), "{rows:?}");
    assert!(rows.iter().all(|c| c["id"] != dm_id.as_str()), "{rows:?}");
    assert!(
        rows.iter().all(|c| c["kind"] == "standing"),
        "channels list is standing-only: {rows:?}"
    );

    let dms = bisa(dir.path(), &["--json", "dm", "list"]);
    assert_ok(&dms, "dm list");
    let rows = json(&dms)["dms"].as_array().cloned().unwrap();
    assert_eq!(rows.len(), 1, "the one DM, and only it: {rows:?}");
    assert_eq!(rows[0]["id"], dm_id.as_str());
    assert_eq!(
        rows[0]["kind"], "direct",
        "the wire word is the Rust enum's"
    );
    assert!(
        rows.iter().all(|c| c["id"] != channel.as_str()),
        "the standing channel is not a DM: {rows:?}"
    );

    // The human rendering carries the id, because it is what every other
    // message command takes as its scope.
    let human = bisa(dir.path(), &["dm", "list"]);
    assert_ok(&human, "dm list (human)");
    assert!(stdout(&human).contains(&dm_id), "{}", stdout(&human));
}

/// Pulse renders the workspace timeline; sessions is empty without a daemon.
#[test]
fn pulse_and_sessions_render() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());
    assert_ok(
        &bisa(
            dir.path(),
            &["new", "Something happened", "--mode", "manual"],
        ),
        "new",
    );

    let pulse = bisa(dir.path(), &["--json", "pulse"]);
    assert_ok(&pulse, "pulse");
    assert!(
        !json(&pulse)["rows"].as_array().unwrap().is_empty(),
        "capturing a goal is workspace activity"
    );

    let sessions = bisa(dir.path(), &["--json", "sessions", "list"]);
    assert_ok(&sessions, "sessions list");
    assert!(json(&sessions)["sessions"].as_array().unwrap().is_empty());
}

// ---------------------------------------------------------------------------
// Projects and workstreams
//
// Every repository here is created by the test inside a `tempfile` directory
// and dies with it; "origin" is a local bare repository addressed by its path,
// so the push path runs with no network and no user repository is touched.
// ---------------------------------------------------------------------------

fn raw_git(dir: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("spawn git");
    assert!(
        out.status.success(),
        "git {args:?} in {}: {}",
        dir.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// Identity in the repository's *local* config — never the developer's global.
fn set_identity(repo: &Path) {
    raw_git(repo, &["config", "user.name", "Bisa Test"]);
    raw_git(repo, &["config", "user.email", "test@example.invalid"]);
}

fn origin_branches(origin: &Path) -> Vec<String> {
    raw_git(
        origin,
        &["for-each-ref", "--format=%(refname:short)", "refs/heads"],
    )
    .lines()
    .map(str::to_string)
    .collect()
}

/// A goal with a git project under it, holding one commit — the state a
/// workstream can branch from. The first commit is raw git because the platform
/// has no command for it: creating the project goes as far as `git init`.
fn git_project(dir: &Path, slug: &str, publish: &str) -> (String, String, std::path::PathBuf) {
    init(dir);
    let goal = bisa(dir, &["--json", "new", "own some code", "--mode", "manual"]);
    assert_ok(&goal, "new");
    let goal = json(&goal)["goal"].as_str().unwrap().to_string();

    let made = bisa(
        dir,
        &[
            "--json",
            "project",
            "new",
            "--goal",
            &goal,
            slug,
            "--publish",
            publish,
        ],
    );
    assert_ok(&made, "project new");
    let made = json(&made);
    let pid = made["project"]["id"].as_str().unwrap().to_string();
    let root = std::path::PathBuf::from(made["path"].as_str().unwrap());
    set_identity(&root);
    std::fs::write(root.join("README.md"), "baseline\n").unwrap();
    raw_git(&root, &["add", "-A"]);
    raw_git(&root, &["commit", "-m", "baseline", "--quiet"]);
    (goal, pid, root)
}

/// A git repository outside the workspace, with one commit — the folder
/// somebody points `project import` or `project adopt` at.
fn outside_repo(subject: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    raw_git(root, &["init", "--quiet"]);
    set_identity(root);
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
    std::fs::write(root.join("README.md"), "# storefront\n").unwrap();
    raw_git(root, &["add", "-A"]);
    raw_git(root, &["commit", "-m", subject, "--quiet"]);
    dir
}

/// Every entry's relative path, kind and exact bytes. Compared before and
/// after, an equal snapshot is the proof that an import only *read* the folder
/// it was given — a file count or an `mtime` would pass for a copy that
/// quietly rewrote a line.
fn tree_snapshot(root: &Path) -> Vec<(String, Vec<u8>)> {
    fn walk(base: &Path, dir: &Path, out: &mut Vec<(String, Vec<u8>)>) {
        let mut entries: Vec<_> = std::fs::read_dir(dir)
            .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
            .map(|e| e.unwrap())
            .collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let path = entry.path();
            let rel = path.strip_prefix(base).unwrap().display().to_string();
            let kind = entry.file_type().unwrap();
            if kind.is_symlink() {
                let target = std::fs::read_link(&path).unwrap();
                out.push((
                    format!("{rel} symlink"),
                    target.display().to_string().into_bytes(),
                ));
            } else if kind.is_dir() {
                out.push((format!("{rel} dir"), Vec::new()));
                walk(base, &path, out);
            } else {
                out.push((format!("{rel} file"), std::fs::read(&path).unwrap()));
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out
}

/// `bisa project import <goal> <path>` — the tree comes across with its
/// history, as a managed root, and the folder it came from is untouched.
#[test]
fn an_import_copies_the_tree_and_leaves_the_source_folder_untouched() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());
    let goal = json(&bisa(
        dir.path(),
        &["--json", "new", "own some code", "--mode", "manual"],
    ))["goal"]
        .as_str()
        .unwrap()
        .to_string();

    let src = outside_repo("the history that must survive");
    let source = src.path().display().to_string();
    let before = tree_snapshot(src.path());

    let out = bisa(
        dir.path(),
        &[
            "--json",
            "project",
            "import",
            "--goal",
            &goal,
            &source,
            "--slug",
            "storefront",
        ],
    );
    assert_ok(&out, "project import");
    let made = json(&out);
    assert_eq!(
        made["project"]["root"]["type"],
        serde_json::json!("managed")
    );
    assert_eq!(made["project"]["vcs"]["type"], serde_json::json!("git"));

    let dest = std::path::PathBuf::from(made["path"].as_str().unwrap());
    assert!(
        dest.starts_with(dir.path()),
        "the copy lives in the workspace"
    );
    assert_eq!(
        std::fs::read_to_string(dest.join("src/main.rs")).unwrap(),
        "fn main() {}\n"
    );
    assert!(dest.join(".git").is_dir(), "history comes across");
    assert_eq!(
        raw_git(&dest, &["log", "--format=%s"]),
        "the history that must survive"
    );

    assert_eq!(
        tree_snapshot(src.path()),
        before,
        "an import only reads the folder it was given"
    );

    // A second import onto the same slug is refused twice over — the record
    // collides, and behind it the folder is already there.
    let again = bisa(
        dir.path(),
        &[
            "project",
            "import",
            "--goal",
            &goal,
            &source,
            "--slug",
            "storefront",
        ],
    );
    assert!(!again.status.success(), "a second import must not merge in");

    // Adopting the same folder still writes nothing into it.
    let adopted = bisa(
        dir.path(),
        &[
            "--json", "project", "adopt", "--goal", &goal, &source, "--slug", "inplace",
        ],
    );
    assert_ok(&adopted, "project adopt");
    assert_eq!(
        json(&adopted)["project"]["root"]["type"],
        serde_json::json!("external")
    );
    assert_eq!(
        tree_snapshot(src.path()),
        before,
        "linking a folder in place writes nothing into it"
    );
}

/// Adopt writes nothing — until the person asks. `project git-init` is the
/// ask: the `.git` lands in their folder, the record turns git, and a second
/// ask is refused rather than repeated.
#[test]
fn project_git_init_turns_a_plain_project_into_a_repository_and_refuses_twice() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());
    let src = tempfile::tempdir().unwrap();
    std::fs::write(src.path().join("NOTES.md"), "not ours\n").unwrap();
    let source = src.path().display().to_string();

    let adopted = bisa(
        dir.path(),
        &["--json", "project", "adopt", &source, "--slug", "theirs"],
    );
    assert_ok(&adopted, "project adopt");
    let pid = json(&adopted)["project"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        json(&adopted)["project"]["vcs"]["type"],
        serde_json::json!("none")
    );
    assert!(!src.path().join(".git").exists(), "adopt writes nothing");

    let done = bisa(dir.path(), &["--json", "project", "git-init", &pid]);
    assert_ok(&done, "project git-init");
    let v = json(&done);
    assert_eq!(v["project"]["vcs"]["type"], serde_json::json!("git"));
    assert_eq!(v["project"]["root"]["type"], serde_json::json!("external"));
    assert!(
        src.path().join(".git").is_dir(),
        "the one write adopt allows"
    );
    assert_eq!(
        std::fs::read_to_string(src.path().join("NOTES.md")).unwrap(),
        "not ours\n"
    );

    let again = bisa(dir.path(), &["project", "git-init", &pid]);
    assert!(!again.status.success(), "already a repository");
    assert!(
        String::from_utf8_lossy(&again.stderr).contains("already a repository"),
        "{}",
        String::from_utf8_lossy(&again.stderr)
    );

    let bad = bisa(
        dir.path(),
        &[
            "project",
            "git-init",
            &pid,
            "--git-config",
            "core.hooksPath=x",
        ],
    );
    assert!(
        !bad.status.success(),
        "a key the platform does not write is a usage error"
    );
}

/// `bisa doctor` on a fresh workspace: five checks in their order, each with
/// a state, a title and a detail; the Decision-Making Agent is off and so
/// unready; the exit code says whether everything is here. What this machine
/// has installed is its own business — the shape is what is held, and the
/// Decision-Making Agent's word, which no machine changes on a fresh
/// workspace.
#[test]
fn doctor_names_the_five_checks_and_exits_one_while_something_is_missing() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());
    let out = bisa(dir.path(), &["--json", "doctor"]);
    let v = json(&out);
    let ids: Vec<&str> = v["checks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids,
        [
            "git",
            "harness",
            "decision_making_agent",
            "general_agent",
            "workflow_agent"
        ]
    );
    for c in v["checks"].as_array().unwrap() {
        assert!(["ready", "missing", "unready"].contains(&c["state"].as_str().unwrap()));
        assert!(c["title"].as_str().is_some_and(|t| !t.is_empty()));
        assert!(c["detail"].as_str().is_some_and(|d| !d.is_empty()));
    }
    let dm = &v["checks"][2];
    assert_eq!(dm["id"], serde_json::json!("decision_making_agent"));
    assert_eq!(dm["title"], serde_json::json!("The Decision-Making Agent"));
    assert_eq!(
        dm["state"],
        serde_json::json!("unready"),
        "off on a fresh workspace"
    );
    assert!(dm["detail"]
        .as_str()
        .unwrap()
        .starts_with("It is switched off."));
    assert_eq!(
        dm["door"],
        serde_json::json!({"door": "settings", "tab": "decision-making"})
    );
    assert_eq!(
        v["checks"][3]["door"],
        serde_json::json!({"door": "agents"})
    );
    assert_eq!(
        v["ready"],
        serde_json::json!(false),
        "the Decision-Making Agent alone keeps it from ready"
    );
    assert!(!out.status.success(), "something is missing: exit 1");
    let harness = &v["checks"][1];
    if harness["state"] == "missing" {
        assert_eq!(
            harness["hint"]["url"],
            serde_json::json!("https://code.claude.com/docs/en/setup")
        );
    }
}

/// A relative path, a file and a folder inside the workspace: three sources an
/// import refuses, each naming why.
#[test]
fn an_import_refuses_a_source_it_has_no_business_copying() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());
    let goal = json(&bisa(
        dir.path(),
        &["--json", "new", "own some code", "--mode", "manual"],
    ))["goal"]
        .as_str()
        .unwrap()
        .to_string();

    let inside = dir.path().join("not-a-project");
    std::fs::create_dir_all(&inside).unwrap();
    let file = tempfile::NamedTempFile::new().unwrap();

    for (path, expected) in [
        (inside.display().to_string(), "inside the Bisa workspace"),
        ("../elsewhere".to_string(), "absolute path"),
        (file.path().display().to_string(), "not a directory"),
    ] {
        let out = bisa(
            dir.path(),
            &[
                "project", "import", "--goal", &goal, &path, "--slug", "nope",
            ],
        );
        assert!(!out.status.success(), "{path} should be refused");
        let err = String::from_utf8_lossy(&out.stderr).to_string();
        assert!(err.contains(expected), "{path}: refusal was {err:?}");
    }

    // And nothing was recorded for any of them.
    let listed = json(&bisa(dir.path(), &["--json", "project", "list"]));
    assert!(
        listed["projects"].as_array().unwrap().is_empty(),
        "a refused import must not leave a record: {listed}"
    );
}

/// The whole loop from the terminal: a project, a workstream, a commit, and the
/// tidy-up — with `git init` happening only where it was asked for.
#[test]
fn project_and_workstream_lifecycle() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());
    let goal = json(&bisa(
        dir.path(),
        &["--json", "new", "run a shop", "--mode", "manual"],
    ))["goal"]
        .as_str()
        .unwrap()
        .to_string();

    // A managed root is a folder Bisa made, so it is a repository from
    // the start — no flag, and nothing outside it touched.
    let plain = bisa(
        dir.path(),
        &["--json", "project", "new", "--goal", &goal, "notes"],
    );
    assert_ok(&plain, "project new");
    let plain_path = std::path::PathBuf::from(json(&plain)["path"].as_str().unwrap());
    assert!(plain_path.is_dir());
    assert!(
        plain_path.join(".git").is_dir(),
        "a managed project is a repository without being asked"
    );

    // A traversing slug never becomes a directory.
    for bad in ["../escape", "/etc/passwd", "..", "a/b", "\u{0430}pi"] {
        let out = bisa(
            dir.path(),
            &["--json", "project", "new", "--goal", &goal, bad],
        );
        assert!(!out.status.success(), "slug {bad:?} should be refused");
    }
    assert!(!dir.path().parent().unwrap().join("escape").exists());

    // A git project in the same workspace, with the first commit made by the
    // test — creating the project goes as far as `git init` and no further.
    let (_owner, pid, root) = git_project(dir.path(), "storefront", "gated");
    let data_dir = dir.path();

    let opened = bisa(
        data_dir,
        &[
            "--json",
            "workstream",
            "open",
            &pid,
            "--label",
            "cart total",
        ],
    );
    assert_ok(&opened, "workstream open");
    let w = json(&opened)["workstream"].clone();
    let wid = w["id"].as_str().unwrap().to_string();
    let wpath = std::path::PathBuf::from(json(&opened)["path"].as_str().unwrap());
    let branch = w["kind"]["branch"].as_str().unwrap().to_string();
    assert!(branch.starts_with("work/cart-total"), "branch: {branch}");
    assert!(
        !wpath.starts_with(&root),
        "a workstream is a checkout under the project's workstreams/, never inside its tree"
    );
    assert!(w["work_item"].is_null(), "opened by hand: no work item");

    // Work in it, and see the change.
    set_identity(&wpath);
    std::fs::write(wpath.join("README.md"), "baseline + cart\n").unwrap();
    let diff = bisa(data_dir, &["--json", "workstream", "diff", &wid]);
    assert_ok(&diff, "workstream diff");
    let diff = json(&diff);
    assert_eq!(diff["clean"], serde_json::json!(false));
    assert!(diff["diff"].as_str().unwrap().contains("README.md"));

    let committed = bisa(
        data_dir,
        &["--json", "workstream", "commit", &wid, "-m", "cart total"],
    );
    assert_ok(&committed, "workstream commit");
    assert!(!json(&committed)["short"].as_str().unwrap().is_empty());

    // A clean tree refuses rather than making an empty commit.
    let again = bisa(
        data_dir,
        &["--json", "workstream", "commit", &wid, "-m", "again"],
    );
    assert!(!again.status.success(), "an empty commit is refused");

    let shown = bisa(data_dir, &["--json", "workstream", "show", &wid]);
    assert_ok(&shown, "workstream show");
    assert_eq!(
        json(&shown)["status"]["ahead_of_base"],
        serde_json::json!(1)
    );

    let listed = bisa(
        data_dir,
        &["--json", "workstream", "list", "--project", &pid],
    );
    assert_ok(&listed, "workstream list");
    // The project's primary first, then the one opened.
    assert_eq!(json(&listed)["workstreams"].as_array().unwrap().len(), 2);

    // Closing keeps the checkout unless asked.
    assert_ok(
        &bisa(data_dir, &["--json", "workstream", "close", &wid]),
        "workstream close",
    );
    assert!(wpath.is_dir(), "a plain close keeps the checkout");
    assert_ok(
        &bisa(data_dir, &["--json", "workstream", "close", &wid, "--tree"]),
        "workstream close --tree",
    );
    assert!(!wpath.exists());

    // Forgetting the project leaves the folder alone.
    let removed = bisa(data_dir, &["--json", "project", "rm", &pid]);
    assert_ok(&removed, "project rm");
    assert_eq!(json(&removed)["removed_tree"], serde_json::json!(false));
    assert!(root.join("README.md").exists(), "the work is still there");
    // "notes" is still there; only the storefront record was forgotten.
    let listed = bisa(data_dir, &["--json", "project", "list"]);
    let slugs: Vec<String> = json(&listed)["projects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["slug"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(slugs, vec!["notes".to_string()]);
}

/// Pushing passes the Publish gate. With no terminal and no `--yes` there is
/// nobody to decide it, so nothing is pushed and the command says so;
/// `--yes` is a person approving on the command line.
#[test]
fn pushing_passes_the_publish_gate() {
    let dir = tempfile::tempdir().unwrap();
    let (goal, pid, root) = git_project(dir.path(), "storefront", "gated");

    // A local bare repository stands in for origin: the whole push path, no
    // network.
    let origin = dir.path().join("origin.git");
    std::fs::create_dir_all(&origin).unwrap();
    raw_git(&origin, &["init", "--bare", "--quiet"]);
    raw_git(
        &root,
        &["remote", "add", "origin", origin.to_str().unwrap()],
    );

    let opened = bisa(
        dir.path(),
        // For the goal: a gated project's publish gate asks the goal's
        // owner, so a workstream of nobody's has nobody to ask.
        &[
            "--json",
            "workstream",
            "open",
            &pid,
            "--goal",
            &goal,
            "--label",
            "checkout",
        ],
    );
    assert_ok(&opened, "workstream open");
    let w = json(&opened)["workstream"].clone();
    let wid = w["id"].as_str().unwrap().to_string();
    let wpath = std::path::PathBuf::from(json(&opened)["path"].as_str().unwrap());
    let branch = w["kind"]["branch"].as_str().unwrap().to_string();
    set_identity(&wpath);
    std::fs::write(wpath.join("checkout.rs"), "fn checkout() {}\n").unwrap();
    assert_ok(
        &bisa(
            dir.path(),
            &["--json", "workstream", "commit", &wid, "-m", "checkout"],
        ),
        "commit",
    );

    // No terminal, no --yes: the gate is reported and nothing goes out.
    let held = bisa(dir.path(), &["--json", "workstream", "push", &wid]);
    assert_ok(&held, "workstream push (undecided)");
    assert_eq!(json(&held)["pushed"], serde_json::json!(false));
    assert!(
        origin_branches(&origin).is_empty(),
        "an undecided publish gate must push nothing"
    );

    // --yes is the human deciding it here rather than in the inbox.
    let pushed = bisa(dir.path(), &["--json", "workstream", "push", &wid, "--yes"]);
    assert_ok(&pushed, "workstream push --yes");
    assert_eq!(json(&pushed)["pushed"], serde_json::json!(true));
    assert_eq!(origin_branches(&origin), vec![branch]);

    // A manual-publishing project refuses outright, gate or no gate.
    let manual = tempfile::tempdir().unwrap();
    let (_i2, pid2, root2) = git_project(manual.path(), "manualonly", "manual");
    let origin2 = manual.path().join("origin2.git");
    std::fs::create_dir_all(&origin2).unwrap();
    raw_git(&origin2, &["init", "--bare", "--quiet"]);
    raw_git(
        &root2,
        &["remote", "add", "origin", origin2.to_str().unwrap()],
    );
    let opened2 = json(&bisa(
        manual.path(),
        &["--json", "workstream", "open", &pid2, "--label", "hand"],
    ));
    let w2 = opened2["workstream"].clone();
    let wid2 = w2["id"].as_str().unwrap().to_string();
    let wpath2 = std::path::PathBuf::from(opened2["path"].as_str().unwrap());
    set_identity(&wpath2);
    std::fs::write(wpath2.join("hand.rs"), "fn hand() {}\n").unwrap();
    assert_ok(
        &bisa(
            manual.path(),
            &["--json", "workstream", "commit", &wid2, "-m", "hand"],
        ),
        "commit",
    );
    let refused = bisa(
        manual.path(),
        &["--json", "workstream", "push", &wid2, "--yes"],
    );
    assert!(!refused.status.success(), "manual publishing refuses");
    assert!(origin_branches(&origin2).is_empty());
}

/// `bisa assign` / `unassign` replace the old `team assign` stopgap:
/// agents, humans and teams all go on a goal through one command.
#[test]
fn assign_and_unassign_carry_an_goal() {
    let dir = tempfile::tempdir().unwrap();
    let me = json(&bisa(dir.path(), &["--json", "init"]))["pubkey"]
        .as_str()
        .unwrap()
        .to_string();
    let agent = json(&bisa(
        dir.path(),
        &[
            "--json",
            "agent",
            "add",
            "--name",
            "Builder",
            "--prompt",
            "You build.",
        ],
    ))["agent"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let team = json(&bisa(
        dir.path(),
        &["--json", "team", "create", "Crew", "--agent", &agent],
    ))["team"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let goal = json(&bisa(
        dir.path(),
        &["--json", "new", "ship the thing", "--mode", "manual"],
    ))["goal"]
        .as_str()
        .unwrap()
        .to_string();

    // The stopgap is gone, with no compatibility shim.
    let gone = bisa(dir.path(), &["team", "assign", &team, &goal]);
    assert!(
        !gone.status.success(),
        "`team assign` was replaced by `bisa assign`"
    );

    let assigned = bisa(
        dir.path(),
        &[
            "--json",
            "assign",
            &goal,
            &format!("agent:{agent}"),
            &format!("team:{team}"),
            &format!("human:{me}"),
        ],
    );
    assert_ok(&assigned, "assign");
    let who = json(&assigned)["assignees"].clone();
    assert_eq!(who.as_array().unwrap().len(), 3, "{who}");

    // Adding is the default: a second assign joins rather than replaces.
    let again = bisa(
        dir.path(),
        &["--json", "assign", &goal, &format!("agent:{agent}")],
    );
    assert_ok(&again, "assign again");
    assert_eq!(json(&again)["assignees"].as_array().unwrap().len(), 3);

    // A bad entry refuses the whole list rather than applying half of it.
    let bad = bisa(dir.path(), &["--json", "assign", &goal, "developer"]);
    assert!(!bad.status.success(), "a bare id is ambiguous and refused");
    assert!(String::from_utf8_lossy(&bad.stderr).contains("agent:<id>"));

    // Named entries come off; no names clears the list.
    let fewer = bisa(
        dir.path(),
        &["--json", "unassign", &goal, &format!("team:{team}")],
    );
    assert_ok(&fewer, "unassign one");
    let who: Vec<String> = json(&fewer)["assignees"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert_eq!(who.len(), 2, "{who:?}");
    assert!(!who.iter().any(|a| a.starts_with("team:")));

    let cleared = bisa(dir.path(), &["--json", "unassign", &goal]);
    assert_ok(&cleared, "unassign all");
    assert!(json(&cleared)["assignees"].as_array().unwrap().is_empty());
    let status = bisa(dir.path(), &["--json", "status", &goal]);
    assert!(json(&status)["goal"]["assignees"]
        .as_array()
        .is_none_or(|a| a.is_empty()));
}

/// Attaching makes a project one more goal's; detaching takes it away and
/// touches nothing else — the same rule the HTTP surface serves.
#[test]
fn projects_attach_to_goals() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());
    let owner = json(&bisa(
        dir.path(),
        &["--json", "new", "owns it", "--mode", "manual"],
    ))["goal"]
        .as_str()
        .unwrap()
        .to_string();
    let other = json(&bisa(
        dir.path(),
        &["--json", "new", "borrows it", "--mode", "manual"],
    ))["goal"]
        .as_str()
        .unwrap()
        .to_string();

    let pid = json(&bisa(
        dir.path(),
        &["--json", "project", "new", "--goal", &owner, "storefront"],
    ))["project"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let seen = |goal: &str| -> usize {
        let out = bisa(dir.path(), &["--json", "project", "list", "--goal", goal]);
        assert_ok(&out, "project list --goal");
        json(&out)["projects"].as_array().unwrap().len()
    };
    assert_eq!(seen(&owner), 1);
    assert_eq!(seen(&other), 0);

    assert_ok(
        &bisa(dir.path(), &["--json", "project", "attach", &pid, &other]),
        "project attach",
    );
    assert_eq!(seen(&other), 1);

    assert_ok(
        &bisa(dir.path(), &["--json", "project", "detach", &pid, &other]),
        "project detach",
    );
    assert_eq!(seen(&other), 0);
    assert_eq!(seen(&owner), 1, "detaching never touches the other goal");

    // A project attached to no goal is an ordinary project: the first-run
    // shape, where a workspace opens on Projects and a goal comes later.
    let standalone = bisa(dir.path(), &["--json", "project", "new", "standalone"]);
    assert_ok(&standalone, "project new without --goal");
    let all = json(&bisa(dir.path(), &["--json", "project", "list"]));
    assert_eq!(all["projects"].as_array().unwrap().len(), 2);
}

/// Adopting points at a folder that already exists, canonicalizes it, and
/// writes nothing into it — and refuses anything that would land inside the
/// workspace.
#[test]
fn adopting_a_folder_is_contained_and_read_only() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());
    let goal = json(&bisa(
        dir.path(),
        &["--json", "new", "adopt something", "--mode", "manual"],
    ))["goal"]
        .as_str()
        .unwrap()
        .to_string();

    let outside = tempfile::tempdir().unwrap();
    let real = outside.path().join("their-repo");
    std::fs::create_dir_all(&real).unwrap();
    raw_git(&real, &["init", "--quiet"]);
    set_identity(&real);
    std::fs::write(real.join("theirs.txt"), "not ours\n").unwrap();
    raw_git(&real, &["add", "-A"]);
    raw_git(&real, &["commit", "-m", "theirs", "--quiet"]);
    let before: Vec<String> = std::fs::read_dir(&real)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();

    // Relative, missing and inside-the-workspace paths are all refused.
    for bad in [
        "relative/path".to_string(),
        "/definitely/not/here".to_string(),
        // The workspace root itself: a project rooted at workspace truth.
        dir.path().to_string_lossy().into_owned(),
    ] {
        let out = bisa(
            dir.path(),
            &["--json", "project", "adopt", "--goal", &goal, &bad],
        );
        assert!(!out.status.success(), "adopting {bad:?} should be refused");
    }

    let adopted = bisa(
        dir.path(),
        &[
            "--json",
            "project",
            "adopt",
            "--goal",
            &goal,
            real.to_str().unwrap(),
            "--slug",
            "theirs",
        ],
    );
    assert_ok(&adopted, "project adopt");
    let adopted = json(&adopted);
    assert_eq!(
        std::path::PathBuf::from(adopted["path"].as_str().unwrap()),
        real.canonicalize().unwrap()
    );
    assert_eq!(
        adopted["project"]["root"]["type"],
        serde_json::json!("external")
    );
    assert_eq!(
        adopted["project"]["vcs"]["type"],
        serde_json::json!("git"),
        "an existing repository is recognised, never re-initialised"
    );

    let after: Vec<String> = std::fs::read_dir(&real)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(before, after, "adopting writes nothing into the folder");

    // ...and Bisa will not delete somebody else's folder.
    let pid = adopted["project"]["id"].as_str().unwrap().to_string();
    let refused = bisa(dir.path(), &["--json", "project", "rm", &pid, "--tree"]);
    assert!(
        !refused.status.success(),
        "an adopted folder is never deleted"
    );
    assert!(real.join("theirs.txt").exists());
}

/// `--listen` says "loopback" and must mean it.
///
/// Every route but `POST /hooks/{id}` is unauthenticated, so a non-loopback
/// bind does not weaken the node a little — it removes its only protection.
/// The refusal has to name the risk and the way past it, or an operator in a
/// hurry just reaches for the flag without reading.
#[test]
fn listening_off_loopback_is_refused_unless_asked_for_explicitly() {
    let dir = tempfile::tempdir().unwrap();
    let out = bisa(dir.path(), &["node", "--listen", "0.0.0.0:47913"]);
    assert!(!out.status.success(), "a wildcard bind must be refused");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("not a loopback address"), "{err}");
    assert!(err.contains("--insecure-allow-remote"), "{err}");

    // The escape hatch exists and is named for what it does. Binding for real
    // would block on the daemon, so this only asserts the guard lets it past:
    // --help renders the flag, which is what an operator needs to find it.
    let help = bisa(dir.path(), &["node", "--help"]);
    let text = String::from_utf8_lossy(&help.stdout);
    assert!(text.contains("insecure-allow-remote"), "{text}");
}

/// Governance has four gates, so the CLI must be able to reach all four.
///
/// `publish` is the one that matters most here: it is the only gate that never
/// widens to a goal's assignees, so an explicit policy set through this
/// command is the *only* way to let somebody other than the owner push.
/// A CLI that hid it would make that impossible without editing JSON by hand.
#[test]
fn governance_covers_every_gate_including_publish() {
    let dir = tempfile::tempdir().unwrap();

    let shown =
        String::from_utf8_lossy(&bisa(dir.path(), &["governance", "show"]).stdout).to_string();
    for gate in ["approval", "escalation", "publish"] {
        assert!(shown.contains(gate), "{gate} missing from show: {shown}");
    }
    assert!(
        !shown.contains("commit") && !shown.contains("acceptance"),
        "the lifecycle's gates are gone: {shown}"
    );

    let set = bisa(dir.path(), &["governance", "set", "publish", "members"]);
    assert!(
        set.status.success(),
        "{}",
        String::from_utf8_lossy(&set.stderr)
    );
    let after =
        String::from_utf8_lossy(&bisa(dir.path(), &["governance", "show"]).stdout).to_string();
    assert!(
        after.contains("publish:    members"),
        "the change must stick: {after}"
    );

    let bad = bisa(dir.path(), &["governance", "set", "nope", "owner"]);
    assert!(!bad.status.success());
    let err = String::from_utf8_lossy(&bad.stderr);
    assert!(err.contains("publish"), "the error lists every gate: {err}");
}

// ---------------------------------------------------------------------------
// M8: the skill library, the MCP registry, and tags as the one filing system
// ---------------------------------------------------------------------------

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).to_string()
}

fn ids(v: &serde_json::Value, key: &str) -> Vec<String> {
    v[key]
        .as_array()
        .unwrap_or_else(|| panic!("{key} should be an array: {v}"))
        .iter()
        .map(|x| x["id"].as_str().unwrap().to_string())
        .collect()
}

/// The library is the whole life cycle of a skill: written once, listed, read,
/// attached to an agent by id, detached, and removed.
#[test]
fn skill_library_roundtrip_and_agent_attachment() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());
    let body = dir.path().join("triage.md");
    std::fs::write(&body, "# Triage\nRead the alert before touching anything.").unwrap();

    let added = bisa(
        dir.path(),
        &[
            "--json",
            "skill",
            "add",
            "--id",
            "alert-triage",
            "--name",
            "Alert Triage",
            "--description",
            "Use when a pager alert arrives.",
            "--file",
            body.to_str().unwrap(),
            "--tag",
            "ops",
        ],
    );
    assert_ok(&added, "skill add");
    let s = json(&added)["skill"].clone();
    assert_eq!(s["id"], "alert-triage");
    assert_eq!(s["tags"], serde_json::json!(["ops"]));
    assert!(
        s["markdown"].as_str().unwrap().contains("Read the alert"),
        "--file is the body, not a path: {s}"
    );

    let listed = bisa(dir.path(), &["--json", "skill", "list", "--tag", "ops"]);
    assert_ok(&listed, "skill list --tag ops");
    assert!(ids(&json(&listed), "skills").contains(&"alert-triage".to_string()));

    let shown = bisa(dir.path(), &["skill", "show", "alert-triage"]);
    assert_ok(&shown, "skill show");
    assert!(
        stdout(&shown).contains("Read the alert"),
        "show prints the procedure: {}",
        stdout(&shown)
    );

    // Attach by id: the agent gains a reference, never a copy.
    let agent = json(&bisa(
        dir.path(),
        &[
            "--json",
            "agent",
            "add",
            "--name",
            "Responder",
            "--prompt",
            "You respond.",
        ],
    ))["agent"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let attached = bisa(
        dir.path(),
        &["--json", "agent", "skill", "add", &agent, "alert-triage"],
    );
    assert_ok(&attached, "agent skill add");
    assert_eq!(
        json(&attached)["agent"]["skills"],
        serde_json::json!(["alert-triage"])
    );

    // Attaching twice is the same as attaching once.
    let again = bisa(
        dir.path(),
        &["--json", "agent", "skill", "add", &agent, "alert-triage"],
    );
    assert_ok(&again, "agent skill add twice");
    assert_eq!(
        json(&again)["agent"]["skills"].as_array().unwrap().len(),
        1,
        "attaching is idempotent"
    );

    let detached = bisa(
        dir.path(),
        &["--json", "agent", "skill", "rm", &agent, "alert-triage"],
    );
    assert_ok(&detached, "agent skill rm");
    assert!(json(&detached)["agent"]["skills"]
        .as_array()
        .unwrap()
        .is_empty());

    assert_ok(
        &bisa(dir.path(), &["skill", "rm", "alert-triage"]),
        "skill rm",
    );
    let after = bisa(dir.path(), &["--json", "skill", "list"]);
    assert!(!ids(&json(&after), "skills").contains(&"alert-triage".to_string()));
}

/// Both transports register, a server can be switched off without being
/// forgotten, and the one name the engine reserves is refused with a message
/// saying why.
#[test]
fn mcp_registry_covers_both_transports_and_the_reserved_name() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());

    let stdio = bisa(
        dir.path(),
        &[
            "--json",
            "mcp",
            "add",
            "--id",
            "files",
            "--name",
            "filesystem",
            "--command",
            "npx",
            "--arg",
            "-y",
            "--arg",
            "@example/fs",
            "--env",
            "ROOT=/tmp",
            "--tag",
            "ops",
        ],
    );
    assert_ok(&stdio, "mcp add stdio");
    let m = json(&stdio)["mcp"].clone();
    assert_eq!(m["transport"]["transport"], "stdio");
    assert_eq!(
        m["transport"]["args"],
        serde_json::json!(["-y", "@example/fs"]),
        "a leading hyphen is an argument, not a flag: {m}"
    );
    // Every answer masks a value that may be a secret (`McpServerConfig::masked`); the key stays.
    assert_eq!(m["transport"]["env"]["ROOT"], "••••••");

    let http = bisa(
        dir.path(),
        &[
            "--json",
            "mcp",
            "add",
            "--id",
            "weather",
            "--name",
            "weather",
            "--url",
            "https://example.com/mcp",
            "--tag",
            "data",
        ],
    );
    assert_ok(&http, "mcp add http");
    assert_eq!(json(&http)["mcp"]["transport"]["transport"], "http");

    // The older HTTP+SSE transport, with a header — which never prints back.
    let sse = bisa(
        dir.path(),
        &[
            "--json",
            "mcp",
            "add",
            "--id",
            "legacy",
            "--name",
            "legacy",
            "--url",
            "https://example.com/sse",
            "--sse",
            "--header",
            "X-Api-Key=k-secret-1",
        ],
    );
    assert_ok(&sse, "mcp add sse");
    let m = json(&sse)["mcp"].clone();
    assert_eq!(m["transport"]["transport"], "sse");
    assert_eq!(
        m["transport"]["headers"]["X-Api-Key"], "••••••",
        "a secret is written once and never printed: {m}"
    );
    assert!(!String::from_utf8_lossy(&sse.stdout).contains("k-secret-1"));
    let shown = bisa(dir.path(), &["mcp", "show", "legacy"]);
    assert_ok(&shown, "mcp show sse");
    let text = String::from_utf8_lossy(&shown.stdout);
    assert!(
        text.contains("sse https://example.com/sse") && text.contains("1 headers"),
        "{text}"
    );
    assert!(!text.contains("k-secret-1"));
    assert_eq!(
        json(&bisa(dir.path(), &["--json", "mcp", "show", "files"]))["mcp"]["transport"]["env"]
            ["ROOT"],
        "••••••"
    );

    // A URL that is not one, and a working directory that is relative, are refused in words.
    let bad = bisa(
        dir.path(),
        &[
            "mcp",
            "add",
            "--id",
            "bad",
            "--name",
            "bad",
            "--url",
            "example.com/mcp",
        ],
    );
    assert!(!bad.status.success());
    assert!(stderr(&bad).contains("http://"), "{}", stderr(&bad));
    let rel = bisa(
        dir.path(),
        &[
            "mcp",
            "add",
            "--id",
            "rel",
            "--name",
            "rel",
            "--command",
            "x",
            "--cwd",
            "./here",
        ],
    );
    assert!(!rel.status.success());
    assert!(stderr(&rel).contains("absolute"), "{}", stderr(&rel));

    let listed = bisa(dir.path(), &["--json", "mcp", "list"]);
    assert_ok(&listed, "mcp list");
    let all = ids(&json(&listed), "mcps");
    assert!(all.contains(&"files".to_string()) && all.contains(&"weather".to_string()));

    let filtered = bisa(dir.path(), &["--json", "mcp", "list", "--tag", "data"]);
    assert_ok(&filtered, "mcp list --tag data");
    assert_eq!(ids(&json(&filtered), "mcps"), vec!["weather".to_string()]);

    // Disabling keeps the record; sessions simply skip it.
    let off = bisa(dir.path(), &["--json", "mcp", "disable", "files"]);
    assert_ok(&off, "mcp disable");
    assert_eq!(json(&off)["mcp"]["enabled"], false);
    assert_ok(&bisa(dir.path(), &["mcp", "show", "files"]), "mcp show");

    // `bisa` is the engine's own server; a second one under that name
    // would shadow the tools the platform runs on.
    let reserved = bisa(
        dir.path(),
        &[
            "mcp",
            "add",
            "--id",
            "shadow",
            "--name",
            "bisa",
            "--command",
            "false",
        ],
    );
    assert!(
        !reserved.status.success(),
        "the reserved name must be refused"
    );
    let err = stderr(&reserved);
    assert!(
        err.contains("bisa") && err.contains("reserved"),
        "the refusal must say which name and why: {err}"
    );

    assert_ok(&bisa(dir.path(), &["mcp", "rm", "files"]), "mcp rm");
    let after = bisa(dir.path(), &["--json", "mcp", "list"]);
    assert!(!ids(&json(&after), "mcps").contains(&"files".to_string()));
}

/// `bisa mcp probe` dials and reports; a server that would not start is a
/// report that says so — the command succeeds, the report does not — and a
/// registered server is probed by id.
#[test]
fn mcp_probe_reports_what_answered_and_where_it_stopped() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());
    let ghost = bisa(
        dir.path(),
        &[
            "--json",
            "mcp",
            "probe",
            "--command",
            "/definitely/not/a/binary",
            "--timeout-secs",
            "3",
        ],
    );
    assert_ok(
        &ghost,
        "a probe of a server that will not start is still an answer",
    );
    let report = json(&ghost)["report"].clone();
    assert_eq!(report["ok"], false);
    assert_eq!(report["stage"], "spawn", "{report}");
    assert_eq!(report["transport"], "stdio");

    let added = bisa(
        dir.path(),
        &[
            "--json",
            "mcp",
            "add",
            "--id",
            "ghost",
            "--name",
            "ghost",
            "--command",
            "/definitely/not/a/binary",
            "--env",
            "TOKEN=sk-1",
        ],
    );
    assert_ok(&added, "mcp add");
    let by_id = bisa(
        dir.path(),
        &["--json", "mcp", "probe", "ghost", "--timeout-secs", "3"],
    );
    assert_ok(&by_id, "mcp probe <id>");
    let report = json(&by_id)["report"].clone();
    assert_eq!(report["ok"], false);
    assert!(
        !by_id.stdout.windows(4).any(|w| w == b"sk-1"),
        "no secret in a report"
    );

    let both = bisa(
        dir.path(),
        &["mcp", "probe", "ghost", "--url", "https://x.test/mcp"],
    );
    assert!(!both.status.success(), "one thing at a time");
    assert_ok(&bisa(dir.path(), &["mcp", "disable", "ghost"]), "disable");
    let off = bisa(dir.path(), &["mcp", "probe", "ghost"]);
    assert!(!off.status.success());
    assert!(stderr(&off).contains("disabled"), "{}", stderr(&off));
}

/// `--tag` narrows, `--match all` narrows further, and a tag that will not
/// normalize is an error naming itself rather than an empty list that looks
/// like an honest answer.
#[test]
fn tag_filters_narrow_and_a_bad_tag_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());
    // A fresh workspace has one untagged agent, so there is nothing to filter
    // until something is installed. The Engineering team brings five tagged
    // agents and the skills they carry.
    assert_ok(
        &bisa(dir.path(), &["catalog", "install", "team", "engineering"]),
        "catalog install team engineering",
    );

    let all = ids(
        &json(&bisa(dir.path(), &["--json", "agent", "list"])),
        "agents",
    );
    let engineering = ids(
        &json(&bisa(
            dir.path(),
            &["--json", "agent", "list", "--tag", "engineering"],
        )),
        "agents",
    );
    assert!(!engineering.is_empty(), "the catalog tags its engineers");
    assert!(
        engineering.len() < all.len(),
        "a filter must actually filter"
    );
    assert!(
        engineering.iter().all(|id| all.contains(id)),
        "a filtered list is a subset of the whole: {engineering:?} vs {all:?}"
    );

    let both = ids(
        &json(&bisa(
            dir.path(),
            &[
                "--json",
                "agent",
                "list",
                "--tag",
                "engineering",
                "--tag",
                "code",
                "--match",
                "all",
            ],
        )),
        "agents",
    );
    assert!(!both.is_empty(), "some catalog agent carries both tags");
    assert!(
        both.len() < engineering.len(),
        "--match all narrows: {both:?} vs {engineering:?}"
    );
    assert!(both.iter().all(|id| engineering.contains(id)));

    // `any` is the default, and it is the wider answer.
    let either = ids(
        &json(&bisa(
            dir.path(),
            &[
                "--json",
                "agent",
                "list",
                "--tag",
                "engineering",
                "--tag",
                "code",
            ],
        )),
        "agents",
    );
    assert!(either.len() >= engineering.len());

    let bad = bisa(dir.path(), &["agent", "list", "--tag", "Not A Tag!"]);
    assert!(!bad.status.success(), "a malformed tag must not be ignored");
    assert!(
        stderr(&bad).contains("Not A Tag!"),
        "the error names the tag: {}",
        stderr(&bad)
    );

    // Setting one is refused the same way, at the same door.
    let bad_set = bisa(
        dir.path(),
        &[
            "agent",
            "add",
            "--name",
            "X",
            "--prompt",
            "y",
            "--tag",
            "Not A Tag!",
        ],
    );
    assert!(!bad_set.status.success());
    assert!(stderr(&bad_set).contains("Not A Tag!"));
}

/// The facet and the filtered list are the same index rows, so they must
/// agree: the count `bisa tags` prints is the length of the list `--tag`
/// returns.
#[test]
fn tags_facet_matches_the_filtered_lists() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());
    // Nothing carries a tag until something is installed. These two entries
    // between them cover all four tagged kinds: agents, skills, a team and a
    // channel.
    for (kind, slug) in [("team", "engineering"), ("channel", "product")] {
        assert_ok(
            &bisa(dir.path(), &["catalog", "install", kind, slug]),
            "catalog install",
        );
    }

    let facet = bisa(dir.path(), &["--json", "tags"]);
    assert_ok(&facet, "tags");
    let v = json(&facet);
    assert!(
        v["vocabulary"]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t == "engineering"),
        "the documented vocabulary is part of the answer: {v}"
    );

    let rows = v["tags"].as_array().unwrap().clone();
    assert!(
        !rows.is_empty(),
        "what the catalog installed is tagged: {v}"
    );

    let mut checked = 0;
    for row in &rows {
        let (entity, tag, count) = (
            row["entity"].as_str().unwrap(),
            row["tag"].as_str().unwrap(),
            row["count"].as_u64().unwrap(),
        );
        let (noun, key) = match entity {
            "agent" => (vec!["agent", "list"], "agents"),
            "skill" => (vec!["skill", "list"], "skills"),
            "team" => (vec!["team", "list"], "teams"),
            "channel" => (vec!["channels", "list"], "channels"),
            _ => continue,
        };
        let mut args = vec!["--json"];
        args.extend(noun);
        args.extend(["--tag", tag]);
        let listed = bisa(dir.path(), &args);
        assert_ok(&listed, "filtered list");
        assert_eq!(
            json(&listed)[key].as_array().unwrap().len() as u64,
            count,
            "`tags` says {count} {entity}(s) carry {tag:?}; the list disagrees"
        );
        checked += 1;
    }
    assert!(checked >= 4, "the facet should cover several kinds");

    // A tag nobody carries is absent from the facet and empty in the list.
    let none = bisa(
        dir.path(),
        &["--json", "agent", "list", "--tag", "nonesuch"],
    );
    assert_ok(&none, "agent list --tag nonesuch");
    assert!(json(&none)["agents"].as_array().unwrap().is_empty());
}

/// A roster is a directory: it is printed with the channel, and it is what the
/// channel's own handle expands to when a message names it.
#[test]
fn channel_roster_is_shown_and_expands_as_a_mention() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());

    let first = json(&bisa(
        dir.path(),
        &["--json", "agent", "add", "--name", "One", "--prompt", "a"],
    ))["agent"]
        .clone();
    let second = json(&bisa(
        dir.path(),
        &["--json", "agent", "add", "--name", "Two", "--prompt", "b"],
    ))["agent"]
        .clone();
    let (a1, a2) = (
        first["id"].as_str().unwrap().to_string(),
        second["id"].as_str().unwrap().to_string(),
    );

    let created = bisa(
        dir.path(),
        &[
            "--json", "channels", "create", "triage", "--agent", &a1, "--agent", &a2, "--tag",
            "ops",
        ],
    );
    assert_ok(&created, "channels create with a roster");
    let channel = json(&created)["channel"].clone();
    let cid = channel["id"].as_str().unwrap().to_string();
    assert_eq!(channel["roster"]["policy"], "listed");
    assert_eq!(channel["roster"]["agents"], serde_json::json!([a1, a2]));
    assert_eq!(channel["tags"], serde_json::json!(["ops"]));

    // The human rendering shows it — a directory nobody can see is unusable.
    let listed = bisa(dir.path(), &["channels", "list", "--tag", "ops"]);
    assert_ok(&listed, "channels list --tag ops");
    let text = stdout(&listed);
    assert!(
        text.contains(&a1) && text.contains(&a2),
        "the roster must be printed: {text}"
    );

    // The channel handle expands to the roster's pubkeys.
    let posted = bisa(
        dir.path(),
        &["--json", "msg", &cid, "who is on this?", "--mention", &cid],
    );
    assert_ok(&posted, "msg --mention <channel-id>");
    let mentioned: Vec<String> = json(&posted)["mentions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m.as_str().unwrap().to_string())
        .collect();
    // The roster it stores, plus the General Agent and the Workflow Agent, which are in every
    // room and stored in none of them (06 — Agents and teams). Addressing
    // the handle reaches everyone in the room.
    let mut expected = vec![
        first["pubkey"].as_str().unwrap().to_string(),
        second["pubkey"].as_str().unwrap().to_string(),
    ];
    for core in ["general-agent", "workflow-agent"] {
        let shown = bisa(dir.path(), &["--json", "agent", "show", core]);
        assert_ok(&shown, "agent show");
        expected.push(
            json(&shown)["agent"]["pubkey"]
                .as_str()
                .unwrap()
                .to_string(),
        );
    }
    expected.sort();
    let mut got = mentioned.clone();
    got.sort();
    assert_eq!(got, expected, "the handle is the roster, in full");

    // Editing replaces the roster it names and leaves the topic alone.
    let edited = bisa(
        dir.path(),
        &["--json", "channels", "edit", &cid, "--agent", &a2],
    );
    assert_ok(&edited, "channels edit");
    assert_eq!(
        json(&edited)["channel"]["roster"]["agents"],
        serde_json::json!([a2])
    );

    // An unknown token is an error, not a message quietly sent to nobody.
    let bad = bisa(dir.path(), &["msg", &cid, "hi", "--mention", "who-dis"]);
    assert!(!bad.status.success());
    assert!(stderr(&bad).contains("who-dis"), "{}", stderr(&bad));
}

/// `search` is the goal list: a query, a tag filter, or neither.
#[test]
fn goals_list_by_tag_with_or_without_a_query() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());

    let tagged = json(&bisa(
        dir.path(),
        &[
            "--json",
            "new",
            "Rotate the signing keys",
            "--mode",
            "manual",
            "--tag",
            "security",
        ],
    ))["goal"]
        .as_str()
        .unwrap()
        .to_string();
    assert_ok(
        &bisa(
            dir.path(),
            &["--json", "new", "Redesign the logo", "--mode", "manual"],
        ),
        "new untagged",
    );

    // No query lists everything.
    let all = bisa(dir.path(), &["--json", "search"]);
    assert_ok(&all, "search with no query");
    assert_eq!(json(&all)["matches"].as_array().unwrap().len(), 2);

    // --tag narrows it to the one that carries the tag.
    let filtered = bisa(dir.path(), &["--json", "search", "--tag", "security"]);
    assert_ok(&filtered, "search --tag security");
    let rows = json(&filtered)["matches"].as_array().unwrap().clone();
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0]["goal"], tagged.as_str());
    assert_eq!(rows[0]["tags"], serde_json::json!(["security"]));

    // The facet counts it too, so the two surfaces agree.
    let facet = json(&bisa(dir.path(), &["--json", "tags", "--entity", "goal"]));
    let rows = facet["tags"].as_array().unwrap();
    assert!(
        rows.iter()
            .any(|r| r["tag"] == "security" && r["count"] == 1),
        "the goal facet must see it: {facet}"
    );

    // A query still filters by text, and --tag composes with it.
    let both = bisa(
        dir.path(),
        &["--json", "search", "signing", "--tag", "security"],
    );
    assert_ok(&both, "search query + tag");
    assert_eq!(json(&both)["matches"].as_array().unwrap().len(), 1);

    let neither = bisa(
        dir.path(),
        &["--json", "search", "logo", "--tag", "security"],
    );
    assert_ok(&neither, "search query + non-matching tag");
    assert!(json(&neither)["matches"].as_array().unwrap().is_empty());
}

// ---------------------------------------------------------------------------
// M9: one agent, and a catalog everything else is installed from
// ---------------------------------------------------------------------------

/// The milestone in one assertion: a workspace opens with the platform's own
/// agent and nothing else. M8 seeded the whole catalog into every first run;
/// none of that is here until somebody installs it.
#[test]
fn a_fresh_workspace_has_the_two_core_agents_and_nothing_else() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());

    let agents = bisa(dir.path(), &["--json", "agent", "list"]);
    assert_ok(&agents, "agent list");
    let mut listed = ids(&json(&agents), "agents");
    listed.sort();
    let mut core: Vec<String> = bisa_core::AgentId::CORE
        .iter()
        .map(|s| s.to_string())
        .collect();
    core.sort();
    assert_eq!(
        listed,
        core,
        "a fresh workspace has exactly the General Agent and the Workflow Agent: {}",
        stdout(&agents)
    );

    // The one channel that is there is `general`, the workspace's own room.
    let channels = json(&bisa(dir.path(), &["--json", "channels", "list"]));
    let ids: Vec<&str> = channels["channels"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|c| c["id"].as_str())
        .collect();
    assert_eq!(
        ids,
        vec!["general"],
        "a fresh workspace has the general channel alone: {channels}"
    );
    for (noun, key) in [
        (["team", "list"], "teams"),
        (["skill", "list"], "skills"),
        (["mcp", "list"], "mcps"),
        (["workflow", "list"], "workflows"),
    ] {
        let listed = bisa(dir.path(), &["--json", noun[0], noun[1]]);
        assert_ok(&listed, "list");
        let v = json(&listed);
        assert!(
            v[key].as_array().unwrap().is_empty(),
            "{key} should be empty on a fresh workspace: {v}"
        );
    }

    // The one agent that is there says what it is, because its provenance is
    // the only one that changes what an owner may do with it.
    let shown = bisa(dir.path(), &["agent", "show", "general-agent"]);
    assert_ok(&shown, "agent show");
    let text = stdout(&shown);
    assert!(
        text.contains("platform's own agent") && text.contains("cannot be removed"),
        "the core agent's listing must say what it is: {text}"
    );
}

/// The two writes the core agent refuses, and the one it accepts. Each refusal
/// names what it refused — finding out from a generic failure which of a dozen
/// fields was rejected is not something an owner can act on.
#[test]
fn the_core_agent_refuses_removal_and_disabling_but_takes_a_new_harness() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());

    let removed = bisa(dir.path(), &["agent", "rm", "general-agent"]);
    assert!(
        !removed.status.success(),
        "the one agent every workspace can reach is not removable"
    );
    assert!(
        stderr(&removed).contains("general-agent"),
        "the refusal names what it refused: {}",
        stderr(&removed)
    );

    let disabled = bisa(
        dir.path(),
        &["agent", "edit", "general-agent", "--enabled", "false"],
    );
    assert!(
        !disabled.status.success(),
        "a workspace whose always-present agent is switched off has nothing to \
         route a question to"
    );
    assert!(
        stderr(&disabled).contains("general-agent"),
        "the refusal names what it refused: {}",
        stderr(&disabled)
    );

    // Its harness and its model plan are the two things an owner may change.
    let swapped = bisa(
        dir.path(),
        &[
            "--json",
            "agent",
            "edit",
            "general-agent",
            "--harness",
            "codex",
        ],
    );
    assert_ok(&swapped, "agent edit --harness");
    assert_eq!(json(&swapped)["agent"]["harness"], "codex");

    // And the change survives the next open: the core agent is ensured, not
    // re-created, so a re-open is a no-op on one that already exists.
    let again = bisa(dir.path(), &["--json", "agent", "list"]);
    assert_ok(&again, "agent list");
    let a = json(&again)["agents"][0].clone();
    assert_eq!(
        a["harness"], "codex",
        "a re-open must not overwrite it: {a}"
    );
    assert_eq!(a["enabled"], true);
}

/// The catalog is what a workspace can install and installs nothing until it
/// is asked: every entry the build ships, browsable by kind and by tag, none
/// of them present.
///
/// The counts are read from `CATALOG` rather than written down. A literal
/// here was wrong within one milestone of being written — M12 added two
/// agents and two channels, and the number was the last thing anybody thought
/// of. A test that restates a fact it could ask for is a second copy of that
/// fact, and the copy is what goes stale.
#[test]
fn catalog_lists_every_entry_and_narrows_by_kind_and_tag() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());

    let all = bisa(dir.path(), &["--json", "catalog", "list"]);
    assert_ok(&all, "catalog list");
    let entries = json(&all)["entries"].as_array().unwrap().clone();
    assert_eq!(
        entries.len(),
        CATALOG.entry_count(),
        "the CLI lists every entry the build ships, of every kind"
    );
    assert!(
        entries.iter().all(|e| e["installed"] == false),
        "nothing in the catalog is present on a fresh workspace"
    );

    let agents = bisa(
        dir.path(),
        &["--json", "catalog", "list", "--kind", "agent"],
    );
    assert_ok(&agents, "catalog list --kind agent");
    let rows = json(&agents)["entries"].as_array().unwrap().clone();
    assert_eq!(rows.len(), CATALOG.agents.len());
    assert!(rows.iter().all(|e| e["kind"] == "agent"), "{rows:?}");

    // The filter runs on the entry's own tags: a catalog entry is compiled
    // into the binary and has no index row until it is installed.
    let tagged = bisa(
        dir.path(),
        &[
            "--json",
            "catalog",
            "list",
            "--kind",
            "agent",
            "--tag",
            "engineering",
        ],
    );
    assert_ok(&tagged, "catalog list --tag engineering");
    let narrowed = json(&tagged)["entries"].as_array().unwrap().clone();
    assert!(
        !narrowed.is_empty() && narrowed.len() < rows.len(),
        "a filter must actually filter: {} of {}",
        narrowed.len(),
        rows.len()
    );
    assert!(
        narrowed.iter().all(|e| e["tags"]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t == "engineering")),
        "{narrowed:?}"
    );

    let both = bisa(
        dir.path(),
        &[
            "--json",
            "catalog",
            "list",
            "--kind",
            "agent",
            "--tag",
            "engineering",
            "--tag",
            "quality",
            "--match",
            "all",
        ],
    );
    assert_ok(&both, "catalog list --match all");
    let strict = json(&both)["entries"].as_array().unwrap().clone();
    assert!(
        !strict.is_empty() && strict.len() < narrowed.len(),
        "--match all narrows further: {} of {}",
        strict.len(),
        narrowed.len()
    );

    let bad = bisa(dir.path(), &["catalog", "list", "--kind", "person"]);
    assert!(!bad.status.success(), "an unknown kind is an error");
    let e = stderr(&bad);
    for kind in ["agent", "skill", "team", "channel"] {
        assert!(e.contains(kind), "the error names the valid kinds: {e}");
    }
}

/// An install is transitive and reported: a team brings its agents, an agent
/// brings its skills, and everything created is named — an install that
/// quietly creates thirteen things is one you cannot undo confidently.
#[test]
fn installing_a_team_creates_its_agents_and_their_skills() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());

    // `show` says what the choice costs before it is made.
    let shown = bisa(dir.path(), &["catalog", "show", "team", "engineering"]);
    assert_ok(&shown, "catalog show");
    let text = stdout(&shown);
    assert!(
        text.contains("developer") && text.contains("code-review-checklist"),
        "show names the agents and the skills the install would create: {text}"
    );

    let installed = bisa(
        dir.path(),
        &["--json", "catalog", "install", "team", "engineering"],
    );
    assert_ok(&installed, "catalog install");
    let made = json(&installed)["installed"].clone();
    assert_eq!(made["teams"], serde_json::json!(["engineering"]));
    let created: Vec<String> = made["agents"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a.as_str().unwrap().to_string())
        .collect();
    assert!(
        created.contains(&"developer".to_string()),
        "a team brings its agents: {made}"
    );
    assert!(
        !made["skills"].as_array().unwrap().is_empty(),
        "an agent brings its skills: {made}"
    );

    // Everything it named is really there, under its bare slug — there is no
    // `builtin-` prefix any more.
    let agents = ids(
        &json(&bisa(dir.path(), &["--json", "agent", "list"])),
        "agents",
    );
    assert!(agents.contains(&"developer".to_string()), "{agents:?}");
    assert!(
        agents.contains(&"general-agent".to_string()),
        "the core agent is still there: {agents:?}"
    );
    let skills = ids(
        &json(&bisa(dir.path(), &["--json", "skill", "list"])),
        "skills",
    );
    assert!(
        skills.contains(&"code-review-checklist".to_string()),
        "{skills:?}"
    );

    // The human rendering groups what it created by kind, so the list of what
    // to walk back is the list it just printed.
    let prose = bisa(dir.path(), &["catalog", "install", "team", "review-board"]);
    assert_ok(&prose, "catalog install review-board");
    let text = stdout(&prose);
    assert!(
        text.contains("agents") && text.contains("teams"),
        "the install names what it created, per kind: {text}"
    );

    // A second install is not an error: it creates nothing and says so.
    let again = bisa(dir.path(), &["catalog", "install", "team", "engineering"]);
    assert_ok(&again, "second install");
    assert!(
        stdout(&again).contains("nothing changed"),
        "installing twice is honest about creating nothing: {}",
        stdout(&again)
    );
    let again_json = bisa(
        dir.path(),
        &["--json", "catalog", "install", "team", "engineering"],
    );
    assert_ok(&again_json, "second install --json");
    let made = json(&again_json)["installed"].clone();
    for kind in ["agents", "skills", "teams", "channels"] {
        assert!(
            made[kind].as_array().unwrap().is_empty(),
            "a second install creates nothing: {made}"
        );
    }

    // And the listing now says the id is taken.
    let listed = json(&bisa(
        dir.path(),
        &["--json", "catalog", "list", "--kind", "team"],
    ));
    let row = listed["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["slug"] == "engineering")
        .cloned()
        .unwrap_or_else(|| panic!("the team should be listed: {listed}"));
    assert_eq!(row["installed"], true, "{row}");
}

/// An id the catalog wants but somebody else holds is a collision, refused by
/// name over the whole plan before anything is written — not an overwrite of
/// work the owner chose.
#[test]
fn a_collision_refuses_the_whole_install_and_names_the_id() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());

    // A skill of the owner's own, under an id one of Engineering's agents
    // wants — three levels down the install's plan.
    assert_ok(
        &bisa(
            dir.path(),
            &[
                "skill",
                "add",
                "--id",
                "code-review-checklist",
                "--name",
                "My checklist",
                "--description",
                "Mine, not the catalog's.",
                "--markdown",
                "Read the diff twice.",
            ],
        ),
        "skill add",
    );

    let refused = bisa(dir.path(), &["catalog", "install", "team", "engineering"]);
    assert!(!refused.status.success(), "a collision is not an overwrite");
    let e = stderr(&refused);
    assert!(
        e.contains("code-review-checklist"),
        "the refusal names the id that is taken: {e}"
    );

    // Nothing was written: the check runs over the whole plan first, so a
    // collision three entries deep refuses before the first file is created.
    let agents = ids(
        &json(&bisa(dir.path(), &["--json", "agent", "list"])),
        "agents",
    );
    assert_eq!(
        agents,
        vec!["general-agent".to_string(), "workflow-agent".to_string()],
        "a refused install creates nothing: {agents:?}"
    );
}

/// `team rm` is the other half of `catalog install team`: installing one
/// creates several agents, and an install with no way back is a one-way door.
#[test]
fn team_rm_reverses_an_installed_team_and_leaves_its_agents() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());
    assert_ok(
        &bisa(dir.path(), &["catalog", "install", "team", "engineering"]),
        "catalog install",
    );
    assert!(ids(
        &json(&bisa(dir.path(), &["--json", "team", "list"])),
        "teams"
    )
    .contains(&"engineering".to_string()));

    let removed = bisa(dir.path(), &["--json", "team", "rm", "engineering"]);
    assert_ok(&removed, "team rm");
    assert_eq!(json(&removed)["removed"], "engineering");
    assert!(!ids(
        &json(&bisa(dir.path(), &["--json", "team", "list"])),
        "teams"
    )
    .contains(&"engineering".to_string()));

    // The agents it brought stay: a team groups agents, it does not own them,
    // and they may already be carrying work.
    assert!(ids(
        &json(&bisa(dir.path(), &["--json", "agent", "list"])),
        "agents"
    )
    .contains(&"developer".to_string()));

    // With the id free again, the catalog offers the team back.
    let listed = json(&bisa(
        dir.path(),
        &["--json", "catalog", "list", "--kind", "team"],
    ));
    let row = listed["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["slug"] == "engineering")
        .cloned()
        .unwrap_or_else(|| panic!("the team should be listed: {listed}"));
    assert_eq!(row["installed"], false, "{row}");

    // Removing what is not there is an error naming it, not a silent success.
    let twice = bisa(dir.path(), &["team", "rm", "engineering"]);
    assert!(!twice.status.success(), "the team is gone for real");
    assert!(
        stderr(&twice).contains("engineering"),
        "the error names the team: {}",
        stderr(&twice)
    );
}

/// Nothing is deleted while something points at it, and the refusal names
/// what — at the surface where a person actually types the delete.
///
/// `usage` exists so that answer is available *before* the delete: a refusal
/// names the first few holders, and the full list is what tells you how much
/// work detaching is.
#[test]
fn usage_lists_the_holders_and_rm_refuses_while_one_remains() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());

    let added = bisa(
        dir.path(),
        &[
            "--json",
            "agent",
            "add",
            "--name",
            "Scribe",
            "--prompt",
            "You write.",
        ],
    );
    assert_ok(&added, "agent add");
    let agent = json(&added)["agent"]["id"].as_str().unwrap().to_string();

    // Nothing points at it yet, and the command says so in the affirmative:
    // silence after a question about deletability reads as a failure.
    let empty = bisa(dir.path(), &["agent", "usage", &agent]);
    assert_ok(&empty, "agent usage");
    assert!(
        stdout(&empty).contains("nothing points at"),
        "{}",
        stdout(&empty)
    );

    assert_ok(
        &bisa(
            dir.path(),
            &["team", "create", "Engineering", "--agent", &agent],
        ),
        "team create",
    );

    // The listing names the holder the way a person would say it.
    let used = bisa(dir.path(), &["agent", "usage", &agent]);
    assert_ok(&used, "agent usage");
    let text = stdout(&used);
    assert!(text.contains("team"), "{text}");
    assert!(text.contains("Engineering"), "{text}");
    assert!(text.contains("Remove it from the team"), "{text}");

    // --json is the same answer, structurally.
    let as_json = bisa(dir.path(), &["--json", "agent", "usage", &agent]);
    assert_ok(&as_json, "agent usage --json");
    let v = json(&as_json);
    assert_eq!(v["usage"][0]["kind"], serde_json::json!("team"));
    assert_eq!(v["usage"][0]["label"], serde_json::json!("Engineering"));

    // And the delete refuses, non-zero, naming that holder.
    let refused = bisa(dir.path(), &["agent", "rm", &agent]);
    assert!(!refused.status.success(), "the delete must not succeed");
    let err = String::from_utf8_lossy(&refused.stderr).to_string();
    assert!(err.contains("team Engineering"), "{err}");

    // Detach, and the same delete goes through.
    let team = json(&bisa(dir.path(), &["--json", "team", "list"]));
    let team_id = team["teams"][0]["id"].as_str().unwrap().to_string();
    assert_ok(
        &bisa(
            dir.path(),
            &["team", "remove-member", &team_id, "--agent", &agent],
        ),
        "team remove-member",
    );
    assert_ok(&bisa(dir.path(), &["agent", "rm", &agent]), "agent rm");
}

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

#[test]
fn settings_are_set_at_a_scope_read_back_with_their_origin_and_refused_off_scope() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());

    let out = bisa(
        dir.path(),
        &["--json", "settings", "get", "editor.tab_size"],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_str(stdout(&out).trim()).unwrap();
    assert_eq!(v["setting"]["origin"], serde_json::json!("default"));

    let out = bisa(
        dir.path(),
        &[
            "--json",
            "settings",
            "set",
            "workspace",
            "editor.tab_size",
            "2",
        ],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_str(stdout(&out).trim()).unwrap();
    assert_eq!(v["setting"]["value"], serde_json::json!(2));
    assert_eq!(v["setting"]["origin"], serde_json::json!("workspace"));

    // A machine-only key at workspace scope is refused, and says which scopes would do.
    let out = bisa(
        dir.path(),
        &["settings", "set", "workspace", "editor.font_size", "14"],
    );
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("editor.font_size") && err.contains("Machine"),
        "{err}"
    );

    let out = bisa(
        dir.path(),
        &[
            "--json",
            "settings",
            "unset",
            "workspace",
            "editor.tab_size",
        ],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_str(stdout(&out).trim()).unwrap();
    assert_eq!(v["setting"]["origin"], serde_json::json!("default"));

    let out = bisa(dir.path(), &["--json", "settings", "registry"]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_str(stdout(&out).trim()).unwrap();
    assert!(v["settings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|d| d["key"] == "editor.tab_size"));
}

// ---------------------------------------------------------------------------
// Search and replace across a project's files
// ---------------------------------------------------------------------------

#[test]
fn files_search_and_replace_across_a_project() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());
    let out = bisa(dir.path(), &["--json", "project", "new", "web"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_str(stdout(&out).trim()).unwrap();
    let pid = v["project"]["id"].as_str().unwrap().to_string();
    let tree = bisa_store::Paths::new(dir.path())
        .project(&"web".parse().unwrap())
        .tree();
    std::fs::create_dir_all(tree.join("src")).unwrap();
    std::fs::write(tree.join("src/a.rs"), "let total = cart_total();\n").unwrap();
    std::fs::write(tree.join("src/b.rs"), "cart_total();\n").unwrap();

    // There is no project scope: a project's root *is* its primary workstream.
    let primary = bisa_core::WorkstreamId::primary_of(pid.parse().unwrap()).to_string();
    let out = bisa(
        dir.path(),
        &[
            "--json",
            "files",
            "search",
            "workstream",
            &primary,
            "cart_total",
        ],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_str(stdout(&out).trim()).unwrap();
    assert_eq!(v["summary"]["matches"], serde_json::json!(2));

    // Preview writes nothing; --apply writes both.
    let out = bisa(
        dir.path(),
        &[
            "--json",
            "files",
            "replace",
            "workstream",
            &primary,
            "cart_total",
            "basket_total",
        ],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_str(stdout(&out).trim()).unwrap();
    assert_eq!(v["applied"], serde_json::json!(false));
    assert_eq!(v["files"].as_array().unwrap().len(), 2);
    assert!(std::fs::read_to_string(tree.join("src/a.rs"))
        .unwrap()
        .contains("cart_total"));

    let out = bisa(
        dir.path(),
        &[
            "--json",
            "files",
            "replace",
            "workstream",
            &primary,
            "cart_total",
            "basket_total",
            "--apply",
        ],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(std::fs::read_to_string(tree.join("src/a.rs"))
        .unwrap()
        .contains("basket_total"));
    assert!(std::fs::read_to_string(tree.join("src/b.rs"))
        .unwrap()
        .contains("basket_total"));
}

/// `--committer "Name <email>"` on a creation writes the repository's local
/// identity ahead of any default, so the first commit is authored
/// as asked; a malformed one is refused by the CLI before anything is made.
#[test]
fn project_new_with_a_committer_writes_the_repository_s_local_identity() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());
    let made = bisa(
        dir.path(),
        &[
            "--json",
            "project",
            "new",
            "seeded",
            "--committer",
            "Ada Lovelace <ada@example.invalid>",
        ],
    );
    assert_ok(&made, "project new --committer");
    let pid = json(&made)["project"]["id"].as_str().unwrap().to_string();
    let path = json(&made)["path"].as_str().unwrap().to_string();

    let shown = bisa(dir.path(), &["--json", "project", "identity", &pid]);
    assert_ok(&shown, "identity show");
    assert_eq!(json(&shown)["source"], serde_json::json!("local"));
    assert_eq!(json(&shown)["name"], serde_json::json!("Ada Lovelace"));

    // In the repository's own config — and only there.
    let local = Command::new("git")
        .args(["-C", &path, "config", "--local", "--get", "user.email"])
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&local.stdout).trim(),
        "ada@example.invalid"
    );
}

/// `--git-config key=value` on a creation lands in the new repository's local
/// layer; `project git-config` reads and writes that layer afterwards, and a
/// key the platform does not know is refused before anything runs.
#[test]
fn git_config_on_a_creation_and_through_project_git_config_is_local_to_the_repository() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());
    let made = bisa(
        dir.path(),
        &[
            "--json",
            "project",
            "new",
            "configured",
            "--git-config",
            "user.useConfigOnly=true",
            "--git-config",
            "pull.rebase=true",
        ],
    );
    assert_ok(&made, "project new --git-config");
    let pid = json(&made)["project"]["id"].as_str().unwrap().to_string();
    let path = json(&made)["path"].as_str().unwrap().to_string();
    let local = Command::new("git")
        .args([
            "-C",
            &path,
            "config",
            "--local",
            "--get",
            "user.useConfigOnly",
        ])
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&local.stdout).trim(), "true");

    let shown = bisa(dir.path(), &["--json", "project", "git-config", &pid]);
    assert_ok(&shown, "git-config read");
    let entries = json(&shown)["entries"].as_array().unwrap().clone();
    let get = |key: &str| entries.iter().find(|e| e["key"] == key).unwrap().clone();
    assert_eq!(get("pull.rebase")["local"], serde_json::json!("true"));
    assert_eq!(get("core.autocrlf")["local"], serde_json::Value::Null);
    assert!(stdout(&bisa(dir.path(), &["project", "git-config", &pid]))
        .contains("pull.rebase = true (local)"));

    let set = bisa(
        dir.path(),
        &[
            "--json",
            "project",
            "git-config",
            &pid,
            "--set",
            "core.autocrlf=input",
            "--unset",
            "pull.rebase",
        ],
    );
    assert_ok(&set, "git-config write");
    let entries = json(&set)["entries"].as_array().unwrap().clone();
    let get = |key: &str| entries.iter().find(|e| e["key"] == key).unwrap().clone();
    assert_eq!(get("core.autocrlf")["local"], serde_json::json!("input"));
    assert_eq!(
        get("pull.rebase")["local"],
        serde_json::Value::Null,
        "unset falls through"
    );

    for bad in ["core.hooksPath=x", "pull.rebase", "init.defaultBranch=main"] {
        let out = bisa(dir.path(), &["project", "git-config", &pid, "--set", bad]);
        assert!(!out.status.success(), "{bad:?} must be refused");
    }
    let out = bisa(
        dir.path(),
        &["project", "git-config", "--set", "user.name=Ada"],
    );
    assert!(!out.status.success(), "neither a project nor --global");
}

#[test]
fn a_malformed_committer_is_refused_before_anything_is_created() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());
    for bad in [
        "Ada Lovelace",
        "ada@example.invalid",
        "--global <ada@example.invalid>",
        "Ada <not an email>",
    ] {
        // `--committer=<value>`: a value that itself begins with `--` is
        // still a value, not a flag.
        let committer = format!("--committer={bad}");
        let out = bisa(dir.path(), &["project", "new", "unmade", &committer]);
        assert!(!out.status.success(), "{bad:?} must be refused");
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(err.contains("Name <email>"), "{bad:?}: {err}");
    }
    let listed = bisa(dir.path(), &["--json", "project", "list"]);
    assert_ok(&listed, "project list");
    assert!(
        !stdout(&listed).contains("unmade"),
        "nothing was created: {}",
        stdout(&listed)
    );
}

#[test]
fn project_identity_is_shown_and_set_in_the_repository_alone() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());
    let made = bisa(dir.path(), &["--json", "project", "new", "idrepo"]);
    assert_ok(&made, "project new");
    let pid = json(&made)["project"]["id"].as_str().unwrap().to_string();

    // Shown before set: whatever the machine inherits, it is not this repository's.
    let shown = bisa(dir.path(), &["--json", "project", "identity", &pid]);
    assert_ok(&shown, "identity show");
    assert_ne!(json(&shown)["source"], serde_json::json!("local"));

    // Half an identity is refused by the parser, not the engine.
    let half = bisa(dir.path(), &["project", "identity", &pid, "--name", "Ada"]);
    assert!(!half.status.success(), "--name needs --email");

    let set = bisa(
        dir.path(),
        &[
            "--json",
            "project",
            "identity",
            &pid,
            "--name",
            "Ada Lovelace",
            "--email",
            "ada@example.invalid",
        ],
    );
    assert_ok(&set, "identity set");
    let v = json(&set);
    assert_eq!(v["source"], serde_json::json!("local"));
    assert_eq!(v["name"], serde_json::json!("Ada Lovelace"));
    assert_eq!(v["email"], serde_json::json!("ada@example.invalid"));

    let again = bisa(dir.path(), &["project", "identity", &pid]);
    assert_ok(&again, "identity show again");
    assert!(
        stdout(&again).contains("Ada Lovelace <ada@example.invalid> (local"),
        "{}",
        stdout(&again)
    );

    // Written to the project's own repository, verifiable without the platform.
    let path = json(&made)["path"].as_str().unwrap().to_string();
    let local = Command::new("git")
        .args(["-C", &path, "config", "--local", "--get", "user.email"])
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&local.stdout).trim(),
        "ada@example.invalid"
    );
}

/// `msg --artifact <path>:<title> --attach <path>`: the files go into the
/// store first, then the message names them — an artifact by its title and
/// kind, an attachment as a file.
#[test]
fn msg_uploads_then_posts_attachments_and_artifacts() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());
    let files = tempfile::tempdir().unwrap();
    let chart = files.path().join("chart.svg");
    std::fs::write(&chart, "<svg xmlns='http://www.w3.org/2000/svg'/>").unwrap();
    let notes = files.path().join("notes.txt");
    std::fs::write(&notes, "some notes").unwrap();
    let spec = format!("{}:Chart", chart.display());

    let posted = bisa(
        dir.path(),
        &[
            "--json",
            "msg",
            "general",
            "the chart and my notes",
            "--artifact",
            &spec,
            "--attach",
            notes.to_str().unwrap(),
        ],
    );
    assert_ok(&posted, "msg");
    let v = json(&posted);
    assert_eq!(v["artifacts"], serde_json::json!(1));
    assert_eq!(v["attachments"], serde_json::json!(1));
    let id = v["id"].as_str().unwrap().to_string();

    let ws = bisa_store::Workspace::open_with_keystore(
        dir.path(),
        Box::new(bisa_store::FileKeyStore::new(
            bisa_store::Paths::new(dir.path()).identity_dir(),
        )),
    )
    .unwrap();
    let row = ws.get_message(&id).unwrap().expect("the message");
    assert_eq!(row.artifacts.len(), 1);
    assert_eq!(row.artifacts[0].artifact.title, "Chart");
    assert_eq!(row.artifacts[0].artifact.kind, bisa_core::ArtifactKind::Svg);
    assert!(row.artifacts[0].present);
    assert_eq!(row.attachments.len(), 1);
    assert_eq!(row.attachments[0].file.name, "notes.txt");

    // A path that is not a file is refused by name, and nothing is posted.
    let refused = bisa(
        dir.path(),
        &[
            "--json",
            "msg",
            "general",
            "x",
            "--artifact",
            "/no/such/file.html:Nope",
        ],
    );
    assert!(!refused.status.success());
    assert_eq!(ws.list_artifacts("general", 10).unwrap().len(), 1);
}

#[test]
fn paths_names_the_workspace_without_opening_it_and_logs_lists_the_folder() {
    let dir = tempfile::tempdir().unwrap();
    let out = bisa(dir.path(), &["--json", "paths"]);
    assert_ok(&out, "paths");
    let v = json(&out);
    assert_eq!(v["data_dir"], dir.path().display().to_string());
    assert_eq!(v["logs_dir"], dir.path().join("logs").display().to_string());
    assert!(
        !dir.path().join("identity").exists(),
        "paths opens nothing and makes nothing but its own log"
    );

    // An empty folder: every family, no file, no report. The command's own
    // `cli` file may be there — the log is on by default — so a family is
    // judged by its name, not its count.
    let out = bisa(dir.path(), &["--json", "logs"]);
    assert_ok(&out, "logs");
    let v = json(&out);
    assert_eq!(v["dir"], dir.path().join("logs").display().to_string());
    let families: Vec<&str> = v["families"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["process"].as_str().unwrap())
        .collect();
    assert_eq!(families, ["node", "cli", "mcp", "desktop"]);
    assert_eq!(v["crashes"], serde_json::json!([]));
    assert_eq!(v["latest_crash"], serde_json::Value::Null);

    // A planted report is listed and is the newest.
    let mut report = bisa_log::CrashReport::new(bisa_log::CrashKind::Panic, "one bad row");
    report.process = bisa_log::Process::Node;
    report.version = "0.0.0-test".to_string();
    report.pid = 7;
    report.at = "2026-09-11T10:22:33Z".to_string();
    let logs = dir.path().join("logs");
    let path = bisa_log::write_crash(&logs, &report, std::time::SystemTime::now()).unwrap();
    let name = path.file_name().unwrap().to_string_lossy().into_owned();
    let out = bisa(dir.path(), &["--json", "logs"]);
    assert_ok(&out, "logs");
    let v = json(&out);
    assert_eq!(v["crashes"][0]["name"], name);
    assert_eq!(v["latest_crash"]["name"], name);
    assert_eq!(v["latest_crash"]["kind"], "panic");
    assert_eq!(v["latest_crash"]["message"], "one bad row");
    let human = bisa(dir.path(), &["logs"]);
    assert_ok(&human, "logs");
    let text = String::from_utf8_lossy(&human.stdout);
    assert!(text.contains("newest crash"), "{text}");
    assert!(text.contains(&name), "{text}");
}

/// The exit codes a script reads: 2 when the command line cannot be parsed
/// (clap's), 1 when the command ran and failed, 0 when it did what it said —
/// and in `--json` a failure leaves stdout empty, its words on stderr alone.
#[test]
fn exit_codes_tell_a_parse_error_from_a_failure_and_json_keeps_stdout_clean() {
    let dir = tempfile::tempdir().unwrap();
    let parse = bisa(dir.path(), &["no-such-verb"]);
    assert_eq!(
        parse.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&parse.stderr)
    );
    let parse = bisa(dir.path(), &["settings", "--no-such-flag"]);
    assert_eq!(parse.status.code(), Some(2));
    let parse = bisa(dir.path(), &["settings", "get"]);
    assert_eq!(
        parse.status.code(),
        Some(2),
        "a required argument missing is a parse error"
    );

    let ok = bisa(dir.path(), &["init", "--json"]);
    assert_eq!(
        ok.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&ok.stderr)
    );
    let failed = bisa(dir.path(), &["--json", "settings", "get", "no.such.key"]);
    assert_eq!(
        failed.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&failed.stderr)
    );
    assert!(
        failed.stdout.is_empty(),
        "a failure writes nothing to stdout in --json: {}",
        String::from_utf8_lossy(&failed.stdout)
    );
    assert!(String::from_utf8_lossy(&failed.stderr).contains("no.such.key"));
}

// ---------------------------------------------------------------------------
// Events and gateways: listening, signals, test runs
// ---------------------------------------------------------------------------

/// A step a run stays live on until somebody releases it.
fn hold() -> serde_json::Value {
    serde_json::json!({
        "id": "hold", "name": "Hold", "kind": "wait", "until": {"until": "release"}
    })
}

/// A workflow only a call begins: one hook start, `ticket` — public or not —
/// that maps the body's subject, and a hold its runs stay live on.
fn hook_workflow(name: &str, public: bool) -> serde_json::Value {
    serde_json::json!({
        "name": name,
        "inputs": [{"name": "subject", "label": "Subject", "kind": "text", "required": true}],
        "steps": [
            {"id": "ticket", "name": "A ticket arrives", "kind": "start",
             "on": {"event": "hook", "public": public},
             "inputs": {"subject": "{event.payload.subject}"},
             "then": ["hold"]},
            hold(),
        ]
    })
}

/// A workflow with two ways in: by hand, and on a call.
fn two_ways_in(name: &str) -> serde_json::Value {
    serde_json::json!({
        "name": name,
        "steps": [
            {"id": "by-hand", "name": "By hand", "kind": "start",
             "on": {"event": "manual"}, "then": ["hold"]},
            {"id": "ticket", "name": "A ticket arrives", "kind": "start",
             "on": {"event": "hook"}, "then": ["hold"]},
            hold(),
        ]
    })
}

/// A workflow that begins every hour and says who its digest is for — an
/// input its event does not supply and no default fills.
fn hourly_digest() -> serde_json::Value {
    serde_json::json!({
        "name": "Hourly digest",
        "inputs": [
            {"name": "who", "label": "Who", "kind": "text", "required": true},
            {"name": "at", "label": "At", "kind": "number"},
        ],
        "steps": [
            {"id": "hourly", "name": "Every hour", "kind": "start",
             "on": {"event": "schedule", "every": 3600},
             "inputs": {"at": "{event.payload.at}"},
             "then": ["tell"]},
            {"id": "tell", "name": "Tell", "kind": "notify",
             "template": "the digest for {inputs.who}", "then": ["hold"]},
            hold(),
        ]
    })
}

/// A workflow that begins when the signal `name` is raised.
fn on_signal(name: &str) -> serde_json::Value {
    serde_json::json!({
        "name": format!("On {name}"),
        "steps": [
            {"id": "raised", "name": "The signal is raised", "kind": "start",
             "on": {"event": "signal", "name": name}, "then": ["hold"]},
            hold(),
        ]
    })
}

/// Record a definition and answer its id. The fixture is sound: one with
/// problems fails the test that wrote it.
fn workflow_of(dir: &Path, definition: serde_json::Value) -> String {
    let path = dir.join("definition.json");
    std::fs::write(&path, definition.to_string()).unwrap();
    let out = bisa(
        dir,
        &[
            "--json",
            "workflow",
            "new",
            "--from",
            path.to_str().unwrap(),
        ],
    );
    assert_ok(&out, "workflow new");
    let v = json(&out);
    assert_eq!(
        v["problems"],
        serde_json::json!([]),
        "the fixture is sound: {v}"
    );
    v["workflow"]["id"].as_str().unwrap().to_string()
}

/// Ask `args` until `done` answers something, a few seconds at most.
fn until<T>(
    dir: &Path,
    args: &[&str],
    what: &str,
    done: impl Fn(&serde_json::Value) -> Option<T>,
) -> T {
    for _ in 0..50 {
        let out = bisa(dir, args);
        if out.status.success() {
            if let Some(found) = done(&json(&out)) {
                return found;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    panic!("{what} never came: {}", stdout(&bisa(dir, args)));
}

/// A library workflow says how it begins and what turning it on asks; on,
/// it says where it stands and what it listens for — and the toggle is never
/// a revision of it.
#[test]
fn a_workflow_is_turned_on_and_off_and_says_where_it_stands() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());
    let wf = workflow_of(dir.path(), hourly_digest());
    let listener = format!("workspace:{wf}/hourly");

    // Off, it says how it begins and what turning it on asks.
    let shown = json(&bisa(dir.path(), &["--json", "workflow", "show", &wf]));
    assert_eq!(shown["listening"], serde_json::Value::Null, "{shown}");
    assert_eq!(shown["event_only"], true, "no start by hand: {shown}");
    assert_eq!(
        shown["listening_needs"],
        serde_json::json!(["who"]),
        "{shown}"
    );
    assert_eq!(shown["starts"][0]["step"], "hourly", "{shown}");
    assert_eq!(shown["starts"][0]["event"], "schedule", "{shown}");
    let revision = shown["workflow"]["revision"].clone();
    let page = stdout(&bisa(dir.path(), &["workflow", "show", &wf]));
    assert!(
        page.contains("begins every 3600 seconds")
            && page.contains(&format!("bisa workflow on {wf} --input"))
            && page.contains("for who"),
        "{page}"
    );

    // What its events do not supply is asked for, and nothing the workflow
    // does not declare is taken: a refusal turns nothing on.
    let refused = bisa(dir.path(), &["workflow", "on", &wf]);
    assert!(!refused.status.success(), "{}", stdout(&refused));
    assert!(stderr(&refused).contains("who"), "{}", stderr(&refused));
    let refused = bisa(
        dir.path(),
        &[
            "workflow",
            "on",
            &wf,
            "--input",
            "who=the team",
            "--input",
            "whom=a typo",
        ],
    );
    assert!(!refused.status.success(), "{}", stdout(&refused));
    assert!(stderr(&refused).contains("whom"), "{}", stderr(&refused));
    assert_eq!(
        json(&bisa(dir.path(), &["--json", "workflow", "listeners", &wf]))["listeners"],
        serde_json::json!([]),
        "a refusal turns nothing on"
    );

    let turn_on = [
        "workflow",
        "on",
        wf.as_str(),
        "--input",
        "who=the team",
        "--budget-tokens",
        "10000",
    ];
    let on = bisa(dir.path(), &[&["--json"][..], &turn_on[..]].concat());
    assert_ok(&on, "workflow on");
    let on = json(&on);
    assert_eq!(
        on["secrets"],
        serde_json::json!([]),
        "no public hook, no secret"
    );
    let standing = &on["workflow"]["listening"];
    assert_eq!(
        standing["inputs"],
        serde_json::json!({"who": "the team"}),
        "{on}"
    );
    assert_eq!(
        standing["budget"],
        serde_json::json!({"max_tokens": 10000}),
        "{on}"
    );
    assert!(standing["since"].is_u64(), "{on}");
    assert_eq!(
        on["workflow"]["workflow"]["revision"], revision,
        "a toggle is never a revision"
    );
    let listeners = on["listeners"].as_array().expect("listeners");
    assert_eq!(listeners.len(), 1, "{on}");
    assert_eq!(listeners[0]["listener"], listener.as_str());
    assert_eq!(listeners[0]["host"], format!("workspace:{wf}"));
    assert_eq!(listeners[0]["step"], "hourly");
    assert_eq!(listeners[0]["event"], "schedule");
    assert_eq!(listeners[0]["backlog"], 0);
    assert_eq!(listeners[0]["live_runs"], 0);

    // Said: where it stands and what it listens for — and, with no node
    // here, that its events are heard once one runs.
    let said = bisa(dir.path(), &turn_on);
    assert_ok(&said, "workflow on, in words");
    let text = stdout(&said);
    for word in [
        format!("workflow {wf} is on"),
        "listening since".to_string(),
        "who=the team".to_string(),
        "at most 10000 tokens".to_string(),
        listener.clone(),
        "begins every 3600 seconds".to_string(),
        "bisa node".to_string(),
    ] {
        assert!(text.contains(&word), "{word}: {text}");
    }

    // Its row in the library and its page say it is on.
    let library = json(&bisa(dir.path(), &["--json", "workflow", "list"]));
    let row = library["workflows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["workflow"]["id"] == wf.as_str())
        .expect("the workflow's row");
    assert_eq!(
        row["listening"]["inputs"],
        serde_json::json!({"who": "the team"})
    );
    let rows = stdout(&bisa(dir.path(), &["workflow", "list"]));
    assert!(rows.contains("· on: begins every 3600 seconds"), "{rows}");
    let page = stdout(&bisa(dir.path(), &["workflow", "show", &wf]));
    assert!(
        page.contains("listening since") && !page.contains("turn it on"),
        "{page}"
    );

    // Its listeners: its own, and among everyone's.
    let own = json(&bisa(dir.path(), &["--json", "workflow", "listeners", &wf]));
    assert_eq!(own["listeners"].as_array().unwrap().len(), 1, "{own}");
    let all = json(&bisa(dir.path(), &["--json", "workflow", "listeners"]));
    assert_eq!(all["listeners"], own["listeners"], "{all}");
    let table = stdout(&bisa(dir.path(), &["workflow", "listeners"]));
    assert!(
        table.contains(&listener) && table.contains("backlog 0"),
        "{table}"
    );
    // One workflow's or one goal's, never both.
    let both = bisa(dir.path(), &["workflow", "listeners", &wf, "--goal", &wf]);
    assert_eq!(both.status.code(), Some(2), "{}", stderr(&both));

    // Off: the standing goes, the listeners with it; off again is nothing.
    let off = bisa(dir.path(), &["--json", "workflow", "off", &wf]);
    assert_ok(&off, "workflow off");
    let off = json(&off);
    assert_eq!(
        off["workflow"]["listening"],
        serde_json::Value::Null,
        "{off}"
    );
    assert_eq!(off["workflow"]["workflow"]["revision"], revision);
    assert_eq!(
        json(&bisa(dir.path(), &["--json", "workflow", "listeners", &wf]))["listeners"],
        serde_json::json!([])
    );
    let again = bisa(dir.path(), &["workflow", "off", &wf]);
    assert_ok(&again, "off already is nothing to do");
    assert!(stdout(&again).contains("is off"), "{}", stdout(&again));
    let nobody = stdout(&bisa(dir.path(), &["workflow", "listeners"]));
    assert!(nobody.contains("nothing is listening"), "{nobody}");

    // A workflow nobody has, and an id that is none.
    for verb in ["on", "off", "listeners"] {
        let ghost = bisa(
            dir.path(),
            &["workflow", verb, "01ARZ3NDEKTSV4RRFFQ69G5FAV"],
        );
        assert!(!ghost.status.success(), "workflow {verb} of nobody's");
        let bad = bisa(dir.path(), &["workflow", verb, "not-an-id"]);
        assert!(!bad.status.success(), "workflow {verb} of no id");
        assert!(
            stderr(&bad).contains("not a workflow id"),
            "{}",
            stderr(&bad)
        );
    }
}

/// A public hook's secret is printed by the turn that mints it and by a
/// rotation — and by nothing else: no page, no list, no second turn.
#[test]
fn a_hook_s_secret_is_shown_when_it_is_minted_or_rotated_and_never_again() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());
    let wf = workflow_of(dir.path(), hook_workflow("From outside", true));
    let public = format!("/hooks/workspace:{wf}/ticket");
    let local = format!("/workflows/{wf}/hooks/ticket");
    let rotate = format!("bisa workflow hook-secret workflow {wf} ticket --rotate");

    let on = bisa(dir.path(), &["--json", "workflow", "on", &wf]);
    assert_ok(&on, "workflow on");
    let on = json(&on);
    let secret = on["secrets"][0]["secret"]
        .as_str()
        .expect("the secret, shown once")
        .to_string();
    assert_eq!(secret.len(), 64, "32 random bytes, hex");
    assert_eq!(on["secrets"][0]["step"], "ticket");
    assert_eq!(on["secrets"][0]["path"], public.as_str());
    assert_eq!(on["listeners"][0]["event"], "hook");
    assert_eq!(on["listeners"][0]["local_hook"], local.as_str());
    assert_eq!(
        on["listeners"][0]["public_hook"],
        serde_json::json!({"path": public, "has_secret": true})
    );

    // Turned on again, nothing is minted and nothing is shown.
    let again = bisa(dir.path(), &["workflow", "on", &wf]);
    assert_ok(&again, "workflow on again");
    let text = stdout(&again);
    assert!(
        !text.contains(&secret) && !text.contains("will not be shown again"),
        "{text}"
    );
    assert!(text.contains("its secret is minted"), "{text}");

    // No read shows it.
    for read in [
        vec!["workflow", "listeners"],
        vec!["--json", "workflow", "listeners", wf.as_str()],
        vec!["workflow", "show", wf.as_str()],
        vec!["--json", "workflow", "show", wf.as_str()],
        vec!["workflow", "list"],
        vec!["signal", "list"],
        vec!["workflow", "hook-secret", "workflow", wf.as_str(), "ticket"],
    ] {
        let out = bisa(dir.path(), &read);
        assert_ok(&out, "a read");
        assert!(
            !stdout(&out).contains(&secret),
            "{read:?} leaked the secret"
        );
    }

    // Its standing says a secret is minted, and how one is ever shown.
    let standing = ["workflow", "hook-secret", "workflow", wf.as_str(), "ticket"];
    let text = stdout(&bisa(dir.path(), &standing));
    for word in [
        local.as_str(),
        public.as_str(),
        "its secret is minted",
        "minted or rotated",
    ] {
        assert!(text.contains(word), "{word}: {text}");
    }
    let read = json(&bisa(
        dir.path(),
        &[&["--json"][..], &standing[..]].concat(),
    ));
    assert_eq!(read["listener"]["step"], "ticket", "{read}");
    assert_eq!(read["listener"]["public_hook"]["has_secret"], true);

    // Rotation is the only recovery: a new secret, shown this once.
    let rotated = bisa(
        dir.path(),
        &[&["--json"][..], &standing[..], &["--rotate"][..]].concat(),
    );
    assert_ok(&rotated, "hook-secret --rotate");
    let rotated = json(&rotated);
    assert_eq!(rotated["step"], "ticket");
    assert_eq!(rotated["path"], public.as_str());
    let new_secret = rotated["secret"].as_str().unwrap().to_string();
    assert_eq!(new_secret.len(), 64);
    assert_ne!(new_secret, secret);
    let said = stdout(&bisa(
        dir.path(),
        &[&standing[..], &["--rotate"][..]].concat(),
    ));
    for word in [
        public.as_str(),
        rotate.as_str(),
        "will not be shown again",
        "X-Bisa-Token",
        "X-Hub-Signature-256",
        "events.public_hooks",
    ] {
        assert!(said.contains(word), "{word}: {said}");
    }
    assert!(
        !said.contains(&secret) && !said.contains(&new_secret),
        "every rotation mints its own: {said}"
    );

    // A hook that is not public has no secret to show or to rotate.
    let private = workflow_of(dir.path(), hook_workflow("From here only", false));
    let on = json(&bisa(dir.path(), &["--json", "workflow", "on", &private]));
    assert_eq!(on["secrets"], serde_json::json!([]));
    assert!(on["listeners"][0].get("public_hook").is_none(), "{on}");
    let own = [
        "workflow",
        "hook-secret",
        "workflow",
        private.as_str(),
        "ticket",
    ];
    let text = stdout(&bisa(dir.path(), &own));
    assert!(
        text.contains(&format!("/workflows/{private}/hooks/ticket")) && text.contains("not public"),
        "{text}"
    );
    let refused = bisa(dir.path(), &[&own[..], &["--rotate"][..]].concat());
    assert!(!refused.status.success(), "{}", stdout(&refused));
    assert!(
        stderr(&refused).contains("not a public hook start"),
        "{}",
        stderr(&refused)
    );

    // A step that is no hook, a host that is off, a word that is no host.
    let no_hook = bisa(
        dir.path(),
        &["workflow", "hook-secret", "workflow", &wf, "hold"],
    );
    assert!(!no_hook.status.success());
    assert!(
        stderr(&no_hook).contains("no hook listens at step hold"),
        "{}",
        stderr(&no_hook)
    );
    assert_ok(&bisa(dir.path(), &["workflow", "off", &wf]), "workflow off");
    let off = bisa(dir.path(), &standing);
    assert!(!off.status.success());
    assert!(
        stderr(&off).contains(&format!("workspace:{wf}")),
        "{}",
        stderr(&off)
    );
    let no_host = bisa(
        dir.path(),
        &["workflow", "hook-secret", "everything", &wf, "ticket"],
    );
    assert_eq!(
        no_host.status.code(),
        Some(2),
        "a host is `workflow` or `goal`: {}",
        stderr(&no_host)
    );
}

/// A named signal is raised by hand — nothing listening is a normal answer
/// — listed where it stands, and let through when the content screen held
/// it. Nothing here starts a run: with no node, what is queued waits for one.
#[test]
fn a_signal_is_raised_listed_and_let_through() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());
    let wf = workflow_of(dir.path(), on_signal("report.ready"));
    let listener = format!("workspace:{wf}/raised");
    let goal = json(&bisa(
        dir.path(),
        &["--json", "new", "raise on me", "--mode", "manual"],
    ))["goal"]
        .as_str()
        .unwrap()
        .to_string();

    // Nothing listening is a normal, quiet answer.
    let quiet = bisa(
        dir.path(),
        &[
            "--json",
            "signal",
            "emit",
            "report.ready",
            "--data",
            r#"{"url": "https://example.test/1"}"#,
        ],
    );
    assert_ok(&quiet, "signal emit");
    let quiet = json(&quiet);
    assert_eq!(quiet["name"], "report.ready");
    assert!(quiet["signal"].is_string(), "{quiet}");
    assert_eq!(quiet["listeners"], serde_json::json!([]), "{quiet}");
    let said = stdout(&bisa(dir.path(), &["signal", "emit", "report.ready"]));
    assert!(
        said.contains("report.ready") && said.contains("nothing is listening"),
        "{said}"
    );

    // On, its listener hears the signal: a signal of its own is written.
    assert_ok(&bisa(dir.path(), &["workflow", "on", &wf]), "workflow on");
    let raised = json(&bisa(
        dir.path(),
        &[
            "--json",
            "signal",
            "emit",
            "report.ready",
            "--data",
            r#"{"url": "https://example.test/2"}"#,
        ],
    ));
    let record = raised["signal"].as_str().expect("the record").to_string();
    let heard = raised["listeners"].as_array().expect("listeners").clone();
    assert_eq!(heard.len(), 1, "{raised}");
    let said = stdout(&bisa(dir.path(), &["signal", "emit", "report.ready"]));
    assert!(said.contains("one listener heard it"), "{said}");
    // Another name is nobody's.
    let other = json(&bisa(
        dir.path(),
        &["--json", "signal", "emit", "report.late"],
    ));
    assert_eq!(other["listeners"], serde_json::json!([]), "{other}");

    let listed = bisa(dir.path(), &["--json", "signal", "list"]);
    assert_ok(&listed, "signal list");
    let signals = json(&listed)["signals"].as_array().expect("a list").clone();
    let kept = signals
        .iter()
        .find(|s| s["id"] == record.as_str())
        .expect("the record");
    assert!(kept.get("listener").is_none(), "{kept}");
    assert!(kept.get("payload").is_none(), "never a payload: {kept}");
    assert_eq!(kept["source"], "signal");
    assert_eq!(kept["name"], "report.ready");
    assert_eq!(kept["state"], "done", "kept for the waits, never claimed");
    assert_eq!(kept["scope"], serde_json::json!({"scope": "workspace"}));
    let for_listener = signals
        .iter()
        .find(|s| s["id"] == heard[0])
        .expect("the listener's");
    assert_eq!(for_listener["listener"], listener.as_str());
    assert_eq!(for_listener["name"], "report.ready");
    assert_eq!(
        for_listener["state"], "queued",
        "no node here: it waits for one"
    );
    let table = stdout(&bisa(dir.path(), &["signal", "list"]));
    assert!(
        table.contains(&record)
            && table.contains("report.ready")
            && table.contains("queued")
            && table.contains(&listener),
        "{table}"
    );
    let one = json(&bisa(
        dir.path(),
        &["--json", "signal", "list", "--limit", "1"],
    ));
    assert_eq!(one["signals"].as_array().unwrap().len(), 1, "{one}");

    // On a goal when one is named; refused for one nobody has.
    let scoped = json(&bisa(
        dir.path(),
        &["--json", "signal", "emit", "report.ready", "--goal", &goal],
    ));
    let signals = json(&bisa(dir.path(), &["--json", "signal", "list"]))["signals"]
        .as_array()
        .unwrap()
        .clone();
    let kept = signals
        .iter()
        .find(|s| s["id"] == scoped["signal"])
        .expect("the record");
    assert_eq!(
        kept["scope"],
        serde_json::json!({"scope": "goal", "goal": goal})
    );
    let nobody = bisa(
        dir.path(),
        &[
            "signal",
            "emit",
            "report.ready",
            "--goal",
            "01ARZ3NDEKTSV4RRFFQ69G5FAV",
        ],
    );
    assert!(!nobody.status.success(), "{}", stdout(&nobody));
    let no_id = bisa(
        dir.path(),
        &["signal", "emit", "report.ready", "--goal", "not-an-id"],
    );
    assert!(!no_id.status.success());
    assert!(
        stderr(&no_id).contains("not a goal id"),
        "{}",
        stderr(&no_id)
    );

    // A name that is no name, a sample that is no JSON.
    for bad in ["Report.Ready", "a..b", "sp ace"] {
        let refused = bisa(dir.path(), &["signal", "emit", bad]);
        assert!(!refused.status.success(), "{bad:?}");
        assert!(
            stderr(&refused).contains("is not a signal name"),
            "{bad:?}: {}",
            stderr(&refused)
        );
    }
    let refused = bisa(
        dir.path(),
        &["signal", "emit", "report.ready", "--data", "{url"],
    );
    assert!(!refused.status.success());
    assert!(
        stderr(&refused).contains("is not JSON"),
        "{}",
        stderr(&refused)
    );

    // A signal the content screen held is let through by a person — and
    // only a held one is.
    let held = {
        let ws = open_ws(dir.path());
        let signal = bisa_core::Signal {
            id: ulid::Ulid::from_parts(7, 7).to_string(),
            listener: Some(listener.parse().expect("a listener")),
            source: bisa_core::SignalSource::Signal,
            name: Some("report.ready".into()),
            at: 7,
            payload: serde_json::json!({"url": "https://example.test/held"}),
            scope: bisa_core::SignalScope::Workspace,
            chain: Default::default(),
            dedupe_key: None,
        };
        let (queued, fresh) = ws
            .enqueue_signal_as(
                &signal,
                bisa_store::SignalState::Held,
                Some("held by the content screen: nobody gave a verdict"),
            )
            .expect("a held signal");
        assert!(fresh, "the fixture is new");
        queued.signal.id
    };
    let listed = json(&bisa(dir.path(), &["--json", "signal", "list"]));
    let row = listed["signals"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"] == held.as_str())
        .cloned()
        .expect("the held signal");
    assert_eq!(row["state"], "held", "{row}");
    assert!(
        row["note"]
            .as_str()
            .is_some_and(|note| note.starts_with("held by the content screen")),
        "{row}"
    );
    let table = stdout(&bisa(dir.path(), &["signal", "list"]));
    assert!(table.contains("held by the content screen"), "{table}");

    let released = bisa(dir.path(), &["--json", "signal", "release", &held]);
    assert_ok(&released, "signal release");
    let released = json(&released);
    assert_eq!(released["signal"]["id"], held.as_str());
    assert_eq!(released["signal"]["state"], "queued", "{released}");
    let again = bisa(dir.path(), &["signal", "release", &held]);
    assert!(!again.status.success(), "{}", stdout(&again));
    assert!(stderr(&again).contains("not held"), "{}", stderr(&again));
    let nobody = bisa(dir.path(), &["signal", "release", "no-such-signal"]);
    assert!(!nobody.status.success());
    assert!(
        stderr(&nobody).contains("signal not found"),
        "{}",
        stderr(&nobody)
    );
    let said = {
        let ws = open_ws(dir.path());
        ws.move_signal(
            &held,
            bisa_store::SignalState::Held,
            Some("held by the content screen: nobody gave a verdict"),
        )
        .expect("held again");
        stdout(&bisa(dir.path(), &["signal", "release", &held]))
    };
    assert!(
        said.contains(&held) && said.contains("let through") && said.contains("queued"),
        "{said}"
    );
}

/// A goal whose workflow begins on events listens when it is started: no
/// run is made, its page says what it hears, the start by hand is a run now
/// and an event start a test run — and stopping it stops the listening.
#[test]
fn a_goal_that_begins_on_events_listens_and_runs_at_the_start_it_is_told() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());
    let wf = workflow_of(dir.path(), two_ways_in("Two ways in"));
    let goal = json(&bisa(
        dir.path(),
        &[
            "--json",
            "new",
            "answer every ticket",
            "--workflow",
            &wf,
            "--no-start",
        ],
    ))["goal"]
        .as_str()
        .unwrap()
        .to_string();
    let listener = format!("goal:{goal}/ticket");
    let hook = format!("/goals/{goal}/hooks/ticket");
    let listeners_of = ["--json", "workflow", "listeners", "--goal", goal.as_str()];

    // Not started: it listens to nothing.
    let page = json(&bisa(dir.path(), &["--json", "status", &goal]));
    assert_eq!(page["status"], "draft", "{page}");
    assert!(page["goal"].get("listening").is_none(), "{page}");
    assert_eq!(
        json(&bisa(dir.path(), &listeners_of))["listeners"],
        serde_json::json!([])
    );

    // A person's start arms its events rather than running: the answer is
    // where the goal stands and what it hears, and no run.
    let begun = bisa(dir.path(), &["--json", "run", &goal]);
    assert_ok(&begun, "run arms the goal");
    let begun = json(&begun);
    assert_eq!(begun["goal"], goal.as_str());
    assert_eq!(begun["status"], "waiting", "on its next event: {begun}");
    assert_eq!(
        begun["run"],
        serde_json::Value::Null,
        "arming makes no run: {begun}"
    );
    assert_eq!(
        begun["secrets"],
        serde_json::json!([]),
        "no public hook, no secret"
    );
    let listeners = begun["listeners"].as_array().expect("listeners");
    assert_eq!(listeners.len(), 1, "the start by hand is no listener");
    assert_eq!(listeners[0]["listener"], listener.as_str());
    assert_eq!(listeners[0]["host"], format!("goal:{goal}"));
    assert_eq!(listeners[0]["event"], "hook");
    assert_eq!(listeners[0]["local_hook"], hook.as_str());
    assert_eq!(
        json(&bisa(dir.path(), &listeners_of))["listeners"],
        begun["listeners"]
    );

    // Its page says it listens: since when, for what, and — with no node
    // here — that its events are heard once one runs.
    let page = json(&bisa(dir.path(), &["--json", "status", &goal]));
    assert_eq!(page["status"], "waiting", "on its next event: {page}");
    assert!(page["goal"]["listening"]["since"].is_u64(), "{page}");
    assert_eq!(page["run"], serde_json::Value::Null, "{page}");
    let text = stdout(&bisa(dir.path(), &["status", &goal]));
    for word in [
        "listening since".to_string(),
        "ticket  hook  begins when called".to_string(),
        format!("bisa workflow listeners --goal {goal}"),
        "bisa node".to_string(),
    ] {
        assert!(text.contains(&word), "{word}: {text}");
    }
    // Started again, it listens as it did, and says so.
    let said = bisa(dir.path(), &["run", &goal]);
    assert_ok(&said, "run on a goal that listens");
    let text = stdout(&said);
    for word in [
        format!("goal {goal} is listening"),
        listener.clone(),
        hook.clone(),
        "bisa node".to_string(),
    ] {
        assert!(text.contains(&word), "{word}: {text}");
    }

    // The start by hand is a run now, whatever else the workflow begins on.
    let now = bisa(dir.path(), &["--json", "run", &goal, "--start", "by-hand"]);
    assert_ok(&now, "run --start by-hand");
    assert!(
        stderr(&now).contains(&format!("bisa step release {goal} hold")),
        "{}",
        stderr(&now)
    );
    let now = json(&now);
    assert_eq!(now["status"], "waiting", "held on its wait: {now}");
    assert_eq!(now["run"]["steps"]["hold"]["state"]["state"], "waiting");
    let first = now["run"]["id"].as_str().unwrap().to_string();

    // An event start is a test run, begun there as if its event had
    // happened — queued behind the live run, which is the one followed.
    let test = bisa(
        dir.path(),
        &[
            "--json",
            "run",
            &goal,
            "--start",
            "ticket",
            "--data",
            r#"{"subject": "help"}"#,
        ],
    );
    assert_ok(&test, "run --start ticket");
    assert!(
        stderr(&test).contains("queued behind run"),
        "{}",
        stderr(&test)
    );
    assert_eq!(
        json(&test)["run"]["id"],
        first.as_str(),
        "the live run is followed"
    );
    let runs = json(&bisa(dir.path(), &["--json", "runs", &goal]))["runs"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(runs.len(), 2, "{runs:?}");
    assert_eq!(
        runs[0]["started_by"],
        serde_json::json!({"by": "test", "event": "hook"}),
        "newest first"
    );
    assert_eq!(runs[0]["position"], 1);
    assert_eq!(runs[1]["id"], first.as_str());
    assert_eq!(runs[1]["started_by"], serde_json::json!({"by": "you"}));
    let listed = stdout(&bisa(dir.path(), &["runs", &goal]));
    assert!(
        listed.contains("test, as if hook had happened") && listed.contains("by you"),
        "{listed}"
    );
    let page = stdout(&bisa(dir.path(), &["status", &goal]));
    assert!(page.contains("started:   by you"), "{page}");

    // A sample needs the start that reads it, the start by hand reads none,
    // and a step that is no start begins nothing.
    let no_start = bisa(dir.path(), &["run", &goal, "--data", "{}"]);
    assert_eq!(no_start.status.code(), Some(2), "{}", stderr(&no_start));
    let by_hand = bisa(
        dir.path(),
        &["run", &goal, "--start", "by-hand", "--data", "{}"],
    );
    assert_eq!(by_hand.status.code(), Some(1), "{}", stderr(&by_hand));
    assert!(
        stderr(&by_hand).contains("begins by hand and reads no event"),
        "{}",
        stderr(&by_hand)
    );
    let no_step = bisa(dir.path(), &["run", &goal, "--start", "hold"]);
    assert_eq!(no_step.status.code(), Some(1), "{}", stderr(&no_step));
    assert!(
        stderr(&no_step).contains("is not a start"),
        "{}",
        stderr(&no_step)
    );
    let no_json = bisa(
        dir.path(),
        &["run", &goal, "--start", "ticket", "--data", "{subject"],
    );
    assert!(!no_json.status.success());
    assert!(
        stderr(&no_json).contains("is not JSON"),
        "{}",
        stderr(&no_json)
    );
    assert_eq!(
        json(&bisa(dir.path(), &["--json", "runs", &goal]))["runs"]
            .as_array()
            .unwrap()
            .len(),
        2,
        "a refusal makes no run"
    );

    // Stopping the goal stops the listening with its runs.
    let stopped = bisa(dir.path(), &["--json", "stop", &goal]);
    assert_ok(&stopped, "stop");
    let s = json(&stopped);
    assert_eq!(s["stopped"], first.as_str(), "{s}");
    assert_eq!(s["withdrawn"].as_array().map(Vec::len), Some(1), "{s}");
    assert_eq!(s["listening_stopped"], true, "{s}");
    let page = json(&bisa(dir.path(), &["--json", "status", &goal]));
    assert!(page["goal"].get("listening").is_none(), "{page}");
    assert_eq!(
        json(&bisa(dir.path(), &listeners_of))["listeners"],
        serde_json::json!([])
    );

    // A goal that only listened says so when it is stopped.
    assert_ok(&bisa(dir.path(), &["run", &goal]), "it listens again");
    let said = stdout(&bisa(dir.path(), &["stop", &goal]));
    assert!(
        said.contains("no longer listens") && !said.contains("nothing to stop"),
        "{said}"
    );
    let nothing = bisa(dir.path(), &["--json", "stop", &goal]);
    assert_ok(&nothing, "stop with nothing to stop");
    assert_eq!(json(&nothing)["listening_stopped"], false);
    let said = stdout(&bisa(dir.path(), &["stop", &goal]));
    assert!(said.contains("nothing to stop"), "{said}");
}

/// A goal captured on a workflow that begins on a public hook listens at
/// once, and its hook's secret is printed with it — once.
#[test]
fn a_goal_captured_on_a_public_hook_is_shown_its_secret_once() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());
    let wf = workflow_of(dir.path(), hook_workflow("From outside", true));

    let captured = bisa(
        dir.path(),
        &["--json", "new", "answer the outside", "--workflow", &wf],
    );
    assert_ok(&captured, "new --workflow");
    let captured = json(&captured);
    let goal = captured["goal"].as_str().unwrap().to_string();
    let path = format!("/hooks/goal:{goal}/ticket");
    assert_eq!(
        captured["run"],
        serde_json::Value::Null,
        "it listens: no run is made"
    );
    let secret = captured["secrets"][0]["secret"]
        .as_str()
        .expect("the secret, shown once")
        .to_string();
    assert_eq!(secret.len(), 64, "32 random bytes, hex");
    assert_eq!(captured["secrets"][0]["step"], "ticket");
    assert_eq!(captured["secrets"][0]["path"], path.as_str());

    // Said, with the way back — and that nothing hears it until a node runs.
    let said = bisa(
        dir.path(),
        &["new", "answer the outside again", "--workflow", &wf],
    );
    assert_ok(&said, "new --workflow, in words");
    let text = stdout(&said);
    for word in [
        "it listens",
        "will not be shown again",
        "X-Bisa-Token",
        "bisa workflow hook-secret goal",
        "ticket --rotate",
        "bisa node",
    ] {
        assert!(text.contains(word), "{word}: {text}");
    }
    assert!(!text.contains(&secret), "each goal's hook has its own");

    // No page shows it afterwards, and a second start mints nothing.
    for read in [
        vec!["status", goal.as_str()],
        vec!["--json", "status", goal.as_str()],
        vec!["workflow", "listeners", "--goal", goal.as_str()],
        vec!["workflow", "hook-secret", "goal", goal.as_str(), "ticket"],
        vec!["run", goal.as_str()],
    ] {
        let out = bisa(dir.path(), &read);
        assert_ok(&out, "a read");
        assert!(
            !stdout(&out).contains(&secret),
            "{read:?} leaked the secret"
        );
    }
    let again = json(&bisa(dir.path(), &["--json", "run", &goal]));
    assert_eq!(again["secrets"], serde_json::json!([]), "shown once");
    assert_eq!(
        again["listeners"][0]["public_hook"],
        serde_json::json!({"path": path, "has_secret": true})
    );

    // Rotated, by its goal.
    let rotated = json(&bisa(
        dir.path(),
        &[
            "--json",
            "workflow",
            "hook-secret",
            "goal",
            &goal,
            "ticket",
            "--rotate",
        ],
    ));
    assert_eq!(rotated["step"], "ticket");
    assert_eq!(rotated["path"], path.as_str());
    assert_eq!(rotated["secret"].as_str().map(str::len), Some(64));
    assert_ne!(rotated["secret"], secret.as_str());
}

/// `workflow run --start` is the one test door: naming an event start makes
/// a test run, begun there as if its event had happened with `--data`; the
/// run and every list of it say what it is.
#[test]
fn a_test_run_begins_at_an_event_start_as_if_its_event_had_happened() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());
    let wf = workflow_of(dir.path(), hook_workflow("Tickets", false));

    let started = bisa(
        dir.path(),
        &[
            "--json",
            "workflow",
            "run",
            &wf,
            "--start",
            "ticket",
            "--data",
            r#"{"subject": "the printer is on fire"}"#,
        ],
    );
    assert_ok(&started, "workflow run --start");
    let s = json(&started);
    assert_eq!(s["scope"], "workspace", "{s}");
    assert_eq!(s["status"], "waiting", "held on its wait: {s}");
    let run = s["run"].as_str().unwrap().to_string();
    let said = stderr(&started);
    assert!(
        said.contains(&format!("test run {run}")) && said.contains("at ticket"),
        "a test run is said to be one: {said}"
    );
    assert!(
        said.contains(&format!("bisa step release {run} hold")),
        "{said}"
    );

    // The sample was mapped onto the run's inputs, and the run, its page and
    // its row say what began it.
    let page = json(&bisa(dir.path(), &["--json", "status", &run]));
    assert_eq!(
        page["run"]["inputs"]["subject"], "the printer is on fire",
        "{page}"
    );
    assert_eq!(page["run"]["start"], "ticket", "{page}");
    assert_eq!(page["run"]["event"]["source"], "test", "{page}");
    assert_eq!(
        page["started_by"],
        serde_json::json!({"by": "test", "event": "hook"})
    );
    let text = stdout(&bisa(dir.path(), &["status", &run]));
    assert!(
        text.contains("started:   test, as if hook had happened"),
        "{text}"
    );
    let runs = json(&bisa(dir.path(), &["--json", "workflow", "runs", &wf]));
    assert_eq!(
        runs["runs"][0]["started_by"],
        serde_json::json!({"by": "test", "event": "hook"}),
        "{runs}"
    );
    let listed = stdout(&bisa(dir.path(), &["workflow", "runs", &wf]));
    assert!(listed.contains("test, as if hook had happened"), "{listed}");

    // A sample that does not fill the run's inputs begins nothing, and only
    // events begin this workflow: by hand there is nowhere to begin.
    let unfilled = bisa(dir.path(), &["workflow", "run", &wf, "--start", "ticket"]);
    assert!(!unfilled.status.success(), "{}", stdout(&unfilled));
    assert!(
        stderr(&unfilled).contains("subject"),
        "{}",
        stderr(&unfilled)
    );
    let by_hand = bisa(
        dir.path(),
        &["workflow", "run", &wf, "--input", "subject=mine"],
    );
    assert!(!by_hand.status.success(), "{}", stdout(&by_hand));
    assert!(
        stderr(&by_hand).contains("only its events begin it"),
        "{}",
        stderr(&by_hand)
    );
    let no_start = bisa(dir.path(), &["workflow", "run", &wf, "--data", "{}"]);
    assert_eq!(no_start.status.code(), Some(2), "{}", stderr(&no_start));
    assert_eq!(
        json(&bisa(dir.path(), &["--json", "workflow", "runs", &wf]))["runs"]
            .as_array()
            .unwrap()
            .len(),
        1,
        "a refusal makes no run"
    );

    // With a start by hand beside its event, a run by hand names it or not.
    let two = workflow_of(dir.path(), two_ways_in("Two ways in"));
    for begin in [vec![], vec!["--start", "by-hand"]] {
        let mut args = vec!["--json", "workflow", "run", two.as_str()];
        args.extend(begin.iter());
        let out = bisa(dir.path(), &args);
        assert_ok(&out, "workflow run by hand");
        assert!(
            !stderr(&out).contains("test run"),
            "{begin:?}: {}",
            stderr(&out)
        );
    }
    let runs = json(&bisa(dir.path(), &["--json", "workflow", "runs", &two]));
    let by: Vec<&serde_json::Value> = runs["runs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| &r["started_by"])
        .collect();
    assert_eq!(by.len(), 2, "{runs}");
    assert!(
        by.iter().all(|by| **by == serde_json::json!({"by": "you"})),
        "{runs}"
    );
    let refused = bisa(
        dir.path(),
        &[
            "workflow", "run", &two, "--start", "by-hand", "--data", "{}",
        ],
    );
    assert_eq!(refused.status.code(), Some(1), "{}", stderr(&refused));
    assert!(
        stderr(&refused).contains("begins by hand and reads no event"),
        "{}",
        stderr(&refused)
    );
}

/// The verbs of listening route through a running node, which is what hears:
/// a signal raised by hand starts the run of the workflow that listens for
/// it, and the run says which signal began it.
#[test]
fn the_listening_verbs_route_through_a_running_node() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());
    let wf = workflow_of(dir.path(), on_signal("report.ready"));
    let two = workflow_of(dir.path(), two_ways_in("Two ways in"));
    let goal = json(&bisa(
        dir.path(),
        &[
            "--json",
            "new",
            "answer every ticket",
            "--workflow",
            &two,
            "--no-start",
        ],
    ))["goal"]
        .as_str()
        .unwrap()
        .to_string();
    let node = spawn_daemon(dir.path());

    // Turned on through the node: its row is the node's, and nothing says
    // that nobody hears.
    let on = bisa(dir.path(), &["--json", "workflow", "on", &wf]);
    assert_ok(&on, "workflow on via node");
    let on = json(&on);
    assert!(on["workflow"]["listening"]["since"].is_u64(), "{on}");
    assert_eq!(on["workflow"]["runs"]["total"], 0, "the node's row: {on}");
    assert_eq!(on["listeners"].as_array().map(Vec::len), Some(1), "{on}");
    let said = bisa(dir.path(), &["workflow", "on", &wf]);
    assert_ok(&said, "workflow on via node, in words");
    assert!(
        stdout(&said).contains("is on") && !stdout(&said).contains("bisa node"),
        "{}",
        stdout(&said)
    );

    // A signal raised by hand is heard, and the node starts the run.
    let raised = bisa(
        dir.path(),
        &[
            "--json",
            "signal",
            "emit",
            "report.ready",
            "--data",
            r#"{"n": 1}"#,
        ],
    );
    assert_ok(&raised, "signal emit via node");
    let raised = json(&raised);
    let heard = raised["listeners"].as_array().expect("listeners").clone();
    assert_eq!(heard.len(), 1, "{raised}");
    let run = until(
        dir.path(),
        &["--json", "workflow", "runs", &wf],
        "the run the signal began",
        |v| v["runs"].as_array()?.first().cloned(),
    );
    assert_eq!(
        run["started_by"],
        serde_json::json!({"by": "event", "event": "signal", "detail": "report.ready"}),
        "{run}"
    );
    let id = run["id"].as_str().unwrap().to_string();
    let listed = stdout(&bisa(dir.path(), &["workflow", "runs", &wf]));
    assert!(listed.contains("by signal report.ready"), "{listed}");
    let page = bisa(dir.path(), &["status", &id]);
    assert_ok(&page, "status of the run via node");
    assert!(
        stdout(&page).contains("started:   by signal report.ready"),
        "{}",
        stdout(&page)
    );
    let signals = json(&bisa(dir.path(), &["--json", "signal", "list"]));
    assert!(
        signals["signals"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["id"] == heard[0]
                && s["listener"] == format!("workspace:{wf}/raised").as_str()),
        "{signals}"
    );
    let listeners = until(
        dir.path(),
        &["--json", "workflow", "listeners", &wf],
        "the listener's live run",
        |v| {
            let listener = v["listeners"].as_array()?.first()?.clone();
            (listener["live_runs"] == 1).then_some(listener)
        },
    );
    assert!(listeners["last_fired_at"].is_u64(), "{listeners}");

    // A test run, through the node.
    let test = bisa(
        dir.path(),
        &[
            "--json",
            "workflow",
            "run",
            &wf,
            "--start",
            "raised",
            "--data",
            r#"{"n": 2}"#,
        ],
    );
    assert_ok(&test, "workflow run --start via node");
    assert!(stderr(&test).contains("test run"), "{}", stderr(&test));
    assert_eq!(json(&test)["status"], "waiting");
    let runs = json(&bisa(dir.path(), &["--json", "workflow", "runs", &wf]));
    assert_eq!(
        runs["runs"][0]["started_by"],
        serde_json::json!({"by": "test", "event": "signal"}),
        "newest first: {runs}"
    );

    // A goal is armed through the node, says it listens, and is stopped.
    let begun = bisa(dir.path(), &["--json", "run", &goal]);
    assert_ok(&begun, "run arms the goal via node");
    let begun = json(&begun);
    assert_eq!(begun["status"], "waiting", "{begun}");
    assert_eq!(begun["run"], serde_json::Value::Null, "{begun}");
    assert_eq!(
        begun["listeners"][0]["listener"],
        format!("goal:{goal}/ticket")
    );
    let page = stdout(&bisa(dir.path(), &["status", &goal]));
    assert!(
        page.contains("listening since") && !page.contains("bisa node"),
        "{page}"
    );
    let stopped = json(&bisa(dir.path(), &["--json", "stop", &goal]));
    assert_eq!(stopped["listening_stopped"], true, "{stopped}");
    assert_eq!(
        json(&bisa(
            dir.path(),
            &["--json", "workflow", "listeners", "--goal", &goal]
        ))["listeners"],
        serde_json::json!([])
    );

    // Off, through the node; a refusal is the node's, in its words.
    let off = json(&bisa(dir.path(), &["--json", "workflow", "off", &wf]));
    assert_eq!(
        off["workflow"]["listening"],
        serde_json::Value::Null,
        "{off}"
    );
    let refused = bisa(dir.path(), &["workflow", "on", &two, "--input", "whom=x"]);
    assert!(!refused.status.success(), "{}", stdout(&refused));
    assert!(stderr(&refused).contains("whom"), "{}", stderr(&refused));

    drop(node);
}

/// The feature the events replaced is gone from the command line: its verb
/// is no verb, its flags are no flags, and no help says its word.
#[test]
fn the_retired_verb_and_its_flags_are_gone() {
    let dir = tempfile::tempdir().unwrap();
    // Spelt in two halves, so this source does not carry the word either.
    let retired = ["trig", "ger"].concat();
    let verb = bisa(dir.path(), &[retired.as_str(), "list"]);
    assert_eq!(verb.status.code(), Some(2), "{}", stderr(&verb));
    for flag in ["--topic", "--scope"] {
        let gone = bisa(dir.path(), &["signal", "emit", "report.ready", flag, "x"]);
        assert_eq!(gone.status.code(), Some(2), "{flag}: {}", stderr(&gone));
    }
    for help in [
        vec!["--help"],
        vec!["workflow", "--help"],
        vec!["signal", "--help"],
        vec!["run", "--help"],
        vec!["pulse", "--help"],
        vec!["tags", "--help"],
    ] {
        let out = bisa(dir.path(), &help);
        assert_ok(&out, "help");
        let text = stdout(&out).to_lowercase();
        assert!(!text.contains(&retired), "{help:?} still says it:\n{text}");
    }
    // The verbs that took its place say what they do.
    let help = stdout(&bisa(dir.path(), &["workflow", "--help"]));
    for verb in ["on", "off", "listeners", "hook-secret"] {
        assert!(
            help.lines()
                .any(|line| line.trim_start().starts_with(&format!("{verb} "))),
            "`workflow {verb}` is in the help:\n{help}"
        );
    }
    let help = stdout(&bisa(dir.path(), &["signal", "--help"]));
    for verb in ["emit", "list", "release"] {
        assert!(
            help.lines()
                .any(|line| line.trim_start().starts_with(&format!("{verb} "))),
            "`signal {verb}` is in the help:\n{help}"
        );
    }
}
