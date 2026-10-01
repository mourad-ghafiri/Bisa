//! One retry policy for every call. A read is retried on what the network
//! and the service may recover from; a write only where the platform says a
//! retry is safe — a rate limit, a gateway that never saw the request — so
//! a message is never posted twice because the first answer was lost.

use crate::http::{Response, TransportError};
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RetryPolicy {
    /// Attempts in all, the first included.
    pub attempts: u8,
    /// The first back-off, doubled per attempt.
    pub base: Duration,
    /// The longest computed back-off.
    pub cap: Duration,
    /// The longest `Retry-After` honoured; a longer one is capped, not obeyed.
    pub retry_after_cap: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            attempts: 3,
            base: Duration::from_millis(500),
            cap: Duration::from_secs(8),
            retry_after_cap: Duration::from_secs(30),
        }
    }
}

impl RetryPolicy {
    /// No second attempt, ever.
    pub fn none() -> Self {
        Self {
            attempts: 1,
            ..Self::default()
        }
    }
}

/// What to do after one attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Next {
    Done,
    RetryAfter(Duration),
}

/// Whether the attempt's result is worth another try, and how long to wait.
/// `attempt` is zero-based; `retry_after` is the service's own wish when it
/// sent one; `jitter` is a fraction in `0..1` from the entropy source.
pub fn decide(
    policy: &RetryPolicy,
    attempt: u8,
    result: &Result<Response, TransportError>,
    writes: bool,
    retry_after: Option<Duration>,
    jitter: f64,
) -> Next {
    if attempt + 1 >= policy.attempts {
        return Next::Done;
    }
    let again = match result {
        Err(TransportError::Connect(_)) | Err(TransportError::Timeout) => !writes,
        Err(TransportError::Body(_)) => false,
        Ok(r) => match r.status {
            429 | 502 | 503 | 504 => true,
            500 => !writes,
            _ => false,
        },
    };
    if !again {
        return Next::Done;
    }
    let wait = match retry_after {
        Some(d) => d.min(policy.retry_after_cap),
        None => {
            let doubled = policy.base.saturating_mul(1u32 << attempt.min(16));
            let base = doubled.min(policy.cap);
            let factor = 0.75 + 0.5 * jitter.clamp(0.0, 1.0);
            base.mul_f64(factor)
        }
    };
    Next::RetryAfter(wait)
}

/// The `Retry-After` header as a wait: integer seconds, or an HTTP date
/// minus `now`. Anything else is nothing.
pub fn retry_after(headers: &[(String, String)], now: u64) -> Option<Duration> {
    let value = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("retry-after"))
        .map(|(_, v)| v.trim())?;
    if let Ok(secs) = value.parse::<u64>() {
        return Some(Duration::from_secs(secs));
    }
    let at = http_date(value)?;
    Some(Duration::from_secs(at.saturating_sub(now)))
}

/// An IMF-fixdate (`Sun, 06 Nov 1994 08:49:37 GMT`) as unix seconds.
pub fn http_date(text: &str) -> Option<u64> {
    let mut parts = text.split_whitespace();
    let _weekday = parts.next()?;
    let day: u64 = parts.next()?.parse().ok()?;
    let month = match parts.next()? {
        "Jan" => 1,
        "Feb" => 2,
        "Mar" => 3,
        "Apr" => 4,
        "May" => 5,
        "Jun" => 6,
        "Jul" => 7,
        "Aug" => 8,
        "Sep" => 9,
        "Oct" => 10,
        "Nov" => 11,
        "Dec" => 12,
        _ => return None,
    };
    let year: u64 = parts.next()?.parse().ok()?;
    let mut hms = parts.next()?.split(':');
    let h: u64 = hms.next()?.parse().ok()?;
    let m: u64 = hms.next()?.parse().ok()?;
    let s: u64 = hms.next()?.parse().ok()?;
    if parts.next()? != "GMT" || day == 0 || day > 31 || h > 23 || m > 59 || s > 60 {
        return None;
    }
    Some(days_from_civil(year, month, day) * 86_400 + h * 3600 + m * 60 + s)
}

