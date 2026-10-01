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
//! menu, whose whole job is to offer what is installed, offers nothing. Run the
//! app with `npm run tauri dev` from a shell and the bug vanishes, which is
//! exactly the shape of a bug that ships.
//!
//! # What this does about it
//!
//! Once, at startup: ask the login shell what `PATH` it builds, and hand that
//! to the node. That is the same question the embedded terminal already answers
//! by construction — it spawns the login shell, so a shell in the panel has
//! always had the right `PATH` while the node deciding what to *offer* did not.
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

use std::time::Duration;

/// How long the login shell gets. A profile that sources nvm and rbenv can
/// take a noticeable fraction of a second; one that blocks on a network mount
/// can take forever, and this runs before the window is shown.
const DEADLINE: Duration = Duration::from_secs(2);

/// The `PATH` the user's login shell builds, or `None` to keep the inherited
/// one.
///
/// `None` on Windows, where there is no login-shell convention and the
/// registry environment a GUI process inherits is already the user's.
pub fn login_path() -> Option<String> {
    if cfg!(not(unix)) {
        return None;
    }
    let shell = std::env::var("SHELL").ok()?;
    login_path_within(&shell, DEADLINE)
}

/// [`login_path`] for a named shell: what `shell -l` says its `PATH` is, or
/// `None` when it says nothing usable. The shell and the deadline are
/// parameters so a test can hand over a script of its own instead of the
/// developer's login shell, and a budget that a loaded machine cannot turn
/// into a flake — the deadline's own test hands in a short one.
fn login_path_within(shell: &str, deadline: Duration) -> Option<String> {
    if shell.trim().is_empty() {
        return None;
    }
    let stdout = run_with_deadline(shell, deadline)?;
    sanitize(&stdout)
}

/// `$SHELL -l -c 'printf %s "$PATH"'`, ended if it outstays [`DEADLINE`]
/// (`probe`'s budget), and declined when the shell refused the flags — a
/// shell that did tells us nothing useful.
///
/// `-l` because the whole point is the login files: a non-login shell reads a
/// different and usually much smaller set, which is how you get a `PATH` that
/// is *almost* right and missing exactly the tool somebody is looking for.
///
/// `printf %s` rather than `echo` so nothing appends a newline of its own —
/// which matters because [`sanitize`] reads the last line, and a profile that
/// prints a banner is the ordinary case rather than the exotic one.
fn run_with_deadline(shell: &str, deadline: Duration) -> Option<String> {
    crate::probe::run(shell, &["-l", "-c", r#"printf %s "$PATH""#], deadline)?.success()
}

/// The `PATH` out of whatever the shell printed, or `None` if it does not look
/// like one.
///
/// **The last line, not the whole output.** A login shell runs the user's
/// profile, and profiles print things — a fortune, a version banner, a warning
/// from a version manager. Since the command ends in `printf %s` with no
/// newline, whatever the profile said is on earlier lines and the `PATH` is the
/// last one. Taking all of stdout would hand the node a `PATH` beginning with
/// somebody's MOTD.
///
/// The shape check is deliberately weak — one absolute entry is enough. This is
/// guarding against "the profile printed something odd", not against a hostile
/// `PATH`; a `PATH` the user's own shell builds is exactly as trustworthy as
/// the shell, which is to say completely, because they are already running
/// their own commands in it.
fn sanitize(stdout: &str) -> Option<String> {
    let last = stdout.lines().next_back()?.trim();
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
    /// the test. It accepts `-l -c <command>` the way a shell does and prints
    /// what the test tells it to — so the whole path, spawn to sanitize, is
    /// exercised without running anybody's profile.
    /// A budget no loaded machine reaches: these tests are about what the
    /// shell printed, not about the clock.
    const PATIENT: Duration = Duration::from_secs(30);

    #[cfg(unix)]
    fn stub_shell(dir: &std::path::Path, body: &str) -> String {
        use std::os::unix::fs::PermissionsExt as _;
        let path = dir.join("shell");
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
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
            r#"echo "Welcome back!"; [ "$1" = "-l" ] && [ "$2" = "-c" ] && printf %s "/opt/tools/bin:/usr/bin:/bin""#,
        );
        assert_eq!(
            login_path_within(&shell, PATIENT).as_deref(),
            Some("/opt/tools/bin:/usr/bin:/bin")
        );
    }

    /// A shell that refuses the flags, or one that is not there, tells us
    /// nothing — and nothing is what the caller gets, never a guess.
    #[cfg(unix)]
    #[test]
    fn a_shell_that_fails_or_is_missing_is_a_none() {
        let dir = tempfile::tempdir().unwrap();
        let failing = stub_shell(dir.path(), "exit 2");
        assert_eq!(login_path_within(&failing, PATIENT), None);
        let missing = dir.path().join("no-such-shell").display().to_string();
        assert_eq!(login_path_within(&missing, PATIENT), None);
        assert_eq!(login_path_within("   ", PATIENT), None);
    }

    /// A profile that hangs is killed at the deadline and the inherited
    /// `PATH` stays; the app never waits on it.
    #[cfg(unix)]
    #[test]
    fn a_shell_that_outstays_the_deadline_is_killed_and_declined() {
        let dir = tempfile::tempdir().unwrap();
        let hanging = stub_shell(dir.path(), "sleep 30");
        let started = Instant::now();
        assert_eq!(
            login_path_within(&hanging, Duration::from_millis(300)),
            None
        );
        assert!(
            started.elapsed() < DEADLINE + Duration::from_secs(3),
            "the deadline held: {:?}",
            started.elapsed()
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
