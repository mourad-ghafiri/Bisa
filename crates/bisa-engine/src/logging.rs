//! The engine's side of the diagnostic log (`bisa-log`): the machine's
//! `logging.*` settings read into the file layer's configuration — at start
//! and on every write of one, beside `cache::refresh_for` and
//! `security::refresh_for` — the folder every child process is told to
//! write under, and the listing a panel reads: the families, the crash
//! reports and the latest of them.
//!
//! The engine never installs a subscriber: the process did, and handed the
//! handle in (`EngineConfig::log`). Without one — every test fixture — the
//! settings are read for nothing and the folder is still named to children,
//! so a test's `bisa mcp` writes under the test's own temp workspace
//! and never under `~/.bisa`.

use std::collections::BTreeMap;
use std::path::PathBuf;

use bisa_core::ResolvedSetting;
pub use bisa_log::{CrashReport, Family, Listing, LogFile};
use bisa_log::{LogConfig, LogLevel, LogRotation, LOG_DIR_ENV};

use crate::Inner;

/// The keys, in the registry's order.
pub const KEYS: [&str; 4] = [
    "logging.enabled",
    "logging.level",
    "logging.rotation",
    "logging.keep_files",
];

/// The resolved `logging.*` settings as the file layer's configuration. A
/// key that is absent or holds a word the crate does not speak keeps the
/// default's value — the registry refused it on the way in, so this is a
/// belt over braces, never a second validation.
pub fn config_from(resolved: &[ResolvedSetting]) -> LogConfig {
    let defaults = LogConfig::default();
    let value = |key: &str| resolved.iter().find(|r| r.key == key).map(|r| &r.value);
    LogConfig {
        enabled: value(KEYS[0])
            .and_then(|v| v.as_bool())
            .unwrap_or(defaults.enabled),
        level: value(KEYS[1])
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<LogLevel>().ok())
            .unwrap_or(defaults.level),
        rotation: value(KEYS[2])
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<LogRotation>().ok())
            .unwrap_or(defaults.rotation),
        keep_files: value(KEYS[3])
            .and_then(|v| v.as_u64())
            .map(|n| n as usize)
            .unwrap_or(defaults.keep_files),
    }
}

/// Read the machine's settings and apply them to the process's log, when
/// the process has one. A store that cannot be read, or a layer that
/// refuses, is said on the log itself and changes nothing.
pub fn apply(inner: &Inner) {
    let Some(handle) = &inner.log else {
        return;
    };
    let config = match inner.ws.settings(None) {
        Ok(resolved) => config_from(&resolved),
        Err(e) => {
            tracing::warn!(target: "bisa_engine::logging", "logging settings not read: {e}");
            return;
        }
    };
    match handle.apply(config) {
        Ok(()) => tracing::debug!(
            target: "bisa_engine::logging",
            enabled = config.enabled,
            level = %config.level,
            rotation = %config.rotation,
            keep_files = config.keep_files,
            "logging settings applied"
        ),
        Err(e) => {
            tracing::warn!(target: "bisa_engine::logging", "logging settings not applied: {e}")
        }
    }
}

/// Re-apply after a settings write, when the key is one of ours.
pub fn refresh_for(inner: &Inner, key: &str) {
    if key.starts_with("logging.") {
        apply(inner);
    }
}

/// The folder — the store's, so the node, the CLI, every child and the
/// desktop shell name the same place.
pub fn dir(inner: &Inner) -> PathBuf {
    inner.ws.paths().logs_dir()
}

/// What the folder holds: every family with its files newest first, and
/// the crash reports; a folder not yet made lists nothing. A folder that
/// cannot be read is said and answered as one not yet made.
pub fn listing(inner: &Inner) -> Listing {
    let dir = dir(inner);
    bisa_log::list(&dir).unwrap_or_else(|e| {
        tracing::warn!(target: "bisa_engine::logging", dir = %dir.display(), "log folder not read: {e}");
        Listing {
            families: Vec::new(),
            crashes: Vec::new(),
        }
    })
}

/// The newest crash report and its file name, when there is one.
pub fn latest_crash(inner: &Inner) -> Option<(String, CrashReport)> {
    bisa_log::latest_crash(&dir(inner))
}

/// One crash report by its file name — a name that is not a crash name, or
/// a report that is not there, is `None`.
pub fn crash(inner: &Inner, name: &str) -> Option<CrashReport> {
    bisa_log::read_crash(&dir(inner), name)
}

/// The environment a child the engine spawns — `bisa mcp`, the hook
/// personalities — writes its own log under: the parent's folder, named,
/// because the child runs without `--data-dir` and must not resolve the
/// default workspace on its own. Empty while the log is switched off, so a
/// child then writes nothing either.
pub fn child_env(inner: &Inner) -> BTreeMap<String, String> {
    let enabled = inner
        .log
        .as_ref()
        .map(|h| h.config().enabled)
        .unwrap_or(true);
    if !enabled {
        return BTreeMap::new();
    }
    BTreeMap::from([(LOG_DIR_ENV.to_string(), dir(inner).display().to_string())])
}

#[cfg(test)]
mod tests {
    use super::*;
    use bisa_core::settings::Origin;
    use serde_json::json;

    fn resolved(pairs: &[(&str, serde_json::Value)]) -> Vec<ResolvedSetting> {
        pairs
            .iter()
            .map(|(k, v)| ResolvedSetting {
                key: (*k).to_string(),
                value: v.clone(),
                origin: Origin::Machine,
            })
            .collect()
    }

    #[test]
    fn the_settings_read_into_a_config_and_an_absent_or_bad_one_keeps_the_default() {
        let c = config_from(&resolved(&[
            ("logging.enabled", json!(false)),
            ("logging.level", json!("debug")),
            ("logging.rotation", json!("hourly")),
            ("logging.keep_files", json!(3)),
        ]));
        assert_eq!(
            c,
            LogConfig {
                enabled: false,
                level: LogLevel::Debug,
                rotation: LogRotation::Hourly,
                keep_files: 3,
            }
        );
        assert_eq!(config_from(&[]), LogConfig::default());
        let odd = config_from(&resolved(&[
            ("logging.level", json!("loud")),
            ("logging.keep_files", json!("many")),
        ]));
        assert_eq!(odd, LogConfig::default());
    }
}
