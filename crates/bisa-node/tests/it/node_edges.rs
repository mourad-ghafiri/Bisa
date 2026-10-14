//! The node's smaller doors at their edges: a note's scope words, a settings
//! query naming no project, a tag query that is not one and one that wants
//! every tag, a workflow named by an id or a slug nobody installed, a
//! workflow retired through its route, an attachment asked of a peer when no
//! pump runs, the inbox's people row for a join that waits, and a node whose
//! data folder's path is too long for a socket.

use crate::node::Node;
use bisa_engine::EngineConfig;
use serde_json::json;

#[tokio::test(flavor = "multi_thread")]
async fn the_small_doors_refuse_in_words_and_take_what_is_well_formed() {
    let node = Node::start().await;

    // A note's scope: a kind nobody knows, an id with no kind, a kind alone
    // (a filter by kind), a kind with an id that is none.
    let (code, v) = node.req("GET", "/notes?scope=bogus", None).await;
    assert_eq!(code, 400, "{v}");
    let (code, v) = node.req("GET", "/notes?id=x", None).await;
    assert_eq!(code, 400, "{v}");
    let (code, v) = node.req("GET", "/notes?scope=goal", None).await;
    assert_eq!(code, 200, "{v}");
    assert_eq!(v["notes"], json!([]));
    let (code, v) = node.req("GET", "/notes?scope=goal&id=zz", None).await;
    assert_eq!(code, 400, "{v}");
    let (code, v) = node
        .req(
            "POST",
            "/notes",
            Some(json!({"scope": "bogus", "title": "t", "body": "b"})),
        )
        .await;
    assert_eq!(code, 400, "{v}");

    // A settings layer asked for a project that is no id.
    let (code, v) = node.req("GET", "/settings/project?project=zz", None).await;
    assert_eq!(code, 400, "{v}");

    // A tag query that cannot be read, and one that wants every tag.
    let (code, v) = node.req("GET", "/agents?tag=%E0%A4%A", None).await;
    assert_eq!(code, 400, "{v}");
    let (code, v) = node.req("GET", "/catalog?tag=a&match=all", None).await;
    assert_eq!(code, 200, "{v}");

    // A goal naming a workflow by an id nobody has, or a slug nobody installed.
    let fresh =
        bisa_core::WorkflowId::from_ulid(ulid::Ulid::from_datetime(std::time::SystemTime::now()));
    let (code, v) = node
        .req(
            "POST",
            "/goals",
            Some(json!({"statement": "do it", "workflow": fresh.to_string()})),
        )
        .await;
    assert_eq!(code, 404, "{v}");
    let (code, v) = node
        .req(
            "POST",
            "/goals",
            Some(json!({"statement": "do it", "workflow": "no-such-slug"})),
        )
        .await;
    assert_eq!(code, 404, "{v}");

    // A workflow retired through its route.
    let wf = node
        .post(
            "/workflows",
            json!({"name": "Ask", "steps": [{
                "id": "ask", "name": "Ask", "kind": "human", "prompt": "which?"
            }]}),
        )
        .await;
    let id = wf["workflow"]["id"].as_str().unwrap().to_string();
    let (code, v) = node
        .req(
            "POST",
            &format!("/workflows/{id}/retire"),
            Some(json!({"workflow": "archive", "projects": "keep"})),
        )
        .await;
    assert_eq!(code, 200, "{v}");

    // An attachment asked of a peer when this node runs no pump.
    let (code, v) = node
        .req(
            "POST",
            &format!("/attachments/{}/fetch", "0".repeat(64)),
            None,
        )
        .await;
    assert!(code >= 400, "{code}: {v}");

    node.shutdown().await;
}

/// A person who claimed an invitation and waits on the owner is a people
/// row in the inbox, with the join the owner acts on.
#[tokio::test(flavor = "multi_thread")]
async fn a_join_that_waits_is_a_people_row_in_the_inbox() {
    let node = Node::start().await;
    let v = node
        .post("/workspace/invites", json!({"role": "member"}))
        .await;
    let id = v["invite"]["id"].as_str().unwrap().to_string();
    let parsed = bisa_collab::InviteCode::parse(v["code"].as_str().unwrap()).unwrap();
    let dana =
        bisa_core::PrincipalId::new(nostr::key::Keys::generate().public_key().to_hex()).unwrap();
    node.ws
        .claim_invite(
            &bisa_collab::hash_secret(&parsed.secret),
            &dana,
            Some("Dana".into()),
            true,
        )
        .unwrap();
    let v = node.get("/inbox?source=people").await;
    let rows = v["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1, "{v}");
    assert_eq!(rows[0]["kind"], json!("people"));
    assert_eq!(rows[0]["title"], json!("Dana"));
    assert_eq!(rows[0]["join"]["invite"], json!(id));
    assert_eq!(rows[0]["join"]["role"], json!("member"));
    node.shutdown().await;
}

/// A data folder whose path is longer than a socket address allows gets a
/// short socket under the system's temporary folder and a pointer beside
/// the place the socket would have been.
#[tokio::test(flavor = "multi_thread")]
async fn a_data_folder_too_long_for_a_socket_gets_a_short_one_and_a_pointer() {
    let dir = tempfile::Builder::new()
        .prefix(&"a-long-folder-name-".repeat(6))
        .tempdir()
        .unwrap();
    let preferred = bisa_store::Paths::new(dir.path()).node_socket();
    assert!(preferred.as_os_str().len() > 90, "{}", preferred.display());
    let node = Node::start_in(
        dir,
        EngineConfig {
            design_enabled: false,
            ..Default::default()
        },
    )
    .await;
    let pointer = bisa_node::pointer_path(&preferred);
    let short = std::fs::read_to_string(&pointer).expect("a pointer to the short socket");
    assert!(std::path::Path::new(short.trim()).exists(), "{short}");
    assert!(!preferred.exists());
    let v = node.get("/settings/registry").await;
    assert!(v.is_object());
    node.shutdown().await;
}

/// A refusal asked for in another language comes rendered — the catalog's
/// fallback or its own words.
#[tokio::test(flavor = "multi_thread")]
async fn a_refusal_is_rendered_in_the_language_asked() {
    let node = Node::start().await;
    let (code, v) = node
        .req_in_language("fr-CA, fr;q=0.9", "GET", "/notes?scope=bogus")
        .await;
    assert_eq!(code, 400, "{v}");
    assert!(v["error"].as_str().is_some_and(|e| !e.is_empty()), "{v}");
    node.shutdown().await;
}
