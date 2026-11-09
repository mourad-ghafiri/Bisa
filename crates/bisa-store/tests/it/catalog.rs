//! The catalog installs nothing until asked; the three permanent objects are
//! ensured at every open.

use bisa_core::{
    AgentId, AgentOrigin, Assignee, ChannelId, ChannelOrigin, Member, ModelPlan, Origin,
    RosterPolicy, SkillId, Tags, TeamId,
};
use bisa_store::{CatalogKind, FileKeyStore, MemoryKeyStore, NewAgent, Paths, Workspace, CATALOG};

fn ws() -> (tempfile::TempDir, Workspace) {
    let dir = tempfile::tempdir().unwrap();
    let ws =
        Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
    (dir, ws)
}

fn file_ws(dir: &tempfile::TempDir) -> Workspace {
    Workspace::open_with_keystore(
        dir.path(),
        Box::new(FileKeyStore::new(Paths::new(dir.path()).identity_dir())),
    )
    .unwrap()
}

fn new_agent(name: &str) -> NewAgent {
    NewAgent {
        name: name.into(),
        system_prompt: "x".into(),
        harness: "mock".into(),
        ..Default::default()
    }
}

#[test]
fn a_fresh_workspace_holds_exactly_the_three_permanent_objects() {
    let (_d, ws) = ws();
    let agents = ws.list_agents().unwrap();
    assert_eq!(agents.len(), 2, "the General Agent and the Workflow Agent");
    let ids: Vec<&str> = agents.iter().map(|a| a.id.as_str()).collect();
    for core in AgentId::CORE {
        assert!(ids.contains(&core), "{core} is ensured at open: {ids:?}");
    }
    for a in &agents {
        assert!(a.id.is_core_id());
        assert_eq!(a.origin, AgentOrigin::Core);
        assert_eq!(
            Some(a.name.as_str()),
            bisa_core::Agent::core_name(&a.id),
            "{}: a core agent's name is fixed",
            a.id
        );
        assert!(a.enabled);
    }
    assert!(ws.list_skills().unwrap().is_empty());
    assert!(ws.list_teams().unwrap().is_empty());
    assert!(ws.list_workflows().unwrap().is_empty());
    let channels = ws.list_channels().unwrap();
    assert_eq!(channels.len(), 1);
    assert!(channels[0].is_general());
    assert_eq!(channels[0].origin, ChannelOrigin::Core);
}

#[test]
fn the_permanent_objects_survive_a_second_open_and_a_rebuild() {
    let dir = tempfile::tempdir().unwrap();
    let general = AgentId::general();
    let (pubkey, edited_harness) = {
        let ws = file_ws(&dir);
        let mut a = ws.get_agent(&general).unwrap();
        a.harness = "codex".into();
        let a = ws.update_agent(a).unwrap();
        (a.pubkey, a.harness)
    };
    let ws = file_ws(&dir);
    let again = ws.get_agent(&general).unwrap();
    assert_eq!(again.pubkey, pubkey, "reopen does not mint a new key");
    assert_eq!(again.harness, edited_harness, "an edit survives a reopen");
    ws.rebuild_index().unwrap();
    assert_eq!(ws.list_agents().unwrap().len(), 2);
    assert!(ws.get_agent(&AgentId::workflow()).is_ok());
    assert!(ws.get_channel(&ChannelId::general()).is_ok());
}

#[test]
fn the_core_agents_are_guarded_by_the_core_rules() {
    let (_d, ws) = ws();
    for core in AgentId::CORE {
        let id = AgentId::new(core).unwrap();
        assert!(ws.remove_agent(&id).is_err(), "{core}");
        assert!(ws.set_agent_enabled(&id, false).is_err(), "{core}");
        let mut a = ws.get_agent(&id).unwrap();
        a.system_prompt = "be evil".into();
        let err = ws.update_agent(a).unwrap_err().to_string();
        assert!(
            err.contains("system_prompt"),
            "{core}: refused by name: {err}"
        );
        let mut a = ws.get_agent(&id).unwrap();
        a.harness = "codex".into();
        a.models = ModelPlan::pinned("opus");
        let a = ws.update_agent(a).unwrap();
        assert_eq!(a.harness, "codex");
        // An unchanged full-object update is accepted.
        ws.update_agent(ws.get_agent(&id).unwrap()).unwrap();
    }
}

