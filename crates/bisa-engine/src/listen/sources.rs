//! The ticker: what comes due on a clock, and what is only seen by looking.
//!
//! Every `events.tick_secs` — when `events.enabled` is on — each armed
//! listener with a cadence is asked whether it came due: a **schedule**
//! occurs; a **connector** start polls its read operation, and every item the
//! answer lists that an earlier poll did not is one occurrence (the first
//! poll learns what is there); a **check** runs its command, judged by the
//! guard like a `check` step, and occurs when its `fire_on` says so. A
//! listener's memory — when it next comes due, what it has seen — is its
//! [`bisa_store::ListenerRuntime`]; a first sight arms and fires nothing, and
//! an occurrence the downtime covered fires once.
//!
//! A **project** is looked at, never listened to — its watcher runs only for
//! the roots a person has open, and the code host sends nothing. So each
//! tick reads, for each project a listener or a wait names, what it needs:
//! its branch heads (`for-each-ref refs/heads`, read-only), its
//! remote-tracking branches (this machine pushed or fetched), its files (a
//! git project's status, a plain folder's bounded scan), and — at
//! `events.pr_poll_secs`, or sooner when a workstream moved — the state of the
//! pull requests its workstreams opened or adopted. What moved is an
//! occurrence: for each listener against its own memory, for the waits
//! against the ticker's.
//!
//! The same pass lets a listener's waiting signals go when its guard has
//! room ([`super::dispatch::release_ready`]).

use super::ear::enqueue_for;
use super::registry::{put_runtime, runtime_of, Armed};
use super::{armed, now_secs, report_once};
use crate::Inner;
use bisa_core::{
    Cadence, Heard, ListenerKey, ProjectChange, ProjectFilter, ProjectId, SignalScope,
    SignalSource, StartOn, ValueRef, Vcs,
};
use bisa_store::{ListenerRuntime, WorkstreamFilter};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// How many item keys a connector poll remembers per listener: the newest
/// stay, the oldest go.
pub const SEEN_KEYS: usize = 1_000;
/// The commits and paths a branch's move carries at most.
const MOVE_COMMITS: usize = 20;
const MOVE_PATHS: usize = 200;

/// The background ticker. Its period is `events.tick_secs`, read anew every
/// turn so a change lands without a restart.
pub async fn run_ticker(inner: Arc<Inner>) {
    loop {
        tokio::time::sleep(inner.listen.settings().tick()).await;
        inner.pause.wait_running().await;
        crate::survive("event sources", tick(&inner, now_secs())).await;
    }
}

/// One pass at an explicit clock — a schedule is otherwise untestable
/// without waiting for it. `events.enabled` off touches nothing: a schedule
/// keeps its `next_due`, so turning it back on picks up where it stopped.
pub async fn tick(inner: &Arc<Inner>, now: u64) {
    if !inner.listen.settings().enabled {
        return;
    }
    // A tick is when anything missed is picked up: the registry is built
    // afresh from the records.
    super::invalidate(inner);
    let registry = armed(inner);
    for listener in &registry.armed {
        match &listener.on {
            StartOn::Schedule { .. } => schedule(inner, listener, now),
            StartOn::Connector { .. } => poll(inner, listener, now).await,
            StartOn::Check { .. } => check(inner, listener, now).await,
            StartOn::Manual
            | StartOn::Hook { .. }
            | StartOn::Message { .. }
            | StartOn::Signal { .. }
            | StartOn::Project { .. }
            | StartOn::Run { .. }
            | StartOn::Platform { .. } => {}
        }
    }
    projects(inner, &registry.armed, now).await;
    for listener in &registry.armed {
        super::dispatch::release_ready(inner, &listener.key);
    }
}

// ---------------------------------------------------------------------------
// Cadences
// ---------------------------------------------------------------------------

