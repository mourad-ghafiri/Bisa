//! The two syntaxes a definition embeds that the core does not parse: a cron
//! expression and a JSON Schema. The core states the seam
//! ([`bisa_core::SyntaxChecks`]) and stays free of both crates; this is
//! the store's implementation with the same crates the engine schedules and
//! checks with, so what validation accepts is exactly what a run can execute.

use bisa_core::SyntaxChecks;
use std::str::FromStr;

/// A cron expression as the `cron` crate reads it, from the one a person
/// writes. The crate wants a seconds field first and numbers the weekdays
/// 1–7 from Sunday; humans write the 5-field crontab they know, whose
/// weekdays run 0–6 from Sunday (7 is Sunday again). So a 5-field
/// expression gets a leading `0` for seconds and its weekday numbers moved
/// onto the crate's — `0 9 * * 1` is Monday at nine, as crontab means it,
/// not the crate's Sunday. Names (`MON-FRI`) read the same in both and pass
/// through. A 6- or 7-field expression is the crate's own and is left alone.
pub fn normalize_cron(expr: &str) -> String {
    let fields: Vec<&str> = expr.split_whitespace().collect();
    match fields.as_slice() {
        [minute, hour, day, month, weekday] => {
            format!(
                "0 {minute} {hour} {day} {month} {}",
                crontab_weekdays(weekday)
            )
        }
        _ => expr.trim().to_string(),
    }
}

/// A crontab day-of-week field on the crate's numbering, item by item: a
/// number, a range or a stepped range of numbers becomes the list of the
/// days it names, each moved from crontab's `0–7` (Sunday 0 and 7) to the
/// crate's `1–7` (Sunday 1). `*` and `*/n` name the same days in both, and a
/// name, `?` or anything malformed is left as written — the crate then
/// accepts it or says why not.
fn crontab_weekdays(field: &str) -> String {
    field
        .split(',')
        .map(crontab_weekday_item)
        .collect::<Vec<_>>()
        .join(",")
}

fn crontab_weekday_item(item: &str) -> String {
    let day = |s: &str| s.parse::<u8>().ok().filter(|n| *n <= 7);
    let (range, step) = match item.split_once('/') {
        Some((range, step)) => match step.parse::<u8>() {
            Ok(n) if n > 0 => (range, n),
            _ => return item.to_string(),
        },
        None => (item, 1),
    };
    let (from, to) = match range.split_once('-') {
        Some((a, b)) => match (day(a), day(b)) {
            (Some(a), Some(b)) if a <= b => (a, b),
            _ => return item.to_string(),
        },
        // `n/step` runs from `n` to crontab's last weekday.
        None if step > 1 => match day(range) {
            Some(a) => (a, 6),
            None => return item.to_string(),
        },
        None => match day(range) {
            Some(a) => (a, a),
            None => return item.to_string(),
        },
    };
    let mut days: Vec<u8> = (from..=to)
        .step_by(usize::from(step))
        .map(|n| if n == 7 { 1 } else { n + 1 })
        .collect();
    days.sort_unstable();
    days.dedup();
    days.iter().map(u8::to_string).collect::<Vec<_>>().join(",")
}

/// The store's [`SyntaxChecks`]: a cron expression must parse, and a schema
/// must compile.
#[derive(Clone, Copy, Debug, Default)]
pub struct StoreSyntaxChecks;

impl SyntaxChecks for StoreSyntaxChecks {
    fn cron_error(&self, expr: &str) -> Option<String> {
        match cron::Schedule::from_str(&normalize_cron(expr)) {
            Ok(_) => None,
            Err(e) => Some(format!("{expr:?} is not a cron expression: {e}")),
        }
    }

