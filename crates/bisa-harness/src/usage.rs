//! What a harness's **account** has left — its usage limits, as the harness
//! itself reports them: Claude Code's five-hour and weekly windows and the
//! per-model week its plan carries, Codex's primary and secondary, every
//! provider OMP is signed into. *Usage* is the
//! account's limit and how much of each window is used; *cost* (`SessionCost`)
//! stays what one session spent. A harness that reports no such thing says
//! so in its own sentence ([`UsageState::Unsupported`]) — pi exposes session
//! tokens only; OpenCode reports through the Claude Pro/Max sign-in it keeps.
//!
//! Reports are read on demand by the engine and cached; every reader lives
//! in `bisa-adapters` as a pure parser over the harness's own payload
//! plus one thin fetcher. **Nothing here carries a credential**: a report is
//! percentages, labels and reset times, safe on any wire.

use serde::{Deserialize, Serialize};

/// One limit window: how much of it is used, and when it resets.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UsageWindow {
    /// The harness's own name for the window — `five_hour`, `weekly:fable`,
    /// `seven_day_opus`, `primary`, `anthropic:weekly` — stable for a row's key.
    pub id: String,
    /// The word a person reads: *5h*, *Weekly*, or a model's own word for its
    /// week — *Fable*, *Opus*.
    pub label: String,
    /// The provider or model the window belongs to, when the harness reports
    /// more than one (OMP's providers; Claude's per-model weeks — a model
    /// window's scope is its model's word).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    /// 0–100; a spend limit may pass 100.
    pub used_percent: f32,
    /// Unix seconds when the window resets, when the harness knows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resets_at: Option<u64>,
}

/// The account behind the report — never a secret, never an id a token could
/// be derived from.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UsageAccount {
    /// The plan's word — *max*, *pro*, *plus* — as the harness gives it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan: Option<String>,
    /// The login or email the harness is signed in as.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub login: Option<String>,
}

/// Where a report came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum UsageSource {
    /// The provider's usage endpoint, with the harness's own sign-in.
    Endpoint,
    /// The harness's app server, asked over its protocol.
    AppServer,
    /// The harness's CLI, asked for JSON.
    Cli,
}

/// One harness account's usage, read at one moment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UsageReport {
    pub harness: String,
    /// Unix seconds the report was read.
    pub read_at: u64,
    pub source: UsageSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<UsageAccount>,
    /// The limit windows, in the harness's order — the short one first.
    pub windows: Vec<UsageWindow>,
    /// Beyond the windows: an extra-usage or credit balance, when reported.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extras: Vec<UsageWindow>,
}

/// What asking a harness for its usage answered.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum UsageState {
    /// The report.
    Report { report: UsageReport },
    /// This harness reports no usage limits; `reason` is its own sentence.
    Unsupported { reason: String },
    /// The harness could report, but is not signed in here.
    NotSignedIn { reason: String },
    /// Reads are off — `harness.usage.reads` — so nothing was asked.
    Off,
    /// The read failed; `reason` never quotes what came back.
    Failed { reason: String },
}

impl UsageState {
    /// The default answer for a harness that exposes no limits, in its own name.
    pub fn unsupported(display_name: &str) -> Self {
        Self::Unsupported {
            reason: format!("{display_name} reports no usage limits — its provider does."),
        }
    }

    pub fn not_signed_in(reason: impl Into<String>) -> Self {
        Self::NotSignedIn {
            reason: reason.into(),
        }
    }

    pub fn failed(reason: impl Into<String>) -> Self {
        Self::Failed {
            reason: reason.into(),
        }
    }

    /// Whether the answer is worth keeping for a while: a report, an honest
    /// *unsupported*, a missing sign-in. A failure is asked again next time.
    pub fn cacheable(&self) -> bool {
        !matches!(self, Self::Failed { .. } | Self::Off)
    }
}

/// The word for a window's length — *5h*, *Daily*, *Weekly*, *Monthly*; else
/// the hours or days as a number. Every adapter labels its windows through
/// this, so a five-hour window reads the same for every harness.
pub fn window_label(secs: u64) -> String {
    const HOUR: u64 = 3_600;
    const DAY: u64 = 24 * HOUR;
    match secs {
        0 => "Now".to_string(),
        s if s == 5 * HOUR => "5h".to_string(),
        s if s == DAY => "Daily".to_string(),
        s if s == 7 * DAY => "Weekly".to_string(),
        s if (28 * DAY..=31 * DAY).contains(&s) => "Monthly".to_string(),
        s if s >= 2 * DAY => format!("{} d", s / DAY),
        s if s >= HOUR => format!("{} h", s / HOUR),
        s => format!("{} min", s.max(60) / 60),
    }
}

/// The window closest to its limit — the one a person watches.
pub fn tightest(windows: &[UsageWindow]) -> Option<&UsageWindow> {
    windows.iter().max_by(|a, b| {
        a.used_percent
            .partial_cmp(&b.used_percent)
            .unwrap_or(std::cmp::Ordering::Equal)
    })
}