/// Whether a listener with a cadence came due at `now`. A first sight arms
/// and answers `None`: turning a host on is not an occurrence.
fn due(inner: &Inner, listener: &Armed, rt: &mut ListenerRuntime, now: u64) -> Option<u64> {
    let cadence = match cadence_of(listener) {
        Ok(c) => c,
        // LCOV_EXCL_START: a cadence read from the inputs is judged when they bind (validate_bound, cron_from) and a fixed one at design; none reaches the tick wrong
        Err(e) => {
            report_once(
                inner,
                &listener.key,
                None,
                format!("its cadence is wrong: {e}"),
            );
            return None;
            // LCOV_EXCL_STOP
        }
    };
    match rt.next_due {
        None => {
            match super::schedule::next_due(&cadence, now) {
                Ok(next) => rt.next_due = Some(next),
                Err(e) => {
                    report_once(inner, &listener.key, None, e);
                }
            }
            put_runtime(inner, &listener.key, rt);
            None
        }
        Some(due) if now >= due => Some(due),
        Some(_) => None,
    }
}

fn cadence_of(listener: &Armed) -> Result<Cadence, String> {
    listener
        .on
        .schedule()
        .ok_or_else(|| "it has no schedule".to_string())?
        .cadence()
        .map_err(|e| e.to_string())
}

/// The cadence consumed: the next due time from now, whatever came of this
/// one — an occurrence the downtime covered fires once.
fn advance(inner: &Inner, listener: &Armed, rt: &mut ListenerRuntime, now: u64) {
    rt.next_due = cadence_of(listener)
        .and_then(|c| super::schedule::next_due(&c, now))
        .ok();
    put_runtime(inner, &listener.key, rt);
}

fn schedule(inner: &Arc<Inner>, listener: &Armed, now: u64) {
    let mut rt = runtime_of(inner, listener);
    let Some(at) = due(inner, listener, &mut rt, now) else {
        return;
    };
    super::ear::occur(
        inner,
        listener,
        json!({ "at": at }),
        &format!("schedule:{at}"),
        None,
    );
    advance(inner, listener, &mut rt, now);
}

// ---------------------------------------------------------------------------
// Connector polls
// ---------------------------------------------------------------------------

async fn poll(inner: &Arc<Inner>, listener: &Armed, now: u64) {
    let mut rt = runtime_of(inner, listener);
    if due(inner, listener, &mut rt, now).is_none() {
        return;
    }
    match poll_items(inner, listener, &mut rt).await {
        Ok(items) => {
            for (id, item) in items {
                let text = item.to_string();
                if let Some(signal) = super::ear::occur(
                    inner,
                    listener,
                    json!({ "id": id, "item": item }),
                    &format!("poll:{id}"),
                    screen_note(inner),
                ) {
                    if screen_note(inner).is_some() {
                        super::hooks::screen_held(
                            inner,
                            listener.key.clone(),
                            signal,
                            format!("a poll of {}", listener.key),
                            text,
                        );
                    }
                }
            }
            super::healthy(inner, &listener.key);
        }
        Err(why) => {
            report_once(inner, &listener.key, None, why);
        }
    }
    advance(inner, listener, &mut rt, now);
}

/// With the content screen on, an outside payload is held for it first.
fn screen_note(inner: &Inner) -> Option<&'static str> {
    inner
        .security
        .policy()
        .content
        .screen
        .then_some(super::hooks::HELD_FOR_SCREEN)
}

