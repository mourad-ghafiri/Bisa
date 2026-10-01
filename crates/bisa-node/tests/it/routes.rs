//! Every route the reference documents is mounted.
//!
//! The `ROUTES` tables beside each module's `routes()` are what
//! `docs/reference/http-api.md` is rendered from. This drives every one of
//! them through a running node with placeholder ids and asserts none comes
//! back as axum's own 404 (empty body) or a 405: a handler's 404 carries an
//! `{"error"}` body and is a different thing. `GET /events` is a stream and is
//! not requested.

use bisa_engine::{Engine, EngineConfig};
use bisa_harness::HarnessCatalog;
use bisa_node::{route_docs, serve, NodeConfig};
use bisa_store::{FileKeyStore, Paths, Workspace};
use http_body_util::{BodyExt as _, Full};
use hyper::body::Bytes;
use hyper::Request;
use hyper_util::rt::TokioIo;
use std::path::PathBuf;
use std::time::Duration;
use tokio::net::UnixStream;

/// The control-plane token every test node is started with, and every request
/// in this file presents. One per process: the value is arbitrary.
const TOKEN: &str = "test-token-0123456789abcdef";

async fn boot() -> (tempfile::TempDir, PathBuf, tokio::sync::oneshot::Sender<()>) {
    let dir = tempfile::tempdir().expect("tempdir");
    let data = dir.path().to_path_buf();
    let ws = Workspace::open_with_keystore(
        &data,
        Box::new(FileKeyStore::new(Paths::new(&data).identity_dir())),
    )
    .expect("workspace");
    let engine = Engine::start(
        ws,
        HarnessCatalog::new(),
        EngineConfig {
            design_enabled: false,
            ..Default::default()
        },
    )
    .expect("engine");
    let socket = Paths::new(&data).node_socket();
    let (stop, stop_rx) = tokio::sync::oneshot::channel::<()>();
    let cfg = NodeConfig {
        socket: socket.clone(),
        http: None,
        data_dir: data,
        collab: None,
        fetch_attachment: None,
        token: Some(TOKEN.to_string()),
        // Built with the feature, the reference documents the A2A routes:
        // the node this guard asks exposes them, so they are mounted. The
        // address is said on the card and never dialled.
        #[cfg(feature = "a2a")]
        a2a: Some(bisa_node::a2a::A2aExposeConfig::new("http://127.0.0.1:1")),
    };
    tokio::spawn(serve(engine, cfg, async {
        let _stopped_or_dropped = stop_rx.await;
    }));
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
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(actual.exists(), "node socket never appeared");
    (dir, actual, stop)
}

async fn request(socket: &std::path::Path, method: &str, path: &str) -> (u16, Vec<u8>) {
    request_as(socket, method, path, Some(&format!("Bearer {TOKEN}"))).await
}

/// One request with exactly the `Authorization` given — or none.
async fn request_as(
    socket: &std::path::Path,
    method: &str,
    path: &str,
    authorization: Option<&str>,
) -> (u16, Vec<u8>) {
    let stream = UnixStream::connect(socket).await.expect("connect");
    let (mut sender, conn) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
        .await
        .expect("handshake");
    tokio::spawn(conn);
    let has_body = matches!(method, "POST" | "PUT" | "PATCH" | "DELETE");
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header(hyper::header::HOST, "localhost")
        .header(hyper::header::CONTENT_TYPE, "application/json");
    if let Some(value) = authorization {
        builder = builder.header(hyper::header::AUTHORIZATION, value);
    }
    let request = builder
        .body(Full::new(Bytes::from(if has_body { "{}" } else { "" })))
        .unwrap();
    let resp = sender.send_request(request).await.expect("request");
    let status = resp.status().as_u16();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (status, bytes.to_vec())
}

