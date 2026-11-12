//! The engine's facts reach the activity feed before their frames do, and a
//! frame that is not a fact never does.

use crate::common::engine_with;
use bisa_core::ActivityConcept;
use bisa_engine::SubmitRequest;

#[tokio::test(flavor = "multi_thread")]
async fn a_goal_made_is_in_the_feed_under_workspace_and_its_story_under_goals() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![]);
    let ws = engine.workspace();
    let goal = engine
        .submit_goal(SubmitRequest::captured("ship the feed"))
        .unwrap();

    let workspace = ws
        .activity_page(Some(ActivityConcept::Workspace), None, 10)
        .unwrap();
    let made = workspace
        .iter()
        .find(|r| r.kind == "goal_created")
        .unwrap_or_else(|| panic!("a goal made is the workspace's shape changing: {workspace:?}"));
    assert_eq!(made.source_kind, "goal");
    assert_eq!(made.source_id, goal.id.to_string());
    let event: serde_json::Value = serde_json::from_str(&made.event).unwrap();
    assert_eq!(event["type"], "goal_created");
    assert_eq!(
        event["origin"]["origin"], "captured",
        "the payload rides verbatim: the origin is a tagged object, as a goal's is"
    );

    // Nothing the roster owns is in the feed.
    let all = ws.activity_page(None, None, 100).unwrap();
    assert!(
        all.iter().all(|r| !matches!(
            r.kind.as_str(),
            "session" | "session_state" | "agent_thinking" | "file_changed"
        )),
        "{all:?}"
    );
    // The truth file is there for a rebuild.
    assert!(
        dir.path()
            .join("activity")
            .read_dir()
            .map(|mut d| d.next().is_some())
            .unwrap_or(false),
        "the engine's facts are logged under activity/"
    );
}

/// What a real run leaves in the feed, drawn: every row's kind, side and
/// whether it is an echo.
fn drawn(engine: &bisa_engine::Engine) -> Vec<(String, bool, serde_json::Value)> {
    let ws = engine.workspace();
    let records = ws.activity_page(None, None, 200).unwrap();
    bisa_engine::feed::page(ws, records)
        .into_iter()
        .rev()
        .map(|row| {
            (
                row.kind.clone(),
                row.echo_of.is_some(),
                serde_json::to_value(&row.event).unwrap(),
            )
        })
        .collect()
}

/// One happening is said twice — by the journal and by the engine — and
/// drawn once: a real run's engine rows are each the echo of the journal's
/// row of the same happening, the journal's rows are nobody's echo, and a
/// fact only one side says stands as itself.
#[tokio::test(flavor = "multi_thread")]
async fn a_runs_engine_facts_are_the_echoes_of_its_journals() {
    use crate::common::*;
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![bisa_harness::mock::MockAdapter::default()]);
    let mut rx = engine.events();
    let (goal, run) = run_on(
        &engine,
        "a run",
        new_workflow("one step", vec![agent_step("build", "mock")]),
    );
    wait_for(&mut rx, "the run's end", |e| {
        matches!(&e.payload, bisa_engine::EnginePayload::RunFinished { .. })
    })
    .await;
    let rows = drawn(&engine);
    let of = |kind: &str| -> Vec<bool> {
        rows.iter()
            .filter(|(k, _, _)| k == kind)
            .map(|(_, echo, _)| *echo)
            .collect()
    };
    for twin in ["run_started", "run_finished", "step_changed"] {
        assert!(
            !of(twin).is_empty() && of(twin).iter().all(|echo| *echo),
            "every {twin} repeats a journal fact: {rows:#?}"
        );
    }
    for journal in ["run", "step", "note", "claim"] {
        assert!(
            !of(journal).is_empty() && of(journal).iter().all(|echo| !*echo),
            "a journal fact is nobody's echo: {journal}"
        );
    }
    for alone in ["goal_created", "scheduled", "workflow_changed"] {
        assert_eq!(of(alone), [false], "{alone} is said once, by the engine");
    }
    // Drawn: the run began and ended once each, whatever was recorded.
    let said = |fact: &str| {
        rows.iter()
            .filter(|(kind, echo, event)| kind == "run" && !*echo && event["event"]["fact"] == fact)
            .count()
    };
    assert_eq!((said("started"), said("finished")), (1, 1));
    assert_eq!(event_run(&rows), run.id.to_string());
    let _ = goal;
    engine.shutdown().await;
}

fn event_run(rows: &[(String, bool, serde_json::Value)]) -> String {
    rows.iter()
        .find(|(kind, _, _)| kind == "run_finished")
        .and_then(|(_, _, event)| event["run"].as_str().map(str::to_string))
        .unwrap_or_default()
}

/// A frame never goes out before its row: whoever reads the feed on a frame
/// finds the fact the frame announced. Every frame of a real run that is a
/// fact of the feed is checked as it is heard.
#[tokio::test(flavor = "multi_thread")]
async fn a_frame_never_goes_out_before_its_row() {
    use crate::common::*;
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![bisa_harness::mock::MockAdapter::default()]);
    let mut rx = engine.events();
    run_on(
        &engine,
        "a run",
        new_workflow("one step", vec![agent_step("build", "mock")]),
    );
    let mut facts = 0;
    loop {
        let frame = tokio::time::timeout(std::time::Duration::from_secs(20), rx.recv())
            .await
            .expect("the run ends")
            .expect("the bus is open");
        if let Some(fact) = bisa_engine::activity::fact_of(&frame) {
            let held = engine
                .workspace()
                .activity_by_kinds(&[fact.kind.as_str()], 50)
                .unwrap();
            assert!(
                held.iter().any(|row| {
                    serde_json::from_str::<serde_json::Value>(&row.event).unwrap() == fact.event
                }),
                "the frame {} was heard and its row is not in the feed",
                fact.kind
            );
            facts += 1;
        }
        if matches!(
            &frame.payload,
            bisa_engine::EnginePayload::RunFinished { .. }
        ) {
            break;
        }
    }
    assert!(facts >= 4, "a run's start, its step and its end: {facts}");
    engine.shutdown().await;
}
