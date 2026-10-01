//! Profiles by organization, SSH for git hosts, code host accounts, and the
//! connection facts a checkout shows (ide/04, ide/08).
//!
//! **Fakes only.** The engine's `git` is a handle whose global config is one
//! temp file this test owns; SSH is a scripted `FakeSsh` over a temp
//! directory holding one synthetic public key; the code host is the in-memory
//! fake, signed in as whichever login the engine binds; `origin` is a URL
//! nothing connects to — or a bare repository on disk for the one probe that
//! reaches a remote. Nothing here reads `~/.ssh`, a keychain, a helper or
//! GitHub, and nothing is deleted.

use crate::common;

use bisa_codehost::fake::FakeCodeHost;
use bisa_codehost::CodeHostKind;
use bisa_core::{ProfileSpec, Slug, WorkstreamId};
use bisa_engine::codehost::{self, Connection};
use bisa_engine::ide::connection::{self, AccountSource, CautionId, IdentitySourceView, Transport};
use bisa_engine::{gitprofiles, identity, ssh, Engine, EngineConfig, EngineError, EnginePayload};
use bisa_harness::mock::MockAdapter;
use bisa_ssh::exec::Program;
use bisa_ssh::{FakeSsh, Ssh};
use bisa_store::NewProject;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The wire form of an ed25519 public key holding 32 zero bytes — a key
/// nobody holds.
fn synthetic_blob() -> Vec<u8> {
    let mut blob = Vec::new();
    blob.extend_from_slice(&11u32.to_be_bytes());
    blob.extend_from_slice(b"ssh-ed25519");
    blob.extend_from_slice(&32u32.to_be_bytes());
    blob.extend_from_slice(&[0u8; 32]);
    blob
}

/// The `.pub` line `ssh-keygen` would write for that key.
fn pub_line(comment: &str) -> String {
    use base64::Engine as _;
    format!(
        "ssh-ed25519 {} {comment}",
        base64::engine::general_purpose::STANDARD.encode(synthetic_blob())
    )
}

fn synthetic_fingerprint() -> String {
    bisa_ssh::fingerprint_sha256(&synthetic_blob())
}

struct Rig {
    dir: tempfile::TempDir,
    engine: Engine,
    fake_host: Arc<FakeCodeHost>,
    fake_ssh: Arc<FakeSsh>,
    keys: PathBuf,
}