    fn schema_error(&self, schema: &serde_json::Value) -> Option<String> {
        match jsonschema::validator_for(schema) {
            Ok(_) => None,
            Err(e) => Some(e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn five_field_cron_gets_its_seconds_and_six_field_is_left_alone() {
        assert_eq!(normalize_cron("0 9 * * *"), "0 0 9 * * *");
        assert_eq!(normalize_cron("0 9 * * 1-5"), "0 0 9 * * 2,3,4,5,6");
        assert_eq!(
            normalize_cron("  30 0 9 * * 1-5 "),
            "30 0 9 * * 1-5",
            "the crate's own six fields are its own numbering"
        );
        let checks = StoreSyntaxChecks;
        assert_eq!(checks.cron_error("0 9 * * 1-5"), None);
        assert!(checks.cron_error("every morning").is_some());
        assert!(checks.cron_error("").is_some());
    }

    /// Crontab counts the weekdays from Sunday = 0 (and 7); the `cron` crate
    /// from Sunday = 1. What a person writes means what crontab means.
    #[test]
    fn a_crontab_weekday_means_what_crontab_means() {
        assert_eq!(crontab_weekdays("1"), "2", "Monday");
        assert_eq!(crontab_weekdays("0"), "1", "Sunday");
        assert_eq!(crontab_weekdays("7"), "1", "Sunday again");
        assert_eq!(crontab_weekdays("6"), "7", "Saturday");
        assert_eq!(crontab_weekdays("0,6"), "1,7", "the weekend");
        assert_eq!(crontab_weekdays("1-7"), "1,2,3,4,5,6,7");
        assert_eq!(crontab_weekdays("5-7"), "1,6,7", "Friday to Sunday");
        assert_eq!(crontab_weekdays("0-6/2"), "1,3,5,7");
        assert_eq!(
            crontab_weekdays("1/2"),
            "2,4,6",
            "Monday, then every other day"
        );
        assert_eq!(crontab_weekdays("*"), "*");
        assert_eq!(crontab_weekdays("*/2"), "*/2", "the same days in both");
        assert_eq!(crontab_weekdays("MON-FRI"), "MON-FRI");
        assert_eq!(crontab_weekdays("MON,3"), "MON,4", "a name beside a number");
        assert_eq!(crontab_weekdays("8"), "8", "out of range is left to refuse");
        assert_eq!(crontab_weekdays("5-2"), "5-2", "a reversed range too");
        assert!(StoreSyntaxChecks.cron_error("0 9 * * 8").is_some());
    }

    /// Read back through the crate itself: the days it schedules are the
    /// days the crontab named.
    #[test]
    fn the_crate_schedules_the_crontab_days() {
        use cron::TimeUnitSpec;
        let days = |expr: &str| -> Vec<u32> {
            let schedule = cron::Schedule::from_str(&normalize_cron(expr)).unwrap();
            (1..=7)
                .filter(|d| schedule.days_of_week().includes(*d))
                .collect()
        };
        // The crate's ordinals: Sunday 1, Monday 2, … Saturday 7.
        assert_eq!(days("0 9 * * 1"), vec![2], "Monday");
        assert_eq!(days("0 9 * * MON"), vec![2], "Monday by name");
        assert_eq!(days("0 9 * * 1-5"), vec![2, 3, 4, 5, 6], "the working week");
        assert_eq!(days("0 9 * * 0"), vec![1], "Sunday");
        assert_eq!(days("0 9 * * *").len(), 7);
    }

    #[test]
    fn a_schema_must_compile() {
        let checks = StoreSyntaxChecks;
        assert_eq!(
            checks.schema_error(&json!({"type": "object", "required": ["ok"]})),
            None
        );
        assert!(checks
            .schema_error(&json!({"type": "not-a-type"}))
            .is_some());
        assert!(checks.schema_error(&json!({"required": "ok"})).is_some());
    }

    // added by the coverage pass: syntax.rs

    #[test]
    fn a_weekday_step_that_is_no_number_or_zero_leaves_the_item_as_it_was() {
        assert_eq!(crontab_weekday_item("1-5/0"), "1-5/0");
        assert_eq!(crontab_weekday_item("1-5/x"), "1-5/x");
    }
}