/// Days since 1970-01-01 for a proleptic Gregorian date (Howard Hinnant's algorithm).
fn days_from_civil(y: u64, m: u64, d: u64) -> u64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resp(status: u16) -> Result<Response, TransportError> {
        Ok(Response {
            status,
            headers: vec![],
            body: vec![],
        })
    }

    #[test]
    fn reads_retry_on_more_than_writes_do() {
        let p = RetryPolicy::default();
        assert!(matches!(
            decide(&p, 0, &resp(429), true, None, 0.5),
            Next::RetryAfter(_)
        ));
        assert!(matches!(
            decide(&p, 0, &resp(503), true, None, 0.5),
            Next::RetryAfter(_)
        ));
        assert!(matches!(
            decide(&p, 0, &resp(500), false, None, 0.5),
            Next::RetryAfter(_)
        ));
        assert_eq!(decide(&p, 0, &resp(500), true, None, 0.5), Next::Done);
        assert!(matches!(
            decide(&p, 0, &Err(TransportError::Timeout), false, None, 0.5),
            Next::RetryAfter(_)
        ));
        assert_eq!(
            decide(&p, 0, &Err(TransportError::Timeout), true, None, 0.5),
            Next::Done
        );
        assert_eq!(decide(&p, 0, &resp(400), false, None, 0.5), Next::Done);
        assert_eq!(decide(&p, 0, &resp(200), false, None, 0.5), Next::Done);
        assert_eq!(
            decide(&p, 2, &resp(429), false, None, 0.5),
            Next::Done,
            "the last attempt is the last"
        );
    }

    #[test]
    fn the_wait_honours_retry_after_within_its_cap_and_backs_off_with_jitter() {
        let p = RetryPolicy::default();
        assert_eq!(
            decide(&p, 0, &resp(429), false, Some(Duration::from_secs(3)), 0.0),
            Next::RetryAfter(Duration::from_secs(3))
        );
        assert_eq!(
            decide(
                &p,
                0,
                &resp(429),
                false,
                Some(Duration::from_secs(3600)),
                0.0
            ),
            Next::RetryAfter(Duration::from_secs(30))
        );
        assert_eq!(
            decide(&p, 0, &resp(503), false, None, 0.5),
            Next::RetryAfter(Duration::from_millis(500))
        );
        assert_eq!(
            decide(&p, 1, &resp(503), false, None, 0.0),
            Next::RetryAfter(Duration::from_millis(750))
        );
        let capped = RetryPolicy {
            attempts: 10,
            ..RetryPolicy::default()
        };
        assert_eq!(
            decide(&capped, 8, &resp(503), false, None, 0.5),
            Next::RetryAfter(Duration::from_secs(8))
        );
    }

    #[test]
    fn retry_after_reads_seconds_and_http_dates() {
        let secs = vec![("Retry-After".to_string(), "7".to_string())];
        assert_eq!(retry_after(&secs, 0), Some(Duration::from_secs(7)));
        let dated = vec![(
            "retry-after".to_string(),
            "Sun, 06 Nov 1994 08:49:37 GMT".to_string(),
        )];
        assert_eq!(
            http_date("Sun, 06 Nov 1994 08:49:37 GMT"),
            Some(784_111_777)
        );
        assert_eq!(
            retry_after(&dated, 784_111_770),
            Some(Duration::from_secs(7))
        );
        assert_eq!(
            retry_after(&dated, 784_111_800),
            Some(Duration::ZERO),
            "a date in the past is no wait"
        );
        let junk = vec![("retry-after".to_string(), "soon".to_string())];
        assert_eq!(retry_after(&junk, 0), None);
        assert_eq!(http_date("Thu, 01 Jan 1970 00:00:00 GMT"), Some(0));
    }
}