/// One poll of a connector's read operation: the answer's items — the
/// selected value as a list, a lone object as one item — each keyed by the
/// field `key` names; a key seen on an earlier poll is not new, and the
/// first poll learns what is there and answers nothing. A refused host, a
/// failed call or a missing account is the poll's error, and nothing is
/// remembered from a poll that failed.
async fn poll_items(
    inner: &Arc<Inner>,
    listener: &Armed,
    rt: &mut ListenerRuntime,
) -> Result<Vec<(String, Value)>, String> {
    let StartOn::Connector {
        connector: Some(connector),
        operation: Some(operation),
        account,
        params,
        key: Some(key),
        ..
    } = &listener.on
    else {
        // LCOV_EXCL_START: a connector start with no connector, operation or key is a problem at design (Unfilled) and never armed
        return Err("its connector, operation or key is not chosen".into());
        // LCOV_EXCL_STOP
    };
    let def = inner
        .ws
        .get_connector(connector)
        .map_err(|e| format!("connector {connector}: {e}"))?;
    let op = def
        .operation(operation)
        .ok_or_else(|| format!("connector {connector} has no operation `{operation}`"))?;
    if op.writes {
        // LCOV_EXCL_START: a start that polls a write is a problem at design (BadPoll, a_connector_start_learns_then_fires_once_per_new_item) and never armed
        return Err(format!(
            "`{connector}.{operation}` writes; a start polls a read operation and never writes"
        ));
        // LCOV_EXCL_STOP
    }
    let account = match account {
        Some(ValueRef::Fixed(a)) => Some(*a),
        _ => None,
    };
    let acct = crate::connectors::resolve_account(&inner.ws, &def, account)
        .map_err(|e| crate::connectors::redact_reason(inner, &e))?;
    let timeout = crate::connectors::operation_timeout(inner, op) + crate::connectors::OVERHEAD;
    // The hosts a call may reach: the declared ones and, for an OAuth2
    // scheme, the consent page's and the token endpoint's.
    let declared = def.declared_hosts();
    let judge = crate::connectors::PolicyHostJudge {
        inner,
        home: listener.key.host.goal().map(bisa_core::Home::from),
        subject: format!("{} {}{}", op.method.as_str(), def.hosts.join("|"), op.path),
        declared: &declared,
    };
    let outcome = tokio::time::timeout(
        timeout,
        crate::connectors::invoke(
            inner,
            crate::connectors::Invocation {
                deadline: None,
                def: &def,
                op,
                account: acct.as_ref(),
                params,
                files: &bisa_connectors::NoFiles,
                judge: &judge,
                key: None,
                caller: crate::connectors::Caller::Poll,
            },
        ),
    )
    .await
    .map_err(|_| format!("the poll timed out after {}s", timeout.as_secs()))?
    .map_err(|e| inner.security.redact(&e.to_string()).text)?;
    let items: Vec<Value> = match outcome.selected {
        Value::Array(items) => items,
        Value::Null => Vec::new(),
        one => vec![one],
    };
    let baseline = rt.seen.is_none();
    let mut seen = rt.seen.take().unwrap_or_default();
    let mut new = Vec::new();
    let mut keyless = 0usize;
    for item in items {
        let Some(id) = bisa_core::json_path(&item, key).map(|v| match v {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        }) else {
            keyless += 1;
            continue;
        };
        if seen.contains(&id) {
            continue;
        }
        seen.push(id.clone());
        if !baseline {
            new.push((id, item));
        }
    }
    if seen.len() > SEEN_KEYS {
        let drop = seen.len() - SEEN_KEYS;
        seen.drain(..drop);
    }
    rt.seen = Some(seen);
    if keyless > 0 {
        report_once(
            inner,
            &listener.key,
            None,
            format!("{keyless} item(s) the answer listed have no `{key}`; skipped"),
        );
    }
    Ok(new)
}

// ---------------------------------------------------------------------------
// Check starts
// ---------------------------------------------------------------------------

async fn check(inner: &Arc<Inner>, listener: &Armed, now: u64) {
    let mut rt = runtime_of(inner, listener);
    let Some(at) = due(inner, listener, &mut rt, now) else {
        return;
    };
    let StartOn::Check {
        command,
        project,
        fire_on,
        ..
    } = &listener.on
    else {
        // LCOV_EXCL_START: check_listener is called for a check start alone; the guard keeps the function total
        return;
        // LCOV_EXCL_STOP
    };
    let cwd = check_cwd(inner, &listener.key, project.as_ref());
    let (code, output) = run_check(inner, &listener.key, command, &cwd).await;
    let passed = code == Some(0);
    if fire_on.fires(passed, rt.last_passed) {
        super::ear::occur(
            inner,
            listener,
            json!({
                "exit_code": code,
                "passed": passed,
                "output": output,
                "command": command,
            }),
            &format!("check:{at}"),
            None,
        );
    }
    rt.last_passed = Some(passed);
    advance(inner, listener, &mut rt, now);
}

/// Where a check start's command runs: its project's tree, or its own
/// scratch folder.
fn check_cwd(inner: &Inner, key: &ListenerKey, project: Option<&ValueRef<ProjectId>>) -> PathBuf {
    if let Some(ValueRef::Fixed(p)) = project {
        if let Ok(project) = inner.ws.get_project(*p) {
            return inner.ws.project_root_path(&project);
        }
    }
    let scratch = inner.ws.paths().listener_scratch_dir(key);
    crate::warn_on_err(
        std::fs::create_dir_all(&scratch),
        "making a check start's folder",
    );
    scratch
}

