//! Settings: a registry of typed keys, each declaring which scopes may hold a
//! value, resolved `Project → Workspace → Machine → default`.
//!
//! The registry is data. The panel, the resolution, the write path and the
//! reference page are four readers of one list and cannot disagree. A value
//! written to a scope its key does not allow is **refused**, not stored and
//! ignored.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Where a value may live. Ordered from the most specific to the least.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    /// `projects/<slug>/settings.json` — syncs; belongs to the code.
    Project,
    /// `settings.json` — syncs; belongs to the workspace.
    Workspace,
    /// `machine.json` — never syncs; belongs to this machine.
    Machine,
}

impl Scope {
    /// Resolution order: the first scope holding a value wins.
    pub const RESOLUTION: [Scope; 3] = [Scope::Project, Scope::Workspace, Scope::Machine];

    pub fn as_str(self) -> &'static str {
        match self {
            Scope::Project => "project",
            Scope::Workspace => "workspace",
            Scope::Machine => "machine",
        }
    }
}

impl std::str::FromStr for Scope {
    type Err = crate::CoreError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Scope::RESOLUTION
            .into_iter()
            .find(|k| k.as_str() == s)
            .ok_or_else(|| crate::CoreError::UnknownScope(s.to_string()))
    }
}

/// Which scopes a key may be held at. A small set, spelled out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScopeSet {
    pub machine: bool,
    pub workspace: bool,
    pub project: bool,
}

impl ScopeSet {
    pub const M: ScopeSet = ScopeSet {
        machine: true,
        workspace: false,
        project: false,
    };
    pub const W: ScopeSet = ScopeSet {
        machine: false,
        workspace: true,
        project: false,
    };
    pub const MW: ScopeSet = ScopeSet {
        machine: true,
        workspace: true,
        project: false,
    };
    pub const WP: ScopeSet = ScopeSet {
        machine: false,
        workspace: true,
        project: true,
    };
    pub const MWP: ScopeSet = ScopeSet {
        machine: true,
        workspace: true,
        project: true,
    };

    pub fn allows(&self, scope: Scope) -> bool {
        match scope {
            Scope::Machine => self.machine,
            Scope::Workspace => self.workspace,
            Scope::Project => self.project,
        }
    }

    pub fn list(&self) -> Vec<Scope> {
        Scope::RESOLUTION
            .into_iter()
            .filter(|s| self.allows(*s))
            .collect()
    }
}

/// The value a setting holds. JSON-shaped and small.
pub type Value = serde_json::Value;

/// What kind of value a key takes, and its bounds.
#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Bool,
    Integer {
        min: i64,
        max: i64,
    },
    Number {
        min: f64,
        max: f64,
    },
    Text,
    Choice(&'static [&'static str]),
    /// A JSON object or array the panel renders as a dedicated editor.
    Structured,
}

/// One setting, declared once.
#[derive(Clone, Debug)]
pub struct SettingDef {
    pub key: &'static str,
    pub kind: Kind,
    pub default: Value,
    pub scopes: ScopeSet,
}

impl SettingDef {
    /// The id of this key's message in the catalog — `setting-<key>` with
    /// the dots as dashes; its value is the label, `.help` the sentence
    /// under it, `.choice-<value>` the word for each value of a `Choice`
    /// (`locales/<lang>/settings.ftl`, [17 — Internationalisation]). The
    /// registry says what a key *is*; what it is *called* is the catalog's,
    /// rendered by whoever shows it — the node, the CLI, the docs generator.
    pub fn message_id(&self) -> String {
        format!("setting-{}", self.key.replace('.', "-"))
    }

    /// The label as a sentence-as-data, for `bisa-i18n` to render.
    pub fn text(&self) -> crate::text::Text {
        crate::text::Text::new(self.message_id())
    }

    /// The registry entry for `key`, if any.
    pub fn lookup(key: &str) -> Option<&'static SettingDef> {
        REGISTRY.iter().find(|d| d.key == key)
    }

    /// The group a key files under: the part before the first dot.
    pub fn group(&self) -> &'static str {
        self.key.split('.').next().unwrap_or(self.key)
    }

    /// May a value for this key be held at `scope`?
    pub fn check_scope(&self, scope: Scope) -> Result<(), SettingsError> {
        if self.scopes.allows(scope) {
            Ok(())
        } else {
            Err(SettingsError::ScopeNotAllowed {
                key: self.key.to_string(),
                scope,
                allowed: self.scopes.list(),
            })
        }
    }

    /// Is `value` the right shape and within bounds?
    pub fn check_value(&self, value: &Value) -> Result<(), SettingsError> {
        let bad = |why: &str| SettingsError::InvalidValue {
            key: self.key.to_string(),
            why: why.to_string(),
        };
        match &self.kind {
            Kind::Bool => value
                .as_bool()
                .map(|_| ())
                .ok_or_else(|| bad("expected true or false")),
            Kind::Integer { min, max } => {
                let n = value.as_i64().ok_or_else(|| bad("expected an integer"))?;
                if n < *min || n > *max {
                    return Err(bad(&format!("expected {min}..={max}")));
                }
                Ok(())
            }
            Kind::Number { min, max } => {
                let n = value.as_f64().ok_or_else(|| bad("expected a number"))?;
                if n < *min || n > *max {
                    return Err(bad(&format!("expected {min}..={max}")));
                }
                Ok(())
            }
            Kind::Text => value
                .as_str()
                .map(|_| ())
                .ok_or_else(|| bad("expected a string")),
            Kind::Choice(choices) => {
                let s = value.as_str().ok_or_else(|| bad("expected a string"))?;
                if choices.contains(&s) {
                    Ok(())
                } else {
                    Err(bad(&format!("expected one of {}", choices.join(", "))))
                }
            }
            Kind::Structured => {
                if value.is_object() || value.is_array() {
                    Ok(())
                } else {
                    Err(bad("expected an object or an array"))
                }
            }
        }
    }
}

/// Where a resolved value came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(rename = "SettingOrigin")]
pub enum Origin {
    Default,
    Machine,
    Workspace,
    Project,
}

impl From<Scope> for Origin {
    fn from(s: Scope) -> Self {
        match s {
            Scope::Machine => Origin::Machine,
            Scope::Workspace => Origin::Workspace,
            Scope::Project => Origin::Project,
        }
    }
}

/// A resolved setting: its value and the scope it came from.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Resolved {
    pub key: String,
    pub value: Value,
    pub origin: Origin,
}

/// The values held at one scope.
pub type Layer = BTreeMap<String, Value>;

/// Resolve one key: `Project → Workspace → Machine → default`, walking only the
/// scopes the key allows. A value that somehow sits at a disallowed scope is
/// ignored, so a hand-edited file cannot override what the registry forbids.
pub fn resolve(def: &SettingDef, layers: &[(Scope, &Layer)]) -> Resolved {
    for scope in Scope::RESOLUTION {
        if !def.scopes.allows(scope) {
            continue;
        }
        if let Some((_, layer)) = layers.iter().find(|(s, _)| *s == scope) {
            if let Some(v) = layer.get(def.key) {
                if def.check_value(v).is_ok() {
                    return Resolved {
                        key: def.key.to_string(),
                        value: v.clone(),
                        origin: scope.into(),
                    };
                }
            }
        }
    }
    Resolved {
        key: def.key.to_string(),
        value: def.default.clone(),
        origin: Origin::Default,
    }
}

