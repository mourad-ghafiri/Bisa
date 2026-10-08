//! The catalog: the staff a workspace can install, and installs nothing until
//! it is asked.
//!
//! An earlier shape seeded the whole library — every agent, skill, team and
//! channel it shipped — into every workspace on its first run. That answered the wrong
//! question. A platform that starts empty asks the owner to invent a staff
//! before they can use it — but a platform that starts with a whole
//! organisation charges them for one, in a roster nobody chose and sessions
//! nobody asked for. So the same library is here, and it is a **catalog**: a
//! browsable list of definitions, each installed on demand. The one agent a
//! workspace opens with are the two in [`crate::core_agents`].
//!
//! An install is **transitive**: a team installs its agents, an agent installs
//! its skills, a channel installs its roster. It is **idempotent**: something
//! already present is left exactly as it is, and is not reported in
//! [`Installed`], which says what was *created* rather than what was touched —
//! an install that quietly creates six things is one you cannot undo
//! confidently. And an id that exists but did not come from this catalog entry
//! is a **collision**, refused by name rather than overwritten: two different
//! definitions cannot share an id, and the one already here is the one
//! somebody chose.
//!
//! **Ids are the bare slugs.** The Developer is `developer`, the Engineering
//! team is `engineering`. There is no `builtin-` prefix any more: it existed
//! to keep seed data from colliding with definitions the owner made, and
//! nothing is seeded now. The bundled TOMLs already name each other by slug.
//!
//! Order matters inside an install and is owned here, not by callers: a skill
//! before the agent that carries it, an agent before the team or channel that
//! names it. A reference that does not resolve is a bundle bug, and
//! `bundle_tests` is where it gets caught — at `cargo test`, not at a user's
//! first install.
//!
//! The `general` channel is **not** in the catalog: it is a permanent object
//! ensured at every open (`library/core/general.toml`), the way the two core
//! agents are. The catalog ships the eight rooms a workspace may choose.
//!
//! **Addons are the seventh kind.** A built-in addon is a folder under
//! `library/addons/<slug>/`, embedded whole by the build script
//! (`build.rs` → `BUILTIN_ADDONS`); installing one materialises its bundle
//! under `addons/<slug>/` with the grants it declares (`addons.rs`). Its
//! entry pair is `(slug, addon.json)` rather than `(slug, toml)`, and it
//! requires nothing.
//!
//! **Workflows are the fifth kind.** A workflow template installs the agents
//! its steps name and the workflows its `spawn` steps start, then itself —
//! under a ULID of its own, with `Origin::Catalog { slug }` remembering where
//! it came from. A template names other templates by slug; the install
//! resolves each slug to the installed workflow's id, so the definition that
//! lands in the workspace is the one the designer and the run machine read.

use crate::agents::NewAgent;
use crate::error::StoreError;
use crate::skills::NewSkill;
use crate::workflows::NewWorkflow;
use crate::workspace::Workspace;
use bisa_core::{
    AgentId, AgentOrigin, Assignee, ChannelId, ChannelOrigin, EffortChoice, InputDef, ModelChoice,
    ModelPlan, ModelStrategy, Origin, RosterPolicy, SkillId, Step, Tags, TeamId, ValueRef,
    WorkflowId, WorkflowOrigin,
};
use serde::{Deserialize, Serialize};

/// The catalog's definitions as `(slug, toml)`, one list per kind.
pub struct Catalog {
    pub skills: &'static [(&'static str, &'static str)],
    pub agents: &'static [(&'static str, &'static str)],
    pub teams: &'static [(&'static str, &'static str)],
    pub channels: &'static [(&'static str, &'static str)],
    pub workflows: &'static [(&'static str, &'static str)],
    pub connectors: &'static [(&'static str, &'static str)],
    /// The built-in pets — unlike every other kind, **installed by nobody**:
    /// a pet is listed and drawn from the binary, never copied into a
    /// workspace, never removed (`pets.rs`).
    pub pets: &'static [BuiltinPet],
    /// The built-in addons as `(slug, addon.json)` — the pair the listing,
    /// the description and the install walk read, like every other kind.
    pub addons: &'static [(&'static str, &'static str)],
    /// The same addons with their bundles, for the install that copies one in.
    pub addon_bundles: &'static [BuiltinAddon],
}

/// One addon the platform ships: its slug — also its id — its manifest and
/// every file of its bundle as `(path, bytes)`, walked from
/// `library/addons/<slug>/` by `build.rs`.
#[derive(Clone, Copy, Debug)]
pub struct BuiltinAddon {
    pub slug: &'static str,
    pub manifest: &'static str,
    pub files: &'static [(&'static str, &'static [u8])],
}

include!(concat!(env!("OUT_DIR"), "/builtin_addons.rs"));

/// One pet the platform ships: its manifest and its sheet, as the pack
/// drew them (`library/pets/<slug>/`). The `sheet.svg` beside them is the
/// drawing's source and does not ship.
#[derive(Clone, Copy, Debug)]
pub struct BuiltinPet {
    pub slug: &'static str,
    pub manifest: &'static str,
    pub sheet: &'static [u8],
}

impl Catalog {
    pub fn entries(&self, kind: CatalogKind) -> &'static [(&'static str, &'static str)] {
        match kind {
            CatalogKind::Agent => self.agents,
            CatalogKind::Skill => self.skills,
            CatalogKind::Team => self.teams,
            CatalogKind::Channel => self.channels,
            CatalogKind::Workflow => self.workflows,
            CatalogKind::Connector => self.connectors,
            CatalogKind::Addon => self.addons,
        }
    }

    /// How many entries the build ships, of every kind — the one tally the
    /// listings are measured against, so a new kind is one edit here and
    /// none in a test.
    pub fn entry_count(&self) -> usize {
        CatalogKind::ALL
            .iter()
            .map(|kind| self.entries(*kind).len())
            .sum()
    }

    /// A built-in addon's bundle, by slug.
    pub fn addon_bundle(&self, slug: &str) -> Option<&'static BuiltinAddon> {
        self.addon_bundles.iter().find(|a| a.slug == slug)
    }

    fn find(&self, kind: CatalogKind, slug: &str) -> Result<&'static str, StoreError> {
        self.entries(kind)
            .iter()
            .find(|(s, _)| *s == slug)
            .map(|(_, toml_str)| *toml_str)
            .ok_or_else(|| {
                StoreError::Invalid(bisa_core::text!(
                    "error-store-invalid-catalog-has-no-called",
                    kind = kind.to_string(),
                    slug = format!("{slug:?}")
                ))
            })
    }
}

/// The catalog this build ships.
pub const CATALOG: Catalog = Catalog {
    skills: CATALOG_SKILLS,
    agents: CATALOG_AGENTS,
    teams: CATALOG_TEAMS,
    channels: CATALOG_CHANNELS,
    workflows: CATALOG_WORKFLOWS,
    connectors: CATALOG_CONNECTORS,
    pets: CATALOG_PETS,
    addons: BUILTIN_ADDON_ENTRIES,
    addon_bundles: BUILTIN_ADDONS,
};

/// One built-in pet's row: the folder's name, its manifest, its sheet.
macro_rules! pet {
    ($slug:literal) => {
        BuiltinPet {
            slug: $slug,
            manifest: include_str!(concat!("../../../library/pets/", $slug, "/pet.json")),
            sheet: include_bytes!(concat!(
                "../../../library/pets/",
                $slug,
                "/spritesheet.webp"
            )),
        }
    };
}

/// The built-in pets, the *midnight-shipping* pack: nine companions drawn
/// against the sheet's nine states, each with its own pace. **Moonrice** is
/// the one shown when the pet is turned on and none was chosen
/// (`desktop/src/pet/petModel.mjs`, `DEFAULT_PET_ID`). The bundle tests hold
/// every manifest to `Pet::validate_animations` and every sheet to the
/// grid's size, so a pack that will not draw is caught at `cargo test`.
pub const CATALOG_PETS: &[BuiltinPet] = &[
    pet!("bracket"),
    pet!("buffer"),
    pet!("fathom"),
    pet!("jolt"),
    pet!("kernel"),
    pet!("loop"),
    pet!("lumen"),
    pet!("moonrice"),
    pet!("nocturn"),
];

/// The built-in connectors: a declarative definition of one outside
/// platform's API each — its hosts, its auth scheme, a handful of its most
/// useful operations — that a `connector` step calls. Nothing here holds a
/// credential; an account is added on the machine that has one.
const CATALOG_CONNECTORS: &[(&str, &str)] = &[
    (
        "confluence",
        include_str!("../../../library/catalog/connectors/confluence.toml"),
    ),
    (
        "facebook-pages",
        include_str!("../../../library/catalog/connectors/facebook-pages.toml"),
    ),
    (
        "gmail",
        include_str!("../../../library/catalog/connectors/gmail.toml"),
    ),
    (
        "google-calendar",
        include_str!("../../../library/catalog/connectors/google-calendar.toml"),
    ),
    (
        "google-drive",
        include_str!("../../../library/catalog/connectors/google-drive.toml"),
    ),
    (
        "instagram",
        include_str!("../../../library/catalog/connectors/instagram.toml"),
    ),
    (
        "jira",
        include_str!("../../../library/catalog/connectors/jira.toml"),
    ),
    (
        "linear",
        include_str!("../../../library/catalog/connectors/linear.toml"),
    ),
    (
        "notion",
        include_str!("../../../library/catalog/connectors/notion.toml"),
    ),
    (
        "obsidian",
        include_str!("../../../library/catalog/connectors/obsidian.toml"),
    ),
    (
        "slack",
        include_str!("../../../library/catalog/connectors/slack.toml"),
    ),
    (
        "tiktok",
        include_str!("../../../library/catalog/connectors/tiktok.toml"),
    ),
    (
        "trello",
        include_str!("../../../library/catalog/connectors/trello.toml"),
    ),
    (
        "x",
        include_str!("../../../library/catalog/connectors/x.toml"),
    ),
    (
        "youtube",
        include_str!("../../../library/catalog/connectors/youtube.toml"),
    ),
];

/// The thirteen workflow templates: one shape per domain a goal is likely to
/// come from. Each begins at an explicit start a person runs by hand; four
/// also begin on an event — `weekly-review` on a schedule,
/// `customer-support-triage` on a hook, `standing-health-check` on a check,
/// `incident-response` on a run that failed.
const CATALOG_WORKFLOWS: &[(&str, &str)] = &[
    (
        "software-feature",
        include_str!("../../../library/catalog/workflows/software-feature.toml"),
    ),
    (
        "bug-fix",
        include_str!("../../../library/catalog/workflows/bug-fix.toml"),
    ),
    (
        "research-report",
        include_str!("../../../library/catalog/workflows/research-report.toml"),
    ),
    (
        "content-pipeline",
        include_str!("../../../library/catalog/workflows/content-pipeline.toml"),
    ),
    (
        "incident-response",
        include_str!("../../../library/catalog/workflows/incident-response.toml"),
    ),
    (
        "hiring-loop",
        include_str!("../../../library/catalog/workflows/hiring-loop.toml"),
    ),
    (
        "event-plan",
        include_str!("../../../library/catalog/workflows/event-plan.toml"),
    ),
    (
        "weekly-review",
        include_str!("../../../library/catalog/workflows/weekly-review.toml"),
    ),
    (
        "customer-support-triage",
        include_str!("../../../library/catalog/workflows/customer-support-triage.toml"),
    ),
    (
        "product-launch",
        include_str!("../../../library/catalog/workflows/product-launch.toml"),
    ),
    (
        "decision-record",
        include_str!("../../../library/catalog/workflows/decision-record.toml"),
    ),
    (
        "standing-health-check",
        include_str!("../../../library/catalog/workflows/standing-health-check.toml"),
    ),
    (
        "mobile-release",
        include_str!("../../../library/catalog/workflows/mobile-release.toml"),
    ),
];

const CATALOG_SKILLS: &[(&str, &str)] = &[
    (
        "acceptance-criteria",
        include_str!("../../../library/catalog/skills/acceptance-criteria.toml"),
    ),
    (
        "app-store-publishing",
        include_str!("../../../library/catalog/skills/app-store-publishing.toml"),
    ),
    (
        "architecture-decision-record",
        include_str!("../../../library/catalog/skills/architecture-decision-record.toml"),
    ),
    (
        "artifacts",
        include_str!("../../../library/catalog/skills/artifacts.toml"),
    ),
    (
        "business-model-canvas",
        include_str!("../../../library/catalog/skills/business-model-canvas.toml"),
    ),
    (
        "code-review-checklist",
        include_str!("../../../library/catalog/skills/code-review-checklist.toml"),
    ),
    (
        "critical-review",
        include_str!("../../../library/catalog/skills/critical-review.toml"),
    ),
    (
        "drawing",
        include_str!("../../../library/catalog/skills/drawing.toml"),
    ),
    (
        "embedded-browser",
        include_str!("../../../library/catalog/skills/embedded-browser.toml"),
    ),
    (
        "evidence-and-citation",
        include_str!("../../../library/catalog/skills/evidence-and-citation.toml"),
    ),
    (
        "flutter-development",
        include_str!("../../../library/catalog/skills/flutter-development.toml"),
    ),
    (
        "google-play-publishing",
        include_str!("../../../library/catalog/skills/google-play-publishing.toml"),
    ),
    (
        "incident-triage",
        include_str!("../../../library/catalog/skills/incident-triage.toml"),
    ),
    (
        "options-and-tradeoffs",
        include_str!("../../../library/catalog/skills/options-and-tradeoffs.toml"),
    ),
    (
        "plain-voice",
        include_str!("../../../library/catalog/skills/plain-voice.toml"),
    ),
    (
        "positioning-and-messaging",
        include_str!("../../../library/catalog/skills/positioning-and-messaging.toml"),
    ),
    (
        "red-teaming",
        include_str!("../../../library/catalog/skills/red-teaming.toml"),
    ),
    (
        "release-checklist",
        include_str!("../../../library/catalog/skills/release-checklist.toml"),
    ),
    (
        "scoring-rubric",
        include_str!("../../../library/catalog/skills/scoring-rubric.toml"),
    ),
    (
        "structured-brainstorming",
        include_str!("../../../library/catalog/skills/structured-brainstorming.toml"),
    ),
    (
        "test-strategy",
        include_str!("../../../library/catalog/skills/test-strategy.toml"),
    ),
    (
        "threat-modeling",
        include_str!("../../../library/catalog/skills/threat-modeling.toml"),
    ),
    (
        "unit-economics",
        include_str!("../../../library/catalog/skills/unit-economics.toml"),
    ),
    (
        "work-item-decomposition",
        include_str!("../../../library/catalog/skills/work-item-decomposition.toml"),
    ),
    (
        "workstream-workflow",
        include_str!("../../../library/catalog/skills/workstream-workflow.toml"),
    ),
];

