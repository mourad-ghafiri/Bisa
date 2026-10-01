//! Who commits in a repository the platform makes — and the one ask when
//! nobody does.
//!
//! The repository's **local** git config is the one truth for who authors a
//! commit; the person's **global** git config is what a new repository
//! inherits by default. What lives here is everything around those two layers:
//!
//! - the **policy** — [`Committer::resolve`], one pure table: git config named
//!   on the creation request is written locally; else a repository with its
//!   own identity is kept; else one that inherits a global identity is left
//!   alone; else the person is **asked** and nothing is written;
//! - the **ask** — [`CommitterDesk`], the questions nobody has answered yet,
//!   and the [`EnginePayload::CommitterNeeded`] frame raised once per project
//!   so a screen can put the question to a person, whoever created the project;
//! - the **answer** — [`committer_set`], which every local identity write
//!   funnels through: the want is cleared, [`EnginePayload::CommitterSet`] is
//!   raised, and a settlement the missing identity had refused is committed;
//! - the **global layer** — [`global_config`] / [`set_global_config`], the one
//!   path that writes the person's global git config, reached only from
//!   Settings → Git (`PUT /git/config`) at their request. No creation, commit,
//!   policy or agent reaches it (I45; `tests/it/projects.rs` scans for it).
//!
//! [`projects::create`](crate::projects::create) applies the policy once, for
//! every creation path alike; [`projects::ensure_identity`](crate::projects)
//! asks on every refused commit.

use bisa_core::{CommitterPolicy, Project, ProjectId, Vcs, WorkstreamId};
use bisa_vcs::{ConfigScope, GitConfigView, GitIdentity, Ident, IdentitySource};
use dashmap::DashMap;
use serde::Serialize;

use crate::events::{EngineEvent, EnginePayload, GitSetup};
use crate::projects::{blocking, checkout_tree};
use crate::{EngineError, Inner};

/// Why a person is being asked who commits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CommitterReason {
    /// A project was just created and no identity resolves in it.
    Created,
    /// A person's commit — Changes view, CLI, API — was refused.
    CommitRefused,
    /// An agent's settlement commit was refused; the work is left uncommitted
    /// and is committed once who commits is set.
    SettlementRefused,
    /// Found at a start with no identity resolving in it — the desk is read
    /// from git on every boot (`rearm`), so a restart never forgets a
    /// repository nobody can commit in. No creation story, no refusal: just
    /// the fact.
    Unresolved,
}

impl CommitterReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::CommitRefused => "commit_refused",
            Self::SettlementRefused => "settlement_refused",
            Self::Unresolved => "unresolved",
        }
    }
}

/// What the policy decided for one new repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Committer {
    /// The repository already has an identity of its own; nothing to do.
    Keep,
    /// No local identity, but the global one resolves: nothing written, nothing asked.
    Inherit { global: Ident },
    /// The global pair, written into the repository's own config at creation
    /// (`git.committer` = `pin`): it keeps its author whatever the global one
    /// becomes.
    Pin { global: Ident },
    /// The request named git config; these keys were written locally.
    Applied { keys: Vec<String> },
    /// Nothing resolves: ask the person.
    Ask,
}

impl Committer {
    /// The one precedence table. `explicit` is the config the creation request
    /// named; `current` is what git resolves in the repository now; `policy`
    /// is the workspace's `git.committer`. Config on the request always wins;
    /// a repository with an identity of its own is always kept; the policy
    /// decides what a global identity means — inherited, pinned, or asked
    /// about all the same — and nothing resolving is always asked for.
    pub fn resolve(
        explicit: &[(String, String)],
        current: &GitIdentity,
        policy: CommitterPolicy,
    ) -> Committer {
        if !explicit.is_empty() {
            return Committer::Applied {
                keys: explicit.iter().map(|(k, _)| k.clone()).collect(),
            };
        }
        match (current.source, &current.global, policy) {
            (IdentitySource::Local, _, _) => Committer::Keep,
            (IdentitySource::None, _, _) => Committer::Ask,
            (IdentitySource::Global, _, CommitterPolicy::Ask) => Committer::Ask,
            (IdentitySource::Global, Some(global), CommitterPolicy::Inherit) => {
                Committer::Inherit {
                    global: global.clone(),
                }
            }
            (IdentitySource::Global, Some(global), CommitterPolicy::Pin) => Committer::Pin {
                global: global.clone(),
            },
            // Resolves outside the global layer (the system file, an include
            // the global pair does not show): nothing to pin, nothing to ask.
            (IdentitySource::Global, None, _) => Committer::Keep,
        }
    }

