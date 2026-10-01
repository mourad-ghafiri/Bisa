//! `bisa settings …` — the three scopes from the command line.
//!
//! Reads go to the workspace directly; writes go through the engine (the
//! running node when there is one), so the change is announced on the bus.

use crate::ctx::Ctx;
use crate::output::Out;
use anyhow::{bail, Context, Result};
use bisa_core::{ProjectId, SettingScope};
use clap::Subcommand;
use serde_json::json;

#[derive(Subcommand, Debug)]
pub enum SettingsCmd {
    /// Every key, its value and where it came from
    Show {
        /// Resolve for a project (its project-scope layer applies)
        #[arg(long)]
        project: Option<String>,
        /// Only keys under this group (`editor`, `git`, …)
        #[arg(long)]
        group: Option<String>,
    },
    /// One key's resolved value
    Get {
        key: String,
        #[arg(long)]
        project: Option<String>,
    },
    /// Write one key at one scope: `machine`, `workspace` or `project`
    Set {
        scope: String,
        key: String,
        /// JSON when it parses (`4`, `true`, `"on"`), the bare string otherwise
        value: String,
        /// Required with the `project` scope
        #[arg(long)]
        project: Option<String>,
    },
    /// Remove one key from one scope; the value falls back to the next layer
    Unset {
        scope: String,
        key: String,
        #[arg(long)]
        project: Option<String>,
    },
    /// The registry: every key, its kind, default and allowed scopes
    Registry,
}

fn parse_scope(raw: &str) -> Result<SettingScope> {
    Ok(match raw {
        "machine" => SettingScope::Machine,
        "workspace" => SettingScope::Workspace,
        "project" => SettingScope::Project,
        other => bail!(bisa_core::text!(
            "cli-settings-unknown-scope-machine-workspace-project",
            other = format!("{other:?}")
        )),
    })
}

fn parse_project(raw: &Option<String>) -> Result<Option<ProjectId>> {
    match raw {
        None => Ok(None),
        Some(s) => s
            .parse::<ProjectId>()
            .map(Some)
            .with_context(|| bisa_core::text!("cli-settings-not-project-id", s = format!("{s:?}"))),
    }
}

fn parse_value(raw: &str) -> serde_json::Value {
    serde_json::from_str(raw).unwrap_or_else(|_| serde_json::Value::String(raw.to_string()))
}

pub async fn settings(ctx: &Ctx, out: &Out, cmd: SettingsCmd) -> Result<()> {
    match cmd {
        SettingsCmd::Show { project, group } => {
            let ws = ctx.workspace()?;
            let rows = ws.settings(parse_project(&project)?)?;
            let rows: Vec<_> = rows
                .into_iter()
                .filter(|r| {
                    group
                        .as_ref()
                        .is_none_or(|g| r.key.starts_with(&format!("{g}.")))
                })
                .collect();
            for r in &rows {
                out.human(&format!(
                    "{:<40} {:<10} {}",
                    r.key,
                    format!("{:?}", r.origin).to_lowercase(),
                    r.value
                ));
            }
            out.json_value(json!({"settings": rows}));
        }
        SettingsCmd::Get { key, project } => {
            let ws = ctx.workspace()?;
            let r = ws.setting(&key, parse_project(&project)?)?;
            out.human(&format!("{} = {} ({:?})", r.key, r.value, r.origin));
            out.json_value(json!({"setting": r}));
        }
        SettingsCmd::Set {
            scope,
            key,
            value,
            project,
        } => {
            let scope = parse_scope(&scope)?;
            let value = parse_value(&value);
            let resolved = if let Some(node) = ctx.node_client().await {
                let q = project
                    .as_ref()
                    .map(|p| format!("?project={p}"))
                    .unwrap_or_default();
                let v = node
                    .put(
                        &format!("/settings/{}{q}", scope.as_str()),
                        json!({"values": {key.clone(): value}}),
                    )
                    .await?;
                serde_json::from_value::<Vec<bisa_core::ResolvedSetting>>(v["settings"].clone())?
                    .into_iter()
                    .next()
                    .context(bisa_core::text!("cli-settings-node-wrote-nothing"))?
            } else {
                let (engine, _) = ctx.engine().await?;
                let r = engine.set_setting(scope, parse_project(&project)?, &key, value)?;
                engine.shutdown().await;
                r
            };
            out.human(&format!(
                "{} = {} ({:?})",
                resolved.key, resolved.value, resolved.origin
            ));
            out.json_value(json!({"setting": resolved}));
        }
        SettingsCmd::Unset {
            scope,
            key,
            project,
        } => {
            let scope = parse_scope(&scope)?;
            let resolved = if let Some(node) = ctx.node_client().await {
                let q = project
                    .as_ref()
                    .map(|p| format!("?project={p}"))
                    .unwrap_or_default();
                let v = node
                    .delete(&format!("/settings/{}/{key}{q}", scope.as_str()))
                    .await?;
                serde_json::from_value(v["setting"].clone())?
            } else {
                let (engine, _) = ctx.engine().await?;
                let r = engine.unset_setting(scope, parse_project(&project)?, &key)?;
                engine.shutdown().await;
                r
            };
            out.human(&format!(
                "{} = {} ({:?})",
                resolved.key, resolved.value, resolved.origin
            ));
            out.json_value(json!({"setting": resolved}));
        }
        SettingsCmd::Registry => {
            // The words are the catalog's, in the CLI's language (17).
            for d in bisa_core::SETTINGS.iter() {
                out.say(&bisa_core::text!(
                    "cli-settings-default-scopes",
                    a0 = format!("{:<40}", d.key),
                    a1 = format!("{:<12}", d.default.to_string()),
                    a2 = format!(
                        "{:?}",
                        d.scopes
                            .list()
                            .iter()
                            .map(|s| s.as_str())
                            .collect::<Vec<_>>()
                    ),
                    a3 = (out.text(&d.text())).to_string()
                ));
            }
            let defs: Vec<_> = bisa_core::SETTINGS
                .iter()
                .map(|d| {
                    json!({
                        "key": d.key,
                        "default": d.default,
                        "scopes": d.scopes.list(),
                        "text": d.text(),
                        "label": out.text(&d.text()),
                        "help": out.attribute(&d.message_id(), "help").unwrap_or_default(),
                    })
                })
                .collect();
            out.json_value(json!({"settings": defs}));
        }
    }
    Ok(())
}
