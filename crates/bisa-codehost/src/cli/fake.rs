//! A scripted CLI runner for tests: canned answers matched by program and by
//! the words an argv must contain, consumed in order, every call recorded with
//! its environment and its stdin. No program is spawned and nothing of the
//! person's is read. A program not marked installed answers `NotInstalled`,
//! which is how a test proves the API is asked when the CLI is absent.

use super::{CliError, CliProgram, CliRunner, Output};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

/// One canned answer.
#[derive(Debug, Clone)]
pub struct Canned {
    pub program: CliProgram,
    /// Every word here must appear in the argv for the answer to match.
    pub words: Vec<String>,
    pub output: Output,
}

/// What the fake was asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call {
    pub program: CliProgram,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub stdin: Option<String>,
}

impl Call {
    /// The argv as one line, for an assertion.
    pub fn line(&self) -> String {
        format!("{} {}", self.program.binary(), self.args.join(" "))
    }

    /// One environment variable the call carried.
    pub fn env(&self, key: &str) -> Option<&str> {
        self.env
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }
}

#[derive(Debug, Default)]
pub struct FakeCli {
    installed: Mutex<HashMap<CliProgram, PathBuf>>,
    answers: Mutex<Vec<Canned>>,
    calls: Mutex<Vec<Call>>,
}

impl FakeCli {
    pub fn new() -> Self {
        Self::default()
    }

    /// Mark `program` installed at `path`. Unmarked, it is not on PATH.
    pub fn with_program(self, program: CliProgram, path: &str) -> Self {
        self.installed
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(program, PathBuf::from(path));
        self
    }

    /// Script one answer: `program` run with an argv containing every one of
    /// `words` answers `code`, `stdout` and `stderr`. Answers are consumed in
    /// order among those that match.
    pub fn answer(
        self,
        program: CliProgram,
        words: &[&str],
        code: i32,
        stdout: &str,
        stderr: &str,
    ) -> Self {
        self.answers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(Canned {
                program,
                words: words.iter().map(|w| w.to_string()).collect(),
                output: Output {
                    code,
                    stdout: stdout.to_string(),
                    stderr: stderr.to_string(),
                },
            });
        self
    }

    /// The same answer, kept for every matching call (a `--version`, an
    /// `auth status` asked more than once).
    pub fn always(
        self,
        program: CliProgram,
        words: &[&str],
        code: i32,
        stdout: &str,
        stderr: &str,
    ) -> Self {
        let mut this = self;
        for _ in 0..16 {
            this = this.answer(program, words, code, stdout, stderr);
        }
        this
    }

    pub fn calls(&self) -> Vec<Call> {
        self.calls.lock().map(|c| c.clone()).unwrap_or_default()
    }

    /// The argv lines asked so far.
    pub fn lines(&self) -> Vec<String> {
        self.calls().iter().map(Call::line).collect()
    }

    /// Answers scripted and never asked for — a test's proof of what was not run.
    pub fn unconsumed(&self) -> usize {
        self.answers.lock().map(|a| a.len()).unwrap_or(0)
    }
}

impl CliRunner for FakeCli {
    fn installed(&self, program: CliProgram) -> Option<PathBuf> {
        self.installed.lock().ok()?.get(&program).cloned()
    }

    fn run(
        &self,
        program: CliProgram,
        args: &[String],
        env: &[(String, String)],
        stdin: Option<&[u8]>,
        _timeout: Duration,
    ) -> Result<Output, CliError> {
        self.calls
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(Call {
                program,
                args: args.to_vec(),
                env: env.to_vec(),
                stdin: stdin.map(|b| String::from_utf8_lossy(b).into_owned()),
            });
        if self.installed(program).is_none() {
            return Err(CliError::NotInstalled(program));
        }
        let mut answers = self.answers.lock().unwrap_or_else(|e| e.into_inner());
        let position = answers.iter().position(|c| {
            c.program == program
                && c.words
                    .iter()
                    .all(|w| args.iter().any(|a| a == w || a.contains(w.as_str())))
        });
        match position {
            Some(i) => Ok(answers.remove(i).output),
            None => Err(CliError::Failed {
                what: format!("{} {}", program.binary(), args.join(" ")),
                detail: "the fake has no answer for this call".into(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fake_answers_by_words_records_calls_and_refuses_what_is_not_installed() {
        let fake = FakeCli::new()
            .with_program(CliProgram::Gh, "/opt/homebrew/bin/gh")
            .answer(CliProgram::Gh, &["auth", "status"], 0, "ok", "")
            .answer(CliProgram::Gh, &["--version"], 0, "gh version 2.63.2", "");
        assert_eq!(
            fake.installed(CliProgram::Gh).unwrap().to_str(),
            Some("/opt/homebrew/bin/gh")
        );
        assert!(fake.installed(CliProgram::Glab).is_none());
        let version = fake
            .run(
                CliProgram::Gh,
                &["--version".into()],
                &[],
                None,
                Duration::from_secs(1),
            )
            .unwrap();
        assert_eq!(version.stdout, "gh version 2.63.2");
        let status = fake
            .run(
                CliProgram::Gh,
                &[
                    "auth".into(),
                    "status".into(),
                    "--hostname".into(),
                    "github.com".into(),
                ],
                &[("GH_TOKEN".into(), "x".into())],
                Some(b"body"),
                Duration::from_secs(1),
            )
            .unwrap();
        assert_eq!(status.stdout, "ok");
        let unmatched = fake
            .run(
                CliProgram::Gh,
                &["pr".into(), "view".into()],
                &[],
                None,
                Duration::from_secs(1),
            )
            .unwrap_err();
        assert!(
            matches!(unmatched, CliError::Failed { .. }),
            "an unscripted call fails loudly"
        );
        assert!(matches!(
            fake.run(
                CliProgram::Glab,
                &["--version".into()],
                &[],
                None,
                Duration::from_secs(1)
            )
            .unwrap_err(),
            CliError::NotInstalled(CliProgram::Glab)
        ));
        let calls = fake.calls();
        assert_eq!(calls.len(), 4);
        assert_eq!(calls[1].line(), "gh auth status --hostname github.com");
        assert_eq!(calls[1].env("GH_TOKEN"), Some("x"));
        assert_eq!(calls[1].stdin.as_deref(), Some("body"));
        assert_eq!(fake.unconsumed(), 0);
    }
}