    /// The clause a creation note ends with, when there is something to say.
    pub fn sentence(&self) -> Option<String> {
        match self {
            Self::Keep => None,
            Self::Inherit { global } => Some(format!("inherits the global git config ({global})")),
            Self::Pin { global } => Some(format!(
                "pinned the global git identity into the repository ({global})"
            )),
            Self::Applied { keys } => Some(format!(
                "git config set on the request: {}",
                keys.join(", ")
            )),
            Self::Ask => Some("nobody is set to commit yet; the person is asked".to_string()),
        }
    }
}

/// A settlement commit that was refused for want of an identity, kept so it
/// can be made once one is set. The message is stored because the work item's
/// spec that produced it is gone by then.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitRetry {
    pub workstream: WorkstreamId,
    pub message: String,
}

/// One unanswered question about one project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitterWant {
    pub reason: CommitterReason,
    pub retry: Option<CommitRetry>,
}

/// The questions nobody has answered yet, one per project. In memory, and
/// true after a restart all the same: git is the truth of who commits, and
/// [`rearm`] reads every live repository at boot and puts the ones with no
/// identity back on the desk.
#[derive(Debug, Default)]
pub struct CommitterDesk {
    wants: DashMap<ProjectId, CommitterWant>,
}

impl CommitterDesk {
    /// Register a want. Answers `true` when the project was not already on
    /// the desk — the caller raises the event exactly then. A repeat keeps
    /// the newest reason and never loses a retry it already holds.
    pub fn want(&self, project: ProjectId, want: CommitterWant) -> bool {
        match self.wants.entry(project) {
            dashmap::mapref::entry::Entry::Vacant(v) => {
                v.insert(want);
                true
            }
            dashmap::mapref::entry::Entry::Occupied(mut o) => {
                let existing = o.get_mut();
                existing.reason = want.reason;
                if want.retry.is_some() {
                    existing.retry = want.retry;
                }
                false
            }
        }
    }

    /// Take the want off the desk — the question was answered.
    pub fn take(&self, project: ProjectId) -> Option<CommitterWant> {
        self.wants.remove(&project).map(|(_, w)| w)
    }

    /// Everything still unanswered.
    pub fn pending(&self) -> Vec<(ProjectId, CommitterReason)> {
        let mut rows: Vec<_> = self
            .wants
            .iter()
            .map(|r| (*r.key(), r.value().reason))
            .collect();
        rows.sort_by_key(|(p, _)| p.to_string());
        rows
    }
}

/// One unanswered question, as a screen lists it — the same facts the
/// `committer_needed` frame carries, so a dialog seeded from the overview
/// and one that heard the frame show the same thing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PendingCommitter {
    pub project: ProjectId,
    pub slug: String,
    pub workstream: WorkstreamId,
    pub reason: CommitterReason,
    pub origin: bisa_core::ProjectOrigin,
    pub global: Option<Ident>,
}

/// What the ask dialog seeds from: the global pair and the open questions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CommitterOverview {
    pub global: Option<Ident>,
    pub pending: Vec<PendingCommitter>,
}

/// The workspace's `git.committer`, read for one project — project, then
/// workspace, then the registry's default — at the moment of its creation.
pub fn policy_for(inner: &Inner, project: &Project) -> CommitterPolicy {
    inner
        .ws
        .setting(bisa_core::git_settings::keys::COMMITTER, Some(project.id))
        .map(|r| CommitterPolicy::from_resolved(&r))
        .unwrap_or_default()
}

