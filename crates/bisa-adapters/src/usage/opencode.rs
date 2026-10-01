//! OpenCode's usage: Anthropic's usage endpoint (`anthropic.rs`), asked
//! with the Claude Pro/Max sign-in OpenCode keeps in its own `auth.json`.
//!
//! OpenCode itself reports no limits (its maintainers declined a quota
//! command), but it signs into providers with OAuth — *Claude Pro/Max*,
//! *ChatGPT Plus/Pro*, GitHub Copilot — and stores each sign-in in
//! `$XDG_DATA_HOME/opencode/auth.json` (else `~/.local/share/opencode/auth.json`,
//! its own data path on every OS; mode 0600):
//!
//! ```json
//! { "anthropic": { "type": "oauth", "access": "…", "refresh": "…", "expires": 1788782400000, "accountId": "…" },
//!   "openai":    { "type": "oauth", "access": "…", "refresh": "…", "expires": 1788782400000 },
//!   "zai":       { "type": "api",   "key": "…" } }
//! ```
//!
//! The Anthropic entry's access token is what the usage endpoint takes, so
//! an OpenCode signed into Claude Pro/Max shows the same windows Claude Code
//! does, scoped *anthropic*. The other sign-ins report no limits; an
//! account without an Anthropic OAuth entry says so and names them. The
//! token is read into memory for the one request and dropped; OpenCode
//! refreshes an expired one itself on its next use — the platform never
//! writes its file.

use bisa_harness::usage::now_secs;
use bisa_harness::UsageState;
use serde_json::Value;

const EXPIRED: &str = "OpenCode's Claude Pro/Max sign-in has expired here — open OpenCode once (it refreshes the sign-in on use), then read again.";

/// The Anthropic OAuth access token in OpenCode's `auth.json`, when it is
/// there and not past `expires` (milliseconds). Pure; the words say what
/// is missing.
// The `Err` is the state a panel shows, answered once per read; boxing it
// would buy nothing but an allocation on the common path.
#[allow(clippy::result_large_err)]
pub fn auth_token(auth_json: &str, now_ms: u64) -> Result<String, UsageState> {
    let root: Value = serde_json::from_str(auth_json)
        .map_err(|_| UsageState::failed("OpenCode's auth.json is not JSON"))?;
    let entries = root
        .as_object()
        .ok_or_else(|| UsageState::failed("OpenCode's auth.json is not an object"))?;
    match entries.get("anthropic") {
        Some(entry) if entry.get("type").and_then(Value::as_str) == Some("oauth") => {
            let expires = entry
                .get("expires")
                .and_then(Value::as_u64)
                .unwrap_or(u64::MAX);
            if expires <= now_ms {
                return Err(UsageState::not_signed_in(EXPIRED));
            }
            entry
                .get("access")
                .and_then(Value::as_str)
                .filter(|t| !t.is_empty())
                .map(str::to_string)
                .ok_or_else(|| UsageState::not_signed_in(EXPIRED))
        }
        _ => {
            let mut others: Vec<&str> = entries
                .keys()
                .map(String::as_str)
                .filter(|k| *k != "anthropic")
                .collect();
            others.sort_unstable();
            let reason = if others.is_empty() {
                "OpenCode has no sign-in here — a Claude Pro/Max sign-in (`opencode auth login`, Anthropic) would report its windows.".to_string()
            } else {
                format!(
                    "OpenCode is signed into {} here, which report no usage limits — a Claude Pro/Max sign-in (`opencode auth login`, Anthropic) would.",
                    others.join(", ")
                )
            };
            Err(UsageState::not_signed_in(reason))
        }
    }
}

/// Where OpenCode keeps its sign-ins on this machine.
fn auth_path() -> Option<std::path::PathBuf> {
    if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
        return Some(
            std::path::Path::new(&xdg)
                .join("opencode")
                .join("auth.json"),
        );
    }
    let home = std::env::var_os("HOME")?;
    Some(
        std::path::Path::new(&home)
            .join(".local")
            .join("share")
            .join("opencode")
            .join("auth.json"),
    )
}

/// Read the account's usage through OpenCode's Anthropic sign-in.
pub async fn read(http: &bisa_http::Clients, harness: &str) -> UsageState {
    let Some(path) = auth_path() else {
        return UsageState::not_signed_in(
            "OpenCode's sign-ins could not be found — no home directory.",
        );
    };
    let text = match tokio::fs::read_to_string(&path).await {
        Ok(t) => t,
        Err(_) => return UsageState::not_signed_in("OpenCode has no sign-in here — a Claude Pro/Max sign-in (`opencode auth login`, Anthropic) would report its windows."),
    };
    let now_ms = now_secs() * 1_000;
    let token = match auth_token(&text, now_ms) {
        Ok(t) => t,
        Err(state) => return state,
    };
    match super::anthropic::fetch(http, &token, EXPIRED).await {
        Ok(body) => match super::anthropic::parse(&body, harness, now_secs(), Some("anthropic")) {
            Ok(report) => UsageState::Report { report },
            Err(reason) => UsageState::failed(reason),
        },
        Err(state) => state,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIGNED_IN: &str = include_str!("../../tests/fixtures/usage/opencode-auth.json");
    const EXPIRED_FIXTURE: &str =
        include_str!("../../tests/fixtures/usage/opencode-auth-expired.json");

    #[test]
    fn the_anthropic_oauth_token_is_the_one_the_endpoint_takes() {
        assert_eq!(
            auth_token(SIGNED_IN, 1_788_775_200_000).as_deref(),
            Ok("fake-anthropic-oauth-access-token")
        );
    }

    #[test]
    fn an_expired_or_missing_anthropic_sign_in_says_so_and_names_the_others() {
        assert!(
            matches!(auth_token(EXPIRED_FIXTURE, 1_788_775_200_000), Err(UsageState::NotSignedIn { reason }) if reason.contains("expired"))
        );
        let others_only = r#"{"openai": {"type": "oauth", "access": "x", "refresh": "y", "expires": 9999999999999}, "zai": {"type": "api", "key": "k"}}"#;
        match auth_token(others_only, 0) {
            Err(UsageState::NotSignedIn { reason }) => assert_eq!(reason, "OpenCode is signed into openai, zai here, which report no usage limits — a Claude Pro/Max sign-in (`opencode auth login`, Anthropic) would."),
            other => panic!("{other:?}"),
        }
        match auth_token("{}", 0) {
            Err(UsageState::NotSignedIn { reason }) => assert!(
                reason.starts_with("OpenCode has no sign-in here"),
                "{reason}"
            ),
            other => panic!("{other:?}"),
        }
        let api_key_anthropic = r#"{"anthropic": {"type": "api", "key": "sk-ant-api"}}"#;
        assert!(
            matches!(
                auth_token(api_key_anthropic, 0),
                Err(UsageState::NotSignedIn { .. })
            ),
            "an API key is not a Pro/Max sign-in"
        );
        assert!(matches!(
            auth_token("nope", 0),
            Err(UsageState::Failed { .. })
        ));
    }
}