/// Run a check start's command. `None` for the code means it timed out,
/// could not be spawned or was refused by the guard — all of which fail. The
/// guard judges the command at every fire: a rule written after the start
/// still applies.
async fn run_check(
    inner: &Arc<Inner>,
    key: &ListenerKey,
    command: &str,
    cwd: &Path,
) -> (Option<i32>, String) {
    let home = key.host.goal().map(bisa_core::Home::from);
    let judge = crate::security::Judge::platform(home, Some(cwd));
    let command = match crate::security::decide_command(inner, command, judge).await {
        crate::security::CommandOutcome::Run { command, .. } => command,
        crate::security::CommandOutcome::Refused(reason) => {
            return (None, format!("the check was refused: {reason}"))
        }
    };
    let timeout = inner.listen.settings().check_timeout();
    // In a group of its own: a check that times out takes what it started
    // with it — never `sh` alone.
    let mut cmd = tokio::process::Command::new("sh");
    cmd.arg("-c").arg(&command).current_dir(cwd);
    let run = tokio::time::timeout(timeout, bisa_harness::proc::group_output(cmd)).await;
    match run {
        Ok(Ok(out)) => {
            let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
            text.push_str(&String::from_utf8_lossy(&out.stderr));
            (
                out.status.code(),
                inner.security.redact(&crate::scripts::tail(&text)).text,
            )
        }
        Ok(Err(e)) => (None, format!("the check failed to spawn: {e}")),
        Err(_) => (None, format!("the check timed out after {timeout:?}")),
    }
}

// ---------------------------------------------------------------------------
// Projects
// ---------------------------------------------------------------------------

/// What one project is looked at for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Needs {
    heads: bool,
    remotes: bool,
    files: bool,
    prs: bool,
}

impl Needs {
    fn of(change: ProjectChange) -> Self {
        match change {
            ProjectChange::Commit => Needs {
                heads: true,
                ..Needs::default()
            },
            ProjectChange::Push => Needs {
                remotes: true,
                ..Needs::default()
            },
            ProjectChange::Files => Needs {
                files: true,
                ..Needs::default()
            },
            ProjectChange::PullRequest | ProjectChange::Merge => Needs {
                prs: true,
                ..Needs::default()
            },
        }
    }

    fn with(self, other: Needs) -> Needs {
        Needs {
            heads: self.heads || other.heads,
            remotes: self.remotes || other.remotes,
            files: self.files || other.files,
            prs: self.prs || other.prs,
        }
    }
}

/// A project as one look saw it. `refs` are full ref names — `refs/heads/…`,
/// `refs/remotes/…` — to their commit; `files` a path to what git's status or
/// a scan's mtime said of it; `prs` a pull request's number to its state.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ProjectView {
    refs: BTreeMap<String, String>,
    files: Option<BTreeMap<String, String>>,
    prs: Option<BTreeMap<String, PullRequestSeen>>,
}

/// A pull request as the code host answered.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PullRequestSeen {
    state: String,
    url: String,
    title: String,
    head: String,
    base: String,
}

/// One project's look, for everyone who named it.
async fn projects(inner: &Arc<Inner>, listeners: &[Armed], now: u64) {
    let mut wanted: BTreeMap<ProjectId, Needs> = BTreeMap::new();
    for listener in listeners {
        if let StartOn::Project { filter } = &listener.on {
            if let Some(p) = filter.project_id() {
                let needs = wanted.entry(p).or_default();
                *needs = needs.with(Needs::of(filter.change));
            }
        }
    }
    for (p, change) in crate::waits::watched_projects(inner) {
        let needs = wanted.entry(p).or_default();
        *needs = needs.with(Needs::of(change));
    }
    if wanted.is_empty() {
        return;
    }
    let prs_due = inner
        .listen
        .prs_due
        .swap(false, std::sync::atomic::Ordering::Relaxed);
    for (project, needs) in wanted {
        let Ok(record) = inner.ws.get_project(project) else {
            continue;
        };
        if record.is_archived() {
            continue;
        }
        let view = match look(inner, &record, needs, prs_due, now).await {
            Ok(view) => view,
            Err(e) => {
                tracing::debug!(target: "bisa_engine::listen", %project, "the project could not be looked at: {e}");
                continue;
            }
        };
        for listener in listeners {
            let StartOn::Project { filter } = &listener.on else {
                continue;
            };
            if filter.project_id() != Some(project) {
                continue;
            }
            project_listener(inner, listener, filter, &record, &view).await;
        }
        project_waits(inner, &record, &view).await;
    }
}