/// Apply the policy to a project whose tree now exists — the one call
/// [`crate::projects::create`] makes for every creation path. A non-git
/// project has nobody to commit and is `Keep`. Config named on the request is
/// written locally, key by key, each validated by the schema; a refused entry
/// is the caller's error (the creation rolls back). A `Pin` writes the global
/// pair into the repository's own config — a local write, never a global one.
/// An `Ask` puts the project on the desk and raises the frame. The answer is
/// returned so the caller's creation note can say what happened.
pub async fn apply_on_creation(
    inner: &Inner,
    project: &Project,
    git_config: Vec<(String, String)>,
) -> Result<Committer, EngineError> {
    if !matches!(project.vcs, Vcs::Git { .. }) {
        return Ok(Committer::Keep);
    }
    let policy = policy_for(inner, project);
    let root = inner.ws.project_root_path(project);
    if !git_config.is_empty() {
        blocking({
            let (git, root, entries) = (inner.git(), root.clone(), git_config.clone());
            move || {
                for (key, value) in &entries {
                    git.config_set(ConfigScope::Local, Some(&root), key, value)?;
                }
                Ok(())
            }
        })
        .await?;
    }
    let current = blocking({
        let (git, root) = (inner.git(), root.clone());
        move || git.identity(&root)
    })
    .await?;
    let decision = Committer::resolve(&git_config, &current, policy);
    match &decision {
        Committer::Ask => ask(inner, project, CommitterReason::Created, None).await,
        Committer::Pin { global } => {
            blocking({
                let (git, root, global) = (inner.git(), root.clone(), global.clone());
                move || git.set_local_identity(&root, &global.name, &global.email)
            })
            .await?;
        }
        _ => {}
    }
    Ok(decision)
}

/// Put a project on the desk and, the first time, tell the screens. Best
/// effort by design: the caller is already refusing or creating, and a frame
/// that cannot be built must not change that outcome. The global pair is
/// read on the blocking pool like every other git call here — this runs on
/// the project-creation path, never on a runtime worker.
pub async fn ask(
    inner: &Inner,
    project: &Project,
    reason: CommitterReason,
    retry: Option<CommitRetry>,
) {
    let newly = inner
        .committers
        .want(project.id, CommitterWant { reason, retry });
    if !newly {
        return;
    }
    let global = blocking({
        let git = inner.git();
        move || git.global_identity()
    })
    .await
    .unwrap_or_else(|e| {
        tracing::debug!("global git identity could not be read: {e}");
        None
    });
    let payload = EnginePayload::CommitterNeeded {
        project: project.id,
        slug: project.slug.to_string(),
        workstream: WorkstreamId::primary_of(project.id),
        reason,
        origin: project.origin.clone(),
        global,
    };
    inner.emit(match single_goal(inner, project) {
        Some(goal) => EngineEvent::scoped(goal, None, payload),
        None => EngineEvent::global(payload),
    });
}

/// The question was answered — the repository now resolves an identity of
/// its own, through whichever door wrote it. The desk forgets the project,
/// the screens learn the identity, and a settlement the refusal had left
/// uncommitted is committed now with the message it was meant to carry.
pub async fn committer_set(
    inner: &Inner,
    project: &Project,
    workstream: WorkstreamId,
    identity: Ident,
) {
    let want = inner.committers.take(project.id);
    inner.emit(EngineEvent::global(EnginePayload::CommitterSet {
        project: project.id,
        workstream,
        identity,
    }));
    if let Some(CommitRetry {
        workstream,
        message,
    }) = want.and_then(|w| w.retry)
    {
        crate::projects::settle_commit(inner, workstream, &message).await;
    }
}

/// What the ask dialog seeds from.
pub async fn overview(inner: &Inner) -> Result<CommitterOverview, EngineError> {
    let global = blocking({
        let git = inner.git();
        move || git.global_identity()
    })
    .await?;
    let mut pending = Vec::new();
    for (project, reason) in inner.committers.pending() {
        // A project deleted while its question was open is simply gone.
        let Ok(p) = inner.ws.get_project(project) else {
            inner.committers.take(project);
            continue;
        };
        pending.push(PendingCommitter {
            project,
            slug: p.slug.to_string(),
            workstream: WorkstreamId::primary_of(project),
            reason,
            origin: p.origin.clone(),
            global: global.clone(),
        });
    }
    Ok(CommitterOverview { global, pending })
}

/// Put every live git repository nobody can commit in back on the desk —
/// run once after the boot walks, off the boot path. Git is the truth of who
/// commits, so the desk needs no file of its own: a restart reads the
/// repositories and asks again, `Unresolved`, for each that resolves nothing.
/// A project whose folder is gone or that is not git has nobody to ask for;
/// a read that fails is one log line, never a stopped walk.
pub async fn rearm(inner: std::sync::Arc<Inner>) {
    let projects = match inner.ws.list_projects() {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!("the committer desk could not list the projects: {e}");
            return;
        }
    };
    let mut asked = 0usize;
    for project in projects {
        if !matches!(project.vcs, Vcs::Git { .. }) || project.archived.is_some() {
            continue;
        }
        let root = inner.ws.project_root_path(&project);
        if !root.is_dir() {
            continue;
        }
        let identity = blocking({
            let (git, root) = (inner.git(), root.clone());
            move || git.identity(&root)
        })
        .await;
        match identity {
            Ok(id) if id.source == IdentitySource::None => {
                ask(&inner, &project, CommitterReason::Unresolved, None).await;
                asked += 1;
            }
            Ok(_) => {}
            Err(e) => {
                tracing::warn!(project = %project.slug, "who commits could not be read at start: {e}");
            }
        }
    }
    if asked > 0 {
        tracing::info!(
            asked,
            "the committer desk was rearmed from the repositories"
        );
    }
}

