//! The hook personalities: `bisa session report` and `bisa session
//! guard`, run by a harness's own hooks while a person drives it in a desktop
//! terminal.
//!
//! Both read the session and its secret from the environment the terminal set
//! (`BISA_SESSION`, `BISA_SESSION_SECRET`, `BISA_NODE_URL`) and
//! take the hook's payload from stdin — or from the one argument, for a
//! harness that passes it that way. Neither uses daemon discovery or the
//! control-plane token: the secret is the whole capability, and it reaches
//! exactly one roster row.
//!
//! A hook process installs no log handle — it must exit at once — so its
//! one channel for a fault is stderr, which the harness keeps with the hook's
//! output; `eprintln!` here is that channel, not a bypass of the log door.
//!
//! **`report` must never slow or block the harness**, so it exits 0 whatever
//! happens, says why on stderr, and gives the node two seconds.
//!
//! **`guard` is the one hook allowed to wait.** It posts the `PreToolUse`
//! payload to the node's guard route and prints the verdict in the shape the
//! harness named reads — `deny` with the reason, `ask`, or `allow` with the
//! input to run (a redacted placeholder restored). The shape is the
//! harness's, and lives beside its hooks (`bisa_adapters::hooks::
//! guard_output`: Claude Code's, GitHub Copilot CLI's); nothing here knows a
//! harness. When the node has no opinion, does not answer in time or is not
//! there, it prints nothing and exits 0: the harness's own prompt stands, and
//! the person at the keyboard decides. Exit 0 is part of the contract — a
//! harness may read a hook that fails as a refusal (Copilot CLI's
//! `PreToolUse` does), and a reporter or a guard that cannot reach its node
//! must never be the reason a tool did not run.

use clap::Subcommand;
use std::io::{Read as _, Write as _};
use std::net::{TcpStream, ToSocketAddrs as _};
use std::time::Duration;

/// How long `guard` waits for the node: the classifier's longest deadline and
/// a margin, and always less than the hook timeout the recipe wrote.
const GUARD_WAIT: Duration = Duration::from_secs(120);

#[derive(Subcommand)]
pub enum SessionCmd {
    /// Report one hook payload for the interactive session in the environment
    Report {
        /// The harness whose payload this is (its catalog id)
        #[arg(long)]
        harness: String,
        /// The payload, when the harness passes it as an argument rather than
        /// on stdin
        payload: Option<String>,
    },
    /// Ask the node's guard about one tool call before it runs, and print the
    /// harness's permission decision
    Guard {
        /// The harness whose payload this is (its catalog id)
        #[arg(long)]
        harness: String,
        /// The payload, when the harness passes it as an argument rather than
        /// on stdin
        payload: Option<String>,
    },
}

pub fn session(command: SessionCmd) {
    match command {
        SessionCmd::Report { harness, payload } => {
            if let Err(e) = report(&harness, payload) {
                eprintln!(
                    "{}",
                    bisa_i18n::say(&bisa_core::text!(
                        "cli-session-bisa-session-report",
                        e = e.to_string()
                    ))
                );
            }
        }
        SessionCmd::Guard { harness, payload } => match guard(&harness, payload) {
            Ok(Some(line)) => println!("{line}"),
            Ok(None) => {}
            Err(e) => eprintln!(
                "{}",
                bisa_i18n::say(&bisa_core::text!(
                    "cli-session-bisa-session-guard",
                    e = e.to_string()
                ))
            ),
        },
    }
}

fn env(name: &str) -> Result<String, String> {
    std::env::var(name)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| {
            bisa_i18n::say(&bisa_core::text!(
                "cli-session-not-set-only-runs-inside-terminal",
                name = name.to_string()
            ))
        })
}

fn payload_json(payload: Option<String>) -> Result<serde_json::Value, String> {
    let raw = match payload {
        Some(p) => p,
        None => {
            let mut buf = String::new();
            std::io::stdin().read_to_string(&mut buf).map_err(|e| {
                bisa_i18n::say(&bisa_core::text!(
                    "cli-session-could-not-read-payload",
                    e = e.to_string()
                ))
            })?;
            buf
        }
    };
    serde_json::from_str(raw.trim()).map_err(|e| {
        bisa_i18n::say(&bisa_core::text!(
            "cli-session-payload-not-json",
            e = e.to_string()
        ))
    })
}

