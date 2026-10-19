//! Muse Code's usage: its own host, asked over its protocol.
//!
//! `muse serve` answers `usage/read` with the subscription usage it last
//! observed — a window the host names by its length (five hours, the manual
//! says) and the rolling week, each a used percentage and a reset time, with
//! the plan's tier; a live session hears the same as `usage/changed`:
//!
//! ```json
//! { "usage": { "observedAtMs": 1788775200000, "tier": "power",
//!              "window": { "usedPercent": 74, "windowDurationMins": 300, "resetsAtMs": 1788782400000 },
//!              "weekly": { "usedPercent": 16, "resetsAtMs": 1789128000000 } } }
//! ```
//!
//! The `usage` member is omitted when nothing has been observed — a fresh
//! host, or an API-key sign-in that has no subscription — and that is a
//! *failed* read in words: the one state asked again next time, since a
//! session may have heard something by then. The reader spawns the host
//! for the one answer and lets it go; Muse Code reads its own sign-in
//! (`~/.config/muse`) — the platform never touches it.

use bisa_harness::usage::{now_secs, window_label};
use bisa_harness::{UsageAccount, UsageReport, UsageSource, UsageState, UsageWindow};
use serde_json::{json, Value};
use std::time::Duration;

/// The payload as a report. Pure. `payload` is `usage/read`'s result or
/// `usage/changed`'s params — the usage under `usage`, or the usage itself.
// The `Err` is the state a panel shows — not signed in, unsupported, failed —
// answered once per read; its size is the report's, and boxing it would buy
// nothing but an allocation on the common path.
#[allow(clippy::result_large_err)]
pub fn parse(payload: &Value, harness: &str, read_at: u64) -> Result<UsageReport, UsageState> {
    let usage = payload
        .get("usage")
        .filter(|u| u.is_object())
        .or_else(|| {
            (payload.get("window").is_some() || payload.get("weekly").is_some()).then_some(payload)
        })
        .ok_or_else(|| {
            UsageState::failed(
                "Muse Code has observed no usage on this machine yet — it reports what its last turn saw; run a turn, then read again",
            )
        })?;
    let mut timed: Vec<(u64, UsageWindow)> = Vec::new();
    if let Some(w) = window(usage.get("window"), "window", None) {
        timed.push(w);
    }
    if let Some(w) = window(usage.get("weekly"), "weekly", Some(7 * 24 * 60)) {
        timed.push(w);
    }
    // The short window first, as every other harness lists them.
    timed.sort_by(|(a, wa), (b, wb)| a.cmp(b).then_with(|| wa.id.cmp(&wb.id)));
    let windows: Vec<UsageWindow> = timed.into_iter().map(|(_, w)| w).collect();
    if windows.is_empty() {
        return Err(UsageState::failed(
            "Muse Code's host reported no usage window for this account",
        ));
    }
    let plan = usage
        .get("tier")
        .and_then(Value::as_str)
        .filter(|t| !t.is_empty())
        .map(str::to_string);
    Ok(UsageReport {
        harness: harness.to_string(),
        read_at,
        source: UsageSource::AppServer,
        account: plan.map(|plan| UsageAccount {
            plan: Some(plan),
            login: None,
        }),
        windows,
        extras: Vec::new(),
    })
}