/// One request with no `Authorization` at all, answering the status and the
/// `WWW-Authenticate` challenge — the gate's 401 carries `Bearer`; a route's
/// own answer (a public hook's 404 while this machine allows none, its 401
/// for a signature that did not match) carries none.
async fn request_open(socket: &std::path::Path, method: &str, path: &str) -> (u16, Option<String>) {
    let stream = UnixStream::connect(socket).await.expect("connect");
    let (mut sender, conn) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
        .await
        .expect("handshake");
    tokio::spawn(conn);
    let has_body = matches!(method, "POST" | "PUT" | "PATCH" | "DELETE");
    let request = Request::builder()
        .method(method)
        .uri(path)
        .header(hyper::header::HOST, "localhost")
        .header(hyper::header::CONTENT_TYPE, "application/json")
        .body(Full::new(Bytes::from(if has_body { "{}" } else { "" })))
        .unwrap();
    let resp = sender.send_request(request).await.expect("request");
    let challenge = resp
        .headers()
        .get(hyper::header::WWW_AUTHENTICATE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    (resp.status().as_u16(), challenge)
}

/// A placeholder that parses wherever the route wants an id, so a miss is the
/// handler's 404 and not an extractor's 400 — either is fine here, but a
/// well-formed id keeps the test honest about what it is asserting.
fn fill(path: &str) -> String {
    let ulid = "01J0000000000000000000000A";
    path.replace("{*path}", "index.html")
        .replace("{id}", ulid)
        .replace("{item}", ulid)
        .replace("{pid}", ulid)
        .replace("{name}", "feature")
        .replace("{login}", "octocat")
        .replace("{sha}", &"a".repeat(40))
        .replace("{wid}", ulid)
        .replace("{host}", &format!("workspace:{ulid}"))
        .replace("{skill_id}", "acceptance-criteria")
        .replace("{mcp_id}", "some-server")
        .replace("{kind}", "agent")
        .replace("{slug}", "developer")
        .replace("{scope}", "goal")
        .replace("{sha256}", &"a".repeat(64))
        .replace("{wfid}", ulid)
        .replace("{cid}", "slack")
        .replace("{aid}", ulid)
        .replace("{step}", "ask")
        .replace("{run}", ulid)
        .replace("{rid}", ulid)
}

#[tokio::test(flavor = "multi_thread")]
async fn every_documented_route_is_mounted() {
    let (_dir, socket, _stop) = boot().await;
    let mut unmounted = Vec::new();
    for r in route_docs::all() {
        if r.path == "/events" {
            continue;
        }
        let (status, body) = request(&socket, r.method, &fill(r.path)).await;
        let router_miss = status == 405 || (status == 404 && body.is_empty());
        if router_miss {
            unmounted.push(format!("{} {} → {status}", r.method, r.path));
        }
    }
    assert!(
        unmounted.is_empty(),
        "documented but not mounted:\n{}",
        unmounted.join("\n")
    );
}

#[test]
fn the_reference_covers_every_module_that_mounts_routes() {
    // Each module with a `routes()` fn has a `ROUTES` table that reaches
    // `sections()`. Read as text, the way the layering tests do.
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let gathered = std::fs::read_to_string(src.join("route_docs.rs")).unwrap();
    for entry in std::fs::read_dir(&src).unwrap() {
        let path = entry.unwrap().path();
        let Some(name) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        if text.contains("fn routes() -> Router<Shared>") {
            assert!(
                text.contains("pub const ROUTES: &[RouteDoc]"),
                "{name}.rs mounts routes but has no ROUTES table"
            );
            assert!(
                gathered.contains(&format!("crate::{name}::ROUTES")),
                "{name}.rs has a ROUTES table that route_docs::sections() does not gather"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// The token
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn every_route_but_the_named_exceptions_answers_401_without_the_token() {
    let (_dir, socket, _stop) = boot().await;
    let mut open = Vec::new();
    for r in route_docs::all() {
        if r.path == "/events" {
            continue;
        }
        let (status, challenge) = request_open(&socket, r.method, &fill(r.path)).await;
        let gate_refused = status == 401 && challenge.as_deref() == Some("Bearer");
        if bisa_node::auth::is_exempt(r.path) {
            // A public hook answers for itself — 404 while the machine
            // allows none, 401 when no signature matches: the route's
            // verdict, not the token gate's.
            assert!(
                !gate_refused,
                "{} {} is a named exception and must not ask for the token",
                r.method, r.path
            );
        } else if !gate_refused {
            open.push(format!("{} {} → {status}", r.method, r.path));
        }
    }
    assert!(
        open.is_empty(),
        "answered without a token:\n{}",
        open.join("\n")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_wrong_token_is_401_and_the_right_one_in_either_place_is_not() {
    let (_dir, socket, _stop) = boot().await;
    let (status, _) = request_as(&socket, "GET", "/goals", Some("Bearer not-the-token")).await;
    assert_eq!(status, 401);
    let (status, _) = request_as(&socket, "GET", "/goals", Some(&format!("Bearer {TOKEN}"))).await;
    assert_eq!(status, 200);
    // The query form, for an EventSource and an <img>, which cannot send a header.
    let (status, _) = request_as(&socket, "GET", &format!("/goals?token={TOKEN}"), None).await;
    assert_eq!(status, 200);
    let (status, _) = request_as(&socket, "GET", "/goals?token=wrong", None).await;
    assert_eq!(status, 401);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_token_is_on_disk_for_the_cli_and_private() {
    let (dir, _socket, _stop) = boot().await;
    let path = Paths::new(dir.path()).token_file();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), TOKEN);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

// ---------------------------------------------------------------------------
// The desktop calls only documented routes
// ---------------------------------------------------------------------------

/// Every URL the desktop builds is a documented route.
///
/// `desktop/src/api.ts` and `bus.ts` are read as text and every request path
/// template extracted: the `get<T>(…)`/`post`/`put`/`patch`/`del` wrappers
/// (the method is the wrapper's name), the one raw `fetch(…, {method})`, and
/// the `withToken(…)` URL builders (GET). `${…}` expressions are rendered by a
/// small set of rules — a base prefix vanishes, an id becomes a placeholder, a
/// ternary contributes one candidate per branch, a query helper marks where
/// the query starts — and an expression no rule reads fails the test, so a new
/// shape is taught, never skipped. Paths are compared with every placeholder
/// collapsed to `{}` and the query dropped. The reverse — every route has a
/// desktop caller — is not asserted: hooks, A2A and CLI-only routes are
/// legitimate.
#[test]
fn every_desktop_call_hits_a_documented_route() {
    let desktop = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../desktop/src");
    let documented: std::collections::BTreeSet<(String, String)> = route_docs::all()
        .into_iter()
        .map(|r| (r.method.to_string(), collapse_placeholders(r.path)))
        .collect();
    let mut calls = Vec::new();
    for file in ["api.ts", "bus.ts"] {
        let text = std::fs::read_to_string(desktop.join(file)).unwrap();
        calls.extend(desktop_calls(file, &text));
    }
    assert!(
        calls.len() > 200,
        "only {} desktop calls were read; the scanner is broken",
        calls.len()
    );
    let misses: Vec<String> = calls
        .iter()
        .filter(|c| !documented.contains(&(c.method.clone(), c.path.clone())))
        .map(|c| {
            format!(
                "{}:{}: {} {} is not a documented route",
                c.file, c.line, c.method, c.path
            )
        })
        .collect();
    assert!(misses.is_empty(), "{}", misses.join("\n"));
}

/// `/runs/{rid}/steps/{step}/answer` → `/runs/{}/steps/{}/answer`.
fn collapse_placeholders(path: &str) -> String {
    let mut out = String::new();
    let mut depth = 0;
    for ch in path.chars() {
        match ch {
            '{' => {
                depth += 1;
                if depth == 1 {
                    out.push_str("{}");
                }
            }
            '}' => depth -= 1,
            _ if depth == 0 => out.push(ch),
            _ => {}
        }
    }
    out
}

struct DesktopCall {
    file: &'static str,
    line: usize,
    method: String,
    path: String,
}

/// Every (method, path) the file requests, with the line it is built on.
fn desktop_calls(file: &'static str, text: &str) -> Vec<DesktopCall> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let rest: String = chars[i..chars.len().min(i + 16)].iter().collect();
        let previous_is_word =
            i > 0 && (chars[i - 1].is_alphanumeric() || matches!(chars[i - 1], '_' | '.' | '$'));
        let mut method = None;
        let mut arg_start = None;
        if !previous_is_word {
            for (name, m) in [
                ("get<", "GET"),
                ("post<", "POST"),
                ("put<", "PUT"),
                ("patch<", "PATCH"),
                ("del<", "DELETE"),
            ] {
                if rest.starts_with(name) {
                    // Skip the generic, then expect the opening parenthesis.
                    let mut j = i + name.len() - 1;
                    let mut depth = 0;
                    while j < chars.len() {
                        match chars[j] {
                            '<' => depth += 1,
                            '>' => {
                                depth -= 1;
                                if depth == 0 {
                                    j += 1;
                                    break;
                                }
                            }
                            _ => {}
                        }
                        j += 1;
                    }
                    if chars.get(j) == Some(&'(') {
                        method = Some(m.to_string());
                        arg_start = Some(j + 1);
                    }
                    break;
                }
            }
            if rest.starts_with("withToken(") {
                method = Some("GET".to_string());
                arg_start = Some(i + "withToken(".len());
            } else if rest.starts_with("fetch(") {
                let call: String = chars[i..chars.len().min(i + 400)].iter().collect();
                let m = call
                    .split("method:")
                    .nth(1)
                    .and_then(|s| s.split('"').nth(1))
                    .unwrap_or("GET")
                    .to_string();
                method = Some(m);
                arg_start = Some(i + "fetch(".len());
            }
        }
        let (Some(method), Some(start)) = (method, arg_start) else {
            i += 1;
            continue;
        };
        let line = text[..char_offset(text, i)].matches('\n').count() + 1;
        let mut pos = start;
        while pos < chars.len() && chars[pos].is_whitespace() {
            pos += 1;
        }
        let (candidates, end) = url_argument(file, line, &chars, pos);
        for path in candidates {
            let path = path.split('?').next().unwrap_or("").to_string();
            // The request core's own `${base}${path}` has no literal text.
            if !path.starts_with('/') {
                continue;
            }
            out.push(DesktopCall {
                file,
                line,
                method: method.clone(),
                path: collapse_placeholders(&path),
            });
        }
        i = end.max(i + 1);
    }
    out
}

fn char_offset(text: &str, chars: usize) -> usize {
    text.char_indices()
        .nth(chars)
        .map(|(b, _)| b)
        .unwrap_or(text.len())
}

/// The first argument of a call, as every URL it can produce, and the index
/// just past it. A string or template literal is read directly; any other
/// expression (a ternary between two literals) is read to the top-level
/// comma or closing parenthesis and every literal inside it is a candidate.
fn url_argument(file: &str, line: usize, chars: &[char], start: usize) -> (Vec<String>, usize) {
    match chars.get(start) {
        Some('`') => read_template(file, line, chars, start),
        Some('"') | Some('\'') => {
            let (s, end) = read_string(chars, start);
            (vec![s], end)
        }
        _ => {
            let mut depth = 0i32;
            let mut j = start;
            let mut candidates = Vec::new();
            while j < chars.len() {
                match chars[j] {
                    '(' | '[' | '{' => depth += 1,
                    ')' | ']' | '}' if depth == 0 => break,
                    ')' | ']' | '}' => depth -= 1,
                    ',' if depth == 0 => break,
                    '`' => {
                        let (found, end) = read_template(file, line, chars, j);
                        candidates.extend(found);
                        j = end;
                        continue;
                    }
                    '"' | '\'' => {
                        let (s, end) = read_string(chars, j);
                        candidates.push(s);
                        j = end;
                        continue;
                    }
                    _ => {}
                }
                j += 1;
            }
            (candidates, j)
        }
    }
}

/// A quoted string, without its quotes; returns the index just past it.
fn read_string(chars: &[char], start: usize) -> (String, usize) {
    let quote = chars[start];
    let mut s = String::new();
    let mut j = start + 1;
    while j < chars.len() && chars[j] != quote {
        s.push(chars[j]);
        j += 1;
    }
    (s, j + 1)
}

/// A template literal rendered to every path it can produce.
fn read_template(file: &str, line: usize, chars: &[char], start: usize) -> (Vec<String>, usize) {
    let mut candidates = vec![String::new()];
    let mut j = start + 1;
    while j < chars.len() && chars[j] != '`' {
        if chars[j] == '$' && chars.get(j + 1) == Some(&'{') {
            // Read the expression to its matching brace, recursing into nested
            // template literals so a ternary's branches are rendered.
            let expr_start = j + 2;
            let mut depth = 1;
            let mut k = expr_start;
            while k < chars.len() && depth > 0 {
                match chars[k] {
                    '{' => depth += 1,
                    '}' => depth -= 1,
                    '`' => {
                        let (_, end) = read_template(file, line, chars, k);
                        k = end;
                        continue;
                    }
                    '"' | '\'' => {
                        let (_, end) = read_string(chars, k);
                        k = end;
                        continue;
                    }
                    _ => {}
                }
                k += 1;
            }
            let expr: String = chars[expr_start..k - 1].iter().collect();
            let renderings = render_expression(file, line, chars, expr_start, k - 1, expr.trim());
            candidates = candidates
                .iter()
                .flat_map(|prefix| renderings.iter().map(move |r| format!("{prefix}{r}")))
                .collect();
            j = k;
            continue;
        }
        for c in candidates.iter_mut() {
            c.push(chars[j]);
        }
        j += 1;
    }
    (candidates, j + 1)
}

/// What a `${…}` contributes to the path. The rules are the shapes `api.ts`
/// uses; an expression none of them reads is a test failure that names it.
fn render_expression(
    file: &str,
    line: usize,
    chars: &[char],
    start: usize,
    end: usize,
    expr: &str,
) -> Vec<String> {
    let is_identifier = |s: &str| {
        !s.is_empty()
            && s.chars()
                .next()
                .is_some_and(|c| c.is_alphabetic() || c == '_' || c == '$')
            && s.chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '$' || c == '.')
    };
    if expr == "base" || expr.starts_with("apiBaseSync()") {
        return vec![String::new()];
    }
    if expr == "query" {
        // `notes(query)` receives a ready-made query string.
        return vec!["?".to_string()];
    }
    if is_identifier(expr) || expr.starts_with("encodeURIComponent(") {
        return vec!["{}".to_string()];
    }
    if expr.starts_with("tagQuery(") || expr.starts_with("page(") {
        return vec!["?".to_string()];
    }
    if expr.strip_suffix(".toString()").is_some_and(is_identifier) {
        // A `URLSearchParams` rendered in place: the query starts here.
        return vec!["?".to_string()];
    }
    if expr.contains('?') {
        // A ternary: every literal in it is a branch. A branch that starts a
        // query marks where the path ends; an empty branch contributes nothing.
        let mut branches = Vec::new();
        // Literals before the top-level `?` are the condition, not branches.
        let mut j = start;
        let mut depth = 0i32;
        while j < end {
            match chars[j] {
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => depth -= 1,
                '`' => j = read_template(file, line, chars, j).1 - 1,
                '"' | '\'' => j = read_string(chars, j).1 - 1,
                '?' if depth == 0 => break,
                _ => {}
            }
            j += 1;
        }
        while j < end {
            match chars[j] {
                '`' => {
                    let (found, next) = read_template(file, line, chars, j);
                    branches.extend(found);
                    j = next;
                }
                '"' | '\'' => {
                    let (s, next) = read_string(chars, j);
                    branches.push(s);
                    j = next;
                }
                _ => j += 1,
            }
        }
        if !branches.is_empty() {
            return branches;
        }
    }
    panic!("{file}:{line}: cannot read the URL expression ${{{expr}}}; teach the gate its shape");
}
