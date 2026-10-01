//! Anthropic's usage endpoint — what a Claude Pro or Max account has left —
//! shared by every harness that signs into Anthropic with OAuth: Claude Code
//! with its own token, OpenCode with the one in its `auth.json`. Each reader
//! owns only where its token comes from; the request and the parse live here.
//!
//! **What is documented, and what is not.** Claude Code's own documentation
//! names one usage contract: the status line's `rate_limits` — a `five_hour`
//! and a `seven_day` window, each `used_percentage` (0–100) and `resets_at`,
//! either one absent when the plan has none — and, behind a Claude apps
//! gateway, a `spend_limit`. Those numbers come from the endpoint Claude
//! Code's `/usage` screen reads, `GET https://api.anthropic.com/api/oauth/usage`
//! with the beta header `anthropic-beta: oauth-2025-04-20`, which is not
//! documented: its shape is what Claude Code itself reads (2.1.263), and it
//! is read here on the same terms — by name, so a key the provider grows is
//! ignored until it is understood, never drawn as a meter it is not.
//!
//! ```json
//! { "five_hour":        { "utilization": 23.5, "resets_at": "2026-09-07T12:00:00Z" },
//!   "seven_day":        { "utilization": 41.2, "resets_at": "…" },
//!   "seven_day_opus":   { "utilization": 12.0, "resets_at": "…" },
//!   "seven_day_sonnet": null,
//!   "seven_day_oauth_apps": { "utilization": 0.0, "resets_at": "…" },
//!   "limits": [ { "kind": "weekly_scoped", "group": "weekly", "percent": 8.5, "resets_at": "…",
//!                 "scope": { "model": { "display_name": "Fable" } } } ],
//!   "extra_usage":      { "is_enabled": true, "monthly_limit": 50, "used_credits": 1.5, "utilization": 3.0 } }
//! ```
//!
//! The windows, in the order the line reads them: `five_hour` (*5h*),
//! `seven_day` (*Weekly* — every model), then **the per-model weeks the
//! server lists in `limits`** — a `weekly_scoped` entry naming a model is
//! that model's week, labelled with the model's own word (*Fable*: on a Max
//! plan or a premium seat Fable counts against the plan with up to half the
//! week usable on it; on Pro it runs on usage credits and the server lists
//! no such window) — then the older per-model keys `seven_day_opus` and
//! `seven_day_sonnet` (*Opus*, *Sonnet*), each skipped when `limits` already
//! names that model so a model is one meter. `extra_usage`, when enabled, is
//! the credit balance — set apart as an extra, not a window. Everything else
//! — `seven_day_oauth_apps` (the Claude apps' window, which Claude Code
//! never draws), `cinder_cove`, a key of tomorrow — is not read.

use bisa_harness::usage::{iso_to_unix, model_word, window_label};
use bisa_harness::{UsageReport, UsageSource, UsageState, UsageWindow};
use serde::Deserialize;

const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const OAUTH_BETA: &str = "oauth-2025-04-20";
/// Who asks — the platform, by name and version; never a pretence of being
/// another client.
const USER_AGENT: &str = concat!("bisa/", env!("CARGO_PKG_VERSION"));
const RATE_LIMITED: &str = "the usage endpoint is rate limiting reads from this machine — the last reading stays until it answers again";

/// The payload, by name. Serde reads the keys named here and no other, so
/// a key the provider adds is ignored by construction; a `null` window is
/// `None`.
#[derive(Deserialize)]
struct Payload {
    five_hour: Option<Window>,
    seven_day: Option<Window>,
    seven_day_opus: Option<Window>,
    seven_day_sonnet: Option<Window>,
    extra_usage: Option<Extra>,
    #[serde(default)]
    limits: Vec<Limit>,
}

/// One window: how much is used, and when it resets.
#[derive(Deserialize)]
struct Window {
    utilization: Option<f64>,
    resets_at: Option<String>,
}

/// The credit balance: a percentage when the server gives one, else the
/// credits used against the monthly limit.
#[derive(Deserialize)]
struct Extra {
    #[serde(default)]
    is_enabled: bool,
    monthly_limit: Option<f64>,
    used_credits: Option<f64>,
    utilization: Option<f64>,
}