/// A model word for a label: `opus` → *Opus*, `fable` → *Fable*.
pub fn model_word(model: &str) -> String {
    let mut chars = model.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// An RFC 3339 timestamp (`2026-09-07T10:00:00Z`, `…+02:00`, with or without
/// fractional seconds) to unix seconds; `None` for anything else. Small on
/// purpose: the usage payloads carry one shape of timestamp and this crate
/// carries no calendar dependency.
pub fn iso_to_unix(iso: &str) -> Option<u64> {
    let s = iso.trim();
    let (date, rest) = s.split_once('T')?;
    let mut parts = date.split('-');
    let year: i64 = parts.next()?.parse().ok()?;
    let month: u32 = parts.next()?.parse().ok()?;
    let day: u32 = parts.next()?.parse().ok()?;
    if parts.next().is_some() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    // The clock, then the offset that follows it.
    let split = rest.find(['Z', 'z', '+', '-']).unwrap_or(rest.len());
    let (clock, zone) = rest.split_at(split);
    let clock = clock.split('.').next()?;
    let mut hms = clock.split(':');
    let hour: i64 = hms.next()?.parse().ok()?;
    let minute: i64 = hms.next()?.parse().ok()?;
    let second: i64 = hms.next().unwrap_or("0").parse().ok()?;
    if hms.next().is_some() || hour > 23 || minute > 59 || second > 60 {
        return None;
    }
    let offset: i64 = match zone {
        "" | "Z" | "z" => 0,
        z => {
            let sign = if z.starts_with('-') { -1 } else { 1 };
            let (oh, om) = z[1..].split_once(':').unwrap_or((&z[1..], "0"));
            sign * (oh.parse::<i64>().ok()? * 3_600 + om.parse::<i64>().ok()? * 60)
        }
    };
    let days = days_from_civil(year, month, day);
    let secs = days * 86_400 + hour * 3_600 + minute * 60 + second - offset;
    u64::try_from(secs).ok()
}

/// Days since 1970-01-01 for a civil date (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (i64::from(m) + 9) % 12;
    let doy = (153 * mp + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Unix seconds now.
pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(id: &str, used: f32) -> UsageWindow {
        UsageWindow {
            id: id.into(),
            label: id.into(),
            scope: None,
            used_percent: used,
            resets_at: None,
        }
    }

    #[test]
    fn a_window_length_has_its_word() {
        assert_eq!(window_label(5 * 3_600), "5h");
        assert_eq!(window_label(86_400), "Daily");
        assert_eq!(window_label(7 * 86_400), "Weekly");
        assert_eq!(window_label(30 * 86_400), "Monthly");
        assert_eq!(window_label(31 * 86_400), "Monthly");
        assert_eq!(window_label(3 * 86_400), "3 d");
        assert_eq!(window_label(2 * 3_600), "2 h");
        assert_eq!(window_label(90), "1 min");
    }

    #[test]
    fn the_tightest_window_is_the_one_closest_to_its_limit() {
        let w = [window("a", 12.0), window("b", 87.5), window("c", 40.0)];
        assert_eq!(tightest(&w).map(|x| x.id.as_str()), Some("b"));
        assert!(tightest(&[]).is_none());
    }

    #[test]
    fn a_model_word_is_capitalised_and_an_unsupported_answer_names_the_harness() {
        assert_eq!(model_word("opus"), "Opus");
        assert_eq!(model_word("fable"), "Fable");
        assert_eq!(model_word(""), "");
        match UsageState::unsupported("pi") {
            UsageState::Unsupported { reason } => {
                assert_eq!(reason, "pi reports no usage limits — its provider does.")
            }
            other => panic!("{other:?}"),
        }
        assert!(UsageState::unsupported("x").cacheable());
        assert!(
            !UsageState::failed("x").cacheable(),
            "a failure is asked again"
        );
        assert!(!UsageState::Off.cacheable());
    }

    #[test]
    fn iso_timestamps_become_unix_seconds() {
        assert_eq!(iso_to_unix("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(iso_to_unix("2026-09-07T10:00:00Z"), Some(1_788_775_200));
        assert_eq!(iso_to_unix("2026-09-07T10:00:00.123Z"), Some(1_788_775_200));
        assert_eq!(
            iso_to_unix("2026-09-07T12:00:00+02:00"),
            Some(1_788_775_200),
            "an offset is applied"
        );
        assert_eq!(
            iso_to_unix("2026-09-07T08:00:00-02:00"),
            Some(1_788_775_200)
        );
        assert_eq!(iso_to_unix("not a date"), None);
        assert_eq!(iso_to_unix("2026-13-01T00:00:00Z"), None);
    }

    #[test]
    fn a_state_serialises_tagged_and_a_report_carries_no_secret_field() {
        let state = UsageState::Report {
            report: UsageReport {
                harness: "claude-code".into(),
                read_at: 1,
                source: UsageSource::Endpoint,
                account: Some(UsageAccount {
                    plan: Some("max".into()),
                    login: None,
                }),
                windows: vec![window("five_hour", 23.5)],
                extras: vec![],
            },
        };
        let json = serde_json::to_value(&state).unwrap();
        assert_eq!(json["state"], "report");
        assert_eq!(json["report"]["source"], "endpoint");
        assert_eq!(json["report"]["windows"][0]["used_percent"], 23.5);
        assert!(
            json["report"].get("extras").is_none(),
            "empty extras are omitted"
        );
        assert_eq!(
            serde_json::to_value(UsageState::Off).unwrap()["state"],
            "off"
        );
    }
}
