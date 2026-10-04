//! The one way this crate starts a process.
//!
//! Everything here exists because the caller is an unattended orchestrator: a
//! subprocess that asks a question, or never answers one, is a stalled work
//! item. So there is exactly one runner, it always pipes stdio (never inherits
//! a terminal a prompt could appear on), and it always has a deadline.

use std::ffi::{OsStr, OsString};
use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::{VcsError, VcsResult};

/// Wall-clock budgets for a child process.
///
/// Two numbers rather than one because the honest expectations differ by two
/// orders of magnitude: a local `git status` that takes 30s is wedged, while a
/// `git push` over a slow link legitimately takes a minute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timeouts {
    /// Budget for purely local operations. Default 30s.
    pub local: Duration,
    /// Budget for anything that may touch a remote. Default 120s.
    pub network: Duration,
}

impl Default for Timeouts {
    fn default() -> Self {
        Self {
            local: Duration::from_secs(30),
            network: Duration::from_secs(120),
        }
    }
}

impl Timeouts {
    pub(crate) const fn pick(&self, network: bool) -> Duration {
        if network {
            self.network
        } else {
            self.local
        }
    }
}

/// A finished child. Unlike [`std::process::Output`] a non-zero exit is not an
/// error here: several callers (`is_repo`, `pr_view`) treat a specific failure
/// as a legitimate negative answer, so classification is the caller's job.
#[derive(Debug)]
pub(crate) struct Output {
    pub success: bool,
    pub code: String,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

impl Output {
    /// stderr as trimmed lossy text, for error messages and classification.
    pub(crate) fn stderr_text(&self) -> String {
        String::from_utf8_lossy(&self.stderr).trim().to_string()
    }

    pub(crate) fn stdout_text(&self) -> String {
        String::from_utf8_lossy(&self.stdout).trim().to_string()
    }
}

/// Convenience for building argv vectors without a `.into()` at every element.
pub(crate) fn s(x: impl AsRef<OsStr>) -> OsString {
    x.as_ref().to_os_string()
}

/// The arguments an invocation is described by before the rest are counted.
/// A staging of sixty files is named by its verb and its first paths, so the
/// reason git gave — which an error says after the command — is still read.
const DESCRIBED_ARGS: usize = 10;

/// Render an argv for error messages. Lossy on purpose: this string is for a
/// human to read, never for a caller to parse. A long argv is cut after
/// [`DESCRIBED_ARGS`] arguments and the rest counted (`… (+50 more)`).
pub(crate) fn describe(bin: &OsStr, args: &[OsString]) -> String {
    let mut out = bin.to_string_lossy().into_owned();
    for a in args.iter().take(DESCRIBED_ARGS) {
        out.push(' ');
        out.push_str(&a.to_string_lossy());
    }
    if args.len() > DESCRIBED_ARGS {
        out.push_str(&format!(" … (+{} more)", args.len() - DESCRIBED_ARGS));
    }
    // An invocation is described in every error and timeout it becomes, and a
    // remote named `https://user:token@host/…` is one of its arguments.
    scrub_userinfo(&out)
}

/// `text` with the userinfo of every URL in it taken out: `scheme://user:secret@host`
/// reads `scheme://***@host`. What an error says travels — to the API's
/// answer, a toast, the journal, a log — and a token a person put in a remote's
/// URL must not travel with it. Git hides a password in some of its own
/// messages and not in others, so its output is scrubbed too, never trusted.
///
/// Only an authority is touched: from `://` to the next `/`, whitespace or
/// quote, and only when an `@` sits inside it. An `scp`-style remote
/// (`git@host:path`) has no secret in it and no `://`, and is left as it is.
pub fn scrub_userinfo(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("://") {
        let (head, tail) = rest.split_at(at + 3);
        out.push_str(head);
        let end = tail
            .find(|c: char| {
                c == '/' || c == '?' || c == '#' || c.is_whitespace() || c == '\'' || c == '"'
            })
            .unwrap_or(tail.len());
        let authority = &tail[..end];
        match authority.rfind('@') {
            Some(host_at) => {
                out.push_str("***");
                out.push_str(&authority[host_at..]);
            }
            None => out.push_str(authority),
        }
        rest = &tail[end..];
    }
    out.push_str(rest);
    out
}

/// Run `cmd` to completion, or kill it when `timeout` expires.
///
/// stdout and stderr are drained by their own threads so a child that fills a
/// 64 KiB pipe buffer cannot deadlock against a parent that is waiting for it
/// to exit. The wait itself polls rather than blocking, because the only way
/// to abandon a blocking `wait()` is to leak the thread doing it, and we want
/// the kill to be ours.
pub(crate) fn run(cmd: Command, what: &str, timeout: Duration) -> VcsResult<Output> {
    run_with_input(cmd, what, timeout, None)
}

/// [`run`], feeding `input` to the child's stdin when there is one.
///
/// Two callers need it: `git apply` — a patch is text the caller built, and
/// writing it to a temporary file first would put somebody's diff on disk for
/// no reason — and `git credential fill`, whose question is a few `key=value`
/// lines ended by a blank one and the EOF this writer's dropped handle gives.
/// stdin is `null` when there is no input, so a child that unexpectedly asks a
/// question still gets EOF, never a terminal.
pub(crate) fn run_with_input(
    mut cmd: Command,
    what: &str,
    timeout: Duration,
    input: Option<Vec<u8>>,
) -> VcsResult<Output> {
    cmd.stdin(if input.is_some() {
        Stdio::piped()
    } else {
        Stdio::null()
    })
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| VcsError::NotAvailable(format!("spawn `{what}`: {e}")))?;

