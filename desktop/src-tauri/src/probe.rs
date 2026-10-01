//! The one way the shell runs a program for its output: under a wall-clock
//! budget, ended past it.
//!
//! `std::process` has no timed wait, so this is a poll rather than a block:
//! stdout is drained on its own thread — a child that fills the pipe would
//! otherwise never exit — and `try_wait` is asked every few milliseconds
//! until the budget runs out, when the child is ended and nothing is
//! answered. A reader that hung would otherwise pin whatever waits on it: the
//! footer's in-flight mark, the window before it is shown, a settings panel.
//!
//! Every shell-out in the shell goes through here — the login shell for its
//! `PATH`, `ioreg` for the accelerator's load, the network readers — so the
//! budget rule is stated once.

use std::io::Read as _;
use std::process::{Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

/// How often to ask whether the child has finished.
const POLL: Duration = Duration::from_millis(20);

/// What a program answered within its budget.
#[derive(Debug)]
pub struct Output {
    pub status: ExitStatus,
    pub stdout: String,
}

impl Output {
    /// The output when the program succeeded, else nothing.
    pub fn success(self) -> Option<String> {
        self.status.success().then_some(self.stdout)
    }
}

/// Run `program` with `args`, stdin closed and stderr dropped, and answer
/// its stdout — or `None` when it could not start or outstayed `budget`.
pub fn run(program: &str, args: &[&str], budget: Duration) -> Option<Output> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        // A program that writes to stderr is normal and none of our business.
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    // A read that fails is no output — which `None` already means below.
    let reader = std::thread::spawn(move || {
        let mut text = String::new();
        stdout.read_to_string(&mut text).map(|_| text)
    });
    let deadline = Instant::now() + budget;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(POLL),
            // Ours to end: this process started it a moment ago and nothing
            // else can be waiting on it.
            _ => {
                if let Err(e) = child.kill() {
                    tracing::debug!("ending a probe past its budget: {e}");
                }
                if let Err(e) = child.wait() {
                    tracing::debug!("reaping a probe: {e}");
                }
                return None;
            }
        }
    };
    let stdout = reader.join().ok()?.ok()?;
    Some(Output { status, stdout })
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn a_program_answers_within_its_budget_and_a_failure_is_not_a_success() {
        let out = run("/bin/sh", &["-c", "printf hello"], Duration::from_secs(5)).unwrap();
        assert!(out.status.success());
        assert_eq!(out.stdout, "hello");
        let failed = run("/bin/sh", &["-c", "exit 3"], Duration::from_secs(5)).unwrap();
        assert_eq!(failed.success(), None);
    }

    #[test]
    fn a_program_past_its_budget_is_ended_and_answers_nothing() {
        let started = Instant::now();
        assert!(run("/bin/sh", &["-c", "sleep 30"], Duration::from_millis(200)).is_none());
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "{:?}",
            started.elapsed()
        );
    }

    #[test]
    fn a_program_that_does_not_exist_answers_nothing() {
        assert!(run("/no/such/program", &[], Duration::from_secs(1)).is_none());
    }
}
