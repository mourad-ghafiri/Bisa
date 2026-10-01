//! Codex's usage: its own app server, asked over its protocol.
//!
//! `codex app-server` speaks JSON-RPC over stdio, one frame per line. After
//! the `initialize` handshake it answers `account/read` (who is signed in —
//! a ChatGPT plan, an API key, Bedrock) and `account/rateLimits/read`:
//!
//! ```json
//! { "rateLimits": { "primary":   { "usedPercent": 74, "windowDurationMins": 300,   "resetsAt": 1788782400 },
//!                   "secondary": { "usedPercent": 16, "windowDurationMins": 10080, "resetsAt": 1789128000 },
//!                   "planType": "pro" } }
//! ```
//!
//! The two windows are named by their length — 300 minutes is *5h*, 10 080
//! is *Weekly* — because the app server does not say which is which, and has
//! moved them around before (a plan whose five-hour bucket vanished left the
//! week as `primary` and `secondary` null). The protocol crate marks the app
//! server experimental; a shape that changes lands as a *failed* read.
//!
//! The reader spawns the app server for the two answers and terminates it;
//! Codex reads its own sign-in (`~/.codex`) — the platform never touches it.

use bisa_harness::usage::{now_secs, window_label};
use bisa_harness::{UsageAccount, UsageReport, UsageSource, UsageState, UsageWindow};
use serde_json::{json, Value};
use std::process::Stdio;
use tokio::io::{AsyncWriteExt, BufReader};

const ID_ACCOUNT: u64 = 2;
const ID_LIMITS: u64 = 3;

/// The two answers as a report. Pure. `account` is the `account/read`
/// result, `limits` the `account/rateLimits/read` result.
// The `Err` is the state a panel shows — not signed in, unsupported, failed —
// answered once per read; its size is the report's, and boxing it would buy
// nothing but an allocation on the common path.
#[allow(clippy::result_large_err)]
pub fn parse(
    account: Option<&Value>,
    limits: &Value,
    harness: &str,
    read_at: u64,
) -> Result<UsageReport, UsageState> {
    let acct = account.map(account_of);
    if let Some(a) = &acct {
        if a.kind == "apiKey" || a.kind == "api_key" {
            return Err(UsageState::not_signed_in("Codex is signed in with an API key here — usage limits belong to a ChatGPT sign-in."));
        }
    }
    let snapshot = limits
        .get("rateLimits")
        .and_then(Value::as_object)
        .ok_or_else(|| UsageState::failed("the app server answered no rate limits"))?;
    let mut timed: Vec<(u64, UsageWindow)> = ["primary", "secondary"]
        .into_iter()
        .filter_map(|key| window(snapshot.get(key), key))
        .collect();
    // The short window first, as every other harness lists them; a window
    // of unknown length last, then by key.
    timed.sort_by(|(a, wa), (b, wb)| a.cmp(b).then_with(|| wa.id.cmp(&wb.id)));
    let windows: Vec<UsageWindow> = timed.into_iter().map(|(_, w)| w).collect();
    let plan = snapshot
        .get("planType")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| acct.as_ref().and_then(|a| a.plan.clone()));
    let login = acct.as_ref().and_then(|a| a.login.clone());
    let account = (plan.is_some() || login.is_some()).then_some(UsageAccount { plan, login });
    if windows.is_empty() {
        return Err(UsageState::failed(
            "the app server reported no usage window for this account",
        ));
    }
    Ok(UsageReport {
        harness: harness.to_string(),
        read_at,
        source: UsageSource::AppServer,
        account,
        windows,
        extras: Vec::new(),
    })
}

struct AccountFacts {
    kind: String,
    plan: Option<String>,
    login: Option<String>,
}