/// Resolve every key.
pub fn resolve_all(layers: &[(Scope, &Layer)]) -> Vec<Resolved> {
    REGISTRY.iter().map(|d| resolve(d, layers)).collect()
}

/// Check a write of `key = value` at `scope`: the key exists, the scope is
/// allowed, the value is well-formed.
pub fn check_write(
    key: &str,
    scope: Scope,
    value: &Value,
) -> Result<&'static SettingDef, SettingsError> {
    let def = SettingDef::lookup(key).ok_or_else(|| SettingsError::UnknownKey(key.to_string()))?;
    def.check_scope(scope)?;
    def.check_value(value)?;
    // A `network.*` value has rules beyond its kind — a proxy is a URL, a
    // bypass entry never a port — stated beside the struct that reads them.
    crate::network_settings::check_write(key, value)?;
    crate::collab_settings::check_write(key, value)?;
    Ok(def)
}

#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum SettingsError {
    #[error("unknown setting `{0}`")]
    UnknownKey(String),
    #[error("`{key}` cannot be set at {scope:?} scope; allowed: {allowed:?}")]
    ScopeNotAllowed {
        key: String,
        scope: Scope,
        allowed: Vec<Scope>,
    },
    #[error("invalid value for `{key}`: {why}")]
    InvalidValue { key: String, why: String },
}

use serde_json::json;

/// One registry entry. Its words — the label, the help, a choice's word —
/// are not here: they are the message `setting-<key>` in
/// `locales/<lang>/settings.ftl`, held to this list by
/// `crates/bisa-i18n/tests/it/catalog.rs`.
macro_rules! def {
    ($key:literal, $kind:expr, $default:expr, $scopes:expr) => {
        SettingDef {
            key: $key,
            kind: $kind,
            default: $default,
            scopes: $scopes,
        }
    };
}

