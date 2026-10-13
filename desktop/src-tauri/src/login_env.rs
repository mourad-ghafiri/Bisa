//! The `PATH` a person actually has, for a process that was not started from a
//! terminal.
//!
//! # The problem this exists for
//!
//! An app launched from Finder, Spotlight or a `.desktop` entry inherits its
//! environment from the launcher, not from a shell. On macOS that is launchd's
//! `PATH` — `/usr/bin:/bin:/usr/sbin:/sbin` — and nothing else. None of
//! Homebrew, nvm, mise, asdf, pipx, cargo or a plain `~/.local/bin` is on it,
//! because every one of those is put there by a line in a shell profile that
//! nobody has run.
//!
//! The node inherits that environment from this process, and
//! `HarnessCatalog::probe` is `which::which(command)`. So a bundled app reports
//! **every harness as not found on PATH** while the same binaries work
//! perfectly in a terminal one window over — and the terminal panel's harness
//! menu, whose whole job is to offer what is installed, offers nothing; the
//! footer's usage line, which shows an installed harness's account, shows
//! nothing. Run the app with `npm run tauri dev` from a shell and the bug
//! vanishes, which is exactly the shape of a bug that ships.
//!
//! # What this does about it
//!
//! Once, at startup: ask the login shell what `PATH` it builds, and hand that
//! to the node. That is the same question the embedded terminal already answers
//! by construction — it spawns the login shell, so a shell in the panel has
//! always had the right `PATH` while the node deciding what to *offer* did not.
//!
//! **An interactive login shell**, `-l -i`, not a login shell alone. A tool's
//! installer puts its folder on the `PATH` by a line in the file the user's
//! *interactive* shell reads — `.zshrc`, `.bashrc` — and a login shell that
//! is not interactive never reads it: OpenCode's installer writes
//! `~/.opencode/bin` into `.zshrc`, and `zsh -l -c` answered without it while
//! `zsh -l -i -c` answered with it. An interactive profile prints more — a
//! prompt, a greeting — so the answer is read by a **marker** the command
//! prints in front of the `PATH`, never by position. A shell whose
//! interactive form says nothing usable is asked once more the old way,
//! `-l -c`, before the memo is read.
//!
//! **And the folders the installers use.** Whatever the shell said, the
//! folders a tool's own installer puts a binary in — `~/.opencode/bin`,
//! `~/.local/bin`, `~/.cargo/bin`, `~/.bun/bin`, Homebrew's and
//! `/usr/local/bin` — are appended when they exist and are not there yet:
//! appended, so the shell's order wins; only when they exist, so nothing
//! is invented. That covers a shell that keeps its `PATH` elsewhere, an
//! install made with `--no-modify-path`, and a memo written before the
//! install.
//!
//! **And remember the answer.** A profile that sources nvm, rbenv and
//! compinit takes longer on a cold first launch than the window may wait, and
//! the node's `PATH` is decided once, at its spawn. So the `PATH` the shell
//! built last time is kept in the app's own config folder (`remember_in`),
//! and a shell that is late for its budget is answered from it — while the
//! shell goes on in the background and refreshes the memo for the next
//! launch (`probe::run_or_defer`). A miss is a line in the log, never silent.
//! The one answer is read once per process, however many callers ask.
//!
//! # What it deliberately does not do
//!
//! - **It does not touch this process's own environment.** `set_var` is
//!   process-global and unsound to call once threads exist; the value is passed
//!   to the one child that needs it.
//! - **It never fails the app.** Every failure — no `$SHELL`, a shell that is
//!   not there, a profile that hangs, output that does not look like a `PATH` —
//!   returns `None` and leaves the inherited environment alone. A missing
//!   harness menu is a bad afternoon; a desktop app that will not start is a
//!   worse one.
//! - **It runs the user's profile**, which is a real consequence and worth
//!   stating plainly. So does every terminal this app opens, and so does every
//!   terminal they open themselves. It is `printf`, in a login shell, with a
//!   deadline.

use crate::probe::{Deferred, Output};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

