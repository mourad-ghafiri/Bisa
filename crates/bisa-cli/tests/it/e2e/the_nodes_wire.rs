//! The node's wire, through the binary: who may call it, what it says to a
//! body it cannot take, what an open event stream carries — and that a node
//! asked to stop while a stream is open ends, says goodbye, and tells the
//! listener the stream is over.

use super::sealed::Sealed;
use serde_json::{json, Value};

fn nobody_works() -> Value {
    json!({ "turns": [] })
}

/// A workflow that waits for a person: a run that stays open.
fn held() -> Value {
    json!({
        "name": "Hold",
        "steps": [{ "id": "hold", "name": "Hold", "kind": "wait", "until": { "until": "release" } }],
    })
}

#[test]
fn the_node_takes_its_token_and_says_no_in_one_shape() {
    let mut ws = Sealed::with_script(&nobody_works());
    ws.start();

    // A liveness probe carries no authority; everything else takes the token.
    let (status, health) = ws.call_with("GET", "/health", &[], None);
    assert_eq!(status, 200, "{health}");
    assert_eq!(health["ok"], true);
    for (headers, what) in [
        (vec![], "no token"),
        (
            vec![("authorization", "Bearer not-the-token")],
            "a wrong token",
        ),
        (
            vec![("authorization", "Basic dXNlcjpwYXNz")],
            "another scheme",
        ),
    ] {
        let (status, refused) = ws.call_with("GET", "/goals", &headers, None);
        assert_eq!(status, 401, "{what}: {refused}");
        assert!(
            refused["error"].is_string(),
            "{what} is refused in words: {refused}"
        );
    }
    let (status, goals) = ws.call("GET", "/goals", &[], None);
    assert_eq!(status, 200, "{goals}");

    // What this node is, as it says it.
    let (status, node) = ws.call("GET", "/node", &[], None);
    assert_eq!(status, 200, "{node}");
    assert_eq!(
        node["socket"].as_str().map(std::path::PathBuf::from),
        Some(ws.socket()),
        "the socket it bound, which is the one a client found: {node}"
    );
    assert_eq!(node["listen"], Value::Null, "it opened no network door");

    // A refusal is one shape, whatever was wrong: a status, and words.
    for (method, path, body, expected) in [
        ("GET", "/no/such/route", None, 404),
        ("GET", "/goals/01ARZ3NDEKTSV4RRFFQ69G5FAV", None, 404),
        ("GET", "/goals/not-an-id", None, 400),
        ("POST", "/goals", Some(json!({ "statement": 5 })), 400),
        // A key nobody knows is refused, never dropped: `mode` misspelt
        // would have captured an auto goal.
        (
            "POST",
            "/goals",
            Some(json!({ "statement": "x", "moed": "manual" })),
            400,
        ),
        ("DELETE", "/health", None, 405),
    ] {
        let (status, refused) = ws.call(method, path, &[], body.as_ref());
        assert_eq!(status, expected, "{method} {path}: {refused}");
        if expected != 405 && path != "/no/such/route" {
            assert!(
                refused["error"]
                    .as_str()
                    .is_some_and(|said| !said.is_empty()),
                "{method} {path} says why: {refused}"
            );
        }
    }

    // A body over the route's limit is refused as too large, in the same shape.
    let huge = json!({ "statement": "x".repeat(3 * 1024 * 1024) });
    let (status, refused) = ws.call("POST", "/goals", &[], Some(&huge));
    assert_eq!(status, 413, "{}", refused["error"]);
    assert!(refused["error"].is_string());
    // And the node is as it was.
    let (status, _) = ws.call("GET", "/goals", &[], None);
    assert_eq!(status, 200);
    ws.stop();
}

#[test]
fn a_node_asked_to_stop_while_a_stream_is_open_ends_and_the_listener_is_told() {
    let mut ws = Sealed::with_script(&nobody_works());
    let file = ws.file("hold.json", &held().to_string());
    let workflow = ws.json(&["workflow", "new", "--from", &file.to_string_lossy()])["workflow"]
        ["id"]
        .as_str()
        .expect("its id")
        .to_string();
    ws.start();

    // Two listeners, as a desktop and a script would be, and a run going.
    let desktop = ws.listen();
    let script = ws.listen();
    let started = ws.json(&["workflow", "run", &workflow]);
    assert_eq!(started["status"], "waiting", "{started}");
    ws.until("the run to be said on the stream", || {
        desktop
            .engine_events()
            .iter()
            .any(|event| event == "run_started")
            .then_some(())
    });
    assert!(
        script
            .engine_events()
            .iter()
            .any(|event| event == "run_started"),
        "every listener hears it: {:?}",
        script.engine_events()
    );

    // Asked to stop with both streams open and the run waiting: it ends
    // well, in time, and leaves no socket — `stop` holds it to that.
    ws.stop();
    for listener in [desktop, script] {
        let heard = listener.ended();
        assert!(heard.ended, "the listener is told the stream is over");
        assert!(!heard.frames.is_empty());
    }

    // The run is where it stood, and goes on when the node is back.
    let run = started["run"].as_str().expect("the run");
    assert_eq!(ws.json(&["status", run])["status"], "waiting");
    ws.start();
    let again = ws.listen();
    let released = ws.json(&["step", "release", run, "hold"]);
    assert_eq!(released["status"], "done", "{released}");
    ws.until("the run's end to be said", || {
        again
            .engine_events()
            .iter()
            .any(|event| event == "run_finished")
            .then_some(())
    });
    ws.stop();
    assert!(again.ended().ended);
}