const CATALOG_AGENTS: &[(&str, &str)] = &[
    (
        "architect",
        include_str!("../../../library/catalog/agents/architect.toml"),
    ),
    (
        "business-analyst",
        include_str!("../../../library/catalog/agents/business-analyst.toml"),
    ),
    (
        "business-strategist",
        include_str!("../../../library/catalog/agents/business-strategist.toml"),
    ),
    (
        "code-humanizer",
        include_str!("../../../library/catalog/agents/code-humanizer.toml"),
    ),
    (
        "code-reviewer",
        include_str!("../../../library/catalog/agents/code-reviewer.toml"),
    ),
    (
        "communications-lead",
        include_str!("../../../library/catalog/agents/communications-lead.toml"),
    ),
    (
        "content-strategist",
        include_str!("../../../library/catalog/agents/content-strategist.toml"),
    ),
    (
        "critic",
        include_str!("../../../library/catalog/agents/critic.toml"),
    ),
    (
        "data-analyst",
        include_str!("../../../library/catalog/agents/data-analyst.toml"),
    ),
    (
        "developer",
        include_str!("../../../library/catalog/agents/developer.toml"),
    ),
    (
        "devops-engineer",
        include_str!("../../../library/catalog/agents/devops-engineer.toml"),
    ),
    (
        "document-humanizer",
        include_str!("../../../library/catalog/agents/document-humanizer.toml"),
    ),
    (
        "evaluator",
        include_str!("../../../library/catalog/agents/evaluator.toml"),
    ),
    (
        "facilitator",
        include_str!("../../../library/catalog/agents/facilitator.toml"),
    ),
    (
        "finance-analyst",
        include_str!("../../../library/catalog/agents/finance-analyst.toml"),
    ),
    (
        "general-advisor",
        include_str!("../../../library/catalog/agents/general-advisor.toml"),
    ),
    (
        "legal-advisor",
        include_str!("../../../library/catalog/agents/legal-advisor.toml"),
    ),
    (
        "marketing-manager",
        include_str!("../../../library/catalog/agents/marketing-manager.toml"),
    ),
    (
        "mobile-developer",
        include_str!("../../../library/catalog/agents/mobile-developer.toml"),
    ),
    (
        "performance-marketer",
        include_str!("../../../library/catalog/agents/performance-marketer.toml"),
    ),
    (
        "product-manager",
        include_str!("../../../library/catalog/agents/product-manager.toml"),
    ),
    (
        "project-manager",
        include_str!("../../../library/catalog/agents/project-manager.toml"),
    ),
    (
        "qa-engineer",
        include_str!("../../../library/catalog/agents/qa-engineer.toml"),
    ),
    (
        "red-team",
        include_str!("../../../library/catalog/agents/red-team.toml"),
    ),
    (
        "release-manager",
        include_str!("../../../library/catalog/agents/release-manager.toml"),
    ),
    (
        "researcher",
        include_str!("../../../library/catalog/agents/researcher.toml"),
    ),
    (
        "sales-lead",
        include_str!("../../../library/catalog/agents/sales-lead.toml"),
    ),
    (
        "security-engineer",
        include_str!("../../../library/catalog/agents/security-engineer.toml"),
    ),
    (
        "social-media-manager",
        include_str!("../../../library/catalog/agents/social-media-manager.toml"),
    ),
    (
        "sre",
        include_str!("../../../library/catalog/agents/sre.toml"),
    ),
    (
        "technical-writer",
        include_str!("../../../library/catalog/agents/technical-writer.toml"),
    ),
    (
        "ux-designer",
        include_str!("../../../library/catalog/agents/ux-designer.toml"),
    ),
];

const CATALOG_TEAMS: &[(&str, &str)] = &[
    (
        "design",
        include_str!("../../../library/catalog/teams/design.toml"),
    ),
    (
        "discovery",
        include_str!("../../../library/catalog/teams/discovery.toml"),
    ),
    (
        "engineering",
        include_str!("../../../library/catalog/teams/engineering.toml"),
    ),
    (
        "go-to-market",
        include_str!("../../../library/catalog/teams/go-to-market.toml"),
    ),
    (
        "launch",
        include_str!("../../../library/catalog/teams/launch.toml"),
    ),
    (
        "platform",
        include_str!("../../../library/catalog/teams/platform.toml"),
    ),
    (
        "review-board",
        include_str!("../../../library/catalog/teams/review-board.toml"),
    ),
    (
        "strategy",
        include_str!("../../../library/catalog/teams/strategy.toml"),
    ),
    (
        "venture",
        include_str!("../../../library/catalog/teams/venture.toml"),
    ),
];

const CATALOG_CHANNELS: &[(&str, &str)] = &[
    (
        "business",
        include_str!("../../../library/catalog/channels/business.toml"),
    ),
    (
        "delivery",
        include_str!("../../../library/catalog/channels/delivery.toml"),
    ),
    (
        "engineering",
        include_str!("../../../library/catalog/channels/engineering.toml"),
    ),
    (
        "go-to-market",
        include_str!("../../../library/catalog/channels/go-to-market.toml"),
    ),
    (
        "ideas",
        include_str!("../../../library/catalog/channels/ideas.toml"),
    ),
    (
        "operations",
        include_str!("../../../library/catalog/channels/operations.toml"),
    ),
    (
        "product",
        include_str!("../../../library/catalog/channels/product.toml"),
    ),
    (
        "review",
        include_str!("../../../library/catalog/channels/review.toml"),
    ),
];

/// What kind of definition a catalog entry is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogKind {
    Agent,
    Skill,
    Team,
    Channel,
    Connector,
    Workflow,
    Addon,
}

impl CatalogKind {
    /// Listing order, and the order an install walks: skills, connectors
    /// and addons are the leaves, channels, teams and workflows the widest
    /// entries.
    pub const ALL: &'static [CatalogKind] = &[
        CatalogKind::Agent,
        CatalogKind::Skill,
        CatalogKind::Team,
        CatalogKind::Channel,
        CatalogKind::Connector,
        CatalogKind::Workflow,
        CatalogKind::Addon,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            CatalogKind::Agent => "agent",
            CatalogKind::Skill => "skill",
            CatalogKind::Team => "team",
            CatalogKind::Channel => "channel",
            CatalogKind::Connector => "connector",
            CatalogKind::Workflow => "workflow",
            CatalogKind::Addon => "addon",
        }
    }
}

impl std::fmt::Display for CatalogKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for CatalogKind {
    type Err = StoreError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        CatalogKind::ALL
            .iter()
            .copied()
            .find(|k| k.as_str() == s)
            .ok_or_else(|| {
                StoreError::Invalid(bisa_core::text!(
                    "error-store-invalid-unknown-catalog-kind",
                    s = format!("{s:?}")
                ))
            })
    }
}

/// One row of the catalog, as a browsing surface sees it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CatalogEntry {
    pub kind: CatalogKind,
    /// The slug, which is also the id the entry installs under.
    pub slug: String,
    pub name: String,
    pub description: String,
    pub tags: Tags,
    /// What this entry pulls in with it — agent slugs for a team or a channel,
    /// skill slugs for an agent, and for a workflow the agent slugs its steps
    /// name plus the templates its `spawn` steps start, spelled
    /// `workflow:<slug>`. Installing the entry installs these too, so a
    /// picker can say what a choice actually costs before it is made.
    pub requires: Vec<String>,
    /// Whether an object with this id already exists here.
    ///
    /// It says the id is taken, not that this entry is what took it: an id
    /// held by something you created reads as installed and is refused by
    /// [`Workspace::install`] rather than overwritten.
    pub installed: bool,
    /// A workflow template's whole definition — its inputs, its steps and
    /// their flows — so a gallery draws the template's graph before anything
    /// is installed. Read with the listing's placeholder for a `spawn` target
    /// that is not here yet, so a fresh workspace still has every picture;
    /// `None` for every other kind.
    pub workflow: Option<NewWorkflow>,
}

/// What a refresh of the installed catalog connectors did
/// ([`Workspace::refresh_catalog_connectors`]): the slugs whose definition
/// moved to the bundle's revision, and how many accounts had a secret move
/// to the field a new scheme reads.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Refreshed {
    pub definitions: Vec<String>,
    pub accounts_moved: usize,
}

/// What an install actually created.
///
/// Only creations are listed. An entry already present is left exactly as it
/// is and appears nowhere here, so an empty `Installed` is the honest answer
/// to "install something twice" and the answer a caller can act on: an install
/// that quietly creates six things is one you cannot undo confidently.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Installed {
    pub agents: Vec<String>,
    pub skills: Vec<String>,
    pub teams: Vec<String>,
    pub channels: Vec<String>,
    /// `(slug, the id it was installed under)` — a workflow's id is a ULID,
    /// so the caller that wants to open what it just installed needs both.
    pub workflows: Vec<(String, WorkflowId)>,
    pub connectors: Vec<String>,
    pub addons: Vec<String>,
}

impl Installed {
    /// Whether this install created nothing — everything it named was here.
    pub fn is_empty(&self) -> bool {
        self.agents.is_empty()
            && self.skills.is_empty()
            && self.teams.is_empty()
            && self.channels.is_empty()
            && self.workflows.is_empty()
            && self.connectors.is_empty()
            && self.addons.is_empty()
    }
}

/// One catalog entry as its TOML alone describes it — no workspace, so no
/// `installed`. This is what the generated reference
/// (`docs/reference/catalog.md`, `just gen-catalog-docs`) and the bundle tests
/// read, so the page cannot drift from the sources: the same parse that
/// installs an entry describes it.
#[derive(Clone, Debug, PartialEq)]
pub struct CatalogDescription {
    pub kind: CatalogKind,
    pub slug: String,
    pub name: String,
    /// An agent's or a workflow's description, a skill's "use it when", a
    /// team's purpose, a channel's topic.
    pub description: String,
    pub tags: Vec<String>,
    pub detail: CatalogDetail,
}

/// What a description says beyond the fields every kind shares.
#[derive(Clone, Debug, PartialEq)]
pub enum CatalogDetail {
    /// The skill slugs the agent carries.
    Agent { skills: Vec<String> },
    /// The names of the catalog agents that carry the skill, in catalog order.
    Skill { carried_by: Vec<String> },
    /// The members' names, in the team's order.
    Team { members: Vec<String> },
    /// The roster's names, in the channel's order.
    Channel { roster: Vec<String> },
    /// The declared input names, the step names in definition order, and
    /// what its runs begin on — each start step's event, in definition order
    /// ([`starts_on`]).
    Workflow {
        inputs: Vec<String>,
        steps: Vec<String>,
        starts_on: Vec<String>,
    },
    /// The auth scheme's word, the hosts it may reach, and its operations as
    /// `(id, description, writes)` in definition order.
    Connector {
        auth: String,
        hosts: Vec<String>,
        operations: Vec<(String, String, bool)>,
    },
    /// The version, the permissions it declares as words, and the window it
    /// opens at as `(width, height)`.
    Addon {
        version: String,
        permissions: Vec<String>,
        window: (u32, u32),
    },
}

impl Catalog {
    /// Describe every entry, kind by kind in [`CatalogKind::ALL`] order and
    /// each kind in catalog order. Pure: reads the bundled text, touches no
    /// workspace. A TOML that does not parse is a bundle bug and is refused
    /// by name, the same refusal an install would give.
    pub fn describe(&self) -> Result<Vec<CatalogDescription>, StoreError> {
        let agents: Vec<(&str, AgentBody)> = self
            .agents
            .iter()
            .map(|(slug, t)| parse_agent(slug, t).map(|a| (*slug, a)))
            .collect::<Result<_, _>>()?;
        let agent_name = |slug: &str| -> String {
            agents
                .iter()
                .find(|(s, _)| *s == slug)
                .map(|(_, a)| a.name.clone())
                .unwrap_or_else(|| slug.to_string())
        };
        let names = |slugs: &[String]| slugs.iter().map(|s| agent_name(s)).collect::<Vec<_>>();
        let tags = |raw: &[String]| bundle_tags(raw).iter().cloned().collect::<Vec<_>>();
        let mut out = Vec::new();
        for kind in CatalogKind::ALL.iter().copied() {
            for (slug, toml_str) in self.entries(kind) {
                let (name, description, raw_tags, detail) = match kind {
                    CatalogKind::Agent => {
                        let a = parse_agent(slug, toml_str)?;
                        (
                            a.name,
                            a.description.unwrap_or_default(),
                            a.tags,
                            CatalogDetail::Agent { skills: a.skills },
                        )
                    }
                    CatalogKind::Skill => {
                        let s = parse_skill(slug, toml_str)?;
                        let carried_by = agents
                            .iter()
                            .filter(|(_, a)| a.skills.iter().any(|k| k == slug))
                            .map(|(_, a)| a.name.clone())
                            .collect();
                        (
                            s.name,
                            s.description,
                            s.tags,
                            CatalogDetail::Skill { carried_by },
                        )
                    }
                    CatalogKind::Team => {
                        let t = parse_team(slug, toml_str)?;
                        (
                            t.name,
                            t.purpose.unwrap_or_default(),
                            t.tags,
                            CatalogDetail::Team {
                                members: names(&t.agents),
                            },
                        )
                    }
                    CatalogKind::Channel => {
                        let c = parse_channel(slug, toml_str)?;
                        (
                            c.name,
                            c.topic.unwrap_or_default(),
                            c.tags,
                            CatalogDetail::Channel {
                                roster: names(&c.agents),
                            },
                        )
                    }
                    CatalogKind::Workflow => {
                        let placeholder = WorkflowId::from_ulid(ulid::Ulid::nil());
                        let w = parse_workflow(slug, toml_str, &|_| Some(placeholder))?;
                        let detail = CatalogDetail::Workflow {
                            inputs: w.inputs.iter().map(|i| i.name.to_string()).collect(),
                            steps: w.steps.iter().map(|s| s.name.clone()).collect(),
                            starts_on: starts_on(&w).into_iter().map(str::to_string).collect(),
                        };
                        (w.name, w.description, w.tags, detail)
                    }
                    CatalogKind::Connector => {
                        let c = parse_connector(slug, toml_str)?;
                        let detail = CatalogDetail::Connector {
                            auth: c.auth.word().to_string(),
                            hosts: c.hosts.clone(),
                            operations: c
                                .operations
                                .iter()
                                .map(|o| (o.id.to_string(), o.description.clone(), o.writes))
                                .collect(),
                        };
                        (c.name, c.description, c.tags, detail)
                    }
                    CatalogKind::Addon => {
                        let m = crate::addons::parse_addon(slug, toml_str)?;
                        let detail = CatalogDetail::Addon {
                            version: m.version.clone(),
                            permissions: m
                                .permissions
                                .iter()
                                .map(|p| p.word().to_string())
                                .collect(),
                            window: (m.window.width, m.window.height),
                        };
                        (m.name, m.description, m.tags, detail)
                    }
                };
                out.push(CatalogDescription {
                    kind,
                    slug: slug.to_string(),
                    name,
                    description,
                    tags: tags(&raw_tags),
                    detail,
                });
            }
        }
        Ok(out)
    }
}

