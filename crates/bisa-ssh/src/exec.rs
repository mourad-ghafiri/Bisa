//! The one way this crate starts a process, behind a port so every test runs
//! against a fake instead.

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::{SshError, SshResult};

/// The three OpenSSH programs this crate asks anything of.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Program {
    Ssh,
    SshKeygen,
    SshAdd,
}

impl Program {
    pub fn binary(self) -> &'static str {
        match self {
            Program::Ssh => "ssh",
            Program::SshKeygen => "ssh-keygen",
            Program::SshAdd => "ssh-add",
        }
    }
}

/// Wall-clock budgets: a local question (`ssh -G`, `ssh-keygen`, `ssh-add`)
/// and one that reaches a host (`ssh -T`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timeouts {
    pub local: Duration,
    pub network: Duration,
}

impl Default for Timeouts {
    fn default() -> Self {
        Self {
            local: Duration::from_secs(15),
            network: Duration::from_secs(30),
        }
    }
}

/// A finished child. A non-zero exit is an answer here — `ssh -T` to GitHub
/// exits 1 on success, `ssh-add -l` exits 1 for *no identities* — so the
/// caller classifies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl Output {
    pub fn success(&self) -> bool {
        self.code == 0
    }
}

/// The port: run one program with one argv under one budget.
pub trait SshRunner: Send + Sync + std::fmt::Debug {
    fn run(&self, program: Program, args: &[String], timeout: Duration) -> SshResult<Output>;
}

/// The real programs, on `PATH`. argv only, never a shell; stdin null; the
/// prompts closed (`SSH_ASKPASS=""`, `SSH_ASKPASS_REQUIRE=never`, no
/// `DISPLAY`); `LC_ALL=C` so a greeting reads the same on every machine;
/// the child terminated when its budget passes.
#[derive(Debug, Clone, Default)]
pub struct Cli {
    pub timeouts: Timeouts,
}

impl SshRunner for Cli {
    fn run(&self, program: Program, args: &[String], timeout: Duration) -> SshResult<Output> {
        let what = format!("{} {}", program.binary(), args.join(" "));
        let mut cmd = Command::new(program.binary());
        cmd.args(args)
            .env("SSH_ASKPASS", "")
            .env("SSH_ASKPASS_REQUIRE", "never")
            .env_remove("DISPLAY")
            .env("LC_ALL", "C")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = cmd
            .spawn()
            .map_err(|e| SshError::Unavailable(format!("spawn `{}`: {e}", program.binary())))?;
        let out_reader = drain(child.stdout.take().expect("stdout was piped"));
        let err_reader = drain(child.stderr.take().expect("stderr was piped"));
        let deadline = Instant::now() + timeout;
        let mut nap = Duration::from_millis(1);
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => {
                    let remaining = deadline.saturating_duration_since(Instant::now());
                    if remaining.is_zero() {
                        // Ended and reaped; an end that fails is a child
                        // already gone, and the timeout is the answer either way.
                        let _already_gone = child.kill();
                        let _reaped = child.wait();
                        return Err(SshError::Timeout {
                            what,
                            secs: timeout.as_secs(),
                        });
                    }
                    std::thread::sleep(nap.min(remaining));
                    nap = (nap * 2).min(Duration::from_millis(25));
                }
                Err(e) => {
                    return Err(SshError::Failed {
                        what,
                        detail: format!("waiting: {e}"),
                    })
                }
            }
        };
        // A pipe that could not be read to its end is a failure whatever the
        // exit status: a truncated answer would be read as the whole one.
        let stdout = joined(out_reader).map_err(|detail| SshError::Failed {
            what: what.clone(),
            detail: format!("reading stdout: {detail}"),
        })?;
        let stderr = joined(err_reader).map_err(|detail| SshError::Failed {
            what: what.clone(),
            detail: format!("reading stderr: {detail}"),
        })?;
        Ok(Output {
            code: status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&stdout).into_owned(),
            stderr: String::from_utf8_lossy(&stderr).into_owned(),
        })
    }
}

/// Read a child's pipe to its end on a thread of its own, keeping the error.
fn drain<R: std::io::Read + Send + 'static>(
    mut pipe: R,
) -> std::thread::JoinHandle<std::io::Result<Vec<u8>>> {
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        pipe.read_to_end(&mut buf)?;
        Ok(buf)
    })
}

/// What a reader thread read, or why it could not — a panic in it counted
/// as one more way not to have read.
fn joined(reader: std::thread::JoinHandle<std::io::Result<Vec<u8>>>) -> Result<Vec<u8>, String> {
    reader
        .join()
        .map_err(|_| "the reader thread panicked".to_string())?
        .map_err(|e| e.to_string())
}
