//! Tags over HTTP: the `?tag=` filter every list route wears, and the facet
//! counts a filter bar is built from.
//!
//! **One filter, one implementation.** A tag is the platform's only grouping
//! mechanism, and eight routes each parsing `?tag=` their own way would be
//! eight places for `?match=all` to drift. So the parsing lives in
//! [`TagFilter`] — an extractor a handler adds to its signature — and the
//! narrowing lives in [`TagFilter::apply`], which intersects
//! [`bisa_store::Workspace::ids_with_tags`] with the list the route has
//! already produced. Filtering *after* the list rather than instead of it
//! keeps each route's ordering, its enrichment (unread counts, workstream
//! counts, webhook paths) and its visibility rules exactly as they were.
//!
//! **A tag that cannot be a tag is a 400 naming it**, never a silent empty
//! list: a filter that quietly matches nothing is indistinguishable from one
//! that correctly matches nothing, and the caller cannot tell which they got.
//! [`bisa_core::Tags::new`] is the same door every write goes through, so
//! a filter and the values it filters on are normalized by one rule.

use crate::dto::{TagEntityCount, TagFacet};
use crate::route_docs::RouteDoc;
use crate::{bad_request, ApiError, Shared};
use axum::extract::{FromRequestParts, Query, State};
use axum::http::request::Parts;
use axum::routing::get;
use axum::{Json, Router};
use bisa_core::tags::{TagEntity, TagMatch};
use bisa_core::Localize as _;
use bisa_core::Tags;
use bisa_store::Workspace;
use serde::Deserialize;
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::str::FromStr;

pub(crate) fn routes() -> Router<Shared> {
    Router::new().route("/tags", get(list_tags))
}

/// Normalize a body's tag list, refusing what cannot become a tag.
///
/// Write paths call this rather than deserializing straight into [`Tags`]:
/// the `Deserialize` impl *sanitizes*, dropping what it cannot slugify, which
/// is right for a truth file that must always load and wrong for a request,
/// where a dropped tag is a caller believing they filed something.
pub(crate) fn parse_tags(raw: &[String]) -> Result<Tags, ApiError> {
    Tags::new(raw).map_err(|e| bad_request(e.text()))
}

/// `?tag=` (repeatable, and comma-separated) plus `?match=any|all`.
pub(crate) struct TagFilter {
    tags: Vec<String>,
    mode: TagMatch,
}

impl<S: Send + Sync> FromRequestParts<S> for TagFilter {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        // Deserialized as pairs, not as a struct: `?tag=` repeats, and
        // serde_urlencoded — what `Query` deserializes with — has no sequence
        // support, so a `Vec<String>` field would fail to parse rather than
        // collect. Pairs also arrive percent-decoded, so an encoded comma and
        // a literal one reach the split below the same way.
        let Query(pairs) = Query::<Vec<(String, String)>>::from_request_parts(parts, state)
            .await
            .map_err(|e| {
                bad_request(bisa_core::text!(
                    "error-node-tags-refused",
                    detail = e.to_string()
                ))
            })?;

