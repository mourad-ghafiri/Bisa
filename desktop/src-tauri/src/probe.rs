//! The one way the shell runs a program for its output: under a wall-clock
//! budget, ended past it — or, for the one probe whose late answer is still
//! worth keeping, left to finish in the background ([`run_or_defer`]).
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
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread::JoinHandle;
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

/// A program started with its stdout being drained on a thread.
struct Running {
    child: Child,
    reader: JoinHandle<std::io::Result<String>>,
}

impl Running {
    /// Start `program` with `args`, stdin closed and stderr dropped.
    fn start(program: &str, args: &[&str]) -> Option<Self> {
        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            // A program that writes to stderr is normal and none of our business.
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let Some(mut stdout) = child.stdout.take() else {
            // No pipe to read: ours to end, since nothing else can be waiting on it.
            end(&mut child);
            return None;
        };
        let reader = std::thread::spawn(move || {
            let mut text = String::new();
            stdout.read_to_string(&mut text).map(|_| text)
        });
        Some(Self { child, reader })
    }

    /// Wait until `deadline` for the program to finish: its output, or itself
    /// still running when the clock ran out.
    fn wait_until(mut self, deadline: Instant) -> Result<Option<Output>, Self> {
        loop {
            match self.child.try_wait() {
                Ok(Some(status)) => {
                    // A read that fails is no output — which `None` already means.
                    let stdout = self.reader.join().ok().and_then(Result::ok);
                    return Ok(stdout.map(|stdout| Output { status, stdout }));
                }
                Ok(None) if Instant::now() < deadline => std::thread::sleep(POLL),
                Ok(None) => return Err(self),
                // The child cannot be waited on: ended, and nothing answered.
                Err(_) => {
                    end(&mut self.child);
                    return Ok(None);
                }
            }
        }
    }

    /// End the program: ours to end, since this process started it a moment ago.
    fn end(mut self) {
        end(&mut self.child);
    }
}

fn end(child: &mut Child) {
    if let Err(e) = child.kill() {
        tracing::debug!("ending a probe past its budget: {e}");
    }
    if let Err(e) = child.wait() {
        tracing::debug!("reaping a probe: {e}");
    }
}

/// Run `program` with `args`, stdin closed and stderr dropped, and answer
/// its stdout — or `None` when it could not start or outstayed `budget`.
pub fn run(program: &str, args: &[&str], budget: Duration) -> Option<Output> {
    let running = Running::start(program, args)?;
    match running.wait_until(Instant::now() + budget) {
        Ok(output) => output,
        Err(running) => {
            running.end();
            None
        }
    }
}

/// How [`run_or_defer`] answered: within its budget, late, or never started.
#[derive(Debug)]
pub enum Deferred {
    /// The program finished within the budget.
    Answered(Output),
    /// It is still running: `late` hears its output — or nothing, past `cap`.
    Late,
    /// It could not be started; `late` is never called.
    Unstarted,
}

/// [`run`], except that a program late for its `budget` is not ended: it goes
/// on in the background up to `cap`, and `late` is handed what it answered —
/// or `None` when it outstayed the cap and was ended then. For the one probe
/// whose answer is worth keeping for next time even when it comes too late
/// for this time (`login_env`).
pub fn run_or_defer(
    program: &str,
    args: &[&str],
    budget: Duration,
    cap: Duration,
    late: impl FnOnce(Option<Output>) + Send + 'static,
) -> Deferred {
    let Some(running) = Running::start(program, args) else {
        return Deferred::Unstarted;
    };
    match running.wait_until(Instant::now() + budget) {
        Ok(Some(output)) => Deferred::Answered(output),
        Ok(None) => Deferred::Unstarted,
        Err(running) => {
            let finish = move || {
                let answer = match running.wait_until(Instant::now() + cap) {
                    Ok(output) => output,
                    Err(running) => {
                        running.end();
                        None
                    }
                };
                late(answer);
            };
            if let Err(e) = std::thread::Builder::new()
                .name("probe-late".into())
                .spawn(finish)
            {
                tracing::debug!("a late probe could not be followed: {e}");
            }
            Deferred::Late
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::sync::mpsc;

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
        assert!(matches!(
            run_or_defer(
                "/no/such/program",
                &[],
                Duration::from_secs(1),
                Duration::from_secs(1),
                |_| {}
            ),
            Deferred::Unstarted
        ));
    }

    /// A program in time is answered now; one that is late is left to finish
    /// and its answer handed over later; one that outstays the cap is ended
    /// and hands over nothing.
    #[test]
    fn a_late_program_is_followed_to_its_answer_and_ended_past_the_cap() {
        let now = run_or_defer(
            "/bin/sh",
            &["-c", "printf now"],
            Duration::from_secs(5),
            Duration::from_secs(5),
            |_| panic!("an answer within the budget is not late"),
        );
        assert!(
            matches!(now, Deferred::Answered(ref out) if out.stdout == "now"),
            "{now:?}"
        );

        let (tx, rx) = mpsc::channel();
        let started = Instant::now();
        let late = run_or_defer(
            "/bin/sh",
            &["-c", "sleep 0.3; printf later"],
            Duration::from_millis(50),
            Duration::from_secs(10),
            move |answer| tx.send(answer.and_then(Output::success)).unwrap(),
        );
        assert!(matches!(late, Deferred::Late), "{late:?}");
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "the budget held: {:?}",
            started.elapsed()
        );
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(10)).unwrap().as_deref(),
            Some("later")
        );

        let (tx, rx) = mpsc::channel();
        let capped = run_or_defer(
            "/bin/sh",
            &["-c", "sleep 30"],
            Duration::from_millis(50),
            Duration::from_millis(300),
            move |answer| tx.send(answer.is_some()).unwrap(),
        );
        assert!(matches!(capped, Deferred::Late));
        assert!(
            !rx.recv_timeout(Duration::from_secs(10)).unwrap(),
            "past the cap nothing is handed over"
        );
    }
}
