//! What the platform needs before it can work, as five checks a person can
//! read and act on (16 — The setup gate): `git` on `PATH`, one installed
//! harness, and the three core agents — the Decision-Making Agent configured
//! and ready, the General Agent and the Workflow Agent runnable — each with
//! the official way to fix it (`bisa_harness::install`) and, where the node
//! can vouch for one, a one-click fix. The desktop's gate, `GET /readiness`
//! and `bisa doctor` read this and nothing else; the platform installs nothing
//! itself and runs no command — it shows the official one to copy.

use crate::decider;
use crate::Inner;
use bisa_core::{AgentId, ModelChoice, ModelPlan, ModelStrategy};
use bisa_harness::install::{git_hint, install_hint, InstallHint};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::Arc;

/// Which check.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CheckId {
    Git,
    Harness,
    DecisionMakingAgent,
    GeneralAgent,
    WorkflowAgent,
}

/// Where a check stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CheckState {
    Ready,
    /// Not on this machine: install it.
    Missing,
    /// Here, but not set up to work: configure it.
    Unready,
}

/// Where the desktop takes the person to fix a check by hand.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case", tag = "door")]
pub enum Door {
    Settings { tab: String },
    Agents,
    None,
}

/// A fix the node vouches for, applied by the caller with one call.
#[derive(Clone, Debug, PartialEq, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum Fix {
    /// Write these workspace settings (`PUT /settings/workspace`).
    Settings {
        label: String,
        set: BTreeMap<String, Value>,
    },
    /// Move an agent onto a harness with a plan (`PATCH /agents/{id}`).
    AgentHarness {
        label: String,
        agent: String,
        harness: String,
        models: ModelPlan,
    },
}

/// One check, in words a person acts on.
#[derive(Clone, Debug, PartialEq, Serialize, schemars::JsonSchema)]
pub struct Check {
    pub id: CheckId,
    pub state: CheckState,
    pub title: String,
    pub detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<InstallHint>,
    pub door: Door,
    pub fixes: Vec<Fix>,
}

/// The five checks and whether every one is ready.
#[derive(Clone, Debug, PartialEq, Serialize, schemars::JsonSchema)]
pub struct Readiness {
    pub ready: bool,
    pub checks: Vec<Check>,
    pub checked_at: u64,
}

/// An installed harness with what the fixes would write: the plan and the
/// judge it recommends, and the models it lists for when it recommends none.
struct Installed {
    id: String,
    label: String,
    models: Vec<String>,
    /// `HarnessAdapter::recommended_plan`.
    plan: Option<ModelPlan>,
    /// `HarnessAdapter::recommended_judge`.
    judge: Option<String>,
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
}

/// The five checks, read now.
pub async fn readiness(inner: &Arc<Inner>) -> Readiness {
    let installed = installed(inner).await;
    let checks = vec![
        git(inner).await,
        harness(&installed),
        decision_making_agent(inner, &installed),
        core_agent(inner, AgentId::general(), "The General Agent", &installed),
        core_agent(inner, AgentId::workflow(), "The Workflow Agent", &installed),
    ];
    Readiness {
        ready: checks.iter().all(|c| c.state == CheckState::Ready),
        checks,
        checked_at: now_secs(),
    }
}

/// The harnesses that probed as installed, each with the models it lists
/// and what it recommends.
async fn installed(inner: &Arc<Inner>) -> Vec<Installed> {
    let cache = inner.cache.settings();
    let listing = inner
        .catalog
        .list_cached(
            cache.harness_listing_ttl(),
            std::time::Duration::from_secs(3),
        )
        .await;
    let mut out = Vec::new();
    for l in listing.into_iter().filter(|l| l.probe.available) {
        // A preset or a custom row has no adapter to run an agent on.
        let Some(adapter) = inner.catalog.get(&l.id) else {
            continue;
        };
        let models = inner
            .catalog
            .models_for(&l.id, cache.harness_models_ttl())
            .await
            .into_iter()
            .map(|m| m.id)
            .collect();
        out.push(Installed {
            id: l.id,
            label: l.label,
            models,
            plan: adapter.recommended_plan(),
            judge: adapter.recommended_judge(),
        });
    }
    out
}