/// What a template's runs begin on, in definition order: the event of each
/// `start` step — `manual` for the one a person runs by hand — or `manual`
/// alone for a template that names no start (it begins by hand at its root).
/// Structural, so the catalog page and the bundle test cannot disagree.
pub(crate) fn starts_on(body: &WorkflowBody) -> Vec<&'static str> {
    let named: Vec<&'static str> = body
        .steps
        .iter()
        .filter_map(Step::start_on)
        .map(bisa_core::StartOn::as_str)
        .collect();
    if named.is_empty() {
        vec![bisa_core::StartOn::Manual.as_str()]
    } else {
        named
    }
}

// --- the TOML shapes -------------------------------------------------------

#[derive(Deserialize)]
struct SkillFile {
    skill: SkillBody,
}

#[derive(Deserialize)]
struct SkillBody {
    name: String,
    description: String,
    #[serde(default)]
    tags: Vec<String>,
    content: String,
}

#[derive(Deserialize)]
struct AgentFile {
    agent: AgentBody,
}

/// The agent shape, shared with [`crate::core_agents`] — the file of a core
/// agent that holds a record is this minus its `skills` and `tags`, so it
/// parses here and a second parser would only be a second place for the two to
/// drift. The Decision-Making Agent holds none, and its file is a shape of its
/// own ([`crate::decision_making_agent`]).
///
/// A key the shape does not name is refused rather than dropped: a plan's
/// effort spelled `effort`, or a model's written above the first
/// `[[agent.models]]`, would otherwise parse and say nothing.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AgentBody {
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) description: Option<String>,
    pub(crate) system_prompt: String,
    #[serde(default = "default_harness")]
    pub(crate) harness: String,
    /// `strategy = "fallback" | "weighted" | "round_robin" | "least_busy" |
    /// "auto_route"`.
    #[serde(default)]
    pub(crate) model_strategy: ModelStrategy,
    /// The plan's effort — `model_effort = "auto" | "minimal" | … | "max"` —
    /// beside `model_strategy`, above the first `[[agent.models]]`. Its name
    /// is not a model's `effort` on purpose: a key written after the last
    /// `[[agent.models]]` belongs to that model. A built-in agent states
    /// none, and runs at the `agents.effort` setting.
    #[serde(default)]
    pub(crate) model_effort: Option<EffortChoice>,
    /// Repeated `[[agent.models]]` tables: `model`, optional `weight`,
    /// optional `enabled`, optional `suited_for`, optional `effort`.
    #[serde(default)]
    pub(crate) models: Vec<ModelEntry>,
    #[serde(default)]
    pub(crate) tags: Vec<String>,
    /// Skill slugs, which are the skill ids.
    #[serde(default)]
    pub(crate) skills: Vec<String>,
    #[serde(default)]
    pub(crate) respond: bisa_core::RespondPolicy,
}

impl AgentBody {
    /// The creation record this definition becomes. Provenance is not in it:
    /// the caller states the origin, because a TOML cannot claim one.
    pub(crate) fn into_new_agent(self) -> NewAgent {
        NewAgent {
            name: self.name,
            photo: None,
            description: self.description,
            system_prompt: self.system_prompt,
            harness: self.harness,
            models: model_plan(self.model_strategy, self.model_effort, self.models),
            // A slug the bundle test has already checked; one that fails here
            // would be a bundle bug, and dropping it is the honest fallback.
            skills: self
                .skills
                .iter()
                .filter_map(|s| SkillId::new(s).ok())
                .collect(),
            mcps: vec![],
            tags: bundle_tags(&self.tags),
            respond: self.respond,
            decision_making: false,
        }
    }
}

/// One `[[agent.models]]` entry. A key it does not name is refused: the
/// plan's `model_effort` written after the last entry lands here.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ModelEntry {
    model: String,
    #[serde(default = "one")]
    weight: u32,
    #[serde(default = "yes")]
    enabled: bool,
    /// What the model is the right one for — read by an `auto_route` plan.
    #[serde(default)]
    suited_for: Option<String>,
    /// How hard this model works, when it says so itself.
    #[serde(default)]
    effort: Option<EffortChoice>,
}

#[derive(Deserialize)]
struct TeamFile {
    team: TeamBody,
}

#[derive(Deserialize)]
struct TeamBody {
    name: String,
    #[serde(default)]
    purpose: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    /// Agent slugs, which are the agent ids.
    #[serde(default)]
    agents: Vec<String>,
}

#[derive(Deserialize)]
struct ChannelFile {
    channel: ChannelBody,
}

#[derive(Deserialize)]
struct ChannelBody {
    name: String,
    #[serde(default)]
    topic: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    /// Agent slugs for the roster, which are the agent ids.
    #[serde(default)]
    agents: Vec<String>,
}

#[derive(Deserialize)]
struct WorkflowFile {
    workflow: WorkflowBody,
}

/// A workflow template: the core's own `InputDef` and `Step` shapes under a
/// `[workflow]` header, so there is one parser for a definition wherever it
/// comes from. The one thing a template says that a definition cannot is a
/// `spawn` step's `workflow = "<slug>"`; [`parse_workflow`] resolves it.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkflowBody {
    pub(crate) name: String,
    pub(crate) description: String,
    #[serde(default)]
    pub(crate) tags: Vec<String>,
    #[serde(default)]
    pub(crate) inputs: Vec<InputDef>,
    pub(crate) steps: Vec<Step>,
}

impl WorkflowBody {
    pub(crate) fn into_new_workflow(self) -> NewWorkflow {
        NewWorkflow {
            name: self.name,
            description: self.description,
            inputs: self.inputs,
            steps: self.steps,
            tags: bundle_tags(&self.tags),
            decision_making: false,
        }
    }

    /// The catalog agent slugs this template's steps name by a fixed reference.
    pub(crate) fn agent_slugs(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for step in &self.steps {
            for r in step.kind.assignee_refs() {
                if let ValueRef::Fixed(Assignee::Agent(id)) = r {
                    if !out.contains(id) {
                        out.push(id.clone());
                    }
                }
            }
        }
        out
    }
}

/// The catalog slugs a template's `spawn` steps name, before resolution.
fn spawn_slugs(toml_str: &str) -> Vec<String> {
    let Ok(value) = toml_str.parse::<toml::Value>() else {
        return vec![];
    };
    let mut out = Vec::new();
    if let Some(steps) = value
        .get("workflow")
        .and_then(|w| w.get("steps"))
        .and_then(|s| s.as_array())
    {
        for step in steps {
            if step.get("kind").and_then(|k| k.as_str()) != Some("spawn") {
                continue;
            }
            if let Some(slug) = step.get("workflow").and_then(|w| w.as_str()) {
                if slug.parse::<WorkflowId>().is_err() && !out.contains(&slug.to_string()) {
                    out.push(slug.to_string());
                }
            }
        }
    }
    out
}

/// Parse one workflow template, resolving each `spawn` step's slug through
/// `resolve` to the id it is installed under. A slug that does not resolve is
/// a refusal: the install order guarantees the referenced template landed
/// first, so this is a bundle bug, not a user's.
pub(crate) fn parse_workflow(
    slug: &str,
    toml_str: &str,
    resolve: &dyn Fn(&str) -> Option<WorkflowId>,
) -> Result<WorkflowBody, StoreError> {
    let value = toml_str.parse::<toml::Value>().map_err(|e| {
        StoreError::Invalid(bisa_core::text!(
            "error-store-invalid-catalog-workflow",
            slug = slug.to_string(),
            e = e.to_string()
        ))
    })?;
    parse_workflow_value(&format!("catalog workflow {slug}"), value, resolve)
}

/// Parse a catalog-shaped TOML document — the definition under a
/// `[workflow]` header, or the bare definition — resolving `spawn` slugs
/// through `resolve`. What the CLI's `workflow new --from x.toml` reads, so a
/// template copied out of the catalog and edited by hand reads the same way
/// the installer reads it, misspelled keys refused and all.
pub fn parse_catalog_shaped(
    label: &str,
    toml_str: &str,
    resolve: &dyn Fn(&str) -> Option<WorkflowId>,
) -> Result<NewWorkflow, StoreError> {
    let mut value = toml_str.parse::<toml::Value>().map_err(|e| {
        StoreError::Invalid(bisa_core::text!(
            "error-store-invalid-refused",
            label = label.to_string(),
            e = e.to_string()
        ))
    })?;
    if value.get("workflow").is_none() {
        let mut wrapped = toml::value::Table::new();
        wrapped.insert("workflow".into(), value);
        value = toml::Value::Table(wrapped);
    }
    Ok(parse_workflow_value(label, value, resolve)?.into_new_workflow())
}

fn parse_workflow_value(
    label: &str,
    mut value: toml::Value,
    resolve: &dyn Fn(&str) -> Option<WorkflowId>,
) -> Result<WorkflowBody, StoreError> {
    if let Some(steps) = value
        .get_mut("workflow")
        .and_then(|w| w.get_mut("steps"))
        .and_then(|s| s.as_array_mut())
    {
        for step in steps.iter_mut() {
            if step.get("kind").and_then(|k| k.as_str()) != Some("spawn") {
                continue;
            }
            let Some(named) = step
                .get("workflow")
                .and_then(|w| w.as_str())
                .map(str::to_string)
            else {
                continue;
            };
            if named.parse::<WorkflowId>().is_ok() {
                continue;
            }
            let id = resolve(&named).ok_or_else(|| {
                StoreError::Invalid(bisa_core::text!(
                    "error-store-invalid-spawn-step-names-which-not-installed",
                    label = label.to_string(),
                    named = format!("{named:?}")
                ))
            })?;
            if let Some(table) = step.as_table_mut() {
                table.insert("workflow".into(), toml::Value::String(id.to_string()));
            }
        }
    }
    let parsed: WorkflowFile = value.try_into().map_err(|e| {
        StoreError::Invalid(bisa_core::text!(
            "error-store-invalid-refused",
            label = label.to_string(),
            e = e.to_string()
        ))
    })?;
    Ok(parsed.workflow)
}

fn one() -> u32 {
    1
}

fn yes() -> bool {
    true
}

fn default_harness() -> String {
    "claude-code".into()
}

/// Catalog tags come from a fixed vocabulary and are validated at build time
/// by `bundle_tests`, so a rejection here would mean the bundle and the test
/// disagree. Sanitizing keeps an install running rather than failing it over a
/// tag; the test is the real gate.
pub(crate) fn bundle_tags(raw: &[String]) -> Tags {
    Tags::sanitize(raw)
}

pub(crate) fn model_plan(
    strategy: ModelStrategy,
    effort: Option<EffortChoice>,
    models: Vec<ModelEntry>,
) -> ModelPlan {
    ModelPlan {
        strategy,
        effort,
        models: models
            .into_iter()
            .map(|m| ModelChoice {
                model: m.model,
                weight: m.weight,
                enabled: m.enabled,
                suited_for: m.suited_for,
                effort: m.effort,
            })
            .collect(),
    }
}

/// Parse one agent definition. [`crate::core_agents`] uses it too, which is why
/// it takes the text rather than reading the catalog.
pub(crate) fn parse_agent(slug: &str, toml_str: &str) -> Result<AgentBody, StoreError> {
    let parsed: AgentFile = toml::from_str(toml_str).map_err(|e| {
        StoreError::Invalid(bisa_core::text!(
            "error-store-invalid-catalog-agent",
            slug = slug.to_string(),
            e = e.to_string()
        ))
    })?;
    Ok(parsed.agent)
}

#[derive(Deserialize)]
struct ConnectorFile {
    connector: ConnectorBody,
}

/// A connector definition as its TOML spells it: every [`Connector`] field
/// but the three the catalog stamps — the slug is the id, the origin is the
/// catalog's, the time is the install's.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectorBody {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub base_url: String,
    pub hosts: Vec<String>,
    #[serde(default)]
    pub insecure_tls: bool,
    pub auth: bisa_core::AuthScheme,
    #[serde(default)]
    pub params: Vec<bisa_core::ParamDef>,
    pub operations: Vec<bisa_core::Operation>,
    #[serde(default)]
    pub check: Option<bisa_core::OperationId>,
    /// The bundle's revision of this definition: bumped when the file
    /// changes, so an installed copy at a lower one is refreshed at start.
    #[serde(default)]
    pub revision: u32,
}

impl ConnectorBody {
    /// The definition as a [`Connector`] would validate it — provenance and
    /// time are placeholders, which the rules never read. The CLI's
    /// `connector new --from` and the bundle tests read it this way.
    pub fn as_connector(&self, slug: &str) -> Result<bisa_core::Connector, StoreError> {
        Ok(bisa_core::Connector {
            id: bisa_core::ConnectorId::new(slug)?,
            name: self.name.clone(),
            description: self.description.clone(),
            tags: bundle_tags(&self.tags),
            origin: Origin::Catalog {
                slug: slug.to_string(),
            },
            base_url: self.base_url.clone(),
            hosts: self.hosts.clone(),
            insecure_tls: self.insecure_tls,
            auth: self.auth.clone(),
            params: self.params.clone(),
            operations: self.operations.clone(),
            check: self.check.clone(),
            revision: self.revision,
            created_at: 0,
        })
    }

    /// The fields a creation takes — what `install` and `connector new` hand
    /// the store.
    pub fn into_new(self, slug: &str) -> Result<crate::connectors::NewConnector, StoreError> {
        Ok(crate::connectors::NewConnector {
            id: bisa_core::ConnectorId::new(slug)?,
            name: self.name,
            description: self.description,
            tags: bundle_tags(&self.tags),
            base_url: self.base_url,
            hosts: self.hosts,
            insecure_tls: self.insecure_tls,
            auth: self.auth,
            params: self.params,
            operations: self.operations,
            check: self.check,
        })
    }
}

/// The credential field that moves when a scheme changes between the two
/// that hold one pasted secret: a `bearer` scheme's `token` and an
/// `api_key` scheme's `api_key` are the same string under another name.
/// Any other move leaves the fields where they are.
fn moved_secret_field(
    old: &bisa_core::AuthScheme,
    new: &bisa_core::AuthScheme,
) -> Option<(bisa_core::SecretField, bisa_core::SecretField)> {
    use bisa_core::{AuthScheme, SecretField};
    match (old, new) {
        (AuthScheme::Bearer, AuthScheme::ApiKey { .. }) => {
            Some((SecretField::Token, SecretField::ApiKey))
        }
        (AuthScheme::ApiKey { .. }, AuthScheme::Bearer) => {
            Some((SecretField::ApiKey, SecretField::Token))
        }
        _ => None,
    }
}