/// The person's global git config, schema keys only (`GET /git/config`).
pub async fn global_config(inner: &Inner) -> Result<GitConfigView, EngineError> {
    let git = inner.git();
    blocking(move || git.config_view(None)).await
}

/// Write the person's global git config — the schema keys' global write,
/// reached only from Settings → Git & code hosts at their request (I45; the
/// platform's profile includes and the default account are the other two
/// named writers). Each key is validated by the schema; the first refusal
/// stops the batch and is the caller's 400. The profile includes are
/// re-appended afterwards, so a value written here never outranks a profile.
/// Not journaled: a person's own configuration. Announced as the setup
/// moving (`GitSetupChanged { Identity }`) — every repository that inherits
/// the global pair resolves differently now — and the desk is settled: a
/// project that was asking for a committer and now inherits one is
/// answered. Answers the fresh view.
pub async fn set_global_config(
    inner: &Inner,
    set: Vec<(String, String)>,
    unset: Vec<String>,
) -> Result<GitConfigView, EngineError> {
    let git = inner.git();
    let profiles = inner.ws.paths().git_profiles_dir();
    let view = blocking(move || {
        for (key, value) in &set {
            git.config_set(ConfigScope::Global, None, key, value)?;
        }
        for key in &unset {
            git.config_unset(ConfigScope::Global, None, key)?;
        }
        crate::gitprofiles::reappend_includes(&git, &profiles)?;
        git.config_view(None)
    })
    .await?;
    inner.emit(EngineEvent::global(EnginePayload::GitSetupChanged {
        what: GitSetup::Identity,
    }));
    settle_resolved(inner).await;
    Ok(view)
}

/// The desk read against the repositories: every project still asking whose
/// primary checkout now resolves an identity — through the global layer, a
/// profile's include, or a local pair written by hand — is answered with
/// `committer_set`. The one call every write that can change what a
/// repository inherits makes afterwards: the global config and the
/// profiles. A project whose tree is gone is left for the desk's own sweep.
pub async fn settle_resolved(inner: &Inner) {
    for (project, _) in inner.committers.pending() {
        let workstream = WorkstreamId::primary_of(project);
        let Ok((project, path)) = checkout_tree(inner, workstream) else {
            continue;
        };
        let identity = blocking({
            let git = inner.git();
            move || git.identity(&path)
        })
        .await;
        if let Ok(GitIdentity {
            source,
            name: Some(name),
            email: Some(email),
            ..
        }) = identity
        {
            if source != IdentitySource::None {
                committer_set(inner, &project, workstream, Ident { name, email }).await;
            }
        }
    }
}

/// The goal an event about this project is scoped to: the one goal it is
/// attached to, when there is exactly one. Otherwise the event is global.
fn single_goal(inner: &Inner, project: &Project) -> Option<bisa_core::GoalId> {
    match inner.ws.goals_of_project(project.id) {
        Ok(goals) if goals.len() == 1 => goals.first().copied(),
        _ => None,
    }
}

/// The identity a connected code host account suggests for a repository
/// nobody commits in: the person's name as the host has it (their login
/// otherwise) and the email the host attributes commits to — the public one
/// when the host shows it, else the host's no-reply address, which GitHub
/// (`{id}+{login}@users.noreply.github.com`) and GitLab
/// (`{id}-{login}@users.noreply.gitlab.com`) build from the numeric id.
/// Bitbucket's user record carries no email, so it suggests nothing. Never
/// written by itself: the person's click is what writes it.
pub fn committer_suggestion(
    kind: crate::codehost::CodeHostKind,
    account: &bisa_codehost::Account,
) -> Option<Ident> {
    let login = account.login.trim();
    if login.is_empty() {
        return None;
    }
    let name = account
        .name
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .unwrap_or(login)
        .to_string();
    let email = match (account.email.as_deref().map(str::trim), account.id, kind) {
        (Some(e), _, _) if !e.is_empty() => e.to_string(),
        (_, Some(id), crate::codehost::CodeHostKind::GitHub) => {
            format!("{id}+{login}@users.noreply.github.com")
        }
        (_, Some(id), crate::codehost::CodeHostKind::GitLab) => {
            format!("{id}-{login}@users.noreply.gitlab.com")
        }
        _ => return None,
    };
    Some(Ident { name, email })
}