async fn git(inner: &Arc<Inner>) -> Check {
    let handle = inner.git();
    let version = tokio::task::spawn_blocking(move || handle.version())
        .await
        .map_err(|e| e.to_string())
        .and_then(|r| r.map_err(|e| e.to_string()));
    match version {
        Ok(v) if v.supports_hasconfig() => Check {
            id: CheckId::Git,
            state: CheckState::Ready,
            title: "git".into(),
            detail: format!("git {}.{}.{} is on your PATH.", v.major, v.minor, v.patch),
            hint: None,
            door: Door::None,
            fixes: Vec::new(),
        },
        Ok(v) => Check {
            id: CheckId::Git,
            state: CheckState::Ready,
            title: "git".into(),
            detail: format!(
                "git {}.{}.{} is on your PATH. Profiles by organization need 2.36 or newer; everything else works.",
                v.major, v.minor, v.patch
            ),
            hint: Some(git_hint()),
            door: Door::None,
            fixes: Vec::new(),
        },
        Err(why) => Check {
            id: CheckId::Git,
            state: CheckState::Missing,
            title: "git".into(),
            detail: format!(
                "git was not found on your PATH ({why}). Projects, workstreams and every commit need it."
            ),
            hint: Some(git_hint()),
            door: Door::None,
            fixes: Vec::new(),
        },
    }
}

fn harness(installed: &[Installed]) -> Check {
    if installed.is_empty() {
        return Check {
            id: CheckId::Harness,
            state: CheckState::Missing,
            title: "A coding harness".into(),
            detail: "No coding harness is installed on this machine. Agents run on one — Claude Code, Codex, OpenCode, GitHub Copilot CLI, Grok Build or Gemini CLI; install one, sign in to it, then check again.".into(),
            hint: install_hint("claude-code"),
            door: Door::Settings {
                tab: "harnesses".into(),
            },
            fixes: Vec::new(),
        };
    }
    let names: Vec<&str> = installed.iter().map(|h| h.label.as_str()).collect();
    Check {
        id: CheckId::Harness,
        state: CheckState::Ready,
        title: "A coding harness".into(),
        detail: format!("Installed: {}.", names.join(", ")),
        hint: None,
        door: Door::Settings {
            tab: "harnesses".into(),
        },
        fixes: Vec::new(),
    }
}

/// The model the judge's fix pins on a harness: the one it recommends for
/// judging, else the first it lists, else the harness's own default (an
/// empty model word).
fn judge_model(h: &Installed) -> String {
    h.judge
        .clone()
        .or_else(|| h.models.first().cloned())
        .unwrap_or_default()
}

/// The plan an agent's fix writes on a harness: the one it recommends, else
/// the first two models it lists, else the plan the agent has. The effort
/// the agent's plan names is the person's word and is kept, whichever plan
/// is written.
fn agent_plan(h: &Installed, current: &ModelPlan) -> ModelPlan {
    let plan = match (&h.plan, h.models.is_empty()) {
        (Some(recommended), _) => recommended.clone(),
        (None, false) => ModelPlan {
            strategy: ModelStrategy::Fallback,
            effort: None,
            models: h.models.iter().take(2).map(ModelChoice::new).collect(),
        },
        // Nothing to write a plan from: the agent's own, when it names a
        // model, and the harness's default otherwise.
        (None, true) if current.models.is_empty() => ModelPlan::default(),
        (None, true) => current.clone(),
    };
    ModelPlan {
        effort: current.effort,
        ..plan
    }
}