/// Look at one project for what is needed of it.
async fn look(
    inner: &Arc<Inner>,
    project: &bisa_core::Project,
    needs: Needs,
    prs_due: bool,
    now: u64,
) -> Result<ProjectView, String> {
    let root = inner.ws.project_root_path(project);
    let git = matches!(project.vcs, Vcs::Git { .. });
    let bounds = inner.listen.settings().scan_bounds();
    let handle = inner.git();
    let (refs, files) = tokio::task::spawn_blocking({
        let root = root.clone();
        move || -> Result<_, String> {
            let mut refs = BTreeMap::new();
            if git && needs.heads {
                for b in handle.branch_list(&root, None).map_err(|e| e.to_string())? {
                    refs.insert(
                        format!("refs/heads/{}", b.name),
                        b.head.as_str().to_string(),
                    );
                }
            }
            if git && needs.remotes {
                for b in handle
                    .remote_branch_list(&root)
                    .map_err(|e| e.to_string())?
                {
                    refs.insert(
                        format!("refs/remotes/{}/{}", b.remote, b.name),
                        b.head.as_str().to_string(),
                    );
                }
            }
            let files = match (needs.files, git) {
                (false, _) => None,
                (true, true) => Some(
                    handle
                        .status_files(&root)
                        .map_err(|e| e.to_string())?
                        .into_iter()
                        .map(|f| {
                            (
                                f.path.to_string_lossy().into_owned(),
                                format!("{}{}", f.index, f.worktree),
                            )
                        })
                        .collect(),
                ),
                (true, false) => Some(scan_tree(&root, bounds)),
            };
            Ok((refs, files))
        }
    })
    .await
    .map_err(|e| e.to_string())??;
    let prs = if needs.prs && git {
        pull_requests(inner, project.id, prs_due, now).await
    } else {
        None
    };
    Ok(ProjectView { refs, files, prs })
}

/// The pull requests a project's workstreams opened or adopted, asked of the
/// code host at `events.pr_poll_secs` — sooner when a workstream moved.
async fn pull_requests(
    inner: &Arc<Inner>,
    project: ProjectId,
    due: bool,
    now: u64,
) -> Option<BTreeMap<String, PullRequestSeen>> {
    let every = inner.listen.settings().pr_poll().as_secs();
    let last = inner
        .listen
        .prs_polled
        .get(&project)
        .map(|at| *at)
        .unwrap_or(0);
    if !due && now.saturating_sub(last) < every {
        return inner.listen.prs_seen.get(&project).map(|v| v.clone());
    }
    inner.listen.prs_polled.insert(project, now);
    let workstreams = inner
        .ws
        .list_workstreams(WorkstreamFilter::Project(project))
        .ok()?;
    let mut out = BTreeMap::new();
    for w in workstreams {
        if !matches!(
            w.state,
            bisa_core::WorkstreamState::PrOpen { .. } | bisa_core::WorkstreamState::Merged { .. }
        ) {
            continue;
        }
        // Asked, never read from the cache: a poll exists to see a change.
        let asked = tokio::time::timeout(
            Duration::from_secs(30),
            crate::codehost::linked_pr_fresh(inner, w.id),
        )
        .await;
        if let Ok(Ok(Some(pr))) = asked {
            out.insert(
                pr.number.to_string(),
                PullRequestSeen {
                    state: pr.state.as_str().to_string(),
                    url: pr.url,
                    title: pr.title,
                    head: pr.head,
                    base: pr.base,
                },
            );
        }
    }
    inner.listen.prs_seen.insert(project, out.clone());
    Some(out)
}