/// Every setting the platform has. See `docs/architecture/ide/13-settings.md`.
pub static REGISTRY: std::sync::LazyLock<Vec<SettingDef>> = std::sync::LazyLock::new(|| {
    use Kind::*;
    use ScopeSet as S;
    vec![
        // --- appearance ---
        def!(
            "appearance.theme",
            Choice(&[
                "system",
                "glass",
                "glass-dark",
                "harbor",
                "harbor-dark",
                "orchard",
                "orchard-dark",
                "dune",
                "dune-dark",
                "suede",
                "suede-dark"
            ]),
            json!("system"),
            S::M
        ),
        def!(
            "appearance.accent",
            Choice(&["amber", "violet", "teal", "rose"]),
            json!("amber"),
            S::M
        ),
        def!(
            "appearance.density",
            Choice(&["comfortable", "compact"]),
            json!("comfortable"),
            S::M
        ),
        def!(
            "appearance.type_scale",
            Number { min: 0.9, max: 1.5 },
            json!(1.0),
            S::M
        ),
        def!(
            "appearance.font_ui",
            Choice(&["system", "inter", "atkinson"]),
            json!("system"),
            S::M
        ),
        def!(
            "appearance.font_mono",
            Choice(&["system", "jetbrains", "plex"]),
            json!("system"),
            S::M
        ),
        // The language the app speaks — this machine's, since it is this
        // machine's person who reads. `system` follows the machine's
        // languages, in order, to the first the platform ships; English is
        // the one shipped today, and a translation is a folder under
        // `locales/` (17 — Internationalisation).
        def!(
            "appearance.language",
            Choice(&["system", "en"]),
            json!("system"),
            S::M
        ),
        // --- ide ---
        def!(
            "ide.default_mode",
            Choice(&["project", "agent", "board"]),
            json!("project"),
            S::MW
        ),
        // --- editor ---
        def!("editor.theme", Text, json!("follow_app"), S::M),
        def!("editor.font_family", Text, json!(""), S::M),
        def!(
            "editor.font_size",
            Integer { min: 8, max: 32 },
            json!(14),
            S::M
        ),
        def!(
            "editor.line_height",
            Number { min: 1.0, max: 2.5 },
            json!(1.5),
            S::M
        ),
        def!(
            "editor.tab_size",
            Integer { min: 1, max: 8 },
            json!(4),
            S::WP
        ),
        def!("editor.insert_spaces", Bool, json!(true), S::WP),
        def!(
            "editor.word_wrap",
            Choice(&["off", "on", "bounded"]),
            json!("off"),
            S::MWP
        ),
        def!(
            "editor.autosave.mode",
            Choice(&["off", "after_delay", "on_focus_change"]),
            json!("after_delay"),
            S::MW
        ),
        def!(
            "editor.autosave.delay_ms",
            Integer {
                min: 300,
                max: 10000
            },
            json!(1000),
            S::MW
        ),
        def!("editor.format_on_save", Bool, json!(false), S::WP),
        def!("editor.minimap", Bool, json!(false), S::M),
        def!(
            "editor.large_file.editable_mib",
            Integer { min: 1, max: 8 },
            json!(2),
            S::M
        ),
        def!(
            "editor.large_file.refuse_mib",
            Integer { min: 8, max: 64 },
            json!(20),
            S::M
        ),
        def!("editor.delete.trash", Bool, json!(true), S::M),
        // --- terminal ---
        def!("terminal.shell", Text, json!("login"), S::M),
        def!("terminal.font_family", Text, json!(""), S::M),
        def!(
            "terminal.font_size",
            Integer { min: 8, max: 32 },
            json!(14),
            S::M
        ),
        def!(
            "terminal.scrollback_lines",
            Integer {
                min: 1000,
                max: 100000
            },
            json!(5000),
            S::M
        ),
        def!("terminal.default_harness", Text, json!("shell"), S::WP),
        def!("terminal.restore_scrollback", Bool, json!(true), S::M),
        def!("harness.usage.reads", Bool, json!(true), S::M),
        def!("terminal.status_reporting", Bool, json!(true), S::M),
        // Whether a resumed harness is nudged into motion is the person's
        // habit at this machine: machine scope, never synced.
        def!("terminal.resume_start", Bool, json!(true), S::M),
        // Whether closing asks first is a fact about the person at this
        // machine, like the grants below: machine scope, never synced.
        def!("terminal.confirm_close", Bool, json!(true), S::M),
        def!("terminal.confirm_terminate", Bool, json!(true), S::M),
        def!(
            "terminal.cursor_style",
            Choice(&["block", "bar", "underline"]),
            json!("block"),
            S::M
        ),
        // --- browser ---
        // The embedded browser is the desktop's; whether this machine shows
        // one is its own fact. Who may drive it and how far is the
        // workspace's, per project where a project says so.
        def!("browser.enabled", Bool, json!(true), S::M),
        def!("browser.home", Text, json!(""), S::WP),
        def!("browser.remember_tabs", Bool, json!(true), S::M),
        def!(
            "browser.agents",
            Choice(&["everyone", "assigned", "nobody"]),
            json!("everyone"),
            S::WP
        ),
        def!(
            "browser.agents.reach",
            Choice(&["anywhere", "local_only"]),
            json!("anywhere"),
            S::WP
        ),
        def!(
            "browser.agents.headless",
            Choice(&["unattended", "always", "never"]),
            json!("unattended"),
            S::WP
        ),
        def!(
            "browser.agents.scripts",
            Choice(&["allow", "refuse"]),
            json!("allow"),
            S::WP
        ),
        def!(
            "browser.screenshot.width",
            Integer {
                min: 320,
                max: 4096
            },
            json!(1280),
            S::MW
        ),
        // --- draw ---
        // The canvas is the desktop's; whether this machine shows one is its
        // own fact. Who may draw into it is the workspace's, per project
        // where a project says so; a snapshot's width is a machine's or the
        // workspace's word.
        def!("draw.enabled", Bool, json!(true), S::M),
        def!(
            "draw.agents",
            Choice(&["everyone", "assigned", "nobody"]),
            json!("everyone"),
            S::WP
        ),
        def!(
            "draw.snapshot.width",
            Integer {
                min: 320,
                max: 4096
            },
            json!(1280),
            S::MW
        ),
        // --- events ---
        // The listeners' runtime is this machine's: whether it hears events
        // here, how often it looks, how long a check start may take, how far
        // a files scan may walk, whether a start may be called from outside,
        // how often a pull request's state is asked of its code host. How
        // deep a chain of events may go, how fast a start may begin runs and
        // how many occurrences may wait per listener are the workspace's
        // rules — a runaway is everyone's problem.
        def!("events.enabled", Bool, json!(true), S::M),
        def!(
            "events.tick_secs",
            Integer { min: 5, max: 3600 },
            json!(15),
            S::M
        ),
        def!(
            "events.check_timeout_secs",
            Integer { min: 5, max: 3600 },
            json!(60),
            S::M
        ),
        def!(
            "events.files.max_depth",
            Integer { min: 1, max: 64 },
            json!(8),
            S::M
        ),
        def!(
            "events.files.max_entries",
            Integer {
                min: 100,
                max: 100_000
            },
            json!(5_000),
            S::M
        ),
        def!("events.public_hooks", Bool, json!(false), S::M),
        def!(
            "events.pr_poll_secs",
            Integer {
                min: 60,
                max: 86_400
            },
            json!(300),
            S::M
        ),
        def!(
            "events.chain_depth",
            Integer { min: 1, max: 10 },
            json!(3),
            S::W
        ),
        def!(
            "events.fires_per_minute",
            Integer { min: 1, max: 600 },
            json!(30),
            S::W
        ),
        def!(
            "events.backlog_per_listener",
            Integer { min: 1, max: 100 },
            json!(5),
            S::W
        ),
        // --- mobile ---
        // Mobile development is the machine's: whether this one develops
        // for phones, for which platforms, and where its Flutter and Android
        // SDK are, are facts about this Mac and never sync. Who may drive
        // the devices is the workspace's, per project where a project says so.
        def!("mobile_development.enabled", Bool, json!(false), S::M),
        def!(
            "mobile_development.platforms",
            Choice(&["both", "ios", "android"]),
            json!("both"),
            S::M
        ),
        def!(
            "mobile_development.agents",
            Choice(&["everyone", "assigned", "nobody"]),
            json!("everyone"),
            S::WP
        ),
        def!("mobile_development.flutter.path", Text, json!(""), S::M),
        def!("mobile_development.android.sdk", Text, json!(""), S::M),
        // --- git ---
        def!("git.default_branch", Text, json!(""), S::WP),
        def!(
            "git.merge_strategy",
            Choice(&["merge", "squash", "rebase"]),
            json!("merge"),
            S::WP
        ),
        def!(
            "git.pull",
            Choice(&["ff_only", "rebase", "merge"]),
            json!("ff_only"),
            S::WP
        ),
        def!("git.delete_branch_after_merge", Bool, json!(true), S::WP),
        def!(
            "git.committer",
            Choice(&["inherit", "pin", "ask"]),
            json!("inherit"),
            S::W
        ),
        def!(
            "git.fetch_interval_secs",
            Integer { min: 0, max: 3600 },
            json!(300),
            S::W
        ),
        // --- workstreams ---
        def!(
            "workstreams.cleanup",
            Choice(&["keep", "ask", "remove_when_merged"]),
            json!("ask"),
            S::WP
        ),
        def!(
            "workstreams.after_merge",
            Choice(&["ask", "return_and_pull", "stay"]),
            json!("ask"),
            S::WP
        ),
        def!(
            "workstreams.dirty_close",
            Choice(&["refuse", "confirm_with_recovery"]),
            json!("confirm_with_recovery"),
            S::W
        ),
        // The workstream scripts (ide/07 §Workstream scripts): shell run by the
        // engine around a checkout's life — before it is created, once it
        // exists, before it is deleted. Empty is no script. Set from Git ›
        // Repository in the IDE, which also approves them on this machine.
        def!("workstreams.script.pre_create", Text, json!(""), S::WP),
        def!("workstreams.script.post_create", Text, json!(""), S::WP),
        def!("workstreams.script.clean", Text, json!(""), S::WP),
        // The run command is not a lifecycle script: the engine never runs
        // it. The IDE's Terminal menu opens it in a terminal (ide/18), once
        // this machine has approved it like the three above.
        def!("workstreams.script.run", Text, json!(""), S::WP),
        def!(
            "workstreams.script.timeout_secs",
            Integer { min: 1, max: 3600 },
            json!(300),
            S::WP
        ),
        def!("workstreams.script.trusted", Structured, json!([]), S::M),
        def!("workstreams.board.enabled", Bool, json!(true), S::MW),
        def!(
            "workstreams.board.due_soon_days",
            Integer { min: 1, max: 30 },
            json!(3),
            S::MW
        ),
        def!(
            "workstreams.board.wip_limit",
            Integer { min: 0, max: 50 },
            json!(0),
            S::MW
        ),
        def!("workstreams.board.show_archived", Bool, json!(false), S::MW),
        // --- security ---
        def!("security.redactor.enabled", Bool, json!(true), S::MW),
        def!("security.redactor.rules", Structured, json!([]), S::MW),
        def!(
            "security.redactor.builtins_off",
            Structured,
            json!([]),
            S::MW
        ),
        def!("security.redactor.env_auto", Bool, json!(true), S::M),
        def!("security.guard.enabled", Bool, json!(true), S::MW),
        def!("security.guard.rules", Structured, json!([]), S::MW),
        def!("security.guard.builtins_off", Structured, json!([]), S::MW),
        def!("security.guard.terminal_hooks", Bool, json!(true), S::M),
        def!("security.classifier.enabled", Bool, json!(true), S::MW),
        def!(
            "security.classifier.agent",
            Text,
            json!("general-agent"),
            S::MW
        ),
        def!(
            "security.classifier.deadline_secs",
            Integer { min: 5, max: 120 },
            json!(20),
            S::MW
        ),
        def!(
            "security.classifier.on_harmful",
            Choice(&["ask", "deny"]),
            json!("ask"),
            S::MW
        ),
        def!(
            "security.classifier.provider",
            Choice(&["agent", "harness", "decision_making_agent"]),
            json!("agent"),
            S::MW
        ),
        def!(
            "security.classifier.harness",
            Text,
            json!("claude-code"),
            S::MW
        ),
        def!(
            "security.classifier.model",
            Text,
            json!("claude-sonnet-5-5[1m]"),
            S::MW
        ),
        // How hard the classifier's model works when a harness reads for it:
        // a level, never `auto` — a verdict is not what the judge is asked
        // about.
        def!(
            "security.classifier.effort",
            Choice(&["minimal", "low", "medium", "high", "xhigh", "max"]),
            json!("high"),
            S::MW
        ),
        // What an agent reads from outside — a page through the platform's
        // browser, a review from a code host — is read by the classifier before
        // the agent may (11-security §What an agent reads from outside).
        def!("security.content.screen", Bool, json!(true), S::MW),
        def!(
            "security.content.on_harmful",
            Choice(&["ask", "deny"]),
            json!("ask"),
            S::MW
        ),
        def!("security.net.deny_hosts", Structured, json!([]), S::MW),
        def!("security.net.allow_hosts", Structured, json!([]), S::MW),
        // People on other nodes write here as humans (14-collaboration): what
        // they say is read by the classifier before an agent may hear it, and
        // an agent they wake runs with its hands put to the owner.
        def!("security.collaboration.classify", Bool, json!(true), S::MW),
        def!(
            "security.collaboration.agent_tools",
            Choice(&["ask", "as_owner"]),
            json!("ask"),
            S::MW
        ),
        // An MCP server a person installs on an agent reaches the machine and
        // the outside world through the harness. A judged harness (Claude
        // Code, ACP) puts each of its tools to the guard; an observed one
        // asks nobody the platform can hear, so the server is kept off it
        // unless the person says otherwise.
        def!(
            "security.mcp.observed",
            Choice(&["refuse", "allow"]),
            json!("refuse"),
            S::MW
        ),
        // --- decisions ---
        def!("decisions.enabled", Bool, json!(false), S::MWP),
        def!("decisions.points_off", Structured, json!([]), S::MWP),
        def!(
            "decisions.provider",
            Choice(&["harness", "agent", "jev", "rlcd"]),
            json!("harness"),
            S::MW
        ),
        def!("decisions.harness.id", Text, json!("claude-code"), S::MW),
        def!(
            "decisions.harness.model",
            Text,
            json!("claude-sonnet-5-5[1m]"),
            S::MW
        ),
        // How hard the judge's own model works: a level, never `auto` — the
        // session the judge runs in asks nobody how hard to work.
        def!(
            "decisions.harness.effort",
            Choice(&["minimal", "low", "medium", "high", "xhigh", "max"]),
            json!("high"),
            S::MW
        ),
        def!("decisions.agent.id", Text, json!("general-agent"), S::MW),
        def!("decisions.jev.model", Text, json!("jev-latest"), S::MW),
        def!("decisions.rlcd.endpoint", Text, json!(""), S::MW),
        def!("decisions.rlcd.model", Text, json!(""), S::MW),
        def!(
            "decisions.rlcd.auth",
            Choice(&["none", "bearer"]),
            json!("bearer"),
            S::MW
        ),
        def!(
            "decisions.deadline_secs",
            Integer { min: 5, max: 120 },
            json!(20),
            S::MW
        ),
        def!(
            "decisions.retries",
            Integer { min: 0, max: 5 },
            json!(2),
            S::MW
        ),
        def!(
            "decisions.confidence.act",
            Number { min: 0.0, max: 1.0 },
            json!(0.7),
            S::MWP
        ),
        def!(
            "decisions.confidence.security",
            Number { min: 0.5, max: 1.0 },
            json!(0.9),
            S::MW
        ),
        // --- connectors ---
        def!(
            "connectors.concurrency",
            Integer { min: 1, max: 32 },
            json!(4),
            S::M
        ),
        def!(
            "connectors.oauth.port",
            Integer {
                min: 1024,
                max: 65535
            },
            json!(4478),
            S::M
        ),
        // --- diagrams ---
        def!(
            "diagrams.theme",
            Choice(&["follow_app", "default", "dark", "forest", "neutral"]),
            json!("follow_app"),
            S::MW
        ),
        def!(
            "diagrams.export.scale",
            Integer { min: 1, max: 4 },
            json!(2),
            S::M
        ),
        // --- artifacts ---
        def!("artifacts.html.libraries", Bool, json!(true), S::MW),
        // --- agents ---
        def!("agents.default", Text, json!("general-agent"), S::WP),
        // How hard a model works when nobody said: the last link of the
        // chain, after a step's pin, the model's own and the agent's plan.
        // `auto` hands the level to the Decision-Making Agent, task by task.
        def!(
            "agents.effort",
            Choice(&["auto", "minimal", "low", "medium", "high", "xhigh", "max"]),
            json!("high"),
            S::WP
        ),
        def!(
            "agents.conversation.mode",
            Choice(&["manual", "auto", "plan"]),
            json!("manual"),
            S::MWP
        ),
        def!(
            "agents.review.checkpoints",
            Integer { min: 1, max: 200 },
            json!(20),
            S::MW
        ),
        def!(
            "agents.context.selection_max_lines",
            Integer { min: 20, max: 2000 },
            json!(400),
            S::W
        ),
        def!(
            "agents.context.terminal_lines",
            Integer { min: 20, max: 2000 },
            json!(200),
            S::W
        ),
        // --- goals ---
        def!(
            "goals.default_mode",
            Choice(&["auto", "guided", "manual"]),
            json!("auto"),
            S::MW
        ),
        def!(
            "goals.auto.repair_limit",
            Integer { min: 0, max: 20 },
            json!(3),
            S::MW
        ),
        // Above a step's ceiling in an auto goal: the classifier reads what
        // no rule decided, where a guided or manual goal asks its person.
        // The rules and the redactor come first as ever, and the classifier
        // never allows what a rule refused.
        def!(
            "goals.auto.permissions",
            Choice(&["classify", "ask"]),
            json!("classify"),
            S::MW
        ),
        // The ceiling an auto goal's steps run under. `exec`: a step that may
        // change files may also run commands and the MCP tools that act, so
        // an unattended run is not one a person answers *Allow Bash?* for —
        // the guard's rules first as ever, a `read` step read-only still;
        // `step`: every step's own ceiling stands, and what is above it is
        // `goals.auto.permissions`'s to settle — the earlier behaviour.
        def!(
            "goals.auto.ceiling",
            Choice(&["exec", "step"]),
            json!("exec"),
            S::MW
        ),
        // --- budget ---
        // The ceiling work that runs on its own — a goal, a run in the
        // workspace a schedule, a check or a project start begins daily for a
        // year — spends against before it stops. A goal or a run of the
        // workspace made without a budget of its own takes this one (the
        // budget a workflow listens with wins); zero is no ceiling. The
        // ledger tracks spend whether or not a ceiling is set.
        def!(
            "budget.default.max_usd_cents",
            Integer {
                min: 0,
                max: 1_000_000_000
            },
            json!(0),
            S::MW
        ),
        def!(
            "budget.default.max_tokens",
            Integer {
                min: 0,
                max: 1_000_000_000_000
            },
            json!(0),
            S::MW
        ),
        def!(
            "budget.default.max_wall_clock_secs",
            Integer {
                min: 0,
                max: 315_360_000
            },
            json!(0),
            S::MW
        ),
        // --- keymap ---
        def!(
            "keymap.preset",
            Choice(&["default", "vscode", "vim"]),
            json!("default"),
            S::M
        ),
        def!("keymap.overrides", Structured, json!({}), S::M),
        // --- lsp ---
        def!("lsp.enabled", Bool, json!(true), S::MW),
        def!("lsp.servers", Structured, json!([]), S::M),
        def!(
            "lsp.idle_ttl_secs",
            Integer { min: 30, max: 3600 },
            json!(300),
            S::M
        ),
        // --- workflow ---
        def!("workflow.designer.snap", Bool, json!(true), S::M),
        def!(
            "workflow.designer.grid",
            Integer { min: 8, max: 32 },
            json!(16),
            S::M
        ),
        def!("workflow.designer.minimap", Bool, json!(false), S::M),
        def!(
            "workflow.autosave.delay_ms",
            Integer {
                min: 300,
                max: 10_000
            },
            json!(800),
            S::MW
        ),
        // A workflow a schedule starts daily for a year would grow the
        // workspace without end: a run of the workspace is a folder. The
        // newest finished runs of a workflow are kept up to this many; the
        // oldest beyond it go — folder and rows — when a run of it ends. A
        // goal's runs are the goal's and are never counted. Never below one:
        // the run that just ended is always there to be read.
        def!(
            "workflow.runs.keep",
            Integer {
                min: 1,
                max: 100_000
            },
            json!(500),
            S::MW
        ),
        // --- cache ---
        // Performance knobs for the in-process caches, all machine
        // scope: caching is a property of this machine's load, not of a project.
        // A TTL of zero disables that cache; `cache.enabled = false` disables all
        // of them at once. See `CacheSettings`.
        def!("cache.enabled", Bool, json!(true), S::M),
        def!(
            "cache.harness_listing.ttl_ms",
            Integer {
                min: 0,
                max: 3_600_000
            },
            json!(30_000),
            S::M
        ),
        def!(
            "cache.harness_models.ttl_ms",
            Integer {
                min: 0,
                max: 86_400_000
            },
            json!(300_000),
            S::M
        ),
        def!(
            "cache.git_status.ttl_ms",
            Integer {
                min: 0,
                max: 600_000
            },
            json!(2_000),
            S::M
        ),
        def!(
            "cache.path_index.ttl_ms",
            Integer {
                min: 0,
                max: 600_000
            },
            json!(5_000),
            S::M
        ),
        def!(
            "cache.path_index.max_roots",
            Integer { min: 1, max: 128 },
            json!(8),
            S::M
        ),
        def!(
            "cache.codehost.ttl_ms",
            Integer {
                min: 0,
                max: 600_000
            },
            json!(10_000),
            S::M
        ),
        def!(
            "cache.codehost.max_entries",
            Integer { min: 1, max: 4_096 },
            json!(256),
            S::M
        ),
        def!(
            "cache.presence.ttl_ms",
            Integer {
                min: 0,
                max: 60_000
            },
            json!(500),
            S::M
        ),
        def!(
            "cache.model_health.ttl_ms",
            Integer {
                min: 0,
                max: 60_000
            },
            json!(500),
            S::M
        ),
        def!(
            "cache.harness_usage.ttl_ms",
            Integer {
                min: 0,
                max: 3_600_000
            },
            json!(180_000),
            S::M
        ),
        def!(
            "cache.updates.ttl_ms",
            Integer {
                min: 0,
                max: 86_400_000
            },
            json!(3_600_000),
            S::M
        ),
        def!(
            "cache.desktop.ports_poll_ms",
            Integer {
                min: 500,
                max: 600_000
            },
            json!(5_000),
            S::M
        ),
        def!(
            "cache.desktop.stats_poll_ms",
            Integer {
                min: 500,
                max: 600_000
            },
            json!(5_000),
            S::M
        ),
        def!(
            "cache.desktop.disk_poll_ms",
            Integer {
                min: 5_000,
                max: 3_600_000
            },
            json!(60_000),
            S::M
        ),
        def!(
            "cache.desktop.network_poll_ms",
            Integer {
                min: 5_000,
                max: 3_600_000
            },
            json!(30_000),
            S::M
        ),
        def!(
            "cache.desktop.sessions_safety_ms",
            Integer {
                min: 5_000,
                max: 3_600_000
            },
            json!(60_000),
            S::M
        ),
        // --- logging ---
        // The diagnostic log the node and the desktop write about themselves
        // under `logs/` — a file on this machine, never sent anywhere, so
        // every key is machine scope. Errors only by default: the words the
        // choices speak are `bisa-log`'s (`LogLevel::WORDS`,
        // `LogRotation::WORDS`), and an engine test holds the two equal.
        def!("logging.enabled", Bool, json!(true), S::M),
        def!(
            "logging.level",
            Choice(&["error", "warn", "info", "debug", "trace"]),
            json!("error"),
            S::M
        ),
        def!(
            "logging.rotation",
            Choice(&["hourly", "daily"]),
            json!("daily"),
            S::M
        ),
        def!(
            "logging.keep_files",
            Integer { min: 1, max: 366 },
            json!(14),
            S::M
        ),
        // --- rail ---
        // The person's manual ordering of the project rail: a view
        // preference, machine-scoped, kept as one JSON blob rather than a field
        // on the synced project/workstream records. Shape:
        // `{ projects: [id…], groups: [name…], workstreams: { [projectId]: [id…] } }`.
        // Anything unlisted falls back to the default (name / created-at) order.
        def!("rail.order", Structured, json!({}), S::M),
        // A project group carries adornment of its own: a photo, keyed
        // by the group's name. A group is still just the distinct `Project.group`
        // values, so this is display-only, like the project's photo — but
        // workspace-scoped, so it syncs across a person's machines rather than
        // staying local like the rail order. Shape:
        // `{ [groupName]: { photo?: AttachmentRef } }`.
        def!("rail.groups", Structured, json!({}), S::W),
        // --- network ---
        // How the platform reaches the internet, and what it hands everything
        // it runs (`network_settings.rs`): the proxy for its own HTTP — the
        // code hosts, the connectors, an A2A peer, a harness's usage endpoint
        // — exported as the standard variables to every harness session, git,
        // `gh`/`glab` and workstream script, so the platform and its tools
        // agree. Machine scope, never synced: a proxy is a fact about this
        // machine's network.
        def!(
            "network.proxy.mode",
            Choice(&["environment", "none", "manual"]),
            json!("environment"),
            S::M
        ),
        def!("network.proxy.http", Text, json!(""), S::M),
        def!("network.proxy.https", Text, json!(""), S::M),
        def!("network.proxy.no_proxy", Text, json!(""), S::M),
        def!("network.http1_only", Bool, json!(false), S::M),
        def!(
            "network.public_ip_url",
            Text,
            json!("https://api.ipify.org"),
            S::M
        ),
        // --- sync ---
        // How this node reaches other nodes (14-collaboration): the relays
        // that carry its wraps, how often it catches up, and the direct QUIC
        // transport for attachment bytes. Machine scope, never synced: a relay
        // list is a fact about this machine's network, as a proxy is.
        def!("sync.enabled", Bool, json!(false), S::M),
        def!(
            "sync.relays",
            Structured,
            json!(crate::collab_settings::DEFAULT_RELAYS),
            S::M
        ),
        def!(
            "sync.interval_secs",
            Integer { min: 5, max: 3600 },
            json!(30),
            S::M
        ),
        def!("sync.publish_relay_list", Bool, json!(true), S::M),
        def!("sync.iroh.enabled", Bool, json!(true), S::M),
        def!("sync.iroh.n0_relays", Bool, json!(false), S::M),
        def!("sync.iroh.peers", Structured, json!([]), S::M),
        // --- collab ---
        // The room's word to the people it hosts: what it is called, how a
        // code is admitted, what an invite defaults to. Workspace scope: the
        // room is the same from every machine the owner opens it on.
        def!("collab.name", Text, json!(""), S::W),
        def!(
            "collab.join",
            Choice(&["admit", "ask"]),
            json!("admit"),
            S::W
        ),
        def!(
            "collab.default_role",
            Choice(&["guest", "member"]),
            json!("guest"),
            S::W
        ),
        def!(
            "collab.invite_ttl_hours",
            Integer { min: 1, max: 720 },
            json!(24),
            S::W
        ),
        // --- desktop ---
        // The desktop app's own behaviour on this machine — machine scope,
        // never synced: whether a person here wants to be asked is theirs.
        def!("desktop.confirm_quit", Bool, json!(true), S::M),
        def!("desktop.close_keeps_running", Bool, json!(true), S::M),
        def!("desktop.dock_icon", Bool, json!(true), S::M),
        // --- notifications ---
        // Which OS notifications reach this person (Settings › System): one
        // master, then categories by what happened — asks that wait on them,
        // failures, finished work, workflows, addons' notices. The app's own
        // notice (a document that did not save on the way out) is under the
        // master alone. Machine scope, never synced: what interrupts a person
        // at this screen is theirs. `done` is off by default — a finished
        // step is not an interruption.
        def!("notifications.enabled", Bool, json!(true), S::M),
        def!("notifications.asks", Bool, json!(true), S::M),
        def!("notifications.failures", Bool, json!(true), S::M),
        def!("notifications.done", Bool, json!(false), S::M),
        def!("notifications.workflows", Bool, json!(true), S::M),
        def!("notifications.addons", Bool, json!(true), S::M),
        // --- addons ---
        // The master switch over every addon window on this machine (18 —
        // Addons). Machine scope, never synced: what floats over this
        // person's screen is theirs; which addons a workspace has and what
        // each was granted is the record's, not a setting's.
        def!("addons.enabled", Bool, json!(true), S::M),
        // --- system ---
        // The platform's *wish* about two grants the operating system
        // gives the desktop app (ide/01: a machine capability, the shell's to
        // ask for). Off by default: nothing of ours relies on a grant until the
        // person turns the switch on, and turning it off revokes nothing — only
        // System Settings can. Machine scope, never synced: whether this Mac
        // lets the app read everywhere is a fact about this Mac.
        def!("system.full_disk_access", Bool, json!(false), S::M),
        def!("system.microphone", Bool, json!(false), S::M),
    ]
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_key_is_unique_grouped_and_has_a_valid_default() {
        let mut keys: Vec<&str> = REGISTRY.iter().map(|d| d.key).collect();
        let n = keys.len();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), n, "duplicate key");
        for d in REGISTRY.iter() {
            assert!(d.key.contains('.'), "{}: keys are grouped by a dot", d.key);
            assert!(
                d.check_value(&d.default).is_ok(),
                "{}: default fails its own kind",
                d.key
            );
            assert!(!d.scopes.list().is_empty(), "{}: no scope", d.key);
        }
    }

    #[test]
    fn resolution_walks_project_then_workspace_then_machine() {
        let def = SettingDef::lookup("editor.word_wrap").unwrap(); // MWP
        let mut m = Layer::new();
        m.insert("editor.word_wrap".into(), json!("on"));
        let mut w = Layer::new();
        w.insert("editor.word_wrap".into(), json!("bounded"));
        let p = Layer::new();
        let r = resolve(
            def,
            &[
                (Scope::Project, &p),
                (Scope::Workspace, &w),
                (Scope::Machine, &m),
            ],
        );
        assert_eq!(r.value, json!("bounded"));
        assert_eq!(r.origin, Origin::Workspace);
        let r = resolve(def, &[(Scope::Machine, &m)]);
        assert_eq!(r.origin, Origin::Machine);
        let r = resolve(def, &[]);
        assert_eq!(r.origin, Origin::Default);
        assert_eq!(r.value, json!("off"));
    }

    #[test]
    fn a_disallowed_scope_is_refused_at_write_and_ignored_at_read() {
        let err = check_write("editor.font_size", Scope::Project, &json!(14)).unwrap_err();
        assert!(matches!(err, SettingsError::ScopeNotAllowed { .. }));
        assert!(check_write("editor.font_size", Scope::Machine, &json!(14)).is_ok());
        // A value that sneaked into a project file is not honoured.
        let mut p = Layer::new();
        p.insert("editor.font_size".into(), json!(30));
        let r = resolve(
            SettingDef::lookup("editor.font_size").unwrap(),
            &[(Scope::Project, &p)],
        );
        assert_eq!(r.origin, Origin::Default);
    }

    #[test]
    fn values_are_checked_against_their_kind_and_bounds() {
        assert!(check_write("editor.tab_size", Scope::Project, &json!(4)).is_ok());
        assert!(matches!(
            check_write("editor.tab_size", Scope::Project, &json!(40)),
            Err(SettingsError::InvalidValue { .. })
        ));
        assert!(matches!(
            check_write("appearance.accent", Scope::Machine, &json!("mauve")),
            Err(SettingsError::InvalidValue { .. })
        ));
        // The Board's dials hold their bounds.
        assert!(check_write(
            "workstreams.board.due_soon_days",
            Scope::Workspace,
            &json!(7)
        )
        .is_ok());
        assert!(matches!(
            check_write(
                "workstreams.board.due_soon_days",
                Scope::Workspace,
                &json!(0)
            ),
            Err(SettingsError::InvalidValue { .. })
        ));
        assert!(matches!(
            check_write("workstreams.board.wip_limit", Scope::Machine, &json!(51)),
            Err(SettingsError::InvalidValue { .. })
        ));
        assert!(matches!(
            check_write("workstreams.board.enabled", Scope::Project, &json!(false)),
            Err(SettingsError::ScopeNotAllowed { .. })
        ));
        // The IDE's default mode is one of two words, at the workspace or the machine.
        assert!(check_write("ide.default_mode", Scope::Workspace, &json!("agent")).is_ok());
        assert!(check_write("ide.default_mode", Scope::Workspace, &json!("board")).is_ok());
        assert!(matches!(
            check_write("ide.default_mode", Scope::Machine, &json!("both")),
            Err(SettingsError::InvalidValue { .. })
        ));
        assert!(matches!(
            check_write("ide.default_mode", Scope::Project, &json!("agent")),
            Err(SettingsError::ScopeNotAllowed { .. })
        ));
        // The browser's policy is three words at the workspace or a project —
        // everyone by default; so is where an agent's tab is kept out of sight;
        // its switch is the machine's alone; a screenshot has bounds.
        assert_eq!(
            SettingDef::lookup("browser.agents").unwrap().default,
            json!("everyone")
        );
        assert_eq!(
            SettingDef::lookup("browser.agents.headless")
                .unwrap()
                .default,
            json!("unattended")
        );
        assert!(check_write("browser.agents", Scope::Project, &json!("assigned")).is_ok());
        assert!(check_write("browser.agents.headless", Scope::Project, &json!("always")).is_ok());
        assert!(matches!(
            check_write("browser.agents.headless", Scope::Machine, &json!("never")),
            Err(SettingsError::ScopeNotAllowed { .. })
        ));
        assert!(matches!(
            check_write("browser.agents.headless", Scope::Workspace, &json!("quiet")),
            Err(SettingsError::InvalidValue { .. })
        ));
        assert!(SettingDef::lookup("browser.agents.reveal").is_none());
        assert!(matches!(
            check_write("browser.agents", Scope::Workspace, &json!("some")),
            Err(SettingsError::InvalidValue { .. })
        ));
        assert!(matches!(
            check_write("browser.enabled", Scope::Workspace, &json!(false)),
            Err(SettingsError::ScopeNotAllowed { .. })
        ));
        assert!(check_write(
            "browser.agents.reach",
            Scope::Workspace,
            &json!("local_only")
        )
        .is_ok());
        assert!(check_write("browser.screenshot.width", Scope::Machine, &json!(1920)).is_ok());
        // The canvas: its switch is the machine's; who may draw is the
        // workspace's or a project's, everyone by default; a snapshot has bounds.
        assert_eq!(
            SettingDef::lookup("draw.agents").unwrap().default,
            json!("everyone")
        );
        assert!(check_write("draw.agents", Scope::Project, &json!("assigned")).is_ok());
        assert!(matches!(
            check_write("draw.agents", Scope::Workspace, &json!("some")),
            Err(SettingsError::InvalidValue { .. })
        ));
        assert!(matches!(
            check_write("draw.enabled", Scope::Workspace, &json!(false)),
            Err(SettingsError::ScopeNotAllowed { .. })
        ));
        assert!(check_write("draw.snapshot.width", Scope::Machine, &json!(1920)).is_ok());
        assert!(matches!(
            check_write("draw.snapshot.width", Scope::Machine, &json!(100)),
            Err(SettingsError::InvalidValue { .. })
        ));
        // Mobile development is off until this machine says otherwise; its
        // platforms and paths are the machine's; who may drive the devices
        // is the workspace's or a project's.
        assert_eq!(
            SettingDef::lookup("mobile_development.enabled")
                .unwrap()
                .default,
            json!(false)
        );
        assert_eq!(
            SettingDef::lookup("mobile_development.platforms")
                .unwrap()
                .default,
            json!("both")
        );
        assert_eq!(
            SettingDef::lookup("mobile_development.agents")
                .unwrap()
                .default,
            json!("everyone")
        );
        assert!(check_write("mobile_development.enabled", Scope::Machine, &json!(true)).is_ok());
        assert!(check_write(
            "mobile_development.agents",
            Scope::Project,
            &json!("assigned")
        )
        .is_ok());
        assert!(check_write(
            "mobile_development.flutter.path",
            Scope::Machine,
            &json!("/opt/flutter")
        )
        .is_ok());
        assert!(matches!(
            check_write("mobile_development.enabled", Scope::Workspace, &json!(true)),
            Err(SettingsError::ScopeNotAllowed { .. })
        ));
        assert!(matches!(
            check_write(
                "mobile_development.platforms",
                Scope::Machine,
                &json!("web")
            ),
            Err(SettingsError::InvalidValue { .. })
        ));
        // The group is `mobile_development`, never `mobile` alone: a mobile
        // application — a client of this platform — will want that word for
        // its own keys, and nothing of this feature may read as its.
        assert!(
            REGISTRY.iter().all(|d| !d.key.starts_with("mobile.")),
            "no key of the feature says mobile alone"
        );
        assert_eq!(
            REGISTRY
                .iter()
                .filter(|d| d.group() == "mobile_development")
                .count(),
            5,
            "the switch, the platforms, the agents, the two paths"
        );
        assert!(matches!(
            check_write(
                "mobile_development.agents",
                Scope::Machine,
                &json!("everyone")
            ),
            Err(SettingsError::ScopeNotAllowed { .. })
        ));
        // Sync is the machine's; the room's words are the workspace's.
        assert!(check_write("sync.enabled", Scope::Machine, &json!(true)).is_ok());
        assert!(matches!(
            check_write("sync.enabled", Scope::Workspace, &json!(true)),
            Err(SettingsError::ScopeNotAllowed { .. })
        ));
        assert_eq!(
            SettingDef::lookup("sync.enabled").unwrap().default,
            json!(false),
            "off by default"
        );
        assert_eq!(
            SettingDef::lookup("sync.relays").unwrap().default,
            json!(crate::collab_settings::DEFAULT_RELAYS)
        );
        // An auto goal's ceiling is the classifier's to read, at either scope.
        assert_eq!(
            SettingDef::lookup("goals.auto.permissions")
                .unwrap()
                .default,
            json!("classify"),
            "an auto goal runs unattended by default"
        );
        assert!(check_write("goals.auto.permissions", Scope::Workspace, &json!("ask")).is_ok());
        assert!(check_write("goals.auto.permissions", Scope::Machine, &json!("classify")).is_ok());
        assert!(
            check_write("goals.auto.permissions", Scope::Workspace, &json!("allow")).is_err(),
            "the classifier never allows what a rule did not; there is no `allow`"
        );
        // An auto goal's `write` step runs commands, at either scope; the
        // ceiling is lifted or the step's own — a bare tier is no choice.
        assert_eq!(
            SettingDef::lookup("goals.auto.ceiling").unwrap().default,
            json!("exec"),
            "an auto goal's write step runs commands by default"
        );
        assert!(check_write("goals.auto.ceiling", Scope::Workspace, &json!("step")).is_ok());
        assert!(check_write("goals.auto.ceiling", Scope::Machine, &json!("exec")).is_ok());
        assert!(
            check_write("goals.auto.ceiling", Scope::Workspace, &json!("write")).is_err(),
            "the ceiling is lifted or the step's own; a tier name is not a choice"
        );
        assert!(check_write(
            "sync.relays",
            Scope::Machine,
            &json!(["wss://relay.example"])
        )
        .is_ok());
        assert!(matches!(
            check_write("sync.relays", Scope::Workspace, &json!([])),
            Err(SettingsError::ScopeNotAllowed { .. })
        ));
        assert!(matches!(
            check_write("sync.interval_secs", Scope::Machine, &json!(1)),
            Err(SettingsError::InvalidValue { .. })
        ));
        assert!(check_write("collab.join", Scope::Workspace, &json!("ask")).is_ok());
        assert!(matches!(
            check_write("collab.join", Scope::Workspace, &json!("never")),
            Err(SettingsError::InvalidValue { .. })
        ));
        assert!(check_write("collab.invite_ttl_hours", Scope::Workspace, &json!(720)).is_ok());
        assert!(matches!(
            check_write("collab.invite_ttl_hours", Scope::Workspace, &json!(0)),
            Err(SettingsError::InvalidValue { .. })
        ));
        assert!(check_write(
            "security.collaboration.agent_tools",
            Scope::Machine,
            &json!("as_owner")
        )
        .is_ok());
        assert!(matches!(
            check_write("browser.screenshot.width", Scope::Machine, &json!(100)),
            Err(SettingsError::InvalidValue { .. })
        ));
        assert!(matches!(
            check_write("nope.key", Scope::Machine, &json!(1)),
            Err(SettingsError::UnknownKey(_))
        ));
        assert!(check_write(
            "keymap.overrides",
            Scope::Machine,
            &json!({"save": "ctrl+s"})
        )
        .is_ok());
    }

    /// The keys the code reads by a constant are the registry's, and the run
    /// bound is never zero: the run that just ended is always there to read.
    #[test]
    fn every_key_named_by_a_constant_is_registered_and_the_run_bound_starts_at_one() {
        for key in [
            crate::Budget::SETTING_TOKENS,
            crate::Budget::SETTING_USD_CENTS,
            crate::Budget::SETTING_WALL_CLOCK_SECS,
            crate::WorkflowRun::SETTING_KEPT,
        ] {
            assert!(SettingDef::lookup(key).is_some(), "{key} is registered");
        }
        let kept = SettingDef::lookup(crate::WorkflowRun::SETTING_KEPT).unwrap();
        assert_eq!(kept.default, json!(500));
        assert!(check_write(kept.key, Scope::Workspace, &json!(1)).is_ok());
        assert!(check_write(kept.key, Scope::Machine, &json!(100_000)).is_ok());
        assert!(matches!(
            check_write(kept.key, Scope::Workspace, &json!(0)),
            Err(SettingsError::InvalidValue { .. })
        ));
        assert!(matches!(
            check_write(kept.key, Scope::Project, &json!(5)),
            Err(SettingsError::ScopeNotAllowed { .. })
        ));
    }

    #[test]
    fn resolve_all_covers_the_registry() {
        let all = resolve_all(&[]);
        assert_eq!(all.len(), REGISTRY.len());
        assert!(all.iter().all(|r| r.origin == Origin::Default));
        assert_eq!("project".parse::<Scope>().unwrap(), Scope::Project);
    }
    #[test]
    fn an_ide_conversation_starts_in_manual_and_the_words_are_the_domains() {
        let def = SettingDef::lookup("agents.conversation.mode").unwrap();
        assert_eq!(
            def.default,
            json!(crate::ConversationMode::default().as_str())
        );
        for mode in crate::ConversationMode::ALL {
            assert!(
                check_write(
                    "agents.conversation.mode",
                    Scope::Project,
                    &json!(mode.as_str())
                )
                .is_ok(),
                "{}",
                mode.as_str()
            );
        }
        assert!(matches!(
            check_write(
                "agents.conversation.mode",
                Scope::Workspace,
                &json!("guided")
            ),
            Err(SettingsError::InvalidValue { .. })
        ));
        assert_eq!(
            SettingDef::lookup("agents.review.checkpoints")
                .unwrap()
                .default,
            json!(20)
        );
    }
    #[test]
    fn the_decision_making_agent_is_off_until_somebody_switches_it_on() {
        let def = |key: &str| SettingDef::lookup(key).unwrap_or_else(|| panic!("{key}"));
        assert_eq!(def("decisions.enabled").default, json!(false));
        assert_eq!(def("decisions.provider").default, json!("harness"));
        assert_eq!(def("decisions.harness.id").default, json!("claude-code"));
        assert_eq!(
            def("decisions.harness.model").default,
            json!("claude-sonnet-5-5[1m]")
        );
        assert_eq!(def("decisions.harness.effort").default, json!("high"));
        assert_eq!(
            def("security.classifier.model").default,
            json!("claude-sonnet-5-5[1m]")
        );
        assert_eq!(def("security.classifier.effort").default, json!("high"));
        assert_eq!(def("decisions.jev.model").default, json!("jev-latest"));
        assert_eq!(def("security.classifier.provider").default, json!("agent"));
        assert_eq!(def("security.content.screen").default, json!(true));
        assert_eq!(def("security.content.on_harmful").default, json!("ask"));
        // The provider words are the domain's, one for one.
        for kind in crate::decision::DecisionProviderKind::ALL {
            assert!(
                check_write(
                    "decisions.provider",
                    Scope::Workspace,
                    &json!(kind.as_str())
                )
                .is_ok(),
                "{}",
                kind.as_str()
            );
        }
        assert!(matches!(
            check_write("decisions.provider", Scope::Workspace, &json!("oracle")),
            Err(SettingsError::InvalidValue { .. })
        ));
        // A project may switch it on for itself; who decides is the node's.
        assert!(check_write("decisions.enabled", Scope::Project, &json!(true)).is_ok());
        assert!(matches!(
            check_write("decisions.provider", Scope::Project, &json!("jev")),
            Err(SettingsError::ScopeNotAllowed { .. })
        ));
        // A security verdict is never gated below a coin toss.
        assert!(matches!(
            check_write(
                "decisions.confidence.security",
                Scope::Workspace,
                &json!(0.2)
            ),
            Err(SettingsError::InvalidValue { .. })
        ));
        assert!(check_write("decisions.confidence.act", Scope::Project, &json!(0.55)).is_ok());
        // No key holds a credential: the key lives in the keystore.
        assert!(REGISTRY
            .iter()
            .filter(|d| d.group() == "decisions")
            .all(|d| !d.key.contains("key") && !d.key.contains("token")));
    }

    #[test]
    fn the_classifier_is_read_by_an_agent_a_harness_or_the_decision_making_agent() {
        let def = SettingDef::lookup("security.classifier.provider").unwrap();
        assert_eq!(
            def.kind,
            Kind::Choice(&["agent", "harness", "decision_making_agent"])
        );
        for word in ["agent", "harness", "decision_making_agent"] {
            assert!(
                check_write(
                    "security.classifier.provider",
                    Scope::Workspace,
                    &json!(word)
                )
                .is_ok(),
                "{word}"
            );
        }
    }

    #[test]
    fn the_effort_settings_offer_exactly_the_domains_words_in_order() {
        use crate::effort::{Effort, EffortChoice};
        let choices = |key: &str| match &SettingDef::lookup(key).unwrap().kind {
            Kind::Choice(words) => words.to_vec(),
            other => panic!("{key} is {other:?}, not a choice"),
        };
        let asked: Vec<&str> = EffortChoice::ALL.iter().map(|c| c.as_str()).collect();
        let levels: Vec<&str> = Effort::ALL.iter().map(|e| e.as_str()).collect();
        // An agent's may be the judge's to name; the judge's own and the
        // classifier's are a level.
        assert_eq!(choices("agents.effort"), asked);
        assert_eq!(choices("decisions.harness.effort"), levels);
        assert_eq!(choices("security.classifier.effort"), levels);
        assert_eq!(asked[0], "auto");
        assert!(!levels.contains(&"auto"));

        // Each says `high` until somebody says otherwise — the domain's own
        // defaults.
        let default = |key: &str| SettingDef::lookup(key).unwrap().default.clone();
        assert_eq!(
            default("agents.effort"),
            json!(EffortChoice::DEFAULT.as_str())
        );
        assert_eq!(
            default("decisions.harness.effort"),
            json!(Effort::DEFAULT.as_str())
        );
        assert_eq!(
            default("security.classifier.effort"),
            json!(Effort::DEFAULT.as_str())
        );

        // A project and a workspace hold the agents' dial; the machine does
        // not. The other two are the node's.
        assert_eq!(
            SettingDef::lookup("agents.effort").unwrap().scopes,
            ScopeSet::WP
        );
        assert!(check_write("agents.effort", Scope::Project, &json!("auto")).is_ok());
        assert!(check_write("agents.effort", Scope::Workspace, &json!("xhigh")).is_ok());
        assert!(matches!(
            check_write("agents.effort", Scope::Machine, &json!("high")),
            Err(SettingsError::ScopeNotAllowed { .. })
        ));
        for key in ["decisions.harness.effort", "security.classifier.effort"] {
            assert_eq!(SettingDef::lookup(key).unwrap().scopes, ScopeSet::MW);
            assert!(check_write(key, Scope::Machine, &json!("max")).is_ok());
            assert!(matches!(
                check_write(key, Scope::Project, &json!("max")),
                Err(SettingsError::ScopeNotAllowed { .. })
            ));
            assert!(
                matches!(
                    check_write(key, Scope::Workspace, &json!("auto")),
                    Err(SettingsError::InvalidValue { .. })
                ),
                "{key} takes a level, never auto"
            );
        }
        // A harness's own word that is no level of ours is refused.
        for odd in ["ultra", "off", "x_high", "High", ""] {
            assert!(matches!(
                check_write("agents.effort", Scope::Workspace, &json!(odd)),
                Err(SettingsError::InvalidValue { .. })
            ));
        }
    }

    #[test]
    fn the_classifiers_retired_provider_word_is_refused() {
        let retired = json!("decision_maker"); // terminology-lint-ignore: decision-maker - proves the retired word is refused
        assert!(matches!(
            check_write("security.classifier.provider", Scope::Workspace, &retired),
            Err(SettingsError::InvalidValue { .. })
        ));
    }
}