/// Parse one connector definition — the catalog's, or a person's own file
/// handed to `bisa connector new --from`.
pub fn parse_connector(slug: &str, toml_str: &str) -> Result<ConnectorBody, StoreError> {
    let parsed: ConnectorFile = toml::from_str(toml_str).map_err(|e| {
        StoreError::Invalid(bisa_core::text!(
            "error-store-invalid-catalog-connector",
            slug = slug.to_string(),
            e = e.to_string()
        ))
    })?;
    Ok(parsed.connector)
}

fn parse_skill(slug: &str, toml_str: &str) -> Result<SkillBody, StoreError> {
    let parsed: SkillFile = toml::from_str(toml_str).map_err(|e| {
        StoreError::Invalid(bisa_core::text!(
            "error-store-invalid-catalog-skill",
            slug = slug.to_string(),
            e = e.to_string()
        ))
    })?;
    Ok(parsed.skill)
}

fn parse_team(slug: &str, toml_str: &str) -> Result<TeamBody, StoreError> {
    let parsed: TeamFile = toml::from_str(toml_str).map_err(|e| {
        StoreError::Invalid(bisa_core::text!(
            "error-store-invalid-catalog-team",
            slug = slug.to_string(),
            e = e.to_string()
        ))
    })?;
    Ok(parsed.team)
}

fn parse_channel(slug: &str, toml_str: &str) -> Result<ChannelBody, StoreError> {
    let parsed: ChannelFile = toml::from_str(toml_str).map_err(|e| {
        StoreError::Invalid(bisa_core::text!(
            "error-store-invalid-catalog-channel",
            slug = slug.to_string(),
            e = e.to_string()
        ))
    })?;
    Ok(parsed.channel)
}

/// The refusal when an id is already held by something this entry did not
/// install. It names the id and what holds it, because "already exists" leaves
/// the owner to guess whether their own work is about to be overwritten.
fn taken(kind: CatalogKind, slug: &str, holder: &str) -> StoreError {
    StoreError::Invalid(bisa_core::text!(
        "error-store-invalid-cannot-install-workspace-already-has-with-id",
        kind = kind.to_string(),
        slug = format!("{slug:?}"),
        holder = holder.to_string()
    ))
}

/// What holds an id, or `None` when the holder is this very catalog entry and
/// there is nothing to refuse — that is the idempotent case, not a collision.
fn holder(origin: &Origin, slug: &str) -> Option<String> {
    match origin {
        Origin::Catalog { slug: s } if s == slug => None,
        Origin::Catalog { slug: s } => Some(format!("the catalog's {s:?}")),
        Origin::Local => Some("one you created here".to_string()),
    }
}

/// The same question for an agent, which has the one origin the others cannot
/// hold.
fn agent_holder(origin: &AgentOrigin, slug: &str) -> Option<String> {
    match origin {
        AgentOrigin::Catalog { slug: s } if s == slug => None,
        AgentOrigin::Catalog { slug: s } => Some(format!("the catalog's {s:?}")),
        AgentOrigin::Local => Some("one you created here".to_string()),
        AgentOrigin::Core => Some("the platform's own agent".to_string()),
    }
}

fn channel_holder(origin: &ChannelOrigin, slug: &str) -> Option<String> {
    match origin {
        ChannelOrigin::Catalog { slug: s } if s == slug => None,
        ChannelOrigin::Catalog { slug: s } => Some(format!("the catalog's {s:?}")),
        ChannelOrigin::Local => Some("one you created here".to_string()),
        ChannelOrigin::Core => Some("the platform's own channel".to_string()),
    }
}

impl Workspace {
    /// The catalog, filtered to one kind or listed whole.
    pub fn catalog_entries(
        &self,
        kind: Option<CatalogKind>,
    ) -> Result<Vec<CatalogEntry>, StoreError> {
        let mut out = Vec::new();
        for k in CatalogKind::ALL.iter().copied() {
            if kind.is_some_and(|want| want != k) {
                continue;
            }
            for (slug, toml_str) in CATALOG.entries(k) {
                out.push(self.entry(k, slug, toml_str)?);
            }
        }
        Ok(out)
    }

    /// One catalog entry. An unknown slug is an error rather than an empty
    /// answer: a picker asking for a name the build does not ship is a bug in
    /// the caller, not an absence.
    pub fn catalog_entry(&self, kind: CatalogKind, slug: &str) -> Result<CatalogEntry, StoreError> {
        self.entry(kind, slug, CATALOG.find(kind, slug)?)
    }

    fn entry(
        &self,
        kind: CatalogKind,
        slug: &str,
        toml_str: &str,
    ) -> Result<CatalogEntry, StoreError> {
        let mut workflow = None;
        let (name, description, tags, requires, installed) = match kind {
            CatalogKind::Agent => {
                let a = parse_agent(slug, toml_str)?;
                (
                    a.name,
                    a.description.unwrap_or_default(),
                    bundle_tags(&a.tags),
                    a.skills,
                    AgentId::new(slug)
                        .map(|id| self.get_agent(&id).is_ok())
                        .unwrap_or(false),
                )
            }
            CatalogKind::Skill => {
                let s = parse_skill(slug, toml_str)?;
                (
                    s.name,
                    s.description,
                    bundle_tags(&s.tags),
                    vec![],
                    SkillId::new(slug)
                        .map(|id| self.get_skill(&id).is_ok())
                        .unwrap_or(false),
                )
            }
            CatalogKind::Team => {
                let t = parse_team(slug, toml_str)?;
                (
                    t.name,
                    t.purpose.unwrap_or_default(),
                    bundle_tags(&t.tags),
                    t.agents,
                    TeamId::new(slug)
                        .map(|id| self.get_team(&id).is_ok())
                        .unwrap_or(false),
                )
            }
            CatalogKind::Channel => {
                let c = parse_channel(slug, toml_str)?;
                (
                    c.name,
                    c.topic.unwrap_or_default(),
                    bundle_tags(&c.tags),
                    c.agents,
                    ChannelId::new(slug)
                        .map(|id| self.get_channel(&id).is_ok())
                        .unwrap_or(false),
                )
            }
            CatalogKind::Workflow => {
                // Listing resolves nothing: an uninstalled spawn target reads
                // as a placeholder id so the body parses, and `requires` says
                // what an install would bring.
                let placeholder = WorkflowId::from_ulid(ulid::Ulid::nil());
                let w = parse_workflow(slug, toml_str, &|_| Some(placeholder))?;
                let mut requires = w.agent_slugs();
                requires.extend(
                    spawn_slugs(toml_str)
                        .into_iter()
                        .map(|s| format!("workflow:{s}")),
                );
                let installed = self.idx().workflow_id_for_slug(slug)?.is_some();
                let definition = w.into_new_workflow();
                let row = (
                    definition.name.clone(),
                    definition.description.clone(),
                    definition.tags.clone(),
                    requires,
                    installed,
                );
                workflow = Some(definition);
                row
            }
            CatalogKind::Connector => {
                let c = parse_connector(slug, toml_str)?;
                (
                    c.name,
                    c.description,
                    bundle_tags(&c.tags),
                    vec![],
                    bisa_core::ConnectorId::new(slug)
                        .map(|id| self.get_connector(&id).is_ok())
                        .unwrap_or(false),
                )
            }
            CatalogKind::Addon => {
                let m = crate::addons::parse_addon(slug, toml_str)?;
                (
                    m.name,
                    m.description,
                    bundle_tags(&m.tags),
                    vec![],
                    bisa_core::AddonId::new(slug)
                        .map(|id| self.get_addon(&id).is_ok())
                        .unwrap_or(false),
                )
            }
        };
        Ok(CatalogEntry {
            kind,
            slug: slug.to_string(),
            name,
            description,
            tags,
            requires,
            installed,
            workflow,
        })
    }

    /// Install a catalog entry and everything it needs.
    ///
    /// Transitive, idempotent, and never destructive: see the module doc for
    /// what each of those means and why.
    ///
    /// It runs in two passes over one traversal — check every id the install
    /// would take, then create — so a collision six entries deep refuses
    /// before anything is written. A half-finished install that reported
    /// nothing would leave the owner with objects they never asked for and no
    /// record of them.
    /// Read one workflow template as a definition, without installing it —
    /// what the Workflow Agent adapts from. A template whose `spawn` step
    /// names another template that is not installed is refused by name:
    /// the definition cannot be complete until that one is here.
    pub fn catalog_workflow(&self, slug: &str) -> Result<Option<NewWorkflow>, StoreError> {
        let Some((_, toml_str)) = CATALOG.workflows.iter().find(|(s, _)| *s == slug) else {
            return Ok(None);
        };
        let idx = self.idx();
        let resolve = |named: &str| {
            idx.workflow_id_for_slug(named)
                .ok()
                .flatten()
                .and_then(|id| id.parse::<WorkflowId>().ok())
        };
        let body = parse_workflow(slug, toml_str, &resolve)?;
        Ok(Some(body.into_new_workflow()))
    }

    /// The inputs a workflow template declares, read without installing it
    /// and whatever it opens goals on — what a command line reads its
    /// person's words by before the template is here.
    pub fn catalog_workflow_inputs(
        &self,
        slug: &str,
    ) -> Result<Option<Vec<bisa_core::InputDef>>, StoreError> {
        let Some((_, toml_str)) = CATALOG.workflows.iter().find(|(s, _)| *s == slug) else {
            return Ok(None);
        };
        // A `spawn` names a template by its slug; which workflow that is
        // here changes nothing of what this one asks for.
        let any = |_: &str| Some(WorkflowId::from_ulid(ulid::Ulid::nil()));
        let body = parse_workflow(slug, toml_str, &any)?;
        Ok(Some(body.into_new_workflow().inputs))
    }

    /// Read a catalog-shaped TOML definition (see [`parse_catalog_shaped`]),
    /// resolving `spawn` slugs against what this workspace has installed.
    pub fn parse_workflow_toml(
        &self,
        label: &str,
        toml_str: &str,
    ) -> Result<NewWorkflow, StoreError> {
        let idx = self.idx();
        let resolve = |named: &str| {
            idx.workflow_id_for_slug(named)
                .ok()
                .flatten()
                .and_then(|id| id.parse::<WorkflowId>().ok())
        };
        parse_catalog_shaped(label, toml_str, &resolve)
    }

    pub fn install(&self, kind: CatalogKind, slug: &str) -> Result<Installed, StoreError> {
        let mut plan = Vec::new();
        self.plan_install(kind, slug, &mut plan)?;
        for (kind, slug) in &plan {
            self.check_free(*kind, slug)?;
        }
        let mut out = Installed::default();
        for (kind, slug) in &plan {
            self.create_entry(*kind, slug, &mut out)?;
        }
        Ok(out)
    }

    /// Everything the install touches, dependencies first: a skill before the
    /// agent that carries it, an agent before the team or channel that names
    /// it. One traversal feeds both passes, so what is checked and what is
    /// created cannot drift apart.
    fn plan_install(
        &self,
        kind: CatalogKind,
        slug: &str,
        plan: &mut Vec<(CatalogKind, String)>,
    ) -> Result<(), StoreError> {
        if plan.iter().any(|(k, s)| *k == kind && s == slug) {
            return Ok(());
        }
        // Every slug is checked here, so a name the build does not ship fails
        // while this is still only a plan.
        CATALOG.find(kind, slug)?;
        // What an entry's `requires` names: skills for an agent, agents for a
        // team or a channel, agents and other workflows for a workflow. A
        // skill is the leaf and needs nothing.
        let required_kind = match kind {
            CatalogKind::Agent => Some(CatalogKind::Skill),
            CatalogKind::Team | CatalogKind::Channel | CatalogKind::Workflow => {
                Some(CatalogKind::Agent)
            }
            CatalogKind::Skill | CatalogKind::Connector | CatalogKind::Addon => None,
        };
        if let Some(required_kind) = required_kind {
            for required in &self.catalog_entry(kind, slug)?.requires {
                match required.strip_prefix("workflow:") {
                    Some(other) => self.plan_install(CatalogKind::Workflow, other, plan)?,
                    None => self.plan_install(required_kind, required, plan)?,
                }
            }
        }
        plan.push((kind, slug.to_string()));
        Ok(())
    }

    /// Whether this entry may take its id: it is free, or it is already held
    /// by this very entry.
    fn check_free(&self, kind: CatalogKind, slug: &str) -> Result<(), StoreError> {
        let held = match kind {
            CatalogKind::Agent => AgentId::new(slug)
                .ok()
                .and_then(|id| self.get_agent(&id).ok())
                .and_then(|d| agent_holder(&d.origin, slug)),
            CatalogKind::Skill => SkillId::new(slug)
                .ok()
                .and_then(|id| self.get_skill(&id).ok())
                .and_then(|d| holder(&d.origin, slug)),
            CatalogKind::Team => TeamId::new(slug)
                .ok()
                .and_then(|id| self.get_team(&id).ok())
                .and_then(|d| holder(&d.origin, slug)),
            CatalogKind::Channel => ChannelId::new(slug)
                .ok()
                .and_then(|id| self.get_channel(&id).ok())
                .and_then(|d| channel_holder(&d.origin, slug)),
            CatalogKind::Connector => bisa_core::ConnectorId::new(slug)
                .ok()
                .and_then(|id| self.get_connector(&id).ok())
                .and_then(|d| holder(&d.origin, slug)),
            CatalogKind::Addon => bisa_core::AddonId::new(slug)
                .ok()
                .and_then(|id| self.get_addon(&id).ok())
                .and_then(|a| holder(&a.record.origin, slug)),
            // A workflow's id is a ULID the install mints, so a slug can never
            // collide with something a person made: present means this very
            // entry, and that is the idempotent case.
            CatalogKind::Workflow => None,
        };
        match held {
            Some(held) => Err(taken(kind, slug, &held)),
            None => Ok(()),
        }
    }

    /// Bring every connector installed from the catalog to the bundle's
    /// revision: a copy whose `revision` is below the bundle's is rewritten
    /// — its id, provenance and time kept, its accounts and secrets kept —
    /// and, when the scheme moved between `bearer` and `api_key`, each
    /// account's credential moves to the field the new scheme reads. A copy
    /// at the bundle's revision or above, a connector the bundle no longer
    /// ships, and a person's own definition are left as they are. One bad
    /// entry is logged and skipped, never the walk's end. The engine calls
    /// this at start, under its lock; `install` stays creations-only.
    pub fn refresh_catalog_connectors(&self) -> Result<Refreshed, StoreError> {
        self.refresh_catalog_connectors_from(CATALOG.connectors)
    }

