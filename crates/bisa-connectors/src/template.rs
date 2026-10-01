//! The definition's own placeholders: `{account.<param>}` and
//! `{params.<name>}`, read by the same rules as the platform's one template
//! grammar — `{{` and `}}` are a literal brace each, a `{` opens a
//! placeholder, and a substituted value is text that is never scanned again.
//!
//! The run's placeholders (`{inputs.…}`, `{steps.…}`) were rendered into the
//! step's parameter values before they reached this crate, so this scanner
//! knows two roots and refuses every other.

use crate::error::ConnectorError;
use serde_json::Value;
use std::collections::BTreeMap;

/// What a template may read: the account's non-secret params and the step's
/// bound params.
#[derive(Clone, Copy, Debug)]
pub struct Values<'a> {
    pub account: &'a BTreeMap<String, Value>,
    pub params: &'a BTreeMap<String, Value>,
}

/// How a substituted value is written into its place.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Encode {
    /// Into a host name: only `[A-Za-z0-9.-]` may appear.
    Authority,
    /// Into one path segment: percent-encoded, `-._~` and alphanumerics kept.
    PathSegment,
    /// As it is — a header or a query value the URL builder encodes itself.
    Text,
}

enum Segment<'a> {
    Text(&'a str),
    Placeholder(&'a str),
}

/// Scan `tmpl` into text and placeholder keys; braces must balance.
fn segments(tmpl: &str) -> Result<Vec<Segment<'_>>, ConnectorError> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    while pos < tmpl.len() {
        let rest = &tmpl[pos..];
        let Some(brace) = rest.find(['{', '}']) else {
            out.push(Segment::Text(rest));
            break;
        };
        if brace > 0 {
            out.push(Segment::Text(&rest[..brace]));
            pos += brace;
            continue;
        }
        if rest.starts_with("{{") {
            out.push(Segment::Text("{"));
            pos += 2;
        } else if rest.starts_with("}}") || rest.starts_with('}') {
            out.push(Segment::Text("}"));
            pos += if rest.starts_with("}}") { 2 } else { 1 };
        } else {
            let after = &rest[1..];
            match after.find('}') {
                Some(close) if !after[..close].contains('{') => {
                    out.push(Segment::Placeholder(&after[..close]));
                    pos += 1 + close + 1;
                }
                _ => {
                    return Err(ConnectorError::BadDefinition(format!(
                        "unbalanced brace in template {tmpl:?}"
                    )))
                }
            }
        }
    }
    Ok(out)
}

/// The value a key names, or why not.
fn lookup<'a>(key: &str, values: &'a Values<'_>) -> Result<&'a Value, ConnectorError> {
    let (root, name) = key.split_once('.').ok_or_else(|| {
        ConnectorError::BadDefinition(format!(
            "{{{key}}} is not a placeholder a connector definition may read; the roots are account.<param> and params.<name>"
        ))
    })?;
    let map = match root {
        "account" => values.account,
        "params" => values.params,
        _ => {
            return Err(ConnectorError::BadDefinition(format!(
                "{{{key}}} is not a placeholder a connector definition may read; the roots are account.<param> and params.<name>"
            )))
        }
    };
    match map.get(name) {
        Some(Value::Null) | None => Err(ConnectorError::Unresolved(key.to_string())),
        Some(v) => Ok(v),
    }
}

/// A JSON value as text: a string bare, anything else compact.
pub fn value_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn write(encode: Encode, key: &str, value: &str, out: &mut String) -> Result<(), ConnectorError> {
    match encode {
        Encode::Text => out.push_str(value),
        Encode::PathSegment => {
            const KEEP: &percent_encoding::AsciiSet = &percent_encoding::NON_ALPHANUMERIC
                .remove(b'-')
                .remove(b'.')
                .remove(b'_')
                .remove(b'~');
            out.push_str(&percent_encoding::utf8_percent_encode(value, KEEP).to_string());
        }
        Encode::Authority => {
            if value.is_empty()
                || !value
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
            {
                return Err(ConnectorError::BadParam {
                    name: key.to_string(),
                    why: format!("{value:?} cannot be part of a host name"),
                });
            }
            out.push_str(value);
        }
    }
    Ok(())
}