fn report(harness: &str, payload: Option<String>) -> Result<(), String> {
    let session = env("BISA_SESSION")?;
    let secret = env("BISA_SESSION_SECRET")?;
    let node = env("BISA_NODE_URL")?;
    let value = payload_json(payload)?;
    let events = bisa_adapters::hooks::translate(harness, &value);
    if events.is_empty() {
        return Ok(());
    }
    let body = serde_json::json!({ "events": events }).to_string();
    post(
        &node,
        &format!("/sessions/{session}/report"),
        &secret,
        &body,
        Duration::from_secs(2),
    )
    .map(|_| ())
}

/// The guard's verdict as the harness's hook reads it, or `None` to say
/// nothing.
fn guard(harness: &str, payload: Option<String>) -> Result<Option<String>, String> {
    let session = env("BISA_SESSION")?;
    let secret = env("BISA_SESSION_SECRET")?;
    let node = env("BISA_NODE_URL")?;
    let value = payload_json(payload)?;
    if value.get("hook_event_name").and_then(|e| e.as_str()) != Some("PreToolUse") {
        return Ok(None);
    }
    let body = serde_json::json!({ "payload": value }).to_string();
    let reply = post(
        &node,
        &format!("/sessions/{session}/guard"),
        &secret,
        &body,
        GUARD_WAIT,
    )?;
    let reply: serde_json::Value = serde_json::from_str(&reply).map_err(|e| {
        bisa_i18n::say(&bisa_core::text!(
            "cli-session-node-s-answer-not-json",
            e = e.to_string()
        ))
    })?;
    Ok(bisa_adapters::hooks::guard_output(harness, &reply).map(|v| v.to_string()))
}

/// `http://host:port` → `(host, port)`.
fn authority(url: &str) -> Result<(String, u16), String> {
    let rest = url.strip_prefix("http://").ok_or_else(|| {
        bisa_i18n::say(&bisa_core::text!(
            "cli-session-bisa-node-url-must-be-http",
            url = format!("{url:?}")
        ))
    })?;
    let rest = rest.trim_end_matches('/');
    let (host, port) = rest.rsplit_once(':').ok_or_else(|| {
        bisa_i18n::say(&bisa_core::text!(
            "cli-session-bisa-node-url-has-no-port",
            url = format!("{url:?}")
        ))
    })?;
    let port: u16 = port.parse().map_err(|_| {
        bisa_i18n::say(&bisa_core::text!(
            "cli-session-bad-port",
            url = format!("{url:?}")
        ))
    })?;
    Ok((host.to_string(), port))
}

/// The request as HTTP/1.1 spells it: every line ended by CR LF, a header a
/// name, a colon and its value, an empty line before the body. **Words for a
/// machine, written here and nowhere else** — never a message of the
/// catalog, where a line's end is a translator's to lose and the node
/// answers 400 to what is left.
fn request(path: &str, host: &str, port: u16, secret: &str, body: &str) -> String {
    let length = body.len();
    // for the machine
    let head = [
        format!("POST {path} HTTP/1.1"),
        format!("Host: {host}:{port}"),
        format!("Authorization: Bearer {secret}"),
        "Content-Type: application/json".to_string(),
        format!("Content-Length: {length}"),
        "Connection: close".to_string(),
    ];
    let mut request = String::with_capacity(body.len() + 256);
    for line in head {
        request.push_str(&line);
        request.push_str("\r\n");
    }
    request.push_str("\r\n");
    request.push_str(body);
    request
}

