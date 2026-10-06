//! The latest release, read from a stub standing in for GitHub: a published
//! release is read once and held for the TTL, a refresh asks again, no
//! release is an answer and is held, a rate limit is said with its wait and
//! asked again, an unreachable source is said as such, a zero TTL asks every
//! time, and an engine nobody configured asks nobody.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::any;
use axum::Router;
use bisa_core::SettingScope;
use bisa_engine::updates::{UpdateCheck, UpdateFailure, UpdatesSource};
use bisa_engine::{Engine, EngineConfig};
use bisa_harness::HarnessCatalog;
use bisa_store::{MemoryKeyStore, Workspace};
use serde_json::json;

/// What the stub answers: a status, its headers and a JSON body.
#[derive(Clone)]
struct Answer {
    status: u16,
    headers: Vec<(&'static str, String)>,
    body: serde_json::Value,
}

#[derive(Clone)]
struct Shared {
    answer: Arc<Mutex<Answer>>,
    hits: Arc<AtomicU32>,
}

struct Stub {
    base_url: String,
    shared: Shared,
}

impl Stub {
    async fn start(answer: Answer) -> Self {
        let shared = Shared {
            answer: Arc::new(Mutex::new(answer)),
            hits: Arc::new(AtomicU32::new(0)),
        };
        let app = Router::new()
            .fallback(any(handle))
            .with_state(shared.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _served = axum::serve(listener, app).await;
        });
        Self {
            base_url: format!("http://{addr}"),
            shared,
        }
    }

    fn source(&self) -> UpdatesSource {
        UpdatesSource::at(format!("{}/repos/o/r/releases/latest", self.base_url))
    }

    fn hits(&self) -> u32 {
        self.shared.hits.load(Ordering::SeqCst)
    }

    fn answer(&self, answer: Answer) {
        *self.shared.answer.lock().unwrap() = answer;
    }
}

async fn handle(State(shared): State<Shared>) -> Response {
    shared.hits.fetch_add(1, Ordering::SeqCst);
    let answer = shared.answer.lock().unwrap().clone();
    let mut headers = HeaderMap::new();
    for (name, value) in &answer.headers {
        headers.insert(*name, HeaderValue::from_str(value).unwrap());
    }
    (
        StatusCode::from_u16(answer.status).unwrap(),
        headers,
        axum::Json(answer.body),
    )
        .into_response()
}

fn release() -> Answer {
    Answer {
        status: 200,
        headers: vec![],
        body: json!({
            "tag_name": "v0.3.0",
            "name": "Bisa 0.3.0",
            "html_url": "https://github.com/mourad-ghafiri/Bisa/releases/tag/v0.3.0",
            "body": "### Added\n\n- The Update dialog.\n\n---\n\nBuilt from commit `abc`.\n",
            "published_at": "2026-10-10T09:00:00Z",
            "prerelease": false,
            "draft": false,
            "assets": [
                {"name": "Bisa-0.3.0-macos-universal.dmg", "browser_download_url": "https://github.com/mourad-ghafiri/Bisa/releases/download/v0.3.0/Bisa-0.3.0-macos-universal.dmg", "size": 124_000_000},
                {"name": "Bisa-0.3.0-macos-universal.dmg.sha256", "browser_download_url": "https://github.com/mourad-ghafiri/Bisa/releases/download/v0.3.0/Bisa-0.3.0-macos-universal.dmg.sha256", "size": 100}
            ]
        }),
    }
}

