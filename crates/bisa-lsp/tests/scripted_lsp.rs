//! A language server from a script, for the supervisor's tests: JSON-RPC over
//! stdio with LSP framing, `initialize` answered with fixed capabilities, a
//! hover answered with the document's uri, `shutdown`/`exit` honoured — and
//! two misbehaviours a test asks for by the file it opens: `malformed.txt`
//! makes the next frame garbage, `crash.txt` exits with status 3 at once.
//! Nothing here reads a file or a network; only stdin and stdout.

use std::io::{Read, Write};

fn main() {
    let stdin = std::io::stdin();
    let mut stdin = stdin.lock();
    let stdout = std::io::stdout();
    let mut stdout = stdout.lock();
    let mut buf: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 8192];
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
                ("initialize", Some(id)) => write(
                    &mut stdout,
                    &serde_json::json!({"jsonrpc": "2.0", "id": id, "result": {
                        "capabilities": {"hoverProvider": true, "definitionProvider": true},
                        "serverInfo": {"name": "scripted-lsp"}
                    }}),
                ),
                ("textDocument/hover", Some(id)) => write(
                    &mut stdout,
                    &serde_json::json!({"jsonrpc": "2.0", "id": id, "result": {
                        "contents": {"kind": "plaintext", "value": format!("hovering {}", msg["params"]["textDocument"]["uri"].as_str().unwrap_or("?"))}
                    }}),
                ),
                ("shutdown", Some(id)) => write(
                    &mut stdout,
                    &serde_json::json!({"jsonrpc": "2.0", "id": id, "result": null}),
                ),
                ("exit", None) => return,
                ("textDocument/didOpen", None) => {
                    let uri = msg["params"]["textDocument"]["uri"].as_str().unwrap_or("");
                    if uri.ends_with("crash.txt") {
                        eprintln!("scripted-lsp: told to crash");
                        std::process::exit(3);
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
