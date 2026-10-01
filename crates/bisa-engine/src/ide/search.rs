//! Content search and replace across a root (ide/12).
//!
//! ripgrep as a library: `ignore` walks the root honouring `.gitignore` and
//! never following a symlink, `grep-regex` matches, `grep-searcher` streams
//! lines with one line of context either side and quits a file at the first
//! NUL. Results are handed to a sink as they are found, so a first page can
//! render while the rest scans, and the whole thing is bounded by `limit`.
//!
//! Replace is a preview first — every change as a line-level diff with the
//! file's `base_hash` — and then one compare-and-swap write per file through
//! [`super::files`]. A file that changed between the preview and the apply is
//! reported as skipped, never clobbered.

use crate::ide::files;
use crate::{EngineError, Inner};
use bisa_store::{content_hash, FileScope};
use grep_matcher::Matcher;
use grep_regex::{RegexMatcher, RegexMatcherBuilder};
use grep_searcher::{
    BinaryDetection, Searcher, SearcherBuilder, Sink, SinkContext, SinkContextKind, SinkMatch,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The most matches a search returns before it says `truncated`.
pub const DEFAULT_LIMIT: usize = 2_000;
pub const MAX_LIMIT: usize = 20_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CaseMode {
    /// Case-insensitive unless the pattern has an upper-case letter.
    #[default]
    Smart,
    Sensitive,
    Insensitive,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SearchQuery {
    pub q: String,
    /// The pattern is a regex; otherwise it is matched literally.
    #[serde(default)]
    pub regex: bool,
    #[serde(default)]
    pub case: CaseMode,
    /// Whole-word matches only.
    #[serde(default)]
    pub word: bool,
    /// Globs a path must match (any), relative to the root.
    #[serde(default)]
    pub include: Vec<String>,
    /// Globs that exclude a path.
    #[serde(default)]
    pub exclude: Vec<String>,
    #[serde(default)]
    pub limit: Option<usize>,
}

/// One matching line.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchHit {
    pub path: String,
    /// 1-based.
    pub line: u64,
    /// 1-based byte column of the first match on the line.
    pub column: usize,
    pub text: String,
    pub before: Option<String>,
    pub after: Option<String>,
}

/// What a finished search says about itself.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SearchSummary {
    pub matches: usize,
    pub files_with_matches: usize,
    pub files_scanned: usize,
    pub truncated: bool,
}

fn escape_literal(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        if "\\.+*?()|[]{}^$#&-~".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

fn matcher_for(q: &SearchQuery) -> Result<RegexMatcher, EngineError> {
    if q.q.is_empty() {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-search-pattern-empty"
        )));
    }
    let pattern = if q.regex {
        q.q.clone()
    } else {
        escape_literal(&q.q)
    };
    let mut b = RegexMatcherBuilder::new();
    match q.case {
        CaseMode::Smart => b.case_smart(true),
        CaseMode::Sensitive => b.case_insensitive(false),
        CaseMode::Insensitive => b.case_insensitive(true),
    };
    b.word(q.word).line_terminator(Some(b'\n'));
    b.build(&pattern).map_err(|e| {
        EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-pattern-not-valid-regex",
            e = e.to_string()
        ))
    })
}

/// The same pattern as a `regex::Regex`, for replacement — one grammar, two
/// engines that agree on what a match is.
/// The query as a regex, or the refusal — what a route checks before a stream opens.
pub fn regex_for(q: &SearchQuery) -> Result<regex::Regex, EngineError> {
    let mut pattern = if q.regex {
        q.q.clone()
    } else {
        escape_literal(&q.q)
    };
    if q.word {
        pattern = format!(r"\b(?:{pattern})\b");
    }
    let insensitive = match q.case {
        CaseMode::Sensitive => false,
        CaseMode::Insensitive => true,
        CaseMode::Smart => !q.q.chars().any(|c| c.is_uppercase()),
    };
    regex::RegexBuilder::new(&pattern)
        .case_insensitive(insensitive)
        .build()
        .map_err(|e| {
            EngineError::Invalid(bisa_core::text!(
                "error-engine-invalid-pattern-not-valid-regex",
                e = e.to_string()
            ))
        })
}

fn walker(root: &Path, q: &SearchQuery) -> Result<ignore::Walk, EngineError> {
    let mut overrides = ignore::overrides::OverrideBuilder::new(root);
    for g in &q.include {
        overrides.add(g).map_err(|e| {
            EngineError::Invalid(bisa_core::text!(
                "error-engine-invalid-include-glob",
                g = format!("{g:?}"),
                e = e.to_string()
            ))
        })?;
    }
    for g in &q.exclude {
        overrides.add(&format!("!{g}")).map_err(|e| {
            EngineError::Invalid(bisa_core::text!(
                "error-engine-invalid-exclude-glob",
                g = format!("{g:?}"),
                e = e.to_string()
            ))
        })?;
    }
    let overrides = overrides.build().map_err(|e| {
        EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-globs",
            e = e.to_string()
        ))
    })?;
    Ok(ignore::WalkBuilder::new(root)
        .follow_links(false)
        .hidden(true)
        // `.gitignore` is honoured whether or not the root is a repository
        // yet: a goal's folder and a fresh project carry one before `git init`.
        .require_git(false)
        .git_ignore(true)
        .git_global(false)
        .git_exclude(true)
        .overrides(overrides)
        .build())
}

