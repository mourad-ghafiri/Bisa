//! `bisa connector …` end to end against a loopback stub standing in for the
//! platform: a custom definition recorded from TOML, shown, its account
//! added with a secret read from a file, checked, made default, forgotten,
//! and the definition removed only once nothing names it. No real host, no
//! keychain (`--file-keys`), no secret ever on a command line.

use crate::cli::{assert_ok, bisa, init, json, stdout};
use std::path::Path;
use std::sync::{Arc, Mutex};

/// The stub platform: `/whoami` answers 200 with a JSON body while `ok` is
/// true and 401 otherwise; every call's `Authorization` is remembered.
struct Stub {
    base_url: String,
    host: String,
    seen: Arc<Mutex<Vec<Option<String>>>>,
    ok: Arc<Mutex<bool>>,
}

async fn start_stub() -> Stub {
    use axum::extract::State;
    use axum::http::{HeaderMap, StatusCode};
    use axum::routing::get;
    use axum::Router;
    #[derive(Clone)]
    struct Shared {
        seen: Arc<Mutex<Vec<Option<String>>>>,
        ok: Arc<Mutex<bool>>,
    }
    async fn whoami(
        State(s): State<Shared>,
        headers: HeaderMap,
    ) -> (StatusCode, axum::Json<serde_json::Value>) {
        let auth = headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        s.seen.lock().unwrap().push(auth);
        if *s.ok.lock().unwrap() {
            (
                StatusCode::OK,
                axum::Json(serde_json::json!({"user": "stub"})),
            )
        } else {
            (
                StatusCode::UNAUTHORIZED,
                axum::Json(serde_json::json!({"message": "no"})),
            )
        }
    }
    let shared = Shared {
        seen: Arc::new(Mutex::new(Vec::new())),
        ok: Arc::new(Mutex::new(true)),
    };
    let app = Router::new()
        .route("/whoami", get(whoami))
        .with_state(shared.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _served = axum::serve(listener, app).await;
    });
    Stub {
        base_url: format!("http://{addr}"),
        host: addr.to_string(),
        seen: shared.seen,
        ok: shared.ok,
    }
}