/// A plain folder's files and their mtimes, bounded by `events.files.*`,
/// never following a symlink: a cycle cannot defeat the bounds, and hitting
/// one reports what was seen.
fn scan_tree(root: &Path, bounds: bisa_core::ScanBounds) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut queue: Vec<(PathBuf, usize)> = vec![(root.to_path_buf(), 0)];
    let mut examined = 0usize;
    while let Some((dir, depth)) = queue.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            if examined >= bounds.entries {
                return out;
            }
            examined += 1;
            let Ok(meta) = entry.path().symlink_metadata() else {
                continue;
            };
            if meta.is_symlink() {
                continue;
            }
            if meta.is_dir() {
                if depth + 1 < bounds.depth {
                    queue.push((entry.path(), depth + 1));
                }
                continue;
            }
            let Some(mtime) = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
            else {
                continue;
            };
            let rel = entry
                .path()
                .strip_prefix(root)
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
            out.insert(rel, mtime.to_string());
        }
    }
    out
}

/// The refs under `prefix` that moved from `before` to `now`: new ones too.
fn moved(
    before: &BTreeMap<String, String>,
    now: &BTreeMap<String, String>,
    prefix: &str,
) -> Vec<(String, Option<String>, String)> {
    now.iter()
        .filter(|(r, _)| r.starts_with(prefix))
        .filter(|(r, sha)| before.get(*r) != Some(*sha))
        .map(|(r, sha)| (r.clone(), before.get(r).cloned(), sha.clone()))
        .collect()
}

/// The paths whose status or mtime changed between two looks — appeared,
/// changed or went.
fn changed_files(before: &BTreeMap<String, String>, now: &BTreeMap<String, String>) -> Vec<String> {
    let mut out: BTreeSet<String> = BTreeSet::new();
    for (path, state) in now {
        if before.get(path) != Some(state) {
            out.insert(path.clone());
        }
    }
    for path in before.keys() {
        if !now.contains_key(path) {
            out.insert(path.clone());
        }
    }
    out.into_iter().take(MOVE_PATHS).collect()
}

/// What a branch's move carries: its commits (newest first), the paths it
/// changed, and whether it was forced — history the old tip had and the new
/// one lacks.
async fn describe_move(
    inner: &Arc<Inner>,
    root: &Path,
    before: Option<&str>,
    after: &str,
) -> (Vec<Value>, Vec<String>, bool) {
    let Some(before) = before.map(str::to_string) else {
        return (Vec::new(), Vec::new(), false);
    };
    let handle = inner.git();
    let root = root.to_path_buf();
    let after = after.to_string();
    tokio::task::spawn_blocking(move || {
        let commits = handle
            .commits_between(&root, &after, &before, MOVE_COMMITS)
            .unwrap_or_default()
            .into_iter()
            .map(|c| {
                json!({
                    "id": c.id.as_str(),
                    "short": c.short,
                    "subject": c.subject,
                    "author": c.author,
                })
            })
            .collect();
        let paths = handle
            .changed_paths(&root, &before, &after, MOVE_PATHS)
            .unwrap_or_default();
        let forced = handle
            .commits_between(&root, &before, &after, 1)
            .is_ok_and(|lost| !lost.is_empty());
        (commits, paths, forced)
    })
    .await
    .unwrap_or_default()
}