        let mut tags: Vec<String> = Vec::new();
        let mut mode = TagMatch::Any;
        for (key, value) in pairs {
            match key.as_str() {
                // Both spellings are typed by real callers: `?tag=a&tag=b`
                // from a checkbox list, `?tag=a,b` from a hand-written URL.
                "tag" => tags.extend(
                    value
                        .split(',')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(str::to_string),
                ),
                "match" => {
                    mode = match value.as_str() {
                        "any" => TagMatch::Any,
                        "all" => TagMatch::All,
                        other => {
                            return Err(bad_request(bisa_core::text!(
                                "error-node-tags-unknown-tag-match-use-any-default-all",
                                other = format!("{other:?}")
                            )))
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(Self {
            tags: parse_tags(&tags)?.as_slice().to_vec(),
            mode,
        })
    }
}

impl TagFilter {
    /// Narrow `rows` to those carrying the requested tags, or hand them back
    /// untouched when no tag was asked for.
    ///
    /// The intersection runs against the tag index rather than each row's own
    /// `tags` field, so one query answers for a list of any size and a route
    /// whose rows only *wrap* a tagged object (a [`crate::dto::ProjectRow`]
    /// holds its project) needs no special case.
    pub(crate) fn apply<T>(
        &self,
        ws: &Workspace,
        entity: TagEntity,
        rows: Vec<T>,
        id_of: impl Fn(&T) -> String,
    ) -> Result<Vec<T>, ApiError> {
        if self.tags.is_empty() {
            return Ok(rows);
        }
        let keep: HashSet<String> = ws
            .ids_with_tags(entity, &self.tags, self.mode)?
            .into_iter()
            .collect();
        Ok(rows
            .into_iter()
            .filter(|row| keep.contains(&id_of(row)))
            .collect())
    }

    /// The same question [`TagFilter::apply`] asks the index, asked of a tag
    /// list directly.
    ///
    /// [`crate::catalog`] is the caller: a catalog entry is a definition
    /// compiled into the binary, not an object in the workspace, so it carries
    /// tags but has no index row. Routing it through `apply` would match
    /// nothing and read as "no entry wears that tag" rather than "the index has
    /// never seen these" — the exact confusion the 400 on a bad tag exists to
    /// prevent.
    pub(crate) fn matches(&self, tags: &Tags) -> bool {
        if self.tags.is_empty() {
            return true;
        }
        match self.mode {
            TagMatch::Any => self.tags.iter().any(|t| tags.contains(t)),
            TagMatch::All => self.tags.iter().all(|t| tags.contains(t)),
        }
    }
}

#[derive(Deserialize)]
struct EntityQuery {
    #[serde(default)]
    entity: Option<String>,
}

/// Every tag in use, what carries it, and how many — the whole filter bar in
/// one request.
///
/// Grouped by tag rather than returned as the index's `(entity, tag, count)`
/// rows, because that is the shape a bar is drawn in: one chip per tag,
/// showing a total, and knowing which sections the chip narrows. `?entity=`
/// narrows to one kind, for a screen that only lists one.
async fn list_tags(
    State(state): State<Shared>,
    Query(q): Query<EntityQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let entity = q
        .entity
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| {
            TagEntity::from_str(s).map_err(|e| {
                bad_request(bisa_core::text!(
                    "error-node-tags-refused",
                    detail = e.to_string()
                ))
            })
        })
        .transpose()?;

    let mut by_tag: HashMap<String, TagFacet> = HashMap::new();
    for row in state.engine.workspace().tag_counts(entity)? {
        // The index writes `TagEntity::as_str()`, so a row that does not parse
        // came from a build that knew a kind this one does not. Counting it
        // under a name no `?entity=` accepts would put an unreachable chip in
        // the bar, so it is left out and said so in the log.
        let Ok(kind) = row.entity.parse::<TagEntity>() else {
            tracing::warn!("tag facets: unknown entity {:?}, skipping", row.entity);
            continue;
        };
        let facet = by_tag.entry(row.tag.clone()).or_insert_with(|| TagFacet {
            tag: row.tag.clone(),
            total: 0,
            entities: Vec::new(),
        });
        facet.total += row.count;
        facet.entities.push(TagEntityCount {
            entity: kind,
            count: row.count,
        });
    }

    let mut tags: Vec<TagFacet> = by_tag.into_values().collect();
    // Most-used first, ties alphabetical — a bar is read top-down, and a
    // stable order keeps chips from moving between two identical requests.
    tags.sort_by(|a, b| b.total.cmp(&a.total).then_with(|| a.tag.cmp(&b.tag)));
    for facet in &mut tags {
        facet.entities.sort_by(|a, b| {
            b.count
                .cmp(&a.count)
                .then_with(|| a.entity.as_str().cmp(b.entity.as_str()))
        });
    }
    Ok(Json(json!({"entity": entity, "tags": tags})))
}

/// The routes this module mounts, for `bisa-node --bin api-docs` and the
/// route test. Kept beside `routes()` so a route added here is added here.
pub const ROUTES: &[RouteDoc] = &[RouteDoc {
    method: "GET",
    path: "/tags",
    summary: "Every tag in use with counts, optionally for one kind (`?entity=`).",
}];
