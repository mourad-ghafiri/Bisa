//! The Decision-Making Agent, through the binary: who answers and that it
//! is off, one question tried with nothing recorded, and a `judge` step that
//! takes the branch a sure answer names — and `otherwise` when the answer is
//! unsure, or is no answer at all. The provider is a harness on this node:
//! the scripted agent, asked with no tools and no door to the platform.

use super::sealed::{Sealed, AGENT_HARNESS};
use serde_json::{json, Value};

/// An answer in the contract's shape to the one question a `judge` step
/// asks (`branch`), as a model would write it.
fn picks(choice: &str, confidence: f64) -> String {
    let other = if choice == "page" { "later" } else { "page" };
    let sure = (1.0 + confidence) / 2.0;
    json!({ "answers": { "branch": {
        "type": "choice",
        "choice": choice,
        "probabilities": { choice: sure, other: 1.0 - sure },
        "confidence": confidence,
    } } })
    .to_string()
}

/// What the model says, by what it is asked about.
fn judge() -> Value {
    json!({
        "turns": [
            { "scope": "none", "when": "payments are failing",
              "say": [picks("page", 0.95)] },
            { "scope": "none", "when": "a typo on the pricing page",
              "say": [picks("later", 0.2)] },
            { "scope": "none", "when": "nobody can read this",
              "say": ["I would rather not say."] },
            { "scope": "none", "when": "Does this convey urgency?",
              "say": [json!({ "answers": { "q": { "type": "noul", "noul": 0.97 } } }).to_string()] },
        ],
    })
}

/// A ticket routed by a judgement: paged, left for later, or — when nobody
/// is sure — put to a person.
fn triage() -> Value {
    json!({
        "name": "Triage",
        "inputs": [{ "name": "ticket", "label": "Ticket", "kind": "text", "required": true }],
        "steps": [
            { "id": "route", "name": "Route", "kind": "judge",
              "state": "{inputs.ticket}",
              "instructions": "Is this ticket urgent enough to page someone right now?",
              "options": [
                  { "branch": "page", "meaning": "an outage, data loss, or a payment failure affecting customers now" },
                  { "branch": "later", "meaning": "a real bug with no immediate harm" },
              ],
              "otherwise": "ask",
              "min_confidence": 0.8,
              "then": [
                  { "to": "paged", "branch": "page" },
                  { "to": "queued", "branch": "later" },
                  { "to": "asked", "branch": "ask" },
              ] },
            { "id": "paged", "name": "Page", "kind": "notify", "template": "paged: {inputs.ticket}" },
            { "id": "queued", "name": "Queue", "kind": "notify", "template": "queued: {inputs.ticket}" },
            { "id": "asked", "name": "Ask", "kind": "notify", "template": "a person decides: {inputs.ticket}" },
        ],
    })
}

/// The scripted agent as the harness that judges, with its own default model.
fn judged_by_the_scripted_agent(ws: &Sealed) {
    for (key, value) in [
        ("decisions.harness.id", AGENT_HARNESS),
        ("decisions.harness.model", ""),
    ] {
        ws.ok(&["settings", "set", "machine", key, value]);
    }
}

/// One run of the triage on `ticket`, followed to its end.
fn routed(ws: &Sealed, workflow: &str, ticket: &str) -> Value {
    let started = ws.json(&[
        "workflow",
        "run",
        workflow,
        "--input",
        &format!("ticket={ticket}"),
    ]);
    assert_eq!(started["status"], "done", "{started}");
    let run = started["run"].as_str().expect("the run");
    ws.json(&["status", run])["run"]["steps"].clone()
}

