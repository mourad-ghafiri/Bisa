//! Time as the files spell it: RFC 3339 on a line and in a report, and the
//! compact UTC stamp a report's file name carries (`20260911T102233Z`), so
//! the names sort by time and a person reads the moment off one.

use std::time::SystemTime;

use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

/// `2026-09-11T10:22:33.123456Z`.
pub fn rfc3339(at: SystemTime) -> String {
    OffsetDateTime::from(at)
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

/// `20260911T102233Z` — the file name's stamp.
pub fn name_stamp(at: SystemTime) -> String {
    let t = OffsetDateTime::from(at);
    format!(
        "{:04}{:02}{:02}T{:02}{:02}{:02}Z",
        t.year(),
        u8::from(t.month()),
        t.day(),
        t.hour(),
        t.minute(),
        t.second()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn the_two_spellings_agree_on_the_moment() {
        let at = SystemTime::UNIX_EPOCH + Duration::from_secs(1_789_000_000);
        assert_eq!(name_stamp(at), "20260910T002640Z");
        assert_eq!(rfc3339(at), "2026-09-10T00:26:40Z");
        assert!(crate::is_crash_name(&format!(
            "node.{}.1.json",
            name_stamp(at)
        )));
    }
}