fn engine_with(dir: &tempfile::TempDir, updates: Option<UpdatesSource>) -> Engine {
    let ws =
        Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
    Engine::start(
        ws,
        HarnessCatalog::new(),
        EngineConfig {
            design_enabled: false,
            events_enabled: false,
            updates,
            ..Default::default()
        },
    )
    .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_published_release_is_read_once_held_for_the_ttl_and_asked_again_on_refresh() {
    let stub = Stub::start(release()).await;
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, Some(stub.source()));

    let first = engine.check_update(false).await;
    let UpdateCheck::Latest {
        release: got,
        checked_at,
    } = &first
    else {
        panic!("{first:?}");
    };
    assert_eq!(got.tag, "v0.3.0");
    assert_eq!(got.version, "0.3.0", "the version is the tag without its v");
    assert_eq!(got.name.as_deref(), Some("Bisa 0.3.0"));
    assert_eq!(
        got.url,
        "https://github.com/mourad-ghafiri/Bisa/releases/tag/v0.3.0"
    );
    assert_eq!(got.published_at, Some(1_791_622_800));
    assert!(got.notes.as_deref().unwrap().starts_with("### Added"));
    assert!(!got.prerelease);
    assert_eq!(got.assets.len(), 2);
    assert_eq!(got.assets[0].name, "Bisa-0.3.0-macos-universal.dmg");
    assert_eq!(got.assets[0].size, 124_000_000);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    assert!(now - checked_at < 60, "checked just now");
    assert_eq!(stub.hits(), 1);

    // Held: a second ask within the hour costs GitHub nothing.
    assert_eq!(engine.check_update(false).await, first);
    assert_eq!(stub.hits(), 1, "the held answer was given");

    // A refresh asks again — and reads what changed.
    let mut newer = release();
    newer.body["tag_name"] = json!("v0.4.0");
    stub.answer(newer);
    let again = engine.check_update(true).await;
    assert_eq!(stub.hits(), 2);
    match again {
        UpdateCheck::Latest { release, .. } => assert_eq!(release.version, "0.4.0"),
        other => panic!("{other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn no_release_is_an_answer_and_is_held() {
    let stub = Stub::start(Answer {
        status: 404,
        headers: vec![],
        body: json!({"message": "Not Found"}),
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, Some(stub.source()));
    assert!(matches!(
        engine.check_update(false).await,
        UpdateCheck::NoRelease { .. }
    ));
    assert!(matches!(
        engine.check_update(false).await,
        UpdateCheck::NoRelease { .. }
    ));
    assert_eq!(stub.hits(), 1, "an absence is held like a presence");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_rate_limit_is_said_with_its_wait_and_asked_again_next_time() {
    let reset = (SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 90)
        .to_string();
    let stub = Stub::start(Answer {
        status: 403,
        headers: vec![
            ("x-ratelimit-remaining", "0".into()),
            ("x-ratelimit-reset", reset),
        ],
        body: json!({"message": "API rate limit exceeded"}),
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, Some(stub.source()));
    match engine.check_update(false).await {
        UpdateCheck::Failed {
            failure: UpdateFailure::RateLimited { retry_in_secs },
            ..
        } => {
            let wait = retry_in_secs.expect("a wait from the reset header");
            assert!((60..=90).contains(&wait), "{wait}");
        }
        other => panic!("{other:?}"),
    }
    // Not held: the next ask tries GitHub again.
    let _ = engine.check_update(false).await;
    assert_eq!(stub.hits(), 2);

    // A plain refusal is unexpected, with its status — and not held either.
    stub.answer(Answer {
        status: 500,
        headers: vec![],
        body: json!({"message": "boom"}),
    });
    assert!(matches!(
        engine.check_update(false).await,
        UpdateCheck::Failed {
            failure: UpdateFailure::Unexpected { status: 500 },
            ..
        }
    ));
    assert_eq!(stub.hits(), 3);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unreachable_source_is_said_as_such() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(
        &dir,
        Some(UpdatesSource::at(
            "http://127.0.0.1:1/repos/o/r/releases/latest",
        )),
    );
    match engine.check_update(false).await {
        UpdateCheck::Failed {
            failure: UpdateFailure::Unreachable { reason },
            ..
        } => assert!(!reason.is_empty()),
        other => panic!("{other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn an_engine_nobody_configured_asks_nobody() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, None);
    assert_eq!(engine.check_update(false).await, UpdateCheck::Off);
    assert_eq!(engine.check_update(true).await, UpdateCheck::Off);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_zero_ttl_asks_every_time() {
    let stub = Stub::start(release()).await;
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, Some(stub.source()));
    engine
        .set_setting(
            SettingScope::Machine,
            None,
            "cache.updates.ttl_ms",
            json!(0),
        )
        .unwrap();
    let _ = engine.check_update(false).await;
    let _ = engine.check_update(false).await;
    assert_eq!(stub.hits(), 2, "nothing is held when the TTL is zero");
}
