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
            _ => MobileDevelopmentError::Failed {
                what: what.clone(),
                detail: format!("spawn: {e}"),
            },
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
                        if let Err(e) = child.kill() {
                            tracing::debug!("terminating a timed-out child: {e}");
                        }
                        if let Err(e) = child.wait() {
                            tracing::debug!("reaping a timed-out child: {e}");
                        }
                        return Err(MobileDevelopmentError::Timeout {
                            what,
                            secs: timeout.as_secs(),
                        });
                    }
                    std::thread::sleep(nap.min(remaining));
                    nap = (nap * 2).min(Duration::from_millis(25));
                }
                Err(e) => {
                    return Err(MobileDevelopmentError::Failed {
                        what,
                        detail: format!("waiting: {e}"),
                    })
                }
            }
        };
        // A pipe that could not be read to its end is a failure whatever the
        // exit status: a truncated frame or listing would be read as the whole.
        let stdout = joined(out_reader).map_err(|detail| MobileDevelopmentError::Failed {
            what: what.clone(),
            detail: format!("reading stdout: {detail}"),
        })?;
        let stderr = joined(err_reader).map_err(|detail| MobileDevelopmentError::Failed {
            what: what.clone(),
            detail: format!("reading stderr: {detail}"),
        })?;
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
            _ => MobileDevelopmentError::Failed {
                what: format!("{} {}", program.display(), args.join(" ")),
                detail: format!("spawn: {e}"),
            },
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
        .map_err(|_| "the reader thread panicked".to_string())?
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
