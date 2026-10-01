//! When a cadence next comes due: whole seconds, or a cron expression in a
//! zone. The one place the engine turns a schedule into a time — a schedule
//! start, a connector or check start's cadence, a `wait { schedule }`.
//!
//! A cron expression is the five-field crontab a person knows (a seconds
//! field is added, and crontab's weekday numbers moved onto the `cron`
//! crate's — [`bisa_store::normalize_cron`]); six or seven fields are the
//! crate's own.

use bisa_core::Cadence;
use bisa_store::normalize_cron;
use chrono::{DateTime, FixedOffset, Local, Utc};

/// The next time a cadence is due, strictly after `after` (unix seconds). A
/// zero interval, a cron expression that does not parse, a zone nobody knows
/// and a cron with no future occurrence are each an error naming what is
/// wrong — a schedule that silently never comes due is the worst answer.
pub fn next_due(cadence: &Cadence, after: u64) -> Result<u64, String> {
    match cadence {
        Cadence::Every { secs } => {
            if *secs == 0 {
                return Err("an interval must be at least one second".into());
            }
            Ok(after.saturating_add(*secs))
        }
        Cadence::Cron { expr, tz } => cron_next(expr, tz.as_deref(), after),
    }
}

/// The zones a cron schedule can name.
enum Zone {
    Utc,
    Local,
    Fixed(FixedOffset),
    /// An IANA zone (`Europe/Paris`), resolved through `chrono-tz` so daylight
    /// saving is handled by the tz database rather than by us.
    Named(chrono_tz::Tz),
}

/// Parse `tz`.
///
/// `None` is **UTC**, not the host's zone: a schedule that means something
/// different depending on which machine restarted the node is a bug waiting
/// for a daylight-saving change. `local` opts into the host zone explicitly,
/// and a fixed offset (`+02:00`, `-0500`, `Z`) is exact.
///
/// An IANA name (`Europe/Paris`) resolves through the tz database, which is
/// what "every weekday at 9am" means to a person: it must stay 9am across a
/// daylight-saving change, and a fixed offset cannot do that. Anything
/// unrecognised is refused loudly rather than guessed at — the alternative is
/// a schedule an hour off twice a year and nobody knows why.
fn zone(tz: Option<&str>) -> Result<Zone, String> {
    let Some(tz) = tz.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(Zone::Utc);
    };
    match tz.to_ascii_lowercase().as_str() {
        "utc" | "z" | "gmt" => return Ok(Zone::Utc),
        "local" | "localtime" | "system" => return Ok(Zone::Local),
        _ => {}
    }
    if let Some(offset) = parse_offset(tz) {
        return Ok(Zone::Fixed(offset));
    }
    // Case-sensitive by design: the tz database is, and silently accepting
    // `europe/paris` would teach a spelling that breaks on another tool.
    if let Ok(named) = tz.parse::<chrono_tz::Tz>() {
        return Ok(Zone::Named(named));
    }
    Err(format!(
        "unsupported timezone {tz:?}: use an IANA name like `Europe/Paris`, UTC (the default), \
         `local`, or a fixed offset like +02:00"
    ))
}

/// `+02:00`, `-0500`, `+02` — the forms a person types.
fn parse_offset(s: &str) -> Option<FixedOffset> {
    let (sign, rest) = match s.split_at_checked(1)? {
        ("+", rest) => (1, rest),
        ("-", rest) => (-1, rest),
        _ => return None,
    };
    let (h, m) = match rest.split_once(':') {
        Some((h, m)) => (h, m),
        None => match rest.len() {
            1 | 2 => (rest, "0"),
            4 => rest.split_at(2),
            _ => return None,
        },
    };
    let hours: i32 = h.parse().ok()?;
    let minutes: i32 = m.parse().ok()?;
    if !(0..=14).contains(&hours) || !(0..60).contains(&minutes) {
        return None;
    }
    FixedOffset::east_opt(sign * (hours * 3600 + minutes * 60))
}