fn searcher() -> Searcher {
    SearcherBuilder::new()
        .line_number(true)
        .before_context(1)
        .after_context(1)
        .binary_detection(BinaryDetection::quit(b'\x00'))
        .build()
}

/// A [`Sink`] that turns matches and their context into [`SearchHit`]s.
struct Collector<'a, F: FnMut(SearchHit) -> bool> {
    path: String,
    matcher: &'a RegexMatcher,
    emit: &'a mut F,
    before: Option<String>,
    pending: Option<SearchHit>,
    emitted: usize,
    budget: usize,
    stopped: bool,
}

impl<F: FnMut(SearchHit) -> bool> Collector<'_, F> {
    fn flush(&mut self) -> bool {
        if let Some(hit) = self.pending.take() {
            self.emitted += 1;
            if !(self.emit)(hit) {
                self.stopped = true;
                return false;
            }
        }
        true
    }
}

fn line_text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .trim_end_matches(['\n', '\r'])
        .to_string()
}

impl<F: FnMut(SearchHit) -> bool> Sink for Collector<'_, F> {
    type Error = std::io::Error;

    fn matched(&mut self, _s: &Searcher, m: &SinkMatch<'_>) -> Result<bool, std::io::Error> {
        if !self.flush() {
            return Ok(false);
        }
        if self.emitted >= self.budget {
            self.stopped = true;
            return Ok(false);
        }
        let column = self
            .matcher
            .find(m.bytes())
            .ok()
            .flatten()
            .map(|r| r.start() + 1)
            .unwrap_or(1);
        self.pending = Some(SearchHit {
            path: self.path.clone(),
            line: m.line_number().unwrap_or(0),
            column,
            text: line_text(m.bytes()),
            before: self.before.take(),
            after: None,
        });
        Ok(true)
    }

    fn context(&mut self, _s: &Searcher, c: &SinkContext<'_>) -> Result<bool, std::io::Error> {
        match c.kind() {
            SinkContextKind::Before => {
                // A line that is both "after" the last match and "before" the
                // next arrives once, as After; a fresh Before belongs to the next.
                self.before = Some(line_text(c.bytes()));
            }
            SinkContextKind::After => {
                if let Some(p) = self.pending.as_mut() {
                    if p.after.is_none() {
                        p.after = Some(line_text(c.bytes()));
                    }
                }
                // It also precedes whatever comes next.
                self.before = Some(line_text(c.bytes()));
                return Ok(self.flush());
            }
            SinkContextKind::Other => {}
        }
        Ok(true)
    }

    fn finish(
        &mut self,
        _s: &Searcher,
        _f: &grep_searcher::SinkFinish,
    ) -> Result<(), std::io::Error> {
        self.flush();
        Ok(())
    }
}

/// Search a scope's root, handing each hit to `emit` as it is found. `emit`
/// returns `false` to stop early. Returns the summary.
pub fn search(
    inner: &Arc<Inner>,
    scope: FileScope,
    id: &str,
    query: &SearchQuery,
    mut emit: impl FnMut(SearchHit) -> bool,
) -> Result<SearchSummary, EngineError> {
    let root = inner.ws.file_root(scope, id)?;
    if !root.is_dir() {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-has-no-directory-yet-so-there-nothing",
            a0 = (scope.as_str()).to_string(),
            id = id.to_string()
        )));
    }
    let matcher = matcher_for(query)?;
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let mut searcher = searcher();
    let mut summary = SearchSummary::default();
    let mut remaining = limit;
    for entry in walker(&root, query)? {
        let Ok(entry) = entry else { continue };
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        summary.files_scanned += 1;
        let rel = entry
            .path()
            .strip_prefix(&root)
            .unwrap_or(entry.path())
            .to_string_lossy()
            .into_owned();
        let mut collector = Collector {
            path: rel,
            matcher: &matcher,
            emit: &mut emit,
            before: None,
            pending: None,
            emitted: 0,
            budget: remaining,
            stopped: false,
        };
        if let Err(e) = searcher.search_path(&matcher, entry.path(), &mut collector) {
            tracing::debug!("search skipped {}: {e}", entry.path().display());
            continue;
        }
        if collector.emitted > 0 {
            summary.files_with_matches += 1;
        }
        summary.matches += collector.emitted;
        remaining = remaining.saturating_sub(collector.emitted);
        if collector.stopped || remaining == 0 {
            summary.truncated = remaining == 0;
            break;
        }
    }
    Ok(summary)
}

