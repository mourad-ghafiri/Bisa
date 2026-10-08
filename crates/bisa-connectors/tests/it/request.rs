//! What reaches the wire: parameters bound by kind, the URL's segments and
//! query encoded, the body's leaves rendered.

use crate::support::*;
use bisa_connectors::hosts::AllowAll;
use bisa_connectors::request::{bind_params, build_request};
use bisa_connectors::spec::{AuthSpec, CallBody, Method, ParamKind, ParamSpec, Part, PartSource};
use bisa_connectors::template::Values;
use bisa_connectors::{ConnectorError, FileData};
use serde_json::json;
use std::collections::BTreeMap;

/// No files and a counting boundary — what a request is built with here.
fn build(
    spec: &bisa_connectors::spec::CallSpec,
    values: &Values<'_>,
) -> Result<bisa_connectors::http::Request, ConnectorError> {
    build_request(
        spec,
        values,
        &BTreeMap::new(),
        CountingEntropy::from(1).as_ref(),
    )
}

fn p(name: &str, kind: ParamKind, required: bool) -> ParamSpec {
    ParamSpec {
        name: name.into(),
        kind,
        required,
    }
}

#[test]
fn params_bind_by_kind_and_refuse_unknown_and_missing() {
    let specs = vec![
        p("q", ParamKind::Text, true),
        p("n", ParamKind::Number, false),
        p("on", ParamKind::Bool, false),
        p("body", ParamKind::Json, false),
    ];
    let bound = bind_params(
        &specs,
        &params(&[
            ("q", "x"),
            ("n", "3"),
            ("on", "TRUE"),
            ("body", r#"{"a":1}"#),
        ]),
    )
    .unwrap();
    assert_eq!(bound["q"], json!("x"));
    assert_eq!(bound["n"], json!(3));
    assert_eq!(bound["on"], json!(true));
    assert_eq!(bound["body"], json!({"a": 1}));
    let bound = bind_params(&specs, &params(&[("q", "x"), ("n", "2.5")])).unwrap();
    assert_eq!(bound["n"], json!(2.5));
    assert_eq!(
        bound["on"],
        serde_json::Value::Null,
        "an absent optional is null"
    );

    let missing = bind_params(&specs, &params(&[("n", "1")])).unwrap_err();
    assert!(
        matches!(missing, ConnectorError::BadParam { name, why } if name == "q" && why == "required")
    );
    let unknown = bind_params(&specs, &params(&[("q", "x"), ("zzz", "1")])).unwrap_err();
    assert!(matches!(unknown, ConnectorError::BadParam { name, .. } if name == "zzz"));
    let bad_number = bind_params(&specs, &params(&[("q", "x"), ("n", "three")])).unwrap_err();
    assert!(matches!(bad_number, ConnectorError::BadParam { name, .. } if name == "n"));
    let bad_bool = bind_params(&specs, &params(&[("q", "x"), ("on", "yes")])).unwrap_err();
    assert!(matches!(bad_bool, ConnectorError::BadParam { name, .. } if name == "on"));
    let bad_json = bind_params(&specs, &params(&[("q", "x"), ("body", "{nope")])).unwrap_err();
    assert!(matches!(bad_json, ConnectorError::BadParam { name, .. } if name == "body"));
}

#[test]
fn account_params_render_before_step_params_and_values_are_never_rescanned() {
    let mut spec = offline_spec(AuthSpec::None);
    spec.base_url = "https://{account.site}.example.com".into();
    spec.hosts = vec!["*.example.com".into()];
    spec.path = "/rest/{params.id}".into();
    spec.params = vec![p("id", ParamKind::Text, true)];
    let account = BTreeMap::from([("site".to_string(), json!("acme"))]);
    // A step value that looks like a placeholder is text, never read again.
    let bound = bind_params(&spec.params, &params(&[("id", "{account.site}")])).unwrap();
    let values = Values {
        account: &account,
        params: &bound,
    };
    let req = build(&spec, &values).unwrap();
    assert_eq!(
        req.url.as_str(),
        "https://acme.example.com/rest/%7Baccount.site%7D"
    );
}

#[test]
fn path_values_are_percent_encoded_and_query_values_form_encoded() {
    let mut spec = offline_spec(AuthSpec::None);
    spec.path = "/v1/items/{params.id}".into();
    spec.query = BTreeMap::from([
        ("q".to_string(), "{params.q}".to_string()),
        ("max".to_string(), "{params.max}".to_string()),
    ]);
    spec.params = vec![
        p("id", ParamKind::Text, true),
        p("q", ParamKind::Text, true),
        p("max", ParamKind::Number, false),
    ];
    let bound = bind_params(&spec.params, &params(&[("id", "a/b c"), ("q", "x=1&y")])).unwrap();
    let values = Values {
        account: &no_account_params(),
        params: &bound,
    };
    let req = build(&spec, &values).unwrap();
    assert_eq!(req.url.path(), "/v1/items/a%2Fb%20c");
    assert_eq!(
        req.url.query(),
        Some("q=x%3D1%26y"),
        "the absent optional pair is dropped"
    );
    assert_eq!(req.header("accept"), Some("application/json"));
}

#[test]
fn a_body_leaf_that_is_one_typed_placeholder_becomes_the_typed_value() {
    let mut spec = offline_spec(AuthSpec::None);
    spec.method = Method::Post;
    spec.body = Some(CallBody::Json {
        value: json!({
            "title": "Issue: {params.title}",
            "count": "{params.count}",
            "flags": "{params.flags}",
            "note": "{params.count} items",
            "nested": {"on": "{params.on}"}
        }),
    });
    spec.params = vec![
        p("title", ParamKind::Text, true),
        p("count", ParamKind::Number, true),
        p("flags", ParamKind::Json, true),
        p("on", ParamKind::Bool, true),
    ];
    let bound = bind_params(
        &spec.params,
        &params(&[
            ("title", "x"),
            ("count", "3"),
            ("flags", "[1,2]"),
            ("on", "false"),
        ]),
    )
    .unwrap();
    let values = Values {
        account: &no_account_params(),
        params: &bound,
    };
    let req = build(&spec, &values).unwrap();
    let body: serde_json::Value = serde_json::from_slice(req.body.as_deref().unwrap()).unwrap();
    assert_eq!(
        body,
        json!({"title": "Issue: x", "count": 3, "flags": [1, 2], "note": "3 items", "nested": {"on": false}})
    );
    assert_eq!(req.header("content-type"), Some("application/json"));
}

#[test]
fn reserved_headers_are_refused() {
    for name in ["Authorization", "host", "Content-Length", "cookie"] {
        let mut spec = offline_spec(AuthSpec::None);
        spec.headers = BTreeMap::from([(name.to_string(), "x".to_string())]);
        let values = Values {
            account: &no_account_params(),
            params: &BTreeMap::new(),
        };
        assert!(
            matches!(build(&spec, &values), Err(ConnectorError::BadDefinition(_))),
            "{name} is reserved"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_call_reaches_the_stub_with_the_rendered_path() {
    let stub = Stub::start(vec![Canned::json(
        "GET",
        "/v1/items/42",
        200,
        json!({"id": 42, "name": "x"}),
    )])
    .await;
    let client = client(MemoryCreds::with(&account(), Default::default()));
    let mut spec = spec(&stub, AuthSpec::None);
    spec.select = Some("name".into());
    let out = client
        .call(
            &spec,
            None,
            &no_account_params(),
            &params(&[("id", "42")]),
            &AllowAll,
        )
        .await
        .unwrap();
    assert_eq!(out.status, 200);
    assert_eq!(out.selected, json!("x"));
    assert_eq!(out.body["id"], json!(42));
    let calls = stub.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].path, "/v1/items/42");
    assert_eq!(calls[0].header("user-agent").as_deref(), Some("bisa"));
    assert!(calls[0].authorization.is_none());
}

/// An upload: a `file` parameter read through the files port, framed as a
/// multipart part beside a text part, under a boundary the client owns.
#[tokio::test(flavor = "multi_thread")]
async fn a_file_parameter_travels_as_a_multipart_part_read_through_the_files_port() {
    let stub = Stub::start(vec![Canned::json(
        "POST",
        "/v1/upload",
        200,
        json!({"id": "u1"}),
    )])
    .await;
    let client = client(MemoryCreds::with(&account(), Default::default()));
    let mut spec = spec(&stub, AuthSpec::None);
    spec.method = Method::Post;
    spec.path = "/v1/upload".into();
    spec.headers = BTreeMap::from([("content-type".to_string(), "text/plain".to_string())]);
    spec.body = Some(CallBody::Multipart {
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
        ],
    });
    spec.params = vec![
        p("meta", ParamKind::Json, true),
        p("video", ParamKind::File, true),
    ];
    let files = MemoryFiles::with(
        "out/clip.mp4",
        FileData {
            filename: "clip.mp4".into(),
            content_type: Some("video/mp4".into()),
            bytes: b"MOVIE".to_vec(),
        },
    );
    let out = client
        .call_with(
            &spec,
            None,
            &no_account_params(),
            &params(&[("meta", r#"{"title":"x"}"#), ("video", "out/clip.mp4")]),
            &files,
            &AllowAll,
        )
        .await
        .unwrap();
    assert_eq!(out.selected, json!({"id": "u1"}));
    let call = &stub.calls()[0];
    let content_type = call.header("content-type").unwrap();
    assert!(
        content_type.starts_with("multipart/form-data; boundary=bisa-"),
        "the client's boundary, not the definition's text/plain: {content_type}"
    );
    let boundary = content_type.split("boundary=").nth(1).unwrap();
    assert!(call.text.contains(&format!("--{boundary}\r\nContent-Disposition: form-data; name=\"meta\"\r\nContent-Type: application/json\r\n\r\n{{\"title\":\"x\"}}\r\n")), "{}", call.text);
    assert!(
        call.text.contains(
            "name=\"media\"; filename=\"clip.mp4\"\r\nContent-Type: video/mp4\r\n\r\nMOVIE\r\n"
        ),
        "{}",
        call.text
    );
    assert!(call.text.ends_with(&format!("--{boundary}--\r\n")));

    // A path the port refuses is the parameter's fault, before any request.
    let refused = MemoryFiles::default();
    let err = client
        .call_with(
            &spec,
            None,
            &no_account_params(),
            &params(&[("meta", "{}"), ("video", "../secret")]),
            &refused,
            &AllowAll,
        )
        .await
        .unwrap_err();
    assert!(
        matches!(&err, ConnectorError::BadParam { name, why } if name == "video" && why.contains("no such file")),
        "{err}"
    );
    // A call with no checkout — a poll, a check — refuses every file by name.
    let err = client
        .call(
            &spec,
            None,
            &no_account_params(),
            &params(&[("meta", "{}"), ("video", "out/clip.mp4")]),
            &AllowAll,
        )
        .await
        .unwrap_err();
    assert!(
        matches!(&err, ConnectorError::BadParam { name, why } if name == "video" && why.contains("no checkout")),
        "{err}"
    );
    assert_eq!(
        stub.calls().len(),
        1,
        "nothing was sent for the refused calls"
    );
}

/// A form body reaches the wire form-encoded; a raw body as the parameter's
/// own bytes under the declared type.
#[tokio::test(flavor = "multi_thread")]
async fn form_and_raw_bodies_reach_the_stub_under_their_content_types() {
    let stub = Stub::start(vec![
        Canned::json("POST", "/v1/token", 200, json!({"ok": 1})),
        Canned::json("PUT", "/v1/doc", 200, json!({"ok": 2})),
    ])
    .await;
    let client = client(MemoryCreds::with(&account(), Default::default()));
    let mut form = spec(&stub, AuthSpec::None);
    form.method = Method::Post;
    form.path = "/v1/token".into();
    form.body = Some(CallBody::Form {
        fields: BTreeMap::from([
            ("grant_type".to_string(), "refresh".to_string()),
            ("code".to_string(), "{params.code}".to_string()),
        ]),
    });
    form.params = vec![p("code", ParamKind::Text, true)];
    client
        .call(
            &form,
            None,
            &no_account_params(),
            &params(&[("code", "a b")]),
            &AllowAll,
        )
        .await
        .unwrap();
    let mut raw = spec(&stub, AuthSpec::None);
    raw.method = Method::Put;
    raw.path = "/v1/doc".into();
    raw.body = Some(CallBody::Raw {
        content_type: "application/xml".into(),
        from: "doc".into(),
    });
    raw.params = vec![p("doc", ParamKind::Text, true)];
    client
        .call(
            &raw,
            None,
            &no_account_params(),
            &params(&[("doc", "<a/>")]),
            &AllowAll,
        )
        .await
        .unwrap();
    let calls = stub.calls();
    assert_eq!(
        calls[0].header("content-type").as_deref(),
        Some("application/x-www-form-urlencoded")
    );
    assert_eq!(
        calls[0].form(),
        BTreeMap::from([
            ("grant_type".to_string(), "refresh".to_string()),
            ("code".to_string(), "a b".to_string()),
        ])
    );
    assert_eq!(
        calls[1].header("content-type").as_deref(),
        Some("application/xml")
    );
    assert_eq!(calls[1].text, "<a/>");
}

#[test]
fn a_path_parameter_keeps_its_slashes_where_a_text_one_is_one_segment() {
    let mut spec = offline_spec(AuthSpec::None);
    spec.path = "/vault/{params.note}/{params.folder}/".into();
    spec.params = vec![
        p("note", ParamKind::Path, true),
        p("folder", ParamKind::Text, true),
    ];
    let account = BTreeMap::new();
    let bound = bind_params(
        &spec.params,
        &params(&[("note", "Projects/Launch plan.md"), ("folder", "a/b")]),
    )
    .unwrap();
    let values = Values {
        account: &account,
        params: &bound,
    };
    let req = build(&spec, &values).unwrap();
    assert_eq!(
        req.url.path(),
        "/vault/Projects/Launch%20plan.md/a%2Fb/",
        "the path kind's slashes stay; the text kind is one segment"
    );
    // A path that climbs is refused whichever kind carries it.
    let bound = bind_params(
        &spec.params,
        &params(&[("note", "../secrets.md"), ("folder", "x")]),
    )
    .unwrap();
    let values = Values {
        account: &account,
        params: &bound,
    };
    assert!(matches!(
        build(&spec, &values),
        Err(ConnectorError::BadDefinition(_))
    ));
}