#[test]
fn a_judge_step_takes_the_branch_a_sure_answer_names_and_otherwise_when_nobody_is_sure() {
    let mut ws = Sealed::with_script(&judge());
    judged_by_the_scripted_agent(&ws);
    let file = ws.file("triage.json", &triage().to_string());
    let made = ws.json(&["workflow", "new", "--from", &file.to_string_lossy()]);
    assert_eq!(made["problems"], json!([]), "{made}");
    let workflow = made["workflow"]["id"].as_str().expect("its id").to_string();
    ws.start();

    // Who answers, and that nothing asks it: off until somebody says so.
    let status = ws.json(&["decisions", "status"]);
    assert_eq!(status["enabled"], false, "{status}");
    assert_eq!(status["provider"], "harness");
    assert_eq!(
        status["calibrated"], false,
        "a generative model's own estimate"
    );
    assert_eq!(status["ready"], true, "{status}");
    let judge_point = status["points"]
        .as_array()
        .and_then(|points| points.iter().find(|p| p["point"] == "workflow.judge"))
        .unwrap_or_else(|| panic!("the judge's point: {status}"));
    assert_eq!(
        judge_point["selected_explicitly"], true,
        "naming a judge step is switching it on there"
    );

    // One question tried: answered, and nothing recorded.
    let tried = ws.json(&[
        "decisions",
        "try",
        "--state",
        "Help! My payouts have been failing for 3 days.",
        "--noul",
        "Does this convey urgency?",
    ]);
    assert_eq!(
        tried["answers"]["q"],
        json!({ "type": "noul", "noul": 0.97 }),
        "{tried}"
    );
    assert_eq!(ws.json(&["decisions", "list"])["judgements"], json!([]));

    // Sure: the branch it named.
    let sure = routed(&ws, &workflow, "payments are failing for every customer");
    assert_eq!(
        sure["route"]["state"],
        json!({ "state": "done", "branches": ["page"] }),
        "{sure}"
    );
    assert_eq!(sure["route"]["output"]["choice"], "page");
    assert_eq!(sure["route"]["output"]["judged"], true);
    assert_eq!(sure["paged"]["state"]["state"], "done");
    assert_eq!(sure["queued"]["state"]["state"], "skipped");
    assert_eq!(sure["asked"]["state"]["state"], "skipped");

    // Unsure: recorded, not acted on — `otherwise`.
    let unsure = routed(&ws, &workflow, "a typo on the pricing page");
    assert_eq!(
        unsure["route"]["state"],
        json!({ "state": "done", "branches": ["ask"] }),
        "{unsure}"
    );
    assert_eq!(unsure["route"]["output"]["judged"], false);
    assert_eq!(unsure["route"]["output"]["choice"], Value::Null);
    assert_eq!(unsure["asked"]["state"]["state"], "done");
    assert_eq!(
        unsure["queued"]["state"]["state"], "skipped",
        "its pick was not sure enough"
    );

    // No answer in the contract's shape: the step does not fail — `otherwise`.
    let unread = routed(&ws, &workflow, "nobody can read this");
    assert_eq!(
        unread["route"]["state"]["branches"],
        json!(["ask"]),
        "{unread}"
    );
    assert_eq!(unread["asked"]["state"]["state"], "done");

    // Every judgement is kept with what became of it, newest first.
    let kept = ws.json(&["decisions", "list"]);
    let outcomes: Vec<(&str, &str)> = kept["judgements"]
        .as_array()
        .expect("the judgements")
        .iter()
        .map(|row| {
            (
                row["judgement"]["point"].as_str().unwrap_or_default(),
                row["judgement"]["outcome"].as_str().unwrap_or_default(),
            )
        })
        .collect();
    assert_eq!(
        outcomes,
        vec![
            ("workflow.judge", "failed"),
            ("workflow.judge", "unsure"),
            ("workflow.judge", "applied"),
        ],
        "{kept}"
    );

    // The model was asked with no tools and no door to the platform, and
    // what it was given to judge was data: the ticket, under STATE.
    let asked = ws.recorded("session_new");
    assert!(!asked.is_empty());
    for session in &asked {
        assert_eq!(session["scope"], "none", "{session}");
        assert_eq!(session["servers"], json!([]), "{session}");
    }
    let prompts = ws.recorded("prompt");
    assert!(
        prompts.iter().any(|p| p["text"].as_str().is_some_and(|t| {
            t.contains("STATE:\npayments are failing for every customer")
                && t.contains("option `page`")
                && t.contains("option `later`")
        })),
        "{prompts:#?}"
    );
    ws.stop();
}