    /// The refresh over a bundle given by hand — the one the catalog ships,
    /// or a test's.
    pub fn refresh_catalog_connectors_from(
        &self,
        bundle: &[(&str, &str)],
    ) -> Result<Refreshed, StoreError> {
        let mut out = Refreshed::default();
        for stored in self.list_connectors()? {
            let Origin::Catalog { slug } = &stored.origin else {
                continue;
            };
            let Some((_, toml_str)) = bundle.iter().find(|(s, _)| s == slug) else {
                continue;
            };
            let body = match parse_connector(slug, toml_str) {
                Ok(body) => body,
                Err(e) => {
                    tracing::error!(target: "bisa_store::catalog", %slug, "the bundled connector does not parse; the installed copy stands: {e}");
                    continue;
                }
            };
            if body.revision <= stored.revision {
                continue;
            }
            let fresh = match body.as_connector(slug) {
                Ok(fresh) => bisa_core::Connector {
                    id: stored.id.clone(),
                    origin: stored.origin.clone(),
                    created_at: stored.created_at,
                    ..fresh
                },
                Err(e) => {
                    tracing::error!(target: "bisa_store::catalog", %slug, "the bundled connector is not a definition; the installed copy stands: {e}");
                    continue;
                }
            };
            if let Err(e) = self.update_connector(fresh.clone()) {
                tracing::error!(target: "bisa_store::catalog", %slug, "the installed copy could not be refreshed: {e}");
                continue;
            }
            if let Some((from, to)) = moved_secret_field(&stored.auth, &fresh.auth) {
                for account in self.list_connector_accounts(&stored.id)? {
                    match self.move_connector_secret(&stored.id, account.id, from, to) {
                        Ok(true) => out.accounts_moved += 1,
                        Ok(false) => {}
                        Err(e) => {
                            tracing::error!(target: "bisa_store::catalog", %slug, account = %account.id, "the account's credential could not move to the new scheme's field; set it again: {e}");
                        }
                    }
                }
            }
            tracing::info!(
                target: "bisa_store::catalog",
                %slug,
                from = stored.revision,
                to = body.revision,
                "catalog connector refreshed"
            );
            out.definitions.push(slug.clone());
        }
        Ok(out)
    }

    /// Create one entry, or leave what is already there exactly as it is.
    /// Only a creation is recorded: `Installed` says what changed.
    fn create_entry(
        &self,
        kind: CatalogKind,
        slug: &str,
        out: &mut Installed,
    ) -> Result<(), StoreError> {
        let toml_str = CATALOG.find(kind, slug)?;
        let origin = Origin::Catalog {
            slug: slug.to_string(),
        };
        match kind {
            CatalogKind::Skill => {
                let id = SkillId::new(slug)?;
                if self.get_skill(&id).is_ok() {
                    return Ok(());
                }
                let s = parse_skill(slug, toml_str)?;
                self.create_skill_with_origin(
                    NewSkill {
                        id,
                        name: s.name,
                        description: s.description,
                        tags: bundle_tags(&s.tags),
                        markdown: s.content,
                    },
                    origin,
                )?;
                out.skills.push(slug.to_string());
            }
            CatalogKind::Agent => {
                let id = AgentId::new(slug)?;
                if self.get_agent(&id).is_ok() {
                    return Ok(());
                }
                let a = parse_agent(slug, toml_str)?;
                self.add_agent_with_id(
                    &id,
                    a.into_new_agent(),
                    AgentOrigin::Catalog {
                        slug: slug.to_string(),
                    },
                )?;
                out.agents.push(slug.to_string());
            }
            CatalogKind::Team => {
                let id = TeamId::new(slug)?;
                if self.get_team(&id).is_ok() {
                    return Ok(());
                }
                let t = parse_team(slug, toml_str)?;
                self.create_team_with_id(
                    &id,
                    &t.name,
                    t.purpose.as_deref(),
                    t.agents.iter().cloned().map(Assignee::Agent).collect(),
                    bundle_tags(&t.tags),
                    origin,
                )?;
                out.teams.push(slug.to_string());
            }
            CatalogKind::Channel => {
                let id = ChannelId::new(slug)?;
                if self.get_channel(&id).is_ok() {
                    return Ok(());
                }
                let c = parse_channel(slug, toml_str)?;
                let agents = c
                    .agents
                    .iter()
                    .map(|a| AgentId::new(a).map_err(StoreError::from))
                    .collect::<Result<Vec<_>, _>>()?;
                self.create_channel_with_id(
                    &id,
                    &c.name,
                    c.topic.as_deref(),
                    RosterPolicy::Listed {
                        agents,
                        teams: vec![],
                        humans: vec![],
                    },
                    bundle_tags(&c.tags),
                    ChannelOrigin::Catalog {
                        slug: slug.to_string(),
                    },
                )?;
                out.channels.push(slug.to_string());
            }
            CatalogKind::Workflow => {
                if self.idx().workflow_id_for_slug(slug)?.is_some() {
                    return Ok(());
                }
                let resolve = |other: &str| -> Option<WorkflowId> {
                    self.idx()
                        .workflow_id_for_slug(other)
                        .ok()
                        .flatten()
                        .and_then(|id| id.parse().ok())
                };
                let w = parse_workflow(slug, toml_str, &resolve)?;
                let created = self.create_workflow(
                    w.into_new_workflow(),
                    WorkflowOrigin::Catalog {
                        slug: slug.to_string(),
                    },
                )?;
                out.workflows.push((slug.to_string(), created.id));
            }
            CatalogKind::Connector => {
                let id = bisa_core::ConnectorId::new(slug)?;
                if self.get_connector(&id).is_ok() {
                    return Ok(());
                }
                let c = parse_connector(slug, toml_str)?;
                let revision = c.revision;
                self.create_connector_with_origin(c.into_new(slug)?, origin, revision)?;
                out.connectors.push(slug.to_string());
            }
            CatalogKind::Addon => {
                let id = bisa_core::AddonId::new(slug)?;
                if self.get_addon(&id).is_ok() {
                    return Ok(());
                }
                self.install_addon_from_catalog(slug)?;
                out.addons.push(slug.to_string());
            }
        }
        Ok(())
    }
}

/// The catalog's own contract, checked at `cargo test` rather than at an
/// install. Everything here would otherwise fail in [`Workspace::install`] on
/// a user's machine, which is the worst possible place to discover a typo.
#[cfg(test)]
mod bundle_tests {
    use super::*;
    use bisa_core::tags::VOCABULARY;
    use bisa_core::StepKind;
    use std::collections::HashSet;

    const CORE_AGENT_ID: &str = AgentId::GENERAL;
    const WORKFLOW_AGENT_ID: &str = AgentId::WORKFLOW;

    /// Every built-in connector, parsed and stamped as the catalog would.
    fn parsed_connectors() -> Vec<(&'static str, bisa_core::Connector)> {
        CATALOG
            .connectors
            .iter()
            .map(|(slug, t)| {
                (
                    *slug,
                    parse_connector(slug, t)
                        .unwrap()
                        .as_connector(slug)
                        .unwrap(),
                )
            })
            .collect()
    }

    /// The built-in addons, by folder. Pinned so a folder added or dropped
    /// is noticed here, not on a user's machine.
    #[test]
    fn the_catalog_ships_thirteen_addons() {
        let slugs: Vec<&str> = CATALOG.addons.iter().map(|(s, _)| *s).collect();
        assert_eq!(
            slugs,
            [
                "calculator",
                "clock",
                "cpu",
                "disk",
                "gpu",
                "memory",
                "needs-you",
                "pomodoro",
                "sticky-note",
                "stopwatch",
                "tic-tac-toe",
                "unit-converter",
                "weather",
            ]
        );
        let bundles: Vec<&str> = CATALOG.addon_bundles.iter().map(|a| a.slug).collect();
        assert_eq!(bundles, slugs, "the entries and the bundles are one list");
    }

    /// Every built-in addon installs: its manifest parses under its own slug
    /// with no problems, its bundle keeps every listing rule and carries the
    /// entry page and the library's tag, its tags keep their shape, and an
    /// install lands it enabled with every declared permission granted — and
    /// a second install creates nothing.
    #[test]
    fn every_addon_is_well_formed() {
        use bisa_core::addon::{check_bundle_listing, SDK_FILE_NAME};
        for bundle in CATALOG.addon_bundles {
            let m = crate::addons::parse_addon(bundle.slug, bundle.manifest)
                .unwrap_or_else(|e| panic!("{}: {e}", bundle.slug));
            let problems = m.validate();
            assert!(
                problems.is_empty(),
                "{}: {:?}",
                bundle.slug,
                problems
                    .iter()
                    .map(|p| format!("{:?}: {}", p.field, p.text))
                    .collect::<Vec<_>>()
            );
            assert!(
                !m.tags.is_empty(),
                "{}: an addon files somewhere",
                bundle.slug
            );
            assert_eq!(m.license, "MIT", "{}: the platform's licence", bundle.slug);
            // A built-in is the platform's: it ships inside the binary, so it
            // wears the platform's version, website and repository — the
            // workspace's, read through Cargo's own environment.
            assert_eq!(
                m.version,
                env!("CARGO_PKG_VERSION"),
                "{}: a built-in wears the platform's version",
                bundle.slug
            );
            assert_eq!(
                m.homepage.as_deref(),
                Some(env!("CARGO_PKG_HOMEPAGE")),
                "{}: the platform's website",
                bundle.slug
            );
            assert_eq!(
                m.repo.as_deref(),
                Some(env!("CARGO_PKG_REPOSITORY")),
                "{}: the platform's repository",
                bundle.slug
            );
            assert_eq!(
                m.author.as_deref(),
                Some("Bisa"),
                "{}: the platform's own",
                bundle.slug
            );
            let listing: Vec<(String, u64)> = bundle
                .files
                .iter()
                .map(|(p, b)| (p.to_string(), b.len() as u64))
                .collect();
            check_bundle_listing(&listing, &m.entry)
                .unwrap_or_else(|e| panic!("{}: {e}", bundle.slug));
            let entry = bundle
                .files
                .iter()
                .find(|(p, _)| *p == m.entry)
                .map(|(_, b)| String::from_utf8_lossy(b).into_owned())
                .expect("the entry page");
            assert!(
                entry.contains(&format!("src=\"{SDK_FILE_NAME}\"")),
                "{}: the page loads the library",
                bundle.slug
            );
            assert!(
                !entry.contains("http://") && !entry.contains("https://"),
                "{}: a built-in reaches nothing by URL from its page",
                bundle.slug
            );
        }
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::open_with_keystore(
            dir.path(),
            Box::new(crate::identity::MemoryKeyStore::default()),
        )
        .unwrap();
        let installed = ws.install(CatalogKind::Addon, "clock").unwrap();
        assert_eq!(installed.addons, vec!["clock".to_string()]);
        let clock = ws
            .get_addon(&bisa_core::AddonId::new("clock").unwrap())
            .unwrap();
        assert!(clock.is_active());
        assert_eq!(clock.record.granted, clock.record.manifest.permissions);
        assert_eq!(
            clock.record.origin,
            Origin::Catalog {
                slug: "clock".into()
            }
        );
        assert!(ws
            .get_addon_file(clock.id(), "index.html")
            .unwrap()
            .is_file());
        assert!(ws.install(CatalogKind::Addon, "clock").unwrap().is_empty());
        assert!(
            ws.catalog_entry(CatalogKind::Addon, "clock")
                .unwrap()
                .installed
        );
    }

    /// The built-in pets, by folder: the nine of the pack. Pinned so a folder
    /// added or dropped is noticed here, not on a user's machine.
    #[test]
    fn the_catalog_ships_nine_pets() {
        let slugs: Vec<&str> = CATALOG.pets.iter().map(|p| p.slug).collect();
        assert_eq!(
            slugs,
            [
                "bracket", "buffer", "fathom", "jolt", "kernel", "loop", "lumen", "moonrice",
                "nocturn"
            ]
        );
        assert!(
            crate::pets::builtin_pets()
                .iter()
                .any(|p| p.id == "bisa-pets.midnight-shipping.moonrice"),
            "Moonrice ships: the default when the pet is turned on"
        );
    }

    /// Every built-in pet draws: a valid namespaced id ending in its folder's
    /// name, a name and a description, a tagline, every animation on its
    /// state's row with a duration per frame, and a WebP sheet of the grid's
    /// exact size under the cap.
    #[test]
    fn every_pet_is_well_formed() {
        use bisa_core::{
            is_webp, validate_pet_id, webp_dimensions, PetOrigin, CELL_HEIGHT, CELL_WIDTH,
            MAX_SPRITESHEET_BYTES, SHEET_COLS, SHEET_ROWS, STATES,
        };
        let parsed = crate::pets::builtin_pets();
        assert_eq!(parsed.len(), CATALOG.pets.len(), "every manifest parses");
        let mut seen = HashSet::new();
        for (row, pet) in CATALOG.pets.iter().zip(parsed.iter()) {
            validate_pet_id(&pet.id).unwrap();
            assert!(
                pet.id.ends_with(&format!(".{}", row.slug)),
                "{} ends in its folder's name",
                pet.id
            );
            assert!(seen.insert(pet.id.clone()), "{} is listed once", pet.id);
            assert!(
                !pet.display_name.trim().is_empty() && !pet.description.trim().is_empty(),
                "{}",
                pet.id
            );
            assert_eq!(
                pet.origin,
                PetOrigin::Catalog,
                "{} is stamped as shipped",
                pet.id
            );
            assert_eq!(pet.spritesheet_path, "spritesheet.webp", "{}", pet.id);
            let flavour = pet
                .flavour
                .as_ref()
                .unwrap_or_else(|| panic!("{} says who it is", pet.id));
            assert!(
                flavour
                    .tagline
                    .as_deref()
                    .is_some_and(|t| !t.trim().is_empty()),
                "{} has a tagline",
                pet.id
            );
            assert!(
                flavour.archetype.is_some() && flavour.mood.is_some(),
                "{} has an archetype and a mood",
                pet.id
            );
            pet.validate_animations().unwrap();
            for state in STATES {
                assert!(
                    pet.animations.contains_key(state),
                    "{} animates {state}",
                    pet.id
                );
                assert!(
                    !pet.animations[state].frame_durations_ms.is_empty(),
                    "{} times {state}",
                    pet.id
                );
            }
            assert!(
                is_webp(row.sheet),
                "{}'s sheet is WebP by its bytes",
                pet.id
            );
            assert!(
                (row.sheet.len() as u64) <= MAX_SPRITESHEET_BYTES,
                "{}'s sheet is under the cap",
                pet.id
            );
            assert_eq!(
                webp_dimensions(row.sheet),
                Some((SHEET_COLS * CELL_WIDTH, SHEET_ROWS * CELL_HEIGHT)),
                "{}'s sheet is the grid's size",
                pet.id
            );
        }
    }

