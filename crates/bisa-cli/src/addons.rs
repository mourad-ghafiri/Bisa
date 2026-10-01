//! `bisa addon`: the overlay widgets installed here, and the switches on
//! each ([18 — Addons](../../../docs/architecture/18-addons.md)).
//!
//! An addon is a folder — `addon.json` and the files its page needs — that
//! lands in the workspace as a record and a bundle. A built-in is installed
//! from the catalog (`bisa catalog list --kind addon`); a person's own is
//! imported from a folder, with the grants they name and nothing more. Every
//! write goes to the running node when there is one, so an open desktop
//! hears `addons.changed` and draws the window; without a node the
//! workspace is written directly.

use crate::ctx::Ctx;
use crate::output::Out;
use anyhow::{anyhow, Result};
use bisa_core::{AddonId, AddonPermission, Origin};
use bisa_store::{AddonEntry, CatalogKind, Workspace};
use clap::Subcommand;
use serde_json::{json, Value};
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum AddonCmd {
    /// List the addons installed here
    List,
    /// Show one addon: its manifest, its grants, whether it runs
    Show { id: String },
    /// Install a built-in from the catalog (`bisa catalog list --kind addon` names them)
    Install { slug: String },
    /// Import an addon from a folder on this machine
    Import {
        path: PathBuf,
        /// Grant a permission the manifest declares (repeatable)
        #[arg(long = "grant")]
        grants: Vec<String>,
        /// Turn it on at once
        #[arg(long)]
        enable: bool,
    },
    /// Turn an addon on
    Enable { id: String },
    /// Turn an addon off
    Disable { id: String },
    /// Remove an addon: its record, its bundle, its snapshot
    #[command(alias = "rm")]
    Remove { id: String },
    /// Grant permissions the manifest declares
    Grant {
        id: String,
        permissions: Vec<String>,
    },
    /// Take permissions back
    Revoke {
        id: String,
        permissions: Vec<String>,
    },
}

fn addon_id(s: &str) -> Result<AddonId> {
    AddonId::new(s).map_err(|e| anyhow!("{e}"))
}

/// The row the node answers and the row the workspace answers, one shape.
fn row(entry: &AddonEntry) -> Value {
    json!({
        "id": entry.id().as_str(),
        "manifest": entry.record.manifest,
        "origin": entry.record.origin,
        "enabled": entry.record.enabled,
        "granted": entry.record.granted,
        "installed_at": entry.record.installed_at,
        "files_present": entry.files_present,
        "active": entry.is_active(),
    })
}

fn state_words(addon: &Value) -> String {
    let text = if addon["active"].as_bool().unwrap_or(false) {
        bisa_core::text!("cli-addons-state-running")
    } else if !addon["files_present"].as_bool().unwrap_or(false) {
        bisa_core::text!("cli-addons-state-files-missing")
    } else {
        bisa_core::text!("cli-addons-state-off")
    };
    bisa_i18n::say(&text)
}

fn origin_words(addon: &Value) -> String {
    match serde_json::from_value::<Origin>(addon["origin"].clone()).unwrap_or_default() {
        Origin::Catalog { slug } => {
            bisa_i18n::say(&bisa_core::text!("cli-addons-origin-catalog", slug = slug))
        }
        Origin::Local => bisa_i18n::say(&bisa_core::text!("cli-addons-origin-local")),
    }
}

