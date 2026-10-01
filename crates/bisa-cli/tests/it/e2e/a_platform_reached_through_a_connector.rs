//! An outside platform reached through a connector, while a node runs — the
//! platform a stub on this machine's loopback, the only host any of it may
//! reach: a definition recorded from a file, an account added with its
//! secret and checked as that account; a workflow that reads through the
//! connector, asks a person, and writes through it once — the write keyed,
//! sent with the account's credential and nothing else, its answer the
//! step's output; an agent reading through the same connector from a
//! conversation and refused the write; a workflow that polls the platform
//! and begins a run for every item it has not seen; and the secret never
//! printed, not in a record, not in an answer, not in what the platform
//! echoed back.

use super::sealed::{Sealed, AGENT_HARNESS};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

const TOKEN: &str = "stub-token-0000-not-a-real-one";

/// One request the stub was sent: what a journey reads back.
#[derive(Clone, Debug)]
struct Seen {
    method: String,
    path: String,
    authorization: Option<String>,
    idempotency_key: Option<String>,
    body: Value,
}

/// The platform: incidents it lists and takes, a `whoami`, and a door that
/// echoes the credential back — what a careless platform does, and what the
/// redactor is for.
struct Stub {
    base_url: String,
    host: String,
    seen: Arc<Mutex<Vec<Seen>>>,
    incidents: Arc<Mutex<Vec<Value>>>,
    _runtime: tokio::runtime::Runtime,
}

impl Stub {
    fn start() -> Self {
        use axum::extract::State;
        use axum::http::{HeaderMap, Method, StatusCode, Uri};
        use axum::routing::get;
        use axum::Router;

        #[derive(Clone)]
        struct Shared {
            seen: Arc<Mutex<Vec<Seen>>>,
            incidents: Arc<Mutex<Vec<Value>>>,
        }
        fn record(s: &Shared, method: &Method, uri: &Uri, headers: &HeaderMap, body: Value) {
            let header = |name: &str| {
                headers
                    .get(name)
                    .and_then(|v| v.to_str().ok())
                    .map(str::to_string)
            };
            s.seen.lock().unwrap().push(Seen {
                method: method.to_string(),
                path: uri.path().to_string(),
                authorization: header("authorization"),
                idempotency_key: header("idempotency-key"),
                body,
            });
        }
        async fn whoami(
            State(s): State<Shared>,
            method: Method,
            uri: Uri,
            headers: HeaderMap,
        ) -> (StatusCode, axum::Json<Value>) {
            record(&s, &method, &uri, &headers, Value::Null);
            match headers.get("authorization").and_then(|v| v.to_str().ok()) {
                Some(v) if v == format!("Bearer {TOKEN}") => {
                    (StatusCode::OK, axum::Json(json!({"user": "the-stub"})))
                }
                _ => (
                    StatusCode::UNAUTHORIZED,
                    axum::Json(json!({"message": "who are you?"})),
                ),
            }
        }
        async fn incidents(
            State(s): State<Shared>,
            method: Method,
            uri: Uri,
            headers: HeaderMap,
        ) -> axum::Json<Value> {
            record(&s, &method, &uri, &headers, Value::Null);
            let open = s.incidents.lock().unwrap().clone();
            axum::Json(json!({"incidents": open, "count": open.len()}))
        }
        async fn open_incident(
            State(s): State<Shared>,
            method: Method,
            uri: Uri,
            headers: HeaderMap,
            body: String,
        ) -> (StatusCode, axum::Json<Value>) {
            let body: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
            record(&s, &method, &uri, &headers, body.clone());
            let mut open = s.incidents.lock().unwrap();
            let id = format!("inc-{}", open.len() + 1);
            open.push(json!({"id": id, "title": body["title"]}));
            (StatusCode::CREATED, axum::Json(json!({"id": id})))
        }
        async fn echo(
            State(s): State<Shared>,
            method: Method,
            uri: Uri,
            headers: HeaderMap,
        ) -> axum::Json<Value> {
            record(&s, &method, &uri, &headers, Value::Null);
            let auth = headers
                .get("authorization")
                .and_then(|v| v.to_str().ok())
                .unwrap_or_default()
                .to_string();
            axum::Json(json!({"you_sent": auth}))
        }
        let shared = Shared {
            seen: Arc::new(Mutex::new(Vec::new())),
            incidents: Arc::new(Mutex::new(Vec::new())),
        };
        let app = Router::new()
            .route("/whoami", get(whoami))
            .route("/incidents", get(incidents).post(open_incident))
            .route("/echo", get(echo))
            .with_state(shared.clone());
        let runtime = tokio::runtime::Runtime::new().expect("a runtime for the stub");
        let listener = runtime
            .block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))
            .expect("a loopback port");
        let addr = listener.local_addr().expect("the port");
        runtime.spawn(async move {
            let _served = axum::serve(listener, app).await;
        });
        Stub {
            base_url: format!("http://{addr}"),
            host: addr.to_string(),
            seen: shared.seen,
            incidents: shared.incidents,
            _runtime: runtime,
        }
    }

    fn seen(&self) -> Vec<Seen> {
        self.seen.lock().unwrap().clone()
    }

    fn incidents(&self) -> Vec<Value> {
        self.incidents.lock().unwrap().clone()
    }
}