    /// The built-in connectors, by slug: one per platform the plan names.
    /// Pinned so a file added or dropped is noticed here, not on a user's
    /// machine.
    #[test]
    fn the_catalog_ships_fifteen_connectors() {
        let slugs: Vec<&str> = CATALOG.connectors.iter().map(|(s, _)| *s).collect();
        assert_eq!(
            slugs,
            [
                "confluence",
                "facebook-pages",
                "gmail",
                "google-calendar",
                "google-drive",
                "instagram",
                "jira",
                "linear",
                "notion",
                "obsidian",
                "slack",
                "tiktok",
                "trello",
                "x",
                "youtube",
            ]
        );
    }

    /// The revision of each built-in, pinned: a changed file bumps its
    /// `revision` on purpose — that is what carries the change to an
    /// installed copy at the next start — and a forgotten bump is noticed
    /// here, not on a user's machine where the fix never lands.
    #[test]
    fn the_catalog_connector_revisions_are_pinned() {
        let revisions: Vec<(&str, u32)> = parsed_connectors()
            .iter()
            .map(|(slug, c)| (*slug, c.revision))
            .collect();
        assert_eq!(
            revisions,
            [
                ("confluence", 1),
                ("facebook-pages", 1),
                ("gmail", 0),
                ("google-calendar", 0),
                ("google-drive", 0),
                ("instagram", 1),
                ("jira", 1),
                ("linear", 1),
                ("notion", 1),
                ("obsidian", 1),
                ("slack", 1),
                ("tiktok", 1),
                ("trello", 0),
                ("x", 1),
                ("youtube", 0),
            ]
        );
    }

    /// Every built-in connector validates by the same rules a custom one
    /// does, carries catalog tags, describes every operation, and names a
    /// `check` operation a person can press — the one request Settings makes
    /// to prove an account works. An optional parameter that reaches a JSON
    /// body is a whole leaf, so the client can leave it out rather than send
    /// it empty; the install carries the bundle's revision.
    #[test]
    fn every_connector_is_well_formed() {
        for (slug, c) in parsed_connectors() {
            let problems = c.validate();
            assert!(
                problems.is_empty(),
                "connector {slug} does not validate:\n{}",
                problems
                    .iter()
                    .map(|p| format!("  {}: {}", p.field.as_deref().unwrap_or("-"), p.text))
                    .collect::<Vec<_>>()
                    .join("\n")
            );
            assert!(!c.name.trim().is_empty(), "connector {slug}: no name");
            assert!(
                !c.description.trim().is_empty(),
                "connector {slug}: no description"
            );
            check_tags(&format!("connector {slug}"), c.tags.as_slice());
            assert!(!c.operations.is_empty(), "connector {slug}: no operations");
            for op in &c.operations {
                assert!(
                    !op.description.trim().is_empty(),
                    "connector {slug}: operation {} has no description",
                    op.id
                );
                for p in &op.params {
                    assert!(
                        !p.doc.trim().is_empty(),
                        "connector {slug}: parameter {} of {} has no doc",
                        p.name,
                        op.id
                    );
                }
            }
            let check = c
                .check
                .as_ref()
                .unwrap_or_else(|| panic!("connector {slug}: no check operation"));
            let op = c.operation(check).unwrap();
            assert!(
                !op.writes,
                "connector {slug}: the check operation {} writes",
                op.id
            );
            // An optional parameter in a JSON body is exactly one leaf — a
            // member the client drops when the parameter is left empty.
            for (site, leaf) in c.templates() {
                let bisa_core::TemplateSite::Body(op_id) = &site else {
                    continue;
                };
                let op = c.operation(op_id).unwrap();
                for (at, _) in leaf.match_indices("{params.") {
                    let rest = &leaf[at + "{params.".len()..];
                    let name = &rest[..rest.find('}').unwrap_or(rest.len())];
                    let optional = op
                        .params
                        .iter()
                        .any(|p| p.name.as_str() == name && !p.required);
                    assert!(
                        !optional || leaf == format!("{{params.{name}}}"),
                        "connector {slug}: optional parameter {name} of {} sits inside the body leaf {leaf:?}; make it a leaf of its own so an empty value is left out",
                        op.id
                    );
                }
            }
            // A catalog install of it lands as the catalog's, and reads back whole.
            let dir = tempfile::tempdir().unwrap();
            let ws = Workspace::open_with_keystore(
                dir.path(),
                Box::new(crate::identity::MemoryKeyStore::default()),
            )
            .unwrap();
            let installed = ws.install(CatalogKind::Connector, slug).unwrap();
            assert_eq!(installed.connectors, vec![slug.to_string()]);
            let stored = ws.get_connector(&c.id).unwrap();
            assert_eq!(
                stored.origin,
                Origin::Catalog {
                    slug: slug.to_string()
                }
            );
            assert_eq!(stored.operations.len(), c.operations.len());
            assert_eq!(
                stored.revision, c.revision,
                "the install carries the bundle's revision"
            );
            assert!(
                ws.install(CatalogKind::Connector, slug).unwrap().is_empty(),
                "idempotent"
            );
            assert!(
                ws.refresh_catalog_connectors()
                    .unwrap()
                    .definitions
                    .is_empty(),
                "a fresh install has nothing to refresh"
            );
            let entry = ws.catalog_entry(CatalogKind::Connector, slug).unwrap();
            assert!(entry.installed);
        }
    }

    /// A fixed id per catalog template, by its place in the table. Every
    /// template gets its own so a `spawn` resolves to the workflow it names
    /// and not, as one shared placeholder would have it, to the template that
    /// holds the step — which the validator reads as a spawn cycle.
    fn template_id(slug: &str) -> Option<WorkflowId> {
        CATALOG
            .workflows
            .iter()
            .position(|(s, _)| *s == slug)
            .map(|i| WorkflowId::from_ulid(ulid::Ulid::from_parts(1, i as u128 + 1)))
    }

    /// Every template parses, with every spawn target resolved to that
    /// template's own id.
    fn parsed_workflows() -> Vec<(&'static str, WorkflowBody)> {
        CATALOG
            .workflows
            .iter()
            .map(|(slug, t)| (*slug, parse_workflow(slug, t, &template_id).unwrap()))
            .collect()
    }

    /// At most this many skills on one catalog agent. Every skill is delivered
    /// into every session the agent runs — as a file for claude-code, as a
    /// prompt appendix everywhere else — so a fat list is a tax on each launch.
    /// Six: four of their own at most, and the two platform skills every
    /// agent carries — Embedded Browser and Drawing.
    const MAX_CATALOG_SKILLS: usize = 6;

    /// The skills every catalog agent carries: the platform's own surfaces,
    /// which every role may need and which double as the permission when
    /// the workspace says *assigned*.
    const PLATFORM_SKILLS: &[&str] = &["embedded-browser", "drawing"];

    fn parsed_skills() -> Vec<(&'static str, SkillBody)> {
        CATALOG
            .skills
            .iter()
            .map(|(slug, t)| (*slug, parse_skill(slug, t).unwrap()))
            .collect()
    }

    fn parsed_agents() -> Vec<(&'static str, AgentBody)> {
        CATALOG
            .agents
            .iter()
            .map(|(slug, t)| (*slug, parse_agent(slug, t).unwrap()))
            .collect()
    }

    fn check_tags(what: &str, tags: &[String]) {
        assert!(
            !tags.is_empty(),
            "{what}: no tags — browsing by category is how a catalog this size \
             stays navigable, so everything files somewhere",
        );
        for tag in tags {
            assert!(
                VOCABULARY.contains(&tag.as_str()),
                "{what}: tag {tag:?} is outside the documented vocabulary {VOCABULARY:?}",
            );
            // A tag that does not survive normalization would be stored under
            // a different string than the one written here, so the documented
            // filter would miss it.
            assert_eq!(
                Tags::new([tag]).unwrap().as_slice(),
                std::slice::from_ref(tag),
                "{what}: tag {tag:?} does not normalize to itself",
            );
        }
    }

    #[test]
    fn every_slug_is_unique_within_its_kind_and_usable_as_an_id() {
        for kind in CatalogKind::ALL.iter().copied() {
            let mut seen = HashSet::new();
            for (slug, _) in CATALOG.entries(kind) {
                assert!(seen.insert(*slug), "duplicate {kind} slug {slug}");
                assert!(
                    bisa_core::validate_slug(slug).is_ok(),
                    "{kind} {slug}: not a usable id",
                );
            }
        }
    }

    /// The slug *is* the id, so a catalog entry called `general-agent` or
    /// `general` would install over a permanent object — which
    /// [`Workspace::install`] refuses, leaving that entry permanently
    /// uninstallable. Catch it here instead.
    #[test]
    fn nothing_in_the_catalog_claims_a_permanent_id() {
        for kind in CatalogKind::ALL.iter().copied() {
            for (slug, _) in CATALOG.entries(kind) {
                assert_ne!(
                    *slug, CORE_AGENT_ID,
                    "catalog {kind} {slug} takes the general agent's id"
                );
                assert_ne!(
                    *slug, WORKFLOW_AGENT_ID,
                    "catalog {kind} {slug} takes the workflow agent's id"
                );
                assert_ne!(
                    *slug,
                    ChannelId::GENERAL,
                    "catalog {kind} {slug} takes the general channel's id"
                );
            }
        }
        assert_eq!(
            CATALOG.channels.len(),
            8,
            "eight rooms to choose; general is ensured, not installed"
        );
    }

    /// The thirteen templates the docs promise, by slug — a template that
    /// disappears from the bundle takes its `include_str!` with it, but one
    /// renamed would slip through without this.
    #[test]
    fn the_catalog_ships_thirteen_workflow_templates() {
        let slugs: Vec<&str> = CATALOG.workflows.iter().map(|(s, _)| *s).collect();
        assert_eq!(
            slugs,
            vec![
                "software-feature",
                "bug-fix",
                "research-report",
                "content-pipeline",
                "incident-response",
                "hiring-loop",
                "event-plan",
                "weekly-review",
                "customer-support-triage",
                "product-launch",
                "decision-record",
                "standing-health-check",
                "mobile-release",
            ]
        );
    }

    /// Every template starts as written, against the staff the catalog ships:
    /// every agent it names is a catalog agent, every spawn target a catalog
    /// workflow, and the graph has no problem of any kind. This is the check
    /// an install would otherwise make on a user's machine.
    #[test]
    fn every_workflow_template_validates_against_the_catalog_staff() {
        let agents: Vec<Assignee> = CATALOG
            .agents
            .iter()
            .map(|(s, _)| Assignee::Agent(s.to_string()))
            .collect();
        let workflows: Vec<WorkflowId> = CATALOG
            .workflows
            .iter()
            .map(|(s, _)| template_id(s).unwrap())
            .collect();
        // The spawn graph as the templates draw it, so the cycle check runs
        // against the real thing: a template spawning one that spawns it back
        // is refused here, and a chain that ends is not.
        let spawns: Vec<(WorkflowId, Vec<WorkflowId>)> = CATALOG
            .workflows
            .iter()
            .map(|(s, t)| {
                (
                    template_id(s).unwrap(),
                    spawn_slugs(t)
                        .iter()
                        .filter_map(|o| template_id(o))
                        .collect(),
                )
            })
            .collect();
        // What each template asks of whoever starts it: a template that
        // opens a goal on another gives it what that one requires.
        let asks: Vec<(WorkflowId, Vec<bisa_core::InputDef>)> = parsed_workflows()
            .into_iter()
            .map(|(slug, body)| (template_id(slug).unwrap(), body.inputs))
            .collect();
        let ctx = bisa_core::ValidationCtx {
            assignees: &agents,
            workflows: &workflows,
            spawns: &spawns,
            asks: &asks,
            checks: &crate::syntax::StoreSyntaxChecks,
            ..Default::default()
        };
        let workflow_slugs: HashSet<&str> = CATALOG.workflows.iter().map(|(s, _)| *s).collect();
        for (slug, body) in parsed_workflows() {
            assert!(!body.name.trim().is_empty(), "workflow {slug}: no name");
            assert!(
                !body.description.trim().is_empty(),
                "workflow {slug}: no description"
            );
            check_tags(&format!("workflow {slug}"), &body.tags);
            let wf = bisa_core::Workflow {
                id: template_id(slug).unwrap(),
                name: body.name.clone(),
                description: body.description.clone(),
                inputs: body.inputs.clone(),
                steps: body.steps.clone(),
                origin: WorkflowOrigin::Catalog {
                    slug: slug.to_string(),
                },
                author: bisa_core::PrincipalId::new("ab".repeat(32)).unwrap(),
                tags: bundle_tags(&body.tags),
                revision: 1,
                archived: None,
                decision_making: false,
                created_at: 0,
            };
            let problems = wf.validate(&ctx);
            assert!(
                problems.is_empty(),
                "workflow {slug} does not validate:\n{}",
                problems
                    .iter()
                    .map(|p| format!("  {:?}: {}", p.kind, p.text))
                    .collect::<Vec<_>>()
                    .join("\n")
            );
            // The topology the run machine relies on, pinned per template: its
            // starts — the manual one first, every way in after it — and
            // exactly these loop edges, the flows the machine lets re-enter a
            // step and never lets hold a join.
            let starts: Vec<&str> = wf.start_steps().iter().map(|s| s.id.as_str()).collect();
            assert_eq!(
                wf.manual_entry().map(|s| s.id.as_str()),
                Some("start"),
                "workflow {slug}: the start a person runs by hand"
            );
            let loops: Vec<(String, String)> = wf
                .loop_edges()
                .into_iter()
                .map(|(from, to)| (from.to_string(), to.to_string()))
                .collect();
            let expect = |from: &str, to: &str| (from.to_string(), to.to_string());
            let (expected_starts, expected_loops): (&[&str], Vec<(String, String)>) = match slug {
                "bug-fix" => (&["start"], vec![expect("ask", "reproduce"), expect("verdict", "fix"), expect("verify", "fix")]),
                "content-pipeline" => (&["start"], vec![expect("decision", "which")]),
                "customer-support-triage" => (&["start", "ticket"], vec![]),
                "decision-record" => (&["start"], vec![]),
                "event-plan" => (&["start"], vec![]),
                "hiring-loop" => (&["start"], vec![expect("route", "interviews")]),
                "incident-response" => (&["start", "run-failed"], vec![expect("held", "triage")]),
                "mobile-release" => (&["start"], vec![expect("tests", "build"), expect("verdict", "build")]),
                "product-launch" => (&["start"], vec![expect("survives", "positioning")]),
                "research-report" => (&["start"], vec![expect("route", "draft"), expect("sound", "survey")]),
                "software-feature" => (&["start"], vec![expect("tests", "implement"), expect("verdict", "implement")]),
                "standing-health-check" => (&["start", "failing"], vec![]),
                "weekly-review" => (&["start", "weekly"], vec![]),
                other => panic!("workflow {other} has no pinned topology in this test; add its starts and loop edges"),
            };
            assert_eq!(starts, expected_starts, "workflow {slug}: the start steps");
            assert_eq!(loops, expected_loops, "workflow {slug}: the loop edges");
            for other in spawn_slugs(CATALOG.find(CatalogKind::Workflow, slug).unwrap()) {
                assert!(
                    workflow_slugs.contains(other.as_str()),
                    "workflow {slug} spawns {other:?}, which the catalog does not ship"
                );
                assert_ne!(other, slug, "workflow {slug} spawns itself");
            }
            // A template cannot know a project's id or a person's key: those
            // are inputs, never fixed values.
            for step in &body.steps {
                if let StepKind::Agent {
                    project: Some(ValueRef::Fixed(_)),
                    ..
                } = &step.kind
                {
                    panic!("workflow {slug}: step {} fixes a project id", step.id);
                }
                for r in step.assignee_refs() {
                    assert!(
                        !matches!(r, ValueRef::Fixed(Assignee::Human(_))),
                        "workflow {slug}: step {} names a person by key",
                        step.id
                    );
                }
            }
        }
    }