#[cfg(test)]
mod suggestion {
    use super::*;
    use crate::codehost::CodeHostKind;
    use bisa_codehost::Account;

    fn account(login: &str, name: Option<&str>, email: Option<&str>, id: Option<u64>) -> Account {
        Account {
            login: login.into(),
            scopes: vec![],
            organizations: vec![],
            name: name.map(str::to_string),
            email: email.map(str::to_string),
            id,
        }
    }

    #[test]
    fn a_public_email_is_used_as_is_and_the_name_falls_back_to_the_login() {
        let s = committer_suggestion(
            CodeHostKind::GitHub,
            &account("ada", None, Some("ada@example.org"), Some(7)),
        );
        assert_eq!(
            s,
            Some(Ident {
                name: "ada".into(),
                email: "ada@example.org".into()
            })
        );
        let s = committer_suggestion(
            CodeHostKind::GitHub,
            &account("ada", Some("Ada Lovelace"), Some("ada@example.org"), None),
        );
        assert_eq!(s.unwrap().name, "Ada Lovelace");
    }

    #[test]
    fn without_a_public_email_the_hosts_no_reply_address_is_built_from_the_id() {
        assert_eq!(
            committer_suggestion(
                CodeHostKind::GitHub,
                &account("ada", Some("Ada"), None, Some(7))
            )
            .unwrap()
            .email,
            "7+ada@users.noreply.github.com"
        );
        assert_eq!(
            committer_suggestion(
                CodeHostKind::GitLab,
                &account("ada", None, Some("  "), Some(9))
            )
            .unwrap()
            .email,
            "9-ada@users.noreply.gitlab.com"
        );
    }

