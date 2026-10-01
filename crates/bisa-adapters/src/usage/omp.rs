//! OMP's usage: `omp usage --json` — OMP's own report of every provider it
//! is signed into, read from its own credential store and cached by OMP
//! itself (`omp usage invalidate` drops that cache).
//!
//! ```json
//! { "generatedAt": 1788775200000,
//!   "reports": [
//!     { "provider": "anthropic",
//!       "metadata": { "email": "dev@example.com", "planType": "max" },
//!       "limits": [
//!         { "label": "Session", "scope": "account",
//!           "window": { "label": "5h", "durationMs": 18000000, "resetsAt": 1788782400000 },
//!           "amount": { "used": 23.5, "limit": 100, "remaining": 76.5, "unit": "percent" } } ] } ],
//!   "accountsWithoutUsage": [ { "provider": "openai", "reason": "…" } ],
//!   "disabledCredentials": [] }
//! ```
//!
//! One window per limit, scoped to its provider; the window's length names
//! it when OMP gives one, else OMP's own label. A percentage is the amount
//! against its limit, or the amount itself when the unit is a percent.

use bisa_harness::usage::{now_secs, window_label};
use bisa_harness::{UsageAccount, UsageReport, UsageSource, UsageState, UsageWindow};
use serde_json::Value;

/// The report as ours. Pure.
// The `Err` is the state a panel shows — not signed in, unsupported, failed —
// answered once per read; its size is the report's, and boxing it would buy
// nothing but an allocation on the common path.
#[allow(clippy::result_large_err)]
pub fn parse(text: &str, harness: &str, read_at: u64) -> Result<UsageReport, UsageState> {
    let root: Value = serde_json::from_str(text)
        .map_err(|_| UsageState::failed("omp usage --json printed something that is not JSON"))?;
    let reports = root
        .get("reports")
        .and_then(Value::as_array)
        .ok_or_else(|| UsageState::failed("omp usage --json printed no reports"))?;
    let mut windows = Vec::new();
    let mut plans = Vec::new();
    let mut logins = Vec::new();
    for report in reports {
        let provider = report
            .get("provider")
            .and_then(Value::as_str)
            .unwrap_or("provider");
        if let Some(plan) = report.pointer("/metadata/planType").and_then(Value::as_str) {
            plans.push(format!("{provider}: {plan}"));
        }
        if let Some(login) = report.pointer("/metadata/email").and_then(Value::as_str) {
            logins.push(login.to_string());
        }
        for limit in report
            .get("limits")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let Some(w) = window(limit, provider) {
                windows.push(w);
            }
        }
    }
    if windows.is_empty() {
        let without = root
            .get("accountsWithoutUsage")
            .and_then(Value::as_array)
            .map(Vec::len)
            .unwrap_or(0);
        return Err(if reports.is_empty() && without == 0 {
            UsageState::not_signed_in("omp has no signed-in account here — run `omp` and sign in to a provider, then read again.")
        } else {
            UsageState::not_signed_in(
                "none of omp's providers reports usage limits for this account.",
            )
        });
    }
    logins.sort();
    logins.dedup();
    let account = (!plans.is_empty() || !logins.is_empty()).then_some(UsageAccount {
        plan: (!plans.is_empty()).then(|| plans.join(" · ")),
        login: (!logins.is_empty()).then(|| logins.join(" · ")),
    });
    Ok(UsageReport {
        harness: harness.to_string(),
        read_at,
        source: UsageSource::Cli,
        account,
        windows,
        extras: Vec::new(),
    })
}

fn window(limit: &Value, provider: &str) -> Option<UsageWindow> {
    let amount = limit.get("amount")?;
    let used = amount.get("used")?.as_f64()?;
    let cap = amount.get("limit").and_then(Value::as_f64);
    let unit = amount.get("unit").and_then(Value::as_str).unwrap_or("");
    let percent = match cap {
        Some(cap) if cap > 0.0 => used / cap * 100.0,
        _ if unit == "percent" => used,
        _ => return None,
    };
    let win = limit.get("window");
    let duration = win
        .and_then(|w| w.get("durationMs"))
        .and_then(Value::as_u64)
        .map(|ms| ms / 1_000);
    let own_label = limit
        .get("label")
        .and_then(Value::as_str)
        .unwrap_or("limit");
    let label = duration
        .map(window_label)
        .unwrap_or_else(|| own_label.to_string());
    let resets_at = win
        .and_then(|w| w.get("resetsAt"))
        .and_then(Value::as_u64)
        .map(|t| if t > 100_000_000_000 { t / 1_000 } else { t });
    Some(UsageWindow {
        id: format!("{provider}:{}", own_label.to_lowercase().replace(' ', "_")),
        label,
        scope: Some(provider.to_string()),
        used_percent: percent as f32,
        resets_at,
    })
}

/// Read every provider's usage through omp's own CLI.
pub async fn read(program: &str, harness: &str) -> UsageState {
    match super::stdout_of(program, &["usage", "--json"]).await {
        Ok(text) => match parse(&text, harness, now_secs()) {
            Ok(report) => UsageState::Report { report },
            Err(state) => state,
        },
        Err(state) => state,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../tests/fixtures/usage/omp.json");

    #[test]
    fn every_provider_limit_is_a_window_scoped_to_its_provider() {
        let r = parse(FIXTURE, "omp", 3).unwrap();
        assert_eq!(r.source, UsageSource::Cli);
        let rows: Vec<(&str, &str, Option<&str>)> = r
            .windows
            .iter()
            .map(|w| (w.id.as_str(), w.label.as_str(), w.scope.as_deref()))
            .collect();
        assert_eq!(
            rows,
            [
                ("anthropic:session", "5h", Some("anthropic")),
                ("anthropic:weekly", "Weekly", Some("anthropic")),
                ("zai:credits", "Monthly", Some("zai")),
            ]
        );
        assert_eq!(
            r.windows[0].used_percent, 23.5,
            "a percent unit is the amount itself"
        );
        assert_eq!(
            r.windows[2].used_percent, 25.0,
            "an amount against its limit"
        );
        assert_eq!(
            r.windows[0].resets_at,
            Some(1_788_782_400),
            "milliseconds become seconds"
        );
        let account = r.account.unwrap();
        assert_eq!(
            account.plan.as_deref(),
            Some("anthropic: max · zai: coding-pro")
        );
        assert_eq!(account.login.as_deref(), Some("dev@example.com"));
    }

    #[test]
    fn nothing_signed_in_says_so_and_a_limitless_amount_is_no_window() {
        assert!(matches!(
            parse(r#"{"reports":[],"accountsWithoutUsage":[]}"#, "omp", 0),
            Err(UsageState::NotSignedIn { .. })
        ));
        assert!(matches!(
            parse(
                r#"{"reports":[],"accountsWithoutUsage":[{"provider":"openai"}]}"#,
                "omp",
                0
            ),
            Err(UsageState::NotSignedIn { .. })
        ));
        assert!(matches!(
            parse("garbage", "omp", 0),
            Err(UsageState::Failed { .. })
        ));
        assert!(window(&serde_json::json!({"label": "x", "amount": {"used": 5, "limit": null, "unit": "tokens"}}), "p").is_none());
    }
}