impl Rig {
    /// An engine whose git sees one temp global file, whose SSH is the fake
    /// `script` builds over `keys/` (one key, `id_ed25519_acme`, already
    /// there), and whose code host is `fake_host`.
    fn start(script: impl FnOnce(&Path) -> FakeSsh, fake_host: FakeCodeHost) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let keys = dir.path().join("keys");
        std::fs::create_dir_all(&keys).unwrap();
        std::fs::write(
            keys.join("id_ed25519_acme.pub"),
            format!("{}\n", pub_line("ada@acme")),
        )
        .unwrap();
        std::fs::write(keys.join("id_ed25519_acme"), "not a key anybody holds\n").unwrap();
        let git = bisa_vcs::Git::new()
            .with_env("GIT_CONFIG_GLOBAL", dir.path().join("global.gitconfig"))
            .with_env("GIT_CONFIG_NOSYSTEM", "1");
        let fake_ssh = Arc::new(script(&keys));
        let fake_host = Arc::new(fake_host);
        let config = EngineConfig {
            design_enabled: false,
            events_enabled: false,
            git: Some(git),
            ssh: Some(Ssh::new(fake_ssh.clone(), &keys, dir.path())),
            code_hosts: Some(bisa_codehost::CodeHostRegistry::new(
                vec![fake_host.clone()],
            )),
            ..Default::default()
        };
        let engine = Engine::start(
            common::workspace(&dir),
            common::catalog_with(vec![MockAdapter::default()]),
            config,
        )
        .unwrap();
        Self {
            dir,
            engine,
            fake_host,
            fake_ssh,
            keys,
        }
    }

    fn key(&self) -> PathBuf {
        self.keys.join("id_ed25519_acme")
    }

    /// A git project whose `origin` is `url` — a URL nothing connects to, or
    /// a bare repository's path. Its primary workstream is what the facts are
    /// read for.
    async fn project(&self, slug: &str, url: &str) -> (WorkstreamId, PathBuf) {
        let ws = self.engine.workspace();
        let project = ws
            .create_project(NewProject::managed(slug).unwrap())
            .unwrap();
        let project = bisa_engine::projects::init_git(self.engine.inner(), &project)
            .await
            .unwrap();
        let root = ws.project_root_path(&project);
        bisa_vcs::git::remote_add(&root, "origin", url).unwrap();
        (WorkstreamId::primary_of(project.id), root)
    }

    fn bare_origin(&self, name: &str) -> PathBuf {
        let origin = self.dir.path().join(format!("{name}.git"));
        std::fs::create_dir_all(&origin).unwrap();
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(&origin)
            .args(["init", "--bare", "--quiet"])
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .unwrap();
        assert!(out.status.success());
        origin
    }

    /// The GitHub token store the engine reads — the file store under
    /// `identity/codehost/github/`, the same directory the engine's own
    /// builder uses for the kind.
    fn tokens(&self) -> bisa_codehost::creds::TokenStore {
        bisa_codehost::creds::TokenStore::file_only(
            bisa_codehost::CodeHostKind::GitHub,
            "github.com",
            self.engine
                .workspace()
                .paths()
                .codehost_tokens_root()
                .join("github"),
        )
    }
}

fn agent_holding(fingerprint: &str) -> String {
    format!("256 {fingerprint} ada@acme (ED25519)\n")
}

fn acme_spec(key: &Path) -> ProfileSpec {
    ProfileSpec {
        label: "Acme".into(),
        host: "github.com".into(),
        owner: "acme".into(),
        aliases: vec!["github-acme".into()],
        name: "Ada Lovelace".into(),
        email: "ada@acme.example".into(),
        ssh_key: Some(key.display().to_string()),
        account: Some("ada-acme".into()),
    }
}

fn fake_ssh_for(fp: &str, agent_answers: usize) -> FakeSsh {
    let mut fake = FakeSsh::new()
        .answer(Program::Ssh, &["-G", "github-acme"], 0, "user git\nhostname github.com\nport 22\nidentitiesonly yes\nidentityfile ~/.ssh/id_ed25519_acme\n", "")
        .answer(Program::Ssh, &["-G", "github-acme"], 0, "user git\nhostname github.com\nport 22\nidentitiesonly yes\nidentityfile ~/.ssh/id_ed25519_acme\n", "")
        .answer(Program::Ssh, &["-G", "github-acme"], 0, "user git\nhostname github.com\nport 22\nidentitiesonly yes\nidentityfile ~/.ssh/id_ed25519_acme\n", "");
    for _ in 0..6 {
        fake = fake.answer(Program::Ssh, &["-G", "github.com"], 0, "user git\nhostname github.com\nport 22\nidentitiesonly no\nidentityfile ~/.ssh/id_rsa\nidentityfile ~/.ssh/id_ed25519\n", "");
    }
    for _ in 0..agent_answers {
        fake = fake.answer(Program::SshAdd, &["-l"], 0, &agent_holding(fp), "");
    }
    fake
}

