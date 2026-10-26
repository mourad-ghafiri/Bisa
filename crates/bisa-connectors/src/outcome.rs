//! An answer read into what the run records: the status, the body as JSON,
//! and the part the definition selects.

use crate::error::ConnectorError;
use crate::http::Response;
use crate::spec::Expect;
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
        // A blank string is no sentence either: the next candidate may be.
        Value::String(_) | Value::Object(_) | Value::Array(_) | Value::Null => None,
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
        429 => "too many requests", // LCOV_EXCL_LINE: a 429 is read as RateLimited before the words are asked for
        500 => "internal error",
        502 => "bad gateway",
        503 => "unavailable",
        504 => "gateway timeout",
        _ => "error",
    }
}

/// Read a response: a non-2xx is the error its status names, with the body's
/// own sentence; a 3xx is a refusal — the crate follows no redirect, so one
/// names a definition's wrong URL, never a reason to stop a write; a 2xx
/// that fails the operation's `expect` is a refusal in the platform's own
/// words; any other 2xx is an [`Outcome`]. `now` reads a dated
/// `Retry-After`.
pub fn parse(
    resp: &Response,
    select: Option<&str>,
    expect: Option<&Expect>,
    now: u64,
) -> Result<Outcome, ConnectorError> {
    if resp.body.len() > MAX_BODY_BYTES {
        return Err(ConnectorError::TooLarge(resp.body.len()));
    }
    let body = body_value(&resp.body);
    let status = resp.status;
    if (300..400).contains(&status) {
        let location = resp
            .headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("location"))
            .map(|(_, v)| v.trim())
            .filter(|v| !v.is_empty());
        let reason = match location {
            Some(to) => format!("redirected to {to}; a connector follows no redirect"),
            None => "redirected; a connector follows no redirect".to_string(),
        };
        return Err(ConnectorError::Refused { status, reason });
    }
    if !(200..300).contains(&status) {
        let reason = explanation(&body).unwrap_or_else(|| status_text(status).to_string());
        return Err(match status {
            // A 403 is the platform's own refusal and keeps its status: a
            // check reports what the platform said, never a 401 it did not.
            401 => ConnectorError::NotAuthenticated(reason),
            404 => ConnectorError::NotFound(reason),
            429 => ConnectorError::RateLimited {
                retry_after: crate::retry::retry_after(&resp.headers, now)
                    .map_or(0, |d| d.as_secs()),
            },
            400..=499 => ConnectorError::Refused { status, reason },
            _ => ConnectorError::Upstream { status, reason },
        });
    }
    if let Some(expect) = expect {
        if let Some(reason) = unmet(expect, &body) {
            return Err(ConnectorError::Refused { status, reason });
        }
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

/// Why a 2xx answer fails the operation's expectation, in the platform's
/// words when it has any; nothing when the expectation holds.
fn unmet(expect: &Expect, body: &Value) -> Option<String> {
    let found = json_path(body, &expect.path);
    let holds = if expect.absent {
        match found {
            None | Some(Value::Null) => true,
            Some(Value::Array(items)) => items.is_empty(),
            Some(_) => false,
        }
    } else {
        expect
            .equals
            .as_ref()
            .is_some_and(|want| found == Some(want))
    };
    if holds {
        return None;
    }
    let said = expect
        .reason
        .as_deref()
        .and_then(|p| json_path(body, p))
        .and_then(|v| match v {
            Value::String(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
            Value::String(_) | Value::Object(_) | Value::Array(_) | Value::Null => None,
            other => Some(other.to_string()),
        })
        .map(|s| s.chars().take(300).collect::<String>())
        .or_else(|| explanation(body));
    Some(match (said, &expect.equals) {
        (Some(s), _) => s,
        (None, _) if expect.absent => format!("the answer carries {}", expect.path),
        (None, Some(want)) => format!("the answer's {} is not {want}", expect.path),
        (None, None) => format!(
            "the answer's {} is not what the operation expects",
            expect.path
        ),
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
            None,
            0,
        )
        .unwrap();
        assert_eq!(out.selected, json!(1));
        assert_eq!(out.body["data"]["items"][0]["id"], json!(1));
        let err = parse(&resp(200, r#"{"data":{}}"#), Some("data.items"), None, 0).unwrap_err();
        assert!(matches!(err, ConnectorError::SelectMissing { path } if path == "data.items"));
        assert_eq!(
            parse(&resp(200, "[1,2]"), None, None, 0).unwrap().selected,
            json!([1, 2])
        );
    }

    #[test]
    fn a_non_json_body_is_text_and_an_empty_one_is_null() {
        assert_eq!(
            parse(&resp(200, "ok"), None, None, 0).unwrap().body,
            json!("ok")
        );
        assert_eq!(
            parse(&resp(204, ""), None, None, 0).unwrap().body,
            Value::Null
        );
    }

    #[test]
    fn statuses_map_to_their_errors_with_the_bodys_own_words() {
        assert!(
            matches!(parse(&resp(401, r#"{"message":"bad token"}"#), None, None, 0), Err(ConnectorError::NotAuthenticated(r)) if r == "bad token")
        );
        assert!(
            matches!(parse(&resp(404, ""), None, None, 0), Err(ConnectorError::NotFound(r)) if r == "not found")
        );
        assert!(
            matches!(parse(&resp(422, r#"{"errors":[{"message":"no title"}]}"#), None, None, 0), Err(ConnectorError::Refused { status: 422, reason }) if reason == "no title")
        );
        assert!(
            matches!(parse(&resp(400, r#"{"error":"invalid_grant","error_description":"revoked"}"#), None, None, 0), Err(ConnectorError::Refused { status: 400, reason }) if reason == "invalid_grant: revoked"),
            "an OAuth error names its code and its sentence"
        );
        assert!(
            matches!(parse(&resp(400, r#"{"error":"invalid_request"}"#), None, None, 0), Err(ConnectorError::Refused { status: 400, reason }) if reason == "invalid_request"),
            "a code alone is the sentence"
        );
        assert!(
            matches!(parse(&resp(503, r#"{"error":"down"}"#), None, None, 0), Err(ConnectorError::Upstream { status: 503, reason }) if reason == "down")
        );
        let limited = Response {
            status: 429,
            headers: vec![("retry-after".into(), "5".into())],
            body: vec![],
        };
        assert!(matches!(
            parse(&limited, None, None, 0),
            Err(ConnectorError::RateLimited { retry_after: 5 })
        ));
        // A dated Retry-After is read against the clock, never against zero.
        let dated = Response {
            status: 429,
            headers: vec![("retry-after".into(), "Sun, 06 Nov 1994 08:49:47 GMT".into())],
            body: vec![],
        };
        let at = crate::retry::http_date("Sun, 06 Nov 1994 08:49:37 GMT").unwrap();
        assert!(matches!(
            parse(&dated, None, None, at),
            Err(ConnectorError::RateLimited { retry_after: 10 })
        ));
    }

    #[test]
    fn a_redirect_is_a_refusal_naming_where_to() {
        let moved = Response {
            status: 301,
            headers: vec![("location".into(), "https://api.example.com/v2/".into())],
            body: vec![],
        };
        let err = parse(&moved, None, None, 0).unwrap_err();
        assert!(
            matches!(&err, ConnectorError::Refused { status: 301, reason } if reason.starts_with("redirected to https://api.example.com/v2/")),
            "{err}"
        );
        assert!(
            err.is_refusal(),
            "a wrong URL is the definition's, never a stop"
        );
        assert!(matches!(
            parse(&resp(302, ""), None, None, 0),
            Err(ConnectorError::Refused { status: 302, .. })
        ));
    }

    #[test]
    fn a_2xx_that_fails_the_expectation_is_a_refusal_in_the_platforms_words() {
        let ok_flag = Expect {
            path: "ok".into(),
            equals: Some(json!(true)),
            absent: false,
            reason: Some("error".into()),
        };
        let fine = parse(
            &resp(200, r#"{"ok":true,"channels":[]}"#),
            Some("channels"),
            Some(&ok_flag),
            0,
        )
        .unwrap();
        assert_eq!(fine.selected, json!([]));
        let err = parse(
            &resp(200, r#"{"ok":false,"error":"invalid_auth"}"#),
            Some("channels"),
            Some(&ok_flag),
            0,
        )
        .unwrap_err();
        assert!(
            matches!(&err, ConnectorError::Refused { status: 200, reason } if reason == "invalid_auth"),
            "the expectation is read before the select: {err}"
        );
        let no_errors = Expect {
            path: "errors".into(),
            equals: None,
            absent: true,
            reason: Some("errors.0.message".into()),
        };
        assert!(parse(
            &resp(200, r#"{"data":{"viewer":{}}}"#),
            None,
            Some(&no_errors),
            0
        )
        .is_ok());
        assert!(parse(
            &resp(200, r#"{"data":{},"errors":[]}"#),
            None,
            Some(&no_errors),
            0
        )
        .is_ok());
        let err = parse(
            &resp(200, r#"{"errors":[{"message":"not authenticated"}]}"#),
            None,
            Some(&no_errors),
            0,
        )
        .unwrap_err();
        assert!(
            matches!(&err, ConnectorError::Refused { reason, .. } if reason == "not authenticated"),
            "{err}"
        );
        let code_ok = Expect {
            path: "error.code".into(),
            equals: Some(json!("ok")),
            absent: false,
            reason: Some("error.message".into()),
        };
        let err = parse(
            &resp(200, r#"{"error":{"code":"access_token_invalid","message":"The access token is invalid"}}"#),
            None,
            Some(&code_ok),
            0,
        )
        .unwrap_err();
        assert!(
            matches!(&err, ConnectorError::Refused { reason, .. } if reason == "The access token is invalid"),
            "{err}"
        );
        // No reason path and nothing the common shapes name: the rule itself.
        let flag = Expect {
            path: "authenticated".into(),
            equals: Some(json!(true)),
            absent: false,
            reason: None,
        };
        let err = parse(
            &resp(200, r#"{"authenticated":false}"#),
            None,
            Some(&flag),
            0,
        )
        .unwrap_err();
        assert!(
            matches!(&err, ConnectorError::Refused { reason, .. } if reason == "the answer's authenticated is not true"),
            "{err}"
        );
    }

    #[test]
    fn the_size_cap_holds() {
        let big = Response {
            status: 200,
            headers: vec![],
            body: vec![b'x'; MAX_BODY_BYTES + 1],
        };
        assert!(matches!(
            parse(&big, None, None, 0),
            Err(ConnectorError::TooLarge(_))
        ));
    }

    // added by the coverage pass: outcome.rs

    /// A path with an empty segment names nothing; a cursor given as a
    /// number is read as its text.
    #[test]
    fn an_empty_segment_names_nothing_and_a_numeric_cursor_is_its_text() {
        let body = json!({"a": {"b": 1}, "next": 5, "last": ""});
        assert_eq!(json_path(&body, "a..b"), None);
        assert_eq!(next_cursor(&body, "next").as_deref(), Some("5"));
        assert_eq!(next_cursor(&body, "last"), None);
    }

    /// What a platform says of its refusal: a code with an empty sentence
    /// is no explanation, a number is read as its text, an object or a list
    /// at a candidate is passed over.
    #[test]
    fn an_explanation_is_the_first_candidate_that_reads_as_words() {
        assert_eq!(
            explanation(&json!({"error": "bad_scope", "error_description": "  "})).as_deref(),
            Some("bad_scope")
        );
        assert_eq!(explanation(&json!({"message": 7})).as_deref(), Some("7"));
        assert_eq!(
            explanation(&json!({"message": "   ", "error": "quota"})).as_deref(),
            Some("quota"),
            "a blank sentence is passed over for the next"
        );
        assert_eq!(explanation(&json!({"message": ""})), None);
        assert_eq!(
            explanation(&json!({"message": {"deep": 1}, "detail": ["x"]})),
            None
        );
    }

    /// Every status the reader has a word for, and the word for the rest.
    #[test]
    fn every_failing_status_has_its_word() {
        for (status, word) in [
            (400, "bad request"),
            (401, "unauthorised"),
            (403, "forbidden"),
            (404, "not found"),
            (409, "conflict"),
            (422, "unprocessable"),
            (500, "internal error"),
            (502, "bad gateway"),
            (503, "unavailable"),
            (504, "gateway timeout"),
            (418, "error"),
        ] {
            let err = parse(&resp(status, ""), None, None, 0).unwrap_err();
            assert!(err.to_string().contains(word), "{status}: {err}");
        }
    }

    /// An expectation that is not met says why in the platform's words when
    /// it has any, else in the operation's: a value where none should be, a
    /// value that is not the one wanted, a path that is not as expected.
    #[test]
    fn an_unmet_expectation_says_why() {
        let expect = |path: &str, equals: Option<Value>, absent: bool, reason: Option<&str>| {
            crate::spec::Expect {
                path: path.into(),
                equals,
                absent,
                reason: reason.map(str::to_string),
            }
        };
        let present = expect("errors", None, true, None);
        let err = parse(&resp(200, r#"{"errors": "one"}"#), None, Some(&present), 0).unwrap_err();
        assert!(err.to_string().contains("carries errors"), "{err}");
        let wanted = expect("ok", Some(json!(true)), false, None);
        let err = parse(&resp(200, r#"{"ok": false}"#), None, Some(&wanted), 0).unwrap_err();
        assert!(err.to_string().contains("is not true"), "{err}");
        let vague = expect("ok", None, false, None);
        let err = parse(&resp(200, r#"{"ok": false}"#), None, Some(&vague), 0).unwrap_err();
        assert!(
            err.to_string().contains("not what the operation expects"),
            "{err}"
        );
        let numbered = expect("ok", Some(json!(true)), false, Some("code"));
        let err = parse(
            &resp(200, r#"{"ok": false, "code": 42}"#),
            None,
            Some(&numbered),
            0,
        )
        .unwrap_err();
        assert!(err.to_string().contains("42"), "{err}");
        let shaped = expect("ok", Some(json!(true)), false, Some("why"));
        let err = parse(
            &resp(200, r#"{"ok": false, "why": {"deep": 1}}"#),
            None,
            Some(&shaped),
            0,
        )
        .unwrap_err();
        assert!(err.to_string().contains("is not true"), "{err}");
        // An absent expectation holds on an empty list.
        let none = expect("errors", None, true, None);
        assert!(parse(&resp(200, r#"{"errors": []}"#), None, Some(&none), 0).is_ok());
        // A reason that reads as words is said; a blank one gives way to the
        // body's own explanation.
        let worded = expect("ok", Some(json!(true)), false, Some("why"));
        let err = parse(
            &resp(200, r#"{"ok": false, "why": "  not today  "}"#),
            None,
            Some(&worded),
            0,
        )
        .unwrap_err();
        assert!(err.to_string().contains("not today"), "{err}");
        let blank = r#"{"ok": false, "why": "   ", "message": "the body's own words"}"#;
        let err = parse(&resp(200, blank), None, Some(&worded), 0).unwrap_err();
        assert!(err.to_string().contains("the body's own words"), "{err}");
    }
}
