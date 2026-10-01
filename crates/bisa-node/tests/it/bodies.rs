//! What the node says to a body it cannot take (`Body<T>` in `lib.rs`): a
//! body over the route's limit is a 413, a body that is not JSON by content
//! type is a 415, JSON of the wrong shape stays the 400 the contract documents
//! — each in the error body's shape, so a client reads one thing whatever the
//! cause. And a save the read side calls editable is one the write side
//! takes: `PUT /ide/file` is sized from `EDITABLE_BYTES`, not the framework's
//! default.

use crate::node::Node;
use serde_json::json;

#[tokio::test(flavor = "multi_thread")]
async fn a_body_without_a_json_content_type_is_a_415_in_the_error_body_shape() {
    let node = Node::start().await;
    let (status, v) = node
        .req_raw(
            "POST",
            "/goals",
            Some("text/plain"),
            br#"{"statement":"x"}"#.to_vec(),
        )
        .await;
    assert_eq!(status, 415, "{v}");
    assert!(
        v["error"]
            .as_str()
            .is_some_and(|s| s.contains("application/json")),
        "the sentence names what the body needs: {v}"
    );
    assert_eq!(
        v["text"]["id"],
        json!("error-node-body-needs-json-content-type"),
        "{v}"
    );
    node.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_body_over_the_route_limit_is_a_413_in_the_error_body_shape() {
    let node = Node::start().await;
    let mut body = br#"{"statement":""#.to_vec();
    body.extend(std::iter::repeat_n(b'x', 2 * 1024 * 1024));
    body.extend_from_slice(br#""}"#);
    let (status, v) = node
        .req_raw("POST", "/goals", Some("application/json"), body)
        .await;
    assert_eq!(status, 413, "{v}");
    assert_eq!(v["text"]["id"], json!("error-node-body-too-large"), "{v}");
    assert!(v["error"].is_string(), "{v}");
    node.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_ill_shaped_body_is_still_a_400_with_the_extractors_sentence() {
    let node = Node::start().await;
    // JSON, the right type, the wrong shape: the documented contract is a 400
    // carrying the extractor's own words, not axum's 422.
    let (status, v) = node
        .req_raw(
            "POST",
            "/goals",
            Some("application/json"),
            br#"{"statement": 5}"#.to_vec(),
        )
        .await;
    assert_eq!(status, 400, "{v}");
    assert_eq!(v["text"]["id"], json!("error-node-body-unreadable"), "{v}");
    assert!(
        v["error"].as_str().is_some_and(|s| s.contains("statement")),
        "the extractor names the field: {v}"
    );
    // And a body that is not JSON at all is the same 400.
    let (status, v) = node
        .req_raw(
            "POST",
            "/goals",
            Some("application/json"),
            b"{not json".to_vec(),
        )
        .await;
    assert_eq!(status, 400, "{v}");
    node.shutdown().await;
}

/// Every route reads its body through the one extractor — the routes that
/// may go without a body among them: a body that is there and does not fit
/// is the 400 every other route answers, in the error body's shape, never
/// the framework's own status in prose.
#[tokio::test(flavor = "multi_thread")]
async fn a_body_that_does_not_fit_is_a_400_on_the_routes_that_may_go_without_one() {
    let node = Node::start().await;
    let ghost = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
    for (path, body) in [
        (format!("/goals/{ghost}/stop"), json!({"rationale": 7})),
        (
            format!("/goals/{ghost}/stop"),
            json!({"why": "a key nobody knows"}),
        ),
        (format!("/goals/{ghost}/close"), json!({"rationale": 7})),
        (format!("/runs/{ghost}/stop"), json!({"rationale": 7})),
        ("/network/check".to_string(), json!({"url": 7})),
        ("/network/check".to_string(), json!({})),
    ] {
        let (status, v) = node.req("POST", &path, Some(body.clone())).await;
        assert_eq!(status, 400, "POST {path} {body}: {v}");
        assert_eq!(
            v["text"]["id"],
            json!("error-node-body-unreadable"),
            "POST {path} {body}: {v}"
        );
    }
    // Not JSON at all, under a JSON content type.
    let (status, v) = node
        .req_raw(
            "POST",
            &format!("/runs/{ghost}/stop"),
            Some("application/json"),
            b"{ not json".to_vec(),
        )
        .await;
    assert_eq!(status, 400, "{v}");
    assert_eq!(v["text"]["id"], json!("error-node-body-unreadable"), "{v}");
    // And with no body at all they are asked about the thing they name.
    let (status, v) = node
        .req_raw("POST", &format!("/runs/{ghost}/stop"), None, Vec::new())
        .await;
    assert_eq!(status, 404, "{v}");
    node.shutdown().await;
}

/// **A key nobody knows is refused by name, never dropped.** `mode` misspelt
/// captured an auto goal — the designer woken, tokens spent — where a manual
/// one was asked for. Held on a body that is one shape, on one that is a
/// choice between shapes, on one another crate declares, and on a key
/// misspelt inside what a body holds; the guard by signature
/// (`layering::every_body_refuses_a_key_nobody_knows`) holds the rest.
#[tokio::test(flavor = "multi_thread")]
async fn a_key_nobody_knows_is_refused_by_name_and_nothing_is_made() {
    let node = Node::start().await;
    let ghost = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
    for (method, path, body, key) in [
        (
            "POST",
            "/goals".to_string(),
            json!({"statement": "pick a shelf", "moed": "manual"}),
            "moed",
        ),
        (
            "POST",
            "/projects".to_string(),
            json!({"kind": "new", "slug": "shelf", "nmae": "Shelf"}),
            "nmae",
        ),
        (
            "POST",
            format!("/goals/{ghost}/retire"),
            json!({"goal": "archive", "projects": "keep", "tre": true}),
            "tre",
        ),
        (
            "POST",
            format!("/conversations/{ghost}/asks/{ghost}"),
            json!({"answer": "deny", "noet": "not this one"}),
            "noet",
        ),
        (
            "PUT",
            "/git/config".to_string(),
            json!({"set": {}, "unste": ["user.name"]}),
            "unste",
        ),
    ] {
        let (status, v) = node.req(method, &path, Some(body.clone())).await;
        assert_eq!(status, 400, "{method} {path} {body}: {v}");
        assert_eq!(
            v["text"]["id"],
            json!("error-node-body-unreadable"),
            "{method} {path}: {v}"
        );
        assert!(
            v["error"].as_str().is_some_and(|said| said.contains(key)),
            "{method} {path} names the key it does not know: {v}"
        );
    }
    let (_, goals) = node.req("GET", "/goals", None).await;
    assert_eq!(goals["goals"], json!([]), "no goal was captured: {goals}");
    let (_, projects) = node.req("GET", "/projects", None).await;
    assert_eq!(
        projects["projects"],
        json!([]),
        "and no project: {projects}"
    );
    node.shutdown().await;
}
