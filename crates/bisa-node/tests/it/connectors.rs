//! Connectors over HTTP: a definition validates before it lands, an account's
//! rows say which secret fields are set and never a value, the OAuth callback
//! listener comes up on the configured port for a flow and goes away after
//! it, its page never echoes a code, and it is the one connector route that
//! answers without the token.

use crate::node::Node;
use http_body_util::{BodyExt as _, Full};
use hyper::body::Bytes;
use hyper::Request;
use hyper_util::rt::TokioIo;
use serde_json::{json, Value};
use std::time::Duration;

/// A loopback stub standing in for a platform: a token endpoint and a
/// `check` endpoint. Nothing real is reached.
async fn platform_stub() -> (String, tokio::task::JoinHandle<()>) {
    use axum::routing::{get, post};
    let app = axum::Router::new()
        .route(
            "/token",
            post(|| async {
                axum::Json(json!({
                    "access_token": "stub-access-token",
                    "token_type": "bearer",
                    "expires_in": 3600,
                    "refresh_token": "stub-refresh-token"
                }))
            }),
        )
        .route("/me", get(|| async { axum::Json(json!({"ok": true})) }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        let _served = axum::serve(listener, app).await;
    });
    (format!("127.0.0.1:{}", addr.port()), task)
}

fn free_port() -> u16 {
    std::net::TcpListener::bind(("127.0.0.1", 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// A bearer-auth definition over the stub, with one `check` operation.
fn bearer_definition(host: &str) -> Value {
    json!({
        "id": "acme",
        "name": "Acme",
        "description": "A stub platform.",
        "tags": ["ops"],
        "base_url": format!("http://{host}"),
        "hosts": [host],
        "auth": { "scheme": "bearer" },
        "operations": [{
            "id": "me",
            "name": "Who am I",
            "description": "Answers the account.",
            "method": "get",
            "path": "/me",
            "output": {},
            "writes": false
        }],
        "check": "me"
    })
}

fn oauth_definition(host: &str) -> Value {
    json!({
        "id": "acme-oauth",
        "name": "Acme OAuth",
        "description": "A stub platform that speaks OAuth.",
        "tags": ["ops"],
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
            "id": "me",
            "name": "Who am I",
            "description": "Answers the account.",
            "method": "get",
            "path": "/me",
            "output": {},
            "writes": false
        }],
        "check": "me"
    })
}

/// One raw request over TCP to the callback listener: status and body text.
async fn tcp_get(port: u16, path: &str) -> std::io::Result<(u16, String)> {
    let stream = tokio::net::TcpStream::connect(("127.0.0.1", port)).await?;
    let (mut sender, conn) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
        .await
        .map_err(std::io::Error::other)?;
    tokio::spawn(conn);
    let request = Request::builder()
        .method("GET")
        .uri(path)
        .header(hyper::header::HOST, "127.0.0.1")
        .body(Full::new(Bytes::new()))
        .unwrap();
    let resp = sender
        .send_request(request)
        .await
        .map_err(std::io::Error::other)?;
    let status = resp.status().as_u16();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    Ok((status, String::from_utf8_lossy(&bytes).into_owned()))
}

fn query_param(url: &str, name: &str) -> Option<String> {
    let (_, query) = url.split_once('?')?;
    query.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=')?;
        (k == name).then(|| v.to_string())
    })
}

