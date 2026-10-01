//! Pure parsers for git's machine-readable formats.
//!
//! Every function here is total: it takes bytes and returns data, with no I/O
//! and no failure path, so the whole surface is table-testable against output
//! captured from a real git. Nothing in here reads a human-facing format —
//! `--porcelain=v2`, `--porcelain`, `-z --numstat` and an explicit
//! `--format` are contracts git keeps across versions; the pretty output is
//! not.
//!
//! `-z` is used wherever git offers it, because the alternative is git
//! C-quoting any path that is not plain ASCII and us unquoting it back. A
//! filename with a space, a quote or a `é` in it is not an edge case, it is
//! Tuesday.

use std::path::PathBuf;

use crate::git::{
    BlameLine, ChangeKind, CommitDetail, CommitId, CommitSummary, ConflictKind, FileStatus,
    GraphCommit, GraphRef, StashEntry, Status, WorktreeEntry,
};

/// Parse `git status --porcelain=v2 --branch -z` into **both** views of it:
/// the counts a badge needs and the rows a per-file surface needs.
///
/// One walk, not two, because git already put the path on every record — the
/// counts used to be computed by stepping over it. Two parsers over the same
/// bytes could disagree about what a record means; two *invocations* could
/// disagree about the tree itself, because it moves between them.
///
/// The shape of a v2 record is `<type> <XY> ...`, where for tracked entries
/// `X` is the index (staged) state and `Y` the worktree (unstaged) state, `.`
/// meaning unmodified. A rename record (`2`) is followed by a second
/// NUL-terminated field holding the original path, which must be consumed or
/// every field after it shifts by one.
pub(crate) fn status(raw: &[u8]) -> (Status, Vec<FileStatus>) {
    let mut st = Status::default();
    let mut files = Vec::new();
    let mut tokens = raw.split(|b| *b == 0).filter(|t| !t.is_empty());

    while let Some(token) = tokens.next() {
        let line = String::from_utf8_lossy(token);
        if let Some(header) = line.strip_prefix("# ") {
            branch_header(header, &mut st);
            continue;
        }
        let bytes = line.as_bytes();
        match bytes.first() {
            // `<type> <XY> <sub> <mH> <mI> <mW> <hH> <hI> <path>`
            Some(b'1') => {
                count_xy(bytes, &mut st);
                files.extend(entry(&line, 8, None, false, false));
            }
            // The rename/copy score is one extra field, and the original path
            // is its own token after the record.
            Some(b'2') => {
                count_xy(bytes, &mut st);
                let old = tokens.next().map(lossy_path);
                files.extend(entry(&line, 9, old, false, false));
            }
            // Unmerged: three stage modes and three stage hashes instead of
            // the tracked record's two of each.
            Some(b'u') => {
                st.conflicted += 1;
                files.extend(entry(&line, 10, None, false, true));
            }
            Some(b'?') => {
                st.untracked += 1;
                files.extend(entry(&line, 1, None, true, false));
            }
            // `!` is an ignored entry; only present with --ignored.
            _ => {}
        }
    }

    st.is_clean = st.staged == 0 && st.unstaged == 0 && st.untracked == 0 && st.conflicted == 0;
    (st, files)
}

fn count_xy(bytes: &[u8], st: &mut Status) {
    if bytes.get(2).is_some_and(|c| *c != b'.') {
        st.staged += 1;
    }
    if bytes.get(3).is_some_and(|c| *c != b'.') {
        st.unstaged += 1;
    }
}

/// One row, or nothing when the record was truncated before its path.
///
/// `fields` is how many space-separated fields precede the path; the path is
/// then the whole remainder, so a name with spaces in it survives without any
/// quoting — which is the reason `-z` is used in the first place.
fn entry(
    line: &str,
    fields: usize,
    old_path: Option<PathBuf>,
    untracked: bool,
    conflicted: bool,
) -> Option<FileStatus> {
    let path = path_after(line, fields)?;
    let letter = |i: usize| line.as_bytes().get(i).map_or('.', |b| *b as char);
    let (index, worktree) = if untracked {
        ('?', '?')
    } else {
        (letter(2), letter(3))
    };
    // An unmerged record's letters say which side did what; a pair git does
    // not write is still a conflict, of the commonest kind.
    let conflict = conflicted
        .then(|| ConflictKind::from_letters(index, worktree).unwrap_or(ConflictKind::BothModified));
    Some(FileStatus {
        path: PathBuf::from(path),
        old_path,
        index,
        worktree,
        untracked,
        conflict,
    })
}

/// The remainder of `line` after skipping `fields` space-separated fields.
fn path_after(line: &str, fields: usize) -> Option<&str> {
    let mut rest = line;
    for _ in 0..fields {
        rest = rest.split_once(' ')?.1;
    }
    (!rest.is_empty()).then_some(rest)
}

fn branch_header(header: &str, st: &mut Status) {
    let (key, value) = header.split_once(' ').unwrap_or((header, ""));
    match key {
        // `(initial)` means an unborn HEAD — a repository with no commits yet.
        "branch.oid" => st.oid = (value != "(initial)").then(|| value.to_string()),
        "branch.head" => {
            st.detached = value == "(detached)";
            st.branch = (!st.detached).then(|| value.to_string());
        }
        "branch.upstream" => st.upstream = Some(value.to_string()),
        // `+<ahead> -<behind>`; only emitted when an upstream is configured.
        "branch.ab" => {
            for part in value.split_whitespace() {
                match (part.as_bytes().first(), part[1..].parse::<u32>()) {
                    (Some(b'+'), Ok(n)) => st.ahead = n,
                    (Some(b'-'), Ok(n)) => st.behind = n,
                    _ => {}
                }
            }
        }
        _ => {}
    }
}