/// Every change a look shows against a memory, as the heards a project
/// filter reads, each with its dedupe key.
async fn changes(
    inner: &Arc<Inner>,
    project: &bisa_core::Project,
    view: &ProjectView,
    refs_before: Option<&BTreeMap<String, String>>,
    files_before: Option<&BTreeMap<String, String>>,
    prs_before: Option<&BTreeMap<String, String>>,
    scope: &SignalScope,
) -> Vec<(Heard, String)> {
    let root = inner.ws.project_root_path(project);
    let base = |change: &str| {
        json!({
            "project": project.id.to_string(),
            "slug": project.slug.as_str(),
            "change": change,
        })
    };
    let heard = |payload: Value, chain: bisa_core::Chain| Heard {
        source: SignalSource::Project,
        name: None,
        scope: scope.clone(),
        payload,
        chain,
    };
    let mut out = Vec::new();
    if let Some(before) = refs_before {
        for (r, from, to) in moved(before, &view.refs, "refs/heads/") {
            let branch = r.trim_start_matches("refs/heads/").to_string();
            let (commits, paths, forced) = describe_move(inner, &root, from.as_deref(), &to).await;
            let mut payload = base("commit");
            payload["branch"] = json!(branch);
            payload["before"] = json!(from);
            payload["after"] = json!(to);
            payload["forced"] = json!(forced);
            payload["commits"] = json!(commits);
            payload["paths"] = json!(paths);
            let chain = super::run_of_commit(inner, &to)
                .map(|run| super::chain_of_run(inner, run))
                .unwrap_or_default();
            out.push((heard(payload, chain), format!("commit:{branch}@{to}")));
        }
        for (r, from, to) in moved(before, &view.refs, "refs/remotes/") {
            let rest = r.trim_start_matches("refs/remotes/");
            let (remote, branch) = rest.split_once('/').unwrap_or((rest, ""));
            let (commits, paths, forced) = describe_move(inner, &root, from.as_deref(), &to).await;
            let mut payload = base("push");
            payload["remote"] = json!(remote);
            payload["branch"] = json!(branch);
            payload["before"] = json!(from);
            payload["after"] = json!(to);
            payload["forced"] = json!(forced);
            payload["commits"] = json!(commits);
            payload["paths"] = json!(paths);
            let chain = super::run_of_commit(inner, &to)
                .map(|run| super::chain_of_run(inner, run))
                .unwrap_or_default();
            out.push((heard(payload, chain), format!("push:{rest}@{to}")));
        }
    }
    if let (Some(before), Some(now)) = (files_before, view.files.as_ref()) {
        let paths = changed_files(before, now);
        if !paths.is_empty() {
            let digest = hex::encode(Sha256::digest(serde_json::to_vec(&now).unwrap_or_default()));
            let mut payload = base("files");
            payload["paths"] = json!(paths);
            out.push((
                heard(payload, bisa_core::Chain::default()),
                format!("files:{}", &digest[..32]),
            ));
        }
    }
    if let (Some(before), Some(now)) = (prs_before, view.prs.as_ref()) {
        for (number, pr) in now {
            if before.get(number) == Some(&pr.state) {
                continue;
            }
            let describe = |change: &str| {
                let mut payload = base(change);
                payload["branch"] = json!(pr.head);
                payload["pull_request"] = json!({
                    "number": number.parse::<u64>().unwrap_or(0),
                    "state": pr.state,
                    "url": pr.url,
                    "title": pr.title,
                    "head": pr.head,
                    "base": pr.base,
                });
                payload
            };
            out.push((
                heard(describe("pull_request"), bisa_core::Chain::default()),
                format!("pr:{number}:{}", pr.state),
            ));
            if pr.state == "merged" {
                out.push((
                    heard(describe("merge"), bisa_core::Chain::default()),
                    format!("merge:{number}"),
                ));
            }
        }
    }
    out
}

/// One project listener against its own memory: every change its filter
/// hears is an occurrence; a first look learns and fires nothing.
async fn project_listener(
    inner: &Arc<Inner>,
    listener: &Armed,
    filter: &ProjectFilter,
    project: &bisa_core::Project,
    view: &ProjectView,
) {
    let mut rt = runtime_of(inner, listener);
    let wants = Needs::of(filter.change);
    let refs_before = rt.heads.clone();
    let files_before = rt.files.clone();
    let prs_before = rt.pr_states.clone();
    let found = changes(
        inner,
        project,
        view,
        refs_before
            .as_ref()
            .filter(|_| wants.heads || wants.remotes),
        files_before.as_ref().filter(|_| wants.files),
        prs_before.as_ref().filter(|_| wants.prs),
        &listener.scope(),
    )
    .await;
    for (heard, dedupe) in found {
        if filter.hears(&heard) {
            enqueue_for(inner, listener, &heard, Some(&dedupe), None);
        }
    }
    // What was seen becomes the memory — written after the occurrences, so a
    // crash between the two sees the same changes again and their dedupe keys
    // make them the same signals.
    if wants.heads || wants.remotes {
        rt.heads = Some(view.refs.clone());
    }
    if wants.files {
        rt.files = view.files.clone().or(rt.files);
    }
    if wants.prs {
        if let Some(prs) = &view.prs {
            rt.pr_states = Some(
                prs.iter()
                    .map(|(n, pr)| (n.clone(), pr.state.clone()))
                    .collect(),
            );
        }
    }
    put_runtime(inner, &listener.key, &rt);
}

