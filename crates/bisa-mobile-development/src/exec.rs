//! The one way this crate starts a process: by absolute path, argv only, a
//! null stdin, a hardened environment, and a budget after which the child is
//! terminated. A program that wants a window of its own is started detached
//! and never waited on.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::{MobileDevelopmentError, MobileDevelopmentResult};

/// Wall-clock budgets: a local question (`simctl list`, `adb devices`), the
/// doctor's whole examination, and a boot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timeouts {
    pub local: Duration,
    pub doctor: Duration,
    pub boot: Duration,
}

impl Default for Timeouts {
    fn default() -> Self {
        Self {
            local: Duration::from_secs(15),
            doctor: Duration::from_secs(90),
            boot: Duration::from_secs(120),
        }
    }
}

/// A finished child. A non-zero exit is an answer here — `java -version`
/// writes to stderr and exits 0, `adb devices` exits 0 with nothing — so the
/// caller classifies.
#[derive(Clone, PartialEq, Eq)]
pub struct Output {
    pub code: i32,
    pub stdout: Vec<u8>,
    pub stderr: String,
}

impl Output {
    pub fn success(&self) -> bool {
        self.code == 0
    }

    /// stdout as text, lossily.
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }
}

impl std::fmt::Debug for Output {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Output")
            .field("code", &self.code)
            .field("stdout_bytes", &self.stdout.len())
            .field("stderr", &self.stderr)
            .finish()
    }
}

/// The environment every child gets, on top of the process's: no colour, no
/// pager, no analytics, one locale, so an answer reads the same on every
/// machine. Nothing is removed — `flutter` needs `HOME` and `PATH`, `adb`
/// needs `ANDROID_HOME` when it is set.
pub const HARDENED_ENV: [(&str, &str); 6] = [
    ("NO_COLOR", "1"),
    ("TERM", "dumb"),
    ("LC_ALL", "C"),
    ("PAGER", "cat"),
    ("FLUTTER_SUPPRESS_ANALYTICS", "true"),
    ("CI", "true"),
];

/// The real programs, by path.
#[derive(Debug, Clone, Default)]
pub struct Exec {
    pub timeouts: Timeouts,
}