/// Render `tmpl` against `values`, writing each substituted value as `encode`
/// says; the template's own text is written as it is.
pub fn render(tmpl: &str, values: &Values<'_>, encode: Encode) -> Result<String, ConnectorError> {
    let mut out = String::with_capacity(tmpl.len());
    for segment in segments(tmpl)? {
        match segment {
            Segment::Text(t) => out.push_str(t),
            Segment::Placeholder(key) => {
                let v = lookup(key, values)?;
                write(encode, key, &value_text(v), &mut out)?;
            }
        }
    }
    Ok(out)
}

/// Whether the template ever names an absent value — the query builder drops
/// such a pair rather than sending an empty one.
pub fn names_absent(tmpl: &str, values: &Values<'_>) -> Result<bool, ConnectorError> {
    for segment in segments(tmpl)? {
        if let Segment::Placeholder(key) = segment {
            match lookup(key, values) {
                Ok(_) => {}
                Err(ConnectorError::Unresolved(_)) => return Ok(true),
                Err(e) => return Err(e),
            }
        }
    }
    Ok(false)
}

/// Every placeholder key in `tmpl`, in order.
pub fn keys(tmpl: &str) -> Result<Vec<&str>, ConnectorError> {
    Ok(segments(tmpl)?
        .into_iter()
        .filter_map(|s| match s {
            Segment::Placeholder(k) => Some(k),
            Segment::Text(_) => None,
        })
        .collect())
}

/// When `tmpl` is exactly one `{params.<name>}` and nothing else, that name —
/// the shape a body leaf takes to carry a typed value rather than text.
pub fn typed_leaf(tmpl: &str) -> Option<&str> {
    let inner = tmpl.strip_prefix('{')?.strip_suffix('}')?;
    if inner.contains(['{', '}']) {
        return None;
    }
    inner.strip_prefix("params.").filter(|n| !n.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn values() -> (BTreeMap<String, Value>, BTreeMap<String, Value>) {
        (
            BTreeMap::from([("site".to_string(), json!("acme"))]),
            BTreeMap::from([
                ("id".to_string(), json!("a/b c")),
                ("n".to_string(), json!(3)),
                ("opt".to_string(), Value::Null),
            ]),
        )
    }

    #[test]
    fn renders_both_roots_and_doubles_braces() {
        let (a, p) = values();
        let v = Values {
            account: &a,
            params: &p,
        };
        assert_eq!(
            render("{account.site} {params.n} {{x}}", &v, Encode::Text).unwrap(),
            "acme 3 {x}"
        );
        assert_eq!(
            render("/issue/{params.id}", &v, Encode::PathSegment).unwrap(),
            "/issue/a%2Fb%20c"
        );
        assert_eq!(
            render("{account.site}.example.com", &v, Encode::Authority).unwrap(),
            "acme.example.com"
        );
        assert!(matches!(
            render("{params.id}", &v, Encode::Authority),
            Err(ConnectorError::BadParam { .. })
        ));
    }

    #[test]
    fn unknown_roots_absent_values_and_bad_braces_are_named() {
        let (a, p) = values();
        let v = Values {
            account: &a,
            params: &p,
        };
        assert!(matches!(
            render("{inputs.x}", &v, Encode::Text),
            Err(ConnectorError::BadDefinition(_))
        ));
        assert!(
            matches!(render("{params.opt}", &v, Encode::Text), Err(ConnectorError::Unresolved(k)) if k == "params.opt")
        );
        assert!(matches!(
            render("{params.nope}", &v, Encode::Text),
            Err(ConnectorError::Unresolved(_))
        ));
        assert!(matches!(
            render("{unclosed", &v, Encode::Text),
            Err(ConnectorError::BadDefinition(_))
        ));
        assert!(names_absent("x={params.opt}", &v).unwrap());
        assert!(!names_absent("x={params.n}", &v).unwrap());
        assert_eq!(
            keys("{account.site}/{params.id}").unwrap(),
            vec!["account.site", "params.id"]
        );
    }

    #[test]
    fn a_typed_leaf_is_exactly_one_params_placeholder() {
        assert_eq!(typed_leaf("{params.n}"), Some("n"));
        assert_eq!(typed_leaf(" {params.n}"), None);
        assert_eq!(typed_leaf("{params.n} x"), None);
        assert_eq!(typed_leaf("{account.site}"), None);
        assert_eq!(typed_leaf("{{params.n}}"), None);
    }
}