/// Parse `git worktree list --porcelain`: blank-line-separated stanzas of
/// `<key>[ <value>]` lines.
///
/// The non-`-z` form is used deliberately: `-z` only arrived in git 2.36 and
/// the sole thing it buys is a path containing a newline, which git itself
/// treats as unrepresentable elsewhere. Spaces and non-ASCII are already fine,
/// because the path is the whole remainder of the line.
pub(crate) fn worktree_list(text: &str) -> Vec<WorktreeEntry> {
    let mut out = Vec::new();
    let mut current: Option<WorktreeEntry> = None;

    for line in text.lines() {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.is_empty() {
            out.extend(current.take());
            continue;
        }
        let (key, value) = match line.split_once(' ') {
            Some((k, v)) => (k, v),
            None => (line, ""),
        };
        if key == "worktree" {
            out.extend(current.take());
            current = Some(WorktreeEntry {
                path: PathBuf::from(value),
                ..WorktreeEntry::default()
            });
            continue;
        }
        let Some(entry) = current.as_mut() else {
            continue;
        };
        match key {
            "HEAD" => entry.head = Some(value.to_string()),
            "branch" => {
                entry.branch = Some(
                    value
                        .strip_prefix("refs/heads/")
                        .unwrap_or(value)
                        .to_string(),
                )
            }
            "detached" => entry.detached = true,
            "bare" => entry.bare = true,
            "locked" => {
                entry.locked = true;
                entry.lock_reason = (!value.is_empty()).then(|| value.to_string());
            }
            "prunable" => {
                entry.prunable = true;
                entry.prune_reason = (!value.is_empty()).then(|| value.to_string());
            }
            _ => {}
        }
    }
    out.extend(current.take());
    out
}

/// One `--numstat` row, before it is merged with its `--name-status` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NumStat {
    pub path: PathBuf,
    pub old_path: Option<PathBuf>,
    pub insertions: u64,
    pub deletions: u64,
    pub binary: bool,
}

