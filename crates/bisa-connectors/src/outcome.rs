//! An answer read into what the run records: the status, the body as JSON,
//! and the part the definition selects.

use crate::error::ConnectorError;
use crate::http::Response;
use serde_json::Value;

/// A response is never read past this: an operation that answers more is
/// asked to select less.
pub const MAX_BODY_BYTES: usize = 1024 * 1024;

/// What a successful call produced.
#[derive(Clone, Debug, PartialEq)]
pub struct Outcome {
    pub status: u16,
    /// The answer's body — the last page's, for a paged call.
    pub body: Value,
    /// `select`ed from the body, or the whole body when nothing is selected;
    /// for a paged call, every page's array joined into one.
    pub selected: Value,
    /// How many pages were read: one, unless the operation pages.
    pub pages: u8,
}

/// A dotted path into a JSON value; numeric parts index arrays.
pub fn json_path<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    let mut cur = value;
    for part in path.split('.') {
        if part.is_empty() {
            return None;
        }
        cur = match cur {
            Value::Object(map) => map.get(part)?,
            Value::Array(items) => items.get(part.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(cur)
}

/// The sentence a service put in an error body, from the places the common
/// APIs put it, else nothing.
pub fn explanation(body: &Value) -> Option<String> {
    // An OAuth error body names a code and a sentence: both, so a caller
    // that decides on the code (`invalid_grant`) still reads the sentence.
    if let (Some(Value::String(code)), Some(Value::String(description))) = (
        json_path(body, "error"),
        json_path(body, "error_description"),
    ) {
        if !code.trim().is_empty() && !description.trim().is_empty() {
            return Some(format!("{}: {}", code.trim(), description.trim()));
        }
    }
    let candidates = [
        json_path(body, "message"),
        json_path(body, "error_description"),
        json_path(body, "error.message"),
        json_path(body, "error"),
        json_path(body, "detail"),
        json_path(body, "errors.0.message"),
        json_path(body, "errorMessages.0"),
    ];
    let text = candidates.into_iter().flatten().find_map(|v| match v {
        Value::String(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
        Value::Object(_) | Value::Array(_) | Value::Null => None,
        other => Some(other.to_string()),
    })?;
    Some(text.chars().take(300).collect())
}

/// The body as JSON when it parses, as text otherwise, and nothing for an
/// empty one.
fn body_value(bytes: &[u8]) -> Value {
    if bytes.iter().all(u8::is_ascii_whitespace) {
        return Value::Null;
    }
    serde_json::from_slice(bytes)
        .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(bytes).into_owned()))
}

fn status_text(status: u16) -> &'static str {
    match status {
        400 => "bad request",
        401 => "unauthorised",
        403 => "forbidden",
        404 => "not found",
        409 => "conflict",
        422 => "unprocessable",
        429 => "too many requests",
        500 => "internal error",
        502 => "bad gateway",
        503 => "unavailable",
        504 => "gateway timeout",
        _ => "error",
    }
}

/// Read a response: a non-2xx is the error its status names, with the body's
/// own sentence; a 2xx is an [`Outcome`].
pub fn parse(resp: &Response, select: Option<&str>) -> Result<Outcome, ConnectorError> {
    if resp.body.len() > MAX_BODY_BYTES {
        return Err(ConnectorError::TooLarge(resp.body.len()));
    }
    let body = body_value(&resp.body);
    let status = resp.status;
    if !(200..300).contains(&status) {
        let reason = explanation(&body).unwrap_or_else(|| status_text(status).to_string());
        return Err(match status {
            401 | 403 => ConnectorError::NotAuthenticated(reason),
            404 => ConnectorError::NotFound(reason),
            429 => ConnectorError::RateLimited {
                retry_after: crate::retry::retry_after(&resp.headers, 0).map_or(0, |d| d.as_secs()),
            },
            400..=499 => ConnectorError::Refused { status, reason },
            _ => ConnectorError::Upstream { status, reason },
        });
    }
    let selected = match select {
        Some(path) if !path.is_empty() => {
            json_path(&body, path)
                .cloned()
                .ok_or_else(|| ConnectorError::SelectMissing {
                    path: path.to_string(),
                })?
        }
        _ => body.clone(),
    };
    Ok(Outcome {
        status,
        body,
        selected,
        pages: 1,
    })
}

/// The cursor an answer names at `path`: a non-empty string, or a number as
/// its text; nothing for anything else — the last page names none.
pub fn next_cursor(body: &Value, path: &str) -> Option<String> {
    match json_path(body, path)? {
        Value::String(s) if !s.trim().is_empty() => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn resp(status: u16, body: &str) -> Response {
        Response {
            status,
            headers: vec![],
            body: body.as_bytes().to_vec(),
        }
    }

    #[test]
    fn select_walks_a_dotted_path_and_a_missing_path_is_named() {
        let out = parse(
            &resp(200, r#"{"data":{"items":[{"id":1}]}}"#),
            Some("data.items.0.id"),
        )
        .unwrap();
        assert_eq!(out.selected, json!(1));
        assert_eq!(out.body["data"]["items"][0]["id"], json!(1));
        let err = parse(&resp(200, r#"{"data":{}}"#), Some("data.items")).unwrap_err();
        assert!(matches!(err, ConnectorError::SelectMissing { path } if path == "data.items"));
        assert_eq!(
            parse(&resp(200, "[1,2]"), None).unwrap().selected,
            json!([1, 2])
        );
    }

    #[test]
    fn a_non_json_body_is_text_and_an_empty_one_is_null() {
        assert_eq!(parse(&resp(200, "ok"), None).unwrap().body, json!("ok"));
        assert_eq!(parse(&resp(204, ""), None).unwrap().body, Value::Null);
    }

    #[test]
    fn statuses_map_to_their_errors_with_the_bodys_own_words() {
        assert!(
            matches!(parse(&resp(401, r#"{"message":"bad token"}"#), None), Err(ConnectorError::NotAuthenticated(r)) if r == "bad token")
        );
        assert!(
            matches!(parse(&resp(404, ""), None), Err(ConnectorError::NotFound(r)) if r == "not found")
        );
        assert!(
            matches!(parse(&resp(422, r#"{"errors":[{"message":"no title"}]}"#), None), Err(ConnectorError::Refused { status: 422, reason }) if reason == "no title")
        );
        assert!(
            matches!(parse(&resp(400, r#"{"error":"invalid_grant","error_description":"revoked"}"#), None), Err(ConnectorError::Refused { status: 400, reason }) if reason == "invalid_grant: revoked"),
            "an OAuth error names its code and its sentence"
        );
        assert!(
            matches!(parse(&resp(400, r#"{"error":"invalid_request"}"#), None), Err(ConnectorError::Refused { status: 400, reason }) if reason == "invalid_request"),
            "a code alone is the sentence"
        );
        assert!(
            matches!(parse(&resp(503, r#"{"error":"down"}"#), None), Err(ConnectorError::Upstream { status: 503, reason }) if reason == "down")
        );
        let limited = Response {
            status: 429,
            headers: vec![("retry-after".into(), "5".into())],
            body: vec![],
        };
        assert!(matches!(
            parse(&limited, None),
            Err(ConnectorError::RateLimited { retry_after: 5 })
        ));
    }

    #[test]
    fn the_size_cap_holds() {
        let big = Response {
            status: 200,
            headers: vec![],
            body: vec![b'x'; MAX_BODY_BYTES + 1],
        };
        assert!(matches!(
            parse(&big, None),
            Err(ConnectorError::TooLarge(_))
        ));
    }
}
