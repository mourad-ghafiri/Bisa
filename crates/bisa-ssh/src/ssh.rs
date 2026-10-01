//! The door: one `Ssh` over a runner and a directory. Every method builds an
//! argv, asks the runner, and reads the answer with the pure parsers; the
//! only files opened are `*.pub` and `config`, through one helper.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::agent::{parse_ssh_add_list, AgentState};
use crate::config::{git_host_blocks, parse_ssh_config, HostBlock};
use crate::exec::{Program, SshRunner, Timeouts};
use crate::greeting::{parse_greeting, HostGreeting};
use crate::keys::{parse_public_key, PublicKey};
use crate::resolve::{parse_ssh_g, Resolved};
use crate::{SshError, SshResult, KNOWN_GIT_HOSTS};

/// A key with whether ssh-agent holds it.
#[derive(
    Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
pub struct KeyRow {
    #[serde(flatten)]
    pub key: PublicKey,
    pub loaded: bool,
}

/// Everything the SSH keys panel shows, in one read.
#[derive(
    Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
pub struct SshOverview {
    pub dir: PathBuf,
    pub keys: Vec<KeyRow>,
    pub agent: AgentState,
    /// The `Host` blocks of `config` that concern git hosts.
    pub hosts: Vec<HostBlock>,
    pub known_git_hosts: Vec<String>,
}

/// A key pair to generate: its file name under the directory and the comment
/// the public key carries (an email, a machine — the person's word).
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NewKey {
    pub name: String,
    #[serde(default)]
    pub comment: String,
}

/// SSH as the engine drives it. Clone-cheap; `Debug` prints the directory.
#[derive(Debug, Clone)]
pub struct Ssh {
    runner: Arc<dyn SshRunner>,
    dir: PathBuf,
    home: PathBuf,
    timeouts: Timeouts,
}