    /// Every template runs in the workspace as well as on a goal: none reads
    /// `{goal.statement}` or `{goal.title}`, which a run of the workspace has
    /// no goal to answer. A goal's run is told its goal by the engine, in the
    /// first prompt, so no template spells it again.
    #[test]
    fn every_template_runs_in_the_workspace() {
        let workspace = bisa_core::RunScope::Workspace {
            budget: Default::default(),
        };
        for (slug, body) in parsed_workflows() {
            let wf = bisa_core::Workflow {
                id: template_id(slug).unwrap(),
                name: body.name.clone(),
                description: body.description.clone(),
                inputs: body.inputs.clone(),
                steps: body.steps.clone(),
                origin: WorkflowOrigin::Catalog {
                    slug: slug.to_string(),
                },
                author: bisa_core::PrincipalId::new("ab".repeat(32)).unwrap(),
                tags: bundle_tags(&body.tags),
                revision: 1,
                archived: None,
                decision_making: false,
                created_at: 0,
            };
            let problems = wf.scope_problems(&workspace);
            assert!(
                problems.is_empty(),
                "workflow {slug} cannot run in the workspace:\n{}",
                problems
                    .iter()
                    .map(|p| format!("  {}", p.text))
                    .collect::<Vec<_>>()
                    .join("\n")
            );
        }
    }

    /// Every input a template declares is read by some step — its kind, its
    /// boundary events, a condition, or a start that fills it from the event
    /// or reads it in its own fields. A dead input is a `UnusedInput`
    /// problem, which the validation test above already refuses; this names
    /// the rule on its own so a template author finds it.
    #[test]
    fn every_template_declares_no_unused_input() {
        let inputs_in = |tmpl: &str, grammar: bisa_core::Grammar| -> Vec<String> {
            bisa_core::placeholders_in(tmpl, grammar)
                .unwrap()
                .into_iter()
                .filter_map(|p| match p {
                    bisa_core::Placeholder::Input(name) => Some(name.to_string()),
                    _ => None,
                })
                .collect()
        };
        for (slug, body) in parsed_workflows() {
            let mut read: HashSet<String> = HashSet::new();
            for step in &body.steps {
                for tmpl in step.templates() {
                    read.extend(inputs_in(tmpl, bisa_core::Grammar::Run));
                }
                for (name, _) in step.input_refs() {
                    read.insert(name.to_string());
                }
                for condition in step.kind.conditions() {
                    for name in condition.reads_inputs() {
                        read.insert(name.to_string());
                    }
                }
                if let StepKind::Start { on, inputs, .. } = &step.kind {
                    read.extend(inputs.keys().cloned());
                    for tmpl in on.templates() {
                        read.extend(inputs_in(tmpl, bisa_core::Grammar::StartEvent));
                    }
                }
            }
            for input in &body.inputs {
                assert!(
                    read.contains(input.name.as_str()),
                    "workflow {slug}: input {} is declared but nothing reads it",
                    input.name
                );
            }
        }
    }

    /// A template with a key the definition does not have is refused when it
    /// is parsed — at `cargo test` for the bundle, at `workflow new --from`
    /// for a file — never installed with the key silently dropped.
    #[test]
    fn a_template_with_a_misspelled_key_is_refused() {
        let good = "[workflow]\nname = \"T\"\ndescription = \"d\"\n\n[[workflow.steps]]\nid = \"a\"\nname = \"A\"\nkind = \"end\"\nfinish = \"done\"\n";
        let none = |_: &str| None;
        assert!(parse_workflow("t", good, &none).is_ok());
        let bare = "name = \"T\"\ndescription = \"d\"\n\n[[steps]]\nid = \"a\"\nname = \"A\"\nkind = \"end\"\nfinish = \"done\"\n";
        let parsed = parse_catalog_shaped("file", bare, &none).unwrap();
        assert_eq!(parsed.name, "T");
        assert_eq!(parsed.steps.len(), 1);

        // On the template itself.
        let err = parse_workflow("t", &good.replace("description", "descripton"), &none)
            .unwrap_err()
            .to_string();
        assert!(err.contains("descripton"), "{err}");
        // On a step.
        let err = parse_workflow("t", &good.replace("finish", "finsh"), &none)
            .unwrap_err()
            .to_string();
        assert!(err.contains("finsh"), "{err}");
        // On an input.
        let with_input = format!(
            "{good}\n[[workflow.inputs]]\nname = \"x\"\nlabel = \"X\"\nkind = \"text\"\nrequierd = true\n"
        );
        let err = parse_workflow("t", &with_input, &none)
            .unwrap_err()
            .to_string();
        assert!(err.contains("requierd"), "{err}");
        // The same file through the CLI's reader.
        let err = parse_catalog_shaped("file", &with_input, &none)
            .unwrap_err()
            .to_string();
        assert!(err.starts_with("file:"), "{err}");
    }

    /// Every template begins at an explicit start a person can run by hand,
    /// and says what else it begins on. The four written for standing work
    /// begin on their event too; the rest only by hand. The rule is
    /// [`starts_on`] — the one the catalog page reads — pinned here so a
    /// template that drifts is noticed.
    #[test]
    fn every_template_begins_at_a_start_and_says_what_it_starts_on() {
        for (slug, body) in parsed_workflows() {
            assert!(
                body.steps
                    .iter()
                    .any(|s| matches!(s.kind, StepKind::Start { .. })),
                "workflow {slug}: no explicit start"
            );
            let on = starts_on(&body);
            assert_eq!(
                on.iter().filter(|e| **e == "manual").count(),
                1,
                "workflow {slug}: one start a person runs by hand, got {on:?}"
            );
            let events: Vec<&str> = on.into_iter().filter(|e| *e != "manual").collect();
            let expected: &[&str] = match slug {
                "weekly-review" => &["schedule"],
                "customer-support-triage" => &["hook"],
                "standing-health-check" => &["check"],
                "incident-response" => &["run"],
                _ => &[],
            };
            assert_eq!(events, expected, "workflow {slug}: what it starts on");
        }
    }

    /// The shape a step yields is its `output_schema`, which the executor
    /// appends to every prompt; an instruction that spells the shape again
    /// as JSON is a second copy that drifts — and, undoubled, a placeholder
    /// the grammar refuses. So no agent step writes a brace into its prose.
    #[test]
    fn no_agent_instruction_spells_its_result_shape() {
        for (slug, body) in parsed_workflows() {
            for step in &body.steps {
                let StepKind::Agent {
                    instructions,
                    output_schema,
                    ..
                } = &step.kind
                else {
                    continue;
                };
                assert!(
                    !instructions.contains("{{"),
                    "workflow {slug}: step {} writes its result shape in prose; put it in output_schema",
                    step.id
                );
                assert!(
                    output_schema.is_some(),
                    "workflow {slug}: agent step {} declares no output_schema",
                    step.id
                );
            }
        }
    }

    #[test]
    fn every_skill_is_usable() {
        for (slug, s) in parsed_skills() {
            assert!(!s.name.trim().is_empty(), "skill {slug}: no name");
            assert!(
                !s.description.trim().is_empty(),
                "skill {slug}: no description — that is the line a model reads \
                 to decide whether to open the skill",
            );
            assert!(
                s.description.len() <= 160,
                "skill {slug}: description is {} chars; keep it to one line",
                s.description.len(),
            );
            assert!(
                !s.content.trim().is_empty() && s.content.len() <= crate::skills::MAX_SKILL_BYTES,
                "skill {slug}: body is {} bytes",
                s.content.len(),
            );
            check_tags(&format!("skill {slug}"), &s.tags);
        }
    }

    /// The M5 lesson, kept as a gate: an agent that ships one model is one
    /// quota wall away from stopping, which is exactly what model plans were
    /// built to end.
    #[test]
    fn every_agent_has_a_primary_and_a_fallback() {
        for (slug, a) in parsed_agents() {
            assert_eq!(
                a.model_strategy,
                ModelStrategy::Fallback,
                "agent {slug}: catalog agents stay on the ordered strategy",
            );
            let plan = a.into_new_agent().models;
            let order = plan.order(&bisa_core::AllHealthy, 0, None);
            assert!(
                order.len() >= 2,
                "agent {slug}: needs a fallback model, got {order:?}",
            );
            assert!(
                order.iter().all(|m| !m.trim().is_empty()),
                "agent {slug}: blank model id in {order:?}",
            );
        }
    }

    /// One plan for every built-in agent — the thirty-two here and the two
    /// core agents that hold a record: Opus 5.5, then Sonnet 5.5, both with
    /// the 1M window, and no effort of their own, so one setting is the dial
    /// for all of them.
    #[test]
    fn every_built_in_agent_runs_opus_then_sonnet_and_states_no_effort() {
        let expected = ModelPlan::fallback(["claude-opus-5-5[1m]", "claude-sonnet-5-5[1m]"]);
        assert_eq!(expected.effort, None);
        assert!(expected.models.iter().all(|m| m.effort.is_none()));
        let catalog = parsed_agents();
        assert_eq!(catalog.len(), 32);
        let core: Vec<(&str, AgentBody)> = crate::core_agents::CORE_AGENTS
            .iter()
            .map(|(id, _, t)| (*id, parse_agent(id, t).unwrap()))
            .collect();
        assert_eq!(core.len(), 2);
        for (slug, a) in catalog.into_iter().chain(core) {
            assert_eq!(a.into_new_agent().models, expected, "agent {slug}");
        }
    }

    const STATED: &str = r#"
[agent]
name = "Stated"
system_prompt = "You state your effort."
model_strategy = "fallback"
model_effort = "high"

[[agent.models]]
model = "claude-opus-5-5[1m]"
effort = "max"

[[agent.models]]
model = "claude-sonnet-5-5[1m]"
effort = "auto"

[[agent.models]]
model = "claude-haiku-4-5"
suited_for = "quick edits"
"#;

    #[test]
    fn the_plans_effort_and_a_models_own_are_read_from_the_toml() {
        let plan = parse_agent("stated", STATED)
            .unwrap()
            .into_new_agent()
            .models;
        assert_eq!(
            plan,
            ModelPlan {
                strategy: ModelStrategy::Fallback,
                effort: Some(EffortChoice::High),
                models: vec![
                    ModelChoice::new("claude-opus-5-5[1m]").at(EffortChoice::Max),
                    ModelChoice::new("claude-sonnet-5-5[1m]").at(EffortChoice::Auto),
                    ModelChoice::suited("claude-haiku-4-5", "quick edits"),
                ],
            }
        );
        // What the record is created with carries it too.
        let created = parse_agent("stated", STATED).unwrap().into_new_agent();
        assert_eq!(created.models, plan);
        // Every word of the vocabulary reads, in both places.
        for choice in EffortChoice::ALL {
            let toml_str = format!(
                "[agent]\nname = \"X\"\nsystem_prompt = \"x\"\nmodel_effort = \"{choice}\"\n\n\
                 [[agent.models]]\nmodel = \"m\"\neffort = \"{choice}\"\n"
            );
            let plan = parse_agent("x", &toml_str).unwrap().into_new_agent().models;
            assert_eq!(plan.effort, Some(choice), "{choice}");
            assert_eq!(plan.models[0].effort, Some(choice), "{choice}");
        }
    }

    #[test]
    fn an_effort_in_the_wrong_place_or_under_the_wrong_name_is_refused() {
        let refused = |toml_str: &str| {
            assert!(
                matches!(parse_agent("x", toml_str), Err(StoreError::Invalid(_))),
                "parsed: {toml_str}"
            );
        };
        // The plan's effort written after the last model belongs to that
        // model, which has no such key.
        refused(
            "[agent]\nname = \"X\"\nsystem_prompt = \"x\"\n\n\
             [[agent.models]]\nmodel = \"m\"\nmodel_effort = \"high\"\n",
        );
        // A model's effort written above the first model is the agent's key,
        // and the agent's is `model_effort`.
        refused(
            "[agent]\nname = \"X\"\nsystem_prompt = \"x\"\neffort = \"high\"\n\n\
             [[agent.models]]\nmodel = \"m\"\n",
        );
        // A word that is no effort.
        refused(
            "[agent]\nname = \"X\"\nsystem_prompt = \"x\"\nmodel_effort = \"ultra\"\n\n\
             [[agent.models]]\nmodel = \"m\"\n",
        );
        refused(
            "[agent]\nname = \"X\"\nsystem_prompt = \"x\"\n\n\
             [[agent.models]]\nmodel = \"m\"\neffort = \"x_high\"\n",
        );
        // The same file with each key in its place parses.
        assert!(parse_agent(
            "x",
            "[agent]\nname = \"X\"\nsystem_prompt = \"x\"\nmodel_effort = \"high\"\n\n\
             [[agent.models]]\nmodel = \"m\"\neffort = \"xhigh\"\n",
        )
        .is_ok());
    }