/// One line the replacement would change.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplaceChange {
    pub line: u64,
    pub before: String,
    pub after: String,
}

/// A file the replacement would touch, with the hash the apply must carry.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplacePreview {
    pub path: String,
    pub base_hash: String,
    pub changes: Vec<ReplaceChange>,
}

/// What happened to one file on apply.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "outcome")]
pub enum ReplaceOutcome {
    Applied {
        path: String,
        changes: usize,
        hash: String,
    },
    /// The file changed between the preview and the apply. Nothing was written.
    SkippedChanged {
        path: String,
        current_hash: String,
    },
    Failed {
        path: String,
        error: String,
    },
}

fn rewrite(re: &regex::Regex, text: &str, replacement: &str) -> (String, Vec<ReplaceChange>) {
    let mut out = String::with_capacity(text.len());
    let mut changes = Vec::new();
    for (i, line) in text.split_inclusive('\n').enumerate() {
        let (body, ending) = match line.strip_suffix("\r\n") {
            Some(b) => (b, "\r\n"),
            None => match line.strip_suffix('\n') {
                Some(b) => (b, "\n"),
                None => (line, ""),
            },
        };
        let replaced = re.replace_all(body, replacement);
        if replaced != body {
            changes.push(ReplaceChange {
                line: i as u64 + 1,
                before: body.to_string(),
                after: replaced.to_string(),
            });
        }
        out.push_str(&replaced);
        out.push_str(ending);
    }
    (out, changes)
}

/// Every file the replacement would change, with the exact lines. Binary and
/// non-UTF-8 files are skipped, as a search skips them.
pub fn preview_replace(
    inner: &Arc<Inner>,
    scope: FileScope,
    id: &str,
    query: &SearchQuery,
    replacement: &str,
) -> Result<Vec<ReplacePreview>, EngineError> {
    let root = inner.ws.file_root(scope, id)?;
    let re = regex_for(query)?;
    // The search decides which files, so the preview cannot disagree with it.
    let mut paths: Vec<String> = Vec::new();
    search(inner, scope, id, query, |hit| {
        if paths.last() != Some(&hit.path) && !paths.contains(&hit.path) {
            paths.push(hit.path);
        }
        true
    })?;
    let mut out = Vec::new();
    for rel in paths {
        let path: PathBuf = root.join(&rel);
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let Ok(text) = String::from_utf8(bytes.clone()) else {
            continue;
        };
        let (_, changes) = rewrite(&re, &text, replacement);
        if changes.is_empty() {
            continue;
        }
        out.push(ReplacePreview {
            path: rel,
            base_hash: content_hash(&bytes),
            changes,
        });
    }
    Ok(out)
}

/// Apply a replacement to the given files, each guarded by the hash the
/// preview carried. A file that moved on is skipped and says so.
pub fn apply_replace(
    inner: &Arc<Inner>,
    scope: FileScope,
    id: &str,
    query: &SearchQuery,
    replacement: &str,
    files: &[(String, String)],
) -> Result<Vec<ReplaceOutcome>, EngineError> {
    let root = inner.ws.file_root(scope, id)?;
    let re = regex_for(query)?;
    let mut out = Vec::new();
    for (rel, base_hash) in files {
        // A path the client names is contained like every other: one that
        // leaves the root is neither read nor hashed.
        let path = match bisa_store::resolve_within(&root, rel) {
            Ok(p) => p,
            Err(e) => {
                out.push(ReplaceOutcome::Failed {
                    path: rel.clone(),
                    error: e.to_string(),
                });
                continue;
            }
        };
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) => {
                out.push(ReplaceOutcome::Failed {
                    path: rel.clone(),
                    error: e.to_string(),
                });
                continue;
            }
        };
        let current_hash = content_hash(&bytes);
        if &current_hash != base_hash {
            out.push(ReplaceOutcome::SkippedChanged {
                path: rel.clone(),
                current_hash,
            });
            continue;
        }
        let Ok(text) = String::from_utf8(bytes) else {
            out.push(ReplaceOutcome::Failed {
                path: rel.clone(),
                error: "not UTF-8 text".into(),
            });
            continue;
        };
        let (new_text, changes) = rewrite(&re, &text, replacement);
        if changes.is_empty() {
            out.push(ReplaceOutcome::Applied {
                path: rel.clone(),
                changes: 0,
                hash: current_hash,
            });
            continue;
        }
        match files::write_file(inner, scope, id, rel, &new_text, Some(base_hash)) {
            Ok(w) => out.push(ReplaceOutcome::Applied {
                path: rel.clone(),
                changes: changes.len(),
                hash: w.hash,
            }),
            Err(EngineError::FileConflict { current_hash, .. }) => {
                out.push(ReplaceOutcome::SkippedChanged {
                    path: rel.clone(),
                    current_hash,
                })
            }
            Err(e) => out.push(ReplaceOutcome::Failed {
                path: rel.clone(),
                error: e.to_string(),
            }),
        }
    }
    Ok(out)
}
