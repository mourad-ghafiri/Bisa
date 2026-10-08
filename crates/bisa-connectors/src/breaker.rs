//! A circuit per host: a platform that has failed five calls in a row is not
//! asked a sixth time for thirty seconds. A `for_each` fanning twenty calls at
//! a host that is down would otherwise spend twenty deadlines and sixty
//! retries learning the same fact; the breaker learns it once and says so —
//! [`ConnectorError::Open`], a transient the person can read — and lets one
//! probe through when the pause ends, closing again on its success.
//!
//! What counts as a failure is the platform's or the network's: a transport
//! error, a timeout, a 5xx. A refusal — a 4xx, a bad parameter, a host the
//! policy denies — is the caller's and moves nothing here. A probe whose
//! call is dropped before it answers — a stopped run, a deadline above the
//! call — is [`Breaker::abandon`]ed, so the next call after the pause is the
//! probe instead of waiting on one nobody is making. The clock is the
//! client's [`crate::creds::Clock`], so a test turns it by hand.

use crate::error::ConnectorError;
use std::collections::HashMap;
use std::sync::Mutex;

/// Consecutive failures that open the circuit.
pub const OPEN_AFTER: u32 = 5;
/// How long an open circuit refuses before it lets one probe through.
pub const COOLDOWN_SECS: u64 = 30;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct State {
    failures: u32,
    /// Unix seconds until which calls are refused; `None` while closed.
    open_until: Option<u64>,
    /// The one call let through after the pause, whose answer decides.
    probing: bool,
}

/// The circuits, by host — `host[:port]` as the URL carries it.
#[derive(Debug, Default)]
pub struct Breaker {
    hosts: Mutex<HashMap<String, State>>,
}

impl Breaker {
    /// May a call go to `host` now? Closed: yes. Open and the pause not over,
    /// or a probe already out: no. Pause over: this call is the probe.
    pub fn admit(&self, host: &str, now: u64) -> Result<(), ConnectorError> {
        let mut hosts = self.hosts.lock().unwrap_or_else(|p| p.into_inner());
        let Some(state) = hosts.get_mut(host) else {
            return Ok(());
        };
        let Some(until) = state.open_until else {
            return Ok(());
        };
        if now < until || state.probing {
            return Err(ConnectorError::Open {
                host: host.to_string(),
                until_secs: until.saturating_sub(now).max(1),
            });
        }
        state.probing = true;
        Ok(())
    }

    /// What `host` answered: a success closes the circuit and forgets the
    /// failures; a failure counts, opens the circuit at [`OPEN_AFTER`], and a
    /// failed probe opens it again for another pause.
    pub fn record(&self, host: &str, ok: bool, now: u64) {
        let mut hosts = self.hosts.lock().unwrap_or_else(|p| p.into_inner());
        if ok {
            hosts.remove(host);
            return;
        }
        let state = hosts.entry(host.to_string()).or_default();
        state.failures += 1;
        if state.probing || state.failures >= OPEN_AFTER {
            state.open_until = Some(now + COOLDOWN_SECS);
            state.probing = false;
        }
    }

    /// The probe out to `host` was dropped before it answered: nothing is
    /// learnt, and the next call after the pause is the probe. A host with
    /// no probe out is left as it is.
    pub fn abandon(&self, host: &str) {
        let mut hosts = self.hosts.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(state) = hosts.get_mut(host) {
            state.probing = false;
        }
    }

    /// Whether calls to `host` are refused right now.
    pub fn is_open(&self, host: &str, now: u64) -> bool {
        let hosts = self.hosts.lock().unwrap_or_else(|p| p.into_inner());
        hosts
            .get(host)
            .and_then(|s| s.open_until)
            .is_some_and(|until| now < until)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn five_failures_open_the_circuit_and_a_success_closes_it() {
        let b = Breaker::default();
        for _ in 0..OPEN_AFTER - 1 {
            b.record("h", false, 100);
            assert!(b.admit("h", 100).is_ok(), "not yet");
        }
        b.record("h", false, 100);
        assert!(
            matches!(b.admit("h", 101), Err(ConnectorError::Open { ref host, until_secs }) if host == "h" && until_secs == COOLDOWN_SECS - 1)
        );
        assert!(b.is_open("h", 101));
        assert!(
            b.admit("other", 101).is_ok(),
            "another host is another circuit"
        );
        // The pause ends: one probe goes, a second waits on it.
        let later = 100 + COOLDOWN_SECS;
        assert!(b.admit("h", later).is_ok(), "the probe");
        assert!(
            matches!(b.admit("h", later), Err(ConnectorError::Open { .. })),
            "one probe at a time"
        );
        b.record("h", true, later);
        assert!(b.admit("h", later).is_ok());
        assert!(!b.is_open("h", later));
    }

    #[test]
    fn a_failed_probe_opens_the_circuit_again() {
        let b = Breaker::default();
        for _ in 0..OPEN_AFTER {
            b.record("h", false, 0);
        }
        let later = COOLDOWN_SECS;
        assert!(b.admit("h", later).is_ok());
        b.record("h", false, later);
        assert!(b.is_open("h", later + 1));
        assert!(
            matches!(b.admit("h", later + 1), Err(ConnectorError::Open { until_secs, .. }) if until_secs == COOLDOWN_SECS - 1)
        );
    }

    #[test]
    fn an_abandoned_probe_lets_the_next_call_probe_instead() {
        let b = Breaker::default();
        for _ in 0..OPEN_AFTER {
            b.record("h", false, 0);
        }
        let later = COOLDOWN_SECS;
        assert!(b.admit("h", later).is_ok(), "the probe goes out");
        assert!(
            matches!(b.admit("h", later), Err(ConnectorError::Open { .. })),
            "a second call waits on it"
        );
        // The probe's call is dropped: without this, every later call would
        // wait on an answer that never comes.
        b.abandon("h");
        assert!(b.admit("h", later).is_ok(), "the next call is the probe");
        b.record("h", true, later);
        assert!(!b.is_open("h", later));
        b.abandon("other");
        assert!(
            b.admit("other", later).is_ok(),
            "a host with no circuit is untouched"
        );
    }

    #[test]
    fn a_success_between_failures_starts_the_count_over() {
        let b = Breaker::default();
        for _ in 0..OPEN_AFTER - 1 {
            b.record("h", false, 0);
        }
        b.record("h", true, 0);
        for _ in 0..OPEN_AFTER - 1 {
            b.record("h", false, 0);
        }
        assert!(b.admit("h", 0).is_ok());
    }
}
