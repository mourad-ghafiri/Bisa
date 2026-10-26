//! A connector's verbs beyond reaching a platform: a definition recorded
//! from a JSON file or from stdin, under its own id or one given on the
//! line; a connector shown with its parameters and with no account yet; an
//! account added with its parameters and a secret read from a file or from
//! stdin, and what the verb refuses — a parameter that is no pair, a secret
//! that is no field, an empty one, one at a path nobody wrote; a check with
//! a budget against a port nobody answers; the catalog's own listed by
//! origin; an OAuth consent started from the terminal, left open, then
//! finished with a pasted code; a connector removed through the daemon; and
//! with the daemon gone, an account added and made the default by hand, and
//! a connection that needs a daemon to host it.

use super::sealed::Sealed;
use serde_json::{json, Value};

/// A platform at a loopback port nobody answers: a connection is refused
/// at once, so a check is quick and no name is looked up.
fn definition(id: &str) -> Value {
    json!({
        "id": id,
        "name": "Acme",
        "description": "A stub platform nobody reaches here.",
        "base_url": "http://127.0.0.1:1",
        "hosts": ["127.0.0.1:1"],
        "auth": { "scheme": "bearer" },
        "params": [{ "name": "region", "label": "Region", "required": true, "doc": "Which region." }],
        "operations": [{
            "id": "me", "name": "Who am I", "description": "Answers the account.",
            "method": "get", "path": "/me", "output": {}, "writes": false
        }],
        "check": "me"
    })
}

/// A token endpoint on the loopback, answering every exchange with a
/// bearer token that lasts an hour.
struct TokenStub {
    host: String,
    _runtime: tokio::runtime::Runtime,
}

impl TokenStub {
    fn start() -> Self {
        use axum::routing::post;
        use axum::Router;
        let app = Router::new().route(
            "/token",
            post(|| async {
                axum::Json(json!({
                    "access_token": "stub-access-token",
                    "token_type": "bearer",
                    "expires_in": 3600,
                    "refresh_token": "stub-refresh-token"
                }))
            }),
        );
        let runtime = tokio::runtime::Runtime::new().expect("a runtime for the stub");
        let listener = runtime
            .block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))
            .expect("a loopback port");
        let host = listener.local_addr().expect("the port").to_string();
        runtime.spawn(async move {
            let _served = axum::serve(listener, app).await;
        });
        TokenStub {
            host,
            _runtime: runtime,
        }
    }
}

/// A platform that speaks OAuth, its token endpoint on the stub.
fn oauth_definition(host: &str) -> Value {
    json!({
        "id": "oauthy",
        "name": "Oauthy",
        "description": "A stub platform that speaks OAuth.",
        "base_url": format!("http://{host}"),
        "hosts": [host],
        "auth": {
            "scheme": "oauth2",
            "authorization_url": format!("http://{host}/auth"),
            "token_url": format!("http://{host}/token"),
            "scopes": ["read"],
            "pkce": true,
            "extra": {}
        },
        "operations": [{
            "id": "me", "name": "Who am I", "description": "Answers the account.",
            "method": "get", "path": "/me", "output": {}, "writes": false
        }],
        "check": "me"
    })
}

fn free_port() -> u16 {
    std::net::TcpListener::bind(("127.0.0.1", 0))
        .expect("a loopback port")
        .local_addr()
        .expect("the port")
        .port()
}

