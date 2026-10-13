//! The usage readers — what a harness's account has left, from the
//! harness's own source. One module per harness that reports one:
//!
//! | harness | source | shape |
//! |---|---|---|
//! | Claude Code | Anthropic's usage endpoint (`anthropic.rs`), with Claude Code's own OAuth sign-in | by name: `five_hour`, `seven_day`, the per-model weeks in `limits[]` (*Fable*), `seven_day_opus` / `seven_day_sonnet`, `extra_usage` as credits; any other key ignored |
//! | OpenCode | the same endpoint, with the Claude Pro/Max sign-in in OpenCode's own `auth.json` | the same windows, scoped *anthropic*; other sign-ins named as reporting none |
//! | Codex | `codex app-server` over stdio, `account/read` + `account/rateLimits/read` | `primary` / `secondary` windows named by their length, the plan |
//! | OMP | `omp usage --json` | one report per provider signed in, each with its limits |
//!
//! Every reader is a **pure parser** over the payload (tested against the
//! fixtures under `tests/fixtures/usage/`, no binary, no network, no
//! credential) plus one thin fetcher that runs the binary or the request
//! with a timeout. A fetcher's failure is a [`UsageState::Failed`] whose
//! reason names what went wrong and **never quotes what came back** — a
//! body may hold an account id, a header may echo a token. A sign-in the
//! reader cannot find is [`UsageState::NotSignedIn`] with the way to fix it.

pub mod anthropic;
pub mod claude;
pub mod codex;
pub mod omp;
pub mod opencode;

use bisa_harness::UsageState;
use std::time::Duration;

/// How long a reader waits for a binary or an endpoint before it gives up.
pub const READ_TIMEOUT: Duration = Duration::from_secs(12);

/// Run `<program> <args>` for its stdout, within `timeout` ([`READ_TIMEOUT`]
/// for every reader); the failure reason names the program and the exit,
/// never its output.
#[allow(
    clippy::result_large_err,
    reason = "the Err is the usage state the caller answers as it stands; boxing it would buy 136 bytes on a path run once a minute"
)]
pub(crate) async fn stdout_of(
    program: &str,
    args: &[&str],
    timeout: Duration,
) -> Result<String, UsageState> {
    let run = tokio::process::Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true)
        .output();
    let out = match tokio::time::timeout(timeout, run).await {
        Ok(Ok(out)) => out,
        Ok(Err(e)) => {
            return Err(UsageState::failed(format!(
                "{program} could not be run: {e}"
            )))
        }
        Err(_) => {
            return Err(UsageState::failed(format!(
                "{program} did not answer within {} s",
                timeout.as_secs()
            )))
        }
    };
    if !out.status.success() {
        return Err(UsageState::failed(format!(
            "{program} {} exited with {}",
            args.join(" "),
            out.status
                .code()
                .map(|c| c.to_string())
                .unwrap_or_else(|| "a signal".to_string())
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stub(dir: &std::path::Path, body: &str) -> String {
        use std::os::unix::fs::PermissionsExt as _;
        let path = dir.join("reader");
        std::fs::write(&path, format!("#!/bin/sh\n{body}")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path.display().to_string()
    }

    fn failed(state: Result<String, UsageState>) -> String {
        match state {
            Err(UsageState::Failed { reason }) => reason,
            other => panic!("{other:?}"),
        }
    }

    /// A binary's stdout is its word; one that cannot be run, one that
    /// exits with a code or a signal, and one that outstays its budget are
    /// each said by name — and never by what they printed.
    #[tokio::test]
    async fn a_binarys_stdout_is_its_word_and_every_other_end_is_said_by_name() {
        let dir = tempfile::tempdir().unwrap();
        let fine = stub(dir.path(), "echo '{\"ok\":1}'\n");
        assert_eq!(
            stdout_of(&fine, &["usage"], READ_TIMEOUT).await.unwrap(),
            "{\"ok\":1}\n"
        );
        let gone = dir.path().join("nobody").display().to_string();
        assert!(failed(stdout_of(&gone, &[], READ_TIMEOUT).await).contains("could not be run"));
        let code = stub(dir.path(), "echo 'a secret-looking body'; exit 2\n");
        let said = failed(stdout_of(&code, &["usage", "--json"], READ_TIMEOUT).await);
        assert!(said.ends_with("usage --json exited with 2"), "{said}");
        assert!(!said.contains("body"), "never what it printed");
        let signalled = stub(dir.path(), "kill -9 $$\n");
        assert!(failed(stdout_of(&signalled, &[], READ_TIMEOUT).await).contains("a signal"));
        let slow = stub(dir.path(), "sleep 5\n");
        let said = failed(stdout_of(&slow, &[], Duration::from_millis(100)).await);
        assert!(said.contains("did not answer within 0 s"), "{said}");
    }
}
