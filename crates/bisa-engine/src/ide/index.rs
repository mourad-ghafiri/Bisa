//! The path index quick open scores against (ide/12): every file under a
//! root, relative, `.gitignore` honoured, symlinks never followed, bounded.
//! The client caches it per root and patches it from `file_changed`; the server
//! now caches the walk too, so a burst of quick-open / name-search
//! requests within `cache.path_index.ttl_ms` shares one walk rather than
//! re-walking up to 500k entries each time. A file change under a root — the
//! engine's own write or a watched external one — invalidates that root's entry.

use crate::{EngineError, Inner};
use bisa_store::FileScope;
use std::sync::{Arc, LazyLock};

/// The most paths an index carries before it says `truncated`.
pub const DEFAULT_LIMIT: usize = 100_000;
pub const MAX_LIMIT: usize = 500_000;

/// The cached walk per root, keyed `(scope, id)` as strings so the two
/// `FileChanged` emit sites can invalidate by the same key they already hold.
/// Only the default-limit walk is cached; a custom limit bypasses it.
static PATH_CACHE: LazyLock<bisa_cache::TtlCache<(String, String), Arc<PathIndex>>> =
    LazyLock::new(|| bisa_cache::TtlCache::with_capacity("ide.path_index", MAX_CACHED_ROOTS));

/// The most roots whose walk is held at once. One index is up to
/// [`DEFAULT_LIMIT`] paths — megabytes — and a node that has opened many
/// checkouts over its life held one for each, for good: an entry was only
/// ever let go when the same root was asked for again past its time. Past
/// this many the oldest goes; a walk is what it costs to ask again.
pub const MAX_CACHED_ROOTS: usize = 16;
// As many roots as a person has open at once, and a few more — checked when this is built.
const _: () = assert!(MAX_CACHED_ROOTS >= 8);

/// Forget one root's cached index — a file changed under it, or the root
/// itself is gone (a workstream closed with its tree, a project deleted).
pub fn invalidate(scope: &str, id: &str) {
    PATH_CACHE.invalidate(&(scope.to_string(), id.to_string()));
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PathIndex {
    pub paths: Vec<String>,
    pub truncated: bool,
}

pub fn paths(
    inner: &Arc<Inner>,
    scope: FileScope,
    id: &str,
    limit: Option<usize>,
) -> Result<PathIndex, EngineError> {
    // Only the default-limit walk is shared; a caller with a custom limit gets a
    // fresh walk and neither reads nor writes the cache.
    let ttl = inner.cache.settings().path_index_ttl();
    let cacheable = limit.is_none() && !ttl.is_zero();
    let cache_key = (scope.as_str().to_string(), id.to_string());
    if cacheable {
        if let Some(idx) = PATH_CACHE.get(ttl, &cache_key) {
            return Ok((*idx).clone());
        }
    }
    let root = inner.ws.file_root(scope, id)?;
    if !root.is_dir() {
        return Ok(PathIndex {
            paths: Vec::new(),
            truncated: false,
        });
    }
    let limit = limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let mut out = Vec::new();
    let mut truncated = false;
    let walk = ignore::WalkBuilder::new(&root)
        .follow_links(false)
        .hidden(true)
        .git_ignore(true)
        .git_global(false)
        .git_exclude(true)
        .sort_by_file_name(|a, b| a.cmp(b))
        .build();
    for entry in walk {
        let Ok(entry) = entry else { continue };
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        if out.len() >= limit {
            truncated = true;
            break;
        }
        if let Ok(rel) = entry.path().strip_prefix(&root) {
            out.push(rel.to_string_lossy().into_owned());
        }
    }
    let index = PathIndex {
        paths: out,
        truncated,
    };
    if cacheable {
        PATH_CACHE.insert(cache_key, Arc::new(index.clone()));
    }
    Ok(index)
}