    // Written from its own thread for the same reason the pipes are read
    // from theirs: a patch larger than the pipe buffer would otherwise block
    // here while the child blocks on a stdout nobody is draining. Dropping
    // the handle at the end of the closure is the EOF git waits for. The
    // write's result is judged after git's own, below.
    let stdin_writer = input.and_then(|bytes| {
        child.stdin.take().map(|mut stdin| {
            std::thread::spawn(move || {
                use std::io::Write as _;
                stdin.write_all(&bytes)
            })
        })
    });

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
                    // The child is ended and reaped; an end that fails is a
                    // child already gone, and the timeout is the answer either
                    // way.
                    let _already_gone = child.kill();
                    let _reaped = child.wait();
                    // The reader threads are left to finish on their own: the
                    // pipes close when the child dies, and joining a reader
                    // that a surviving grandchild still holds open would
                    // reintroduce exactly the hang we just prevented.
                    return Err(VcsError::Timeout {
                        what: what.to_string(),
                        secs: timeout.as_secs(),
                    });
                }
                std::thread::sleep(nap.min(remaining));
                nap = (nap * 2).min(Duration::from_millis(25));
            }
            Err(e) => return Err(VcsError::other(format!("waiting for `{what}`: {e}"))),
        }
    };

    // A pipe that could not be read to its end is a failure whatever the exit
    // status: the caller classifies stderr, and a truncated stderr would
    // misclassify.
    let stdout = joined(out_reader)
        .map_err(|e| VcsError::other(format!("reading `{what}`'s stdout: {e}")))?;
    let stderr = joined(err_reader)
        .map_err(|e| VcsError::other(format!("reading `{what}`'s stderr: {e}")))?;
    // git's own verdict first: a write the child stopped reading (EPIPE) is
    // git's failure to report, in git's words; only when git succeeded does a
    // short write mean the input never arrived whole.
    if let Some(writer) = stdin_writer {
        let fed = writer.join().map_err(|_| {
            VcsError::other(format!("feeding `{what}`: the writer thread panicked"))
        })?;
        if status.success() {
            fed.map_err(|e| VcsError::other(format!("feeding `{what}`: {e}")))?;
        }
    }
    Ok(Output {
        success: status.success(),
        code: status
            .code()
            .map(|c| c.to_string())
            .unwrap_or_else(|| "signal".into()),
        stdout,
        stderr,
    })
}

