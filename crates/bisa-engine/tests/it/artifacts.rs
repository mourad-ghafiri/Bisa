//! Artifacts through the intake door: what a session may publish from where,
//! how a file becomes a titled, kinded artifact with its source, and how the
//! next turn's transcript names it.

use crate::common::{engine_with, intake_roundtrip};
use bisa_core::{AgentId, ArtifactKind, FileScope, MessageBody, RespondPolicy};
use bisa_engine::{Engine, SubmitRequest};
use bisa_harness::mock::MockAdapter;
use bisa_store::{NewAgent, NewProject, PostOrigin, Workspace, WorkstreamFilter};
use serde_json::{json, Value};
use std::path::Path;
use std::time::Duration;

fn general() -> String {
    AgentId::GENERAL.to_string()
}

fn general_id() -> AgentId {
    AgentId::new(AgentId::GENERAL).unwrap()
}

fn write(dir: &Path, rel: &str, bytes: &[u8]) -> std::path::PathBuf {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, bytes).unwrap();
    path
}

async fn post(engine: &Engine, mut req: Value) -> Value {
    req["op"] = json!("post_message");
    intake_roundtrip(engine.socket_path(), req).await
}

fn artifacts_of(ws: &Workspace, id: &str) -> Vec<bisa_store::MessageArtifact> {
    ws.get_message(id).unwrap().expect("the message").artifacts
}