/// One entry of `limits`: a per-model week when its kind and scope say so.
#[derive(Deserialize)]
struct Limit {
    #[serde(default)]
    kind: String,
    percent: Option<f64>,
    resets_at: Option<String>,
    scope: Option<LimitScope>,
}

#[derive(Deserialize)]
struct LimitScope {
    model: Option<ModelScope>,
}

#[derive(Deserialize)]
struct ModelScope {
    display_name: String,
}

/// The kind of `limits` entry that is a model's week.
const WEEKLY_SCOPED: &str = "weekly_scoped";

/// The payload as a report. Pure. `scope` names the provider on every
/// window when the harness signs into more than one (OpenCode); `None`
/// for a harness that is Anthropic's alone (Claude Code).
pub fn parse(
    text: &str,
    harness: &str,
    read_at: u64,
    scope: Option<&str>,
) -> Result<UsageReport, String> {
    let payload: Payload = serde_json::from_str(text).map_err(|e| match e.classify() {
        serde_json::error::Category::Syntax | serde_json::error::Category::Eof => {
            "the usage endpoint answered something that is not JSON".to_string()
        }
        _ => "the usage endpoint answered something that is not its usage object".to_string(),
    })?;
    let scoped = |model: Option<&str>| match (scope, model) {
        (Some(s), Some(m)) => Some(format!("{s} · {m}")),
        (Some(s), None) => Some(s.to_string()),
        (None, m) => m.map(str::to_string),
    };
    let mut windows = Vec::new();
    if let Some(w) = payload
        .five_hour
        .as_ref()
        .and_then(|w| window(w, "five_hour", &window_label(5 * 3_600), scoped(None)))
    {
        windows.push(w);
    }
    if let Some(w) = payload
        .seven_day
        .as_ref()
        .and_then(|w| window(w, "seven_day", &window_label(7 * 86_400), scoped(None)))
    {
        windows.push(w);
    }
    // The per-model weeks the server lists, in its order; then the older
    // keys for a model the list does not name.
    let mut named: Vec<String> = Vec::new();
    for limit in &payload.limits {
        if let Some(w) = model_week(limit, &scoped) {
            named.push(w.label.to_lowercase());
            windows.push(w);
        }
    }
    let older = [
        ("seven_day_opus", "opus", payload.seven_day_opus.as_ref()),
        (
            "seven_day_sonnet",
            "sonnet",
            payload.seven_day_sonnet.as_ref(),
        ),
    ];
    for (key, model, value) in older {
        if named.iter().any(|n| n == model) {
            continue;
        }
        let word = model_word(model);
        if let Some(w) = value.and_then(|w| window(w, key, &word, scoped(Some(&word)))) {
            windows.push(w);
        }
    }
    let extras: Vec<UsageWindow> = payload
        .extra_usage
        .as_ref()
        .and_then(|e| credits(e, scoped(None)))
        .into_iter()
        .collect();
    if windows.is_empty() && extras.is_empty() {
        return Err(
            "the usage endpoint reported no window — this account may have no usage limits".into(),
        );
    }
    Ok(UsageReport {
        harness: harness.to_string(),
        read_at,
        source: UsageSource::Endpoint,
        account: None,
        windows,
        extras,
    })
}

/// A `{utilization, resets_at}` window with its id and label; none without a number.
fn window(w: &Window, id: &str, label: &str, scope: Option<String>) -> Option<UsageWindow> {
    let used = w.utilization?;
    let resets_at = w.resets_at.as_deref().and_then(iso_to_unix);
    Some(UsageWindow {
        id: id.to_string(),
        label: label.to_string(),
        scope,
        used_percent: used as f32,
        resets_at,
    })
}

/// A `limits` entry as a model's week: `weekly_scoped`, naming a model, with
/// a percentage. Its id is the model's word lowercased — `weekly:fable`.
fn model_week(
    limit: &Limit,
    scoped: &dyn Fn(Option<&str>) -> Option<String>,
) -> Option<UsageWindow> {
    if limit.kind != WEEKLY_SCOPED {
        return None;
    }
    let name = limit.scope.as_ref()?.model.as_ref()?.display_name.trim();
    if name.is_empty() {
        return None;
    }
    let used = limit.percent?;
    let resets_at = limit.resets_at.as_deref().and_then(iso_to_unix);
    Some(UsageWindow {
        id: format!("weekly:{}", name.to_lowercase().replace(' ', "_")),
        label: name.to_string(),
        scope: scoped(Some(name)),
        used_percent: used as f32,
        resets_at,
    })
}