fn definition(dir: &Path, stub: &Stub) -> String {
    let path = dir.join("stubchat.toml");
    std::fs::write(
        &path,
        format!(
            r#"[connector]
name = "Stub chat"
description = "A chat platform on the loopback stub."
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
timeout_secs = 20

[[connector.operations]]
id = "history"
name = "History"
description = "The messages, paged."
method = "get"
path = "/history"
query = {{ cursor = "{{params.cursor}}" }}
output = {{ select = "messages" }}
page = {{ cursor_param = "cursor", next_cursor = "next", max_pages = 5 }}

[[connector.operations.params]]
name = "cursor"
label = "Cursor"
doc = "Where the page before ended."

[[connector.operations]]
id = "post"
name = "Post"
description = "Posts a message."
method = "post"
path = "/post"
body = {{ kind = "json", value = {{ text = "{{params.text}}" }} }}
writes = true
idempotency = {{ header = "Idempotency-Key" }}

[[connector.operations.params]]
name = "text"
label = "Text"
required = true
doc = "One line."
"#,
            base = stub.base_url,
            host = stub.host
        ),
    )
    .unwrap();
    path.to_string_lossy().into_owned()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_connector_is_recorded_shown_given_an_account_checked_and_forgotten() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());
    let stub = start_stub().await;
    let from = definition(dir.path(), &stub);

    // Recorded from TOML, under the file's stem.
    let recorded = bisa(dir.path(), &["--json", "connector", "new", "--from", &from]);
    assert_ok(&recorded, "connector new");
    assert_eq!(json(&recorded)["connector"]["id"], "stubchat");

    // Listed, and shown with how each operation holds up.
    let listed = bisa(dir.path(), &["--json", "connector", "list"]);
    assert_ok(&listed, "connector list");
    assert!(
        json(&listed)["connectors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["connector"]["id"] == "stubchat"),
        "{}",
        stdout(&listed)
    );
    let shown = bisa(dir.path(), &["connector", "show", "stubchat"]);
    assert_ok(&shown, "connector show");
    let text = stdout(&shown);
    assert!(text.contains("timeout 20s"), "{text}");
    assert!(text.contains("idempotency Idempotency-Key"), "{text}");
    assert!(text.contains("pages by cursor ← next (≤5)"), "{text}");
    assert!(
        text.contains("(writes;") || text.contains("(writes)"),
        "{text}"
    );
    let shown_json = bisa(dir.path(), &["--json", "connector", "show", "stubchat"]);
    let op = &json(&shown_json)["connector"]["operations"][2];
    assert_eq!(op["idempotency"]["header"], "Idempotency-Key");
    assert_eq!(
        json(&shown_json)["connector"]["operations"][1]["page"]["max_pages"],
        5
    );

    // An account, its secret read from a file — never from the command line.
    let secret_file = dir.path().join("token.txt");
    std::fs::write(&secret_file, "xoxb-not-a-real-token\n").unwrap();
    let added = bisa(
        dir.path(),
        &[
            "--json",
            "connector",
            "account",
            "add",
            "stubchat",
            "--label",
            "work",
            "--secret",
            &format!("token=@{}", secret_file.display()),
        ],
    );
    assert_ok(&added, "account add");
    let account = json(&added)["id"].as_str().unwrap().to_string();
    let accounts = bisa(dir.path(), &["--json", "connector", "accounts", "stubchat"]);
    assert_ok(&accounts, "accounts");
    let listing = stdout(&accounts);
    assert!(listing.contains("token"), "the field is named: {listing}");
    assert!(
        !listing.contains("xoxb-not-a-real-token"),
        "never the value: {listing}"
    );

    // One live request as the account: connected, then refused.
    let checked = bisa(
        dir.path(),
        &[
            "--json",
            "connector",
            "account",
            "check",
            "stubchat",
            &account,
        ],
    );
    assert_ok(&checked, "account check");
    assert_eq!(json(&checked)["state"], "connected", "{}", stdout(&checked));
    assert_eq!(
        stub.seen
            .lock()
            .unwrap()
            .last()
            .cloned()
            .flatten()
            .as_deref(),
        Some("Bearer xoxb-not-a-real-token"),
        "the secret travelled once, to the platform"
    );
    *stub.ok.lock().unwrap() = false;
    let refused = bisa(
        dir.path(),
        &[
            "--json",
            "connector",
            "account",
            "check",
            "stubchat",
            &account,
        ],
    );
    assert_ok(&refused, "account check (refused)");
    assert_eq!(json(&refused)["state"], "refused", "{}", stdout(&refused));

    // The definition stays while an account names it; forgotten, it goes.
    let kept = bisa(dir.path(), &["connector", "rm", "stubchat"]);
    assert!(
        !kept.status.success(),
        "rm is refused while an account exists"
    );
    let forgotten = bisa(
        dir.path(),
        &["--json", "connector", "account", "rm", "stubchat", &account],
    );
    assert_ok(&forgotten, "account rm");
    let removed = bisa(dir.path(), &["--json", "connector", "rm", "stubchat"]);
    assert_ok(&removed, "connector rm");
    assert_eq!(json(&removed)["removed"], true);
}

#[test]
fn a_definition_that_breaks_a_rule_is_refused_by_name_and_nothing_is_recorded() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());
    let path = dir.path().join("bad.toml");
    std::fs::write(
        &path,
        r#"[connector]
name = "Bad"
description = "A read that pages a write."
base_url = "https://api.example.com"
hosts = ["api.example.com"]
auth = { scheme = "bearer" }
check = "post"

[[connector.operations]]
id = "post"
name = "Post"
description = "Posts."
method = "post"
path = "/post"
writes = true
idempotency = { header = "Authorization" }
page = { cursor_param = "cursor", next_cursor = "next", max_pages = 99 }
"#,
    )
    .unwrap();
    let out = bisa(
        dir.path(),
        &["connector", "new", "--from", &path.to_string_lossy()],
    );
    assert!(!out.status.success(), "refused");
    let text = format!("{}{}", stdout(&out), String::from_utf8_lossy(&out.stderr));
    assert!(text.contains("idempotency.header"), "{text}");
    assert!(text.contains("page"), "{text}");
    assert!(
        text.contains("check"),
        "a writing check is refused too: {text}"
    );
    let listed = bisa(dir.path(), &["--json", "connector", "list"]);
    assert!(
        !json(&listed)["connectors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["id"] == "bad"),
        "nothing recorded"
    );
}
