//! Claude Code's usage: Anthropic's usage endpoint (`anthropic.rs`), asked
//! with Claude Code's own OAuth sign-in.
//!
//! Claude Code shows the same numbers on its `/usage` screen and hands them
//! to a status line inside an interactive session (`rate_limits` — a
//! documented field, Pro and Max only); outside a session the endpoint is
//! the one source. **The sign-in.** Claude Code keeps its OAuth token in
//! `.credentials.json` under its config directory — `CLAUDE_CONFIG_DIR` when
//! set, else `~/.claude` — as `claudeAiOauth.accessToken`, or on macOS in the
//! login keychain item *Claude Code-credentials* with the same JSON. The file
//! is tried first; a file that is missing, holds no token or holds one whose
//! `expiresAt` has passed falls through to the keychain — Claude Code keeps
//! one or the other, and a stale file must not shadow the fresh sign-in
//! beside it. The keychain is asked under a short budget: a locked keychain
//! must not hold the footer's read for the desktop's whole deadline. The
//! token is read into memory for the one request and dropped; it is never
//! logged, never in a report, never stored by the platform.

use bisa_harness::usage::now_secs;
use bisa_harness::UsageState;
use serde_json::Value;
use std::ffi::OsString;
use std::path::PathBuf;

const KEYCHAIN_ITEM: &str = "Claude Code-credentials";
const CONFIG_DIR_VAR: &str = "CLAUDE_CONFIG_DIR";
const NOT_SIGNED_IN: &str =
    "Claude Code is not signed in on this machine — run `claude` and sign in, then read again.";
const EXPIRED: &str =
    "Claude Code's sign-in has expired here — run `claude` and sign in again, then read again.";

/// Claude Code's config directory: the override, else `~/.claude`. Pure.
fn config_dir(override_dir: Option<OsString>, home: Option<OsString>) -> Option<PathBuf> {
    match override_dir {
        Some(dir) if !dir.is_empty() => Some(PathBuf::from(dir)),
        _ => home.map(|h| PathBuf::from(h).join(".claude")),
    }
}

/// How long the keychain gets to answer: a locked keychain, or one asking the
/// person, must not hold the read for the desktop's whole deadline.
const KEYCHAIN_BUDGET: std::time::Duration = std::time::Duration::from_secs(5);

/// The token from the credential JSON: the parsed field only, never the file.
fn token_of(credentials_json: &str) -> Option<String> {
    let v: Value = serde_json::from_str(credentials_json).ok()?;
    v.get("claudeAiOauth")?
        .get("accessToken")?
        .as_str()
        .map(str::to_string)
}

/// When the credential JSON says its token expires — `claudeAiOauth.expiresAt`,
/// milliseconds since the epoch — as seconds; `None` when it does not say.
fn expires_at_of(credentials_json: &str) -> Option<u64> {
    let v: Value = serde_json::from_str(credentials_json).ok()?;
    let ms = v.get("claudeAiOauth")?.get("expiresAt")?;
    ms.as_u64()
        .or_else(|| ms.as_f64().map(|f| f.max(0.0) as u64))
        .map(|ms| ms / 1_000)
}

/// The credential JSON's token, unless the JSON itself says it has expired:
/// a stale file is no sign-in, and the keychain beside it may hold a fresh one.
fn live_token_of(credentials_json: &str, now: u64) -> Option<String> {
    if expires_at_of(credentials_json).is_some_and(|at| at <= now) {
        return None;
    }
    token_of(credentials_json)
}

/// The token from the credential file, when there is one with a live token in it.
async fn file_token() -> Option<String> {
    let dir = config_dir(std::env::var_os(CONFIG_DIR_VAR), std::env::var_os("HOME"))?;
    let text = tokio::fs::read_to_string(dir.join(".credentials.json"))
        .await
        .ok()?;
    live_token_of(&text, now_secs())
}