/// How long the login shell gets before the node is spawned without waiting
/// for it. A profile that sources nvm and rbenv can take a noticeable fraction
/// of a second warm and a few seconds cold; one that blocks on a network mount
/// can take forever, and this runs before the window is shown.
const DEADLINE: Duration = Duration::from_secs(4);
/// How long a late shell may go on in the background before it is ended: its
/// answer is kept for the next launch, not for this one.
const LATE_CAP: Duration = Duration::from_secs(30);
/// The file the last good `PATH` is kept in, under the folder `remember_in` names.
const MEMO_FILE: &str = "login_path";
/// The word the shell is asked to print in front of its `PATH`, so the answer
/// is read by what it begins with and never by where it landed among the
/// profile's own lines.
const MARKER: &str = "__BISA_LOGIN_PATH__";
/// What the shell runs: a newline first, so the marker starts a line of its
/// own even after a profile that printed without one; a newline last, so a
/// prompt printed after it does not join it.
const PRINT_PATH: &str = r#"printf '\n%s%s\n' "__BISA_LOGIN_PATH__" "$PATH""#;

/// Where the last good `PATH` is kept — set once, before the first ask.
static MEMO_DIR: OnceLock<Option<PathBuf>> = OnceLock::new();
/// The one answer per process.
static ANSWER: OnceLock<Option<String>> = OnceLock::new();

/// Where to keep the `PATH` the login shell built, for the launch after this
/// one — the app's own config folder. Called once, before the first
/// [`login_path`]; `None` keeps nothing and asks the shell alone.
pub fn remember_in(dir: Option<PathBuf>) {
    if MEMO_DIR.set(dir).is_err() {
        tracing::debug!(target: "bisa_desktop", "the login PATH's folder was named twice; the first stands");
    }
}

/// The `PATH` the user's login shell builds — this time's answer, or the
/// remembered one when the shell is late — or `None` to keep the inherited
/// one. Asked once per process; every caller reads the same answer.
///
/// `None` on Windows, where there is no login-shell convention and the
/// registry environment a GUI process inherits is already the user's.
pub fn login_path() -> Option<String> {
    ANSWER
        .get_or_init(|| {
            if cfg!(not(unix)) {
                return None;
            }
            let shell = std::env::var("SHELL").ok()?;
            let memo = MEMO_DIR
                .get()
                .cloned()
                .flatten()
                .map(|dir| dir.join(MEMO_FILE));
            login_path_within(
                &shell,
                DEADLINE,
                LATE_CAP,
                memo.as_deref(),
                &known_tool_dirs(),
            )
        })
        .clone()
}

/// The folders a tool's own installer puts a binary in, each appended to the
/// shell's answer when it exists and is not there yet — a tool is found where
/// it was installed, whatever the profile says. Appended, never prepended.
fn known_tool_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(home) = dirs::home_dir() {
        for under_home in [".opencode/bin", ".local/bin", ".cargo/bin", ".bun/bin"] {
            dirs.push(home.join(under_home));
        }
    }
    dirs.push(PathBuf::from("/opt/homebrew/bin"));
    dirs.push(PathBuf::from("/usr/local/bin"));
    dirs
}

/// `path` with every folder of `known` that exists on disk and is not an
/// entry of it already appended once, in the order given.
fn with_known_dirs(path: String, known: &[PathBuf]) -> String {
    let mut out = path;
    for dir in known {
        if !dir.is_dir() {
            continue;
        }
        let entry = dir.to_string_lossy();
        if out.split(':').any(|e| e == entry) {
            continue;
        }
        if !out.is_empty() {
            out.push(':');
        }
        out.push_str(&entry);
    }
    out
}

/// With no shell's answer and no memo: the inherited `PATH` with the known
/// folders appended — or nothing, when none of them adds anything, so the
/// inherited environment is left exactly as it was.
fn inherited_with_known(known: &[PathBuf]) -> Option<String> {
    let inherited = std::env::var("PATH").ok()?;
    let extended = with_known_dirs(inherited.clone(), known);
    (extended != inherited).then_some(extended)
}

