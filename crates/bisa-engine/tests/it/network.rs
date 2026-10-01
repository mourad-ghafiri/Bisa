//! The engine's side of the `network.*` settings: every key is machine scope,
//! a write swaps the clients' policy and the environment a child is handed,
//! `none` removes the proxy names a session would inherit, a bad URL is
//! refused at write naming the key, and the loopback client never carries a
//! proxy. Nothing here sends a request.

use bisa_core::network_settings::keys;
use bisa_core::{SettingDef, SettingScope};
use bisa_engine::network::{self, InForce};
use bisa_engine::{Engine, EngineConfig, EngineError};
use bisa_harness::HarnessCatalog;
use bisa_http::ProxyPolicy;
use bisa_store::{MemoryKeyStore, Workspace};
use serde_json::json;

fn engine(dir: &tempfile::TempDir) -> Engine {
    let ws =
        Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
    Engine::start(
        ws,
        HarnessCatalog::new(),
        EngineConfig {
            design_enabled: false,
            events_enabled: false,
            ..Default::default()
        },
    )
    .unwrap()
}

#[test]
fn every_network_key_is_machine_scope_and_the_defaults_are_the_environments() {
    for key in [
        keys::PROXY_MODE,
        keys::PROXY_HTTP,
        keys::PROXY_HTTPS,
        keys::PROXY_NO_PROXY,
        keys::HTTP1_ONLY,
        keys::PUBLIC_IP_URL,
    ] {
        let def = SettingDef::lookup(key).expect(key);
        assert!(
            def.scopes.allows(SettingScope::Machine),
            "{key} is this machine's"
        );
        assert!(
            !def.scopes.allows(SettingScope::Workspace),
            "{key} never syncs"
        );
        assert!(
            !def.scopes.allows(SettingScope::Project),
            "{key} is no project's"
        );
    }
    let defaults = bisa_core::resolve_settings(&[]);
    let settings = bisa_core::NetworkSettings::from_resolved(&defaults);
    let (policy, problems) = network::policy(&settings);
    assert!(problems.is_empty());
    assert_eq!(policy.proxy, ProxyPolicy::Environment);
    assert!(!policy.http1_only);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_write_swaps_the_policy_and_the_environment_a_child_is_handed() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let inner = engine.inner();
    assert_eq!(inner.http.policy().proxy, ProxyPolicy::Environment);
    assert!(network::child_env(inner).set.is_empty());

    engine
        .set_setting(
            SettingScope::Machine,
            None,
            keys::PROXY_MODE,
            json!("manual"),
        )
        .unwrap();
    engine
        .set_setting(
            SettingScope::Machine,
            None,
            keys::PROXY_HTTPS,
            json!("http://ada:s3cret@proxy.example:3128"),
        )
        .unwrap();
    engine
        .set_setting(
            SettingScope::Machine,
            None,
            keys::PROXY_NO_PROXY,
            json!(".corp.example"),
        )
        .unwrap();
    engine
        .set_setting(SettingScope::Machine, None, keys::HTTP1_ONLY, json!(true))
        .unwrap();

    let policy = inner.http.policy();
    assert!(policy.names_a_proxy());
    assert!(policy.http1_only);
    let child = network::child_env(inner);
    assert!(child.set["HTTPS_PROXY"].starts_with("http://ada:s3cret@proxy.example:3128"));
    assert_eq!(child.set["https_proxy"], child.set["HTTPS_PROXY"]);
    assert_eq!(
        child.set["NO_PROXY"],
        "localhost,127.0.0.1,::1,.corp.example"
    );
    assert!(
        child.remove.contains(&"HTTP_PROXY"),
        "the pair left unnamed is removed"
    );
    let (env, remove) = network::session_env(inner, Default::default());
    assert_eq!(env["HTTPS_PROXY"], child.set["HTTPS_PROXY"]);
    assert!(remove.contains(&"ALL_PROXY".to_string()));

    // The status masks the login and says what is in force.
    let status = engine.network_status();
    assert_eq!(status.mode, bisa_core::ProxyMode::Manual);
    assert!(!status.https.as_deref().unwrap().contains("s3cret"));
    assert!(status.https.as_deref().unwrap().contains("ada:"));
    assert!(status.problems.is_empty(), "{:?}", status.problems);
    match status.in_force {
        InForce::Proxy { https, .. } => assert!(!https.unwrap().contains("s3cret")),
        other => panic!("{other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn none_removes_the_proxy_names_a_session_would_inherit_and_environment_hands_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let inner = engine.inner();
    engine
        .set_setting(SettingScope::Machine, None, keys::PROXY_MODE, json!("none"))
        .unwrap();
    let (env, remove) = network::session_env(inner, Default::default());
    assert!(env.is_empty());
    assert_eq!(
        remove,
        bisa_http::PROXY_ENV_NAMES.map(str::to_string).to_vec()
    );
    assert_eq!(engine.network_status().in_force, InForce::Direct);

    engine
        .set_setting(
            SettingScope::Machine,
            None,
            keys::PROXY_MODE,
            json!("environment"),
        )
        .unwrap();
    let (env, remove) = network::session_env(inner, Default::default());
    assert!(env.is_empty() && remove.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_bad_url_and_a_bypass_with_a_port_are_refused_at_write_naming_the_key() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    for (key, value) in [
        (keys::PROXY_HTTP, json!("proxy.example:3128")),
        (keys::PROXY_HTTPS, json!("socks5://proxy.example:1080")),
        (keys::PROXY_NO_PROXY, json!("a.example:8080")),
        (keys::PUBLIC_IP_URL, json!("http://api.ipify.org")),
        (keys::PUBLIC_IP_URL, json!("https://localhost")),
    ] {
        let err = engine
            .set_setting(SettingScope::Machine, None, key, value)
            .unwrap_err();
        assert!(err.to_string().contains(key), "{err}");
    }
    // Nothing landed: the policy is still the default.
    assert_eq!(engine.inner().http.policy().proxy, ProxyPolicy::Environment);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_check_refuses_this_machine_and_a_scheme_that_is_not_http() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    for url in [
        "http://127.0.0.1:1",
        "http://localhost",
        "ftp://example.com",
        "nope",
    ] {
        assert!(
            matches!(
                engine.network_check(url).await,
                Err(EngineError::Invalid(_))
            ),
            "{url}"
        );
    }
}
