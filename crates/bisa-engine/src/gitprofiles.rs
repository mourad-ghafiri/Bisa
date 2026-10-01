//! Git profiles by organization (ide/04 §Profiles by organization): who a
//! person is for one owner on one code host — the author, the SSH key, the
//! code host account — kept as one git config file the platform owns under
//! the workspace's `identity/git/profiles/` and included by the person's
//! global config with `includeIf "hasconfig:remote.*.url:<glob>"` entries.
//!
//! No second store: the includes and the files **are** the profiles, read
//! back through `git config` the way git itself reads them, so the platform
//! and git cannot disagree about which profile a checkout falls under. The
//! includes are written **after** every other global key (git reads a file
//! top to bottom and the last value wins), and re-appended after every other
//! global write — [`reappend_includes`] — so a profile keeps the last word.
//!
//! This module is one of the three sanctioned writers of the global layer
//! (with `identity::set_global_config` and `codehost::set_default_account`);
//! the engine's guard test pins the global scope to those functions.

use crate::events::{EnginePayload, GitSetup};
use crate::projects::blocking;
use crate::{EngineError, Inner};
use bisa_core::{GitProfile, ProfileSpec, Slug};
use bisa_vcs::{ConfigScope, Git, GitVersion, Include, ProfileFile, VcsResult};
use std::path::{Path, PathBuf};

/// A profile as the routes and the panel see it: the profile, its file, and
/// the globs its includes carry.
#[derive(
    Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
pub struct GitProfileView {
    #[serde(flatten)]
    pub profile: GitProfile,
    pub file: PathBuf,
    pub globs: Vec<String>,
}

/// Everything the Identity panel shows about profiles: the profiles, the
/// includes of the global file that are **not** the platform's (a person's own,
/// left alone and shown as such), and whether this git can evaluate them.
#[derive(
    Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
pub struct GitProfilesView {
    pub profiles: Vec<GitProfileView>,
    pub foreign_includes: Vec<ForeignInclude>,
    pub git_version: String,
    /// `includeIf "hasconfig:…"` needs git 2.36; below it a profile is refused.
    pub hasconfig_supported: bool,
}

#[derive(
    Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
pub struct ForeignInclude {
    pub condition: String,
    pub path: PathBuf,
}

const CONDITION_PREFIX: &str = "hasconfig:remote.*.url:";

fn condition_of(glob: &str) -> String {
    format!("{CONDITION_PREFIX}{glob}")
}

fn file_of(dir: &Path, slug: &Slug) -> PathBuf {
    dir.join(format!("{slug}.gitconfig"))
}

/// The globs' hosts and owner, read back from a profile's includes: the host
/// is the one the `https://` glob names, every other SSH host is an alias.
fn spec_of_globs(globs: &[String]) -> Option<(String, String, Vec<String>)> {
    let mut host = None;
    let mut owner = None;
    let mut aliases = Vec::new();
    for glob in globs {
        let body = glob.strip_suffix("/**")?;
        if let Some(rest) = body.strip_prefix("https://") {
            let (h, o) = rest.split_once('/')?;
            host = Some(h.to_string());
            owner = Some(o.to_string());
        } else if let Some(rest) = body.strip_prefix("ssh://git@") {
            let (h, _) = rest.split_once('/')?;
            aliases.push(h.to_string());
        } else if let Some(rest) = body.strip_prefix("git@") {
            let (h, _) = rest.split_once(':')?;
            aliases.push(h.to_string());
        }
    }
    let host = host?;
    aliases.retain(|a| a != &host);
    aliases.sort();
    aliases.dedup();
    Some((host, owner?, aliases))
}

