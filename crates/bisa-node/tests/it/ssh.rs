//! SSH for git hosts over HTTP (`ssh.rs`, ide/04), against a scripted
//! `FakeSsh` over a temp directory holding one synthetic public key: the
//! overview lists the pairs there and what ssh-agent holds, a new pair is
//! generated and lands as files, a key is loaded, `ssh -G` is read for a host,
//! a greeting is one handshake — and a key name that is not one is refused
//! before anything would run. Nothing reads `~/.ssh`, no program is spawned,
//! and an engine started without SSH answers 503.

use crate::node::Node;
use bisa_engine::EngineConfig;
use bisa_ssh::{fingerprint_sha256, FakeSsh, Program, Ssh, SshRunner};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The wire form of an ed25519 public key nobody holds (its 32 bytes count
/// upwards), and the `.pub` line `ssh-keygen` would write for it.
fn blob() -> Vec<u8> {
    let mut blob = Vec::new();
    blob.extend_from_slice(&11u32.to_be_bytes());
    blob.extend_from_slice(b"ssh-ed25519");
    blob.extend_from_slice(&32u32.to_be_bytes());
    blob.extend(1u8..=32);
    blob
}

fn pub_line(comment: &str) -> String {
    format!(
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8g {comment}"
    )
}

fn agent_holding(fingerprint: &str) -> String {
    format!("256 {fingerprint} ada@acme (ED25519)\n")
}

struct Rig {
    node: Node,
    fake: Arc<FakeSsh>,
    keys: PathBuf,
    _home: tempfile::TempDir,
}

/// A node whose SSH is the fake `script` builds over `keys/` — one pair,
/// `id_ed25519_acme`, already there — with the temp directory as `~`.
async fn rig(script: impl FnOnce(&Path) -> FakeSsh) -> Rig {
    let home = tempfile::tempdir().unwrap();
    let keys = home.path().join("ssh");
    std::fs::create_dir_all(&keys).unwrap();
    std::fs::write(
        keys.join("id_ed25519_acme.pub"),
        format!("{}\n", pub_line("ada@acme")),
    )
    .unwrap();
    std::fs::write(keys.join("id_ed25519_acme"), "not a key anybody holds\n").unwrap();
    let fake = Arc::new(script(&keys));
    let runner: Arc<dyn SshRunner> = fake.clone();
    let node = Node::start_with(EngineConfig {
        design_enabled: false,
        events_enabled: false,
        ssh: Some(Ssh::new(runner, &keys, home.path())),
        ..Default::default()
    })
    .await;
    Rig {
        node,
        fake,
        keys,
        _home: home,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_overview_lists_the_pairs_on_disk_and_what_the_agent_holds() {
    let fp = fingerprint_sha256(&blob());
    let rig =
        rig(|_| FakeSsh::new().answer(Program::SshAdd, &["-l"], 0, &agent_holding(&fp), "")).await;
    let v = rig.node.get("/git/ssh").await;
    assert_eq!(v["dir"], json!(rig.keys.display().to_string()), "{v}");
    let keys = v["keys"].as_array().unwrap();
    assert_eq!(keys.len(), 1, "{v}");
    assert_eq!(keys[0]["name"], json!("id_ed25519_acme"));
    assert_eq!(keys[0]["algorithm"], json!("ssh-ed25519"));
    assert_eq!(keys[0]["comment"], json!("ada@acme"));
    assert_eq!(keys[0]["fingerprint"], json!(fp));
    assert_eq!(keys[0]["loaded"], json!(true), "the agent holds it: {v}");
    assert_eq!(keys[0]["public_line"], json!(pub_line("ada@acme")));
    assert_eq!(v["agent"]["available"], json!(true), "{v}");
    assert_eq!(
        rig.fake
            .calls()
            .iter()
            .map(|c| c.line())
            .collect::<Vec<_>>(),
        vec!["ssh-add -l"],
        "one program asked, nothing else spawned"
    );
    rig.node.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_pair_is_generated_as_files_a_taken_name_is_refused_before_any_spawn_and_a_key_is_loaded()
{
    let rig = rig(|keys| {
        FakeSsh::new()
            .answer(Program::SshKeygen, &["-t", "ed25519"], 0, "", "")
            .writing(
                keys.join("id_ed25519_widgets.pub"),
                &format!("{}\n", pub_line("ada@widgets")),
            )
            .writing(keys.join("id_ed25519_widgets"), "the fake's private half\n")
            .answer(
                Program::SshAdd,
                &[&keys.join("id_ed25519_widgets").display().to_string()],
                0,
                "Identity added\n",
                "",
            )
    })
    .await;
    let (status, made) = rig
        .node
        .req(
            "POST",
            "/git/ssh/keys",
            Some(json!({"name": "id_ed25519_widgets", "comment": "ada@widgets"})),
        )
        .await;
    assert_eq!(status, 201, "{made}");
    assert_eq!(made["name"], json!("id_ed25519_widgets"));
    assert_eq!(made["public_line"], json!(pub_line("ada@widgets")));
    assert!(rig.keys.join("id_ed25519_widgets.pub").exists());

    // The name is taken now: refused in words, and ssh-keygen is not asked.
    let before = rig.fake.calls().len();
    let (status, v) = rig
        .node
        .req(
            "POST",
            "/git/ssh/keys",
            Some(json!({"name": "id_ed25519_widgets"})),
        )
        .await;
    assert_eq!(status, 400, "{v}");
    assert_eq!(
        rig.fake.calls().len(),
        before,
        "nothing spawned for a refusal"
    );

    // Loaded into the agent: 204, and the argv names the private file.
    let (status, v) = rig
        .node
        .req("POST", "/git/ssh/keys/id_ed25519_widgets/load", None)
        .await;
    assert_eq!(status, 204, "{v}");
    let last = rig.fake.calls().pop().unwrap();
    assert_eq!(last.program, Program::SshAdd);
    assert!(
        last.line()
            .contains(&rig.keys.join("id_ed25519_widgets").display().to_string()),
        "{}",
        last.line()
    );
    rig.node.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_key_name_that_is_not_one_is_refused_before_anything_runs() {
    let rig = rig(|_| FakeSsh::new()).await;
    for name in [
        "..%2Fetc%2Fpasswd",
        "no-such-key",
        "id_ed25519_acme%2F..%2Fx",
    ] {
        let (status, v) = rig
            .node
            .req("POST", &format!("/git/ssh/keys/{name}/load"), None)
            .await;
        assert_eq!(status, 400, "{name}: {v}");
        assert!(v["error"].is_string(), "{v}");
    }
    assert!(
        rig.fake.calls().is_empty(),
        "nothing spawned for a name that is not one"
    );
    rig.node.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn ssh_g_is_read_for_a_host_and_a_greeting_is_one_handshake_in_batch_mode() {
    let rig = rig(|_| {
        FakeSsh::new()
            .answer(
                Program::Ssh,
                &["-G", "github-acme"],
                0,
                "user git\nhostname github.com\nport 22\nidentitiesonly yes\nidentityfile ~/ssh/id_ed25519_acme\n",
                "",
            )
            .answer(
                Program::Ssh,
                &["-T", "git@github.com"],
                1,
                "",
                "Hi ada-acme! You've successfully authenticated, but GitHub does not provide shell access.\n",
            )
    })
    .await;
    let v = rig.node.get("/git/ssh/resolve?host=github-acme").await;
    assert_eq!(v["hostname"], json!("github.com"), "{v}");
    assert_eq!(v["user"], json!("git"));
    assert_eq!(v["port"], json!(22));
    assert_eq!(v["identities_only"], json!(true));
    let files = v["identity_files"].as_array().unwrap();
    assert_eq!(files.len(), 1, "{v}");

    let greeting = rig
        .node
        .post("/git/ssh/test", json!({"host": "github.com"}))
        .await;
    assert_eq!(greeting["state"], json!("authenticated"), "{greeting}");
    assert_eq!(greeting["login"], json!("ada-acme"));
    let handshake = rig.fake.calls().pop().unwrap();
    assert!(
        handshake.args.contains(&"BatchMode=yes".to_string()),
        "never a prompt: {}",
        handshake.line()
    );
    rig.node.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_node_started_without_ssh_answers_503_on_every_ssh_route() {
    let node = Node::start().await;
    let (status, v) = node.req("GET", "/git/ssh", None).await;
    assert_eq!(status, 503, "{v}");
    let (status, _) = node
        .req(
            "POST",
            "/git/ssh/keys",
            Some(json!({"name": "id_ed25519_x"})),
        )
        .await;
    assert_eq!(status, 503);
    let (status, _) = node
        .req("GET", "/git/ssh/resolve?host=github.com", None)
        .await;
    assert_eq!(status, 503);
    node.shutdown().await;
}