impl Ssh {
    /// `dir` is the SSH directory (`~/.ssh` in production, a temp dir in a
    /// test); `home` is what `~` expands to in `ssh -G`'s answers.
    pub fn new(
        runner: Arc<dyn SshRunner>,
        dir: impl Into<PathBuf>,
        home: impl Into<PathBuf>,
    ) -> Self {
        Self {
            runner,
            dir: dir.into(),
            home: home.into(),
            timeouts: Timeouts::default(),
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The one file read of the crate — a `.pub` file or `config`, nothing
    /// else, so a private key can never come through here.
    fn read_public_text(&self, path: &Path) -> SshResult<Option<String>> {
        let public = path.extension().is_some_and(|e| e == "pub")
            || path.file_name().is_some_and(|n| n == "config");
        if !public {
            return Err(SshError::Refused(format!(
                "{} is not a public file",
                path.display()
            )));
        }
        match std::fs::read_to_string(path) {
            Ok(text) => Ok(Some(text)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(SshError::Failed {
                what: format!("read {}", path.display()),
                detail: e.to_string(),
            }),
        }
    }

    /// Every key pair in the directory with a `.pub` file, sorted by name.
    pub fn keys(&self) -> SshResult<Vec<PublicKey>> {
        let entries = match std::fs::read_dir(&self.dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => {
                return Err(SshError::Failed {
                    what: format!("list {}", self.dir.display()),
                    detail: e.to_string(),
                })
            }
        };
        let mut keys = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|e| e != "pub") {
                continue;
            }
            let Some(name) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let Some(line) = self.read_public_text(&path)? else {
                continue;
            };
            if let Some(key) = parse_public_key(name, self.dir.join(name), &line) {
                keys.push(key);
            }
        }
        keys.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(keys)
    }

    /// What ssh-agent holds.
    pub fn agent(&self) -> SshResult<AgentState> {
        let out = self
            .runner
            .run(Program::SshAdd, &["-l".to_string()], self.timeouts.local)?;
        Ok(parse_ssh_add_list(out.code, &out.stdout, &out.stderr))
    }

    /// The `Host` blocks of the directory's `config` that concern git hosts.
    pub fn config(&self) -> SshResult<Vec<HostBlock>> {
        let Some(text) = self.read_public_text(&self.dir.join("config"))? else {
            return Ok(Vec::new());
        };
        Ok(git_host_blocks(&parse_ssh_config(&text), KNOWN_GIT_HOSTS))
    }

    /// Keys, agent and hosts in one read.
    pub fn overview(&self) -> SshResult<SshOverview> {
        let agent = self.agent().unwrap_or_else(|e| AgentState {
            available: false,
            keys: Vec::new(),
            reason: Some(e.to_string()),
        });
        let keys = self
            .keys()?
            .into_iter()
            .map(|key| KeyRow {
                loaded: agent.holds(&key.fingerprint),
                key,
            })
            .collect();
        Ok(SshOverview {
            dir: self.dir.clone(),
            keys,
            agent,
            hosts: self.config()?,
            known_git_hosts: KNOWN_GIT_HOSTS.iter().map(|h| h.to_string()).collect(),
        })
    }

    /// Generate an ed25519 key pair under the directory — `ssh-keygen -q -t
    /// ed25519 -f <dir>/<name> -C <comment> -N ""`. The directory is created
    /// owner-only when absent; a name already taken, a name or a comment that
    /// reads as an option, are refused before anything is spawned. **No
    /// passphrase**: one on argv would be visible to every process; a person
    /// adds one with `ssh-keygen -p` in a terminal.
    pub fn generate(&self, key: &NewKey) -> SshResult<PublicKey> {
        let name = validate_key_name(&key.name)?;
        let comment = key.comment.trim();
        if comment.starts_with('-') || comment.contains(['\r', '\n', '\0']) {
            return Err(SshError::Refused(format!(
                "{comment:?} is not a comment ssh-keygen will take"
            )));
        }
        let private = self.dir.join(name);
        let public = self.dir.join(format!("{name}.pub"));
        if private.exists() || public.exists() {
            return Err(SshError::Refused(format!(
                "a key named {name} already exists in {}",
                self.dir.display()
            )));
        }
        create_dir_owner_only(&self.dir)?;
        let args = vec![
            "-q".to_string(),
            "-t".to_string(),
            "ed25519".to_string(),
            "-f".to_string(),
            private.display().to_string(),
            "-C".to_string(),
            comment.to_string(),
            "-N".to_string(),
            String::new(),
        ];
        let out = self
            .runner
            .run(Program::SshKeygen, &args, self.timeouts.local)?;
        if !out.success() {
            return Err(SshError::Failed {
                what: "ssh-keygen".to_string(),
                detail: first_line(&out.stderr),
            });
        }
        let line = self
            .read_public_text(&public)?
            .ok_or_else(|| SshError::Failed {
                what: "ssh-keygen".to_string(),
                detail: format!("{} was not written", public.display()),
            })?;
        parse_public_key(name, private, &line).ok_or_else(|| SshError::Failed {
            what: "ssh-keygen".to_string(),
            detail: format!("{} is not a public key", public.display()),
        })
    }

    /// Load a key into ssh-agent — `ssh-add <dir>/<name>` (with
    /// `--apple-use-keychain` on macOS). A key that is not in the directory is
    /// refused before anything is spawned; one that wants a passphrase cannot
    /// be loaded from here (no prompt is possible) and the refusal says which
    /// command to run in a terminal.
    pub fn agent_add(&self, name: &str) -> SshResult<()> {
        let name = validate_key_name(name)?;
        let private = self.dir.join(name);
        if !self.dir.join(format!("{name}.pub")).exists() || !private.exists() {
            return Err(SshError::Refused(format!(
                "no key named {name} in {}",
                self.dir.display()
            )));
        }
        let mut args = Vec::new();
        if cfg!(target_os = "macos") {
            args.push("--apple-use-keychain".to_string());
        }
        args.push(private.display().to_string());
        let out = self
            .runner
            .run(Program::SshAdd, &args, self.timeouts.local)?;
        if out.success() {
            return Ok(());
        }
        let detail = first_line(&out.stderr);
        if out.code == 2 || detail.contains("authentication agent") {
            return Err(SshError::Refused(format!(
                "ssh-agent is not running: {detail}"
            )));
        }
        Err(SshError::Refused(format!(
            "ssh-add could not load {name} without a prompt ({detail}); in a terminal, run `ssh-add {}`",
            private.display()
        )))
    }

    /// What ssh would do for `host` — `ssh -G`, offline. With `identity`, the
    /// `-i <key> -o IdentitiesOnly=yes` a profile's `core.sshCommand` carries,
    /// so the answer is the one a push through that profile would get.
    pub fn resolve(&self, host: &str, identity: Option<&Path>) -> SshResult<Resolved> {
        validate_host(host)?;
        let mut args = vec!["-G".to_string()];
        args.extend(identity_args(identity)?);
        args.push(host.to_string());
        let out = self.runner.run(Program::Ssh, &args, self.timeouts.local)?;
        if !out.success() {
            return Err(SshError::Failed {
                what: format!("ssh -G {host}"),
                detail: first_line(&out.stderr),
            });
        }
        Ok(parse_ssh_g(&out.stdout, &self.home))
    }

    /// The one handshake: `ssh -T -o BatchMode=yes user@host`, read into a
    /// [`HostGreeting`]. Nothing is written on either side; an unknown host
    /// key is reported, never accepted.
    pub fn test(&self, host: &str, user: &str, identity: Option<&Path>) -> SshResult<HostGreeting> {
        validate_host(host)?;
        validate_host(user)?;
        let mut args = vec![
            "-T".to_string(),
            "-o".to_string(),
            "BatchMode=yes".to_string(),
            "-o".to_string(),
            "ConnectTimeout=15".to_string(),
        ];
        args.extend(identity_args(identity)?);
        args.push(format!("{user}@{host}"));
        let out = self
            .runner
            .run(Program::Ssh, &args, self.timeouts.network)?;
        Ok(parse_greeting(host, out.code, &out.stdout, &out.stderr))
    }
}

fn identity_args(identity: Option<&Path>) -> SshResult<Vec<String>> {
    let Some(key) = identity else {
        return Ok(Vec::new());
    };
    let text = key
        .to_str()
        .ok_or_else(|| SshError::Refused("a key path that is not text".into()))?;
    if !key.is_absolute() || text.starts_with('-') || text.contains(char::is_whitespace) {
        return Err(SshError::Refused(format!(
            "{text:?} is not a key path ssh can be handed"
        )));
    }
    Ok(vec![
        "-i".to_string(),
        text.to_string(),
        "-o".to_string(),
        "IdentitiesOnly=yes".to_string(),
    ])
}

/// A key pair's file name: letters, digits, `.`, `_`, `-`, not starting with
/// a dash or a dot, no separator — one path component that is not an option.
fn validate_key_name(name: &str) -> SshResult<&str> {
    let name = name.trim();
    let ok = !name.is_empty()
        && name.len() <= 100
        && !name.starts_with(['-', '.'])
        && !name.ends_with(".pub")
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'));
    if !ok {
        return Err(SshError::Refused(format!(
            "{name:?} is not a key name: letters, digits, dots, dashes and underscores, not starting with a dash"
        )));
    }
    Ok(name)
}

/// A host, an alias or a user handed to ssh: one word that is not an option.
fn validate_host(word: &str) -> SshResult<()> {
    let ok = !word.is_empty()
        && !word.starts_with('-')
        && word
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'));
    if !ok {
        return Err(SshError::Refused(format!(
            "{word:?} is not a host name ssh can be handed"
        )));
    }
    Ok(())
}

fn first_line(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or_default()
        .to_string()
}

fn create_dir_owner_only(dir: &Path) -> SshResult<()> {
    if dir.is_dir() {
        return Ok(());
    }
    let io = |e: std::io::Error| SshError::Failed {
        what: format!("create {}", dir.display()),
        detail: e.to_string(),
    };
    std::fs::create_dir_all(dir).map_err(io)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700)).map_err(io)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::FakeSsh;
    use crate::keys::tests::synthetic_pub_line;

    fn ssh_over(fake: FakeSsh, dir: &Path) -> (Ssh, Arc<FakeSsh>) {
        let fake = Arc::new(fake);
        (Ssh::new(fake.clone(), dir, dir.join("home")), fake)
    }

    fn write_pub(dir: &Path, name: &str, comment: &str) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join(format!("{name}.pub")), synthetic_pub_line(comment)).unwrap();
        std::fs::write(dir.join(name), "not a key anybody holds\n").unwrap();
    }

    #[test]
    fn keys_are_listed_from_their_pub_files_and_marked_loaded_from_the_agents_answer() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("ssh");
        write_pub(&dir, "id_ed25519_acme", "ada@acme");
        write_pub(&dir, "id_ed25519", "ada@home");
        std::fs::write(dir.join("known_hosts"), "github.com ssh-ed25519 AAAA\n").unwrap();
        std::fs::write(
            dir.join("config"),
            "Host github-acme\n  HostName github.com\n  IdentityFile ~/.ssh/id_ed25519_acme\n",
        )
        .unwrap();
        let fp = parse_public_key("x", PathBuf::from("/x"), &synthetic_pub_line(""))
            .unwrap()
            .fingerprint;
        let fake = FakeSsh::new().answer(
            Program::SshAdd,
            &["-l"],
            0,
            &format!("256 {fp} ada@acme (ED25519)\n"),
            "",
        );
        let (ssh, fake) = ssh_over(fake, &dir);
        let over = ssh.overview().unwrap();
        assert_eq!(
            over.keys
                .iter()
                .map(|k| k.key.name.as_str())
                .collect::<Vec<_>>(),
            ["id_ed25519", "id_ed25519_acme"],
            "sorted, .pub files only"
        );
        assert!(
            over.keys.iter().all(|k| k.loaded),
            "both share the synthetic blob, so both are loaded"
        );
        assert_eq!(over.keys[0].key.path, dir.join("id_ed25519"));
        assert!(over.agent.available);
        assert_eq!(over.hosts.len(), 1);
        assert_eq!(over.hosts[0].hostname(), Some("github.com"));
        assert_eq!(fake.calls()[0].line(), "ssh-add -l");
        assert_eq!(
            fake.calls().len(),
            1,
            "keys and hosts are files; only the agent is a program"
        );

        let missing = Ssh::new(
            Arc::new(FakeSsh::new().answer(
                Program::SshAdd,
                &["-l"],
                2,
                "",
                "Could not open a connection to your authentication agent.\n",
            )),
            root.path().join("absent"),
            root.path(),
        );
        let over = missing.overview().unwrap();
        assert!(over.keys.is_empty() && over.hosts.is_empty());
        assert!(!over.agent.available);
        assert!(
            ssh.read_public_text(&dir.join("id_ed25519")).is_err(),
            "a private key is never read, whoever asks"
        );
    }

    #[test]
    fn generate_refuses_a_taken_name_and_a_bad_comment_before_any_spawn_and_reads_the_new_pub_file()
    {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("ssh");
        write_pub(&dir, "id_ed25519", "ada@home");
        let public_line = synthetic_pub_line("ada@acme");
        let expected_private = dir.join("id_ed25519_acme");
        // The fake writes the `.pub` file as ssh-keygen would, once the argv is right.
        let fake = FakeSsh::new()
            .answer(Program::SshKeygen, &["-t", "ed25519"], 0, "", "")
            .writing(dir.join("id_ed25519_acme.pub"), &public_line);
        let (ssh, fake) = ssh_over(fake, &dir);
        let taken = ssh
            .generate(&NewKey {
                name: "id_ed25519".into(),
                comment: String::new(),
            })
            .unwrap_err();
        assert!(
            matches!(taken, SshError::Refused(ref r) if r.contains("already exists")),
            "{taken}"
        );
        assert!(ssh
            .generate(&NewKey {
                name: "-rf".into(),
                comment: String::new()
            })
            .is_err());
        assert!(ssh
            .generate(&NewKey {
                name: "a/b".into(),
                comment: String::new()
            })
            .is_err());
        assert!(ssh
            .generate(&NewKey {
                name: "ok".into(),
                comment: "-o Bad".into()
            })
            .is_err());
        assert!(fake.calls().is_empty(), "every refusal came before a spawn");

        let key = ssh
            .generate(&NewKey {
                name: " id_ed25519_acme ".into(),
                comment: "ada@acme".into(),
            })
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(key.name, "id_ed25519_acme");
        assert_eq!(key.comment, "ada@acme");
        assert_eq!(key.path, expected_private);
        let call = &fake.calls()[0];
        assert_eq!(call.program, Program::SshKeygen);
        assert_eq!(
            call.args,
            [
                "-q",
                "-t",
                "ed25519",
                "-f",
                &expected_private.display().to_string(),
                "-C",
                "ada@acme",
                "-N",
                ""
            ]
        );
    }

    #[test]
    fn agent_add_refuses_a_missing_key_and_names_the_terminal_command_when_a_passphrase_is_wanted()
    {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("ssh");
        write_pub(&dir, "id_ed25519_acme", "ada@acme");
        let fake = FakeSsh::new()
            .answer(
                Program::SshAdd,
                &[&dir.join("id_ed25519_acme").display().to_string()],
                1,
                "",
                "Enter passphrase for key: \n",
            )
            .answer(
                Program::SshAdd,
                &[&dir.join("id_ed25519_acme").display().to_string()],
                0,
                "Identity added\n",
                "",
            );
        let (ssh, fake) = ssh_over(fake, &dir);
        assert!(matches!(ssh.agent_add("nope"), Err(SshError::Refused(_))));
        assert!(fake.calls().is_empty());
        let prompt = ssh.agent_add("id_ed25519_acme").unwrap_err();
        assert!(
            prompt.to_string().contains("ssh-add") && prompt.to_string().contains("terminal"),
            "{prompt}"
        );
        ssh.agent_add("id_ed25519_acme").unwrap();
        let args = &fake.calls()[1].args;
        assert_eq!(
            args.last().unwrap(),
            &dir.join("id_ed25519_acme").display().to_string()
        );
        if cfg!(target_os = "macos") {
            assert_eq!(args[0], "--apple-use-keychain");
        }
    }

    #[test]
    fn resolve_and_test_carry_the_profiles_key_and_batch_mode_and_never_a_shell() {
        let root = tempfile::tempdir().unwrap();
        let key = root.path().join("id_ed25519_acme");
        let fake = FakeSsh::new()
            .answer(Program::Ssh, &["-G", "github-acme"], 0, "user git\nhostname github.com\nport 22\nidentitiesonly yes\nidentityfile ~/.ssh/id_ed25519_acme\n", "")
            .answer(Program::Ssh, &["-T", "git@github-acme"], 1, "", "Hi ada-acme! You've successfully authenticated, but GitHub does not provide shell access.\n")
            .answer(Program::Ssh, &["-T", "git@github.com"], 255, "", "git@github.com: Permission denied (publickey).\n");
        let (ssh, fake) = ssh_over(fake, root.path());
        let resolved = ssh.resolve("github-acme", Some(&key)).unwrap();
        assert_eq!(resolved.hostname, "github.com");
        assert_eq!(
            resolved.identity_files,
            [root
                .path()
                .join("home")
                .join(".ssh")
                .join("id_ed25519_acme")],
            "~ expands against the home handed in"
        );
        assert_eq!(
            ssh.test("github-acme", "git", Some(&key)).unwrap(),
            HostGreeting::Authenticated {
                login: Some("ada-acme".into())
            }
        );
        assert!(matches!(
            ssh.test("github.com", "git", None).unwrap(),
            HostGreeting::Refused { .. }
        ));
        let calls = fake.calls();
        assert_eq!(
            calls[0].args,
            [
                "-G",
                "-i",
                &key.display().to_string(),
                "-o",
                "IdentitiesOnly=yes",
                "github-acme"
            ]
        );
        assert!(calls[1].args.contains(&"BatchMode=yes".to_string()));
        assert!(
            calls[1].args.contains(&"-i".to_string())
                && calls[1].args.last().unwrap() == "git@github-acme"
        );
        assert!(
            !calls[2].args.contains(&"-i".to_string()),
            "no key named, none forced"
        );
        assert!(
            calls.iter().all(|c| c
                .args
                .iter()
                .all(|a| !a.contains(';') && !a.contains("sh -c"))),
            "argv only"
        );
        assert!(
            ssh.resolve("-oProxyCommand=x", None).is_err(),
            "an option-shaped host is refused before spawn"
        );
        assert!(ssh.test("github.com", "-x", None).is_err());
        assert!(ssh
            .resolve("github.com", Some(Path::new("relative/key")))
            .is_err());
        assert_eq!(fake.calls().len(), 3);
    }
}