    #[test]
    fn nothing_is_suggested_without_an_email_or_an_id_and_bitbucket_never_builds_one() {
        assert_eq!(
            committer_suggestion(
                CodeHostKind::GitHub,
                &account("ada", Some("Ada"), None, None)
            ),
            None
        );
        assert_eq!(
            committer_suggestion(
                CodeHostKind::Bitbucket,
                &account("ada", Some("Ada"), None, Some(3))
            ),
            None
        );
        assert_eq!(
            committer_suggestion(
                CodeHostKind::GitHub,
                &account("  ", Some("Ada"), Some("a@b.c"), Some(1))
            ),
            None
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grace() -> Ident {
        Ident {
            name: "Grace Hopper".into(),
            email: "grace@example.invalid".into(),
        }
    }

    fn identity(source: IdentitySource, global: Option<Ident>) -> GitIdentity {
        GitIdentity {
            name: None,
            email: None,
            source,
            global,
        }
    }

    fn pair(k: &str, v: &str) -> (String, String) {
        (k.to_string(), v.to_string())
    }

    #[test]
    fn config_on_the_request_is_applied_whatever_the_policy_or_the_layers_say() {
        let explicit = vec![pair("user.name", "Ada"), pair("user.useConfigOnly", "true")];
        for policy in [
            CommitterPolicy::Inherit,
            CommitterPolicy::Pin,
            CommitterPolicy::Ask,
        ] {
            for source in [
                IdentitySource::Local,
                IdentitySource::Global,
                IdentitySource::None,
            ] {
                assert_eq!(
                    Committer::resolve(&explicit, &identity(source, Some(grace())), policy),
                    Committer::Applied {
                        keys: vec!["user.name".into(), "user.useConfigOnly".into()]
                    }
                );
            }
        }
    }

    #[test]
    fn the_policy_decides_what_a_global_identity_means_and_nothing_else() {
        let none: Vec<(String, String)> = vec![];
        let global = identity(IdentitySource::Global, Some(grace()));
        assert_eq!(
            Committer::resolve(&none, &global, CommitterPolicy::Inherit),
            Committer::Inherit { global: grace() }
        );
        assert_eq!(
            Committer::resolve(&none, &global, CommitterPolicy::Pin),
            Committer::Pin { global: grace() }
        );
        assert_eq!(
            Committer::resolve(&none, &global, CommitterPolicy::Ask),
            Committer::Ask,
            "ask means ask, whatever resolves"
        );
        // A repository's own identity is kept under every policy but ask.
        let local = identity(IdentitySource::Local, Some(grace()));
        for policy in [
            CommitterPolicy::Inherit,
            CommitterPolicy::Pin,
            CommitterPolicy::Ask,
        ] {
            assert_eq!(
                Committer::resolve(&none, &local, policy),
                Committer::Keep,
                "{policy:?}"
            );
        }
        // Nothing resolving is asked for under every policy.
        let missing = identity(IdentitySource::None, None);
        for policy in [
            CommitterPolicy::Inherit,
            CommitterPolicy::Pin,
            CommitterPolicy::Ask,
        ] {
            assert_eq!(
                Committer::resolve(&none, &missing, policy),
                Committer::Ask,
                "{policy:?}"
            );
        }
        // Resolving outside the global layer: nothing to pin.
        let elsewhere = identity(IdentitySource::Global, None);
        assert_eq!(
            Committer::resolve(&none, &elsewhere, CommitterPolicy::Pin),
            Committer::Keep
        );
        assert_eq!(
            Committer::resolve(&none, &elsewhere, CommitterPolicy::Inherit),
            Committer::Keep
        );
        assert_eq!(
            Committer::resolve(&none, &elsewhere, CommitterPolicy::Ask),
            Committer::Ask
        );
    }

    #[test]
    fn a_local_identity_is_kept_a_global_one_is_inherited_and_none_is_asked_for() {
        let none: Vec<(String, String)> = vec![];
        assert_eq!(
            Committer::resolve(
                &none,
                &identity(IdentitySource::Local, Some(grace())),
                CommitterPolicy::default()
            ),
            Committer::Keep
        );
        assert_eq!(
            Committer::resolve(
                &none,
                &identity(IdentitySource::Global, Some(grace())),
                CommitterPolicy::default()
            ),
            Committer::Inherit { global: grace() },
            "nothing written, nothing asked — the default policy"
        );
        assert_eq!(
            Committer::resolve(
                &none,
                &identity(IdentitySource::None, None),
                CommitterPolicy::default()
            ),
            Committer::Ask
        );
    }

    #[test]
    fn a_creation_note_says_what_the_policy_did() {
        assert_eq!(Committer::Keep.sentence(), None);
        assert_eq!(
            Committer::Inherit { global: grace() }.sentence().as_deref(),
            Some("inherits the global git config (Grace Hopper <grace@example.invalid>)")
        );
        assert_eq!(
            Committer::Pin { global: grace() }.sentence().as_deref(),
            Some("pinned the global git identity into the repository (Grace Hopper <grace@example.invalid>)")
        );
        assert_eq!(
            Committer::Applied {
                keys: vec!["user.name".into(), "user.email".into()]
            }
            .sentence()
            .as_deref(),
            Some("git config set on the request: user.name, user.email")
        );
        assert!(Committer::Ask
            .sentence()
            .unwrap()
            .contains("the person is asked"));
        assert_eq!(
            CommitterReason::SettlementRefused.as_str(),
            "settlement_refused"
        );
    }

    #[test]
    fn the_desk_reports_a_want_once_and_keeps_the_retry() {
        let desk = CommitterDesk::default();
        let project = ProjectId::from_ulid(ulid::Ulid::from_datetime(std::time::SystemTime::now()));
        let retry = CommitRetry {
            workstream: WorkstreamId::primary_of(project),
            message: "settle".into(),
        };
        assert!(desk.want(
            project,
            CommitterWant {
                reason: CommitterReason::Created,
                retry: None
            }
        ));
        assert!(
            !desk.want(
                project,
                CommitterWant {
                    reason: CommitterReason::SettlementRefused,
                    retry: Some(retry.clone())
                }
            ),
            "a second want on the same project raises nothing new"
        );
        assert!(
            !desk.want(
                project,
                CommitterWant {
                    reason: CommitterReason::CommitRefused,
                    retry: None
                }
            ),
            "a later want without a retry keeps the one already held"
        );
        assert_eq!(
            desk.pending(),
            vec![(project, CommitterReason::CommitRefused)]
        );
        let taken = desk.take(project).expect("on the desk");
        assert_eq!(taken.reason, CommitterReason::CommitRefused);
        assert_eq!(taken.retry, Some(retry));
        assert!(desk.take(project).is_none(), "taken once");
        assert!(desk.pending().is_empty());
    }
}