/// One HTTP/1.1 request over TCP, with the connection closed after it, and
/// the response body handed back. Blocking and time-boxed on purpose: a hook
/// is a short-lived process with no runtime to carry. `read` is how long the
/// node may take to answer — two seconds for a report, the guard's wait for a
/// verdict.
fn post(
    node: &str,
    path: &str,
    secret: &str,
    body: &str,
    read: Duration,
) -> Result<String, String> {
    let (host, port) = authority(node)?;
    let addr = (host.as_str(), port)
        .to_socket_addrs()
        .map_err(|e| format!("{host}:{port}: {e}"))?
        .next()
        .ok_or_else(|| {
            bisa_i18n::say(&bisa_core::text!(
                "cli-session-resolves-nothing",
                host = host.to_string(),
                port = port.to_string()
            ))
        })?;
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(1)).map_err(|e| {
        bisa_i18n::say(&bisa_core::text!(
            "cli-session-node-did-not-answer",
            host = host.to_string(),
            port = port.to_string(),
            e = e.to_string()
        ))
    })?;
    // Without the deadlines a hook could hang its harness: a socket that
    // refuses them is not one to speak on.
    stream
        .set_write_timeout(Some(Duration::from_secs(1)))
        .map_err(|e| {
            bisa_i18n::say(&bisa_core::text!(
                "cli-session-no-write-deadline-socket",
                e = e.to_string()
            ))
        })?;
    stream.set_read_timeout(Some(read)).map_err(|e| {
        bisa_i18n::say(&bisa_core::text!(
            "cli-session-no-read-deadline-socket",
            e = e.to_string()
        ))
    })?;
    let request = request(path, &host, port, secret, body);
    stream.write_all(request.as_bytes()).map_err(|e| {
        bisa_i18n::say(&bisa_core::text!(
            "cli-session-could-not-send-request",
            e = e.to_string()
        ))
    })?;
    // Read as bytes: what arrived before a deadline passed is kept, and a
    // length in the answer counts bytes, never characters.
    let mut response = Vec::new();
    if let Err(e) = stream.read_to_end(&mut response) {
        // A deadline that passed or a connection that dropped: said, so a
        // hook that got no answer is not read as one that got an empty one.
        if response.is_empty() {
            return Err(bisa_i18n::say(&bisa_core::text!(
                "cli-session-node-s-answer-did-not-arrive",
                e = e.to_string()
            )));
        }
    }
    match status_of(&response) {
        Some(status) if status.starts_with('2') => response_body(&response).ok_or_else(|| {
            bisa_i18n::say(&bisa_core::text!("cli-session-node-s-answer-cut-short"))
        }),
        Some(status) => Err(bisa_i18n::say(&bisa_core::text!(
            "cli-session-node-answered",
            status = status
        ))),
        None => Err(bisa_i18n::say(&bisa_core::text!(
            "cli-session-node-closed-connection-without-status-line"
        ))),
    }
}

/// Where `mark` begins in `bytes`.
fn find(bytes: &[u8], mark: &[u8]) -> Option<usize> {
    bytes.windows(mark.len()).position(|w| w == mark)
}

/// The status of the answer's first line — its second word.
fn status_of(response: &[u8]) -> Option<String> {
    let line = &response[..find(response, b"\r\n").unwrap_or(response.len())];
    String::from_utf8_lossy(line)
        .split_whitespace()
        .nth(1)
        .map(str::to_string)
}