/// The profiles the includes name under `dir`, read with `git` — pure over
/// the two reads, so `list` and `reappend_includes` share it.
fn read_profiles(git: &Git, dir: &Path) -> VcsResult<(Vec<GitProfileView>, Vec<ForeignInclude>)> {
    let includes = git.includes()?;
    let mut by_file: Vec<(PathBuf, Vec<String>)> = Vec::new();
    let mut foreign = Vec::new();
    for Include { condition, path } in includes {
        let ours = path.starts_with(dir) && path.extension().is_some_and(|e| e == "gitconfig");
        match (ours, condition.strip_prefix(CONDITION_PREFIX)) {
            (true, Some(glob)) => match by_file.iter_mut().find(|(p, _)| *p == path) {
                Some((_, globs)) => globs.push(glob.to_string()),
                None => by_file.push((path, vec![glob.to_string()])),
            },
            _ => foreign.push(ForeignInclude { condition, path }),
        }
    }
    let mut profiles = Vec::new();
    for (file, globs) in by_file {
        let Some(slug) = file
            .file_stem()
            .and_then(|s| s.to_str())
            .and_then(|s| Slug::new(s).ok())
        else {
            continue;
        };
        let Some(pf) = git.config_file_read(&file)? else {
            continue;
        };
        let Some((host, owner, aliases)) = spec_of_globs(&globs) else {
            continue;
        };
        let spec = ProfileSpec {
            label: pf.label.clone().unwrap_or_else(|| slug.to_string()),
            host,
            owner,
            aliases,
            name: pf.name.clone(),
            email: pf.email.clone(),
            ssh_key: pf.ssh_key.as_ref().map(|k| k.display().to_string()),
            account: pf.account.clone(),
        };
        if let Ok(profile) = GitProfile::from_spec(slug, spec) {
            profiles.push(GitProfileView {
                globs: profile.url_globs(),
                profile,
                file,
            });
        }
    }
    profiles.sort_by(|a, b| a.profile.slug.cmp(&b.profile.slug));
    Ok((profiles, foreign))
}

/// Every profile, the foreign includes, and whether this git can evaluate
/// them (`GET /git/profiles`).
pub async fn list(inner: &Inner) -> Result<GitProfilesView, EngineError> {
    let git = inner.git();
    let dir = inner.ws.paths().git_profiles_dir();
    blocking(move || {
        let version = git.version()?;
        let (profiles, foreign_includes) = read_profiles(&git, &dir)?;
        Ok(GitProfilesView {
            profiles,
            foreign_includes,
            git_version: version.to_string(),
            hasconfig_supported: version.supports_hasconfig(),
        })
    })
    .await
}

/// Save a profile under `slug` (`PUT /git/profiles/{slug}`): the spec
/// validated by the core, the file written, every glob's include ensured at
/// the end of the global file and the stale ones for that file removed. A git
/// older than 2.36 refuses before anything is written, since it would ignore
/// the include and silently commit as somebody else.
pub async fn put(
    inner: &Inner,
    slug: Slug,
    spec: ProfileSpec,
) -> Result<GitProfileView, EngineError> {
    let profile = GitProfile::from_spec(slug, spec).map_err(bisa_core::CoreError::from)?;
    let git = inner.git();
    let dir = inner.ws.paths().git_profiles_dir();
    let view = blocking(move || {
        let version = git.version()?;
        if !version.supports_hasconfig() {
            return Err(bisa_vcs::VcsError::NotAvailable(format!(
                "git {version} cannot evaluate `includeIf \"hasconfig:…\"`; profiles need git {} or newer",
                GitVersion::HASCONFIG
            )));
        }
        let file = file_of(&dir, &profile.slug);
        git.config_file_write(
            &file,
            &ProfileFile {
                label: Some(profile.label.clone()),
                name: profile.name.clone(),
                email: profile.email.clone(),
                ssh_key: profile.ssh_key.as_ref().map(PathBuf::from),
                credential_username: profile.account.clone(),
                account: profile.account.clone(),
            },
        )?;
        git.include_remove(ConfigScope::Global, &file)?;
        let globs = profile.url_globs();
        for glob in &globs {
            git.include_set(ConfigScope::Global, &condition_of(glob), &file)?;
        }
        Ok(GitProfileView { profile, file, globs })
    })
    .await?;
    inner.emit(crate::events::EngineEvent::global(
        EnginePayload::GitSetupChanged {
            what: GitSetup::Profiles,
        },
    ));
    // A profile names who commits for its remotes: a repository that was
    // asking may resolve through it now.
    crate::identity::settle_resolved(inner).await;
    Ok(view)
}