/// A connector or an account nobody recorded is not found; one whose file
/// was cut short is said as that, by its file, and the lists go on without
/// it — never a bare parse error, never a refusal of the request.
#[tokio::test(flavor = "multi_thread")]
async fn a_connector_nobody_recorded_is_not_found_and_one_cut_short_is_said_by_its_file() {
    let node = Node::start().await;
    let (host, _stub) = platform_stub().await;
    let said = |v: &Value| v["error"].as_str().unwrap_or_default().to_string();

    let (status, v) = node.req("GET", "/connectors/nobody", None).await;
    assert_eq!(status, 404, "{v}");
    assert!(said(&v).contains("nobody"), "{v}");
    let (status, v) = node
        .req(
            "POST",
            "/connectors/nobody/accounts/01ARZ3NDEKTSV4RRFFQ69G5FAV/check",
            Some(json!({})),
        )
        .await;
    assert_eq!(status, 404, "{v}");

    node.post("/connectors", bearer_definition(&host)).await;
    let mut other = bearer_definition(&host);
    other["id"] = json!("beta");
    other["name"] = json!("Beta");
    node.post("/connectors", other).await;
    let row = node
        .put(
            "/connectors/acme/accounts",
            json!({"label": "work", "secrets": {"token": "acme-token-that-must-never-show"}}),
        )
        .await;
    let aid = row["id"].as_str().unwrap().to_string();
    let (status, v) = node
        .req(
            "POST",
            "/connectors/acme/accounts/01ARZ3NDEKTSV4RRFFQ69G5FAV/check",
            Some(json!({})),
        )
        .await;
    assert_eq!(status, 404, "an account nobody added: {v}");

    // Written over by something that is no record — another shape of the
    // code, a disk that lost the end of the file.
    let paths = node.ws.paths();
    let connector = paths.connector_file(&bisa_core::ConnectorId::new("beta").unwrap());
    let account = paths.connector_account_file(
        &bisa_core::ConnectorId::new("acme").unwrap(),
        aid.parse().unwrap(),
    );
    for file in [&connector, &account] {
        assert!(file.is_file(), "{}", file.display());
        std::fs::write(file, b"{ \"id\": ").unwrap();
    }
    let (status, v) = node.req("GET", "/connectors/beta", None).await;
    assert_eq!(status, 404, "{v}");
    assert!(
        said(&v).contains("beta.json") && !said(&v).contains("not found"),
        "names the file it could not read: {v}"
    );
    let listed = node.get("/connectors").await;
    let ids: Vec<&str> = listed["connectors"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|c| c["connector"]["id"].as_str().or(c["id"].as_str()))
        .collect();
    assert_eq!(ids, ["acme"], "the list goes on without it: {listed}");
    let accounts = node.get("/connectors/acme/accounts").await;
    assert_eq!(accounts["accounts"], json!([]), "{accounts}");
    let (status, v) = node
        .req(
            "POST",
            &format!("/connectors/acme/accounts/{aid}/check"),
            Some(json!({})),
        )
        .await;
    assert_eq!(status, 404, "{v}");
    assert!(said(&v).contains(&format!("{aid}.json")), "{v}");
    node.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn connector_accounts_carry_which_fields_are_set_and_never_a_value() {
    let node = Node::start().await;
    let (host, _stub) = platform_stub().await;
    let created = node.post("/connectors", bearer_definition(&host)).await;
    assert_eq!(created["connector"]["id"], "acme");
    assert_eq!(created["connector"]["auth"], "bearer");
    assert_eq!(created["connector"]["accounts"], 0);

    let secret = "acme-token-that-must-never-show";
    let row = node
        .put(
            "/connectors/acme/accounts",
            json!({"label": "work", "secrets": {"token": secret}}),
        )
        .await;
    assert_eq!(row["label"], "work");
    assert_eq!(row["default"], true, "the first account is the default");
    assert_eq!(row["secrets_set"], json!(["token"]));
    assert_eq!(row["token_source"], "file");
    assert!(
        row.get("oauth").is_none(),
        "a bearer account has no OAuth facts"
    );
    let aid = row["id"].as_str().unwrap().to_string();

    for path in [
        "/connectors",
        "/connectors/acme",
        "/connectors/acme/accounts",
    ] {
        let text = node.get(path).await.to_string();
        assert!(!text.contains(secret), "{path} leaks the token: {text}");
    }
    let detail = node.get("/connectors/acme").await;
    assert_eq!(detail["accounts"][0]["id"], aid);
    assert_eq!(detail["connector"]["operations"][0]["id"], "me");

    // A field the scheme does not have is refused by name.
    let (status, v) = node
        .req(
            "PUT",
            "/connectors/acme/accounts",
            Some(json!({"id": aid, "label": "work", "secrets": {"api_key": "k"}})),
        )
        .await;
    assert_eq!(status, 400, "{v}");
    assert!(v["error"].as_str().unwrap().contains("api_key"), "{v}");

    // The check runs the one operation against the stub.
    let check = node
        .post(&format!("/connectors/acme/accounts/{aid}/check"), json!({}))
        .await;
    assert_eq!(check["state"], "connected", "{check}");
    assert_eq!(check["status"], 200);

    // Forgotten: the row is gone, and so is the token.
    node.delete(&format!("/connectors/acme/accounts/{aid}"))
        .await;
    assert_eq!(
        node.get("/connectors/acme/accounts").await["accounts"],
        json!([])
    );
    node.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn validate_answers_problems_without_saving() {
    let node = Node::start().await;
    let (host, _stub) = platform_stub().await;
    let mut bad = bearer_definition(&host);
    bad["hosts"] = json!([]);
    let v = node.post("/connectors/validate", bad.clone()).await;
    assert_eq!(v["ok"], false);
    assert!(!v["problems"].as_array().unwrap().is_empty(), "{v}");
    assert!(
        v["problems"][0]["text"]["id"].is_string(),
        "a problem is a message, not prose"
    );
    assert_eq!(
        node.get("/connectors").await["connectors"],
        json!([]),
        "nothing was saved"
    );

    // The same definition refused on create, with the problems typed.
    let (status, e) = node.req("POST", "/connectors", Some(bad)).await;
    assert_eq!(status, 400, "{e}");
    assert!(
        e["detail"]["problems"]
            .as_array()
            .is_some_and(|p| !p.is_empty()),
        "{e}"
    );

    let good = node
        .post("/connectors/validate", bearer_definition(&host))
        .await;
    assert_eq!(good, json!({"ok": true, "problems": []}));
    node.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_catalog_connector_cannot_be_put_and_a_used_one_cannot_be_deleted() {
    let node = Node::start().await;
    let installed = node
        .post(
            "/catalog/install",
            json!({"kind": "connector", "slug": "slack"}),
        )
        .await;
    assert_eq!(installed["installed"]["connectors"], json!(["slack"]));
    let slack = node.get("/connectors/slack").await;
    assert_eq!(
        slack["connector"]["origin"],
        json!({"catalog": {"slug": "slack"}})
    );

    // The catalog's is not edited here.
    let mut body = slack["connector"].clone();
    body.as_object_mut().unwrap().remove("origin");
    body.as_object_mut().unwrap().remove("created_at");
    body["name"] = json!("Renamed");
    let (status, v) = node.req("PUT", "/connectors/slack", Some(body)).await;
    assert_eq!(status, 409, "{v}");

    // An account makes it used; so does a step.
    let row = node
        .put(
            "/connectors/slack/accounts",
            json!({"label": "team", "secrets": {"token": "xoxb-stub"}}),
        )
        .await;
    let aid = row["id"].as_str().unwrap().to_string();
    let (status, v) = node.req("DELETE", "/connectors/slack", None).await;
    assert_eq!(status, 409, "{v}");
    assert!(v["error"].as_str().unwrap().contains("team"), "{v}");
    node.delete(&format!("/connectors/slack/accounts/{aid}"))
        .await;
    let wf = node
        .workflow(json!({
            "name": "tell slack",
            "steps": [{
                "id": "tell", "name": "Tell", "kind": "connector",
                "connector": "slack", "operation": "post_message",
                "params": {"channel": "general", "text": "hello"}
            }]
        }))
        .await;
    let (status, v) = node.req("DELETE", "/connectors/slack", None).await;
    assert_eq!(status, 409, "{v}");
    assert!(v["error"].as_str().unwrap().contains("tell slack"), "{v}");
    node.delete(&format!("/workflows/{wf}")).await;
    node.delete("/connectors/slack").await;
    assert_eq!(node.get("/connectors").await["connectors"], json!([]));
    node.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_callback_is_token_exempt_and_everything_else_under_connectors_is_not() {
    let node = Node::start().await;
    // Without the token: the callback answers (a 400 for a stale link), the
    // rest is 401.
    let (status, _) = node
        .req_without_token("GET", "/connectors/oauth/callback?state=nope&code=x")
        .await;
    assert_eq!(status, 400);
    for path in [
        "/connectors",
        "/connectors/slack",
        "/connectors/slack/accounts",
    ] {
        let (status, _) = node.req_without_token("GET", path).await;
        assert_eq!(status, 401, "{path}");
    }
    node.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn oauth_start_binds_the_configured_port_and_stops_after_completion() {
    let node = Node::start().await;
    let (host, _stub) = platform_stub().await;
    let port = free_port();
    node.put(
        "/settings/machine",
        json!({"values": {"connectors.oauth.port": port}}),
    )
    .await;
    node.post("/connectors", oauth_definition(&host)).await;
    let row = node
        .put(
            "/connectors/acme-oauth/accounts",
            json!({"label": "me", "secrets": {"client_id": "stub-client", "client_secret": "stub-secret"}}),
        )
        .await;
    let aid = row["id"].as_str().unwrap().to_string();
    assert_eq!(row["oauth"]["expired"], false);
    assert!(row["oauth"].get("expires_at").is_none());

    // Nothing listens on the port before a flow starts.
    assert!(tcp_get(port, "/connectors/oauth/callback").await.is_err());

    let started = node
        .post(
            &format!("/connectors/acme-oauth/accounts/{aid}/oauth/start"),
            json!({}),
        )
        .await;
    let url = started["url"].as_str().unwrap().to_string();
    assert!(url.starts_with(&format!("http://{host}/auth?")), "{url}");
    assert_eq!(
        started["redirect_uri"],
        format!("http://127.0.0.1:{port}/connectors/oauth/callback")
    );
    let state = query_param(&url, "state").expect("a state");
    assert!(query_param(&url, "code_challenge").is_some(), "PKCE");

    // The listener is up: a stale state is a 400 page that echoes nothing.
    let (status, body) = tcp_get(
        port,
        "/connectors/oauth/callback?state=stale&code=SECRETCODE",
    )
    .await
    .expect("the callback listener is up");
    assert_eq!(status, 400);
    assert!(body.contains("stale or unknown"), "{body}");
    assert!(!body.contains("SECRETCODE"));
    assert!(!body.contains("<script"));

    // The browser comes back: the exchange runs against the stub, the tokens
    // land in the keystore, and the page says so without the code.
    let (status, body) = tcp_get(
        port,
        &format!("/connectors/oauth/callback?state={state}&code=THECODE"),
    )
    .await
    .unwrap();
    assert_eq!(status, 200, "{body}");
    assert!(body.contains("Connected"), "{body}");
    assert!(!body.contains("THECODE"));
    let rows = node.get("/connectors/acme-oauth/accounts").await;
    let mine = &rows["accounts"][0];
    let set = mine["secrets_set"].as_array().unwrap();
    assert!(
        set.contains(&json!("access_token")) && set.contains(&json!("refresh_token")),
        "{mine}"
    );
    assert!(mine["oauth"]["expires_at"].as_u64().is_some());
    assert_eq!(mine["oauth"]["expired"], false);
    assert!(!rows.to_string().contains("stub-access-token"));

    // No flow pending: the listener is gone.
    let mut down = false;
    for _ in 0..50 {
        if tcp_get(port, "/connectors/oauth/callback").await.is_err() {
            down = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(down, "the callback listener stayed up after the last flow");

    // The pasted-code door completes a started flow too.
    node.post(
        &format!("/connectors/acme-oauth/accounts/{aid}/oauth/start"),
        json!({}),
    )
    .await;
    let done = node
        .post(
            &format!("/connectors/acme-oauth/accounts/{aid}/oauth/complete"),
            json!({"code": "PASTED"}),
        )
        .await;
    assert_eq!(done["connected"], true);
    let (status, v) = node
        .req(
            "POST",
            &format!("/connectors/acme-oauth/accounts/{aid}/oauth/complete"),
            Some(json!({"code": "AGAIN"})),
        )
        .await;
    assert_eq!(status, 400, "no flow is pending any more: {v}");
    node.shutdown().await;
}