/// [`login_path`] for a named shell: what `shell -l -i` says its `PATH` is
/// within `deadline` — or, when that says nothing usable, what `shell -l`
/// says; else what `memo` remembers from last time, the shell going on up to
/// `cap` to refresh it; else the inherited `PATH` when a known folder adds to
/// it; else `None`. Whatever answered, the known folders are appended. The
/// shell, the clocks, the memo and the folders are parameters so a test can
/// hand over a script of its own instead of the developer's login shell,
/// folders of its own instead of this machine's, and a budget that a loaded
/// machine cannot turn into a flake.
fn login_path_within(
    shell: &str,
    deadline: Duration,
    cap: Duration,
    memo: Option<&Path>,
    known: &[PathBuf],
) -> Option<String> {
    if shell.trim().is_empty() {
        return None;
    }
    let remembered = memo.and_then(read_memo);
    let refresh = memo.map(Path::to_path_buf);
    let asked = crate::probe::run_or_defer(
        shell,
        &["-l", "-i", "-c", PRINT_PATH],
        deadline,
        cap,
        move |answer| match answer
            .and_then(Output::success)
            .as_deref()
            .and_then(sanitize)
        {
            Some(path) => {
                if let Some(file) = refresh {
                    write_memo(&file, &path);
                }
                tracing::info!(target: "bisa_desktop", "the login shell answered late; its PATH is kept for the next launch");
            }
            None => {
                tracing::warn!(target: "bisa_desktop", "the login shell never said its PATH; the next launch reads the one remembered, if any")
            }
        },
    );
    let answered = match asked {
        Deferred::Answered(out) => match out.success().as_deref().and_then(sanitize) {
            Some(path) => Some(path),
            None => {
                tracing::warn!(target: "bisa_desktop", "the interactive login shell answered nothing usable for a PATH; asking the login shell alone");
                login_shell_alone(shell, deadline)
            }
        },
        Deferred::Late => {
            tracing::warn!(
                target: "bisa_desktop",
                budget_s = deadline.as_secs(),
                remembered = remembered.is_some(),
                "the login shell outstayed its budget; the node starts with the PATH it built last time, or the inherited one"
            );
            None
        }
        Deferred::Unstarted => {
            tracing::warn!(target: "bisa_desktop", shell, remembered = remembered.is_some(), "the login shell could not be run");
            login_shell_alone(shell, deadline)
        }
    };
    match answered {
        Some(path) => {
            if let Some(file) = memo {
                write_memo(file, &path);
            }
            Some(with_known_dirs(path, known))
        }
        None => remembered
            .map(|path| with_known_dirs(path, known))
            .or_else(|| inherited_with_known(known)),
    }
}

/// The old question, asked once more when the interactive form says nothing
/// usable: `shell -l -c`, which a profile that misbehaves only when it is
/// interactive still answers. Within `budget`, or nothing.
fn login_shell_alone(shell: &str, budget: Duration) -> Option<String> {
    crate::probe::run(shell, &["-l", "-c", PRINT_PATH], budget)?
        .success()
        .as_deref()
        .and_then(sanitize)
}

/// The `PATH` remembered in `file`, when it still looks like one.
fn read_memo(file: &Path) -> Option<String> {
    sanitize(&std::fs::read_to_string(file).ok()?)
}

/// Keep `path` in `file` for the next launch: written beside and moved into
/// place, so a crash mid-write leaves the old memo, never half of a new one.
/// A folder that cannot be written costs nothing but a debug line.
fn write_memo(file: &Path, path: &str) {
    let attempt = (|| -> std::io::Result<()> {
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = file.with_extension("tmp");
        std::fs::write(&tmp, path)?;
        std::fs::rename(&tmp, file)
    })();
    if let Err(e) = attempt {
        tracing::debug!(target: "bisa_desktop", file = %file.display(), "the login PATH could not be remembered: {e}");
    }
}