/// The waits holding for a project, against the ticker's own memory of it:
/// what moved since the last look is heard by every wait it concerns.
async fn project_waits(inner: &Arc<Inner>, project: &bisa_core::Project, view: &ProjectView) {
    let before = inner
        .listen
        .project_views
        .get(&project.id)
        .map(|v| v.clone());
    inner.listen.project_views.insert(project.id, view.clone());
    let Some(before) = before else {
        return; // a first look learns
    };
    let prs_before = before.prs.as_ref().map(|prs| {
        prs.iter()
            .map(|(n, pr)| (n.clone(), pr.state.clone()))
            .collect::<BTreeMap<_, _>>()
    });
    let found = changes(
        inner,
        project,
        view,
        Some(&before.refs),
        before.files.as_ref(),
        prs_before.as_ref(),
        &SignalScope::Workspace,
    )
    .await;
    for (heard, _) in found {
        crate::waits::on_heard(inner, &heard);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bisa_core::FireOn;

    fn refs(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn a_ref_that_moved_or_appeared_is_a_change_and_one_that_stayed_is_not() {
        let before = refs(&[
            ("refs/heads/main", "a1"),
            ("refs/heads/stays", "b1"),
            ("refs/remotes/origin/main", "a1"),
        ]);
        let now = refs(&[
            ("refs/heads/main", "a2"),
            ("refs/heads/stays", "b1"),
            ("refs/heads/new", "c1"),
            ("refs/remotes/origin/main", "a1"),
        ]);
        let heads = moved(&before, &now, "refs/heads/");
        assert_eq!(
            heads,
            vec![
                ("refs/heads/main".into(), Some("a1".into()), "a2".into()),
                ("refs/heads/new".into(), None, "c1".into()),
            ]
        );
        assert!(moved(&before, &now, "refs/remotes/").is_empty());
    }

    #[test]
    fn a_file_that_appeared_changed_or_went_is_a_change() {
        let before = refs(&[("a.csv", "1"), ("b.csv", "1"), ("gone.csv", "1")]);
        let now = refs(&[("a.csv", "1"), ("b.csv", "2"), ("new.csv", "1")]);
        assert_eq!(
            changed_files(&before, &now),
            vec!["b.csv".to_string(), "gone.csv".into(), "new.csv".into()]
        );
        assert!(changed_files(&now, &now).is_empty());
    }

    #[test]
    fn a_scan_is_bounded_and_never_follows_a_link() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.csv"), "1").unwrap();
        let mut deep = dir.path().to_path_buf();
        for i in 0..6 {
            deep = deep.join(format!("l{i}"));
        }
        std::fs::create_dir_all(&deep).unwrap();
        std::fs::write(deep.join("buried.txt"), "x").unwrap();
        let bounds = bisa_core::ScanBounds {
            depth: 3,
            entries: 5_000,
        };
        let seen = scan_tree(dir.path(), bounds);
        assert!(seen.contains_key("a.csv"));
        assert!(
            !seen.keys().any(|k| k.ends_with("buried.txt")),
            "nothing past the depth: {seen:?}"
        );
        let few = scan_tree(
            dir.path(),
            bisa_core::ScanBounds {
                depth: 8,
                entries: 1,
            },
        );
        assert!(few.len() <= 1);
    }

    #[test]
    fn the_needs_of_each_change() {
        assert!(Needs::of(ProjectChange::Commit).heads);
        assert!(Needs::of(ProjectChange::Push).remotes);
        assert!(Needs::of(ProjectChange::Files).files);
        assert!(Needs::of(ProjectChange::Merge).prs);
        let both = Needs::of(ProjectChange::Commit).with(Needs::of(ProjectChange::Files));
        assert!(both.heads && both.files && !both.prs);
    }

    #[test]
    fn a_check_fires_as_its_rule_says() {
        assert!(FireOn::StartsFailing.fires(false, None));
        assert!(!FireOn::StartsFailing.fires(false, Some(false)));
        assert!(FireOn::StartsFailing.fires(false, Some(true)));
        assert!(!FireOn::Passing.fires(false, None));
    }
}