/// The credit balance when it is on: the server's percentage, else the
/// credits used against the monthly limit; none when there is no limit to
/// measure against.
fn credits(extra: &Extra, scope: Option<String>) -> Option<UsageWindow> {
    if !extra.is_enabled {
        return None;
    }
    let used = match (extra.utilization, extra.used_credits, extra.monthly_limit) {
        (Some(u), _, _) => u,
        (None, Some(used), Some(limit)) if limit > 0.0 => used / limit * 100.0,
        _ => return None,
    };
    Some(UsageWindow {
        id: "extra_usage".to_string(),
        label: "Credits".to_string(),
        scope,
        used_percent: used as f32,
        resets_at: None,
    })
}

/// Ask the endpoint with one token, which lives for this call. A 401 or 403
/// is the sign-in's fault, worded by the caller for its harness; a 429 is
/// the endpoint's, and says the last reading stays.
#[allow(
    clippy::result_large_err,
    reason = "the Err is the usage state the caller answers as it stands; boxing it would buy 136 bytes on a path run once a minute"
)]
pub async fn fetch(
    http: &bisa_http::Clients,
    token: &str,
    expired_words: &str,
) -> Result<String, UsageState> {
    let response = http
        .outbound()
        .get(USAGE_URL)
        .timeout(super::READ_TIMEOUT)
        .bearer_auth(token)
        .header("anthropic-beta", OAUTH_BETA)
        .header("accept", "application/json")
        .header("user-agent", USER_AGENT)
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                UsageState::failed("the usage endpoint did not answer in time")
            } else {
                UsageState::failed("the usage endpoint could not be reached")
            }
        })?;
    let status = response.status();
    match status.as_u16() {
        401 | 403 => return Err(UsageState::not_signed_in(expired_words)),
        429 => return Err(UsageState::failed(RATE_LIMITED)),
        code if !status.is_success() => {
            return Err(UsageState::failed(format!(
                "the usage endpoint answered {code}"
            )))
        }
        _ => {}
    }
    response
        .text()
        .await
        .map_err(|_| UsageState::failed("the usage endpoint's answer could not be read"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../tests/fixtures/usage/claude.json");

    fn rows(r: &UsageReport) -> Vec<(&str, &str, Option<&str>)> {
        r.windows
            .iter()
            .map(|w| (w.id.as_str(), w.label.as_str(), w.scope.as_deref()))
            .collect()
    }

    #[test]
    fn the_windows_are_the_session_the_week_and_the_per_model_weeks_the_server_lists() {
        let r = parse(FIXTURE, "claude-code", 7, None).unwrap();
        assert_eq!(r.source, UsageSource::Endpoint);
        assert_eq!(r.read_at, 7);
        assert_eq!(
            rows(&r),
            [
                ("five_hour", "5h", None),
                ("seven_day", "Weekly", None),
                ("weekly:fable", "Fable", Some("Fable")),
                ("seven_day_opus", "Opus", Some("Opus"))
            ],
            "the list's model week before the older keys; a null week is skipped"
        );
        assert!(
            r.windows
                .iter()
                .all(|w| !w.id.contains("oauth_apps") && !w.id.contains("cinder")),
            "the Claude apps' window and an unknown key are never meters"
        );
        assert_eq!(r.windows[0].used_percent, 23.5);
        assert_eq!(r.windows[0].resets_at, Some(1_788_782_400));
        assert_eq!(
            r.windows[2].used_percent, 8.5,
            "a model week's number is its `percent`"
        );
        assert_eq!(r.windows[2].resets_at, Some(1_789_102_800));
        let extras: Vec<(&str, &str, f32)> = r
            .extras
            .iter()
            .map(|w| (w.id.as_str(), w.label.as_str(), w.used_percent))
            .collect();
        assert_eq!(extras, [("extra_usage", "Credits", 3.0)]);
        assert!(r.account.is_none(), "the payload names no plan");
        let json = serde_json::to_string(&r).unwrap();
        assert!(
            !json.contains("token"),
            "nothing of the credential reaches the report"
        );
    }

    #[test]
    fn a_provider_scope_names_every_window_for_a_harness_that_signs_into_several() {
        let r = parse(FIXTURE, "opencode", 0, Some("anthropic")).unwrap();
        let scopes: Vec<Option<&str>> = r.windows.iter().map(|w| w.scope.as_deref()).collect();
        assert_eq!(
            scopes,
            [
                Some("anthropic"),
                Some("anthropic"),
                Some("anthropic · Fable"),
                Some("anthropic · Opus")
            ]
        );
        assert_eq!(r.extras[0].scope.as_deref(), Some("anthropic"));
    }

    #[test]
    fn a_limit_that_is_not_a_scoped_week_or_names_no_model_is_skipped_and_a_model_is_one_meter() {
        let text = r#"{
            "five_hour": {"utilization": 1, "resets_at": "2026-09-07T10:00:00Z"},
            "seven_day_opus": {"utilization": 12, "resets_at": "2026-09-07T10:00:00Z"},
            "limits": [
                {"kind": "weekly", "percent": 50},
                {"kind": "weekly_scoped", "percent": 50, "scope": {"model": null}},
                {"kind": "weekly_scoped", "percent": 50, "scope": {"model": {"display_name": "  "}}},
                {"kind": "weekly_scoped", "scope": {"model": {"display_name": "Sonnet"}}},
                {"kind": "weekly_scoped", "percent": 30.5, "resets_at": "2026-09-11T05:00:00Z", "scope": {"model": {"display_name": "Opus"}}}
            ]
        }"#;
        let r = parse(text, "claude-code", 0, None).unwrap();
        assert_eq!(
            rows(&r),
            [
                ("five_hour", "5h", None),
                ("weekly:opus", "Opus", Some("Opus"))
            ],
            "the list's Opus wins over the older key; a week without a percent is nothing"
        );
        assert_eq!(r.windows[1].used_percent, 30.5);
    }

    #[test]
    fn credits_are_the_utilization_else_used_over_the_limit_and_off_is_no_balance() {
        let with = |extra: &str| {
            parse(
                &format!(r#"{{"five_hour":{{"utilization":1}},"extra_usage":{extra}}}"#),
                "claude-code",
                0,
                None,
            )
            .unwrap()
            .extras
        };
        assert_eq!(
            with(r#"{"is_enabled":true,"monthly_limit":50,"used_credits":12.5}"#)[0].used_percent,
            25.0
        );
        assert_eq!(
            with(r#"{"is_enabled":true,"monthly_limit":50,"used_credits":12.5,"utilization":40}"#)
                [0]
            .used_percent,
            40.0,
            "the server's percentage first"
        );
        assert!(with(r#"{"is_enabled":false,"utilization":40}"#).is_empty());
        assert!(
            with(r#"{"is_enabled":true,"monthly_limit":0,"used_credits":1}"#).is_empty(),
            "no limit, no balance"
        );
        assert!(with(r#"{"is_enabled":true}"#).is_empty());
        let alone = parse(r#"{"five_hour":null,"seven_day":null,"extra_usage":{"is_enabled":true,"utilization":3}}"#, "claude-code", 0, None).unwrap();
        assert!(
            alone.windows.is_empty() && alone.extras.len() == 1,
            "credits alone are still a report"
        );
    }

    #[test]
    fn no_window_is_an_error_and_so_is_an_answer_of_another_shape() {
        assert_eq!(
            parse(
                r#"{"five_hour":null,"seven_day":null}"#,
                "claude-code",
                0,
                None
            )
            .unwrap_err(),
            "the usage endpoint reported no window — this account may have no usage limits"
        );
        assert_eq!(
            parse("not json", "claude-code", 0, None).unwrap_err(),
            "the usage endpoint answered something that is not JSON"
        );
        assert_eq!(
            parse("[]", "claude-code", 0, None).unwrap_err(),
            "the usage endpoint answered something that is not its usage object"
        );
        assert!(
            parse(
                r#"{"five_hour":{"utilization":"lots"}}"#,
                "claude-code",
                0,
                None
            )
            .is_err(),
            "a number that is not one is not read as one"
        );
        assert!(USER_AGENT.starts_with("bisa/"));
    }
}
