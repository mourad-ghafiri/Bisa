//! The one way this crate starts a process: by absolute path, argv only, a
//! null stdin, a hardened environment, a group of its own, and a budget
//! after which the child and everything it started are terminated. What it
//! writes is read up to a cap and never waited on past the budget. A program
//! that wants a window of its own is started detached, watched while a call
//! needs to know it lives, and reaped when it ends.

use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use crate::{MobileDevelopmentError, MobileDevelopmentResult};

/// Wall-clock budgets: a local question (`simctl list`, `adb devices`), the
/// doctor's whole examination, a boot, how long an emulator told to end may
/// take to go, how often a starting or stopping device is looked for, and
/// how long a finished child's pipes are read on when something it started
/// still holds them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timeouts {
    pub local: Duration,
    pub doctor: Duration,
    pub boot: Duration,
    pub shutdown: Duration,
    pub poll: Duration,
    pub settle: Duration,
}

impl Default for Timeouts {
    fn default() -> Self {
        Self {
            local: Duration::from_secs(15),
            doctor: Duration::from_secs(90),
            boot: Duration::from_secs(120),
            shutdown: Duration::from_secs(15),
            poll: Duration::from_secs(2),
            settle: Duration::from_secs(2),
        }
    }
}

/// The most that is kept of what a child writes: a screen's capture is the
/// largest answer any program here gives, and an error's words are short.
/// What is written past a cap is read and dropped, so the child never waits
/// on a full pipe, and the answer says it was clipped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub stdout: usize,
    pub stderr: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            stdout: 48 * 1024 * 1024,
            stderr: 64 * 1024,
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
    /// Whether the child wrote more than was kept ([`Limits`]).
    pub clipped: bool,
}

impl Output {
    pub fn success(&self) -> bool {
        self.code == 0
    }

    /// stdout as text, lossily.
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }

    /// What the program said went wrong, trimmed: its stderr, or its
    /// stdout when it wrote its error there.
    pub fn complaint(&self) -> String {
        let stderr = self.stderr.trim();
        if stderr.is_empty() {
            self.text().trim().to_string()
        } else {
            stderr.to_string()
        }
    }
}

impl std::fmt::Debug for Output {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Output")
            .field("code", &self.code)
            .field("stdout_bytes", &self.stdout.len())
            .field("stderr", &self.stderr)
            .field("clipped", &self.clipped)
            .finish()
    }
}

/// The environment every child gets, on top of the process's: no colour, no
/// pager, no analytics, one locale, so an answer reads the same on every
/// machine. Nothing is removed but the node's own token
/// ([`crate::SCRUBBED_ENV`]) — `flutter` needs `HOME` and `PATH`, `adb`
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
    pub limits: Limits,
}

/// A program started with a window of its own and left running. While the
/// handle is held, a call can ask whether it is still alive; when the
/// handle goes, the child is handed to a thread that waits for its end, so
/// it never lingers as a zombie of this process.
#[derive(Debug)]
pub struct Detached {
    child: Option<Child>,
}

impl Detached {
    pub fn pid(&self) -> u32 {
        self.child.as_ref().map_or(0, Child::id)
    }

    /// The exit status when the program has ended, `None` while it runs.
    pub fn exited(&mut self) -> Option<i32> {
        let child = self.child.as_mut()?;
        match child.try_wait() {
            Ok(Some(status)) => Some(status.code().unwrap_or(-1)),
            _ => None,
        }
    }
}

impl Drop for Detached {
    fn drop(&mut self) {
        let Some(mut child) = self.child.take() else {
            return; // LCOV_EXCL_LINE a handle always holds its child; the arm keeps the take total
        };
        if matches!(child.try_wait(), Ok(Some(_))) {
            return;
        }
        // Still running: its end is waited for elsewhere, however long.
        let reaper = std::thread::Builder::new().name("mobile-development-reaper".into());
        if let Err(e) = reaper.spawn(move || {
            child.wait().ok();
        }) {
            tracing::debug!("no thread to reap a detached child: {e}"); // LCOV_EXCL_LINE the OS refused a thread
        }
    }
}

impl Exec {
    fn command(program: &Path, args: &[String], env: &[(String, String)]) -> Command {
        let mut cmd = Command::new(program);
        for name in crate::SCRUBBED_ENV {
            cmd.env_remove(name);
        }
        cmd.args(args)
            .envs(HARDENED_ENV.iter().map(|(k, v)| (*k, *v)))
            .envs(env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
            .stdin(Stdio::null());
        // A group of its own: a budget that runs out ends what the program
        // started too, and a signal sent to the node's terminal does not
        // reach a device's window.
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
        }
        cmd
    }

