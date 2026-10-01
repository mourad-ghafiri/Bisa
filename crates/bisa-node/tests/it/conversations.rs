//! The conversation routes over HTTP: started with an origin, listed and
//! narrowed, titled and put away, spoken into, read mid-turn, deleted — and
//! the workstream thread that is no longer a route.

use bisa_engine::{Engine, EngineConfig};
use bisa_harness::mock::MockAdapter;
use bisa_harness::HarnessCatalog;
use bisa_node::{serve, NodeConfig};
use bisa_store::{FileKeyStore, NewProject, Workspace};
use http_body_util::{BodyExt as _, Full};
use hyper::body::Bytes;
use hyper::Request;
use hyper_util::rt::TokioIo;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::UnixStream;

const TOKEN: &str = "test-token-0123456789abcdef";

struct Node {
    socket: PathBuf,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    server: Option<tokio::task::JoinHandle<()>>,
    /// The project made before the node started, and its primary's id.
    project: String,
    goal: String,
    _dir: tempfile::TempDir,
}

impl Node {
    async fn start() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let data = dir.path().to_path_buf();
        let ws = Workspace::open_with_keystore(
            &data,
            Box::new(FileKeyStore::new(
                bisa_store::Paths::new(&data).identity_dir(),
            )),
        )
        .expect("workspace");
        let mut general = ws.get_agent(&bisa_core::AgentId::general()).unwrap();
        general.harness = "mock".into();
        ws.update_agent(general).unwrap();
        let project = ws
            .create_project(NewProject::managed("web-app").unwrap())
            .unwrap();
        let primary = ws.primary_workstream(project.id).unwrap();
        std::fs::create_dir_all(ws.checkout_in(&project, &primary)).unwrap();
        let goal = ws
            .create_goal(bisa_store::NewGoal::captured("a goal to talk about"))
            .unwrap();
        let mut catalog = HarnessCatalog::new();
        catalog.register(Arc::new(MockAdapter::default()));
        let engine = Engine::start(
            ws,
            catalog,
            EngineConfig {
                design_enabled: false,
                events_enabled: false,
                ..Default::default()
            },
        )
        .expect("engine");
        let socket = bisa_store::Paths::new(&data).node_socket();
        let (stop, stop_rx) = tokio::sync::oneshot::channel::<()>();
        let cfg = NodeConfig {
            socket: socket.clone(),
            http: None,
            data_dir: data.clone(),
            collab: None,
            fetch_attachment: None,
            token: Some(TOKEN.to_string()),
            #[cfg(feature = "a2a")]
            a2a: None,
        };
        let server = tokio::spawn(async move {
            serve(engine, cfg, async {
                let _stopped_or_dropped = stop_rx.await;
            })
            .await
            .expect("serve");
        });
        let mut actual = socket.clone();
        for _ in 0..50 {
            if actual.exists() {
                break;
            }
            if let Ok(p) = std::fs::read_to_string(bisa_node::pointer_path(&socket)) {
                actual = PathBuf::from(p.trim());
                if actual.exists() {
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        Self {
            socket: actual,
            stop: Some(stop),
            server: Some(server),
            project: project.id.to_string(),
            goal: goal.id.to_string(),
            _dir: dir,
        }
    }

    async fn req(&self, method: &str, path: &str, body: Option<Value>) -> (u16, Value) {
        let stream = UnixStream::connect(&self.socket).await.expect("connect");
        let (mut sender, conn) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
            .await
            .expect("handshake");
        tokio::spawn(conn);
        let request = Request::builder()
            .method(method)
            .uri(path)
            .header(hyper::header::HOST, "localhost")
            .header(hyper::header::AUTHORIZATION, format!("Bearer {TOKEN}"))
            .header(hyper::header::CONTENT_TYPE, "application/json")
            .body(Full::new(Bytes::from(
                body.map(|b| b.to_string()).unwrap_or_default(),
            )))
            .unwrap();
        let resp = sender.send_request(request).await.expect("request");
        let status = resp.status().as_u16();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    async fn get(&self, path: &str) -> Value {
        let (status, v) = self.req("GET", path, None).await;
        assert_eq!(status, 200, "{path}: {v}");
        v
    }

    async fn start_conversation(&self, origin: Value, title: Option<&str>) -> String {
        let (status, v) = self
            .req(
                "POST",
                "/conversations",
                Some(json!({"origin": origin, "title": title})),
            )
            .await;
        assert_eq!(status, 201, "{v}");
        v["conversation"]["id"].as_str().unwrap().to_string()
    }

    async fn stop(mut self) {
        let _server_gone = self.stop.take().unwrap().send(());
        let server = self.server.take().unwrap();
        tokio::time::timeout(Duration::from_secs(10), server)
            .await
            .expect("server did not stop")
            .expect("server task");
    }
}

async fn until<T, F, Fut>(what: &str, mut probe: F) -> T
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Option<T>>,
{
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(v) = probe().await {
            return v;
        }
        assert!(std::time::Instant::now() < deadline, "timed out: {what}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn ids(v: &Value) -> Vec<String> {
    v["conversations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap().to_string())
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_conversation_is_started_with_an_origin_and_an_origin_that_is_not_here_is_refused() {
    let node = Node::start().await;
    let pid = node.project.clone();
    let gid = node.goal.clone();
    for (origin, expect_id) in [
        (json!({"kind": "node"}), false),
        (json!({"kind": "workspace"}), false),
        (json!({"kind": "goal", "id": gid}), true),
        (json!({"kind": "project", "id": pid}), true),
        (
            json!({"kind": "workstream", "id": pid, "project": pid}),
            true,
        ),
    ] {
        let (status, v) = node
            .req("POST", "/conversations", Some(json!({"origin": origin})))
            .await;
        assert_eq!(status, 201, "{origin}: {v}");
        let c = &v["conversation"];
        assert_eq!(c["origin"]["kind"], origin["kind"]);
        assert_eq!(c["origin"].get("id").is_some(), expect_id, "{origin}");
        assert_eq!(c["message_count"], json!(0));
        assert!(
            c.get("since_summary").is_none(),
            "no compaction counter on the wire"
        );
        assert_eq!(c["archived"], json!(false));
        assert!(c.get("title").is_none());
        assert_eq!(c["agents"], json!([]));
        let one = node
            .get(&format!("/conversations/{}", c["id"].as_str().unwrap()))
            .await;
        assert_eq!(one["conversation"], *c);
    }
    let stranger = ulid::Ulid::from_parts(1, 1).to_string();
    for origin in [
        json!({"kind": "goal", "id": stranger}),
        json!({"kind": "project", "id": stranger}),
        json!({"kind": "workstream", "id": stranger, "project": pid}),
        json!({"kind": "workflow", "id": stranger}),
    ] {
        let (status, v) = node
            .req("POST", "/conversations", Some(json!({"origin": origin})))
            .await;
        assert_eq!(status, 404, "{origin}: {v}");
    }
    let (status, _) = node
        .req(
            "POST",
            "/conversations",
            Some(json!({"origin": {"kind": "room"}})),
        )
        .await;
    assert_eq!(status, 400, "an origin kind that does not exist");
    let (status, _) = node
        .req(
            "POST",
            "/conversations",
            Some(json!({"origin": {"kind": "node"}, "title": "   "})),
        )
        .await;
    assert_eq!(status, 400, "a blank title");
    let (status, _) = node.req("GET", "/conversations/not-an-id", None).await;
    assert_eq!(status, 400);
    let (status, _) = node
        .req("GET", &format!("/conversations/{stranger}"), None)
        .await;
    assert_eq!(status, 404);
    node.stop().await;
}

/// Every kind a conversation may be about — the five surfaces of the desktop
/// start one each (a goal, a workflow, a checkout, a drawing, a note), and
/// the rest stand in a project, the workspace or the node. Each is listed by
/// its own origin and nobody else's, and read by its id whatever a page of
/// the list holds: what a surface is on is a record, never a row.
#[tokio::test(flavor = "multi_thread")]
async fn each_origin_kind_is_started_listed_by_its_own_and_read_by_id_off_any_page() {
    let node = Node::start().await;
    let (pid, gid) = (node.project.clone(), node.goal.clone());
    let made = |what: &'static str, route: &'static str, body: Value| {
        let node = &node;
        async move {
            let (status, v) = node.req("POST", route, Some(body)).await;
            assert!(status == 200 || status == 201, "{what}: {status} {v}");
            v[what]["id"]
                .as_str()
                .unwrap_or_else(|| panic!("the {what}'s id: {v}"))
                .to_string()
        }
    };
    let workflow = made(
        "workflow",
        "/workflows",
        json!({
            "name": "Something to talk about",
            "steps": [
                {"id": "begin", "name": "Begin", "kind": "start",
                 "on": {"event": "manual"}, "then": ["say"]},
                {"id": "say", "name": "Say", "kind": "notify", "template": "begun"},
            ]
        }),
    )
    .await;
    let drawing = made(
        "drawing",
        "/drawings",
        json!({"scope": "workspace", "title": "A sketch"}),
    )
    .await;
    let note = made(
        "note",
        "/notes",
        json!({"scope": "workspace", "title": "A plan"}),
    )
    .await;

    let origin_of = |kind: &str| match kind {
        "node" | "workspace" => json!({ "kind": kind }),
        "goal" => json!({"kind": kind, "id": gid}),
        "workflow" => json!({"kind": kind, "id": workflow}),
        "project" => json!({"kind": kind, "id": pid}),
        "workstream" => json!({"kind": kind, "id": pid, "project": pid}),
        "drawing" => json!({"kind": kind, "id": drawing}),
        "note" => json!({"kind": kind, "id": note}),
        other => panic!("an origin kind this test does not know how to stand in: {other}"),
    };
    let mut started = Vec::new();
    for kind in bisa_core::ConversationOrigin::KINDS {
        let origin = origin_of(kind);
        let id = node.start_conversation(origin.clone(), None).await;
        started.push((kind, origin, id));
    }
    assert_eq!(started.len(), 8, "every kind the wire knows");

    for (kind, origin, id) in &started {
        let narrowed = match origin.get("id").and_then(Value::as_str) {
            Some(about) => format!("/conversations?origin={kind}&id={about}"),
            None => format!("/conversations?origin={kind}"),
        };
        let listed = node.get(&narrowed).await;
        assert_eq!(
            ids(&listed),
            vec![id.clone()],
            "{kind}: its own and no other"
        );
        assert_eq!(listed["conversations"][0]["origin"], *origin, "{kind}");
    }

    // One row a page: seven of the eight are on no page a surface holds, and
    // each is still read — by its id.
    let page = node.get("/conversations?limit=1").await;
    let on_the_page = ids(&page);
    assert_eq!(on_the_page.len(), 1);
    for (kind, origin, id) in &started {
        let one = node.get(&format!("/conversations/{id}")).await;
        assert_eq!(one["conversation"]["id"], json!(id), "{kind}");
        assert_eq!(one["conversation"]["origin"], *origin, "{kind}");
    }
    assert_eq!(
        started
            .iter()
            .filter(|(_, _, id)| !on_the_page.contains(id))
            .count(),
        7
    );

    // A drawing and a note nobody made are origins that are not here.
    let stranger = ulid::Ulid::from_parts(1, 1).to_string();
    for kind in ["drawing", "note"] {
        let (status, v) = node
            .req(
                "POST",
                "/conversations",
                Some(json!({"origin": {"kind": kind, "id": stranger}})),
            )
            .await;
        assert_eq!(status, 404, "{kind}: {v}");
    }

    // One that is gone is not found — what a surface drops its pick on —
    // and the others stand.
    let (_, _, gone) = &started[0];
    let (status, _) = node
        .req("DELETE", &format!("/conversations/{gone}"), None)
        .await;
    assert_eq!(status, 204);
    let (status, v) = node
        .req("GET", &format!("/conversations/{gone}"), None)
        .await;
    assert_eq!(status, 404, "{v}");
    assert_eq!(ids(&node.get("/conversations?limit=100").await).len(), 7);
    node.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_list_narrows_by_origin_agent_words_and_archived_and_pages() {
    let node = Node::start().await;
    let pid = node.project.clone();
    let gid = node.goal.clone();
    let about_goal = node
        .start_conversation(json!({"kind": "goal", "id": gid}), Some("Palette"))
        .await;
    let about_checkout = node
        .start_conversation(
            json!({"kind": "workstream", "id": pid, "project": pid}),
            None,
        )
        .await;
    let quiet = node
        .start_conversation(json!({"kind": "workspace"}), Some("Quiet"))
        .await;

    // A post wakes the general agent, whose reply counts it into the agents.
    let (status, v) = node
        .req(
            "POST",
            &format!("/conversations/{about_checkout}/messages"),
            Some(json!({"content": "which palette do we ship?\nsecond line"})),
        )
        .await;
    assert_eq!(status, 200, "{v}");
    let answered = until("the general agent answers", || {
        Box::pin(async {
            let v = node.get(&format!("/conversations/{about_checkout}")).await;
            (v["conversation"]["message_count"] == json!(2)).then_some(v)
        })
    })
    .await;
    let row = &answered["conversation"];
    assert_eq!(row["agents"], json!(["general-agent"]));
    assert_eq!(row["first_line"], json!("which palette do we ship?"));

    let all = node.get("/conversations").await;
    // The order is the rule's — last activity, newest first, then the newer
    // id — read from the rows themselves: the three were made and the one
    // spoken in within the same second, so the clock alone decides nothing.
    let mut by_rule: Vec<&Value> = all["conversations"].as_array().unwrap().iter().collect();
    by_rule.sort_by(|a, b| {
        let moved = |c: &Value| {
            c["last_message_at"]
                .as_u64()
                .unwrap_or_else(|| c["created_at"].as_u64().unwrap())
        };
        moved(b)
            .cmp(&moved(a))
            .then_with(|| b["id"].as_str().cmp(&a["id"].as_str()))
    });
    assert_eq!(
        ids(&all),
        by_rule
            .iter()
            .map(|c| c["id"].as_str().unwrap().to_string())
            .collect::<Vec<_>>(),
        "the one that moved last is first; the rest by birth, newest first"
    );
    let mut every = ids(&all);
    every.sort();
    let mut made = vec![about_checkout.clone(), quiet.clone(), about_goal.clone()];
    made.sort();
    assert_eq!(every, made);
    let by_origin = node
        .get(&format!("/conversations?origin=goal&id={gid}"))
        .await;
    assert_eq!(ids(&by_origin), vec![about_goal.clone()]);
    let by_kind = node.get("/conversations?origin=workspace").await;
    assert_eq!(ids(&by_kind), vec![quiet.clone()]);
    let by_checkout = node
        .get(&format!("/conversations?origin=workstream&id={pid}"))
        .await;
    assert_eq!(ids(&by_checkout), vec![about_checkout.clone()]);
    // By project: the project's own and its checkouts', the view naming the
    // project on each row; a goal's carries none.
    let about_project = node
        .start_conversation(json!({"kind": "project", "id": pid}), Some("Web"))
        .await;
    let by_project = node.get(&format!("/conversations?project={pid}")).await;
    let mut of_project = ids(&by_project);
    of_project.sort();
    let mut expected = vec![about_checkout.clone(), about_project.clone()];
    expected.sort();
    assert_eq!(of_project, expected, "{by_project}");
    for row in by_project["conversations"].as_array().unwrap() {
        assert_eq!(row["project"], json!(pid), "{row}");
    }
    assert!(
        by_origin["conversations"][0].get("project").is_none(),
        "a goal's conversation stands in no project"
    );
    let live_of_project = node
        .get(&format!("/conversations?project={pid}&archived=false"))
        .await;
    assert_eq!(ids(&live_of_project).len(), 2);
    let (status, v) = node
        .req(
            "GET",
            &format!("/conversations?project={pid}&origin=project&id={pid}"),
            None,
        )
        .await;
    assert_eq!(status, 400, "by project or by origin, not both: {v}");
    assert!(v["error"].as_str().unwrap().contains("not both"), "{v}");
    let (status, _) = node
        .req("GET", "/conversations?project=not-an-id", None)
        .await;
    assert_eq!(status, 400, "a project id is parsed at the wire");
    node.req("DELETE", &format!("/conversations/{about_project}"), None)
        .await;
    let by_agent = node.get("/conversations?agent=general-agent").await;
    assert_eq!(ids(&by_agent), vec![about_checkout.clone()]);
    let by_words = node.get("/conversations?q=palette").await;
    assert_eq!(
        ids(&by_words),
        vec![about_checkout.clone(), about_goal.clone()],
        "words in the messages and in the titles alike"
    );
    let one = node.get("/conversations?limit=1").await;
    assert_eq!(ids(&one).len(), 1);
    let (status, _) = node.req("GET", "/conversations?origin=room", None).await;
    assert_eq!(status, 400);
    let (status, _) = node.req("GET", "/conversations?origin=goal", None).await;
    assert_eq!(status, 400, "a goal origin needs its id");
    let (status, _) = node
        .req("GET", "/conversations?origin=node&id=x", None)
        .await;
    assert_eq!(status, 400, "the node takes no id");
    let (status, _) = node
        .req("GET", "/conversations?agent=Not%20An%20Id", None)
        .await;
    assert_eq!(status, 400);

    // Archived: out of the live list, refused a message, back on request.
    let (status, v) = node
        .req(
            "PATCH",
            &format!("/conversations/{quiet}"),
            Some(json!({"archived": true})),
        )
        .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["conversation"]["archived"], json!(true));
    let live = node.get("/conversations?archived=false").await;
    assert!(!ids(&live).contains(&quiet));
    let put_away = node.get("/conversations?archived=true").await;
    assert_eq!(ids(&put_away), vec![quiet.clone()]);
    let (status, _) = node
        .req(
            "POST",
            &format!("/conversations/{quiet}/messages"),
            Some(json!({"content": "anyone?"})),
        )
        .await;
    assert_eq!(
        status, 409,
        "an archived conversation is read, not continued"
    );
    let (status, v) = node
        .req(
            "PATCH",
            &format!("/conversations/{quiet}"),
            Some(json!({"archived": false})),
        )
        .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["conversation"]["archived"], json!(false));
    node.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_title_is_tri_state_and_a_delete_takes_everything() {
    let node = Node::start().await;
    let id = node.start_conversation(json!({"kind": "node"}), None).await;
    let (status, v) = node
        .req(
            "PATCH",
            &format!("/conversations/{id}"),
            Some(json!({"title": "  Ship it  "})),
        )
        .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["conversation"]["title"], json!("Ship it"));
    let (status, v) = node
        .req("PATCH", &format!("/conversations/{id}"), Some(json!({})))
        .await;
    assert_eq!(status, 200);
    assert_eq!(v["conversation"]["title"], json!("Ship it"), "absent keeps");
    let (status, v) = node
        .req(
            "PATCH",
            &format!("/conversations/{id}"),
            Some(json!({"title": null})),
        )
        .await;
    assert_eq!(status, 200);
    assert!(
        v["conversation"].get("title").is_none(),
        "null takes it away"
    );
    let (status, _) = node
        .req(
            "PATCH",
            &format!("/conversations/{id}"),
            Some(json!({"title": " "})),
        )
        .await;
    assert_eq!(status, 400, "a blank title");
    let too_long = "x".repeat(bisa_core::MAX_CONVERSATION_TITLE_CHARS + 1);
    let (status, _) = node
        .req(
            "PATCH",
            &format!("/conversations/{id}"),
            Some(json!({"title": too_long})),
        )
        .await;
    assert_eq!(status, 400);

    let (status, _) = node
        .req(
            "POST",
            &format!("/conversations/{id}/messages"),
            Some(json!({"content": "remember this"})),
        )
        .await;
    assert_eq!(status, 200);
    let (status, _) = node
        .req("DELETE", &format!("/conversations/{id}"), None)
        .await;
    assert_eq!(status, 204);
    let (status, _) = node.req("GET", &format!("/conversations/{id}"), None).await;
    assert_eq!(status, 404);
    let (status, _) = node
        .req("GET", &format!("/conversations/{id}/messages"), None)
        .await;
    assert_eq!(status, 404, "its messages went with it");
    let (status, _) = node
        .req("DELETE", &format!("/conversations/{id}"), None)
        .await;
    assert_eq!(status, 404, "twice is not found");
    node.stop().await;
}

/// The mock harness answers whole — `echo: <prompt>` in one delta at the
/// turn's end — so the turn in flight is read before a post and gone after
/// it, and the reply it posts carries no thinking, since the mock thinks
/// none aloud; the message keeps its shape either way.
#[tokio::test(flavor = "multi_thread")]
async fn a_live_turn_is_readable_mid_turn_and_empty_after() {
    let node = Node::start().await;
    let id = node
        .start_conversation(json!({"kind": "workspace"}), None)
        .await;
    let v = node.get(&format!("/conversations/{id}/live")).await;
    assert_eq!(
        v,
        json!({"turns": []}),
        "nothing runs before a word is said"
    );
    let (status, _) = node
        .req(
            "POST",
            &format!("/conversations/{id}/messages"),
            Some(json!({"content": "we chose the muted palette"})),
        )
        .await;
    assert_eq!(status, 200);
    until("the general agent answers", || {
        Box::pin(async {
            let v = node.get(&format!("/conversations/{id}")).await;
            (v["conversation"]["message_count"] == json!(2)).then_some(())
        })
    })
    .await;
    let v = node.get(&format!("/conversations/{id}/live")).await;
    assert_eq!(
        v["turns"],
        json!([]),
        "the reply landed: nothing is in flight"
    );
    let messages = node.get(&format!("/conversations/{id}/messages")).await;
    let posts = messages["messages"].as_array().unwrap();
    assert_eq!(posts.len(), 2);
    assert!(
        posts.iter().all(|m| m["body_kind"] == json!("post")),
        "every message is a post; no summary kind exists"
    );
    let reply = posts
        .iter()
        .find(|m| {
            m["author"] != posts[0]["author"] || m["content"].as_str().unwrap().starts_with("echo")
        })
        .expect("the agent's reply");
    assert!(
        reply.get("thinking").is_none() || reply["thinking"].is_null(),
        "the mock thinks nothing aloud: {reply}"
    );
    let (status, _) = node
        .req(
            "GET",
            &format!(
                "/conversations/{}/live",
                bisa_core::ConversationId::from_ulid(ulid::Ulid::from_datetime(
                    std::time::SystemTime::now()
                ))
            ),
            None,
        )
        .await;
    assert_eq!(status, 404, "an unknown conversation has no live turns");
    node.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_turn_of_a_conversation_is_on_the_roster_with_its_kind_and_its_conversation() {
    let node = Node::start().await;
    let pid = node.project.clone();
    let id = node
        .start_conversation(
            json!({"kind": "workstream", "id": pid, "project": pid}),
            None,
        )
        .await;
    let (status, _) = node
        .req(
            "POST",
            &format!("/conversations/{id}/messages"),
            Some(json!({"content": "what stands here?"})),
        )
        .await;
    assert_eq!(status, 200);
    let row = until("the turn is on the roster", || {
        Box::pin(async {
            let v = node.get("/sessions").await;
            v["sessions"]
                .as_array()
                .unwrap()
                .iter()
                .find(|s| s["conversation"] == json!(id))
                .cloned()
        })
    })
    .await;
    assert_eq!(row["kind"], json!("conversation"));
    assert_eq!(row["workstream"], json!(pid));
    assert_eq!(row["project"], json!(pid));
    assert_eq!(row["agent"], json!("general-agent"));
    // The workstream's status counts the sessions standing in the checkout;
    // a conversation's turn stands on the roster, never in that count.
    let status = node.get(&format!("/workstreams/{pid}/status")).await;
    assert_eq!(
        status["status"]["running_agents"],
        json!(0),
        "a conversation's turn is not one of the checkout's running agents: {status}"
    );
    node.stop().await;
}