/// The body after the blank line, whether the node chunked it or not. Read
/// as bytes, since a chunk's length counts bytes and a chunk may end inside
/// a character. `None` for an answer that ends before its body does — no
/// blank line, a chunk cut short, a length that is no number, bytes that are
/// no text: an answer that was not read, never half of one taken for whole.
fn response_body(response: &[u8]) -> Option<String> {
    let blank = find(response, b"\r\n\r\n")?;
    let (head, body) = (&response[..blank], &response[blank + 4..]);
    const CHUNKED: &str = "transfer-encoding: chunked"; // for the machine
    const LENGTH: &str = "content-length:"; // for the machine
    let head = String::from_utf8_lossy(head).to_ascii_lowercase();
    if !head.contains(CHUNKED) {
        // The length the answer names is the length it has, or it was cut.
        let named = head
            .lines()
            .find_map(|line| line.strip_prefix(LENGTH))
            .map(|n| n.trim().parse::<usize>());
        return match named {
            Some(Ok(length)) => String::from_utf8(body.get(..length)?.to_vec()).ok(),
            Some(Err(_)) => None,
            None => String::from_utf8(body.to_vec()).ok(),
        };
    }
    // Chunked: `<hex length>[;extension]\r\n<bytes>\r\n` until a chunk of
    // no length.
    let mut out = Vec::new();
    let mut rest = body;
    loop {
        let line = find(rest, b"\r\n")?;
        let size = std::str::from_utf8(&rest[..line]).ok()?;
        let size = size.split(';').next().unwrap_or_default().trim();
        let length = usize::from_str_radix(size, 16).ok()?;
        rest = &rest[line + 2..];
        if length == 0 {
            break;
        }
        out.extend_from_slice(rest.get(..length)?);
        rest = rest.get(length..)?.strip_prefix(b"\r\n")?;
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Whatever the node answers — any text, chunked at any sizes, with a
    /// chunk extension or a `Content-Length` — the reader gives the body
    /// back whole; cut anywhere before its end it gives nothing, never half;
    /// and any bytes at all never make it panic.
    mod any_answer {
        use super::*;
        use proptest::prelude::*;

        fn chunked(body: &[u8], sizes: &[usize], extension: bool) -> Vec<u8> {
            let mut out = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n".to_vec();
            let mut rest = body;
            let mut sizes = sizes.iter().cycle();
            while !rest.is_empty() {
                let take = (*sizes.next().unwrap_or(&1)).clamp(1, rest.len());
                let (chunk, after) = rest.split_at(take);
                out.extend_from_slice(format!("{take:x}").as_bytes());
                if extension {
                    out.extend_from_slice(b";ext=1");
                }
                out.extend_from_slice(b"\r\n");
                out.extend_from_slice(chunk);
                out.extend_from_slice(b"\r\n");
                rest = after;
            }
            out.extend_from_slice(b"0\r\n\r\n");
            out
        }

        fn with_length(body: &[u8]) -> Vec<u8> {
            let mut out =
                format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", body.len()).into_bytes();
            out.extend_from_slice(body);
            out
        }

        proptest! {
            #[test]
            fn a_body_chunked_at_any_sizes_reads_back_whole(
                body in "\\PC{0,200}",
                sizes in proptest::collection::vec(1usize..17, 1..6),
                extension in any::<bool>(),
            ) {
                let answer = chunked(body.as_bytes(), &sizes, extension);
                let read = response_body(&answer);
                prop_assert_eq!(read.as_deref(), Some(body.as_str()));
                let read = response_body(&with_length(body.as_bytes()));
                prop_assert_eq!(read.as_deref(), Some(body.as_str()));
            }

            #[test]
            fn an_answer_cut_before_its_end_is_nothing_never_half(
                body in "\\PC{1,120}",
                sizes in proptest::collection::vec(1usize..17, 1..6),
                cut in 0usize..1000,
            ) {
                let answer = chunked(body.as_bytes(), &sizes, false);
                // The head ends at the blank line; a cut inside the body's
                // chunks is an answer not read. (The last two bytes are the
                // line end after the empty chunk: the body is whole by then.)
                let head_end = answer.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
                let cut = head_end + cut % (answer.len() - 2 - head_end);
                let read = response_body(&answer[..cut]);
                prop_assert!(read.is_none(), "a cut answer read as {read:?}");
                let short = with_length(body.as_bytes());
                prop_assert!(response_body(&short[..short.len() - 1]).is_none(), "a body shorter than its length read as whole");
            }

            #[test]
            fn any_bytes_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..300)) {
                let _ = response_body(&bytes);
            }
        }
    }

    #[test]
    fn the_node_url_is_a_loopback_authority() {
        assert_eq!(
            authority("http://127.0.0.1:4321").unwrap(),
            ("127.0.0.1".into(), 4321)
        );
        assert_eq!(
            authority("http://localhost:80/").unwrap(),
            ("localhost".into(), 80)
        );
        assert!(authority("https://x:1").is_err());
        assert!(authority("http://nohost").is_err());
    }

    /// A loopback listener standing in for the node: it accepts one
    /// connection and does with it what `answer` says.
    fn node_that(answer: impl FnOnce(std::net::TcpStream) + Send + 'static) -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            if let Ok((stream, _)) = listener.accept() {
                answer(stream);
            }
        });
        format!("http://127.0.0.1:{port}")
    }

    /// The request's own shape, byte for byte: CR LF after every line, no
    /// line that begins with a space, the length the body's own.
    #[test]
    fn the_request_is_spelt_as_http_spells_it() {
        let text = request(
            "/sessions/s/report",
            "127.0.0.1",
            4321,
            "secret",
            "{\"a\":1}",
        );
        let (head, body) = text.split_once("\r\n\r\n").expect("an empty line");
        assert_eq!(body, "{\"a\":1}");
        let lines: Vec<&str> = head.split("\r\n").collect();
        assert_eq!(
            lines,
            [
                "POST /sessions/s/report HTTP/1.1",
                "Host: 127.0.0.1:4321",
                "Authorization: Bearer secret",
                "Content-Type: application/json",
                "Content-Length: 7",
                "Connection: close",
            ]
        );
        assert!(
            !head.replace("\r\n", "").contains(['\r', '\n']),
            "no line ends another way"
        );
    }

    /// What a server heard of one request.
    struct Heard {
        authorization: Option<String>,
        content_type: Option<String>,
        body: String,
    }

    /// A node that reads what it is sent the way the node does — hyper
    /// behind a loopback listener — answers one request and stops. What it
    /// heard comes back on the channel.
    fn reading_node() -> (String, std::sync::mpsc::Receiver<Heard>) {
        let (heard_tx, heard_rx) = std::sync::mpsc::channel();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        listener.set_nonblocking(true).unwrap();
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(async move {
                let listener = tokio::net::TcpListener::from_std(listener).unwrap();
                let (done_tx, done_rx) = tokio::sync::oneshot::channel::<()>();
                let done = std::sync::Arc::new(std::sync::Mutex::new(Some(done_tx)));
                let read = move |headers: axum::http::HeaderMap, body: String| {
                    let (heard_tx, done) = (heard_tx.clone(), std::sync::Arc::clone(&done));
                    async move {
                        let header = |name: &str| {
                            headers
                                .get(name)
                                .and_then(|v| v.to_str().ok())
                                .map(str::to_string)
                        };
                        let _test_gone = heard_tx.send(Heard {
                            authorization: header("authorization"),
                            content_type: header("content-type"),
                            body,
                        });
                        if let Some(done) = done.lock().unwrap().take() {
                            let _served = done.send(());
                        }
                        axum::Json(serde_json::json!({ "ok": true }))
                    }
                };
                let app =
                    axum::Router::new().route("/sessions/{id}/report", axum::routing::post(read));
                axum::serve(listener, app)
                    .with_graceful_shutdown(async move {
                        let _stopped_or_dropped = done_rx.await;
                    })
                    .await
                    .unwrap();
            });
        });
        (format!("http://127.0.0.1:{port}"), heard_rx)
    }

    /// The request is HTTP as a server reads it — the node's own parser, not
    /// a stand-in that never looks: the secret as the bearer, the body whole
    /// and typed, the answer read back. A request line ended another way, or
    /// a header line that begins with a space, is refused before any route.
    #[test]
    fn the_hooks_request_is_one_a_server_reads() {
        let (node, heard) = reading_node();
        let body = r#"{"events":[{"tier":"progress","event":{"type":"turn_started"}}]}"#;
        let answer = post(
            &node,
            "/sessions/01J8ZQ0000000000000000SESS/report",
            "the-session-secret",
            body,
            Duration::from_secs(2),
        )
        .expect("the node read the request and answered");
        assert_eq!(answer, r#"{"ok":true}"#);
        let heard = heard
            .recv_timeout(Duration::from_secs(2))
            .expect("the request reached its route");
        assert_eq!(
            heard.authorization.as_deref(),
            Some("Bearer the-session-secret")
        );
        assert_eq!(heard.content_type.as_deref(), Some("application/json"));
        assert_eq!(heard.body, body);
    }

    #[test]
    fn a_node_that_answers_nothing_is_an_error_never_an_empty_answer() {
        // Closed without a byte: no status line to read — or, when the
        // close lands before the request is written, a reset. Either is an
        // error and never an empty answer.
        let closed = node_that(drop);
        let err = post(&closed, "/x", "s", "{}", Duration::from_millis(500)).unwrap_err();
        assert!(
            err.contains("without a status line")
                || err.contains("did not arrive")
                || err.contains("could not send"),
            "{err}"
        );

        // Open but silent past the read deadline: the deadline is the answer.
        let silent = node_that(|stream| {
            std::thread::sleep(Duration::from_millis(600));
            drop(stream);
        });
        let err = post(&silent, "/x", "s", "{}", Duration::from_millis(100)).unwrap_err();
        assert!(err.contains("did not arrive"), "{err}");

        // A real answer still lands whole.
        let spoken = node_that(|mut stream| {
            use std::io::{Read, Write};
            let mut buf = [0u8; 1024];
            // Awaited so the client has spoken; what it said is not read.
            let _request = stream.read(&mut buf);
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\nConnection: close\r\n\r\nok!!")
                .unwrap();
        });
        assert_eq!(
            post(&spoken, "/x", "s", "{}", Duration::from_millis(500)).unwrap(),
            "ok!!"
        );
    }

    /// The guard prints what the harness's own shape says and knows none
    /// itself: one line of JSON for a verdict, nothing for no opinion.
    #[test]
    fn a_verdict_is_printed_as_the_named_harness_reads_it_and_no_opinion_prints_nothing() {
        let printed = |harness: &str, reply: serde_json::Value| {
            bisa_adapters::hooks::guard_output(harness, &reply).map(|v| v.to_string())
        };
        let deny = serde_json::json!({ "decision": "deny", "reason": "refused by a rule" });
        let claude = printed("claude-code", deny.clone()).unwrap();
        assert!(!claude.contains('\n'), "one line");
        assert!(claude.contains(r#""hookSpecificOutput""#) && claude.contains(r#""deny""#));
        let copilot = printed("copilot", deny.clone()).unwrap();
        assert!(!copilot.contains('\n'), "one line");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&copilot).unwrap(),
            serde_json::json!({
                "permissionDecision": "deny",
                "permissionDecisionReason": "refused by a rule"
            })
        );
        for harness in ["claude-code", "copilot"] {
            assert!(
                printed(harness, serde_json::json!({})).is_none(),
                "{harness}"
            );
        }
        for bare in ["grok", "gemini"] {
            assert!(
                printed(bare, deny.clone()).is_none(),
                "{bare}: a harness with no hook that reads a verdict is printed nothing"
            );
        }
    }

    #[test]
    fn a_response_body_is_read_plain_or_chunked() {
        assert_eq!(
            response_body(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}").as_deref(),
            Some("{}")
        );
        assert_eq!(
            response_body(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n4\r\n{\"a\"\r\n3;x=y\r\n:1}\r\n0\r\n\r\n").as_deref(),
            Some("{\"a\":1}")
        );
        assert_eq!(
            response_body(b"HTTP/1.1 204 No Content\r\n\r\n").as_deref(),
            Some(""),
            "no body is a body of nothing"
        );
        assert_eq!(response_body(b"garbage"), None, "no blank line, no body");
        assert_eq!(
            status_of(b"HTTP/1.1 404 Not Found\r\n\r\n").as_deref(),
            Some("404")
        );
        assert_eq!(status_of(b""), None);
    }

    /// The answer in chunks, cut after each of `cuts` bytes.
    fn chunked(body: &[u8], cuts: &[usize]) -> Vec<u8> {
        let mut out = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n".to_vec();
        let mut from = 0;
        for cut in cuts.iter().copied().chain([body.len()]) {
            if cut > from {
                out.extend_from_slice(format!("{:x}\r\n", cut - from).as_bytes());
                out.extend_from_slice(&body[from..cut]);
                out.extend_from_slice(b"\r\n");
                from = cut;
            }
        }
        out.extend_from_slice(b"0\r\n\r\n");
        out
    }

    /// A verdict's reason is a sentence with quotation marks and any
    /// language's letters in it, and a chunk ends where the sender's buffer
    /// did — inside a character as soon as not. Wherever the answer is cut,
    /// in two or in three, the body read is the body sent; and an answer
    /// that stops short of its end is not read at all, so a refusal is never
    /// lost to half a sentence that is no JSON.
    #[test]
    fn a_chunked_answer_is_read_whole_wherever_it_was_cut() {
        let body = r#"{"decision":"deny","reason":"refused by the rule “écriture — 書く”"}"#;
        let bytes = body.as_bytes();
        for first in 0..=bytes.len() {
            assert_eq!(
                response_body(&chunked(bytes, &[first])).as_deref(),
                Some(body),
                "cut at {first}"
            );
            for second in (first..=bytes.len()).step_by(7) {
                assert_eq!(
                    response_body(&chunked(bytes, &[first, second])).as_deref(),
                    Some(body),
                    "cut at {first} and {second}"
                );
            }
        }
        // Whole once its last chunk — the one of no length — has been read.
        let whole = chunked(bytes, &[10]);
        let last_chunk_read = whole.len() - 2;
        for short in 0..last_chunk_read {
            let read = response_body(&whole[..short]);
            assert!(
                read.is_none(),
                "an answer that ends after {short} of {} bytes is not read: {read:?}",
                whole.len()
            );
        }
        // And one that names its length has it.
        let plain = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
            bytes.len()
        );
        assert_eq!(response_body(plain.as_bytes()).as_deref(), Some(body));
        for short in 0..plain.len() {
            assert!(
                response_body(&plain.as_bytes()[..short]).is_none(),
                "cut after {short} bytes"
            );
        }
        assert!(response_body(b"HTTP/1.1 200 OK\r\nContent-Length: many\r\n\r\n{}").is_none());
    }
}
