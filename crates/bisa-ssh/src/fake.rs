//! A scripted runner for tests: canned answers matched by program and by the
//! words an argv must contain, consumed in order, every call recorded. No
//! program is spawned and no directory of the person's is read.

use std::sync::Mutex;
use std::time::Duration;

use crate::exec::{Output, Program, SshRunner};
use crate::{SshError, SshResult};

/// One canned answer.
#[derive(Debug, Clone)]
pub struct Canned {
    pub program: Program,
    /// Every word here must appear in the argv for the answer to match.
    pub words: Vec<String>,
    pub output: Output,
    /// Files the program would have written — `ssh-keygen`'s key pair —
    /// written by the fake when the answer is consumed.
    pub writes: Vec<(std::path::PathBuf, String)>,
}

/// What the fake was asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call {
    pub program: Program,
    pub args: Vec<String>,
}

impl Call {
    /// The argv as one line, for an assertion.
    pub fn line(&self) -> String {
        format!("{} {}", self.program.binary(), self.args.join(" "))
    }
}

#[derive(Debug, Default)]
pub struct FakeSsh {
    answers: Mutex<Vec<Canned>>,
    calls: Mutex<Vec<Call>>,
}

impl FakeSsh {
    pub fn new() -> Self {
        Self::default()
    }

    /// Script one answer: `program` run with an argv containing every one of
    /// `words` answers `code`, `stdout` and `stderr`. Answers are consumed in
    /// order among those that match.
    pub fn answer(
        self,
        program: Program,
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
                writes: Vec::new(),
            });
        self
    }

    /// The last scripted answer also writes `text` to `path` when consumed —
    /// what `ssh-keygen` does with a key pair. May be chained for both files.
    pub fn writing(self, path: impl Into<std::path::PathBuf>, text: &str) -> Self {
        if let Some(last) = self
            .answers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .last_mut()
        {
            last.writes.push((path.into(), text.to_string()));
        }
        self
    }

    pub fn calls(&self) -> Vec<Call> {
        self.calls.lock().map(|c| c.clone()).unwrap_or_default()
    }
}

impl SshRunner for FakeSsh {
    fn run(&self, program: Program, args: &[String], _timeout: Duration) -> SshResult<Output> {
        self.calls
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(Call {
                program,
                args: args.to_vec(),
            });
        let mut answers = self.answers.lock().unwrap_or_else(|e| e.into_inner());
        let at = answers.iter().position(|a| {
            a.program == program && a.words.iter().all(|w| args.iter().any(|arg| arg == w))
        });
        match at {
            Some(i) => {
                let canned = answers.remove(i);
                for (path, text) in &canned.writes {
                    if let Some(parent) = path.parent() {
                        std::fs::create_dir_all(parent).map_err(|e| SshError::Failed {
                            what: "the fake's write".to_string(),
                            detail: e.to_string(),
                        })?;
                    }
                    std::fs::write(path, text).map_err(|e| SshError::Failed {
                        what: "the fake's write".to_string(),
                        detail: e.to_string(),
                    })?;
                }
                Ok(canned.output)
            }
            None => Err(SshError::Unavailable(format!(
                "the fake has no answer for `{} {}`",
                program.binary(),
                args.join(" ")
            ))),
        }
    }
}