/// Remove a profile: its includes, then its file. A slug with no profile is
/// not an error.
pub async fn remove(inner: &Inner, slug: Slug) -> Result<(), EngineError> {
    let git = inner.git();
    let dir = inner.ws.paths().git_profiles_dir();
    blocking(move || {
        let file = file_of(&dir, &slug);
        git.include_remove(ConfigScope::Global, &file)?;
        match std::fs::remove_file(&file) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                return Err(bisa_vcs::VcsError::other(format!(
                    "{}: {e}",
                    file.display()
                )))
            }
        }
        Ok(())
    })
    .await?;
    inner.emit(crate::events::EngineEvent::global(
        EnginePayload::GitSetupChanged {
            what: GitSetup::Profiles,
        },
    ));
    crate::identity::settle_resolved(inner).await;
    Ok(())
}

/// The profile a raw remote URL falls under — the same prefix test git makes
/// on the includes' globs — or `None`.
pub async fn matching(
    inner: &Inner,
    remote_url: &str,
) -> Result<Option<GitProfileView>, EngineError> {
    let url = remote_url.to_string();
    let git = inner.git();
    let dir = inner.ws.paths().git_profiles_dir();
    blocking(move || {
        let (profiles, _) = read_profiles(&git, &dir)?;
        Ok(profiles.into_iter().find(|p| p.profile.matches(&url)))
    })
    .await
}

/// The profile whose file `origin` — a `git config --show-origin` answer's
/// file — is, when it is one of ours. Pure over the path.
pub fn profile_slug_of_file(dir: &Path, file: &Path) -> Option<Slug> {
    if !file.starts_with(dir) || file.extension().is_none_or(|e| e != "gitconfig") {
        return None;
    }
    file.file_stem()
        .and_then(|s| s.to_str())
        .and_then(|s| Slug::new(s).ok())
}

/// Move every platform include back to the end of the global file — called
/// after any other global write, so a value written below an include cannot
/// silently outrank a profile.
pub fn reappend_includes(git: &Git, dir: &Path) -> VcsResult<()> {
    let (profiles, _) = read_profiles(git, dir)?;
    for p in profiles {
        for glob in &p.globs {
            git.include_set(ConfigScope::Global, &condition_of(glob), &p.file)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_globs_read_back_into_host_owner_and_aliases() {
        let globs = vec![
            "https://github.com/acme/**".to_string(),
            "ssh://git@github.com/acme/**".to_string(),
            "git@github.com:acme/**".to_string(),
            "ssh://git@github-acme/acme/**".to_string(),
            "git@github-acme:acme/**".to_string(),
        ];
        let (host, owner, aliases) = spec_of_globs(&globs).unwrap();
        assert_eq!((host.as_str(), owner.as_str()), ("github.com", "acme"));
        assert_eq!(aliases, ["github-acme"]);
        assert!(
            spec_of_globs(&["gitdir:/x/**".to_string()]).is_none(),
            "not one of ours"
        );
        let dir = Path::new("/ws/identity/git/profiles");
        assert_eq!(
            profile_slug_of_file(dir, &dir.join("acme.gitconfig"))
                .unwrap()
                .as_str(),
            "acme"
        );
        assert!(profile_slug_of_file(dir, Path::new("/home/ada/.gitconfig")).is_none());
        assert!(profile_slug_of_file(dir, &dir.join("notes.txt")).is_none());
    }
}
