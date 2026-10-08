//! What the file layer is told: whether it writes, how much, how often it
//! rolls and how many files it keeps — the four `logging.*` settings, as a
//! typed value with the words they are spelled with on the wire and in the
//! registry.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use tracing_appender::rolling::Rotation;
use tracing_subscriber::filter::LevelFilter;

/// A word the settings speak that this crate does not.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ConfigError {
    #[error("{word:?} is not a log level (one of {choices})", word = .0, choices = LogLevel::WORDS.join(", "))]
    UnknownLevel(String),
    #[error("{word:?} is not a rotation (one of {choices})", word = .0, choices = LogRotation::WORDS.join(", "))]
    UnknownRotation(String),
}

/// How much is written: the lowest level that reaches the file.
///
/// The words are the registry's `logging.level` choice, in the order a
/// person reads them — the quietest first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl LogLevel {
    /// Every level, quietest first — the order the setting offers them in.
    pub const ALL: [LogLevel; 5] = [
        LogLevel::Error,
        LogLevel::Warn,
        LogLevel::Info,
        LogLevel::Debug,
        LogLevel::Trace,
    ];
    /// The wire words, in [`Self::ALL`]'s order.
    pub const WORDS: [&'static str; 5] = ["error", "warn", "info", "debug", "trace"];

    pub fn as_str(self) -> &'static str {
        Self::WORDS[self as usize]
    }

    /// The `tracing` filter that admits this level and the louder ones.
    pub fn filter(self) -> LevelFilter {
        match self {
            LogLevel::Error => LevelFilter::ERROR,
            LogLevel::Warn => LevelFilter::WARN,
            LogLevel::Info => LevelFilter::INFO,
            LogLevel::Debug => LevelFilter::DEBUG,
            LogLevel::Trace => LevelFilter::TRACE,
        }
    }
}

impl FromStr for LogLevel {
    type Err = ConfigError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|l| l.as_str() == s)
            .ok_or_else(|| ConfigError::UnknownLevel(s.to_string()))
    }
}

impl fmt::Display for LogLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// How often the file rolls over to a new one. The file names carry the
/// period in UTC — `node.2026-09-08.jsonl` daily, `node.2026-09-08-14.jsonl`
/// hourly — so a person reads the period off the name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogRotation {
    Hourly,
    Daily,
}

impl LogRotation {
    pub const ALL: [LogRotation; 2] = [LogRotation::Hourly, LogRotation::Daily];
    /// The wire words, in [`Self::ALL`]'s order.
    pub const WORDS: [&'static str; 2] = ["hourly", "daily"];

    pub fn as_str(self) -> &'static str {
        Self::WORDS[self as usize]
    }

    pub(crate) fn rotation(self) -> Rotation {
        match self {
            LogRotation::Hourly => Rotation::HOURLY,
            LogRotation::Daily => Rotation::DAILY,
        }
    }
}

impl FromStr for LogRotation {
    type Err = ConfigError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|r| r.as_str() == s)
            .ok_or_else(|| ConfigError::UnknownRotation(s.to_string()))
    }
}

impl fmt::Display for LogRotation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The file layer's whole configuration. [`Default`] is what a process runs
/// with before anything resolved a setting — and what the registry's
/// defaults resolve to: **errors only**, one file a day, fourteen kept.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogConfig {
    /// The master switch. Off, nothing is written and no file is opened.
    pub enabled: bool,
    pub level: LogLevel,
    pub rotation: LogRotation,
    /// How many files of one process family are kept; the oldest past this
    /// count is pruned when a new one opens — the one thing the platform
    /// removes on its own.
    pub keep_files: usize,
}

impl LogConfig {
    pub const DEFAULT_KEEP_FILES: usize = 14;

    /// The filter the file layer runs under: the level, or `OFF` when the
    /// switch is off.
    pub fn filter(&self) -> LevelFilter {
        if self.enabled {
            self.level.filter()
        } else {
            LevelFilter::OFF
        }
    }
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            level: LogLevel::Error,
            rotation: LogRotation::Daily,
            keep_files: Self::DEFAULT_KEEP_FILES,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_word_round_trips_in_the_order_the_setting_offers() {
        for (level, word) in LogLevel::ALL.into_iter().zip(LogLevel::WORDS) {
            assert_eq!(level.as_str(), word);
            assert_eq!(word.parse::<LogLevel>().unwrap(), level);
            assert_eq!(level.to_string(), word);
        }
        for (rotation, word) in LogRotation::ALL.into_iter().zip(LogRotation::WORDS) {
            assert_eq!(rotation.as_str(), word);
            assert_eq!(word.parse::<LogRotation>().unwrap(), rotation);
        }
        assert_eq!(
            "loud".parse::<LogLevel>(),
            Err(ConfigError::UnknownLevel("loud".into()))
        );
        assert_eq!(
            "weekly".parse::<LogRotation>(),
            Err(ConfigError::UnknownRotation("weekly".into()))
        );
    }

    #[test]
    fn the_default_is_errors_only_daily_and_fourteen_and_off_filters_everything() {
        let d = LogConfig::default();
        assert!(d.enabled);
        assert_eq!(d.level, LogLevel::Error);
        assert_eq!(d.rotation, LogRotation::Daily);
        assert_eq!(d.keep_files, 14);
        assert_eq!(d.filter(), LevelFilter::ERROR);
        let off = LogConfig {
            enabled: false,
            level: LogLevel::Trace,
            ..d
        };
        assert_eq!(off.filter(), LevelFilter::OFF);
    }

    #[test]
    fn the_levels_order_quiet_to_loud_and_serde_speaks_the_words() {
        assert!(LogLevel::Error < LogLevel::Trace);
        let json = serde_json::to_string(&LogConfig::default()).unwrap();
        assert_eq!(
            json,
            r#"{"enabled":true,"level":"error","rotation":"daily","keep_files":14}"#
        );
        let back: LogConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back, LogConfig::default());
    }

    // added by the coverage pass: b6-config.rs
    #[test]
    fn every_level_has_its_filter() {
        assert_eq!(LogLevel::Info.filter(), LevelFilter::INFO);
        assert_eq!(LogLevel::Trace.filter(), LevelFilter::TRACE);
        assert_eq!(LogLevel::Debug.filter(), LevelFilter::DEBUG);
    }
}
