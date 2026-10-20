//! A language server from a script, for the supervisor's tests: JSON-RPC over
//! stdio with LSP framing, `initialize` answered with fixed capabilities, a
//! hover answered with the document's uri, `shutdown`/`exit` honoured — and
//! the misbehaviours a test asks for by the file it opens: `malformed.txt`
//! makes the next frame garbage, `crash.txt` exits with status 3 at once,
//! `loud.txt` exits after more stderr than the supervisor keeps, `chatty.txt`
//! asks the client three things and sends a frame that is nothing,
//! `mute.txt` leaves every hover unanswered, `mute-once.txt` the next one
//! only (each unanswered hover is told as a `scripted/heard` notification,
//! so a test knows the request arrived), `refuse-shutdown.txt` refuses the
//! shutdown and `slow-shutdown.txt` answers neither shutdown nor exit. Run
//! with `--deaf-init` it never answers `initialize`. Nothing here reads a
//! file or a network; only stdin and stdout.

use std::io::{Read, Write};

/// How the scripted server misbehaves from here on, as the opened files asked.
#[derive(Default)]
struct Mood {
    mute: bool,
    mute_once: bool,
    refuse_shutdown: bool,
    slow_shutdown: bool,
    /// The answers to the three things `chatty.txt` asked, by their id.
    answered: Vec<(String, serde_json::Value)>,
}

