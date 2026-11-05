//! Every answer the node gives wears the same three headers unless the route
//! set its own: `X-Content-Type-Options: nosniff` (a body is what its type
//! says), `Referrer-Policy: no-referrer` (nothing of a URL leaves with a
//! follow-up request) and `Cache-Control: no-store` (nothing is kept) — on a
//! 200, on a 404, on the token gate's 401, on the token-less probe, and on the
//! event stream, whose own `no-cache` stands because the layer defers to a
//! value a route already set.

use crate::node::Node;
use http_body_util::BodyExt as _;
use hyper::body::Bytes;
use hyper::Request;
use hyper_util::rt::TokioIo;
use tokio::net::UnixStream;

/// One request with the token or without, answering the status and the
/// response headers; the body is left unread so a stream can be asked.
async fn head_of(
    socket: &std::path::Path,
    path: &str,
    token: Option<&str>,
) -> (u16, hyper::HeaderMap, hyper::body::Incoming) {
    let stream = UnixStream::connect(socket).await.expect("connect");
    let (mut sender, conn) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
        .await
        .expect("handshake");
    tokio::spawn(conn);
    let mut builder = Request::builder()
        .method("GET")
        .uri(path)
        .header(hyper::header::HOST, "localhost");
    if let Some(token) = token {
        builder = builder.header(hyper::header::AUTHORIZATION, format!("Bearer {token}"));
    }
    let request = builder
        .body(http_body_util::Full::new(Bytes::new()))
        .unwrap();
    let resp = sender.send_request(request).await.expect("request");
    let status = resp.status().as_u16();
    let headers = resp.headers().clone();
    (status, headers, resp.into_body())
}

fn header<'a>(headers: &'a hyper::HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name).and_then(|v| v.to_str().ok())
}

#[tokio::test(flavor = "multi_thread")]
async fn every_answer_wears_the_security_headers() {
    let node = Node::start().await;
    let token = Node::token();
    // A 200, a route's own 404, the gate's 401, and the token-less probe.
    for (path, with_token, expected) in [
        ("/goals", true, 200),
        ("/goals/01J0000000000000000000000A", true, 404),
        ("/goals", false, 401),
        ("/health", false, 200),
    ] {
        let (status, headers, body) =
            head_of(node.socket(), path, with_token.then_some(token)).await;
        let _collected = body.collect().await;
        assert_eq!(status, expected, "{path}");
        assert_eq!(
            header(&headers, "x-content-type-options"),
            Some("nosniff"),
            "{path}"
        );
        assert_eq!(
            header(&headers, "referrer-policy"),
            Some("no-referrer"),
            "{path}"
        );
        assert_eq!(
            header(&headers, "cache-control"),
            Some("no-store"),
            "{path}"
        );
    }
    // The event stream keeps its own `no-cache` and still streams a frame.
    let (status, headers, mut body) =
        head_of(node.socket(), &format!("/events?token={token}"), None).await;
    assert_eq!(status, 200);
    assert_eq!(header(&headers, "x-content-type-options"), Some("nosniff"));
    assert_eq!(header(&headers, "referrer-policy"), Some("no-referrer"));
    assert_eq!(header(&headers, "cache-control"), Some("no-cache"));
    // Something to stream: a goal created is a frame on the stream.
    let (status, made) = node
        .req(
            "POST",
            "/goals",
            Some(serde_json::json!({"statement": "say something on the stream"})),
        )
        .await;
    assert_eq!(status, 200, "{made}");
    let first = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            match body.frame().await {
                Some(Ok(frame)) => {
                    if let Some(data) = frame.data_ref() {
                        if !data.is_empty() {
                            break String::from_utf8_lossy(data).into_owned();
                        }
                    }
                }
                _ => break String::new(),
            }
        }
    })
    .await
    .expect("a first frame within ten seconds");
    assert!(!first.is_empty(), "the stream said nothing");
    node.shutdown().await;
}