impl Exec {
    /// Run one program under one budget and read everything it wrote.
    pub fn run(
        &self,
        program: &Path,
        args: &[String],
        env: &[(String, String)],
        timeout: Duration,
    ) -> MobileDevelopmentResult<Output> {
        let what = format!("{} {}", program.display(), args.join(" "));
        let mut cmd = Command::new(program);
        cmd.args(args)
            .envs(HARDENED_ENV.iter().map(|(k, v)| (*k, *v)))
            .envs(env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = cmd.spawn().map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => {
                MobileDevelopmentError::NotInstalled(program.display().to_string())
            }
            // LCOV_EXCL_START a spawn refused for another reason; an OS arm
            _ => MobileDevelopmentError::Failed {
                what: what.clone(),
                detail: format!("spawn: {e}"),
            },
            // LCOV_EXCL_STOP
        })?;
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
                        // The budget is spent: the child is terminated and
                        // reaped, and the caller reads a timeout.
                        // LCOV_EXCL_START a child already gone or reaped; by construction
                        if let Err(e) = child.kill() {
                            tracing::debug!("terminating a timed-out child: {e}");
                        }
                        if let Err(e) = child.wait() {
                            tracing::debug!("reaping a timed-out child: {e}");
                        }
                        // LCOV_EXCL_STOP
                        return Err(MobileDevelopmentError::Timeout {
                            what,
                            secs: timeout.as_secs(),
                        });
                    }
                    std::thread::sleep(nap.min(remaining));
                    nap = (nap * 2).min(Duration::from_millis(25));
                }
                // LCOV_EXCL_START waiting on a child this process spawned; by construction
                Err(e) => {
                    return Err(MobileDevelopmentError::Failed {
                        what,
                        detail: format!("waiting: {e}"),
                    })
                } // LCOV_EXCL_STOP
            }
        };
        // A pipe that could not be read to its end is a failure whatever the
        // exit status: a truncated frame or listing would be read as the whole.
        // LCOV_EXCL_START a reader thread that failed; by construction
        let stdout = joined(out_reader).map_err(|detail| MobileDevelopmentError::Failed {
            what: what.clone(),
            detail: format!("reading stdout: {detail}"),
        })?;
        let stderr = joined(err_reader).map_err(|detail| MobileDevelopmentError::Failed {
            what: what.clone(),
            detail: format!("reading stderr: {detail}"),
        })?;
        // LCOV_EXCL_STOP
        Ok(Output {
            code: status.code().unwrap_or(-1),
            stdout,
            stderr: String::from_utf8_lossy(&stderr).into_owned(),
        })
    }

    /// Start a program that keeps a window of its own — an emulator, the
    /// Simulator app — and leave it running: every stream null, never waited
    /// on. Answers the child's pid.
    pub fn spawn_detached(
        &self,
        program: &Path,
        args: &[String],
        env: &[(String, String)],
    ) -> MobileDevelopmentResult<u32> {
        let mut cmd = Command::new(program);
        cmd.args(args)
            .envs(HARDENED_ENV.iter().map(|(k, v)| (*k, *v)))
            .envs(env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let child = cmd.spawn().map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => {
                MobileDevelopmentError::NotInstalled(program.display().to_string())
            }
            // LCOV_EXCL_START a spawn refused for another reason; an OS arm
            _ => MobileDevelopmentError::Failed {
                what: format!("{} {}", program.display(), args.join(" ")),
                detail: format!("spawn: {e}"),
            },
            // LCOV_EXCL_STOP
        })?;
        Ok(child.id())
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
        .map_err(|_| "the reader thread panicked".to_string())? // LCOV_EXCL_LINE a reader thread that panicked; by construction
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_program_that_is_not_there_is_said_to_be_not_installed() {
        let exec = Exec::default();
        let missing = Path::new("/nonexistent/bisa-mobile-development-test/no-such-program");
        assert_eq!(
            exec.run(missing, &[], &[], Duration::from_secs(1)),
            Err(MobileDevelopmentError::NotInstalled(
                missing.display().to_string()
            ))
        );
        assert_eq!(
            exec.spawn_detached(missing, &[], &[]),
            Err(MobileDevelopmentError::NotInstalled(
                missing.display().to_string()
            ))
        );
    }

    /// The runner against programs every machine has, by absolute path: what a
    /// program wrote is read whole, its exit status is an answer, a child
    /// past its budget is ended and said to have timed out, and one started
    /// detached answers its pid.
    #[cfg(unix)]
    #[test]
    fn a_program_is_run_under_its_budget_and_read_whole_or_ended_past_it() {
        let exec = Exec::default();
        let sh = Path::new("/bin/sh");
        let args = |line: &str| vec!["-c".to_string(), line.to_string()];
        let out = exec
            .run(
                sh,
                &args("printf hello; printf oops >&2; exit 3"),
                &[],
                Duration::from_secs(5),
            )
            .unwrap();
        assert_eq!(
            (out.code, out.text().as_str(), out.stderr.as_str()),
            (3, "hello", "oops")
        );
        assert!(!out.success());
        let env = vec![("BISA_MOBILE_TEST_WORD".to_string(), "spoken".to_string())];
        let out = exec
            .run(
                sh,
                &args("printf \"$BISA_MOBILE_TEST_WORD $NO_COLOR\""),
                &env,
                Duration::from_secs(5),
            )
            .unwrap();
        assert_eq!(
            out.text(),
            "spoken 1",
            "the caller's environment and the hardened one"
        );
        let began = Instant::now();
        let late = exec.run(sh, &args("sleep 5"), &[], Duration::from_millis(300));
        assert!(
            matches!(late, Err(MobileDevelopmentError::Timeout { ref what, secs: 0 }) if what.contains("sleep 5")),
            "{late:?}"
        );
        assert!(
            began.elapsed() < Duration::from_secs(4),
            "ended, not waited out"
        );
        let pid = exec.spawn_detached(sh, &args("exit 0"), &[]).unwrap();
        assert!(pid > 0);
        let text_only = Output {
            code: 0,
            stdout: b"a line\n".to_vec(),
            stderr: String::new(),
        };
        assert_eq!(text_only.text(), "a line\n");
    }

    #[test]
    fn an_output_says_its_size_and_not_its_bytes() {
        let out = Output {
            code: 0,
            stdout: vec![0; 2048],
            stderr: String::new(),
        };
        let shown = format!("{out:?}");
        assert!(shown.contains("stdout_bytes: 2048"), "{shown}");
        assert!(out.success());
    }
}