/// The token from the macOS login keychain, when the item is there and the
/// keychain answers within its budget.
async fn keychain_token() -> Option<String> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    let asked = tokio::process::Command::new("security")
        .args(["find-generic-password", "-s", KEYCHAIN_ITEM, "-w"])
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true)
        .output();
    let out = tokio::time::timeout(KEYCHAIN_BUDGET, asked)
        .await
        .ok()?
        .ok()?;
    if !out.status.success() {
        return None;
    }
    live_token_of(String::from_utf8_lossy(&out.stdout).trim(), now_secs())
}

#[allow(
    clippy::result_large_err,
    reason = "the Err is the usage state the caller answers as it stands; boxing it would buy 136 bytes on a path run once a minute"
)]
async fn credential() -> Result<String, UsageState> {
    if let Some(token) = file_token().await {
        return Ok(token);
    }
    keychain_token()
        .await
        .ok_or_else(|| UsageState::not_signed_in(NOT_SIGNED_IN))
}

/// Read the account's usage. The token lives for this call only.
pub async fn read(http: &bisa_http::Clients, harness: &str) -> UsageState {
    let token = match credential().await {
        Ok(t) => t,
        Err(state) => return state,
    };
    match super::anthropic::fetch(http, &token, EXPIRED).await {
        Ok(text) => match super::anthropic::parse(&text, harness, now_secs(), None) {
            Ok(report) => UsageState::Report { report },
            Err(reason) => UsageState::failed(reason),
        },
        Err(state) => state,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_token_is_the_one_field_of_the_credential_json() {
        assert_eq!(
            token_of(r#"{"claudeAiOauth":{"accessToken":"sk-ant-oat-x","refreshToken":"r"}}"#)
                .as_deref(),
            Some("sk-ant-oat-x")
        );
        assert_eq!(token_of(r#"{"claudeAiOauth":{}}"#), None);
        assert_eq!(token_of("{}"), None);
        assert_eq!(token_of("nope"), None);
    }

    /// A file whose own `expiresAt` has passed is no sign-in — the keychain
    /// beside it is asked instead — while one that says nothing, or a time
    /// still to come, hands its token over.
    #[test]
    fn an_expired_credential_file_yields_to_the_keychain() {
        let now = 1_700_000_000;
        let fresh = format!(
            r#"{{"claudeAiOauth":{{"accessToken":"sk-ant-oat-fresh","expiresAt":{}}}}}"#,
            (now + 3_600) * 1_000
        );
        let stale = format!(
            r#"{{"claudeAiOauth":{{"accessToken":"sk-ant-oat-stale","expiresAt":{}}}}}"#,
            (now - 60) * 1_000
        );
        assert_eq!(expires_at_of(&fresh), Some(now + 3_600));
        assert_eq!(
            expires_at_of(r#"{"claudeAiOauth":{"accessToken":"x"}}"#),
            None
        );
        assert_eq!(
            live_token_of(&fresh, now).as_deref(),
            Some("sk-ant-oat-fresh")
        );
        assert_eq!(
            live_token_of(&stale, now),
            None,
            "a stale file is no sign-in"
        );
        assert_eq!(
            live_token_of(r#"{"claudeAiOauth":{"accessToken":"x"}}"#, now).as_deref(),
            Some("x"),
            "a file that does not say when it expires is taken at its word"
        );
        assert_eq!(live_token_of("nope", now), None);
    }

    #[test]
    fn the_config_dir_is_the_override_else_home_slash_dot_claude() {
        assert_eq!(
            config_dir(Some("/cfg/claude".into()), Some("/home/me".into())),
            Some(PathBuf::from("/cfg/claude"))
        );
        assert_eq!(
            config_dir(Some("".into()), Some("/home/me".into())),
            Some(PathBuf::from("/home/me/.claude")),
            "an empty override is none"
        );
        assert_eq!(
            config_dir(None, Some("/home/me".into())),
            Some(PathBuf::from("/home/me/.claude"))
        );
        assert_eq!(config_dir(None, None), None);
    }
}