/// The one core agent with no record to read: what stands ready or not is
/// the `decisions.*` settings, so its door is Settings and not Agents.
fn decision_making_agent(inner: &Arc<Inner>, installed: &[Installed]) -> Check {
    let door = Door::Settings {
        tab: "decision-making".into(),
    };
    let status = match decider::status(inner) {
        Ok(s) => s,
        Err(e) => {
            return Check {
                id: CheckId::DecisionMakingAgent,
                state: CheckState::Unready,
                title: "The Decision-Making Agent".into(),
                detail: format!("The Decision-Making Agent could not be read: {e}"),
                hint: None,
                door,
                fixes: Vec::new(),
            }
        }
    };
    if status.enabled && status.ready {
        // The model, and the effort it works at when a harness answers.
        let answers = match status.effort {
            Some(effort) => format!("{} · {effort}", status.answers_as),
            None => status.answers_as.clone(),
        };
        return Check {
            id: CheckId::DecisionMakingAgent,
            state: CheckState::Ready,
            title: "The Decision-Making Agent".into(),
            detail: format!("On — answers as {answers}."),
            hint: None,
            door,
            fixes: Vec::new(),
        };
    }
    let why = if !status.enabled {
        "It is switched off.".to_string()
    } else {
        status
            .problem
            .clone()
            .unwrap_or_else(|| "It cannot be asked as it is set up.".into())
    };
    let fixes = installed
        .iter()
        .map(|h| {
            let model = judge_model(h);
            let mut set = BTreeMap::new();
            set.insert(decider::keys::ENABLED.to_string(), json!(true));
            set.insert(decider::keys::PROVIDER.to_string(), json!("harness"));
            set.insert(decider::keys::HARNESS.to_string(), json!(h.id));
            set.insert(decider::keys::HARNESS_MODEL.to_string(), json!(model));
            Fix::Settings {
                label: if model.is_empty() {
                    format!("Use {} as the Decision-Making Agent", h.label)
                } else {
                    format!("Use {} · {model} as the Decision-Making Agent", h.label)
                },
                set,
            }
        })
        .collect();
    Check {
        id: CheckId::DecisionMakingAgent,
        state: CheckState::Unready,
        title: "The Decision-Making Agent".into(),
        detail: format!(
            "{why} Pick a model that answers the platform's judgements: one of an installed harness, or Jev with its API key (Settings › Decision Settings › Decision Making)."
        ),
        hint: None,
        door,
        fixes,
    }
}

fn core_agent(inner: &Arc<Inner>, id: AgentId, title: &str, installed: &[Installed]) -> Check {
    let agent = match inner.ws.get_agent(&id) {
        Ok(a) => a,
        Err(e) => {
            return Check {
                id: check_of(&id),
                state: CheckState::Unready,
                title: title.into(),
                detail: format!("{title} could not be read: {e}"),
                hint: None,
                door: Door::Agents,
                fixes: Vec::new(),
            }
        }
    };
    let on_installed = installed.iter().any(|h| h.id == agent.harness);
    let has_plan = !agent.models.models.is_empty();
    if agent.enabled && on_installed && has_plan {
        return Check {
            id: check_of(&id),
            state: CheckState::Ready,
            title: title.into(),
            detail: format!(
                "Runs on {} · {} model{}.",
                agent.harness,
                agent.models.models.len(),
                if agent.models.models.len() == 1 {
                    ""
                } else {
                    "s"
                }
            ),
            hint: None,
            door: Door::Agents,
            fixes: Vec::new(),
        };
    }
    let why = if !on_installed {
        format!(
            "Its harness, {}, is not installed on this machine.",
            agent.harness
        )
    } else {
        "It names no model to run on.".to_string()
    };
    let fixes = installed
        .iter()
        .map(|h| Fix::AgentHarness {
            label: format!("Run on {}", h.label),
            agent: id.to_string(),
            harness: h.id.clone(),
            models: agent_plan(h, &agent.models),
        })
        .collect();
    Check {
        id: check_of(&id),
        state: CheckState::Unready,
        title: title.into(),
        detail: format!("{why} Move it onto an installed harness with a model, under Agents."),
        hint: None,
        door: Door::Agents,
        fixes,
    }
}

fn check_of(id: &AgentId) -> CheckId {
    if id.as_str() == AgentId::WORKFLOW {
        CheckId::WorkflowAgent
    } else {
        CheckId::GeneralAgent
    }
}
