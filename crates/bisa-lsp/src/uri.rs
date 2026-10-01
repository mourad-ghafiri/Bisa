//! `file://` on the server's side, root-relative on the editor's (ide/10):
//! the webview never learns an absolute path from a server.

use serde_json::Value;
use std::path::Path;

fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The `file://` URI of a root-relative path.
pub fn to_uri(root: &Path, relative: &str) -> String {
    let full = root.join(relative.trim_start_matches('/'));
    format!("file://{}", percent_encode(&full.to_string_lossy()))
}

/// The `file://` URI of the root itself.
pub fn root_uri(root: &Path) -> String {
    format!("file://{}", percent_encode(&root.to_string_lossy()))
}

/// A root-relative path from a `file://` URI under the root; `None` when the
/// URI is somewhere else (the standard library, a dependency), which the
/// editor cannot open and must not be told about as an absolute path.
pub fn to_relative(root: &Path, uri: &str) -> Option<String> {
    let path = uri.strip_prefix("file://")?;
    let path = percent_decode(path);
    let root_s = root.to_string_lossy();
    let rest = path.strip_prefix(root_s.as_ref())?;
    let rest = rest.strip_prefix('/').unwrap_or(rest);
    if rest.is_empty() {
        return None;
    }
    Some(rest.to_string())
}

/// Rewrite every `file://` URI under the root to a root-relative path, and
/// drop `uri` fields that point elsewhere — replaced by `null` so the shape
/// stays and the editor knows it cannot follow. Walks the whole value.
pub fn outbound(root: &Path, value: Value) -> Value {
    match value {
        Value::String(s) if s.starts_with("file://") => match to_relative(root, &s) {
            Some(rel) => Value::String(rel),
            None => Value::Null,
        },
        Value::Array(a) => Value::Array(a.into_iter().map(|v| outbound(root, v)).collect()),
        Value::Object(o) => {
            Value::Object(o.into_iter().map(|(k, v)| (k, outbound(root, v))).collect())
        }
        other => other,
    }
}

/// Rewrite the editor's root-relative `uri` fields to `file://` URIs before
/// they reach the server. Only `uri` keys are touched: a path in a string
/// the person typed (a rename's `newName`) is not a URI.
pub fn inbound(root: &Path, value: Value) -> Value {
    fn walk(root: &Path, value: Value, under_uri: bool) -> Value {
        match value {
            Value::String(s) if under_uri && !s.starts_with("file://") => {
                Value::String(to_uri(root, &s))
            }
            Value::Array(a) => Value::Array(a.into_iter().map(|v| walk(root, v, false)).collect()),
            Value::Object(o) => Value::Object(
                o.into_iter()
                    .map(|(k, v)| {
                        let is_uri = k == "uri" || k == "targetUri";
                        (k, walk(root, v, is_uri))
                    })
                    .collect(),
            ),
            other => other,
        }
    }
    walk(root, value, false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn paths_round_trip_and_leave_the_root_only_as_null() {
        let root = Path::new("/tmp/proj a");
        let uri = to_uri(root, "src/main.rs");
        assert_eq!(uri, "file:///tmp/proj%20a/src/main.rs");
        assert_eq!(to_relative(root, &uri), Some("src/main.rs".into()));
        assert_eq!(to_relative(root, "file:///usr/lib/x.rs"), None);
        assert_eq!(
            to_relative(root, "file:///tmp/proj%20a"),
            None,
            "the root itself is not a document"
        );
        let out = outbound(
            root,
            json!({"uri": "file:///tmp/proj%20a/a.rs", "other": {"uri": "file:///elsewhere/b.rs"}, "n": 1}),
        );
        assert_eq!(out, json!({"uri": "a.rs", "other": {"uri": null}, "n": 1}));
        let inb = inbound(
            root,
            json!({"textDocument": {"uri": "a.rs"}, "newName": "b.rs", "list": [{"uri": "c.rs"}]}),
        );
        assert_eq!(
            inb,
            json!({"textDocument": {"uri": "file:///tmp/proj%20a/a.rs"}, "newName": "b.rs", "list": [{"uri": "file:///tmp/proj%20a/c.rs"}]})
        );
        assert_eq!(root_uri(root), "file:///tmp/proj%20a");
    }
}