fn main() {
    let stdin = std::io::stdin();
    let mut stdin = stdin.lock();
    let stdout = std::io::stdout();
    let mut stdout = stdout.lock();
    let mut buf: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 8192];
    let mut mood = Mood::default();
    let deaf_init = std::env::args().any(|a| a == "--deaf-init");
    loop {
        let n = match stdin.read(&mut chunk) {
            Ok(0) | Err(_) => return,
            Ok(n) => n,
        };
        buf.extend_from_slice(&chunk[..n]);
        while let Some(body) = next_body(&mut buf) {
            let msg: serde_json::Value = match serde_json::from_slice(&body) {
                Ok(v) => v,
                Err(_) => return,
            };
            let method = msg["method"].as_str().unwrap_or_default();
            let id = msg.get("id").cloned();
            match (method, id) {
                // A response to something this server asked: `chatty.txt`
                // tells the client what it heard once all three are in.
                ("", Some(id)) => {
                    let asked = id.as_str().unwrap_or_default().to_string();
                    if asked.starts_with("ask-") {
                        mood.answered
                            .push((asked, msg.get("result").cloned().unwrap_or_default()));
                        if mood.answered.len() == 3 {
                            let heard: serde_json::Map<String, serde_json::Value> =
                                mood.answered.drain(..).collect();
                            write(
                                &mut stdout,
                                &serde_json::json!({"jsonrpc": "2.0", "method": "scripted/answered", "params": heard}),
                            );
                        }
                    }
                }
                ("initialize", Some(_)) if deaf_init => {}
                ("initialize", Some(id)) => write(
                    &mut stdout,
                    &serde_json::json!({"jsonrpc": "2.0", "id": id, "result": {
                        "capabilities": {"hoverProvider": true, "definitionProvider": true},
                        "serverInfo": {"name": "scripted-lsp"}
                    }}),
                ),
                ("textDocument/hover", Some(_)) if mood.mute || mood.mute_once => {
                    mood.mute_once = false;
                    write(
                        &mut stdout,
                        &serde_json::json!({"jsonrpc": "2.0", "method": "scripted/heard", "params": {"method": "textDocument/hover"}}),
                    );
                }
                ("textDocument/hover", Some(id)) => write(
                    &mut stdout,
                    &serde_json::json!({"jsonrpc": "2.0", "id": id, "result": {
                        "contents": {"kind": "plaintext", "value": format!("hovering {}", msg["params"]["textDocument"]["uri"].as_str().unwrap_or("?"))}
                    }}),
                ),
                ("shutdown", Some(_)) if mood.slow_shutdown => {}
                ("shutdown", Some(id)) if mood.refuse_shutdown => write(
                    &mut stdout,
                    &serde_json::json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32600, "message": "not going anywhere"}}),
                ),
                ("shutdown", Some(id)) => write(
                    &mut stdout,
                    &serde_json::json!({"jsonrpc": "2.0", "id": id, "result": null}),
                ),
                ("exit", None) if mood.slow_shutdown => {}
                ("exit", None) => return,
                ("textDocument/didOpen", None) => {
                    let uri = msg["params"]["textDocument"]["uri"].as_str().unwrap_or("");
                    if uri.ends_with("crash.txt") {
                        eprintln!("scripted-lsp: told to crash");
                        std::process::exit(3);
                    }
                    if uri.ends_with("loud.txt") {
                        // More than the supervisor keeps of stderr: the
                        // beginning is cut, the last words stay.
                        eprint!("{}", "x".repeat(3000));
                        eprintln!("the last words");
                        std::process::exit(2);
                    }
                    if uri.ends_with("mute.txt") {
                        mood.mute = true;
                    }
                    if uri.ends_with("mute-once.txt") {
                        mood.mute_once = true;
                    }
                    if uri.ends_with("refuse-shutdown.txt") {
                        mood.refuse_shutdown = true;
                    }
                    if uri.ends_with("slow-shutdown.txt") {
                        mood.slow_shutdown = true;
                    }
                    if uri.ends_with("chatty.txt") {
                        // Three things a server may ask its client, a frame
                        // that is neither a request nor a notification, then
                        // the diagnostic every document gets.
                        write(
                            &mut stdout,
                            &serde_json::json!({"jsonrpc": "2.0", "id": "ask-configuration", "method": "workspace/configuration", "params": {"items": [{"section": "a"}, {"section": "b"}]}}),
                        );
                        write(
                            &mut stdout,
                            &serde_json::json!({"jsonrpc": "2.0", "id": "ask-folders", "method": "workspace/workspaceFolders", "params": null}),
                        );
                        write(
                            &mut stdout,
                            &serde_json::json!({"jsonrpc": "2.0", "id": "ask-oddity", "method": "scripted/oddity", "params": {}}),
                        );
                        write(&mut stdout, &serde_json::json!({"jsonrpc": "2.0"}));
                        // Two answers nobody asked for: one with an id that is
                        // no number, one with a number nobody waits on.
                        write(
                            &mut stdout,
                            &serde_json::json!({"jsonrpc": "2.0", "id": "stray", "result": null}),
                        );
                        write(
                            &mut stdout,
                            &serde_json::json!({"jsonrpc": "2.0", "id": 999, "result": null}),
                        );
                    }
                    if uri.ends_with("malformed.txt") {
                        stdout
                            .write_all(b"Content-Length: 5\r\n\r\n<bad>")
                            .expect("the supervisor reads stdout");
                        stdout.flush().expect("the supervisor reads stdout");
                    } else {
                        // A diagnostic for every document, so a listener has one.
                        write(
                            &mut stdout,
                            &serde_json::json!({"jsonrpc": "2.0", "method": "textDocument/publishDiagnostics",
                                "params": {"uri": uri, "diagnostics": [{"range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 1}}, "message": "scripted", "severity": 2}]}}),
                        );
                    }
                }
                (_, Some(id)) => write(
                    &mut stdout,
                    &serde_json::json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32601, "message": format!("{method} is not scripted")}}),
                ),
                _ => {}
            }
        }
    }
}

/// One framed body when the buffer holds a whole one.
fn next_body(buf: &mut Vec<u8>) -> Option<Vec<u8>> {
    let head_end = buf.windows(4).position(|w| w == b"\r\n\r\n")?;
    let head = String::from_utf8_lossy(&buf[..head_end]).into_owned();
    let len: usize = head
        .lines()
        .find_map(|l| {
            l.split_once(':')
                .filter(|(k, _)| k.trim().eq_ignore_ascii_case("content-length"))
        })
        .and_then(|(_, v)| v.trim().parse().ok())?;
    let start = head_end + 4;
    if buf.len() < start + len {
        return None;
    }
    let body = buf[start..start + len].to_vec();
    buf.drain(..start + len);
    Some(body)
}

fn write(out: &mut impl Write, msg: &serde_json::Value) {
    let body = serde_json::to_vec(msg).unwrap_or_default();
    out.write_all(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes())
        .expect("the supervisor reads stdout");
    out.write_all(&body).expect("the supervisor reads stdout");
    out.flush().expect("the supervisor reads stdout");
}
