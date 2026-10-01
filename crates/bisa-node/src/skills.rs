//! The skill library over HTTP: the procedures agents follow, written once.
//!
//! An agent's system prompt is its role; a skill is a procedure — the
//! checklist it works through when it does one specific kind of thing. These
//! routes edit the library; [`crate::agents_api`] wires an agent to an entry
//! by id. Nothing here takes an agent, and nothing in the agent routes takes
//! markdown, which is what keeps one code-review checklist from becoming
//! twenty drifting copies.
//!
//! The id is a slug, and it is immutable: it becomes a directory name inside
//! a session's skills dir and it is what every agent references, so a rename
//! is a create plus a re-attach rather than a silent detachment.

use crate::dto::*;
use crate::route_docs::RouteDoc;
use crate::tags::{parse_tags, TagFilter};
use crate::{skill_id, ApiError, Shared};
use axum::extract::{Path as AxPath, State};
use axum::routing::get;
use axum::{Json, Router};
use bisa_core::tags::TagEntity;
use bisa_store::NewSkill;
use serde_json::json;

pub(crate) fn routes() -> Router<Shared> {
    Router::new()
        .route("/skills", get(list).post(create))
        .route(
            "/skills/{id}",
            get(detail).patch(patch_skill).delete(remove),
        )
}

async fn list(
    State(state): State<Shared>,
    filter: TagFilter,
) -> Result<Json<serde_json::Value>, ApiError> {
    let ws = state.engine.workspace();
    let skills = filter.apply(ws, TagEntity::Skill, ws.list_skills()?, |s| {
        s.id.to_string()
    })?;
    Ok(Json(json!({"skills": skills})))
}

async fn create(
    State(state): State<Shared>,
    crate::Body(body): crate::Body<NewSkillBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // The store validates the slug, the empty description and the size cap
    // with messages worth passing through; only the tags need a door of their
    // own, because `Tags` on a body would sanitize rather than refuse.
    let skill = bisa_engine::directory::create_skill(
        state.engine.inner(),
        NewSkill {
            id: skill_id(&body.id)?,
            name: body.name,
            description: body.description,
            tags: parse_tags(&body.tags)?,
            markdown: body.markdown,
        },
    )?;
    Ok(Json(json!({"skill": skill})))
}

async fn detail(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let ws = state.engine.workspace();
    let id = skill_id(&id)?;
    let skill = ws.get_skill(&id)?;
    // A skill detail screen's other half: who follows this procedure. Cheap
    // here (agents are already listed for every screen) and impossible for a
    // client to ask for otherwise without fetching every agent.
    let agents: Vec<serde_json::Value> = ws
        .list_agents()?
        .into_iter()
        .filter(|a| a.skills.contains(&id))
        .map(|a| json!({"id": a.id, "name": a.name}))
        .collect();
    Ok(Json(json!({"skill": skill, "agents": agents})))
}

async fn patch_skill(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
    crate::Body(body): crate::Body<PatchSkillBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let ws = state.engine.workspace();
    let mut skill = ws.get_skill(&skill_id(&id)?)?;
    if let Some(v) = body.name {
        skill.name = v;
    }
    if let Some(v) = body.description {
        skill.description = v;
    }
    if let Some(v) = &body.tags {
        skill.tags = parse_tags(v)?;
    }
    if let Some(v) = body.markdown {
        skill.markdown = v;
    }
    Ok(Json(
        json!({"skill": bisa_engine::directory::update_skill(state.engine.inner(), skill)?}),
    ))
}

/// Delete a skill, or refuse while any agent still carries it.
///
/// This used to detach the skill from every agent first, which meant one
/// `DELETE` could rewrite twenty agent definitions with nothing in the
/// response to say so. Now the store refuses and names the agents, arriving
/// here as a 400 carrying that message; `GET /usage/skill/{id}` asks the same
/// question without attempting the delete.
async fn remove(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    bisa_engine::directory::remove_skill(state.engine.inner(), &skill_id(&id)?)?;
    Ok(Json(json!({"ok": true, "skill": id})))
}

/// The routes this module mounts, for `bisa-node --bin api-docs` and the
/// route test. Kept beside `routes()` so a route added here is added here.
pub const ROUTES: &[RouteDoc] = &[
    RouteDoc {
        method: "GET",
        path: "/skills",
        summary: "Every skill.",
    },
    RouteDoc {
        method: "POST",
        path: "/skills",
        summary: "Create a skill: `{name, description, markdown, tags?}`.",
    },
    RouteDoc {
        method: "GET",
        path: "/skills/{id}",
        summary: "One skill.",
    },
    RouteDoc {
        method: "PATCH",
        path: "/skills/{id}",
        summary: "Edit a skill.",
    },
    RouteDoc {
        method: "DELETE",
        path: "/skills/{id}",
        summary: "Delete a skill, or 409 while an agent still carries it.",
    },
];