#[test]
fn the_core_agents_are_participants_everywhere_and_stored_nowhere() {
    let (_d, ws) = ws();
    let general = AgentId::general();
    let workflow = AgentId::workflow();
    let dev = ws.add_agent(new_agent("Dev")).unwrap();
    let team = ws
        .create_team(
            "eng",
            None,
            vec![
                Assignee::Agent(general.to_string()),
                Assignee::Agent(workflow.to_string()),
                Assignee::Agent(dev.id.to_string()),
            ],
            Tags::default(),
        )
        .unwrap();
    assert_eq!(team.members, vec![Assignee::Agent(dev.id.to_string())]);
    assert_eq!(ws.team_agents(&team.id).unwrap(), vec![dev.id.clone()]);
    let participants = ws.team_participants(&team.id).unwrap();
    assert!(participants.contains(&Assignee::Agent(general.to_string())));
    assert!(participants.contains(&Assignee::Agent(workflow.to_string())));
    let c = ws
        .create_channel(
            "room",
            None,
            RosterPolicy::Listed {
                agents: vec![general.clone(), workflow.clone(), dev.id.clone()],
                teams: vec![],
                humans: vec![],
            },
            Tags::default(),
        )
        .unwrap();
    assert!(
        matches!(&c.roster, RosterPolicy::Listed { agents, .. } if agents == &vec![dev.id.clone()])
    );
    let pks = ws.channel_roster_pubkeys(&c.id).unwrap();
    let general_pk = ws.get_agent(&general).unwrap().pubkey;
    let workflow_pk = ws.get_agent(&workflow).unwrap().pubkey;
    assert_eq!(pks.iter().filter(|p| **p == general_pk).count(), 1);
    assert_eq!(pks.iter().filter(|p| **p == workflow_pk).count(), 1);
    assert_eq!(&pks[pks.len() - 2..], &[general_pk, workflow_pk]);
    // In `general`, membership is everyone that is enabled.
    let members = ws.channel_members(&ChannelId::general()).unwrap();
    assert!(members.contains(&Member::Agent(dev.id.clone())));
    assert!(members.contains(&Member::Team(team.id.clone())));
}

#[test]
fn catalog_entries_list_every_kind_and_mark_what_is_installed() {
    let (_d, ws) = ws();
    let all = ws.catalog_entries(None).unwrap();
    assert_eq!(all.len(), CATALOG.entry_count());
    assert!(all.iter().all(|e| !e.installed));
    assert!(!all
        .iter()
        .any(|e| e.slug == ChannelId::GENERAL && e.kind == CatalogKind::Channel));
    ws.install(CatalogKind::Agent, "developer").unwrap();
    let dev = ws.catalog_entry(CatalogKind::Agent, "developer").unwrap();
    assert!(dev.installed);
    assert!(!dev.requires.is_empty(), "an agent requires its skills");
    for s in &dev.requires {
        assert!(ws.catalog_entry(CatalogKind::Skill, s).unwrap().installed);
    }
    assert!(ws.catalog_entry(CatalogKind::Agent, "nope").is_err());
    let health = ws
        .catalog_entry(CatalogKind::Workflow, "standing-health-check")
        .unwrap();
    assert!(
        health
            .requires
            .contains(&"workflow:incident-response".to_string()),
        "a workflow requires the templates its spawn steps start: {:?}",
        health.requires
    );
    assert!(health.requires.contains(&"sre".to_string()));
    // A workflow template's row carries its definition — the graph a gallery
    // draws before an install — read whole on a fresh workspace, a `spawn`
    // target that is not here yet included; no other kind carries one.
    for e in ws.catalog_entries(Some(CatalogKind::Workflow)).unwrap() {
        let wf = e
            .workflow
            .as_ref()
            .unwrap_or_else(|| panic!("{}: a template's row carries its definition", e.slug));
        assert!(!wf.steps.is_empty(), "{}: a definition has steps", e.slug);
        assert!(
            wf.steps.iter().any(|s| !s.then.is_empty()),
            "{}: a definition has flows",
            e.slug
        );
        assert_eq!(wf.name, e.name);
    }
    assert!(
        ws.catalog_entries(Some(CatalogKind::Agent))
            .unwrap()
            .iter()
            .all(|e| e.workflow.is_none()),
        "only a workflow row carries a definition"
    );
}