/// Read a child's pipe to its end on a thread of its own, keeping the error.
fn drain<R: Read + Send + 'static>(
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_remotes_credentials_never_ride_in_what_an_error_says() {
        for (given, said) in [
            (
                "git clone https://mona:ghp_0123456789abcdef@code.example.test/acme/shop.git /tmp/x",
                "git clone https://***@code.example.test/acme/shop.git /tmp/x",
            ),
            (
                "fatal: unable to access 'https://x-access-token:s3cr3t@code.example.test/a/b.git/': The requested URL returned error: 403",
                "fatal: unable to access 'https://***@code.example.test/a/b.git/': The requested URL returned error: 403",
            ),
            ("https://token-only@host.test/r.git", "https://***@host.test/r.git"),
            ("ssh://git@host.test:2222/r.git", "ssh://***@host.test:2222/r.git"),
            (
                "two: http://a:b@one.test/x and https://c:d@two.test/y?z=1",
                "two: http://***@one.test/x and https://***@two.test/y?z=1",
            ),
            // A password with an `@` in it: everything before the last one goes.
            ("https://user:p@ss@host.test/r.git", "https://***@host.test/r.git"),
        ] {
            assert_eq!(scrub_userinfo(given), said);
            assert!(!scrub_userinfo(given).contains("s3cr3t") && !scrub_userinfo(given).contains("ghp_"));
        }
        // Nothing to hide is left exactly as it is.
        for plain in [
            "git@code.example.test:acme/shop.git",
            "https://code.example.test/acme/shop.git",
            "error: pathspec 'a@b.rs' did not match any file(s) known to git",
            "mailto someone@example.test about https://host.test/path@v2",
            "file:///Users/someone/repos/shop.git",
            "",
        ] {
            assert_eq!(scrub_userinfo(plain), plain);
        }
        let described = describe(
            OsStr::new("git"),
            &[
                s("remote"),
                s("add"),
                s("origin"),
                s("https://mona:hunter2@host.test/r.git"),
            ],
        );
        assert_eq!(
            described,
            "git remote add origin https://***@host.test/r.git"
        );
    }

    /// A staging of sixty files is described by its first ten arguments and a
    /// count, so the reason git gives after it is read; a short one is whole,
    /// and a credential is scrubbed in either.
    #[test]
    fn a_long_argv_is_described_by_its_head_and_a_count() {
        let mut args = vec![s("add"), s("--")];
        args.extend((0..60).map(|i| s(format!(":(top,literal)lib/f{i}.dart"))));
        let described = describe(OsStr::new("git"), &args);
        assert_eq!(
            described,
            "git add -- :(top,literal)lib/f0.dart :(top,literal)lib/f1.dart :(top,literal)lib/f2.dart \
             :(top,literal)lib/f3.dart :(top,literal)lib/f4.dart :(top,literal)lib/f5.dart \
             :(top,literal)lib/f6.dart :(top,literal)lib/f7.dart … (+52 more)"
        );
        let ten: Vec<OsString> = (0..10).map(|i| s(format!("a{i}"))).collect();
        assert_eq!(
            describe(OsStr::new("git"), &ten),
            "git a0 a1 a2 a3 a4 a5 a6 a7 a8 a9",
            "ten arguments are described whole"
        );
        let mut long_push = vec![s("push"), s("https://mona:hunter2@host.test/r.git")];
        long_push.extend((0..12).map(|i| s(format!("refs/heads/b{i}"))));
        let pushed = describe(OsStr::new("git"), &long_push);
        assert!(
            pushed.starts_with("git push https://***@host.test/r.git refs/heads/b0"),
            "{pushed}"
        );
        assert!(
            pushed.ends_with("… (+4 more)") && !pushed.contains("hunter2"),
            "{pushed}"
        );
    }

    #[test]
    fn captures_stdout_and_exit_code() {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "printf hi; exit 3"]);
        let out = run(cmd, "sh", Duration::from_secs(10)).expect("ran");
        assert!(!out.success);
        assert_eq!(out.code, "3");
        assert_eq!(out.stdout_text(), "hi");
    }

    #[test]
    fn feeds_stdin_when_given_input() {
        let mut cmd = Command::new("cat");
        cmd.arg("-");
        let out = run_with_input(
            cmd,
            "cat",
            Duration::from_secs(10),
            Some(b"patch\n".to_vec()),
        )
        .expect("ran");
        assert!(out.success);
        assert_eq!(out.stdout_text(), "patch");
    }

    #[test]
    fn a_child_that_stops_reading_its_stdin_is_judged_by_its_own_exit() {
        // A megabyte the child never reads: the writer's EPIPE is not the
        // story — the exit code is, and git's words with it.
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "exit 3"]);
        let out = run_with_input(
            cmd,
            "sh",
            Duration::from_secs(10),
            Some(vec![b'x'; 1 << 20]),
        )
        .expect("the child's own verdict, never the write's");
        assert!(!out.success);
        assert_eq!(out.code, "3");
    }

    #[test]
    fn kills_a_child_that_outlives_its_budget() {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "sleep 30"]);
        let started = Instant::now();
        let err = run(cmd, "sleep", Duration::from_millis(150)).expect_err("must time out");
        assert!(matches!(err, VcsError::Timeout { .. }), "got {err}");
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "killed promptly"
        );
    }

    #[test]
    fn drains_more_than_a_pipe_buffer() {
        // 1 MiB through a 64 KiB pipe: this deadlocks a naive wait-then-read.
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "yes abcdefghij | head -c 1048576"]);
        let out = run(cmd, "yes", Duration::from_secs(20)).expect("ran");
        assert_eq!(out.stdout.len(), 1_048_576);
    }

    #[test]
    fn missing_binary_is_not_available() {
        let cmd = Command::new("bisa-vcs-no-such-binary");
        let err = run(cmd, "nope", Duration::from_secs(5)).expect_err("cannot spawn");
        assert!(err.is_unavailable(), "got {err}");
    }
}