    #[test]
    fn every_agent_is_well_formed_and_its_skills_exist() {
        let skills: HashSet<&str> = CATALOG.skills.iter().map(|(s, _)| *s).collect();
        for (slug, a) in parsed_agents() {
            assert!(!a.name.trim().is_empty(), "agent {slug}: no name");
            assert!(
                a.description.as_ref().is_some_and(|d| !d.trim().is_empty()),
                "agent {slug}: no description",
            );
            assert!(
                !a.system_prompt.trim().is_empty(),
                "agent {slug}: empty system prompt",
            );
            check_tags(&format!("agent {slug}"), &a.tags);
            for platform in PLATFORM_SKILLS {
                assert!(
                    a.skills.iter().any(|s| s == platform),
                    "agent {slug}: every catalog agent carries the {platform} skill",
                );
            }
            assert!(
                a.skills.len() <= MAX_CATALOG_SKILLS,
                "agent {slug}: {} skills; the cap is {MAX_CATALOG_SKILLS} because \
                 every one of them is delivered into every session",
                a.skills.len(),
            );
            for s in &a.skills {
                assert!(
                    skills.contains(s.as_str()),
                    "agent {slug}: names skill {s:?}, which the catalog does not ship",
                );
            }
        }
    }

    #[test]
    fn every_team_and_channel_names_a_catalog_agent() {
        let agents: HashSet<&str> = CATALOG.agents.iter().map(|(s, _)| *s).collect();
        for (slug, toml_str) in CATALOG.teams {
            let t = parse_team(slug, toml_str).unwrap();
            assert!(!t.name.trim().is_empty(), "team {slug}: no name");
            assert!(
                t.purpose.as_ref().is_some_and(|p| !p.trim().is_empty()),
                "team {slug}: no purpose",
            );
            assert!(!t.agents.is_empty(), "team {slug}: no members");
            check_tags(&format!("team {slug}"), &t.tags);
            for a in &t.agents {
                assert!(
                    agents.contains(a.as_str()),
                    "team {slug}: names agent {a:?}, which the catalog does not ship",
                );
            }
        }
        for (slug, toml_str) in CATALOG.channels {
            let c = parse_channel(slug, toml_str).unwrap();
            assert!(!c.name.trim().is_empty(), "channel {slug}: no name");
            assert!(
                c.topic.as_ref().is_some_and(|t| !t.trim().is_empty()),
                "channel {slug}: no topic",
            );
            check_tags(&format!("channel {slug}"), &c.tags);
            for a in &c.agents {
                assert!(
                    agents.contains(a.as_str()),
                    "channel {slug}: rosters agent {a:?}, which the catalog does not ship",
                );
            }
        }
    }

    /// Every shipped skill is reachable by installing something. A skill no
    /// agent carries is a document in a library with no shelf — nothing would
    /// ever pull it in.
    #[test]
    fn no_skill_ships_unused() {
        let mut used: HashSet<String> = HashSet::new();
        for (_, a) in parsed_agents() {
            used.extend(a.skills);
        }
        for (slug, _) in CATALOG.skills {
            assert!(
                used.contains(*slug),
                "skill {slug} is shipped but no catalog agent carries it",
            );
        }
    }

    /// The tools a catalog agent reports, asks and records through. The house
    /// shape closes every prompt by naming the ones that role actually uses,
    /// so an agent arrives knowing how its output leaves the session.
    const PLATFORM_TOOLS: &[&str] = &[
        // Every session.
        "get_goal",
        "ask_human",
        "await_human",
        "ask_human_and_wait",
        "add_note",
        "spawn_sub_goal",
        "emit_signal",
        "post_message",
        "recall_store",
        "recall_get",
        "recall_list",
        "note_read",
        "note_append",
        "review_notes_list",
        "review_note_resolve",
        "create_project",
        // The mobile tools, every session's menu (ide/19).
        "mobile_development_status",
        "mobile_development_devices",
        "mobile_development_boot",
        "mobile_development_screenshot",
        // Work-item sessions.
        "get_run",
        "yield_result",
        "report_progress",
        // Goal (designing) sessions.
        "revise_statement",
        "propose_workflow",
        "amend_workflow",
    ];

    /// The last paragraph of a prompt — the closing tool-wiring block, in the
    /// house shape. Paragraphs are blank-line separated, which is also how the
    /// principles list is separated from what follows it.
    fn closing_paragraph(prompt: &str) -> &str {
        prompt
            .trim_end()
            .rsplit("\n\n")
            .next()
            .expect("rsplit always yields once")
            .trim()
    }

    /// A number as the core agent's prompt spells it. The range stops where
    /// the catalog plausibly stops: a count past it fails loudly here rather
    /// than passing because the phrase happened not to match.
    fn spelled(n: usize) -> String {
        const ONES: [&str; 20] = [
            "zero",
            "one",
            "two",
            "three",
            "four",
            "five",
            "six",
            "seven",
            "eight",
            "nine",
            "ten",
            "eleven",
            "twelve",
            "thirteen",
            "fourteen",
            "fifteen",
            "sixteen",
            "seventeen",
            "eighteen",
            "nineteen",
        ];
        const TENS: [&str; 10] = [
            "", "", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety",
        ];
        match n {
            0..=19 => ONES[n].to_string(),
            20..=99 if n % 10 == 0 => TENS[n / 10].to_string(),
            20..=99 => format!("{}-{}", TENS[n / 10], ONES[n % 10]),
            _ => panic!("the catalog has grown past what this test can spell out: {n}"),
        }
    }

    /// The core agent's prompt tells a model how big the catalog is, and that
    /// number decides how much staff it installs. Documentation does not get
    /// to run ahead of the code — least of all documentation a model acts on.
    /// The sentence had already drifted once by the time this test was
    /// written.
    #[test]
    fn the_core_agents_prompt_states_the_catalog_size_it_actually_ships() {
        let prompt = parse_agent(CORE_AGENT_ID, crate::core_agents::GENERAL_AGENT_TOML)
            .unwrap()
            .system_prompt;
        // The prompt is hard-wrapped, so the sentence is compared against the
        // text reflowed onto one line rather than against a wrapping that a
        // reword would break for no reason.
        let flowed = prompt.split_whitespace().collect::<Vec<_>>().join(" ");
        let want = format!(
            "The catalog has {} agents and {} teams",
            spelled(CATALOG.agents.len()),
            spelled(CATALOG.teams.len()),
        );
        assert!(
            flowed.contains(&want),
            "the core agent's prompt should say {want:?}; it ships \
             {} agents and {} teams",
            CATALOG.agents.len(),
            CATALOG.teams.len(),
        );
    }

    /// The same rule for the workflow agent: the number of templates it is
    /// told about decides whether it adapts one or invents a shape.
    #[test]
    fn the_workflow_agents_prompt_states_the_template_count_it_actually_ships() {
        let prompt = parse_agent(WORKFLOW_AGENT_ID, crate::core_agents::WORKFLOW_AGENT_TOML)
            .unwrap()
            .system_prompt;
        let flowed = prompt.split_whitespace().collect::<Vec<_>>().join(" ");
        let want = format!(
            "The catalog has {} workflow templates",
            spelled(CATALOG.workflows.len()),
        );
        assert!(
            flowed.contains(&want),
            "the workflow agent's prompt should say {want:?}; it ships {} templates",
            CATALOG.workflows.len(),
        );
    }

    /// An agent in no team and no channel is installable and then alone: it
    /// takes no pooled work item and sits in no room, so the only way to reach
    /// it is to name it directly, every time. `project-manager` shipped that
    /// way for a whole milestone.
    #[test]
    fn every_agent_has_a_home_in_a_team_or_a_channel() {
        let mut homed: HashSet<String> = HashSet::new();
        for (slug, toml_str) in CATALOG.teams {
            homed.extend(parse_team(slug, toml_str).unwrap().agents);
        }
        for (slug, toml_str) in CATALOG.channels {
            homed.extend(parse_channel(slug, toml_str).unwrap().agents);
        }
        for (slug, _) in CATALOG.agents {
            assert!(
                homed.contains(*slug),
                "agent {slug} is in no team and no channel: installable, then \
                 orphaned",
            );
        }
    }

    /// The tools the General Agent and the Workflow Agent share, which only their prompts may name.
    const CORE_SHARED_TOOLS: &[&str] = &["workspace_overview", "list_staff", "list_catalog"];

    /// The general agent's own ops, which only its prompt may name.
    ///
    /// Separate from [`PLATFORM_TOOLS`] because they are separate at run time:
    /// the MCP server offers these to a session whose agent is the general
    /// agent and to nobody else, so a catalog agent naming one would be
    /// describing a tool it will never be handed.
    const GENERAL_AGENT_TOOLS: &[&str] = &["install_catalog_entry", "assign", "capture_goal"];

    /// The workflow agent's own ops, on the same terms.
    const WORKFLOW_AGENT_TOOLS: &[&str] = &[
        "list_workflow_templates",
        "get_workflow",
        "validate_workflow",
        "save_workflow",
        "list_connectors",
    ];

    /// Words that look like a tool and are not.
    ///
    /// Short by construction — a prompt is prose, and the only `snake_case` in
    /// it should be something the agent can call. `system_prompt` and
    /// `owner_only` reach here from the TOML around the text; `output_schema`
    /// and `max_visits` are the workflow fields the Workflow Agent is taught.
    const NOT_TOOLS: &[&str] = &[
        "system_prompt",
        "owner_only",
        "work_item",
        "base_hash",
        "output_schema",
        "max_visits",
        "for_each",
        "max_iterations",
    ];

    /// No prompt names a tool that does not exist — and no core agent names
    /// the other's tools.
    ///
    /// This is the check the milestone that removed the Brief needed and did
    /// not have. Three shipped agents instructed `brief_get` in as many words;
    /// the closing-paragraph test below passed the whole time, because naming
    /// *one* real tool was enough and nothing looked at the rest. An agent
    /// told to call a tool it will not be handed does not fail loudly — it
    /// improvises, which is the failure the tool contract exists to prevent.
    ///
    /// Every `snake_case` token in a prompt is either a tool or one of the few
    /// words in [`NOT_TOOLS`]; there is no third kind, because prose does not
    /// otherwise contain underscores.
    #[test]
    fn no_prompt_names_a_tool_that_does_not_exist() {
        // Every session is handed the browser tools (ide/18) and the drawing
        // tools (19) too; the core's tables are the lists.
        let catalog_known: Vec<&str> = PLATFORM_TOOLS
            .iter()
            .chain(bisa_core::browser::BROWSER_TOOLS)
            .chain(bisa_core::draw::DRAW_TOOLS)
            .chain(NOT_TOOLS)
            .copied()
            .collect();
        for (slug, a) in parsed_agents() {
            for token in snake_case_tokens(&a.system_prompt) {
                assert!(
                    catalog_known.contains(&token.as_str()),
                    "agent {slug}: the prompt names {token:?}, which is not a \
                     tool any session is handed",
                );
            }
        }
        let general_known: Vec<&str> = catalog_known
            .iter()
            .chain(CORE_SHARED_TOOLS)
            .chain(GENERAL_AGENT_TOOLS)
            .copied()
            .collect();
        for token in snake_case_tokens(crate::core_agents::GENERAL_AGENT_TOML) {
            assert!(
                general_known.contains(&token.as_str()),
                "the general agent names {token:?}, which it is not handed",
            );
        }
        let workflow_known: Vec<&str> = catalog_known
            .iter()
            .chain(CORE_SHARED_TOOLS)
            .chain(WORKFLOW_AGENT_TOOLS)
            .copied()
            .collect();
        for token in snake_case_tokens(crate::core_agents::WORKFLOW_AGENT_TOML) {
            assert!(
                workflow_known.contains(&token.as_str()),
                "the workflow agent names {token:?}, which it is not handed",
            );
        }
    }

    /// Every `foo_bar` in a string. Deliberately not a parser: a prompt is
    /// prose, so an underscore inside a word is the whole signal.
    fn snake_case_tokens(text: &str) -> Vec<String> {
        let mut out = Vec::new();
        for word in text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')) {
            let word = word.trim_matches('_');
            if word.contains('_')
                && word.starts_with(|c: char| c.is_ascii_lowercase())
                && word
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
            {
                out.push(word.to_string());
            }
        }
        out
    }

    /// Every prompt closes by naming the tools that role works through. An
    /// implicit tool contract is one the agent invents at run time, which is
    /// how a result ends up in prose instead of in `yield_result`.
    ///
    /// The check is structural rather than a match on wording: the last
    /// paragraph must be a paragraph — not another principle — and must name
    /// at least one platform tool. What it says about them is a matter for
    /// reading the file. That every tool it names is *real* is the separate
    /// assertion above, which this one used not to make.
    #[test]
    fn every_agent_prompt_ends_by_naming_the_tools_it_works_through() {
        for (slug, a) in parsed_agents() {
            let close = closing_paragraph(&a.system_prompt);
            assert!(
                !close.starts_with("- "),
                "agent {slug}: the prompt ends on a principle, so its tool \
                 contract is left implicit",
            );
            assert!(
                PLATFORM_TOOLS.iter().any(|t| close.contains(t)),
                "agent {slug}: the closing paragraph names no platform tool: \
                 {close:?}",
            );
        }
    }

    /// Six principles, each one specific to the role. The count is the part a
    /// test can hold: a prompt that grows a seventh bullet is usually one that
    /// stopped choosing, and one that drops to four is a role that was never
    /// thought through. Whether each principle names that role's own
    /// characteristic failure is a reading, not an assertion.
    #[test]
    fn every_agent_states_six_principles() {
        for (slug, a) in parsed_agents() {
            let principles = a
                .system_prompt
                .lines()
                .filter(|l| l.starts_with("- "))
                .count();
            assert_eq!(
                principles, 6,
                "agent {slug}: {principles} principles; the house shape is six",
            );
        }
    }

    /// The vocabulary is a closed set, and a word in it that nothing carries
    /// is a filter that returns nothing — worse than an absent category,
    /// because the catalog offers it. Adding a word means using it.
    #[test]
    fn every_word_in_the_vocabulary_is_carried_by_something_in_the_catalog() {
        let mut used: HashSet<String> = HashSet::new();
        for (_, a) in parsed_agents() {
            used.extend(a.tags);
        }
        for (_, s) in parsed_skills() {
            used.extend(s.tags);
        }
        for (slug, toml_str) in CATALOG.teams {
            used.extend(parse_team(slug, toml_str).unwrap().tags);
        }
        for (slug, toml_str) in CATALOG.channels {
            used.extend(parse_channel(slug, toml_str).unwrap().tags);
        }
        for (_, w) in parsed_workflows() {
            used.extend(w.tags);
        }
        for word in VOCABULARY {
            assert!(
                used.contains(*word),
                "the vocabulary offers {word:?} and nothing in the catalog \
                 carries it, so filtering by it lands on an empty page",
            );
        }
    }

    /// `CatalogKind` is written into an id-shaped position in the API and read
    /// back out of one, so the two directions must agree.
    #[test]
    fn catalog_kinds_round_trip_through_their_names() {
        for kind in CatalogKind::ALL.iter().copied() {
            assert_eq!(kind.as_str().parse::<CatalogKind>().unwrap(), kind);
        }
        assert!("harness".parse::<CatalogKind>().is_err());
    }
}