/// What `account/read` says, leniently: the shape is the protocol crate's
/// externally tagged enum, but only three facts matter and each is read
/// wherever it sits.
fn account_of(v: &Value) -> AccountFacts {
    let inner = v.get("account").unwrap_or(v);
    let kind = inner
        .get("type")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| inner.as_object().and_then(|o| o.keys().next().cloned()))
        .unwrap_or_default();
    let body = inner
        .as_object()
        .and_then(|o| o.get(&kind))
        .unwrap_or(inner);
    let plan = body
        .get("planType")
        .and_then(Value::as_str)
        .map(str::to_string);
    let login = body
        .get("email")
        .and_then(Value::as_str)
        .map(str::to_string);
    AccountFacts { kind, plan, login }
}

/// A window with its length in minutes — `u64::MAX` when the app server
/// does not say, so it sorts last.
fn window(value: Option<&Value>, id: &str) -> Option<(u64, UsageWindow)> {
    let v = value?.as_object()?;
    let used = v.get("usedPercent")?.as_f64()?;
    let mins = v.get("windowDurationMins").and_then(Value::as_u64);
    let label = mins
        .map(|m| window_label(m * 60))
        .unwrap_or_else(|| id.to_string());
    let resets_at = v.get("resetsAt").and_then(Value::as_u64);
    Some((
        mins.unwrap_or(u64::MAX),
        UsageWindow {
            id: id.to_string(),
            label,
            scope: None,
            used_percent: used as f32,
            resets_at,
        },
    ))
}

/// Read the account's usage from the app server. Codex's own binary does
/// the sign-in; this spawns it, asks, and terminates it.
pub async fn read(program: &str, harness: &str) -> UsageState {
    let mut child = match tokio::process::Command::new(program)
        .arg("app-server")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            return UsageState::failed(format!("{program} app-server could not be started: {e}"))
        }
    };
    let outcome = tokio::time::timeout(super::READ_TIMEOUT, talk(&mut child)).await;
    if let Err(e) = child.kill().await {
        tracing::warn!("the codex usage probe did not end on request: {e}");
    }
    match outcome {
        Ok(Ok((account, limits))) => match parse(account.as_ref(), &limits, harness, now_secs()) {
            Ok(report) => UsageState::Report { report },
            Err(state) => state,
        },
        Ok(Err(state)) => state,
        Err(_) => UsageState::failed(format!(
            "{program} app-server did not answer within {} s",
            super::READ_TIMEOUT.as_secs()
        )),
    }
}