/// The next occurrence of `expr` in `tz` strictly after `after`.
pub fn cron_next(expr: &str, tz: Option<&str>, after: u64) -> Result<u64, String> {
    use std::str::FromStr;
    let schedule = cron::Schedule::from_str(&normalize_cron(expr))
        .map_err(|e| format!("bad cron expression {expr:?}: {e}"))?;
    let after = DateTime::<Utc>::from_timestamp(after as i64, 0)
        .ok_or_else(|| format!("{after} is not a representable time"))?;
    let next = match zone(tz)? {
        Zone::Utc => schedule.after(&after).next().map(|d| d.timestamp()),
        Zone::Local => schedule
            .after(&after.with_timezone(&Local))
            .next()
            .map(|d| d.timestamp()),
        Zone::Fixed(off) => schedule
            .after(&after.with_timezone(&off))
            .next()
            .map(|d| d.timestamp()),
        Zone::Named(tz) => schedule
            .after(&after.with_timezone(&tz))
            .next()
            .map(|d| d.timestamp()),
    };
    // A cron expression can name a year that has passed; it then has no next
    // occurrence at all, which is a configuration error, not a quiet schedule.
    next.map(|t| t.max(0) as u64)
        .ok_or_else(|| format!("cron expression {expr:?} has no future occurrence"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cron(expr: &str, tz: Option<&str>) -> Cadence {
        Cadence::Cron {
            expr: expr.into(),
            tz: tz.map(str::to_string),
        }
    }

    #[test]
    fn cron_accepts_five_and_six_field_expressions() {
        // 2024-01-01 00:00:00 UTC
        let base = 1_704_067_200;
        let five = next_due(&cron("0 9 * * *", None), base).unwrap();
        let six = next_due(&cron("0 0 9 * * *", None), base).unwrap();
        assert_eq!(five, six);
        assert_eq!(five, base + 9 * 3600);
    }

    /// 2024-01-01 was a Monday: crontab's `1` is that very morning, and its
    /// `0` the Sunday after — what a person writing a crontab means.
    #[test]
    fn a_crontab_weekday_is_the_day_crontab_means() {
        let monday = 1_704_067_200;
        let day = 24 * 3600;
        assert_eq!(
            next_due(&cron("0 9 * * 1", None), monday).unwrap(),
            monday + 9 * 3600
        );
        assert_eq!(
            next_due(&cron("0 9 * * MON", None), monday).unwrap(),
            monday + 9 * 3600
        );
        assert_eq!(
            next_due(&cron("0 9 * * 0", None), monday).unwrap(),
            monday + 6 * day + 9 * 3600
        );
        assert_eq!(
            next_due(&cron("0 9 * * 1-5", None), monday + 10 * 3600).unwrap(),
            monday + day + 9 * 3600,
            "the working week: Tuesday next"
        );
    }

    #[test]
    fn cron_honours_a_fixed_offset() {
        let base = 1_704_067_200; // 2024-01-01 00:00 UTC
        let utc = next_due(&cron("0 9 * * *", Some("UTC")), base).unwrap();
        let paris = next_due(&cron("0 9 * * *", Some("+01:00")), base).unwrap();
        assert_eq!(utc - paris, 3600, "09:00+01:00 is an hour before 09:00Z");
    }

    /// The whole reason a named zone is worth a dependency: "every weekday at
    /// 9am" has to stay 9am across a daylight-saving change, which no fixed
    /// offset can do.
    #[test]
    fn a_named_zone_follows_daylight_saving() {
        let daily = |tz: &str, after: u64| next_due(&cron("0 9 * * *", Some(tz)), after).unwrap();
        // Winter: Paris is UTC+1, so 09:00 local is 08:00Z.
        let winter = 1_704_067_200; // 2024-01-01 00:00 UTC
        assert_eq!(daily("UTC", winter) - daily("Europe/Paris", winter), 3600);
        // Summer: Paris is UTC+2 — the offset moved, the wall clock did not.
        let summer = 1_720_396_800; // 2024-07-08 00:00 UTC
        assert_eq!(daily("UTC", summer) - daily("Europe/Paris", summer), 7200);
        // A fixed offset cannot do that: it is the same distance year-round.
        assert_eq!(daily("UTC", winter) - daily("+01:00", winter), 3600);
        assert_eq!(daily("UTC", summer) - daily("+01:00", summer), 3600);
    }

    #[test]
    fn an_unknown_zone_is_refused_rather_than_guessed() {
        for bad in ["Mars/Olympus", "europe/paris", "PST", "+99:00"] {
            let e = next_due(&cron("0 9 * * *", Some(bad)), 0).unwrap_err();
            assert!(
                e.contains("unsupported timezone"),
                "{bad} should be refused, got: {e}"
            );
        }
    }

    #[test]
    fn a_bad_cadence_is_an_error_not_a_silent_never() {
        assert!(next_due(&cron("not a cron", None), 0).is_err());
        assert!(next_due(&Cadence::Every { secs: 0 }, 0).is_err());
        assert_eq!(next_due(&Cadence::Every { secs: 30 }, 100).unwrap(), 130);
    }

    #[test]
    fn offsets_parse_in_the_shapes_people_type() {
        for (s, secs) in [
            ("+02:00", 7200),
            ("-05:00", -18000),
            ("+0200", 7200),
            ("-0530", -19800),
            ("+2", 7200),
        ] {
            assert_eq!(parse_offset(s).unwrap().local_minus_utc(), secs, "{s}");
        }
        for bad in ["02:00", "+25:00", "+02:99", "", "+", "abc"] {
            assert!(parse_offset(bad).is_none(), "{bad}");
        }
    }
}