/// A window with its length in minutes — the host's for the window it
/// names, the week's for the week; `u64::MAX` when neither says, so it
/// sorts last.
fn window(value: Option<&Value>, id: &str, minutes: Option<u64>) -> Option<(u64, UsageWindow)> {
    let v = value?.as_object()?;
    let used = v.get("usedPercent")?.as_f64()?;
    let mins = v
        .get("windowDurationMins")
        .and_then(Value::as_u64)
        .or(minutes);
    let label = mins
        .map(|m| window_label(m * 60))
        .unwrap_or_else(|| id.to_string());
    let resets_at = v
        .get("resetsAtMs")
        .and_then(Value::as_u64)
        .map(|ms| ms / 1_000);
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

/// Read the account's usage from a host started with `args` — the words
/// that make the CLI a host of nothing but this question.
pub async fn read(program: &str, args: &[&str], harness: &str) -> UsageState {
    read_within(program, args, harness, super::READ_TIMEOUT).await
}

async fn read_within(program: &str, args: &[&str], harness: &str, timeout: Duration) -> UsageState {
    match crate::msp::ask(program, args, "usage/read", json!({}), timeout).await {
        Ok(result) => match parse(&result, harness, now_secs()) {
            Ok(report) => UsageState::Report { report },
            Err(state) => state,
        },
        Err(refusal) => UsageState::failed(format!("Muse Code's usage was not read: {refusal}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const USAGE: &str = include_str!("../../tests/fixtures/usage/muse-code.json");

    #[test]
    fn the_windows_are_named_by_their_length_short_first_and_the_plan_is_the_tier() {
        let payload: Value = serde_json::from_str(USAGE).unwrap();
        let report = parse(&payload, "muse-code", 1_788_775_300).unwrap();
        assert_eq!(report.harness, "muse-code");
        assert_eq!(report.read_at, 1_788_775_300);
        assert_eq!(report.source, UsageSource::AppServer);
        assert_eq!(
            report
                .windows
                .iter()
                .map(|w| w.label.as_str())
                .collect::<Vec<_>>(),
            ["5h", "Weekly"]
        );
        assert_eq!(report.windows[0].id, "window");
        assert_eq!(report.windows[0].used_percent, 74.0);
        assert_eq!(report.windows[0].resets_at, Some(1_788_782_400));
        assert_eq!(report.windows[1].id, "weekly");
        assert_eq!(report.windows[1].used_percent, 16.0);
        assert_eq!(report.windows[1].resets_at, Some(1_789_128_000));
        assert_eq!(report.account.unwrap().plan.as_deref(), Some("power"));
        assert!(report.extras.is_empty());
        // The usage itself, as a live session hears it, reads the same.
        let bare = parse(&payload["usage"], "muse-code", 1).unwrap();
        assert_eq!(bare.windows, report.windows);
    }

    #[test]
    fn a_window_of_unknown_length_sorts_last_and_a_tier_is_optional() {
        let payload = json!({ "usage": { "window": { "usedPercent": 10.5 }, "weekly": { "usedPercent": 1, "resetsAtMs": 2_000 } } });
        let report = parse(&payload, "muse-code", 1).unwrap();
        assert_eq!(
            report
                .windows
                .iter()
                .map(|w| w.label.as_str())
                .collect::<Vec<_>>(),
            ["Weekly", "window"]
        );
        assert_eq!(report.windows[1].used_percent, 10.5);
        assert_eq!(report.windows[1].resets_at, None);
        assert_eq!(report.windows[0].resets_at, Some(2));
        assert!(report.account.is_none());
        let only_week = parse(
            &json!({ "usage": { "tier": "", "weekly": { "usedPercent": 3 } } }),
            "muse-code",
            1,
        )
        .unwrap();
        assert_eq!(only_week.windows.len(), 1);
        assert!(only_week.account.is_none(), "an empty tier is none");
    }

    fn reason(state: UsageState) -> String {
        match state {
            UsageState::Failed { reason } => reason,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn nothing_observed_and_no_window_are_failures_in_words() {
        assert!(
            reason(parse(&json!({}), "muse-code", 1).unwrap_err()).contains("observed no usage")
        );
        assert!(
            reason(parse(&json!({ "usage": null }), "muse-code", 1).unwrap_err())
                .contains("observed no usage")
        );
        assert!(reason(
            parse(&json!({ "usage": { "tier": "power" } }), "muse-code", 1).unwrap_err()
        )
        .contains("no usage window"));
        assert!(
            reason(
                parse(
                    &json!({ "usage": { "window": { "windowDurationMins": 300 } } }),
                    "muse-code",
                    1
                )
                .unwrap_err()
            )
            .contains("no usage window"),
            "a window with no used percentage is no window"
        );
        assert!(reason(parse(&json!("junk"), "muse-code", 1).unwrap_err())
            .contains("observed no usage"));
    }

    /// A host of the test's own: a shell script that answers the handshake
    /// and the question as `body` says.
    fn host(dir: &std::path::Path, body: &str) -> String {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join("muse");
        std::fs::write(&path, format!("#!/bin/bash\n{body}")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path.display().to_string()
    }

    /// A host that says, before each answer, things a client passes over:
    /// a line that is no frame, a notification, an answer to somebody else.
    const ANSWERS: &str = r#"
while read -r line; do
  case "$line" in
    *'"method":"initialize"'*) echo 'muse host warming up'; echo '{"jsonrpc":"2.0","method":"log/record","params":{}}'; echo '{"jsonrpc":"2.0","id":1,"result":{"serverInfo":{"name":"muse-session-server","version":"1.4.10"},"schema":{"version":1,"fingerprint":"x"}}}' ;;
    *'"method":"usage/read"'*) echo '{"jsonrpc":"2.0","id":9,"result":{}}'; echo '{"jsonrpc":"2.0","id":2,"result":{"usage":{"tier":"everyday","window":{"usedPercent":30,"windowDurationMins":300,"resetsAtMs":1788782400000},"weekly":{"usedPercent":2,"resetsAtMs":1789128000000}}}}' ;;
  esac
done
"#;

    #[tokio::test]
    async fn the_host_is_asked_over_its_protocol_and_every_end_is_said_by_name() {
        let dir = tempfile::tempdir().unwrap();
        let program = host(dir.path(), ANSWERS);
        match read(&program, &["serve"], "muse-code").await {
            UsageState::Report { report } => {
                assert_eq!(report.windows.len(), 2);
                assert_eq!(report.windows[0].used_percent, 30.0);
                assert_eq!(report.account.unwrap().plan.as_deref(), Some("everyday"));
            }
            other => panic!("{other:?}"),
        }
        // A host with nothing observed yet.
        let program = host(
            dir.path(),
            r#"
while read -r line; do
  case "$line" in
    *'"method":"initialize"'*) echo '{"jsonrpc":"2.0","id":1,"result":{}}' ;;
    *'"method":"usage/read"'*) echo '{"jsonrpc":"2.0","id":2,"result":{}}' ;;
  esac
done
"#,
        );
        assert!(reason(read(&program, &["serve"], "muse-code").await).contains("observed no usage"));
        // A host that refuses the question.
        let program = host(
            dir.path(),
            r#"
while read -r line; do
  case "$line" in
    *'"method":"initialize"'*) echo '{"jsonrpc":"2.0","id":1,"result":{}}' ;;
    *'"method":"usage/read"'*) echo '{"jsonrpc":"2.0","id":2,"error":{"code":-32010,"message":"capabilityRequired"}}' ;;
  esac
done
"#,
        );
        let said = reason(read(&program, &["serve"], "muse-code").await);
        assert!(
            said.contains("refused (-32010)") && said.contains("capabilityRequired"),
            "{said}"
        );
        // A host that leaves with the manual's code before answering.
        let program = host(dir.path(), "read -r _line; exit 3\n");
        let said = reason(read(&program, &["serve"], "muse-code").await);
        assert!(
            said.contains("ended before answering") && said.contains("sign in"),
            "{said}"
        );
        // One that leaves with a code the manual does not name, and one
        // that says nothing in time.
        let program = host(dir.path(), "read -r _line; exit 9\n");
        let said = reason(read(&program, &["serve"], "muse-code").await);
        assert!(said.contains("exit 9"), "{said}");
        let program = host(dir.path(), "while read -r _line; do :; done\n");
        let said = reason(
            read_within(
                &program,
                &["serve"],
                "muse-code",
                Duration::from_millis(300),
            )
            .await,
        );
        assert!(said.contains("did not answer within 0 s"), "{said}");
        // One that is not there at all, and one that cannot be run.
        let said = reason(read("/nowhere/muse", &["serve"], "muse-code").await);
        assert!(
            said.contains("could not be started") && said.contains("not found on PATH"),
            "{said}"
        );
        let unrunnable = dir.path().join("muse-unrunnable");
        std::fs::write(&unrunnable, "not a program").unwrap();
        let said = reason(read(&unrunnable.display().to_string(), &["serve"], "muse-code").await);
        assert!(
            said.contains("could not be started") && !said.contains("not found"),
            "{said}"
        );
        // One that answers and then ignores the end of its input: given the
        // leaving grace, then ended — its answer still read.
        let program = host(
            dir.path(),
            r#"
read -r line; echo '{"jsonrpc":"2.0","id":1,"result":{}}'
read -r line; read -r line; echo '{"jsonrpc":"2.0","id":2,"result":{"usage":{"tier":"power","window":{"usedPercent":1,"windowDurationMins":300}}}}'
exec sleep 30
"#,
        );
        let started = std::time::Instant::now();
        match read(&program, &["serve"], "muse-code").await {
            UsageState::Report { report } => {
                assert_eq!(report.account.unwrap().plan.as_deref(), Some("power"))
            }
            other => panic!("{other:?}"),
        }
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "ended past the leaving grace, not waited out: {:?}",
            started.elapsed()
        );
    }
}