/// The `PATH` out of whatever the shell printed, or `None` if it does not look
/// like one.
///
/// **The marked line when there is one, the last line otherwise.** A login
/// shell runs the user's profile, and profiles print things — a fortune, a
/// version banner, a warning from a version manager, and an interactive one
/// a prompt besides. The command prints the `PATH` behind [`MARKER`] on a
/// line of its own, so the line that begins with the marker is the answer
/// wherever it landed. A memo, and a shell that printed without the marker,
/// are read by the older rule: the last line. Taking all of stdout would hand
/// the node a `PATH` beginning with somebody's MOTD.
///
/// The shape check is deliberately weak — one absolute entry is enough. This is
/// guarding against "the profile printed something odd", not against a hostile
/// `PATH`; a `PATH` the user's own shell builds is exactly as trustworthy as
/// the shell, which is to say completely, because they are already running
/// their own commands in it.
fn sanitize(stdout: &str) -> Option<String> {
    let marked = stdout
        .lines()
        .rev()
        .find_map(|line| line.trim().strip_prefix(MARKER));
    let last = match marked {
        Some(path) => path.trim(),
        None => stdout.lines().next_back()?.trim(),
    };
    if last.is_empty() || last.contains('\0') {
        return None;
    }
    if !last.split(':').any(|entry| entry.starts_with('/')) {
        return None;
    }
    Some(last.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn a_profile_that_prints_a_banner_does_not_end_up_in_the_path() {
        let out = "Welcome back!\nnvm: using v20\n/opt/homebrew/bin:/usr/bin:/bin";
        assert_eq!(
            sanitize(out).as_deref(),
            Some("/opt/homebrew/bin:/usr/bin:/bin")
        );
    }

    #[test]
    fn a_plain_path_survives_unchanged() {
        assert_eq!(
            sanitize("/usr/local/bin:/usr/bin:/bin").as_deref(),
            Some("/usr/local/bin:/usr/bin:/bin")
        );
        // A trailing newline is not what `printf %s` emits, but a shell that
        // adds one must not cost the answer.
        assert_eq!(
            sanitize("/usr/bin:/bin\n").as_deref(),
            Some("/usr/bin:/bin")
        );
    }

    /// Anything that does not look like a `PATH` leaves the inherited one
    /// alone. Handing the node a broken `PATH` would be worse than the launchd
    /// default this exists to replace: it would break harnesses that currently
    /// work in a dev run.
    #[test]
    fn output_that_is_not_a_path_is_declined_rather_than_guessed_at() {
        assert_eq!(sanitize(""), None);
        assert_eq!(sanitize("\n\n"), None);
        assert_eq!(sanitize("   "), None);
        // Every entry relative: not a login shell's `PATH`, whatever it is.
        assert_eq!(sanitize("bin:usr/bin"), None);
        assert_eq!(sanitize("command not found: printf"), None);
    }

    /// A script standing in for a login shell, in a directory that dies with
    /// the test. It accepts `-l -i -c <command>` and `-l -c <command>` the
    /// way a shell does and prints what the test tells it to — so the whole
    /// path, spawn to sanitize, is exercised without running anybody's
    /// profile. `$shape` is `interactive` or `login`, for a body that answers
    /// each form differently.
    /// A budget no loaded machine reaches: these tests are about what the
    /// shell printed, not about the clock.
    const PATIENT: Duration = Duration::from_secs(30);

    #[cfg(unix)]
    fn stub_shell(dir: &std::path::Path, body: &str) -> String {
        use std::os::unix::fs::PermissionsExt as _;
        let path = dir.join("shell");
        let preamble = r#"shape=none; if [ "$1" = "-l" ] && [ "$2" = "-i" ] && [ "$3" = "-c" ]; then shape=interactive; elif [ "$1" = "-l" ] && [ "$2" = "-c" ]; then shape=login; fi"#;
        std::fs::write(&path, format!("#!/bin/sh\n{preamble}\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path.display().to_string()
    }

    /// The profile's chatter lands on earlier lines; the `PATH` is the last.
    #[cfg(unix)]
    #[test]
    fn a_shell_that_prints_a_banner_and_then_its_path_answers_the_path() {
        let dir = tempfile::tempdir().unwrap();
        let shell = stub_shell(
            dir.path(),
            r#"echo "Welcome back!"; [ "$shape" = interactive ] && printf %s "/opt/tools/bin:/usr/bin:/bin""#,
        );
        assert_eq!(
            login_path_within(&shell, PATIENT, PATIENT, None, &[]).as_deref(),
            Some("/opt/tools/bin:/usr/bin:/bin")
        );
    }

    /// The interactive form is the one asked, and its answer is the marked
    /// line — wherever the profile's own chatter put it, a prompt after it
    /// included.
    #[cfg(unix)]
    #[test]
    fn the_interactive_login_shell_is_asked_and_its_marked_line_is_the_path() {
        let dir = tempfile::tempdir().unwrap();
        let shell = stub_shell(
            dir.path(),
            r#"[ "$shape" = interactive ] || exit 3; printf 'Welcome back!'; eval "$4"; printf 'me@host %% '"#,
        );
        let path = login_path_within(&shell, PATIENT, PATIENT, None, &[]).unwrap();
        assert_eq!(path, std::env::var("PATH").unwrap(), "the shell's own PATH, by the marker");
    }

    /// A shell whose interactive form says nothing usable is asked once more
    /// the old way — a profile that only misbehaves when it is interactive
    /// still answers as a login shell.
    #[cfg(unix)]
    #[test]
    fn a_shell_whose_interactive_form_says_nothing_usable_is_asked_as_a_login_shell_alone() {
        let dir = tempfile::tempdir().unwrap();
        let shell = stub_shell(
            dir.path(),
            r#"if [ "$shape" = interactive ]; then echo "command not found: printf"; exit 0; fi; [ "$shape" = login ] && printf %s "/plain/bin:/usr/bin""#,
        );
        assert_eq!(
            login_path_within(&shell, PATIENT, PATIENT, None, &[]).as_deref(),
            Some("/plain/bin:/usr/bin")
        );
    }

    /// The folders installers use are appended once when they exist, never
    /// when they do not, and not again when the shell already named them;
    /// with nothing from the shell or the memo, the inherited `PATH` is
    /// extended by them — or left alone when they add nothing.
    #[cfg(unix)]
    #[test]
    fn known_tool_folders_that_exist_are_appended_once_and_missing_ones_never() {
        let dir = tempfile::tempdir().unwrap();
        let tools = dir.path().join("tools").join("bin");
        std::fs::create_dir_all(&tools).unwrap();
        let missing = dir.path().join("nowhere").join("bin");
        let known = vec![tools.clone(), missing, tools.clone()];
        let tools_entry = tools.display().to_string();
        assert_eq!(
            with_known_dirs("/usr/bin:/bin".into(), &known),
            format!("/usr/bin:/bin:{tools_entry}")
        );
        assert_eq!(
            with_known_dirs(format!("{tools_entry}:/usr/bin"), &known),
            format!("{tools_entry}:/usr/bin"),
            "already there, not repeated"
        );
        let shell = stub_shell(dir.path(), r#"printf %s "/opt/tools/bin:/usr/bin""#);
        assert_eq!(
            login_path_within(&shell, PATIENT, PATIENT, None, &known).as_deref(),
            Some(format!("/opt/tools/bin:/usr/bin:{tools_entry}").as_str()),
            "the shell's answer, then the folder"
        );
        let silent = stub_shell(dir.path(), "exit 2");
        let inherited = std::env::var("PATH").unwrap();
        assert_eq!(
            login_path_within(&silent, PATIENT, PATIENT, None, &known).as_deref(),
            Some(format!("{inherited}:{tools_entry}").as_str()),
            "no answer, no memo: the inherited PATH with the folder"
        );
        assert_eq!(
            login_path_within(&silent, PATIENT, PATIENT, None, &[]),
            None,
            "nothing to add: the inherited environment is left alone"
        );
    }

    /// The marked line is the answer wherever it landed; a marked line that
    /// is no path is declined, not guessed around.
    #[test]
    fn a_marked_line_is_the_path_wherever_the_profile_put_it() {
        assert_eq!(
            sanitize("Welcome\n__BISA_LOGIN_PATH__/a/bin:/usr/bin\nme@host % ").as_deref(),
            Some("/a/bin:/usr/bin")
        );
        assert_eq!(
            sanitize("Welcome__BISA_LOGIN_PATH__/a/bin\n").as_deref(),
            None,
            "a marker that did not start its line is the profile's text, not the answer"
        );
        assert_eq!(sanitize("__BISA_LOGIN_PATH__not a path\n/usr/bin"), None);
    }

    /// A shell that refuses the flags, or one that is not there, tells us
    /// nothing — and nothing is what the caller gets, never a guess.
    #[cfg(unix)]
    #[test]
    fn a_shell_that_fails_or_is_missing_is_a_none() {
        let dir = tempfile::tempdir().unwrap();
        let failing = stub_shell(dir.path(), "exit 2");
        assert_eq!(login_path_within(&failing, PATIENT, PATIENT, None, &[]), None);
        let missing = dir.path().join("no-such-shell").display().to_string();
        assert_eq!(login_path_within(&missing, PATIENT, PATIENT, None, &[]), None);
        assert_eq!(login_path_within("   ", PATIENT, PATIENT, None, &[]), None);
    }

    /// A profile that hangs is left behind at the deadline and the inherited
    /// `PATH` stays; the app never waits on it.
    #[cfg(unix)]
    #[test]
    fn a_shell_that_outstays_the_deadline_is_left_behind_and_declined() {
        let dir = tempfile::tempdir().unwrap();
        let hanging = stub_shell(dir.path(), "sleep 30");
        let started = Instant::now();
        assert_eq!(
            login_path_within(
                &hanging,
                Duration::from_millis(300),
                Duration::from_millis(300),
                None,
                &[]
            ),
            None
        );
        assert!(
            started.elapsed() < DEADLINE + Duration::from_secs(3),
            "the deadline held: {:?}",
            started.elapsed()
        );
    }

    /// A shell in time writes the memo; a shell that is late is answered from
    /// it, and its own answer — when it comes — is what the next launch reads.
    #[cfg(unix)]
    #[test]
    fn the_path_is_remembered_for_the_launch_after_a_late_shell() {
        let dir = tempfile::tempdir().unwrap();
        let memo = dir.path().join("memo").join(MEMO_FILE);
        let prompt = stub_shell(dir.path(), r#"printf %s "/first/bin:/usr/bin""#);
        assert_eq!(
            login_path_within(&prompt, PATIENT, PATIENT, Some(&memo), &[]).as_deref(),
            Some("/first/bin:/usr/bin")
        );
        assert_eq!(
            std::fs::read_to_string(&memo).unwrap(),
            "/first/bin:/usr/bin",
            "a shell in time is remembered"
        );

        let late = stub_shell(dir.path(), r#"sleep 0.4; printf %s "/second/bin:/usr/bin""#);
        let started = Instant::now();
        assert_eq!(
            login_path_within(&late, Duration::from_millis(50), PATIENT, Some(&memo), &[]).as_deref(),
            Some("/first/bin:/usr/bin"),
            "a late shell is answered from the memo"
        );
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "the budget held: {:?}",
            started.elapsed()
        );
        let deadline = Instant::now() + Duration::from_secs(10);
        while std::fs::read_to_string(&memo).ok().as_deref() != Some("/second/bin:/usr/bin") {
            assert!(
                Instant::now() < deadline,
                "the late shell's answer never reached the memo"
            );
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    /// With nothing remembered, a late shell is a `None` — as it always was —
    /// and a memo that does not look like a `PATH` is nobody's answer.
    #[cfg(unix)]
    #[test]
    fn no_memo_and_a_late_shell_is_a_none_and_a_broken_memo_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let memo = dir.path().join(MEMO_FILE);
        let late = stub_shell(dir.path(), r#"sleep 2; printf %s "/late/bin""#);
        assert_eq!(
            login_path_within(
                &late,
                Duration::from_millis(50),
                Duration::from_millis(100),
                Some(&memo),
                &[]
            ),
            None
        );
        std::fs::write(&memo, "not a path at all").unwrap();
        assert_eq!(
            login_path_within(
                &late,
                Duration::from_millis(50),
                Duration::from_millis(100),
                Some(&memo),
                &[]
            ),
            None
        );
    }

    /// The real thing, on the developer's own machine — ignored by default
    /// because it runs their login profile, which is theirs to run.
    ///
    /// Asserts only what must hold everywhere — that a shell answered and the
    /// answer parses — because the actual value is whatever this machine's
    /// profile builds, and pinning that would fail on the next one.
    #[cfg(unix)]
    #[test]
    #[ignore = "runs the developer's own login shell and profile"]
    fn a_real_login_shell_answers_with_something_that_parses() {
        let Some(path) = login_path() else {
            // No `$SHELL` (a bare CI container) is a legitimate `None`.
            return;
        };
        assert!(path.split(':').any(|e| e.starts_with('/')), "{path}");
        assert!(!path.contains('\n'), "{path}");
    }
}
