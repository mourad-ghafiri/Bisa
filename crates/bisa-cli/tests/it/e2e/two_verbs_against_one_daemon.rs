//! Verbs at once. With a daemon running, a dozen verbs begun in the same
//! moment — half of them writing, half reading — are every one answered by
//! the node, none refused as a second engine, and what they wrote is whole:
//! one goal per capture, each readable, none half-made. With no daemon, two
//! verbs begun in the same moment each start an engine of their own over
//! the one workspace: the lock lets one through, the other is told so in
//! words — never a second scheduler on the same work, never a panic — and
//! the workspace reads whole afterwards.

use super::sealed::Sealed;
use std::process::ExitStatus;

/// The sentence the command line says when the workspace's engine lock is
/// somebody else's ([`bisa_engine::EngineError::Locked`], through the CLI).
const LOCKED: &str = "another engine holds this workspace";

/// Every begun verb waited for, by name: its exit and what it said.
fn ended(
    ws: &Sealed,
    begun: Vec<(String, super::sealed::Begun)>,
) -> Vec<(String, ExitStatus, String, String)> {
    begun
        .into_iter()
        .map(|(name, mut verb)| {
            let status = ws.until(&format!("{name} to end"), || verb.ended());
            let (out, err) = ws.said_by(&name);
            (name, status, out, err)
        })
        .collect()
}

#[test]
fn a_dozen_verbs_at_once_are_every_one_answered_by_the_node_and_what_they_wrote_is_whole() {
    let mut ws = Sealed::bare();
    ws.start();

    let mut begun = Vec::new();
    for n in 0..6 {
        let statement = format!("goal {n} of the same moment");
        begun.push((
            format!("writer-{n}"),
            ws.begin(
                &format!("writer-{n}"),
                &["--json", "new", &statement, "--mode", "manual"],
            ),
        ));
        begun.push((
            format!("reader-{n}"),
            ws.begin(&format!("reader-{n}"), &["--json", "search"]),
        ));
    }
    let mut captured = Vec::new();
    for (name, status, out, err) in ended(&ws, begun) {
        assert!(status.success(), "{name} was not answered: {err}\n{out}");
        assert!(
            !err.contains(LOCKED),
            "{name} took the node for a second engine: {err}"
        );
        let said: serde_json::Value = serde_json::from_str(&out)
            .unwrap_or_else(|e| panic!("{name} printed no JSON ({e}): {out}"));
        if name.starts_with("writer") {
            captured.push(
                said["goal"]
                    .as_str()
                    .expect("the goal captured")
                    .to_string(),
            );
        } else {
            assert!(said["matches"].is_array(), "{name}: {said}");
        }
    }
    captured.sort();
    captured.dedup();
    assert_eq!(
        captured.len(),
        6,
        "six captures, six goals, no two the same"
    );

    let listed = ws.json(&["search"]);
    let mut ids: Vec<String> = listed["matches"]
        .as_array()
        .expect("the goals")
        .iter()
        .map(|g| g["goal"].as_str().expect("an id").to_string())
        .collect();
    ids.sort();
    assert_eq!(
        ids, captured,
        "every capture is a goal the list has: {listed}"
    );
    for goal in &captured {
        assert_eq!(
            ws.json(&["status", goal])["status"],
            "draft",
            "{goal} reads whole"
        );
    }
    ws.stop();
}

#[test]
fn two_verbs_at_once_with_no_daemon_share_one_workspace_and_the_lock_says_so_in_words() {
    let ws = Sealed::bare();
    // Twice, so the race is met both ways at least once in the run.
    let mut winners = 0;
    let mut told = 0;
    for round in 0..3 {
        let begun = vec![
            (
                format!("first-{round}"),
                ws.begin(
                    &format!("first-{round}"),
                    &[
                        "--json",
                        "new",
                        &format!("first of round {round}"),
                        "--mode",
                        "manual",
                    ],
                ),
            ),
            (
                format!("second-{round}"),
                ws.begin(
                    &format!("second-{round}"),
                    &[
                        "--json",
                        "new",
                        &format!("second of round {round}"),
                        "--mode",
                        "manual",
                    ],
                ),
            ),
        ];
        for (name, status, out, err) in ended(&ws, begun) {
            if status.success() {
                winners += 1;
                let said: serde_json::Value = serde_json::from_str(&out)
                    .unwrap_or_else(|e| panic!("{name} printed no JSON ({e}): {out}"));
                assert!(said["goal"].is_string(), "{name}: {said}");
            } else {
                told += 1;
                assert!(
                    err.contains(LOCKED),
                    "{name} was refused for another reason: {err}"
                );
                assert!(out.trim().is_empty(), "{name} printed on a refusal: {out}");
                assert!(!err.contains("panicked"), "{name} panicked: {err}");
            }
        }
    }
    assert!(
        winners >= 3,
        "at least one verb of each round got the workspace"
    );
    // Whatever the races came to, the workspace reads whole: as many goals
    // as verbs that were let through, each a draft.
    let listed = ws.json(&["search"]);
    let goals = listed["matches"].as_array().expect("the goals");
    assert_eq!(goals.len(), winners, "{listed}");
    for goal in goals {
        assert_eq!(goal["status"], "draft", "{goal}");
    }
    eprintln!("{winners} let through, {told} told the lock's word");
}