#[tokio::test(flavor = "multi_thread")]
async fn an_agent_posts_an_artifact_from_its_scratch_and_the_title_defaults_to_the_name() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![]);
    let ws = engine.workspace();
    let scratch = ws.paths().agent(&general_id()).scratch();
    write(
        &scratch,
        "chart.svg",
        b"<svg xmlns='http://www.w3.org/2000/svg'/>",
    );

    let reply = post(
        &engine,
        json!({"scope": "general", "content": "the chart", "agent": general(),
               "artifacts": [{"path": "chart.svg"}]}),
    )
    .await;
    assert_eq!(reply["ok"], json!(true), "{reply}");
    let arts = artifacts_of(ws, reply["message"].as_str().unwrap());
    assert_eq!(arts.len(), 1);
    assert_eq!(arts[0].artifact.title, "chart");
    assert_eq!(arts[0].artifact.kind, ArtifactKind::Svg);
    assert_eq!(arts[0].artifact.mime, "image/svg+xml");
    assert!(arts[0].present);
    assert!(
        arts[0].artifact.source.is_none(),
        "a scratch folder is nobody's root but the agent's"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_conversation_about_a_checkout_may_post_from_it_and_the_source_is_recorded() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![]);
    let ws = engine.workspace();
    let project = ws
        .create_project(NewProject::managed("web").unwrap())
        .unwrap();
    let primary = ws
        .list_workstreams(WorkstreamFilter::Project(project.id))
        .unwrap()
        .into_iter()
        .next()
        .expect("the primary workstream");
    let checkout = ws.checkout_in(&project, &primary);
    let page = write(&checkout, "out/report.html", b"<h1>Report</h1>");
    let conversation = ws
        .create_conversation(bisa_store::NewConversation {
            origin: bisa_core::ConversationOrigin::Workstream {
                id: primary.id,
                project: project.id,
            },
            title: None,
            mode: bisa_core::ConversationMode::Auto,
        })
        .unwrap();

    // A relative path, and the same file by its absolute path.
    for path in ["out/report.html".to_string(), page.display().to_string()] {
        let reply = post(
            &engine,
            json!({"scope": conversation.id.to_string(), "content": "", "agent": general(),
                   "artifacts": [{"path": path, "title": "Weekly report"}]}),
        )
        .await;
        assert_eq!(reply["ok"], json!(true), "{path}: {reply}");
        let arts = artifacts_of(ws, reply["message"].as_str().unwrap());
        assert_eq!(arts[0].artifact.title, "Weekly report");
        assert_eq!(arts[0].artifact.kind, ArtifactKind::Html);
        let source = arts[0]
            .artifact
            .source
            .as_ref()
            .expect("a checkout is a root");
        assert_eq!(source.scope, FileScope::Workstream);
        assert_eq!(source.id, primary.id.to_string());
        assert_eq!(source.path.as_str(), "out/report.html");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_goal_session_may_post_from_the_goal_scratch() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![]);
    let ws = engine.workspace();
    let goal = engine
        .submit_goal(SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            ..SubmitRequest::captured("draw the plan")
        })
        .unwrap()
        .id;
    write(
        &ws.paths().goal(goal).scratch(),
        "plan.mmd",
        b"graph TD; a-->b",
    );

    let reply = post(
        &engine,
        json!({"scope": "general", "content": "the plan", "agent": general(),
               "goal": goal.to_string(), "artifacts": [{"path": "plan.mmd", "title": "Plan"}]}),
    )
    .await;
    assert_eq!(reply["ok"], json!(true), "{reply}");
    let arts = artifacts_of(ws, reply["message"].as_str().unwrap());
    assert_eq!(arts[0].artifact.kind, ArtifactKind::Diagram);
    let source = arts[0].artifact.source.as_ref().unwrap();
    assert_eq!(source.scope, FileScope::Goal);
    assert_eq!(source.id, goal.to_string());
    assert_eq!(source.path.as_str(), "plan.mmd");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_artifact_outside_every_root_is_refused_and_names_the_boundaries() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![]);
    let ws = engine.workspace();
    let elsewhere = write(dir.path(), "elsewhere/secret.html", b"<p>no</p>");
    let scratch = ws.paths().agent(&general_id()).scratch();

    for path in [
        elsewhere.display().to_string(),
        "../elsewhere/secret.html".to_string(),
        "missing.html".to_string(),
    ] {
        let reply = post(
            &engine,
            json!({"scope": "general", "content": "x", "agent": general(),
                   "artifacts": [{"path": path}]}),
        )
        .await;
        assert_eq!(reply["ok"], json!(false), "{path}: {reply}");
        let words =
            reply["error"].as_str().unwrap_or_default().to_string() + &reply["errors"].to_string();
        assert!(
            words.contains(&scratch.display().to_string())
                || words.contains("may publish from")
                || words.contains("leaves"),
            "{path}: the refusal names the boundary: {reply}"
        );
    }
    // Nothing reached the store.
    assert!(ws.list_artifacts("general", 10).unwrap().is_empty());

    // A person's post through the socket cannot publish an agent's file.
    write(&scratch, "chart.png", b"\x89PNG");
    let reply = post(
        &engine,
        json!({"scope": "general", "content": "x", "artifacts": [{"path": "chart.png"}]}),
    )
    .await;
    assert_eq!(reply["ok"], json!(false));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_ninth_artifact_is_refused_before_anything_is_stored() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![]);
    let ws = engine.workspace();
    let scratch = ws.paths().agent(&general_id()).scratch();
    let specs: Vec<Value> = (0..9)
        .map(|i| {
            write(
                &scratch,
                &format!("f{i}.txt"),
                format!("file {i}").as_bytes(),
            );
            json!({"path": format!("f{i}.txt")})
        })
        .collect();
    let reply = post(
        &engine,
        json!({"scope": "general", "content": "all of them", "agent": general(), "artifacts": specs}),
    )
    .await;
    assert_eq!(reply["ok"], json!(false), "{reply}");
    assert!(reply.to_string().contains("at most 8"), "{reply}");
    assert!(ws.list_artifacts("general", 10).unwrap().is_empty());
    assert!(
        !ws.paths().attachments_dir().is_dir()
            || std::fs::read_dir(ws.paths().attachments_dir())
                .unwrap()
                .next()
                .is_none(),
        "nothing was read into the store"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_transcript_lists_artifacts_with_title_kind_and_path_and_the_framing_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let adapter = MockAdapter {
        id: "chat-harness".into(),
        ..Default::default()
    };
    let engine = engine_with(&dir, vec![adapter]);
    let ws = engine.workspace();
    let agent = ws
        .add_agent(NewAgent {
            name: "Scout".into(),
            photo: None,
            description: None,
            system_prompt: "You are Scout.".into(),
            harness: "chat-harness".into(),
            models: Default::default(),
            skills: vec![],
            mcps: vec![],
            tags: Default::default(),
            respond: RespondPolicy::OwnerOnly,
            decision_making: false,
        })
        .unwrap();
    let dm = ws.open_dm(std::slice::from_ref(&agent.pubkey)).unwrap();
    let file = ws
        .put_attachment(b"a,b\n1,2\n", "numbers.csv", "text/csv")
        .unwrap();
    let artifact = bisa_core::ArtifactRef::from_attachment(file, Some("Numbers".into()), None);
    let blob = ws.attachment_path(&artifact.sha256).unwrap();
    ws.post_message(
        dm.id.as_str(),
        MessageBody::Post {
            text: "look at these".into(),
            context: vec![],
            artifacts: vec![artifact],
            thinking: None,
            said: None,
        },
        None,
        &[],
        &[],
        None,
        PostOrigin::Asked,
    )
    .unwrap();

    // The mock echoes its prompt, so the reply is the transcript it saw.
    let reply = {
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        loop {
            let found = ws
                .messages(dm.id.as_str(), None, 20)
                .unwrap()
                .into_iter()
                .find(|m| m.author == agent.pubkey.as_hex());
            if let Some(m) = found {
                break m;
            }
            assert!(std::time::Instant::now() < deadline, "no reply");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    };
    let expected = format!(
        "[artifact] Numbers · sheet · {} (numbers.csv)",
        blob.display()
    );
    assert!(
        reply.content.contains(&expected),
        "the transcript names the artifact: {:?}",
        reply.content
    );
    assert!(
        reply.content.contains("post it as an artifact"),
        "the framing tells the agent how: {:?}",
        reply.content
    );
}
