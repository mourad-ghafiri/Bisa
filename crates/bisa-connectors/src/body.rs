//! How a body reaches the wire: one encoder per kind behind one trait, chosen
//! by the definition's `kind`. Pure — values and bytes in, bytes out; the
//! only randomness is a multipart boundary from the [`Entropy`] port.

use crate::creds::Entropy;
use crate::error::ConnectorError;
use crate::files::FileData;
use crate::spec::{CallBody, CallSpec, ParamKind, Part, PartSource};
use crate::template::{self, Encode, Values};
use serde_json::Value;
use std::collections::BTreeMap;

/// A body ready to send.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Encoded {
    pub content_type: String,
    pub bytes: Vec<u8>,
}

/// What an encoder reads: the call, its values, the files read for its
/// `file` parameters, and the entropy a boundary is drawn from.
pub struct Sources<'a> {
    pub spec: &'a CallSpec,
    pub values: &'a Values<'a>,
    pub files: &'a BTreeMap<String, FileData>,
    pub entropy: &'a dyn Entropy,
}

/// One body kind's way onto the wire.
pub trait BodyEncoder {
    fn encode(&self, cx: &Sources<'_>) -> Result<Encoded, ConnectorError>;
}

/// The encoder a body's kind names — the one place the kinds meet the
/// encoders.
pub fn encoder_for(body: &CallBody) -> Box<dyn BodyEncoder + '_> {
    match body {
        CallBody::Json { value } => Box::new(JsonEncoder(value)),
        CallBody::Form { fields } => Box::new(FormEncoder(fields)),
        CallBody::Multipart { parts } => Box::new(MultipartEncoder(parts)),
        CallBody::Raw { content_type, from } => Box::new(RawEncoder { content_type, from }),
    }
}

/// Encode the call's body, if it has one.
pub fn encode(cx: &Sources<'_>) -> Result<Option<Encoded>, ConnectorError> {
    match &cx.spec.body {
        None => Ok(None),
        Some(body) => encoder_for(body).encode(cx).map(Some),
    }
}

/// `application/json`: every string leaf rendered; a leaf that is exactly one
/// typed placeholder carries the typed value instead of text; a member or an
/// item that is exactly one placeholder naming an absent optional parameter
/// is left out, as a query pair is — a platform reads an empty `description`
/// or a `null` filter as a value, and an absent one as none.
pub struct JsonEncoder<'a>(pub &'a Value);

impl BodyEncoder for JsonEncoder<'_> {
    fn encode(&self, cx: &Sources<'_>) -> Result<Encoded, ConnectorError> {
        let rendered = render_leaves(self.0, cx.spec, cx.values)?;
        let bytes = serde_json::to_vec(&rendered).map_err(|e| {
            ConnectorError::BadDefinition(format!("the body cannot be written as JSON: {e}"))
        })?;
        Ok(Encoded {
            content_type: "application/json".into(),
            bytes,
        })
    }
}

fn render_leaves(v: &Value, spec: &CallSpec, values: &Values<'_>) -> Result<Value, ConnectorError> {
    Ok(match v {
        Value::String(s) => {
            if let Some(name) = template::typed_leaf(s) {
                if spec.param(name).is_some_and(|p| p.kind != ParamKind::Text) {
                    return values
                        .params
                        .get(name)
                        .cloned()
                        .ok_or_else(|| ConnectorError::Unresolved(format!("params.{name}")));
                }
            }
            Value::String(template::render(s, values, Encode::Text)?)
        }
        Value::Array(items) => Value::Array(
            items
                .iter()
                .filter(|i| !left_out(i, values))
                .map(|i| render_leaves(i, spec, values))
                .collect::<Result<_, _>>()?,
        ),
        Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (k, item) in map {
                if left_out(item, values) {
                    continue;
                }
                out.insert(k.clone(), render_leaves(item, spec, values)?);
            }
            Value::Object(out)
        }
        other => other.clone(),
    })
}

/// A leaf that is exactly one placeholder naming an absent parameter — left
/// out of its object or array rather than rendered.
fn left_out(leaf: &Value, values: &Values<'_>) -> bool {
    matches!(leaf, Value::String(s) if template::absent_leaf(s, values))
}

/// `application/x-www-form-urlencoded`: each field rendered as text; a field
/// that names an absent optional parameter is dropped, as a query pair is.
pub struct FormEncoder<'a>(pub &'a BTreeMap<String, String>);

