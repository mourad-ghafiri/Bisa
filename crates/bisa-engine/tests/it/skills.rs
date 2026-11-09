//! `skill_read` at the intake: an installed skill read whole — its body and
//! the names of its reference sections — or one section by slug or title,
//! and the sentences a session reads when it names a skill or a section that
//! is not there. Every call goes through the engine's own socket, the way a
//! harness session's tool does.

use crate::common::{self, intake_roundtrip};
use bisa_core::{RespondPolicy, SkillId, Tags};
use bisa_engine::Engine;
use bisa_store::{NewAgent, NewSkill};
use serde_json::{json, Value};

/// A skill with a body and two reference sections — the shape the UI/UX
/// Design skill takes.
const DESIGN: &str = "# UI/UX Design\n\nLook first.\n\n## Reference: Craft floor\n\nContrast 4.5:1.\n\n## Reference: Modes in depth\n\nOperate · Persuade.\n";

fn engine(dir: &tempfile::TempDir) -> Engine {
    common::engine_with(dir, vec![common::yielding("mock", json!({"ok": true}))])
}

async fn op(engine: &Engine, req: Value) -> Value {
    intake_roundtrip(engine.socket_path(), req).await
}

/// The refusal's words, joined.
fn refused(reply: &Value) -> String {
    assert_eq!(reply["ok"], false, "a refusal: {reply}");
    reply["errors"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>()
        .join("; ")
}

fn skill(engine: &Engine, id: &str, markdown: &str) {
    engine
        .inner()
        .ws
        .create_skill(NewSkill {
            id: SkillId::new(id).unwrap(),
            name: "UI/UX Design".into(),
            description: "Use when work changes a screen.".into(),
            tags: Tags::default(),
            markdown: markdown.into(),
        })
        .unwrap();
}

#[tokio::test]
async fn a_skill_is_read_whole_with_its_index_or_one_section_by_slug_or_title() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    skill(&engine, "ui-ux-design", DESIGN);
    let whole = op(
        &engine,
        json!({"op": "skill_read", "skill": "ui-ux-design", "agent": "general-agent"}),
    )
    .await;
    assert_eq!(whole["ok"], true, "{whole}");
    assert_eq!(whole["skill"], "ui-ux-design");
    assert_eq!(whole["name"], "UI/UX Design");
    assert_eq!(whole["description"], "Use when work changes a screen.");
    assert_eq!(
        whole["body"], "# UI/UX Design\n\nLook first.",
        "the body alone: a section is read on demand"
    );
    assert_eq!(
        whole["sections"],
        json!([
            {"slug": "craft-floor", "title": "Craft floor"},
            {"slug": "modes-in-depth", "title": "Modes in depth"}
        ])
    );
    let by_slug = op(
        &engine,
        json!({"op": "skill_read", "skill": "ui-ux-design", "section": "craft-floor"}),
    )
    .await;
    assert_eq!(
        by_slug["section"],
        json!({"slug": "craft-floor", "title": "Craft floor", "markdown": "Contrast 4.5:1."}),
        "{by_slug}"
    );
    let by_title = op(
        &engine,
        json!({"op": "skill_read", "skill": "ui-ux-design", "section": " Modes In Depth "}),
    )
    .await;
    assert_eq!(
        by_title["section"]["slug"], "modes-in-depth",
        "a title, any case, blanks aside: {by_title}"
    );
    engine.shutdown().await;
}

#[tokio::test]
async fn an_unknown_section_lists_the_sections_and_an_unknown_skill_names_what_the_agent_carries() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    skill(&engine, "ui-ux-design", DESIGN);
    skill(&engine, "plain", "# Plain\n\nNo sections here.\n");
    let scout = engine
        .inner()
        .ws
        .add_agent(NewAgent {
            name: "Scout".into(),
            photo: None,
            description: None,
            system_prompt: "You are Scout.".into(),
            harness: "mock".into(),
            models: Default::default(),
            skills: vec![SkillId::new("ui-ux-design").unwrap()],
            mcps: vec![],
            tags: Default::default(),
            respond: RespondPolicy::OwnerOnly,
            decision_making: false,
        })
        .unwrap();
    let unknown_section = op(
        &engine,
        json!({"op": "skill_read", "skill": "ui-ux-design", "section": "motion"}),
    )
    .await;
    assert_eq!(
        refused(&unknown_section),
        "skill `ui-ux-design` has no section `motion`; its sections are: craft-floor · modes-in-depth"
    );
    let no_sections = op(
        &engine,
        json!({"op": "skill_read", "skill": "plain", "section": "anything"}),
    )
    .await;
    assert_eq!(
        refused(&no_sections),
        "skill `plain` has no reference sections — read it whole, naming no section"
    );
    let carrier = op(
        &engine,
        json!({"op": "skill_read", "skill": "nope", "agent": scout.id.to_string()}),
    )
    .await;
    assert_eq!(
        refused(&carrier),
        "no skill `nope` is installed in this workspace; this agent carries: ui-ux-design"
    );
    let general = op(
        &engine,
        json!({"op": "skill_read", "skill": "nope", "agent": "general-agent"}),
    )
    .await;
    assert_eq!(
        refused(&general),
        "no skill `nope` is installed in this workspace; the installed skills are: plain, ui-ux-design",
        "an agent carrying none is told what is installed, by id"
    );
    let not_an_id = op(&engine, json!({"op": "skill_read", "skill": "Not An Id!"})).await;
    assert!(
        refused(&not_an_id).contains("is not a skill id"),
        "{not_an_id}"
    );
    engine.shutdown().await;
}

#[tokio::test]
async fn a_workspace_with_no_skill_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let reply = op(
        &engine,
        json!({"op": "skill_read", "skill": "nope", "agent": "general-agent"}),
    )
    .await;
    assert_eq!(
        refused(&reply),
        "no skill `nope` is installed in this workspace; no skill is installed here"
    );
    engine.shutdown().await;
}