#[tokio::test(flavor = "multi_thread")]
async fn a_profile_gives_an_owners_checkouts_their_author_key_and_account_and_an_alias_still_matches(
) {
    let fp = synthetic_fingerprint();
    let rig = Rig::start(
        |_| fake_ssh_for(&fp, 12),
        FakeCodeHost::of_kind(CodeHostKind::GitHub, "github.com")
            .signed_in_as("personal")
            .with_organizations("ada-acme", &["acme"]),
    );
    let inner = rig.engine.inner();
    let mut bus = inner.subscribe();
    rig.tokens().store("personal", "ghp_personal").unwrap();
    rig.tokens().store("ada-acme", "ghp_acme").unwrap();
    identity::set_global_config(
        inner,
        vec![
            ("user.name".into(), "Grace Hopper".into()),
            ("user.email".into(), "grace@example.invalid".into()),
        ],
        vec![],
    )
    .await
    .unwrap();
    codehost::set_default_account(inner, CodeHostKind::GitHub, Some("personal"))
        .await
        .unwrap();

    let saved = gitprofiles::put(inner, Slug::new("acme").unwrap(), acme_spec(&rig.key()))
        .await
        .unwrap();
    assert_eq!(saved.profile.account.as_deref(), Some("ada-acme"));
    assert_eq!(
        saved.globs.len(),
        5,
        "three spellings plus two for the alias: {:?}",
        saved.globs
    );
    common::wait_for(&mut bus, "profiles changed", |e| {
        matches!(
            e.payload,
            EnginePayload::GitSetupChanged {
                what: bisa_engine::events::GitSetup::Profiles
            }
        )
    })
    .await;
    let listed = gitprofiles::list(inner).await.unwrap();
    assert!(listed.hasconfig_supported, "git {}", listed.git_version);
    assert_eq!(listed.profiles.len(), 1);
    assert_eq!(listed.profiles[0].profile.aliases, ["github-acme"]);
    assert!(listed.foreign_includes.is_empty());

    // The organization's checkout: the profile's author, key and account.
    let (acme, _) = rig.project("acme-web", "git@github.com:acme/web.git").await;
    let c = connection::connection(inner, acme).await.unwrap();
    assert_eq!(c.remote.as_ref().unwrap().summary, "github.com · acme/web");
    assert_eq!(c.code_host.as_deref(), Some("github"));
    assert_eq!(c.profile.as_ref().map(|p| p.slug.as_str()), Some("acme"));
    assert_eq!(
        c.identity.name.as_deref(),
        Some("Ada Lovelace"),
        "the profile's author, not the global one"
    );
    assert_eq!(
        c.identity.source,
        IdentitySourceView::Global,
        "resolved outside the repository"
    );
    assert_eq!(
        c.identity.profile.as_deref(),
        Some("acme"),
        "and the card says which profile"
    );
    assert_eq!(c.account.login.as_deref(), Some("ada-acme"));
    assert_eq!(c.account.source, AccountSource::Profile);
    match &c.transport {
        Transport::Ssh {
            key,
            loaded,
            identities_only,
            ssh_configured,
            ..
        } => {
            assert_eq!(
                key.as_deref(),
                Some(rig.key().as_path()),
                "the profile's key, before anything ssh -G would offer"
            );
            assert_eq!(*loaded, Some(true));
            assert!(*identities_only && *ssh_configured);
        }
        other => panic!("{other:?}"),
    }
    assert!(c.cautions.is_empty(), "{:?}", c.cautions);

    // A personal checkout: the global author, the default account, no profile.
    let (notes, _) = rig
        .project("grace-notes", "git@github.com:grace/notes.git")
        .await;
    let c = connection::connection(inner, notes).await.unwrap();
    assert!(c.profile.is_none());
    assert_eq!(c.identity.name.as_deref(), Some("Grace Hopper"));
    assert_eq!(c.identity.profile, None);
    assert_eq!(
        (c.account.login.as_deref(), c.account.source),
        (Some("personal"), AccountSource::Global)
    );
    assert!(
        matches!(
            &c.transport,
            Transport::Ssh {
                key: None,
                loaded: None,
                ..
            }
        ),
        "no key of ours would be offered: {:?}",
        c.transport
    );
    assert!(
        c.cautions.is_empty(),
        "a personal checkout under the global config earns no caution: {:?}",
        c.cautions
    );

    // An SSH alias: git matches the raw URL through the alias glob, and the
    // code host is found through what `ssh -G` says the alias stands for.
    let (tools, _) = rig
        .project("acme-tools", "git@github-acme:acme/tools.git")
        .await;
    let c = connection::connection(inner, tools).await.unwrap();
    let remote = c.remote.as_ref().unwrap();
    assert_eq!(remote.alias.as_deref(), Some("github-acme"));
    assert_eq!(remote.host.as_deref(), Some("github.com"));
    assert_eq!(c.code_host.as_deref(), Some("github"));
    assert_eq!(c.profile.as_ref().map(|p| p.slug.as_str()), Some("acme"));
    assert_eq!(c.identity.name.as_deref(), Some("Ada Lovelace"));
    assert_eq!(c.account.login.as_deref(), Some("ada-acme"));
    assert!(rig
        .fake_ssh
        .calls()
        .iter()
        .any(|call| call.args.contains(&"github-acme".to_string())
            && call.args.contains(&"-G".to_string())));

    // A local pin outranks the profile; a global write never outranks it.
    let c = connection::set_account(inner, acme, Some("Personal".into()))
        .await
        .unwrap();
    assert_eq!(
        (c.account.login.as_deref(), c.account.source),
        (Some("personal"), AccountSource::Local)
    );
    let c = connection::set_account(inner, acme, None).await.unwrap();
    assert_eq!(c.account.source, AccountSource::Profile);
    identity::set_global_config(inner, vec![("user.name".into(), "Grace H.".into())], vec![])
        .await
        .unwrap();
    let c = connection::connection(inner, acme).await.unwrap();
    assert_eq!(
        c.identity.name.as_deref(),
        Some("Ada Lovelace"),
        "the includes were re-appended after the global write"
    );

    // Nothing secret in what the routes would serve.
    let text = serde_json::to_string(&c).unwrap();
    let accounts = codehost::accounts(inner, CodeHostKind::GitHub)
        .await
        .unwrap();
    let text2 = serde_json::to_string(&accounts).unwrap();
    assert!(
        !text.contains("ghp_") && !text2.contains("ghp_"),
        "{text}\n{text2}"
    );
    assert_eq!(accounts.default.as_deref(), Some("personal"));
    assert_eq!(
        accounts
            .status
            .accounts
            .iter()
            .map(|a| a.login.as_str())
            .collect::<Vec<_>>(),
        ["ada-acme", "personal"]
    );

    // Removing the profile returns the checkout to the global layer.
    gitprofiles::remove(inner, Slug::new("acme").unwrap())
        .await
        .unwrap();
    let c = connection::connection(inner, acme).await.unwrap();
    assert!(c.profile.is_none());
    assert_eq!(c.identity.name.as_deref(), Some("Grace H."));
    assert!(gitprofiles::list(inner).await.unwrap().profiles.is_empty());
    let _ = &rig.fake_host;
    rig.engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_cautions_fire_on_a_local_author_that_is_not_the_profiles_and_on_a_key_ssh_agent_does_not_hold(
) {
    let fp = synthetic_fingerprint();
    // ssh-agent holds the key once, then nothing.
    let rig = Rig::start(
        |_| {
            fake_ssh_for(&fp, 1).answer(
                Program::SshAdd,
                &["-l"],
                1,
                "",
                "The agent has no identities.\n",
            )
        },
        FakeCodeHost::of_kind(CodeHostKind::GitHub, "github.com").signed_in_as("ada-acme"),
    );
    let inner = rig.engine.inner();
    rig.tokens().store("ada-acme", "ghp_acme").unwrap();
    gitprofiles::put(inner, Slug::new("acme").unwrap(), acme_spec(&rig.key()))
        .await
        .unwrap();
    let (acme, root) = rig.project("acme-web", "git@github.com:acme/web.git").await;

    let c = connection::connection(inner, acme).await.unwrap();
    assert!(c.cautions.is_empty(), "{:?}", c.cautions);

    bisa_vcs::git::set_local_identity(&root, "Bob", "bob@example.invalid").unwrap();
    let c = connection::connection(inner, acme).await.unwrap();
    let ids: Vec<CautionId> = c.cautions.iter().map(|c| c.id).collect();
    assert_eq!(c.identity.source, IdentitySourceView::Local);
    assert!(
        ids.contains(&CautionId::IdentityDiffersFromProfile),
        "{ids:?}"
    );
    assert!(
        ids.contains(&CautionId::KeyNotLoaded),
        "the second agent answer held nothing: {ids:?}"
    );
    assert!(c.cautions.iter().all(|c| !c.sentence.is_empty()));
    rig.engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_probes_bind_the_account_the_checkout_names_and_two_stored_with_none_named_is_a_refusal(
) {
    let fp = synthetic_fingerprint();
    let rig = Rig::start(
        |_| fake_ssh_for(&fp, 4),
        FakeCodeHost::of_kind(CodeHostKind::GitHub, "acme-origin").signed_in_as("personal"),
    );
    let inner = rig.engine.inner();
    // A bare repository on disk stands for the remote: the fake claims it by
    // name, and `ls-remote` reaches it without a network.
    let origin = rig.bare_origin("acme-origin");
    let (wid, _) = rig.project("acme-web", origin.to_str().unwrap()).await;
    rig.tokens().store("personal", "ghp_personal").unwrap();
    rig.tokens().store("ada-acme", "ghp_acme").unwrap();

    // Two stored, none named: nothing resolves, and the card says so. (The
    // GitHub implementation refuses the request by name at that point — the
    // codehost crate's own suite holds that; the fake has no chain to refuse from.)
    let c = connection::connection(inner, wid).await.unwrap();
    assert_eq!(
        (c.account.login.as_deref(), c.account.source),
        (None, AccountSource::None)
    );
    assert_eq!(
        c.cautions.iter().map(|c| c.id).collect::<Vec<_>>(),
        [CautionId::NoAccount]
    );
    let check = connection::check(inner, wid).await.unwrap();
    let ls = check.ls_remote.expect("origin exists");
    assert!(ls.ok && ls.heads == 0, "{ls:?}");
    assert!(check.ssh.is_none(), "a local remote has no SSH handshake");
    assert!(
        matches!(&check.code_host, Some(Connection::Connected { login, .. }) if login == "personal"),
        "unbound, the fake answers as itself: {:?}",
        check.code_host
    );

    codehost::set_default_account(inner, CodeHostKind::GitHub, Some("ada-acme"))
        .await
        .unwrap();
    let check = connection::check(inner, wid).await.unwrap();
    assert!(
        matches!(&check.code_host, Some(Connection::Connected { login, .. }) if login == "ada-acme"),
        "{:?}",
        check.code_host
    );
    assert_eq!(
        check.access,
        Some(bisa_codehost::RepoAccess {
            found: true,
            push: true
        })
    );
    let asked = rig.fake_host.asked_as();
    assert!(
        asked.len() >= 2 && asked[asked.len() - 2..].iter().all(|l| l == "ada-acme"),
        "once the default is set every request goes as it: {asked:?}"
    );

    let c = connection::connection(inner, wid).await.unwrap();
    assert!(matches!(c.transport, Transport::Local));
    assert_eq!(
        (c.account.login.as_deref(), c.account.source),
        (Some("ada-acme"), AccountSource::Global)
    );
    assert!(c.cautions.is_empty());

    let per_account = codehost::check_account(inner, CodeHostKind::GitHub, "ada-acme")
        .await
        .unwrap();
    assert!(matches!(per_account, Connection::Connected { .. }));
    assert_eq!(
        codehost::check_account(inner, CodeHostKind::GitHub, "nobody")
            .await
            .unwrap(),
        Connection::NoToken
    );
    codehost::forget_account(inner, CodeHostKind::GitHub, "personal").unwrap();
    assert_eq!(
        codehost::accounts(inner, CodeHostKind::GitHub)
            .await
            .unwrap()
            .status
            .accounts
            .len(),
        1
    );
    rig.engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn ssh_keys_are_listed_generated_loaded_and_tested_through_the_fake_and_unavailable_without_one(
) {
    let fp = synthetic_fingerprint();
    let widgets_line = pub_line("ada@widgets");
    // The fake writes the key pair as ssh-keygen would, once the argv is right.
    let rig = Rig::start(
        |keys| {
            FakeSsh::new()
                .answer(Program::SshAdd, &["-l"], 1, "", "The agent has no identities.\n")
                .answer(Program::SshKeygen, &["-t", "ed25519"], 0, "", "")
                .writing(keys.join("id_ed25519_widgets.pub"), &widgets_line)
                .writing(keys.join("id_ed25519_widgets"), "not a key anybody holds\n")
                .answer(Program::SshAdd, &[&keys.join("id_ed25519_widgets").display().to_string()], 0, "Identity added\n", "")
                .answer(Program::SshAdd, &["-l"], 0, &agent_holding(&fp), "")
                .answer(Program::Ssh, &["-T", "git@github.com"], 1, "", "Hi ada-acme! You've successfully authenticated, but GitHub does not provide shell access.\n")
        },
        FakeCodeHost::of_kind(CodeHostKind::GitHub, "github.com"),
    );
    let inner = rig.engine.inner();
    let mut bus = inner.subscribe();

    let before = ssh::overview(inner).await.unwrap();
    assert_eq!(before.keys.len(), 1);
    assert_eq!(before.keys[0].key.name, "id_ed25519_acme");
    assert!(!before.keys[0].loaded);
    assert!(before.agent.available);

    // A taken name is refused before anything is spawned.
    let taken = ssh::generate(
        inner,
        ssh::NewKey {
            name: "id_ed25519_acme".into(),
            comment: "x".into(),
        },
    )
    .await
    .unwrap_err();
    assert!(
        matches!(taken, EngineError::Ssh(bisa_ssh::SshError::Refused(_))),
        "{taken}"
    );
    assert!(rig
        .fake_ssh
        .calls()
        .iter()
        .all(|c| c.program != Program::SshKeygen));

    let key = ssh::generate(
        inner,
        ssh::NewKey {
            name: "id_ed25519_widgets".into(),
            comment: "ada@widgets".into(),
        },
    )
    .await
    .unwrap();
    assert_eq!(key.name, "id_ed25519_widgets");
    assert_eq!(key.comment, "ada@widgets");
    let keygen = rig
        .fake_ssh
        .calls()
        .into_iter()
        .find(|c| c.program == Program::SshKeygen)
        .expect("ssh-keygen ran");
    assert!(
        keygen.args.contains(&"-N".to_string()) && keygen.args.contains(&String::new()),
        "no passphrase from the platform: {:?}",
        keygen.args
    );
    common::wait_for(&mut bus, "keys changed", |e| {
        matches!(
            e.payload,
            EnginePayload::GitSetupChanged {
                what: bisa_engine::events::GitSetup::Keys
            }
        )
    })
    .await;

    ssh::load(inner, "id_ed25519_widgets".into()).await.unwrap();
    let after = ssh::overview(inner).await.unwrap();
    assert!(
        after.keys.iter().all(|k| k.loaded),
        "both keys share the synthetic blob the agent now lists"
    );

    let greeting = ssh::test(inner, "github.com".into(), "git".into(), Some(rig.key()))
        .await
        .unwrap();
    assert_eq!(
        greeting,
        ssh::HostGreeting::Authenticated {
            login: Some("ada-acme".into())
        }
    );
    let test_call = rig
        .fake_ssh
        .calls()
        .into_iter()
        .find(|c| c.args.contains(&"-T".to_string()))
        .unwrap();
    assert!(
        test_call.args.contains(&"BatchMode=yes".to_string())
            && test_call.args.contains(&"-i".to_string())
    );
    rig.engine.shutdown().await;

    // An engine nobody gave SSH answers `SshUnavailable` and spawns nothing.
    let dir = tempfile::tempdir().unwrap();
    let engine = common::engine_with(&dir, vec![MockAdapter::default()]);
    let err = ssh::overview(engine.inner()).await.unwrap_err();
    assert!(matches!(err, EngineError::SshUnavailable(_)), "{err}");
    assert!(
        !err.is_refusal(),
        "a missing configuration is not the person's refusal"
    );
    engine.shutdown().await;
}

/// A repository nobody commits in, whose remote resolves to a connected
/// account, is offered that account's identity — the host's name and the
/// no-reply address built from its id — and nothing is written until the
/// person says so.
#[tokio::test(flavor = "multi_thread")]
async fn a_repository_nobody_commits_in_suggests_the_connected_account() {
    let fp = synthetic_fingerprint();
    let rig = Rig::start(
        |_| fake_ssh_for(&fp, 1),
        FakeCodeHost::of_kind(CodeHostKind::GitHub, "github.com")
            .signed_in_as("ada-acme")
            .known_as("Ada Lovelace", None, Some(7)),
    );
    let inner = rig.engine.inner();
    rig.tokens().store("ada-acme", "ghp_acme").unwrap();
    let (acme, root) = rig.project("acme-web", "git@github.com:acme/web.git").await;

    let identity = bisa_engine::ide::git::identity(inner, acme).await.unwrap();
    assert_eq!(
        identity.source,
        bisa_vcs::IdentitySource::None,
        "the temp global holds nobody: {identity:?}"
    );
    let suggested = bisa_engine::ide::git::committer_suggestion(inner, acme)
        .await
        .unwrap()
        .expect("the one stored account is the checkout's");
    assert_eq!(suggested.login, "ada-acme");
    assert_eq!(suggested.ident.name, "Ada Lovelace");
    assert_eq!(suggested.ident.email, "7+ada-acme@users.noreply.github.com");
    let after = bisa_engine::ide::git::identity(inner, acme).await.unwrap();
    assert_eq!(
        after.source,
        bisa_vcs::IdentitySource::None,
        "offered, never written"
    );

    // Said yes: the pair lands in the repository's local layer, as any answer does.
    bisa_vcs::git::set_local_identity(&root, &suggested.ident.name, &suggested.ident.email)
        .unwrap();
    let set = bisa_engine::ide::git::identity(inner, acme).await.unwrap();
    assert_eq!(set.source, bisa_vcs::IdentitySource::Local);
    assert_eq!(
        set.email.as_deref(),
        Some("7+ada-acme@users.noreply.github.com")
    );
    rig.engine.shutdown().await;
}

/// The credential chain ends at git's own helper, and on a developer's
/// machine that helper is the OS keychain. A source guard: a test file that
/// calls what walks the chain hands its engine a `git` without the machine's
/// configuration — by its own rig, or the shared `common::isolated_git`.
#[test]
fn a_test_that_walks_the_credential_chain_never_reaches_the_machines_git() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/it");
    let walks = [
        "codehost::accounts(",
        "codehost::health(",
        "codehost::check_account(",
        "codehost::login_plan(",
        "codehost::inspect(",
    ];
    let mut owed = Vec::new();
    let mut seen = 0;
    for entry in std::fs::read_dir(&dir).unwrap().flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        if !walks.iter().any(|w| text.contains(w)) {
            continue;
        }
        seen += 1;
        let isolated = text.contains("GIT_CONFIG_GLOBAL") || text.contains("isolated_git()");
        if !isolated {
            owed.push(path.file_name().unwrap().to_string_lossy().into_owned());
        }
    }
    assert!(seen >= 1, "this file walks the chain itself");
    assert_eq!(owed, Vec::<String>::new());
}