fn said_by(out: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

#[test]
fn a_connectors_verbs_through_the_daemon_and_with_it_gone() {
    let mut ws = Sealed::bare();
    ws.start();

    // --- the definition: from a file, from stdin, under which id ----------------------
    let file = ws.file("acme.json", &definition("acme").to_string());
    let path = file.to_string_lossy().to_string();
    let made = ws.json(&["connector", "new", "--from", &path]);
    assert_eq!(made["connector"]["id"], "acme", "{made}");
    let aliased = ws.json(&["connector", "new", "--from", &path, "--id", "acme-eu"]);
    assert_eq!(aliased["connector"]["id"], "acme-eu", "{aliased}");
    let said = said_by(&ws.ok(&["connector", "new", "--from", &path, "--id", "quoted"]));
    assert!(
        said.contains("connector quoted recorded") && !said.contains("\"quoted\""),
        "the id, not its JSON: {said}"
    );
    let not_one = ws.file("not-one.json", r#"{"nope": 1}"#);
    let refused = ws.refused(&["connector", "new", "--from", &not_one.to_string_lossy()]);
    assert!(refused.contains("definition"), "{refused}");
    let mut anonymous = definition("x");
    anonymous.as_object_mut().unwrap().remove("id");
    let no_id = ws.bisa_stdin(
        &["--json", "connector", "new", "--from", "-"],
        &anonymous.to_string(),
    );
    assert!(!no_id.status.success(), "{}", said_by(&no_id));
    assert!(said_by(&no_id).contains("id"), "{}", said_by(&no_id));
    let piped = ws.json_stdin(
        &["connector", "new", "--from", "-", "--id", "from-stdin"],
        &anonymous.to_string(),
    );
    assert_eq!(piped["connector"]["id"], "from-stdin", "{piped}");

    // --- shown: its parameter named, no account on this machine yet -------------------
    let shown = said_by(&ws.ok(&["connector", "show", "acme"]));
    assert!(shown.contains("region"), "{shown}");
    assert!(shown.contains("accounts (0)"), "{shown}");
    let none = said_by(&ws.ok(&["connector", "accounts", "acme"]));
    assert!(none.contains("acme"), "{none}");

    // --- accounts: a secret from a file, none, one from stdin -------------------------
    let secret_file = ws.file("acme.token", "stub-token-for-the-test\n");
    let from_file = format!("token=@{}", secret_file.to_string_lossy());
    let added = ws.json(&[
        "connector",
        "account",
        "add",
        "acme",
        "--label",
        "eu",
        "--param",
        "region=eu",
        "--secret",
        &from_file,
    ]);
    let account = added["id"]
        .as_str()
        .unwrap_or_else(|| panic!("the account's id: {added}"))
        .to_string();
    assert_eq!(added["secrets_set"], json!(["token"]), "{added}");
    let bare = ws.json(&[
        "connector",
        "account",
        "add",
        "acme",
        "--label",
        "bare",
        "--param",
        "region=us",
    ]);
    assert_eq!(bare["secrets_set"], json!([]), "{bare}");
    let piped = ws.json_stdin(
        &[
            "connector",
            "account",
            "add",
            "acme",
            "--label",
            "piped",
            "--param",
            "region=us",
            "--secret",
            "token=@stdin",
        ],
        "piped-token\n",
    );
    assert_eq!(piped["secrets_set"], json!(["token"]), "{piped}");
    let listed = said_by(&ws.ok(&["connector", "accounts", "acme"]));
    assert!(
        listed.contains("none"),
        "the bare account's secrets: {listed}"
    );
    assert_eq!(
        ws.json(&["connector", "accounts", "acme"])["accounts"]
            .as_array()
            .map(Vec::len),
        Some(3)
    );

    // --- what the verb refuses, by name -----------------------------------------------
    let add = |rest: &[&str]| {
        let mut args = vec!["connector", "account", "add", "acme", "--label", "x"];
        args.extend_from_slice(rest);
        ws.refused(&args)
    };
    let no_pair = add(&["--param", "region"]);
    assert!(no_pair.contains("region"), "{no_pair}");
    let no_field = add(&["--param", "region=eu", "--secret", "nope=x"]);
    assert!(no_field.contains("nope"), "{no_field}");
    let empty = add(&["--param", "region=eu", "--secret", "token="]);
    assert!(empty.contains("token"), "{empty}");
    let nowhere = add(&[
        "--param",
        "region=eu",
        "--secret",
        "token=@/definitely/missing/token.txt",
    ]);
    assert!(
        nowhere.contains("/definitely/missing/token.txt"),
        "{nowhere}"
    );
    let not_id = ws.refused(&["connector", "account", "check", "acme", "not-an-id"]);
    assert!(not_id.contains("not-an-id"), "{not_id}");

    // --- a check with a budget: the port answers nobody -------------------------------
    let checked = ws.json(&[
        "connector",
        "account",
        "check",
        "acme",
        &account,
        "--timeout-secs",
        "1",
    ]);
    assert_eq!(checked["state"], "unreachable", "{checked}");

    // --- the catalog's own, listed by origin ------------------------------------------
    ws.ok(&["catalog", "install", "connector", "confluence"]);
    let all = said_by(&ws.ok(&["connector", "list"]));
    assert!(all.contains("catalog:confluence"), "{all}");

    // --- OAuth from the terminal: left open, then finished with a pasted code ---------
    let tokens = TokenStub::start();
    let port = free_port().to_string();
    ws.ok(&["settings", "set", "machine", "connectors.oauth.port", &port]);
    let oauth_file = ws.file("oauthy.json", &oauth_definition(&tokens.host).to_string());
    ws.json(&["connector", "new", "--from", &oauth_file.to_string_lossy()]);
    let me = ws.json(&[
        "connector",
        "account",
        "add",
        "oauthy",
        "--label",
        "me",
        "--secret",
        "client_id=my-app",
    ]);
    let me_id = me["id"].as_str().expect("the account's id").to_string();
    let left_open = ws.json(&["connector", "connect", "oauthy", &me_id]);
    assert!(
        left_open["url"]
            .as_str()
            .unwrap_or_default()
            .contains("/auth?"),
        "{left_open}"
    );
    // The code pasted and entered, the pipe kept open as a terminal keeps
    // it: the verb ends on the line, never waiting for an Enter nobody owes.
    let (mut child, stdin) = ws.bisa_stdin_open(
        &["--json", "connector", "connect", "oauthy", &me_id],
        "the-code\n",
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while child.try_wait().expect("the verb's state").is_none() {
        assert!(
            std::time::Instant::now() < deadline,
            "connect did not end on the pasted line while stdin stayed open"
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    drop(stdin);
    let out = child.wait_with_output().expect("the verb ends");
    assert!(out.status.success(), "{}", said_by(&out));
    let done: Value = serde_json::from_slice(&out.stdout).expect("JSON");
    assert_eq!(done["connected"], true, "{done}");
    // The browser lands while the verb waits with the pipe open and nothing
    // on it: the verb ends on the landing — read off the account's row,
    // whose health the reconnect forgets — with no Enter owed. A second
    // connect's paste stands in for the browser here; a check first gives
    // the row a health for the landing to forget.
    ws.json(&[
        "connector",
        "account",
        "check",
        "oauthy",
        &me_id,
        "--timeout-secs",
        "1",
    ]);
    let (mut waiting, open) =
        ws.bisa_stdin_open(&["--json", "connector", "connect", "oauthy", &me_id], "");
    // The waiting verb has read the row and started its flow before the
    // landing moves the row under it.
    std::thread::sleep(std::time::Duration::from_millis(1500));
    let again = ws.json_stdin(&["connector", "connect", "oauthy", &me_id], "the-code\n");
    assert_eq!(again["connected"], true, "{again}");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while waiting.try_wait().expect("the verb's state").is_none() {
        assert!(
            std::time::Instant::now() < deadline,
            "connect did not end on the landing while stdin stayed open"
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    drop(open);
    let out = waiting.wait_with_output().expect("the verb ends");
    assert!(out.status.success(), "{}", said_by(&out));
    let landed: Value = serde_json::from_slice(&out.stdout).expect("JSON");
    assert_eq!(landed["connected"], true, "ended on the landing: {landed}");
    let connected = said_by(&ws.ok(&["connector", "accounts", "oauthy"]));
    assert!(connected.contains("expires"), "{connected}");

    // --- removed through the daemon ---------------------------------------------------
    let removed = ws.json(&["connector", "rm", "acme-eu"]);
    assert!(removed.is_object(), "{removed}");
    assert!(ws
        .refused(&["connector", "show", "acme-eu"])
        .contains("acme-eu"));

    // --- with the daemon gone ---------------------------------------------------------
    ws.stop();
    let off = ws.json(&[
        "connector",
        "account",
        "add",
        "acme",
        "--label",
        "off",
        "--param",
        "region=us",
    ]);
    assert_eq!(off["secrets_set"], json!([]), "{off}");
    assert_eq!(
        off["token_source"],
        json!("file"),
        "one row shape with or without the daemon: {off}"
    );
    assert_eq!(off["health"]["state"], json!("unknown"), "{off}");
    assert!(off.get("expires_at").is_some(), "{off}");
    let undeclared = ws.refused(&[
        "connector",
        "account",
        "add",
        "acme",
        "--label",
        "bad",
        "--param",
        "nope=1",
    ]);
    assert!(undeclared.contains("nope"), "{undeclared}");
    let chosen = ws.json(&["connector", "account", "default", "acme", &account]);
    assert_eq!(chosen["default"], json!(account), "{chosen}");
    let needs_node = ws.refused(&["connector", "connect", "oauthy", &me_id]);
    assert!(needs_node.to_lowercase().contains("node"), "{needs_node}");
}
