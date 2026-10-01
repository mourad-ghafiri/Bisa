//! From a call shape and its values to a request: parameters bound by kind,
//! the URL built segment by segment, the query form-encoded, the body
//! encoded as its kind says ([`crate::body`]). Pure — nothing here sends.

use crate::body::{self, Encoded, Sources};
use crate::creds::Entropy;
use crate::error::ConnectorError;
use crate::files::FileData;
use crate::http::Request;
use crate::spec::{CallBody, CallSpec, ParamKind, ParamSpec};
use crate::template::{self, Encode, Values};
use serde_json::Value;
use std::collections::BTreeMap;
use url::Url;

pub use bisa_netrules::RESERVED_HEADERS;

/// A step's rendered parameter text, read by each parameter's kind. An
/// unknown name, a missing required one, or text the kind cannot read is a
/// [`ConnectorError::BadParam`]; an absent optional one is `Null`, which the
/// templates treat as absent.
pub fn bind_params(
    specs: &[ParamSpec],
    given: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, Value>, ConnectorError> {
    for name in given.keys() {
        if !specs.iter().any(|p| &p.name == name) {
            return Err(ConnectorError::BadParam {
                name: name.clone(),
                why: "not a parameter of this operation".into(),
            });
        }
    }
    let mut out = BTreeMap::new();
    for spec in specs {
        let text = given
            .get(&spec.name)
            .map(|t| t.trim())
            .filter(|t| !t.is_empty());
        let value = match text {
            None if spec.required => {
                return Err(ConnectorError::BadParam {
                    name: spec.name.clone(),
                    why: "required".into(),
                })
            }
            None => Value::Null,
            Some(t) => coerce(spec, t)?,
        };
        out.insert(spec.name.clone(), value);
    }
    Ok(out)
}

fn coerce(spec: &ParamSpec, text: &str) -> Result<Value, ConnectorError> {
    let bad = |why: String| ConnectorError::BadParam {
        name: spec.name.clone(),
        why,
    };
    Ok(match spec.kind {
        ParamKind::Text => Value::String(text.to_string()),
        ParamKind::Number => {
            if let Ok(n) = text.parse::<i64>() {
                Value::Number(n.into())
            } else if let Some(n) = text
                .parse::<f64>()
                .ok()
                .and_then(serde_json::Number::from_f64)
            {
                Value::Number(n)
            } else {
                return Err(bad(format!("{text:?} is not a number")));
            }
        }
        ParamKind::Bool => match text.to_ascii_lowercase().as_str() {
            "true" => Value::Bool(true),
            "false" => Value::Bool(false),
            _ => return Err(bad(format!("{text:?} is not true or false"))),
        },
        ParamKind::Json => serde_json::from_str(text).map_err(|e| bad(format!("not JSON: {e}")))?,
        // A path, read later through the files port; here it is text.
        ParamKind::File => Value::String(text.to_string()),
    })
}

/// The base URL and the path rendered and joined, the query pairs appended.
pub fn build_url(spec: &CallSpec, values: &Values<'_>) -> Result<Url, ConnectorError> {
    let base_text = template::render(&spec.base_url, values, Encode::Authority)?;
    let mut url = Url::parse(&base_text).map_err(|e| {
        ConnectorError::BadDefinition(format!("base URL {base_text:?} does not parse: {e}"))
    })?;
    let path = template::render(&spec.path, values, Encode::PathSegment)?;
    if path.split('/').any(|seg| seg == "..") {
        return Err(ConnectorError::BadDefinition(format!(
            "path {path:?} climbs out of the base URL"
        )));
    }
    let joined = format!(
        "{}/{}",
        url.path().trim_end_matches('/'),
        path.trim_start_matches('/')
    );
    url.set_path(&joined);
    let mut pairs: Vec<(String, String)> = Vec::new();
    for (name, tmpl) in &spec.query {
        if template::names_absent(tmpl, values)? {
            continue;
        }
        pairs.push((name.clone(), template::render(tmpl, values, Encode::Text)?));
    }
    if !pairs.is_empty() {
        url.query_pairs_mut().extend_pairs(pairs);
    }
    Ok(url)
}

/// The definition's headers rendered; a reserved or unprintable one refused.
pub fn build_headers(
    spec: &CallSpec,
    values: &Values<'_>,
) -> Result<Vec<(String, String)>, ConnectorError> {
    let mut out = Vec::new();
    for (name, tmpl) in &spec.headers {
        let lower = name.to_ascii_lowercase();
        if bisa_netrules::is_reserved_header(&lower) {
            return Err(ConnectorError::BadDefinition(format!(
                "header {name:?} is set by the platform, never by a definition"
            )));
        }
        let value = template::render(tmpl, values, Encode::Text)?;
        if !value.chars().all(|c| c == '\t' || (' '..='~').contains(&c)) {
            return Err(ConnectorError::BadParam {
                name: name.clone(),
                why: "a header value must be printable ASCII".into(),
            });
        }
        out.push((name.clone(), value));
    }
    Ok(out)
}

/// The body encoded as its kind says, with the files read for its `file`
/// parameters; nothing for an operation that sends none.
pub fn build_body(
    spec: &CallSpec,
    values: &Values<'_>,
    files: &BTreeMap<String, FileData>,
    entropy: &dyn Entropy,
) -> Result<Option<Encoded>, ConnectorError> {
    body::encode(&Sources {
        spec,
        values,
        files,
        entropy,
    })
}

/// The whole request, without its credential. The body's kind decides the
/// `Content-Type`; a definition's own header wins over it, except for a
/// multipart body whose boundary only the client knows.
pub fn build_request(
    spec: &CallSpec,
    values: &Values<'_>,
    files: &BTreeMap<String, FileData>,
    entropy: &dyn Entropy,
) -> Result<Request, ConnectorError> {
    let url = build_url(spec, values)?;
    let mut req = Request::new(spec.method, url, spec.timeout);
    req.headers = build_headers(spec, values)?;
    if let Some(encoded) = build_body(spec, values, files, entropy)? {
        let multipart = matches!(spec.body, Some(CallBody::Multipart { .. }));
        if multipart {
            req.headers
                .retain(|(k, _)| !k.eq_ignore_ascii_case("content-type"));
        }
        if req.header("content-type").is_none() {
            req.headers
                .push(("content-type".into(), encoded.content_type));
        }
        req.body = Some(encoded.bytes);
    }
    if req.header("accept").is_none() {
        req.headers
            .push(("accept".into(), "application/json".into()));
    }
    // A write's key, so the platform tells a resend from a second request.
    if let (Some(idem), Some(key)) = (&spec.idempotency, &spec.idempotency_key) {
        if bisa_netrules::is_reserved_header(&idem.header) {
            return Err(ConnectorError::BadDefinition(format!(
                "idempotency header {:?} is set by the platform, never by a definition",
                idem.header
            )));
        }
        req.headers.push((idem.header.clone(), key.clone()));
    }
    Ok(req)
}