/// The connector, in the catalog's own shape.
fn definition(stub: &Stub) -> String {
    format!(
        r#"[connector]
name = "Status page"
description = "The status page's API on the loopback stub."
tags = ["ops"]
base_url = "{base}"
hosts = ["{host}"]
auth = {{ scheme = "bearer" }}
check = "whoami"

[[connector.operations]]
id = "whoami"
name = "Who am I"
description = "Answers who the token is."
method = "get"
path = "/whoami"
output = {{ select = "user" }}

[[connector.operations]]
id = "incidents"
name = "Open incidents"
description = "The incidents that are open."
method = "get"
path = "/incidents"
output = {{ select = "incidents" }}

[[connector.operations]]
id = "count"
name = "How many"
description = "How many incidents are open."
method = "get"
path = "/incidents"
output = {{ select = "count" }}

[[connector.operations]]
id = "echo"
name = "Echo"
description = "A door that says back what it was sent."
method = "get"
path = "/echo"
output = {{ select = "you_sent" }}

[[connector.operations]]
id = "open"
name = "Open an incident"
description = "Opens an incident with a title."
method = "post"
path = "/incidents"
body = {{ kind = "json", value = {{ title = "{{params.title}}" }} }}
output = {{ select = "id" }}
writes = true
idempotency = {{ header = "Idempotency-Key" }}

[[connector.operations.params]]
name = "title"
label = "Title"
required = true
doc = "One line."
"#,
        base = stub.base_url,
        host = stub.host
    )
}

/// Reads how many incidents are open, asks a person, opens one, and says
/// which.
fn the_workflow() -> Value {
    json!({
        "name": "Open an incident",
        "description": "read the platform, ask, write once",
        "inputs": [{ "name": "title", "label": "Title", "kind": "text", "required": true }],
        "steps": [
            { "id": "begin", "name": "By hand", "kind": "start", "on": { "event": "manual" }, "then": ["how_many"] },
            { "id": "how_many", "name": "How many are open", "kind": "connector",
              "connector": "status-page", "operation": "count", "params": {}, "then": ["ask"] },
            { "id": "ask", "name": "Open one?", "kind": "approval",
              "prompt": "{steps.how_many.output} open. Open {inputs.title}?", "then": ["open"] },
            { "id": "open", "name": "Open it", "kind": "connector",
              "connector": "status-page", "operation": "open",
              "params": { "title": "{inputs.title}" }, "then": ["say"] },
            { "id": "say", "name": "Say which", "kind": "notify",
              "template": "opened {steps.open.output} for {inputs.title}", "then": ["finish"] },
            { "id": "finish", "name": "Done", "kind": "end", "finish": "done" },
        ],
    })
}