/// The handshake and the two questions; answers by id.
#[allow(
    clippy::result_large_err,
    reason = "the Err is the usage state the caller answers as it stands; boxing it would buy 136 bytes on a path run once a minute"
)]
async fn talk(child: &mut tokio::process::Child) -> Result<(Option<Value>, Value), UsageState> {
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| UsageState::failed("the app server has no stdin"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| UsageState::failed("the app server has no stdout"))?;
    let frames = [
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"clientInfo": {"name": "bisa", "title": "Bisa", "version": env!("CARGO_PKG_VERSION")}}}),
        json!({"jsonrpc": "2.0", "method": "initialized"}),
        json!({"jsonrpc": "2.0", "id": ID_ACCOUNT, "method": "account/read", "params": {}}),
        json!({"jsonrpc": "2.0", "id": ID_LIMITS, "method": "account/rateLimits/read", "params": {}}),
    ];
    for frame in frames {
        let line = format!("{frame}\n");
        stdin
            .write_all(line.as_bytes())
            .await
            .map_err(|_| UsageState::failed("the app server closed its stdin"))?;
    }
    // The harness crate's reader: a byte that is no text, or a line without
    // end, never stops the reading — the answer may be the line after it.
    let mut reader = BufReader::new(stdout);
    let mut account = None;
    let mut limits = None;
    while let Ok(Some(read)) =
        bisa_harness::proc::next_line(&mut reader, bisa_harness::proc::MAX_LINE_BYTES).await
    {
        let Ok(v) = serde_json::from_str::<Value>(&read.text) else {
            continue;
        };
        match v.get("id").and_then(Value::as_u64) {
            Some(ID_ACCOUNT) => account = v.get("result").cloned(),
            Some(ID_LIMITS) => {
                if let Some(err) = v.get("error") {
                    let message = err
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("refused");
                    return Err(UsageState::failed(format!(
                        "the app server refused the rate-limits read: {message}"
                    )));
                }
                limits = v.get("result").cloned();
            }
            _ => {}
        }
        if limits.is_some()
            && (account.is_some() || v.get("id").and_then(Value::as_u64) == Some(ID_LIMITS))
        {
            break;
        }
    }
    let limits = limits.ok_or_else(|| {
        UsageState::failed("the app server ended before answering the rate-limits read")
    })?;
    Ok((account, limits))
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIMITS: &str = include_str!("../../tests/fixtures/usage/codex-limits.json");
    const ACCOUNT: &str = include_str!("../../tests/fixtures/usage/codex-account.json");

    #[test]
    fn the_windows_are_named_by_their_length_short_first_and_the_plan_is_read() {
        let limits: Value = serde_json::from_str(LIMITS).unwrap();
        let account: Value = serde_json::from_str(ACCOUNT).unwrap();
        let r = parse(Some(&account), &limits, "codex", 9).unwrap();
        assert_eq!(r.source, UsageSource::AppServer);
        let labels: Vec<&str> = r.windows.iter().map(|w| w.label.as_str()).collect();
        assert_eq!(labels, ["5h", "Weekly"]);
        assert_eq!(r.windows[0].id, "primary");
        assert_eq!(r.windows[0].used_percent, 74.0);
        assert_eq!(r.windows[0].resets_at, Some(1_788_782_400));
        assert_eq!(r.windows[1].used_percent, 16.0);
        let account = r.account.unwrap();
        assert_eq!(account.plan.as_deref(), Some("pro"));
        assert_eq!(account.login.as_deref(), Some("dev@example.com"));
    }

    #[test]
    fn a_vanished_bucket_leaves_one_window_and_the_week_is_still_the_week() {
        let limits = json!({"rateLimits": {"primary": {"usedPercent": 16, "windowDurationMins": 10080}, "secondary": null, "planType": "plus"}});
        let r = parse(None, &limits, "codex", 0).unwrap();
        assert_eq!(r.windows.len(), 1);
        assert_eq!(r.windows[0].label, "Weekly");
        assert_eq!(
            r.windows[0].resets_at, None,
            "the app server does not always know"
        );
        assert_eq!(r.account.unwrap().plan.as_deref(), Some("plus"));
    }

    #[test]
    fn an_api_key_sign_in_has_no_limits_and_no_snapshot_is_a_failure() {
        let limits: Value = serde_json::from_str(LIMITS).unwrap();
        let api_key = json!({"account": {"type": "apiKey"}});
        assert!(matches!(
            parse(Some(&api_key), &limits, "codex", 0),
            Err(UsageState::NotSignedIn { .. })
        ));
        assert!(matches!(
            parse(None, &json!({}), "codex", 0),
            Err(UsageState::Failed { .. })
        ));
        assert!(matches!(
            parse(
                None,
                &json!({"rateLimits": {"primary": null, "secondary": null}}),
                "codex",
                0
            ),
            Err(UsageState::Failed { .. })
        ));
    }

    #[test]
    fn the_account_is_read_from_either_tagging() {
        let internal = account_of(
            &json!({"account": {"type": "chatgpt", "email": "a@b.c", "planType": "team"}}),
        );
        assert_eq!(
            (
                internal.kind.as_str(),
                internal.plan.as_deref(),
                internal.login.as_deref()
            ),
            ("chatgpt", Some("team"), Some("a@b.c"))
        );
        let external =
            account_of(&json!({"account": {"chatgpt": {"email": "a@b.c", "planType": "pro"}}}));
        assert_eq!(
            (external.kind.as_str(), external.plan.as_deref()),
            ("chatgpt", Some("pro"))
        );
    }
}