#[test]
fn installing_is_transitive_idempotent_and_never_destructive() {
    let (_d, ws) = ws();
    let team = ws.install(CatalogKind::Team, "engineering").unwrap();
    assert!(!team.agents.is_empty() && !team.skills.is_empty());
    for a in &team.agents {
        let agent = ws.get_agent(&AgentId::new(a).unwrap()).unwrap();
        assert_eq!(agent.origin, AgentOrigin::Catalog { slug: a.clone() });
        for s in &agent.skills {
            assert_eq!(
                ws.get_skill(s).unwrap().origin,
                Origin::Catalog {
                    slug: s.to_string()
                }
            );
        }
    }
    assert!(ws
        .install(CatalogKind::Team, "engineering")
        .unwrap()
        .is_empty());

    let room = ws.install(CatalogKind::Channel, "engineering").unwrap();
    assert_eq!(room.channels, vec!["engineering".to_string()]);
    let c = ws
        .get_channel(&ChannelId::new("engineering").unwrap())
        .unwrap();
    assert_eq!(
        c.origin,
        ChannelOrigin::Catalog {
            slug: "engineering".into()
        }
    );
    assert!(matches!(&c.roster, RosterPolicy::Listed { agents, .. } if !agents.is_empty()));
    for a in ws.channel_members(&c.id).unwrap() {
        if let Member::Agent(id) = a {
            assert!(ws.get_agent(&id).is_ok());
        }
    }

    // A colliding local id is refused by name and creates nothing.
    let d2 = tempfile::tempdir().unwrap();
    let ws2 =
        Workspace::open_with_keystore(d2.path(), Box::new(MemoryKeyStore::default())).unwrap();
    ws2.create_skill(bisa_store::NewSkill {
        id: SkillId::new("code-review-checklist").unwrap(),
        name: "mine".into(),
        description: "mine".into(),
        tags: Tags::default(),
        markdown: "# mine".into(),
    })
    .unwrap();
    let err = ws2
        .install(CatalogKind::Agent, "code-reviewer")
        .unwrap_err()
        .to_string();
    assert!(err.contains("code-review-checklist"), "{err}");
    assert!(err.contains("one you created here"), "{err}");
    assert_eq!(ws2.list_agents().unwrap().len(), 2, "nothing was created");
    assert_eq!(
        ws2.get_skill(&SkillId::new("code-review-checklist").unwrap())
            .unwrap()
            .name,
        "mine"
    );
    let _ = TeamId::new("engineering").unwrap();
}