    fn spawn(cmd: &mut Command, program: &Path, what: &str) -> MobileDevelopmentResult<Child> {
        cmd.spawn().map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => {
                MobileDevelopmentError::NotInstalled(program.display().to_string())
            }
            // A file that is there and cannot be run: no permission, not a
            // program.
            _ => MobileDevelopmentError::Failed {
                what: what.to_string(),
                detail: format!("spawn: {e}"),
            },
        })
    }

    /// Run one program under one budget and read what it wrote, up to the
    /// caps.
    pub fn run(
        &self,
        program: &Path,
        args: &[String],
        env: &[(String, String)],
        timeout: Duration,
    ) -> MobileDevelopmentResult<Output> {
        let what = format!("{} {}", program.display(), args.join(" "));
        let mut cmd = Self::command(program, args, env);
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = Self::spawn(&mut cmd, program, &what)?;
        let out_reader = Reader::start(
            child.stdout.take().expect("stdout was piped"),
            self.limits.stdout,
        );
        let err_reader = Reader::start(
            child.stderr.take().expect("stderr was piped"),
            self.limits.stderr,
        );
        let deadline = Instant::now() + timeout;
        let mut nap = Duration::from_millis(1);
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => {
                    let remaining = deadline.saturating_duration_since(Instant::now());
                    if remaining.is_zero() {
                        // The budget is spent: the child and what it
                        // started are terminated, the child reaped, and the
                        // caller reads a timeout.
                        end(&mut child);
                        return Err(MobileDevelopmentError::Timeout {
                            what,
                            secs: timeout.as_secs().max(1),
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
        // The child is gone; what it wrote is in its pipes. Something it
        // started may hold them open long after — a daemon, a helper — so
        // the reading stops after a moment, with what the child itself wrote.
        let (stdout, clipped) = out_reader
            .finish(self.timeouts.settle)
            .map_err(|detail| unread(&what, "stdout", &detail))?;
        let (stderr, _) = err_reader
            .finish(self.timeouts.settle)
            .map_err(|detail| unread(&what, "stderr", &detail))?;
        Ok(Output {
            code: status.code().unwrap_or(-1),
            stdout,
            stderr: String::from_utf8_lossy(&stderr).into_owned(),
            clipped,
        })
    }

    /// Start a program that keeps a window of its own — an emulator — and
    /// leave it running: every stream null, never waited on by the call.
    pub fn spawn_detached(
        &self,
        program: &Path,
        args: &[String],
        env: &[(String, String)],
    ) -> MobileDevelopmentResult<Detached> {
        let what = format!("{} {}", program.display(), args.join(" "));
        let mut cmd = Self::command(program, args, env);
        cmd.stdout(Stdio::null()).stderr(Stdio::null());
        let child = Self::spawn(&mut cmd, program, &what)?;
        Ok(Detached { child: Some(child) })
    }
}

// LCOV_EXCL_START a pipe of a child this process spawned failing to read; an OS arm
fn unread(what: &str, pipe: &str, detail: &str) -> MobileDevelopmentError {
    MobileDevelopmentError::Failed {
        what: what.to_string(),
        detail: format!("reading {pipe}: {detail}"),
    }
}
// LCOV_EXCL_STOP

/// End a child past its budget: its whole group first — what it started
/// would otherwise live on and hold its pipes — then the child itself, and
/// reap it. Each step may find its target already gone, which is the end
/// that was wanted.
fn end(child: &mut Child) {
    #[cfg(unix)]
    if let Some(group) = i32::try_from(child.id())
        .ok()
        .and_then(rustix::process::Pid::from_raw)
    {
        rustix::process::kill_process_group(group, rustix::process::Signal::KILL).ok();
    }
    child.kill().ok();
    child.wait().ok();
}

/// What a pipe has given so far.
#[derive(Default)]
struct ReadSoFar {
    bytes: Vec<u8>,
    clipped: bool,
    error: Option<String>,
}

/// One pipe of a child, read on a thread of its own up to a cap; what comes
/// past the cap is read and dropped.
struct Reader {
    so_far: Arc<Mutex<ReadSoFar>>,
    done: mpsc::Receiver<()>,
}

impl Reader {
    fn start<R: Read + Send + 'static>(mut pipe: R, limit: usize) -> Self {
        let so_far = Arc::new(Mutex::new(ReadSoFar::default()));
        let (tell, done) = mpsc::channel();
        let theirs = so_far.clone();
        std::thread::spawn(move || {
            let mut chunk = [0u8; 8192];
            loop {
                match pipe.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(n) => {
                        let mut kept = theirs.lock().unwrap_or_else(|e| e.into_inner());
                        let room = limit.saturating_sub(kept.bytes.len());
                        kept.clipped |= n > room;
                        kept.bytes.extend_from_slice(&chunk[..n.min(room)]);
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue, // LCOV_EXCL_LINE a read interrupted by a signal; an OS arm
                    // LCOV_EXCL_START a pipe of a child this process spawned failing to read; an OS arm
                    Err(e) => {
                        theirs.lock().unwrap_or_else(|p| p.into_inner()).error =
                            Some(e.to_string());
                        break;
                    } // LCOV_EXCL_STOP
                }
            }
            tell.send(()).ok();
        });
        Self { so_far, done }
    }

    /// What was read, once the pipe closed or the moment allowed passed —
    /// whichever comes first.
    fn finish(self, grace: Duration) -> Result<(Vec<u8>, bool), String> {
        // Closed, timed out, or a reader that died: what stands is the answer.
        self.done.recv_timeout(grace).ok();
        let mut kept = self.so_far.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(e) = kept.error.take() {
            return Err(e); // LCOV_EXCL_LINE the read error above; an OS arm
        }
        Ok((std::mem::take(&mut kept.bytes), kept.clipped))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sh(line: &str) -> Vec<String> {
        vec!["-c".to_string(), line.to_string()]
    }

    /// A runner with a short moment for pipes somebody else still holds.
    fn quick() -> Exec {
        Exec {
            timeouts: Timeouts {
                settle: Duration::from_millis(300),
                ..Timeouts::default()
            },
            limits: Limits::default(),
        }
    }

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
        assert!(matches!(
            exec.spawn_detached(missing, &[], &[]),
            Err(MobileDevelopmentError::NotInstalled(ref what)) if *what == missing.display().to_string()
        ));
    }

    /// A file that is there and is no program — no permission to run it, a
    /// folder — is a failure that names the call, not "not installed".
    #[cfg(unix)]
    #[test]
    fn a_file_that_cannot_be_run_is_a_failure_in_the_systems_words() {
        let dir = tempfile::tempdir().unwrap();
        let plain = dir.path().join("not-a-program");
        std::fs::write(&plain, "#!/bin/sh\nexit 0\n").unwrap();
        let exec = Exec::default();
        for refused in [plain.as_path(), dir.path()] {
            let ran = exec.run(refused, &["x".to_string()], &[], Duration::from_secs(1));
            assert!(
                matches!(ran, Err(MobileDevelopmentError::Failed { ref what, ref detail })
                    if what.ends_with(" x") && detail.starts_with("spawn: ")),
                "{ran:?}"
            );
            let started = exec.spawn_detached(refused, &[], &[]);
            assert!(
                matches!(started, Err(MobileDevelopmentError::Failed { ref detail, .. }) if detail.starts_with("spawn: ")),
                "{started:?}"
            );
        }
    }

    /// The runner against programs every machine has, by absolute path: what a
    /// program wrote is read whole, its exit status is an answer, and a child
    /// past its budget is ended and said to have timed out.
    #[cfg(unix)]
    #[test]
    fn a_program_is_run_under_its_budget_and_read_whole_or_ended_past_it() {
        let exec = Exec::default();
        let shell = Path::new("/bin/sh");
        let out = exec
            .run(
                shell,
                &sh("printf hello; printf oops >&2; exit 3"),
                &[],
                Duration::from_secs(5),
            )
            .unwrap();
        assert_eq!(
            (
                out.code,
                out.text().as_str(),
                out.stderr.as_str(),
                out.clipped
            ),
            (3, "hello", "oops", false)
        );
        assert!(!out.success());
        let env = vec![("BISA_TEST_WORD".to_string(), "spoken".to_string())];
        let out = exec
            .run(
                shell,
                &sh("printf \"$BISA_TEST_WORD $NO_COLOR\""),
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
        let late = exec.run(shell, &sh("sleep 5"), &[], Duration::from_millis(300));
        assert!(
            matches!(late, Err(MobileDevelopmentError::Timeout { ref what, secs: 1 }) if what.contains("sleep 5")),
            "a budget under a second is said as one: {late:?}"
        );
        assert!(
            began.elapsed() < Duration::from_secs(4),
            "ended, not waited out"
        );
        let text_only = Output {
            code: 0,
            stdout: b"a line\n".to_vec(),
            stderr: String::new(),
            clipped: false,
        };
        assert_eq!(text_only.text(), "a line\n");
    }

    /// Whether a process is still there: signal 0 reaches it or does not.
    #[cfg(unix)]
    fn lives(pid: i32) -> bool {
        rustix::process::Pid::from_raw(pid)
            .is_some_and(|pid| rustix::process::test_kill_process(pid).is_ok())
    }

    /// A budget that runs out ends what the program started, not the
    /// program alone: nothing it left behind holds a pipe or lives on.
    #[cfg(unix)]
    #[test]
    fn a_budget_that_runs_out_ends_what_the_child_started_too() {
        let dir = tempfile::tempdir().unwrap();
        let pid_file = dir.path().join("grandchild.pid");
        let line = format!("sleep 30 & echo $! > '{}'; wait", pid_file.display());
        let began = Instant::now();
        let late = quick().run(
            Path::new("/bin/sh"),
            &sh(&line),
            &[],
            Duration::from_millis(400),
        );
        assert!(
            matches!(late, Err(MobileDevelopmentError::Timeout { .. })),
            "{late:?}"
        );
        assert!(began.elapsed() < Duration::from_secs(5), "not waited out");
        let grandchild: i32 = std::fs::read_to_string(&pid_file)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while lives(grandchild) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(
            !lives(grandchild),
            "the sleep the shell started was ended with it"
        );
    }

    /// A program that ends while something it started still holds its
    /// pipes — a daemon, a helper — is answered with what it wrote, after a
    /// moment: its end is never waited on through somebody else's life.
    #[cfg(unix)]
    #[test]
    fn a_finished_childs_pipes_are_read_for_a_moment_not_for_a_helpers_life() {
        let began = Instant::now();
        let out = quick()
            .run(
                Path::new("/bin/sh"),
                &sh("sleep 4 & printf said; printf told >&2"),
                &[],
                Duration::from_secs(20),
            )
            .unwrap();
        assert_eq!(
            (out.code, out.text().as_str(), out.stderr.as_str()),
            (0, "said", "told")
        );
        assert!(
            began.elapsed() < Duration::from_secs(3),
            "answered in {:?}, not after the helper's four seconds",
            began.elapsed()
        );
    }

    /// What a child writes past the cap is read and dropped: the answer is
    /// the cap's worth and says it was clipped, and a child that writes far
    /// more than a pipe holds still ends.
    #[cfg(unix)]
    #[test]
    fn what_is_written_past_the_cap_is_dropped_and_said() {
        let small = Exec {
            timeouts: Timeouts::default(),
            limits: Limits {
                stdout: 10,
                stderr: 4,
            },
        };
        let out = small
            .run(
                Path::new("/bin/sh"),
                &sh("printf abcdefghijklmnop; printf oops-and-more >&2"),
                &[],
                Duration::from_secs(5),
            )
            .unwrap();
        assert_eq!(
            (out.text().as_str(), out.stderr.as_str(), out.clipped),
            ("abcdefghij", "oops", true)
        );
        let flood = small
            .run(
                Path::new("/bin/sh"),
                &sh("head -c 300000 /dev/zero"),
                &[],
                Duration::from_secs(10),
            )
            .unwrap();
        assert_eq!(
            (flood.code, flood.stdout.len(), flood.clipped),
            (0, 10, true)
        );
        let within = small
            .run(
                Path::new("/bin/sh"),
                &sh("printf 0123456789"),
                &[],
                Duration::from_secs(5),
            )
            .unwrap();
        assert!(!within.clipped, "exactly the cap is the whole answer");
    }

    /// A detached program answers its pid, says when it has ended and with
    /// what, and one still running when its handle goes is left running.
    #[cfg(unix)]
    #[test]
    fn a_detached_program_is_watched_while_held_and_left_running_after() {
        let exec = Exec::default();
        let mut ended = exec
            .spawn_detached(Path::new("/bin/sh"), &sh("exit 3"), &[])
            .unwrap();
        assert!(ended.pid() > 0);
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut code = None;
        while code.is_none() && Instant::now() < deadline {
            code = ended.exited();
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(code, Some(3));
        drop(ended);
        let mut running = exec
            .spawn_detached(Path::new("/bin/sh"), &sh("sleep 2"), &[])
            .unwrap();
        let pid = i32::try_from(running.pid()).unwrap();
        assert_eq!(running.exited(), None, "still running");
        drop(running);
        assert!(lives(pid), "dropping the handle ends nothing");
    }

    #[test]
    fn an_output_says_its_size_and_not_its_bytes_and_complains_in_the_programs_words() {
        let out = Output {
            code: 0,
            stdout: vec![0; 2048],
            stderr: String::new(),
            clipped: true,
        };
        let shown = format!("{out:?}");
        assert!(shown.contains("stdout_bytes: 2048"), "{shown}");
        assert!(shown.contains("clipped: true"), "{shown}");
        assert!(out.success());
        let on_stderr = Output {
            code: 1,
            stdout: b"ignored\n".to_vec(),
            stderr: "  error: device offline\n".into(),
            clipped: false,
        };
        assert_eq!(on_stderr.complaint(), "error: device offline");
        let on_stdout = Output {
            code: 1,
            stdout: b"KO: unknown command\n".to_vec(),
            stderr: " \n".into(),
            clipped: false,
        };
        assert_eq!(on_stdout.complaint(), "KO: unknown command");
    }
}
