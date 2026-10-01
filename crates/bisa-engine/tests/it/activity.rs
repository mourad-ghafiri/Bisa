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