fn words_of(permissions: &[AddonPermission]) -> String {
    if permissions.is_empty() {
        return bisa_i18n::say(&bisa_core::text!("cli-addons-permissions-none"));
    }
    permissions
        .iter()
        .map(|p| match p {
            AddonPermission::Network { hosts } => format!("network ({})", hosts.join(", ")),
            other => other.word().to_string(),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn permissions_in(addon: &Value, field: &str) -> Vec<AddonPermission> {
    serde_json::from_value(addon[field].clone()).unwrap_or_default()
}

fn render_row(out: &Out, addon: &Value) {
    out.say(&bisa_core::text!(
        "cli-addons-row",
        id = addon["id"].as_str().unwrap_or_default().to_string(),
        name = addon["manifest"]["name"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        version = addon["manifest"]["version"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        state = state_words(addon),
        origin = origin_words(addon)
    ));
}

fn render_show(out: &Out, addon: &Value) {
    render_row(out, addon);
    let declared = permissions_in(addon, "granted");
    let all: Vec<AddonPermission> = permissions_in(&addon["manifest"], "permissions");
    out.say(&bisa_core::text!(
        "cli-addons-show",
        description = addon["manifest"]["description"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        license = addon["manifest"]["license"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        declared = words_of(&all),
        granted = words_of(&declared),
        width = addon["manifest"]["window"]["width"].to_string(),
        height = addon["manifest"]["window"]["height"].to_string()
    ));
}

/// The declared permissions the words name, exactly as declared — a word the
/// manifest never declared is refused with the list it does.
fn resolve_words(words: &[String], declared: &[AddonPermission]) -> Result<Vec<AddonPermission>> {
    let mut out = Vec::new();
    for word in words {
        match AddonPermission::grant_by_word(word, declared) {
            Some(p) => {
                if !out.contains(p) {
                    out.push(p.clone());
                }
            }
            None => {
                return Err(anyhow!(bisa_core::text!(
                    "cli-addons-unknown-permission",
                    word = word.clone(),
                    a0 = words_of(declared)
                )))
            }
        }
    }
    Ok(out)
}

/// One addon as JSON, from the node or the workspace.
async fn fetch_row(ctx: &Ctx, id: &AddonId) -> Result<Value> {
    if let Some(client) = ctx.node_client().await {
        let v = client.get(&format!("/addons/{id}")).await?;
        return Ok(v["addon"].clone());
    }
    let ws = ctx.workspace()?;
    Ok(row(&ws.get_addon(id)?))
}

async fn patch(ctx: &Ctx, id: &AddonId, body: Value) -> Result<Value> {
    if let Some(client) = ctx.node_client().await {
        let v = client.patch(&format!("/addons/{id}"), body).await?;
        return Ok(v["addon"].clone());
    }
    let ws: Workspace = ctx.workspace()?;
    let mut entry = None;
    if let Some(enabled) = body["enabled"].as_bool() {
        entry = Some(ws.set_addon_enabled(id, enabled)?);
    }
    if !body["granted"].is_null() {
        let granted: Vec<AddonPermission> = serde_json::from_value(body["granted"].clone())?;
        entry = Some(ws.set_addon_grants(id, granted)?);
    }
    Ok(row(&match entry {
        Some(e) => e,
        None => ws.get_addon(id)?,
    }))
}

pub async fn addon(ctx: &Ctx, out: &Out, cmd: AddonCmd) -> Result<()> {
    match cmd {
        AddonCmd::List => {
            let addons: Vec<Value> = if let Some(client) = ctx.node_client().await {
                client.get("/addons").await?["addons"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
            } else {
                ctx.workspace()?.list_addons()?.iter().map(row).collect()
            };
            if addons.is_empty() {
                out.say(&bisa_core::text!("cli-addons-none-installed"));
            }
            for a in &addons {
                render_row(out, a);
            }
            out.json_value(json!({"addons": addons}));
        }
        AddonCmd::Show { id } => {
            let a = fetch_row(ctx, &addon_id(&id)?).await?;
            render_show(out, &a);
            out.json_value(json!({"addon": a}));
        }
        AddonCmd::Install { slug } => {
            let installed: Vec<String> = if let Some(client) = ctx.node_client().await {
                let v = client
                    .post("/catalog/install", json!({"kind": "addon", "slug": slug}))
                    .await?;
                serde_json::from_value(v["installed"]["addons"].clone()).unwrap_or_default()
            } else {
                ctx.workspace()?.install(CatalogKind::Addon, &slug)?.addons
            };
            if installed.is_empty() {
                out.say(&bisa_core::text!(
                    "cli-addons-already-installed",
                    slug = slug.clone()
                ));
            } else {
                out.say(&bisa_core::text!("cli-addons-installed", id = slug.clone()));
            }
            out.json_value(json!({"installed": installed}));
        }
        AddonCmd::Import {
            path,
            grants,
            enable,
        } => {
            let path = path
                .canonicalize()
                .map_err(|e| anyhow!("{}: {e}", path.display()))?;
            // The words are resolved against the folder's own manifest, so a
            // word it never declared is refused here before anything is copied.
            let manifest_path = path.join(bisa_core::ADDON_MANIFEST_FILE_NAME);
            // A folder with no manifest, or one that is no manifest, is said
            // by its file — never as a parser's words alone.
            let manifest: bisa_core::AddonManifest = serde_json::from_slice(
                &std::fs::read(&manifest_path)
                    .map_err(|e| anyhow!("{}: {e}", manifest_path.display()))?,
            )
            .map_err(|e| anyhow!("{}: {e}", manifest_path.display()))?;
            let granted = resolve_words(&grants, &manifest.permissions)?;
            let a = if let Some(client) = ctx.node_client().await {
                client
                    .post(
                        "/addons",
                        json!({"path": path.to_string_lossy(), "granted": granted, "enabled": enable}),
                    )
                    .await?["addon"]
                    .clone()
            } else {
                row(&ctx.workspace()?.install_addon(&path, granted, enable)?)
            };
            out.say(&bisa_core::text!(
                "cli-addons-imported",
                id = a["id"].as_str().unwrap_or_default().to_string(),
                state = state_words(&a)
            ));
            out.json_value(json!({"addon": a}));
        }
        AddonCmd::Enable { id } => {
            let a = patch(ctx, &addon_id(&id)?, json!({"enabled": true})).await?;
            out.say(&bisa_core::text!("cli-addons-enabled", id = id.clone()));
            out.json_value(json!({"addon": a}));
        }
        AddonCmd::Disable { id } => {
            let a = patch(ctx, &addon_id(&id)?, json!({"enabled": false})).await?;
            out.say(&bisa_core::text!("cli-addons-disabled", id = id.clone()));
            out.json_value(json!({"addon": a}));
        }
        AddonCmd::Remove { id } => {
            let aid = addon_id(&id)?;
            if let Some(client) = ctx.node_client().await {
                client.delete(&format!("/addons/{aid}")).await?;
            } else {
                ctx.workspace()?.remove_addon(&aid)?;
            }
            out.say(&bisa_core::text!("cli-addons-removed", id = id.clone()));
            out.json_value(json!({"removed": id}));
        }
        AddonCmd::Grant { id, permissions } => {
            let aid = addon_id(&id)?;
            let current = fetch_row(ctx, &aid).await?;
            let declared: Vec<AddonPermission> =
                permissions_in(&current["manifest"], "permissions");
            let mut granted = permissions_in(&current, "granted");
            for p in resolve_words(&permissions, &declared)? {
                if !granted.contains(&p) {
                    granted.push(p);
                }
            }
            let a = patch(ctx, &aid, json!({"granted": granted})).await?;
            out.say(&bisa_core::text!(
                "cli-addons-granted",
                id = id.clone(),
                words = words_of(&permissions_in(&a, "granted"))
            ));
            out.json_value(json!({"addon": a}));
        }
        AddonCmd::Revoke { id, permissions } => {
            let aid = addon_id(&id)?;
            let current = fetch_row(ctx, &aid).await?;
            let declared: Vec<AddonPermission> =
                permissions_in(&current["manifest"], "permissions");
            let taken = resolve_words(&permissions, &declared)?;
            let granted: Vec<AddonPermission> = permissions_in(&current, "granted")
                .into_iter()
                .filter(|p| !taken.contains(p))
                .collect();
            let a = patch(ctx, &aid, json!({"granted": granted})).await?;
            out.say(&bisa_core::text!(
                "cli-addons-revoked",
                id = id.clone(),
                words = words_of(&permissions_in(&a, "granted"))
            ));
            out.json_value(json!({"addon": a}));
        }
    }
    Ok(())
}
