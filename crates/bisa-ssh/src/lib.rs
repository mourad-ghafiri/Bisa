//! SSH for git hosts, as typed, time-boxed subprocess calls — the OpenSSH
//! programs (`ssh`, `ssh-keygen`, `ssh-add`) the way `bisa-vcs` treats
//! `git` (ide/04 §Profiles by organization, Settings › Git & code hosts › SSH keys).
//!
//! What this crate does: lists a person's **public** keys with their
//! fingerprints, asks ssh-agent which keys it holds, asks `ssh -G` which
//! identity would be offered to a host, reads the `Host` blocks of
//! `ssh_config` that concern git hosts, generates a new ed25519 key pair,
//! loads a key into ssh-agent, and runs the one authentication handshake a
//! git host answers with a greeting (`ssh -T git@github.com`).
//!
//! Three properties are load-bearing and held by tests:
//!
//! 1. **Private keys are never opened.** The only files read are `*.pub` and
//!    `ssh_config`; a fingerprint is computed from the public key's blob
//!    (SHA-256 over the base64 payload, no subprocess); which key a host gets
//!    is `ssh -G`'s answer. A unit test greps the crate for any other read.
//! 2. **argv only, never a shell, never a prompt.** One runner ([`exec::Cli`])
//!    behind a port ([`exec::SshRunner`]): `BatchMode=yes`, a null stdin,
//!    `SSH_ASKPASS` emptied and `SSH_ASKPASS_REQUIRE=never`, every child
//!    time-boxed and terminated on expiry. Nothing in this crate can wait on a
//!    passphrase — a key that needs one is refused with the command a person
//!    runs in a terminal.
//! 3. **Every test runs against [`fake::FakeSsh`]** on a `tempfile` directory:
//!    scripted answers, recorded argv, no `ssh` spawned, no `~/.ssh` read.
//!
//! The SSH directory is a parameter of [`ssh::Ssh`] — `~/.ssh` in production,
//! a temp dir in every test — and so is the home the `~` of `ssh -G` expands
//! against. The greeting parser ([`greeting`]) is the crate's one prose
//! boundary, like `bisa-vcs::classify`: each git host's sentence is read
//! once into a typed [`greeting::HostGreeting`].

pub mod agent;
pub mod config;
pub mod exec;
pub mod fake;
pub mod greeting;
pub mod keys;
pub mod resolve;
pub mod ssh;

pub use agent::{AgentKey, AgentState};
pub use config::{git_host_blocks, host_block_text, parse_ssh_config, HostBlock};
pub use exec::{Cli, Output, Program, SshRunner, Timeouts};
pub use fake::FakeSsh;
pub use greeting::HostGreeting;
pub use keys::{fingerprint_sha256, parse_public_key, PublicKey};
pub use resolve::Resolved;
pub use ssh::{KeyRow, NewKey, Ssh, SshOverview};

/// The git hosts whose `Host` blocks Settings shows and whose greetings the
/// parser knows. A host not here still works; it just earns no login from its
/// greeting.
pub const KNOWN_GIT_HOSTS: &[&str] = &[
    "github.com",
    "gitlab.com",
    "bitbucket.org",
    "codeberg.org",
    "ssh.dev.azure.com",
    "git.sr.ht",
];

/// Every way an SSH call can fail, as a type the caller can act on.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SshError {
    /// The program is missing, or SSH was not configured for this engine at all.
    #[error("ssh unavailable: {0}")]
    Unavailable(String),
    /// Refused before anything was spawned, or by the program for a reason the
    /// person has to act on: a name already taken, a key not in the directory,
    /// an agent that wants a passphrase.
    #[error("{0}")]
    Refused(String),
    /// The child exceeded its budget and was terminated.
    #[error("{what} timed out after {secs}s")]
    Timeout { what: String, secs: u64 },
    /// An unclassified failure, with the program's own words.
    #[error("{what}: {detail}")]
    Failed { what: String, detail: String },
}

pub type SshResult<T> = Result<T, SshError>;

#[cfg(test)]
mod guard {
    /// The two rules the crate's shape holds: a process is spawned in
    /// `exec.rs` alone, and a file is read in `ssh.rs` alone — through one
    /// helper that accepts a `.pub` file or `config` and nothing else.
    #[test]
    fn processes_are_spawned_in_exec_alone_and_only_public_files_are_read() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        for entry in std::fs::read_dir(&src).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            let text = std::fs::read_to_string(&path).unwrap();
            let shipped = text.split("#[cfg(test)]").next().unwrap_or_default();
            if name != "exec.rs" {
                assert!(!shipped.contains("std::process"), "{name} spawns a process");
                assert!(!shipped.contains("Command::new"), "{name} spawns a process");
            }
            let reads = shipped.matches("fs::read_to_string").count()
                + shipped.matches("fs::read(").count();
            match name.as_str() {
                "ssh.rs" => assert_eq!(
                    reads, 1,
                    "ssh.rs reads files through `read_public_text` alone"
                ),
                _ => assert_eq!(reads, 0, "{name} reads a file"),
            }
        }
    }
}