#[test]
fn installing_a_workflow_brings_its_agents_and_the_templates_it_spawns() {
    let (_d, ws) = ws();
    let installed = ws
        .install(CatalogKind::Workflow, "standing-health-check")
        .unwrap();
    let slugs: Vec<&str> = installed
        .workflows
        .iter()
        .map(|(s, _)| s.as_str())
        .collect();
    assert!(slugs.contains(&"standing-health-check"), "{slugs:?}");
    assert!(slugs.contains(&"incident-response"), "{slugs:?}");
    assert!(slugs.contains(&"bug-fix"), "{slugs:?}");
    assert!(installed.agents.contains(&"sre".to_string()));
    assert!(installed.agents.contains(&"developer".to_string()));
    for (slug, id) in &installed.workflows {
        let wf = ws.get_workflow(*id).unwrap();
        assert_eq!(
            wf.origin,
            bisa_core::WorkflowOrigin::Catalog {
                slug: slug.to_string()
            }
        );
        assert!(
            ws.validate_workflow(&wf).unwrap().is_empty(),
            "{slug} installs clean"
        );
    }
    // The spawn step now names the installed id, not the slug.
    let (_, health_id) = installed
        .workflows
        .iter()
        .find(|(s, _)| s == "standing-health-check")
        .unwrap();
    let (_, incident_id) = installed
        .workflows
        .iter()
        .find(|(s, _)| s == "incident-response")
        .unwrap();
    let health = ws.get_workflow(*health_id).unwrap();
    let spawn = health
        .steps
        .iter()
        .find_map(|s| match &s.kind {
            bisa_core::StepKind::Spawn { workflow, .. } => *workflow,
            _ => None,
        })
        .expect("a spawn step");
    assert_eq!(spawn, *incident_id);
    // Idempotent: a second install creates nothing and keeps the ids.
    assert!(ws
        .install(CatalogKind::Workflow, "standing-health-check")
        .unwrap()
        .is_empty());
    assert!(
        ws.catalog_entry(CatalogKind::Workflow, "bug-fix")
            .unwrap()
            .installed
    );
    assert_eq!(
        ws.workflow_for_slug("bug-fix").unwrap().map(|w| w.id),
        installed
            .workflows
            .iter()
            .find(|(s, _)| s == "bug-fix")
            .map(|(_, id)| *id)
    );
    assert_eq!(ws.list_workflows().unwrap().len(), 3);
}

// ---------------------------------------------------------------------------
// The prose names real tools
// ---------------------------------------------------------------------------

/// The MCP tool names, read from the one place they are declared. The store
/// cannot depend on `bisa-mcp` (layering), so this reads the source the
/// way `layout.rs` reads every crate: as text, by path.
fn mcp_tool_names() -> std::collections::BTreeSet<String> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../bisa-mcp/src/server.rs");
    let src = std::fs::read_to_string(&path).expect("bisa-mcp/src/server.rs is readable");
    let mut names = std::collections::BTreeSet::new();
    let mut in_tool_attr = false;
    for line in src.lines() {
        let t = line.trim();
        if t.starts_with("#[tool(") {
            in_tool_attr = true;
        }
        if in_tool_attr {
            if let Some(rest) = t.strip_prefix("name = \"") {
                if let Some(end) = rest.find('"') {
                    names.insert(rest[..end].to_string());
                }
            }
            if t.ends_with(")]") {
                in_tool_attr = false;
            }
        }
    }
    assert!(
        names.contains("get_goal"),
        "the tool list did not parse: {names:?}"
    );
    names
}

/// Words in `snake_case` that a prompt or a template may use without naming a
/// tool: wire values, field names and the workflow grammar's own keys, which a
/// TOML inline table puts after an `=` where the scanner reads prose. Anything
/// else that looks like a tool must be one.
const WIRE_WORDS: &[&str] = &[
    "owner_only",                // `respond` policy value
    "publish_mode",              // a config key quoted in the release checklist
    "pr_open",                   // a workstream state
    "default_branch",            // a project field
    "system_prompt",             // a TOML key, quoted in skills that teach prompt-writing
    "commit_refuses_clean_tree", // a test name used as an example
    "test_commit_3",             // the counter-example beside it
    "on_fail",                   // a step's failure rule, as an inline table key
    "max_visits",                // a step's loop bound
    "output_equals",             // the condition tags a decide step's rules carry
    "output_matches",
    "input_equals",
    "fire_on",        // a check start's rule for which results begin a run
    "starts_failing", // its default word
    "by_x",           // `browser_scroll`'s two pixel offsets, named in the browser skill
    "by_y",
    "full_page", // `browser_screenshot`'s whole-page switch, named in the browser skill
    "output_schema", // a step's result schema, when quoted in prose
    "statement_template", // a spawn step's field
    "for_each",  // the loop step kind
    "max_iterations", // a loop step's bound
    "mobile_development", // a settings group, quoted as `mobile_development.agents`
    "pull_request", // a project change the Workflow Agent names
];