/// Polls the platform's incidents and says each new one.
fn the_watch() -> Value {
    json!({
        "name": "Watch the incidents",
        "description": "one run per incident not seen before",
        "inputs": [{ "name": "incident", "label": "Incident", "kind": "text", "required": true }],
        "steps": [
            { "id": "seen", "name": "A new incident", "kind": "start",
              "on": { "event": "connector", "connector": "status-page", "operation": "incidents",
                      "key": "id", "every": 1, "params": {} },
              "inputs": { "incident": "{event.payload.id}" },
              "then": ["say"] },
            { "id": "say", "name": "Say so", "kind": "notify",
              "template": "incident {inputs.incident} is open", "then": ["finish"] },
            { "id": "finish", "name": "Done", "kind": "end", "finish": "done" },
        ],
    })
}

fn recorded(ws: &Sealed, name: &str, definition: &Value) -> String {
    let file = ws.file(name, &definition.to_string());
    let made = ws.json(&["workflow", "new", "--from", &file.to_string_lossy()]);
    assert_eq!(made["problems"], json!([]), "{made}");
    made["workflow"]["id"]
        .as_str()
        .unwrap_or_else(|| panic!("the workflow's id: {made}"))
        .to_string()
}

fn general(ws: &Sealed) -> Vec<String> {
    ws.json(&["msgs", "general"])["messages"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter_map(|m| m["content"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Nothing a journey can read carries the secret.
fn never_carries_the_secret(what: &str, text: &str) {
    assert!(
        !text.contains(TOKEN),
        "{what} carries the account's secret: {text}"
    );
}

#[test]
fn a_platform_is_reached_through_a_connector_and_the_secret_stays_home() {
    let stub = Stub::start();
    let mut ws = Sealed::with_script(&json!({ "turns": [
        {
            "scope": "conversation", "when": "how many incidents", "times": 1,
            "tools": [
                { "name": "list_connectors", "arguments": {} },
                { "name": "call_connector", "arguments": {
                    "connector": "status-page", "operation": "count", "params": {} } },
                { "name": "call_connector", "arguments": {
                    "connector": "status-page", "operation": "echo", "params": {} } },
                { "name": "call_connector", "arguments": {
                    "connector": "status-page", "operation": "open",
                    "params": { "title": "an agent's own" } } },
            ],
            "say": ["I looked."],
        },
        // The content screen: what an agent reads from outside is judged by
        // the classifier, a session of its own — which says it is safe.
        { "scope": "none", "when": "security reviewer", "say": ["SAFE"] },
    ]}));
    ws.start();

    // --- the definition, and this machine's account ---------------------------------
    let file = ws.file("status-page.toml", &definition(&stub));
    let made = ws.json(&["connector", "new", "--from", &file.to_string_lossy()]);
    assert_eq!(made["connector"]["id"], "status-page", "{made}");
    let shown = ws.json(&["connector", "show", "status-page"]);
    let operations: Vec<&str> = shown["connector"]["operations"]
        .as_array()
        .expect("the operations")
        .iter()
        .filter_map(|op| op["id"].as_str())
        .collect();
    assert_eq!(operations, ["whoami", "incidents", "count", "echo", "open"]);
    let secret = format!("token={TOKEN}");
    let added = ws.json(&[
        "connector",
        "account",
        "add",
        "status-page",
        "--label",
        "ops",
        "--secret",
        &secret,
        "--default",
    ]);
    never_carries_the_secret("the answer to adding an account", &added.to_string());
    let account = added["id"]
        .as_str()
        .unwrap_or_else(|| panic!("the account's id: {added}"))
        .to_string();
    assert_eq!(
        added["secrets_set"],
        json!(["token"]),
        "which fields, never a value"
    );
    let listed = ws.json(&["connector", "accounts", "status-page"]);
    never_carries_the_secret("the accounts listed", &listed.to_string());
    let checked = ws.json(&["connector", "account", "check", "status-page", &account]);
    assert_eq!(checked["state"], "connected", "{checked}");
    // A second account, made the default by its verb — and the default moves
    // to nothing nobody added.
    let spare = ws.json(&[
        "connector",
        "account",
        "add",
        "status-page",
        "--label",
        "spare",
        "--secret",
        &secret,
    ])["id"]
        .as_str()
        .expect("the second account")
        .to_string();
    ws.ok(&["connector", "account", "default", "status-page", &spare]);
    let default_of = |ws: &Sealed| -> String {
        ws.json(&["connector", "accounts", "status-page"])["accounts"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|a| a["default"] == true)
            .and_then(|a| a["id"].as_str())
            .map(str::to_string)
            .expect("one account is the default")
    };
    assert_eq!(default_of(&ws), spare);
    let nobody = ws.bisa(&[
        "connector",
        "account",
        "default",
        "status-page",
        "01ARZ3NDEKTSV4RRFFQ69G5FAV",
    ]);
    assert!(!nobody.status.success(), "an account nobody added");
    assert_eq!(default_of(&ws), spare, "and the default stands");
    ws.ok(&["connector", "account", "default", "status-page", &account]);
    assert_eq!(default_of(&ws), account);
    ws.ok(&["connector", "account", "rm", "status-page", &spare]);
    // A key account is not one a browser connects: the verb says so, and
    // asks nobody for a code.
    let no_oauth = ws.bisa(&["connector", "connect", "status-page", &account]);
    assert!(
        !no_oauth.status.success(),
        "{}",
        String::from_utf8_lossy(&no_oauth.stdout)
    );
    never_carries_the_secret("the refusal", &String::from_utf8_lossy(&no_oauth.stderr));
    let whoami = stub.seen();
    assert_eq!(whoami.len(), 1, "one request: {whoami:?}");
    assert_eq!(whoami[0].path, "/whoami");
    assert_eq!(
        whoami[0].authorization.as_deref(),
        Some(format!("Bearer {TOKEN}").as_str()),
        "the credential went to the platform, and nowhere else"
    );

    // --- a workflow that reads, asks and writes once ---------------------------------
    let workflow = recorded(&ws, "open-incident.json", &the_workflow());
    let started = ws.json(&["workflow", "run", &workflow, "--input", "title=the pager"]);
    assert_eq!(started["status"], "waiting", "{started}");
    let run = started["run"].as_str().expect("the run").to_string();
    let waiting = ws.json(&["status", &run]);
    assert_eq!(
        waiting["run"]["steps"]["how_many"]["output"], 0,
        "{waiting}"
    );
    assert_eq!(
        waiting["run"]["steps"]["ask"]["state"]["state"], "waiting",
        "the write waits for a person: {waiting}"
    );
    assert!(
        stub.incidents().is_empty(),
        "nothing was written before the person said so"
    );
    ws.ok(&["approve", &run]);
    let done = ws.until("the run to come to its end", || {
        let status = ws.json(&["status", &run]);
        (status["status"] == "done").then_some(status)
    });
    assert_eq!(done["run"]["steps"]["open"]["output"], "inc-1", "{done}");
    let opened = stub.incidents();
    assert_eq!(opened.len(), 1, "written once: {opened:?}");
    assert_eq!(opened[0]["title"], "the pager");
    let writes: Vec<Seen> = stub
        .seen()
        .into_iter()
        .filter(|s| s.method == "POST")
        .collect();
    assert_eq!(writes.len(), 1, "{writes:?}");
    assert_eq!(writes[0].body, json!({"title": "the pager"}));
    assert!(
        writes[0]
            .idempotency_key
            .as_deref()
            .is_some_and(|k| !k.is_empty()),
        "a keyed write carries its key: {writes:?}"
    );
    assert_eq!(
        writes[0].authorization.as_deref(),
        Some(format!("Bearer {TOKEN}").as_str())
    );
    ws.until("the run's word in general", || {
        general(&ws)
            .iter()
            .any(|m| m == "opened inc-1 for the pager")
            .then_some(())
    });
    never_carries_the_secret("the run's record", &done.to_string());
    never_carries_the_secret("the run's journal", &ws.json(&["log", &run]).to_string());

    // --- an agent reads through it, and is refused the write ------------------------
    let scout = ws.json(&[
        "agent",
        "add",
        "--name",
        "Scout",
        "--prompt",
        "You watch the status page.",
        "--harness",
        AGENT_HARNESS,
    ])["agent"]["id"]
        .as_str()
        .expect("the agent")
        .to_string();
    let talk = ws.json(&["conversation", "new", "workspace", "--title", "The page"]);
    let conversation = talk["conversation"]["id"]
        .as_str()
        .expect("the conversation")
        .to_string();
    ws.ok(&[
        "conversation",
        "post",
        &conversation,
        "how many incidents are open?",
        "--mention",
        &scout,
    ]);
    let tools = ws.until("the agent's four calls", || {
        let calls: Vec<Value> = ws
            .recorded("tool")
            .into_iter()
            .filter(|t| t["session"].is_string())
            .collect();
        (calls.len() >= 4).then_some(calls)
    });
    let answer = |n: usize| -> String {
        let result = &tools[n]["result"];
        result["content"][0]["text"]
            .as_str()
            .or(result["error"].as_str())
            .unwrap_or_default()
            .to_string()
    };
    assert!(
        answer(0).contains("status-page") && answer(0).contains("open"),
        "the roster names the connector and its writing operation: {}",
        answer(0)
    );
    never_carries_the_secret("the roster an agent reads", &answer(0));
    assert!(
        answer(1).contains('1'),
        "the count, read as the account: {}",
        answer(1)
    );
    let echoed = answer(2);
    never_carries_the_secret("what the platform echoed back", &echoed);
    assert!(
        !echoed.contains(TOKEN),
        "a secret the platform says back is redacted before an agent reads it: {echoed}"
    );
    let refused = answer(3);
    assert!(
        tools[3]["failed"] == true && refused.contains("open"),
        "a write is refused to the tool by name: {refused}"
    );
    assert_eq!(stub.incidents().len(), 1, "the agent opened nothing");

    // --- a workflow that polls, and begins a run per item it has not seen -------------
    let watch = recorded(&ws, "watch.json", &the_watch());
    ws.ok(&["workflow", "on", &watch]);
    let before = stub
        .seen()
        .iter()
        .filter(|s| s.path == "/incidents" && s.method == "GET")
        .count();
    ws.until("the first poll, which learns what is there", || {
        (stub
            .seen()
            .iter()
            .filter(|s| s.path == "/incidents" && s.method == "GET")
            .count()
            > before)
            .then_some(())
    });
    // Two more incidents appear at the platform; the next poll sees them.
    stub.incidents.lock().unwrap().extend([
        json!({"id": "inc-2", "title": "the lights"}),
        json!({"id": "inc-3", "title": "the door"}),
    ]);
    ws.until("a run per new incident", || {
        let said = general(&ws);
        (said.iter().any(|m| m == "incident inc-2 is open")
            && said.iter().any(|m| m == "incident inc-3 is open"))
        .then_some(())
    });
    let said = general(&ws);
    assert!(
        !said.iter().any(|m| m == "incident inc-1 is open"),
        "what was there at the first poll began nothing: {said:?}"
    );
    ws.ok(&["workflow", "off", &watch]);

    // --- the secret is nowhere a person can read --------------------------------------
    for verb in [
        vec!["connector", "show", "status-page"],
        vec!["connector", "accounts", "status-page"],
        vec!["status", &run],
        vec!["msgs", "general"],
    ] {
        never_carries_the_secret(&format!("bisa {verb:?}"), &ws.json(&verb).to_string());
    }
    ws.stop();
}