impl BodyEncoder for FormEncoder<'_> {
    fn encode(&self, cx: &Sources<'_>) -> Result<Encoded, ConnectorError> {
        let mut form = url::form_urlencoded::Serializer::new(String::new());
        for (name, tmpl) in self.0 {
            if template::names_absent(tmpl, cx.values)? {
                continue;
            }
            form.append_pair(name, &template::render(tmpl, cx.values, Encode::Text)?);
        }
        Ok(Encoded {
            content_type: "application/x-www-form-urlencoded".into(),
            bytes: form.finish().into_bytes(),
        })
    }
}

/// `multipart/form-data` under a boundary drawn from the entropy port: text
/// parts rendered (one naming an absent optional parameter is dropped), file
/// parts carrying the bytes read for their parameter.
pub struct MultipartEncoder<'a>(pub &'a [Part]);

impl BodyEncoder for MultipartEncoder<'_> {
    fn encode(&self, cx: &Sources<'_>) -> Result<Encoded, ConnectorError> {
        let boundary = boundary(cx.entropy);
        let mut out: Vec<u8> = Vec::new();
        for part in self.0 {
            let (bytes, filename, content_type): (Vec<u8>, Option<String>, Option<String>) =
                match &part.source {
                    PartSource::Text { text } => {
                        if template::names_absent(text, cx.values)? {
                            continue;
                        }
                        (
                            template::render(text, cx.values, Encode::Text)?.into_bytes(),
                            part.filename.clone(),
                            part.content_type.clone(),
                        )
                    }
                    PartSource::File { file } => {
                        let data = cx
                            .files
                            .get(file)
                            .ok_or_else(|| ConnectorError::Unresolved(format!("params.{file}")))?;
                        (
                            data.bytes.clone(),
                            Some(
                                part.filename
                                    .clone()
                                    .unwrap_or_else(|| data.filename.clone()),
                            ),
                            Some(
                                part.content_type
                                    .clone()
                                    .or_else(|| data.content_type.clone())
                                    .unwrap_or_else(|| "application/octet-stream".into()),
                            ),
                        )
                    }
                };
            out.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
            let mut disposition = format!(
                "Content-Disposition: form-data; name=\"{}\"",
                quote(&part.name)
            );
            if let Some(f) = filename {
                disposition.push_str(&format!("; filename=\"{}\"", quote(&f)));
            }
            out.extend_from_slice(disposition.as_bytes());
            out.extend_from_slice(b"\r\n");
            if let Some(ct) = content_type {
                out.extend_from_slice(format!("Content-Type: {ct}\r\n").as_bytes());
            }
            out.extend_from_slice(b"\r\n");
            out.extend_from_slice(&bytes);
            out.extend_from_slice(b"\r\n");
        }
        out.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
        Ok(Encoded {
            content_type: format!("multipart/form-data; boundary={boundary}"),
            bytes: out,
        })
    }
}

/// A boundary no body is likely to contain: a fixed prefix and 24 hex digits
/// from the port.
fn boundary(entropy: &dyn Entropy) -> String {
    let mut bytes = [0u8; 12];
    entropy.fill(&mut bytes);
    format!("bisa-{}", hex::encode(bytes))
}

/// A part or file name inside quotes: the quote and the backslash escaped,
/// line breaks dropped.
fn quote(name: &str) -> String {
    name.chars()
        .filter(|c| *c != '\r' && *c != '\n')
        .flat_map(|c| match c {
            '"' => vec!['\\', '"'],
            '\\' => vec!['\\', '\\'],
            other => vec![other],
        })
        .collect()
}

/// One parameter's bytes under the declared content type: a `file`
/// parameter's as read, another kind's as its rendered text.
pub struct RawEncoder<'a> {
    pub content_type: &'a str,
    pub from: &'a str,
}