fn snake_tokens(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for raw in text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')) {
        let word = raw.trim_matches('_');
        if word.contains('_')
            && word
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        {
            out.push(word.to_string());
        }
    }
    out
}

#[test]
fn every_snake_case_word_in_the_catalog_prose_is_a_real_tool() {
    let tools = mcp_tool_names();
    let mut sources: Vec<(String, String)> = Vec::new();
    for kind in CatalogKind::ALL.iter().copied() {
        // A connector definition is a platform's API spelled out — operation
        // ids, query keys, parameter names — and none of those words is a tool
        // an agent calls; the Workflow Agent reads them through the roster.
        // An addon's manifest is a shape — window bounds, permission words —
        // that no agent reads; its display words are checked by
        // `every_addon_is_well_formed` and the i18n content seam.
        if matches!(kind, CatalogKind::Connector | CatalogKind::Addon) {
            continue;
        }
        for (slug, toml_str) in CATALOG.entries(kind) {
            sources.push((format!("{kind} {slug}"), toml_str.to_string()));
        }
    }
    for core in ["general-agent.toml", "workflow-agent.toml", "general.toml"] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../library/core")
            .join(core);
        sources.push((core.to_string(), std::fs::read_to_string(path).unwrap()));
    }

    let mut offenders = Vec::new();
    for (name, text) in &sources {
        for line in text.lines() {
            // A TOML key on its own line is structure, not prose.
            let prose = match line.split_once('=') {
                Some((key, rest)) if !key.trim().contains(' ') && !key.trim().is_empty() => rest,
                _ => line,
            };
            for word in snake_tokens(prose) {
                if !tools.contains(&word) && !WIRE_WORDS.contains(&word.as_str()) {
                    offenders.push(format!("{name}: `{word}`"));
                }
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "prose names something that is not a tool — rename it, or add a wire word:\n{}",
        offenders.join("\n")
    );
}

// ---------------------------------------------------------------------------
// Update from the catalog — a person's act, the one that changes an installed
// copy (06 § Skills; the guide's catalog section)
// ---------------------------------------------------------------------------

fn differs(ws: &Workspace, kind: CatalogKind, slug: &str) -> Option<bool> {
    ws.catalog_entry(kind, slug).unwrap().installed_differs
}

/// A plan a person pinned, which an update never touches.
fn pinned_plan() -> ModelPlan {
    ModelPlan {
        strategy: bisa_core::ModelStrategy::Weighted,
        effort: None,
        models: vec![bisa_core::ModelChoice {
            model: "pinned-model".into(),
            weight: 2,
            enabled: true,
            suited_for: None,
            effort: None,
        }],
    }
}

#[test]
fn updating_an_installed_agent_restates_the_bundles_words_and_keeps_what_the_person_chose() {
    let (_d, ws) = ws();
    let dev = AgentId::new("developer").unwrap();
    assert_eq!(
        differs(&ws, CatalogKind::Agent, "developer"),
        None,
        "not installed: nothing to compare"
    );
    let created = ws.update_entry(CatalogKind::Agent, "developer").unwrap();
    assert!(
        !created.restated && created.installed.agents == vec!["developer".to_string()],
        "an update of an agent that is not here installs it, with its skills: {created:?}"
    );
    assert!(!created.installed.skills.is_empty(), "{created:?}");
    assert_eq!(
        differs(&ws, CatalogKind::Agent, "developer"),
        Some(false),
        "freshly installed, the copy is the bundle's"
    );
    let shipped = ws.get_agent(&dev).unwrap();

    // The person's edits: the words the bundle owns, and the choices that are
    // theirs to make.
    ws.create_skill(bisa_store::NewSkill {
        id: SkillId::new("house-style").unwrap(),
        name: "House style".into(),
        description: "ours".into(),
        tags: Tags::default(),
        markdown: "# ours".into(),
    })
    .unwrap();
    let mut mine = shipped.clone();
    mine.name = "Dev".into();
    mine.description = Some("mine".into());
    mine.system_prompt = "You are mine.".into();
    mine.respond = bisa_core::RespondPolicy::Members;
    mine.tags = Tags::default();
    mine.harness = "codex".into();
    mine.models = pinned_plan();
    mine.decision_making = true;
    mine.enabled = false;
    mine.skills.retain(|s| s.as_str() != "test-strategy");
    mine.skills.push(SkillId::new("house-style").unwrap());
    let mine = ws.update_agent(mine).unwrap();
    // A skill the agent no longer carries can go — and the update brings it
    // back, since the bundle's agent needs it.
    ws.remove_skill(&SkillId::new("test-strategy").unwrap())
        .unwrap();
    assert_eq!(differs(&ws, CatalogKind::Agent, "developer"), Some(true));

    let updated = ws.update_entry(CatalogKind::Agent, "developer").unwrap();
    assert!(updated.restated, "{updated:?}");
    assert_eq!(
        updated.installed.skills,
        vec!["test-strategy".to_string()],
        "what the entry needs and was missing is installed: {updated:?}"
    );
    assert!(updated.installed.agents.is_empty(), "{updated:?}");
    let after = ws.get_agent(&dev).unwrap();
    assert_eq!(after.name, shipped.name);
    assert_eq!(after.description, shipped.description);
    assert_eq!(after.system_prompt, shipped.system_prompt);
    assert_eq!(after.respond, shipped.respond);
    assert_eq!(after.tags, shipped.tags);
    // The skill the person added stays where it was; the bundle's missing one
    // follows, in the bundle's order.
    let mut want: Vec<SkillId> = mine.skills.clone();
    for s in &shipped.skills {
        if !want.contains(s) {
            want.push(s.clone());
        }
    }
    assert_eq!(after.skills, want);
    // What the person chose stands.
    assert_eq!(after.harness, "codex");
    assert_eq!(after.models, pinned_plan());
    assert!(after.decision_making);
    assert!(!after.enabled, "the switch is the person's");
    assert_eq!(after.pubkey, shipped.pubkey);
    assert_eq!(after.created_at, shipped.created_at);
    assert_eq!(after.origin, shipped.origin);
    assert_eq!(differs(&ws, CatalogKind::Agent, "developer"), Some(false));
    let again = ws.update_entry(CatalogKind::Agent, "developer").unwrap();
    assert!(
        !again.restated && again.installed.is_empty(),
        "nothing to restate twice: {again:?}"
    );
}

#[test]
fn updating_an_installed_skill_takes_the_bundles_markdown_and_keeps_its_birth() {
    let (_d, ws) = ws();
    let id = SkillId::new("plain-voice").unwrap();
    let created = ws.update_entry(CatalogKind::Skill, "plain-voice").unwrap();
    assert!(
        !created.restated && created.installed.skills == vec!["plain-voice".to_string()],
        "an update of an entry that is not here installs it: {created:?}"
    );
    let shipped = ws.get_skill(&id).unwrap();
    assert_eq!(differs(&ws, CatalogKind::Skill, "plain-voice"), Some(false));
    let mut mine = shipped.clone();
    mine.name = "Our voice".into();
    mine.markdown = "# ours\n\nShort.".into();
    mine.tags = Tags::default();
    ws.update_skill(mine).unwrap();
    assert_eq!(differs(&ws, CatalogKind::Skill, "plain-voice"), Some(true));
    let updated = ws.update_entry(CatalogKind::Skill, "plain-voice").unwrap();
    assert!(
        updated.restated && updated.installed.is_empty(),
        "{updated:?}"
    );
    assert_eq!(
        ws.get_skill(&id).unwrap(),
        shipped,
        "the bundle's words, its origin and its birth"
    );
}

#[test]
fn updating_a_team_restores_its_agents_and_keeps_the_people_on_it() {
    let (_d, ws) = ws();
    let created = ws.update_entry(CatalogKind::Team, "design").unwrap();
    assert!(
        !created.restated && created.installed.teams == vec!["design".to_string()],
        "an update of a team that is not here installs it, with its agents: {created:?}"
    );
    let id = TeamId::new("design").unwrap();
    let shipped = ws.get_team(&id).unwrap();
    let person = bisa_core::PrincipalId::new("f".repeat(64)).unwrap();
    let mut mine = shipped.clone();
    mine.purpose = Some("ours".into());
    mine.members
        .retain(|m| !matches!(m, Assignee::Agent(a) if a == "content-strategist"));
    mine.members.push(Assignee::Human(person.clone()));
    mine.enabled = false;
    ws.update_team(mine).unwrap();
    assert_eq!(differs(&ws, CatalogKind::Team, "design"), Some(true));
    let updated = ws.update_entry(CatalogKind::Team, "design").unwrap();
    assert!(
        updated.restated && updated.installed.is_empty(),
        "{updated:?}"
    );
    let after = ws.get_team(&id).unwrap();
    assert_eq!(after.purpose, shipped.purpose);
    let mut want = shipped.members.clone();
    want.push(Assignee::Human(person));
    assert_eq!(
        after.members, want,
        "the bundle's agents in its order, then the people"
    );
    assert!(!after.enabled, "the switch is the person's");
    assert_eq!(after.created_at, shipped.created_at);
    assert_eq!(differs(&ws, CatalogKind::Team, "design"), Some(false));
}

#[test]
fn an_update_of_an_id_that_is_not_this_entrys_or_of_a_kind_installed_once_is_refused_by_name() {
    let (_d, ws) = ws();
    // A channel is installed once and is then the person's.
    ws.install(CatalogKind::Channel, "engineering").unwrap();
    let err = ws
        .update_entry(CatalogKind::Channel, "engineering")
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("channel") && err.contains("cannot update"),
        "{err}"
    );
    assert_eq!(differs(&ws, CatalogKind::Channel, "engineering"), None);
    // A skill of the person's own under a catalog id: the entry is not theirs
    // to restate, and nothing of theirs moves.
    let (_d2, mine) = self::ws();
    mine.create_skill(bisa_store::NewSkill {
        id: SkillId::new("code-review-checklist").unwrap(),
        name: "mine".into(),
        description: "mine".into(),
        tags: Tags::default(),
        markdown: "# mine".into(),
    })
    .unwrap();
    let err = mine
        .update_entry(CatalogKind::Skill, "code-review-checklist")
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("code-review-checklist") && err.contains("one you created here"),
        "{err}"
    );
    assert_eq!(
        mine.get_skill(&SkillId::new("code-review-checklist").unwrap())
            .unwrap()
            .name,
        "mine",
        "nothing moved"
    );
    assert_eq!(
        differs(&mine, CatalogKind::Skill, "code-review-checklist"),
        None,
        "an id held by something else is not this entry's copy"
    );
    // An agent one of whose skills is somebody else's: refused before anything
    // is created, as an install is.
    let err = mine
        .update_entry(CatalogKind::Agent, "code-reviewer")
        .unwrap_err()
        .to_string();
    assert!(err.contains("code-review-checklist"), "{err}");
    assert_eq!(mine.list_agents().unwrap().len(), 2, "nothing was created");
    // A slug the build does not ship.
    assert!(mine
        .update_entry(CatalogKind::Skill, "no-such-skill")
        .is_err());
}