/// Parse `git diff -z --numstat`.
///
/// Rows are `<ins>\t<del>\t<path>` with a NUL terminator, except that a
/// rename or copy leaves the path field *empty* and follows the row with two
/// more NUL-terminated fields: the old path, then the new one. A binary file
/// reports `-` for both counts.
pub(crate) fn numstat(raw: &[u8]) -> Vec<NumStat> {
    let mut out = Vec::new();
    let mut tokens = raw.split(|b| *b == 0).filter(|t| !t.is_empty());

    while let Some(token) = tokens.next() {
        let row = String::from_utf8_lossy(token);
        let mut fields = row.splitn(3, '\t');
        let (Some(ins), Some(del), Some(rest)) = (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        let binary = ins == "-" || del == "-";
        let (old_path, path) = if rest.is_empty() {
            let old = tokens.next().map(lossy_path);
            let new = tokens.next().map(lossy_path);
            match new {
                Some(new) => (old, new),
                // Truncated output: nothing sane to report for this row.
                None => continue,
            }
        } else {
            (None, PathBuf::from(rest))
        };
        out.push(NumStat {
            path,
            old_path,
            insertions: ins.parse().unwrap_or(0),
            deletions: del.parse().unwrap_or(0),
            binary,
        });
    }
    out
}

/// Parse `git diff -z --name-status` into `(new path, kind, old path)`.
///
/// The status letter is its own NUL-terminated field; `R`/`C` carry a
/// similarity score and are followed by two paths instead of one.
pub(crate) fn name_status(raw: &[u8]) -> Vec<(PathBuf, ChangeKind, Option<PathBuf>)> {
    let mut out = Vec::new();
    let mut tokens = raw.split(|b| *b == 0).filter(|t| !t.is_empty());

    while let Some(token) = tokens.next() {
        let code = String::from_utf8_lossy(token);
        let kind = ChangeKind::from_status_letter(code.as_bytes().first().copied());
        let paired = matches!(kind, ChangeKind::Renamed | ChangeKind::Copied);
        let first = match tokens.next() {
            Some(p) => lossy_path(p),
            None => break,
        };
        if paired {
            match tokens.next() {
                Some(second) => out.push((lossy_path(second), kind, Some(first))),
                None => break,
            }
        } else {
            out.push((first, kind, None));
        }
    }
    out
}

/// Parse the record stream produced by `git::LOG_FORMAT`: fields
/// separated by US (0x1f), records by RS (0x1e). Chosen over JSON-ish
/// pretty formats because a commit subject can contain anything except a
/// control character, and these two are the ones git guarantees it will not
/// emit from `%s` or `%an`.
pub(crate) fn log(text: &str) -> Vec<CommitSummary> {
    text.split('\u{1e}')
        .map(|record| record.trim_start_matches('\n'))
        .filter(|record| !record.is_empty())
        .filter_map(|record| {
            let mut f = record.split('\u{1f}');
            Some(CommitSummary {
                id: CommitId::new(f.next()?),
                short: f.next()?.to_string(),
                author: f.next()?.to_string(),
                email: f.next()?.to_string(),
                timestamp: f.next()?.parse().unwrap_or(0),
                subject: f.next().unwrap_or("").to_string(),
            })
        })
        .collect()
}

/// Parse the record stream produced by `git::STASH_FORMAT` — the reflog of
/// `refs/stash`, newest first. The index is the record's position: that is
/// what `stash@{n}` means at the moment of the read, and `%gd` (kept for a
/// cross-check) can print a longer selector under a branch named `stash`.
pub(crate) fn stash_list(text: &str) -> Vec<StashEntry> {
    text.split('\u{1e}')
        .map(|record| record.trim_start_matches('\n'))
        .filter(|record| !record.is_empty())
        .enumerate()
        .filter_map(|(index, record)| {
            let mut f = record.split('\u{1f}');
            let commit = CommitId::new(f.next()?);
            let _selector = f.next()?;
            let parents = f.next()?.split_whitespace().count();
            let at = f.next()?.trim().parse().unwrap_or(0);
            let subject = f.next().unwrap_or("").trim().to_string();
            let (branch, message) = stash_subject(&subject);
            Some(StashEntry {
                index: index as u32,
                commit,
                branch,
                message,
                subject,
                at,
                untracked: parents >= 3,
            })
        })
        .collect()
}

/// Read a stash's reflog subject: git writes `WIP on <branch>: <sha> <subject>`
/// when no message was given and `On <branch>: <message>` when one was;
/// `(no branch)` stands for a detached HEAD. `:` cannot appear in a ref name,
/// so the first `": "` is the split. Anything else is a message with no branch.
pub(crate) fn stash_subject(subject: &str) -> (Option<String>, Option<String>) {
    let branch_of = |b: &str| {
        let b = b.trim();
        (!b.is_empty() && b != "(no branch)").then(|| b.to_string())
    };
    if let Some(rest) = subject.strip_prefix("WIP on ") {
        return match rest.split_once(": ") {
            Some((branch, _)) => (branch_of(branch), None),
            None => (branch_of(rest), None),
        };
    }
    if let Some(rest) = subject.strip_prefix("On ") {
        if let Some((branch, message)) = rest.split_once(": ") {
            let message = message.trim();
            return (
                branch_of(branch),
                (!message.is_empty()).then(|| message.to_string()),
            );
        }
    }
    let subject = subject.trim();
    (None, (!subject.is_empty()).then(|| subject.to_string()))
}

/// Parse `%D` decorations: `HEAD -> refs/heads/main, refs/remotes/origin/main,
/// tag: refs/tags/v1`. Full ref names are classified by prefix; a short name
/// (an older git ignoring `--decorate=full` for `%D`) falls back to "has a
/// slash means remote".
pub(crate) fn decorations(text: &str) -> Vec<GraphRef> {
    let mut out = Vec::new();
    for raw in text.split(',') {
        let item = raw.trim();
        if item.is_empty() {
            continue;
        }
        let mut push = |name: &str| {
            let name = name.trim();
            if name.is_empty() {
                return;
            }
            // The platform's own refs — the recovery points under
            // `refs/bisa/safety/` — are Safety's to show, never a chip that
            // reads as a remote in the history.
            if name.starts_with(crate::git::BISA_REFS_PREFIX) {
                return;
            }
            let (name, kind) = if let Some(n) = name.strip_prefix("tag: ") {
                (n.strip_prefix("refs/tags/").unwrap_or(n), "tag")
            } else if let Some(n) = name.strip_prefix("refs/tags/") {
                (n, "tag")
            } else if let Some(n) = name.strip_prefix("refs/heads/") {
                (n, "branch")
            } else if let Some(n) = name.strip_prefix("refs/remotes/") {
                (n, "remote")
            } else if name == "HEAD" {
                (name, "head")
            } else if name.contains('/') {
                (name, "remote")
            } else {
                (name, "branch")
            };
            out.push(GraphRef {
                name: name.to_string(),
                kind: kind.to_string(),
            });
        };
        if let Some((head, target)) = item.split_once(" -> ") {
            push(head);
            push(target);
        } else {
            push(item);
        }
    }
    out
}

/// Parse the graph log (`GRAPH_FORMAT`).
pub(crate) fn graph_log(text: &str) -> Vec<GraphCommit> {
    text.split('\u{1e}')
        .map(|record| record.trim_start_matches('\n'))
        .filter(|record| !record.is_empty())
        .filter_map(|record| {
            let mut f = record.split('\u{1f}');
            Some(GraphCommit {
                id: CommitId::new(f.next()?),
                short: f.next()?.to_string(),
                parents: f.next()?.split_whitespace().map(CommitId::new).collect(),
                refs: decorations(f.next()?),
                author: f.next()?.to_string(),
                email: f.next()?.to_string(),
                timestamp: f.next()?.parse().unwrap_or(0),
                subject: f.next().unwrap_or("").to_string(),
            })
        })
        .collect()
}

/// Parse one commit in the `DETAIL_FORMAT`. `files` is left empty for the
/// caller to fill from `diff-tree`.
pub(crate) fn commit_detail(text: &str) -> Option<CommitDetail> {
    let mut f = text.trim_start_matches('\n').splitn(9, '\u{1f}');
    Some(CommitDetail {
        id: CommitId::new(f.next()?.trim()),
        short: f.next()?.to_string(),
        parents: f.next()?.split_whitespace().map(CommitId::new).collect(),
        refs: decorations(f.next()?),
        author: f.next()?.to_string(),
        email: f.next()?.to_string(),
        timestamp: f.next()?.parse().unwrap_or(0),
        subject: f.next()?.to_string(),
        body: f.next().unwrap_or("").trim().to_string(),
        files: Vec::new(),
    })
}

/// Parse `git blame --porcelain`.
///
/// Porcelain groups are: a header `<sha> <orig-line> <final-line> [<count>]`,
/// zero or more `key value` lines — author, author-time, summary and friends
/// appear the *first* time a commit is seen and are omitted afterwards, so
/// they are remembered per sha — then one line of content prefixed by a tab.
/// The all-zero sha is git's name for "not committed yet".
pub(crate) fn blame(text: &str) -> Vec<BlameLine> {
    use std::collections::HashMap;
    #[derive(Default, Clone)]
    struct Who {
        author: String,
        time: u64,
        summary: String,
    }
    let mut known: HashMap<String, Who> = HashMap::new();
    let mut out = Vec::new();
    let mut current: Option<(String, u32)> = None;
    for raw in text.split('\n') {
        if let Some(content) = raw.strip_prefix('\t') {
            let _ = content;
            if let Some((sha, line)) = current.take() {
                let who = known.get(&sha).cloned().unwrap_or_default();
                let uncommitted = sha.chars().all(|c| c == '0');
                out.push(BlameLine {
                    line,
                    short: sha.chars().take(7).collect(),
                    commit: CommitId::new(&sha),
                    author: if uncommitted {
                        "Not committed yet".to_string()
                    } else {
                        who.author
                    },
                    timestamp: if uncommitted { 0 } else { who.time },
                    summary: if uncommitted {
                        String::new()
                    } else {
                        who.summary
                    },
                    uncommitted,
                });
            }
            continue;
        }
        if raw.is_empty() {
            continue;
        }
        match &mut current {
            None => {
                let mut f = raw.split(' ');
                let sha = f.next().unwrap_or_default();
                let is_sha = sha.len() >= 40 && sha.chars().all(|c| c.is_ascii_hexdigit());
                let final_line = f.nth(1).and_then(|n| n.parse::<u32>().ok());
                if let (true, Some(line)) = (is_sha, final_line) {
                    known.entry(sha.to_string()).or_default();
                    current = Some((sha.to_string(), line));
                }
            }
            Some((sha, _)) => {
                let who = known.entry(sha.clone()).or_default();
                if let Some(v) = raw.strip_prefix("author ") {
                    who.author = v.to_string();
                } else if let Some(v) = raw.strip_prefix("author-time ") {
                    who.time = v.trim().parse().unwrap_or(0);
                } else if let Some(v) = raw.strip_prefix("summary ") {
                    who.summary = v.to_string();
                }
            }
        }
    }
    out
}

/// Parse `git rev-list --left-right --count <base>...<head>` → `(behind, ahead)`.
/// Left is what only the base has (behind), right is what only HEAD has (ahead).
pub(crate) fn left_right(text: &str) -> (u32, u32) {
    let mut f = text.split_whitespace();
    let behind = f.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    let ahead = f.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    (behind, ahead)
}

/// Parse `%(upstream:track,nobracket)` → `(ahead, behind)`: `ahead 2`,
/// `behind 1`, `ahead 2, behind 1`, nothing when in step, `gone` when the
/// upstream no longer exists — read as 0/0.
pub(crate) fn upstream_track(text: &str) -> (u32, u32) {
    let mut ahead = 0;
    let mut behind = 0;
    for part in text.split(',') {
        let mut words = part.split_whitespace();
        match (
            words.next(),
            words.next().and_then(|n| n.parse::<u32>().ok()),
        ) {
            (Some("ahead"), Some(n)) => ahead = n,
            (Some("behind"), Some(n)) => behind = n,
            _ => {}
        }
    }
    (ahead, behind)
}

fn lossy_path(bytes: &[u8]) -> PathBuf {
    PathBuf::from(String::from_utf8_lossy(bytes).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// Every fixture in this module is verbatim output captured from
    /// git 2.50 against a throwaway repository, with NULs written as `\0`.
    const CLEAN: &str =
        "# branch.oid abecdaf3dea9659bbe0e9572ef5c72b5ec7e41a0\0# branch.head main\0";
    const EMPTY_REPO: &str = "# branch.oid (initial)\0# branch.head main\0";
    const DETACHED: &str =
        "# branch.oid d9c9ead0d59f831c86569403bad537feae8abfdb\0# branch.head (detached)\0";
    const AHEAD: &str = "# branch.oid 828ac972b950e1705aa9d025235903f5ae585b89\0# branch.head main\0# branch.upstream origin/main\0# branch.ab +1 -0\0";
    const UNMERGED: &str = "# branch.oid d9c9ead0d59f831c86569403bad537feae8abfdb\0# branch.head main\0u UU N... 100644 100644 100644 100644 5626abf0f72e58d7a153368ba57db4c673c0e171 2bdf67abb163a4ffb2d7f3f0880c9fe5068ce782 f719efd430d52bcfc8566a43b2eb655688d38871 c.txt\0";
    /// A staged rename that was then modified again, a modified unicode
    /// filename, and two untracked files, one with a space.
    const MIXED: &str = "# branch.oid abecdaf3dea9659bbe0e9572ef5c72b5ec7e41a0\0# branch.head main\0\
2 RM N... 100644 100644 100644 de980441c3ab03a8c07dda1ad27b8a11f39deb1e d68dd4031d2ad5b7a3829ad7df6635e27a7daa22 R75 renamed-a.txt\0a.txt\0\
1 .M N... 100644 100644 100644 4ae8ef021bf6fcfff43a13be5abfa52bb6fb5dbc 4ae8ef021bf6fcfff43a13be5abfa52bb6fb5dbc ünïcode-fïle.txt\0\
? untracked with space.txt\0? untracked.txt\0";

    #[test]
    fn status_table() {
        struct Case {
            name: &'static str,
            raw: &'static str,
            branch: Option<&'static str>,
            detached: bool,
            has_oid: bool,
            upstream: Option<&'static str>,
            ahead: u32,
            behind: u32,
            staged: u32,
            unstaged: u32,
            untracked: u32,
            conflicted: u32,
            clean: bool,
        }
        let cases = [
            Case {
                name: "clean",
                raw: CLEAN,
                branch: Some("main"),
                detached: false,
                has_oid: true,
                upstream: None,
                ahead: 0,
                behind: 0,
                staged: 0,
                unstaged: 0,
                untracked: 0,
                conflicted: 0,
                clean: true,
            },
            Case {
                name: "empty repo, unborn HEAD",
                raw: EMPTY_REPO,
                branch: Some("main"),
                detached: false,
                has_oid: false,
                upstream: None,
                ahead: 0,
                behind: 0,
                staged: 0,
                unstaged: 0,
                untracked: 0,
                conflicted: 0,
                clean: true,
            },
            Case {
                name: "detached HEAD",
                raw: DETACHED,
                branch: None,
                detached: true,
                has_oid: true,
                upstream: None,
                ahead: 0,
                behind: 0,
                staged: 0,
                unstaged: 0,
                untracked: 0,
                conflicted: 0,
                clean: true,
            },
            Case {
                name: "one commit ahead of upstream",
                raw: AHEAD,
                branch: Some("main"),
                detached: false,
                has_oid: true,
                upstream: Some("origin/main"),
                ahead: 1,
                behind: 0,
                staged: 0,
                unstaged: 0,
                untracked: 0,
                conflicted: 0,
                clean: true,
            },
            Case {
                name: "unmerged path",
                raw: UNMERGED,
                branch: Some("main"),
                detached: false,
                has_oid: true,
                upstream: None,
                ahead: 0,
                behind: 0,
                staged: 0,
                unstaged: 0,
                untracked: 0,
                conflicted: 1,
                clean: false,
            },
            Case {
                name: "rename + unicode + spaces",
                raw: MIXED,
                branch: Some("main"),
                detached: false,
                has_oid: true,
                upstream: None,
                ahead: 0,
                behind: 0,
                staged: 1,
                unstaged: 2,
                untracked: 2,
                conflicted: 0,
                clean: false,
            },
        ];

        for c in cases {
            let (st, _) = status(c.raw.as_bytes());
            assert_eq!(st.branch.as_deref(), c.branch, "{}: branch", c.name);
            assert_eq!(st.detached, c.detached, "{}: detached", c.name);
            assert_eq!(st.oid.is_some(), c.has_oid, "{}: oid", c.name);
            assert_eq!(st.upstream.as_deref(), c.upstream, "{}: upstream", c.name);
            assert_eq!(st.ahead, c.ahead, "{}: ahead", c.name);
            assert_eq!(st.behind, c.behind, "{}: behind", c.name);
            assert_eq!(st.staged, c.staged, "{}: staged", c.name);
            assert_eq!(st.unstaged, c.unstaged, "{}: unstaged", c.name);
            assert_eq!(st.untracked, c.untracked, "{}: untracked", c.name);
            assert_eq!(st.conflicted, c.conflicted, "{}: conflicted", c.name);
            assert_eq!(st.is_clean, c.clean, "{}: is_clean", c.name);
        }
    }

    #[test]
    fn status_keeps_the_path_of_every_record() {
        let (_, files) = status(MIXED.as_bytes());
        let row = |p: &str| {
            files
                .iter()
                .find(|f| f.path == Path::new(p))
                .unwrap_or_else(|| panic!("no row for {p} in {files:#?}"))
        };

        assert_eq!(files.len(), 4, "{files:#?}");

        // Staged as a rename, modified again since: the two letters are what
        // makes that visible, and one verdict could not say it.
        let renamed = row("renamed-a.txt");
        assert_eq!((renamed.index, renamed.worktree), ('R', 'M'));
        assert_eq!(renamed.old_path, Some(PathBuf::from("a.txt")));
        assert!(renamed.is_staged() && renamed.is_unstaged());

        // A non-ASCII name arrives whole, because `-z` means git never quotes.
        let unicode = row("ünïcode-fïle.txt");
        assert_eq!((unicode.index, unicode.worktree), ('.', 'M'));
        assert!(!unicode.is_staged() && unicode.is_unstaged());

        // A space in a name is not an edge case, and it is not a field break.
        let spaced = row("untracked with space.txt");
        assert!(spaced.untracked && spaced.is_unstaged() && !spaced.is_staged());
        assert_eq!((spaced.index, spaced.worktree), ('?', '?'));

        let (_, conflicted) = status(UNMERGED.as_bytes());
        assert_eq!(conflicted.len(), 1);
        assert!(conflicted[0].is_conflicted());
        assert_eq!(conflicted[0].conflict, Some(ConflictKind::BothModified));
        assert_eq!(conflicted[0].path, PathBuf::from("c.txt"));
        assert!(
            !conflicted[0].is_staged() && !conflicted[0].is_unstaged(),
            "an unmerged path is neither, and a surface must not offer to stage it blind"
        );
    }

    #[test]
    fn an_unmerged_records_letters_say_which_side_did_what() {
        let record = |xy: &str| {
            let raw = format!("# branch.oid d9c9\0# branch.head main\0u {xy} N... 100644 100644 100644 100644 5626 2bdf f719 c.txt\0");
            let (st, files) = status(raw.as_bytes());
            assert_eq!(st.conflicted, 1);
            files[0].conflict
        };
        assert_eq!(record("UU"), Some(ConflictKind::BothModified));
        assert_eq!(record("AA"), Some(ConflictKind::BothAdded));
        assert_eq!(record("DD"), Some(ConflictKind::BothDeleted));
        assert_eq!(record("DU"), Some(ConflictKind::DeletedByUs));
        assert_eq!(record("UD"), Some(ConflictKind::DeletedByThem));
        assert_eq!(record("AU"), Some(ConflictKind::AddedByUs));
        assert_eq!(record("UA"), Some(ConflictKind::AddedByThem));
        assert_eq!(
            record("XX"),
            Some(ConflictKind::BothModified),
            "a pair git does not write is still a conflict, of the commonest kind"
        );
        assert_eq!(
            ConflictKind::DeletedByThem.deleted_by(),
            Some(crate::git::ConflictSide::Theirs)
        );
        assert_eq!(ConflictKind::BothAdded.deleted_by(), None);
    }

    #[test]
    fn status_survives_truncated_and_empty_input() {
        let (empty, files) = status(b"");
        assert!(empty.is_clean && empty.branch.is_none() && files.is_empty());

        // A `2` record whose original-path field never arrived must not leave
        // the parser reading past the end.
        let (st, files) = status(b"2 R. N... 1 1 1 aaa bbb R100 new.txt\0");
        assert_eq!(st.staged, 1);
        assert_eq!(st.unstaged, 0);
        assert!(!st.is_clean);
        assert_eq!(files[0].path, PathBuf::from("new.txt"));
        assert_eq!(files[0].old_path, None, "the field simply was not there");

        // A record shorter than its XY field must not panic, and must not
        // invent a path it did not see.
        let (st, files) = status(b"1\0? x\0");
        assert_eq!(st.untracked, 1);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, PathBuf::from("x"));
    }

    const WORKTREES: &str = "\
worktree /tmp/fx/r
HEAD 828ac972b950e1705aa9d025235903f5ae585b89
branch refs/heads/main

worktree /tmp/fx/wt spaced
HEAD 828ac972b950e1705aa9d025235903f5ae585b89
branch refs/heads/feature
prunable gitdir file points to non-existent location

worktree /tmp/fx/wt-détaché
HEAD 828ac972b950e1705aa9d025235903f5ae585b89
detached
locked held for review
";

    #[test]
    fn worktree_list_table() {
        let entries = worktree_list(WORKTREES);
        assert_eq!(entries.len(), 3);

        assert_eq!(entries[0].path, PathBuf::from("/tmp/fx/r"));
        assert_eq!(entries[0].branch.as_deref(), Some("main"));
        assert!(!entries[0].detached && !entries[0].locked && !entries[0].prunable);
        assert_eq!(
            entries[0].head.as_deref(),
            Some("828ac972b950e1705aa9d025235903f5ae585b89")
        );

        // A space in the path is the whole remainder of the line.
        assert_eq!(entries[1].path, PathBuf::from("/tmp/fx/wt spaced"));
        assert_eq!(entries[1].branch.as_deref(), Some("feature"));
        assert!(entries[1].prunable);
        assert_eq!(
            entries[1].prune_reason.as_deref(),
            Some("gitdir file points to non-existent location")
        );

        assert_eq!(entries[2].path, PathBuf::from("/tmp/fx/wt-détaché"));
        assert!(entries[2].detached);
        assert_eq!(entries[2].branch, None);
        assert!(entries[2].locked);
        assert_eq!(entries[2].lock_reason.as_deref(), Some("held for review"));
    }

    #[test]
    fn worktree_list_handles_bare_and_valueless_lock() {
        let text =
            "worktree /tmp/origin.git\nbare\n\nworktree /tmp/w\nHEAD abc\ndetached\nlocked\n";
        let entries = worktree_list(text);
        assert_eq!(entries.len(), 2);
        assert!(entries[0].bare);
        assert!(entries[1].locked && entries[1].lock_reason.is_none());
        assert!(worktree_list("").is_empty());
    }

    #[test]
    fn numstat_table() {
        // Captured: a rename with edits, plus a modified unicode filename.
        let raw = "2\t0\t\0a.txt\0renamed-a.txt\0\
1\t0\tünïcode-fïle.txt\0\
-\t-\tlogo.png\0\
3\t4\tdir/file with space.txt\0";
        let rows = numstat(raw.as_bytes());
        assert_eq!(
            rows,
            vec![
                NumStat {
                    path: PathBuf::from("renamed-a.txt"),
                    old_path: Some(PathBuf::from("a.txt")),
                    insertions: 2,
                    deletions: 0,
                    binary: false,
                },
                NumStat {
                    path: PathBuf::from("ünïcode-fïle.txt"),
                    old_path: None,
                    insertions: 1,
                    deletions: 0,
                    binary: false,
                },
                NumStat {
                    path: PathBuf::from("logo.png"),
                    old_path: None,
                    insertions: 0,
                    deletions: 0,
                    binary: true,
                },
                NumStat {
                    path: PathBuf::from("dir/file with space.txt"),
                    old_path: None,
                    insertions: 3,
                    deletions: 4,
                    binary: false,
                },
            ]
        );
        assert!(numstat(b"").is_empty());
    }

    #[test]
    fn name_status_table() {
        let raw = "R060\0a.txt\0renamed-a.txt\0M\0ünïcode-fïle.txt\0A\0new one.txt\0D\0gone.txt\0C100\0src.txt\0copy.txt\0U\0conflicted.txt\0T\0link.txt\0";
        let rows = name_status(raw.as_bytes());
        assert_eq!(
            rows,
            vec![
                (
                    PathBuf::from("renamed-a.txt"),
                    ChangeKind::Renamed,
                    Some(PathBuf::from("a.txt"))
                ),
                (
                    PathBuf::from("ünïcode-fïle.txt"),
                    ChangeKind::Modified,
                    None
                ),
                (PathBuf::from("new one.txt"), ChangeKind::Added, None),
                (PathBuf::from("gone.txt"), ChangeKind::Deleted, None),
                (
                    PathBuf::from("copy.txt"),
                    ChangeKind::Copied,
                    Some(PathBuf::from("src.txt"))
                ),
                (PathBuf::from("conflicted.txt"), ChangeKind::Unmerged, None),
                (PathBuf::from("link.txt"), ChangeKind::TypeChanged, None),
            ]
        );
        // A trailing status letter with no path must not panic.
        assert_eq!(name_status(b"M\0").len(), 0);
        assert_eq!(name_status(b"R100\0only-old.txt\0").len(), 0);
    }

    #[test]
    fn log_table() {
        let raw = "828ac97\u{1f}828ac97\u{1f}test\u{1f}t@e.invalid\u{1f}1787757299\u{1f}four\u{1e}\n\
d9c9ead\u{1f}d9c9ead\u{1f}Ada L\u{1f}ada@e.invalid\u{1f}1787757290\u{1f}subject: with, punctuation\u{1e}\n";
        let commits = log(raw);
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].id.as_str(), "828ac97");
        assert_eq!(commits[0].subject, "four");
        assert_eq!(commits[1].author, "Ada L");
        assert_eq!(commits[1].timestamp, 1_787_757_290);
        assert_eq!(commits[1].subject, "subject: with, punctuation");
        assert!(log("").is_empty());
    }

    #[test]
    fn left_right_counts() {
        assert_eq!(left_right("0\t1\n"), (0, 1));
        assert_eq!(left_right("3\t7"), (3, 7));
        assert_eq!(left_right(""), (0, 0));
    }

    #[test]
    fn upstream_track_reads_every_shape_git_prints() {
        assert_eq!(upstream_track(""), (0, 0), "in step, or no upstream");
        assert_eq!(upstream_track("ahead 2"), (2, 0));
        assert_eq!(upstream_track("behind 1"), (0, 1));
        assert_eq!(upstream_track("ahead 2, behind 1"), (2, 1));
        assert_eq!(
            upstream_track("gone"),
            (0, 0),
            "an upstream that no longer exists"
        );
    }

    #[test]
    fn blame_porcelain_remembers_authors_per_commit_and_names_uncommitted_lines() {
        let sha = "a".repeat(40);
        let zero = "0".repeat(40);
        let text = format!(
            "{sha} 1 1 2\nauthor Ada\nauthor-mail <ada@example.invalid>\nauthor-time 1700000000\nsummary first\nfilename f.txt\n\thello\n{sha} 2 2\n\tworld\n{zero} 3 3 1\nauthor Not Committed Yet\nauthor-time 1700000001\nsummary Version of f.txt from f.txt\n\tnew line\n"
        );
        let rows = blame(&text);
        assert_eq!(rows.len(), 3, "{rows:#?}");
        assert_eq!(rows[0].line, 1);
        assert_eq!(rows[0].author, "Ada");
        assert_eq!(rows[0].timestamp, 1_700_000_000);
        assert_eq!(rows[0].summary, "first");
        assert_eq!(rows[0].short, "aaaaaaa");
        // The second line of the same commit carries no headers; it is filled in.
        assert_eq!(rows[1].line, 2);
        assert_eq!(rows[1].author, "Ada");
        assert!(!rows[1].uncommitted);
        assert!(rows[2].uncommitted);
        assert_eq!(rows[2].author, "Not committed yet");
        assert_eq!(rows[2].timestamp, 0);
    }

    #[test]
    fn decorations_classify_full_and_short_ref_names() {
        let refs =
            decorations("HEAD -> refs/heads/main, refs/remotes/origin/main, tag: refs/tags/v1.0");
        let pairs: Vec<(&str, &str)> = refs
            .iter()
            .map(|r| (r.name.as_str(), r.kind.as_str()))
            .collect();
        assert_eq!(
            pairs,
            vec![
                ("HEAD", "head"),
                ("main", "branch"),
                ("origin/main", "remote"),
                ("v1.0", "tag")
            ]
        );
        // A recovery point is no chip: Safety shows it.
        let with_safety = decorations(
            "HEAD -> refs/heads/main, refs/bisa/safety/1700000000-checkout.wip, refs/tags/v1",
        );
        let pairs: Vec<(&str, &str)> = with_safety
            .iter()
            .map(|r| (r.name.as_str(), r.kind.as_str()))
            .collect();
        assert_eq!(
            pairs,
            vec![("HEAD", "head"), ("main", "branch"), ("v1", "tag")]
        );
        let short = decorations("HEAD -> main, origin/main, tag: v2");
        let pairs: Vec<(&str, &str)> = short
            .iter()
            .map(|r| (r.name.as_str(), r.kind.as_str()))
            .collect();
        assert_eq!(
            pairs,
            vec![
                ("HEAD", "head"),
                ("main", "branch"),
                ("origin/main", "remote"),
                ("v2", "tag")
            ]
        );
        assert!(decorations("").is_empty());
    }

    #[test]
    fn graph_log_reads_parents_first_parent_first() {
        let a = "a".repeat(40);
        let b = "b".repeat(40);
        let c = "c".repeat(40);
        let text = format!(
            "{a}\u{1f}aaaaaaa\u{1f}{b} {c}\u{1f}HEAD -> refs/heads/main\u{1f}Ada\u{1f}ada@x\u{1f}10\u{1f}merge\u{1e}\n{b}\u{1f}bbbbbbb\u{1f}\u{1f}\u{1f}Ada\u{1f}ada@x\u{1f}9\u{1f}root\u{1e}\n"
        );
        let rows = graph_log(&text);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].parents, vec![CommitId::new(&b), CommitId::new(&c)]);
        assert_eq!(rows[0].refs[1].name, "main");
        assert!(rows[1].parents.is_empty());
        assert!(rows[1].refs.is_empty());
        assert_eq!(rows[1].subject, "root");
    }

    #[test]
    fn commit_detail_keeps_a_body_with_separators_in_it() {
        let a = "a".repeat(40);
        let text = format!(
            "{a}\u{1f}aaaaaaa\u{1f}\u{1f}tag: refs/tags/v1\u{1f}Ada\u{1f}ada@x\u{1f}7\u{1f}subject\u{1f}line one\n\nline\u{1f}with a separator\n"
        );
        let d = commit_detail(&text).unwrap();
        assert_eq!(d.subject, "subject");
        assert_eq!(d.body, "line one\n\nline\u{1f}with a separator");
        assert_eq!(d.refs[0].kind, "tag");
        assert!(d.files.is_empty());
    }

    #[test]
    fn the_stash_list_reads_index_by_position_the_branch_the_message_and_untracked() {
        let a = "a".repeat(40);
        let b = "b".repeat(40);
        let p = "1".repeat(40);
        let q = "2".repeat(40);
        let r = "3".repeat(40);
        // Newest first, as `log -g refs/stash` walks: a message with untracked
        // files (three parents), then git's own WIP subject (two parents).
        let text = format!(
            "{a}\u{1f}stash@{{0}}\u{1f}{p} {q} {r}\u{1f}20\u{1f}On feature: half a change\u{1e}\n\
             {b}\u{1f}stash@{{1}}\u{1f}{p} {q}\u{1f}10\u{1f}WIP on main: 1234567 baseline\u{1e}\n"
        );
        let rows = stash_list(&text);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].index, 0);
        assert_eq!(rows[0].commit, CommitId::new(&a));
        assert_eq!(rows[0].branch.as_deref(), Some("feature"));
        assert_eq!(rows[0].message.as_deref(), Some("half a change"));
        assert_eq!(rows[0].subject, "On feature: half a change");
        assert_eq!(rows[0].at, 20);
        assert!(rows[0].untracked);
        assert_eq!(rows[1].index, 1);
        assert_eq!(rows[1].branch.as_deref(), Some("main"));
        assert_eq!(
            rows[1].message, None,
            "git's own WIP subject is not a message"
        );
        assert!(!rows[1].untracked);
        assert!(stash_list("").is_empty());
    }

    #[test]
    fn a_stash_subject_splits_on_the_first_colon_and_a_detached_head_has_no_branch() {
        assert_eq!(
            stash_subject("On main: fix: the colon stays"),
            (Some("main".into()), Some("fix: the colon stays".into()))
        );
        assert_eq!(
            stash_subject("WIP on (no branch): abc1234 detached work"),
            (None, None)
        );
        assert_eq!(
            stash_subject("On (no branch): kept"),
            (None, Some("kept".into()))
        );
        assert_eq!(
            stash_subject("something else entirely"),
            (None, Some("something else entirely".into()))
        );
        assert_eq!(stash_subject(""), (None, None));
    }
}