impl BodyEncoder for RawEncoder<'_> {
    fn encode(&self, cx: &Sources<'_>) -> Result<Encoded, ConnectorError> {
        let is_file = cx
            .spec
            .param(self.from)
            .is_some_and(|p| p.kind == ParamKind::File);
        let bytes = if is_file {
            cx.files
                .get(self.from)
                .map(|d| d.bytes.clone())
                .ok_or_else(|| ConnectorError::Unresolved(format!("params.{}", self.from)))?
        } else {
            match cx.values.params.get(self.from) {
                Some(Value::Null) | None => {
                    return Err(ConnectorError::Unresolved(format!("params.{}", self.from)))
                }
                Some(v) => template::value_text(v).into_bytes(),
            }
        };
        Ok(Encoded {
            content_type: self.content_type.to_string(),
            bytes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::{AuthSpec, Method, ParamSpec};
    use serde_json::json;
    use std::time::Duration;

    struct Zeros;
    impl Entropy for Zeros {
        fn fill(&self, out: &mut [u8]) {
            out.fill(0);
        }
    }

    fn spec(body: CallBody, params: Vec<(&str, ParamKind)>) -> CallSpec {
        CallSpec {
            connector: "c".into(),
            operation: "o".into(),
            base_url: "https://api.example.com".into(),
            hosts: vec!["api.example.com".into()],
            insecure_tls: false,
            auth: AuthSpec::None,
            method: Method::Post,
            path: "/x".into(),
            query: BTreeMap::new(),
            headers: BTreeMap::new(),
            body: Some(body),
            params: params
                .into_iter()
                .map(|(n, k)| ParamSpec {
                    name: n.into(),
                    kind: k,
                    required: false,
                })
                .collect(),
            select: None,
            expect: None,
            writes: true,
            idempotency: None,
            idempotency_key: None,
            page: None,
            timeout: Duration::from_secs(5),
        }
    }

    #[test]
    fn a_json_leaf_naming_an_absent_optional_parameter_is_left_out() {
        let spec = spec(
            CallBody::Json {
                value: json!({
                    "summary": "{params.summary}",
                    "description": "{params.description}",
                    "start": {"dateTime": "{params.start}"},
                    "filter": "{params.filter}",
                    "page_size": "{params.max}",
                    "tags": ["{params.tag}", "fixed"],
                    "note": "a {params.description}"
                }),
            },
            vec![
                ("summary", ParamKind::Text),
                ("description", ParamKind::Text),
                ("start", ParamKind::Text),
                ("filter", ParamKind::Json),
                ("max", ParamKind::Number),
                ("tag", ParamKind::Text),
            ],
        );
        let account = BTreeMap::new();
        let present = BTreeMap::from([
            ("summary".to_string(), json!("Standup")),
            ("description".to_string(), json!("daily")),
            ("start".to_string(), json!("2026-10-08T09:00:00Z")),
            ("filter".to_string(), json!({"a": 1})),
            ("max".to_string(), json!(10)),
            ("tag".to_string(), json!("x")),
        ]);
        let values = Values {
            account: &account,
            params: &present,
        };
        let out = encode(&Sources {
            spec: &spec,
            values: &values,
            files: &BTreeMap::new(),
            entropy: &Zeros,
        })
        .unwrap()
        .unwrap();
        let body: Value = serde_json::from_slice(&out.bytes).unwrap();
        assert_eq!(body["description"], json!("daily"));
        assert_eq!(body["filter"], json!({"a": 1}));
        assert_eq!(body["page_size"], json!(10));
        assert_eq!(body["tags"], json!(["x", "fixed"]));
        assert_eq!(body["note"], json!("a daily"));

        // The optional ones left empty: their members and items are gone, the
        // typed and text ones alike; a leaf mixing text with the absent
        // parameter is still unresolved — it cannot be half a sentence.
        let absent = BTreeMap::from([
            ("summary".to_string(), json!("Standup")),
            ("description".to_string(), Value::Null),
            ("start".to_string(), json!("2026-10-08T09:00:00Z")),
            ("filter".to_string(), Value::Null),
            ("max".to_string(), Value::Null),
            ("tag".to_string(), Value::Null),
        ]);
        let values = Values {
            account: &account,
            params: &absent,
        };
        let err = encode(&Sources {
            spec: &spec,
            values: &values,
            files: &BTreeMap::new(),
            entropy: &Zeros,
        })
        .unwrap_err();
        assert!(
            matches!(err, ConnectorError::Unresolved(ref k) if k == "params.description"),
            "{err}"
        );
        let mut without_note = spec.clone();
        if let Some(CallBody::Json { value }) = &mut without_note.body {
            value.as_object_mut().unwrap().remove("note");
        }
        let out = encode(&Sources {
            spec: &without_note,
            values: &values,
            files: &BTreeMap::new(),
            entropy: &Zeros,
        })
        .unwrap()
        .unwrap();
        let body: Value = serde_json::from_slice(&out.bytes).unwrap();
        assert_eq!(
            body,
            json!({"summary": "Standup", "start": {"dateTime": "2026-10-08T09:00:00Z"}, "tags": ["fixed"]})
        );
    }

    #[test]
    fn a_form_body_drops_absent_fields_and_encodes_the_rest() {
        let spec = spec(
            CallBody::Form {
                fields: BTreeMap::from([
                    ("grant_type".to_string(), "authorization_code".to_string()),
                    ("code".to_string(), "{params.code}".to_string()),
                    ("note".to_string(), "{params.note}".to_string()),
                ]),
            },
            vec![("code", ParamKind::Text), ("note", ParamKind::Text)],
        );
        let account = BTreeMap::new();
        let params = BTreeMap::from([
            ("code".to_string(), json!("a b&c")),
            ("note".to_string(), Value::Null),
        ]);
        let values = Values {
            account: &account,
            params: &params,
        };
        let out = encode(&Sources {
            spec: &spec,
            values: &values,
            files: &BTreeMap::new(),
            entropy: &Zeros,
        })
        .unwrap()
        .unwrap();
        assert_eq!(out.content_type, "application/x-www-form-urlencoded");
        assert_eq!(
            String::from_utf8(out.bytes).unwrap(),
            "code=a+b%26c&grant_type=authorization_code"
        );
    }

    #[test]
    fn a_multipart_body_frames_text_and_file_parts_under_one_boundary() {
        let spec = spec(
            CallBody::Multipart {
                parts: vec![
                    Part {
                        name: "meta".into(),
                        source: PartSource::Text {
                            text: "{params.meta}".into(),
                        },
                        filename: None,
                        content_type: Some("application/json".into()),
                    },
                    Part {
                        name: "media".into(),
                        source: PartSource::File {
                            file: "video".into(),
                        },
                        filename: None,
                        content_type: None,
                    },
                    Part {
                        name: "skip".into(),
                        source: PartSource::Text {
                            text: "{params.absent}".into(),
                        },
                        filename: None,
                        content_type: None,
                    },
                ],
            },
            vec![
                ("meta", ParamKind::Json),
                ("video", ParamKind::File),
                ("absent", ParamKind::Text),
            ],
        );
        let account = BTreeMap::new();
        let params = BTreeMap::from([
            ("meta".to_string(), json!({"title": "x"})),
            ("video".to_string(), json!("clips/a.mp4")),
            ("absent".to_string(), Value::Null),
        ]);
        let files = BTreeMap::from([(
            "video".to_string(),
            FileData {
                filename: "a \"quoted\".mp4".into(),
                content_type: Some("video/mp4".into()),
                bytes: b"\x00\x01binary".to_vec(),
            },
        )]);
        let values = Values {
            account: &account,
            params: &params,
        };
        let out = encode(&Sources {
            spec: &spec,
            values: &values,
            files: &files,
            entropy: &Zeros,
        })
        .unwrap()
        .unwrap();
        let boundary = "bisa-000000000000000000000000";
        assert_eq!(
            out.content_type,
            format!("multipart/form-data; boundary={boundary}")
        );
        let expected = format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"meta\"\r\nContent-Type: application/json\r\n\r\n{{\"title\":\"x\"}}\r\n\
             --{boundary}\r\nContent-Disposition: form-data; name=\"media\"; filename=\"a \\\"quoted\\\".mp4\"\r\nContent-Type: video/mp4\r\n\r\n\x00\x01binary\r\n\
             --{boundary}--\r\n"
        );
        assert_eq!(String::from_utf8_lossy(&out.bytes), expected);
    }

    #[test]
    fn a_raw_body_is_one_parameters_bytes_and_a_missing_file_is_named() {
        let raw = CallBody::Raw {
            content_type: "application/octet-stream".into(),
            from: "blob".into(),
        };
        let spec_file = spec(raw.clone(), vec![("blob", ParamKind::File)]);
        let account = BTreeMap::new();
        let params = BTreeMap::from([("blob".to_string(), json!("out/a.bin"))]);
        let values = Values {
            account: &account,
            params: &params,
        };
        let missing = encode(&Sources {
            spec: &spec_file,
            values: &values,
            files: &BTreeMap::new(),
            entropy: &Zeros,
        })
        .unwrap_err();
        assert!(matches!(missing, ConnectorError::Unresolved(k) if k == "params.blob"));
        let files = BTreeMap::from([(
            "blob".to_string(),
            FileData {
                filename: "a.bin".into(),
                content_type: None,
                bytes: vec![1, 2, 3],
            },
        )]);
        let out = encode(&Sources {
            spec: &spec_file,
            values: &values,
            files: &files,
            entropy: &Zeros,
        })
        .unwrap()
        .unwrap();
        assert_eq!(out.bytes, vec![1, 2, 3]);
        assert_eq!(out.content_type, "application/octet-stream");

        let spec_text = spec(
            CallBody::Raw {
                content_type: "text/xml".into(),
                from: "doc".into(),
            },
            vec![("doc", ParamKind::Text)],
        );
        let params = BTreeMap::from([("doc".to_string(), json!("<a/>"))]);
        let values = Values {
            account: &account,
            params: &params,
        };
        let out = encode(&Sources {
            spec: &spec_text,
            values: &values,
            files: &BTreeMap::new(),
            entropy: &Zeros,
        })
        .unwrap()
        .unwrap();
        assert_eq!(out.bytes, b"<a/>".to_vec());
    }
}
