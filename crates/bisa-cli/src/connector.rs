//! `bisa connector …`: the connectors this workspace holds, a person's
//! own definition from a file, and the **accounts** on this machine — added
//! with their secrets read once and never printed, checked against the
//! platform, made the default, forgotten, or connected through the
//! platform's OAuth consent page.
//!
//! Reads come from the store; writes go through the node when one runs and
//! through an embedded engine otherwise, so the same door records an
//! account either way. A secret is best given as `field=@stdin`: a value on
//! the command line lands in the shell's history.

use crate::ctx::Ctx;
use crate::output::Out;
use anyhow::{anyhow, bail, Context as _, Result};
use bisa_core::{AccountId, Connector, ConnectorId, SecretField};
use bisa_store::catalog::parse_connector;
use bisa_store::{NewConnector, NewConnectorAccount};
use clap::Subcommand;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;

#[derive(Subcommand)]
pub enum ConnectorCmd {
    /// List the connectors here — the catalog's and your own — with how many
    /// accounts this machine has for each
    List,
    /// One connector: its hosts, auth, operations and this machine's accounts
    Show { id: String },
    /// Record your own connector definition from a file (`-` reads stdin):
    /// TOML in the catalog's shape under `[connector]`, or JSON
    New {
        #[arg(long)]
        from: String,
        /// The id to record it under (the file's stem by default)
        #[arg(long)]
        id: Option<String>,
    },
    /// Forget a definition. Refused while an account or a workflow step names it.
    Rm { id: String },
    /// This machine's accounts for a connector — never a value
    Accounts { id: String },
    /// An account: add, check, make the default, forget
    Account {
        #[command(subcommand)]
        command: AccountCmd,
    },
    /// Connect an OAuth account: prints the consent URL to open, then reads
    /// the code the platform showed you and finishes the connection
    Connect { id: String, account: String },
}

#[derive(Subcommand)]
pub enum AccountCmd {
    /// Add an account with its secrets. `--secret field=@stdin` reads the
    /// value from standard input; `field=VALUE` is accepted but lands in
    /// your shell's history — prefer `@stdin`.
    Add {
        connector: String,
        #[arg(long)]
        label: String,
        /// A non-secret parameter the connector declares, `k=v`
        #[arg(long = "param")]
        params: Vec<String>,
        /// A secret field, `field=@stdin`, `field=@path` or `field=VALUE`
        #[arg(long = "secret")]
        secrets: Vec<String>,
        /// Make it the connector's default account
        #[arg(long)]
        default: bool,
    },
    /// One live request as the account — the connector's `check` operation
    Check { connector: String, account: String },
    /// Make the account the connector's default
    Default { connector: String, account: String },
    /// Forget the account and every secret it held
    Rm { connector: String, account: String },
}

pub async fn connector(ctx: &Ctx, out: &Out, cmd: ConnectorCmd) -> Result<()> {
    match cmd {
        ConnectorCmd::List => list(ctx, out),
        ConnectorCmd::Show { id } => show(ctx, out, &id),
        ConnectorCmd::New { from, id } => new(ctx, out, &from, id.as_deref()).await,
        ConnectorCmd::Rm { id } => rm(ctx, out, &id).await,
        ConnectorCmd::Accounts { id } => accounts(ctx, out, &id),
        ConnectorCmd::Account { command } => match command {
            AccountCmd::Add {
                connector,
                label,
                params,
                secrets,
                default,
            } => add_account(ctx, out, &connector, label, &params, &secrets, default).await,
            AccountCmd::Check { connector, account } => check(ctx, out, &connector, &account).await,
            AccountCmd::Default { connector, account } => {
                make_default(ctx, out, &connector, &account).await
            }
            AccountCmd::Rm { connector, account } => {
                rm_account(ctx, out, &connector, &account).await
            }
        },
        ConnectorCmd::Connect { id, account } => connect(ctx, out, &id, &account).await,
    }
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

fn connector_id(raw: &str) -> Result<ConnectorId> {
    ConnectorId::new(raw.trim()).map_err(|e| anyhow!("{raw:?}: {e}"))
}

fn account_id(raw: &str) -> Result<AccountId> {
    raw.trim().parse::<AccountId>().map_err(|_| {
        anyhow!(bisa_core::text!(
            "cli-connector-not-account-id-bisa-connector-accounts",
            raw = format!("{raw:?}")
        ))
    })
}

/// A connector definition as a JSON file spells it: the catalog's shape,
/// plus the id.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DefinitionJson {
    id: Option<String>,
    name: String,
    description: String,
    #[serde(default)]
    tags: Vec<String>,
    base_url: String,
    hosts: Vec<String>,
    #[serde(default)]
    insecure_tls: bool,
    auth: bisa_core::AuthScheme,
    #[serde(default)]
    params: Vec<bisa_core::ParamDef>,
    operations: Vec<bisa_core::Operation>,
    #[serde(default)]
    check: Option<bisa_core::OperationId>,
}

/// A definition from a file or stdin: TOML through the catalog's parser,
/// anything else as JSON. The id is `--id`, the JSON's `id`, or the file's
/// stem.
fn read_definition(src: &str, id: Option<&str>) -> Result<NewConnector> {
    let text = if src == "-" {
        std::io::read_to_string(std::io::stdin()).context(bisa_core::text!(
            "cli-connector-reading-definition-from-stdin"
        ))?
    } else {
        std::fs::read_to_string(src).with_context(|| format!("reading {src}"))?
    };
    let stem = std::path::Path::new(src)
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| *s != "-")
        .map(str::to_string);
    if src.ends_with(".toml") {
        let slug = id.map(str::to_string).or(stem).ok_or_else(|| {
            anyhow!(bisa_core::text!(
                "cli-connector-say-which-id-record-under-id"
            ))
        })?;
        return Ok(parse_connector(&slug, &text)?.into_new(&slug)?);
    }
    let parsed: DefinitionJson = serde_json::from_str(&text).with_context(|| {
        bisa_i18n::say(&bisa_core::text!(
            "cli-connector-not-connector-definition",
            src = src.to_string()
        ))
    })?;
    let slug = id
        .map(str::to_string)
        .or(parsed.id)
        .or(stem)
        .ok_or_else(|| {
            anyhow!(bisa_core::text!(
                "cli-connector-say-which-id-record-under-id"
            ))
        })?;
    Ok(NewConnector {
        id: connector_id(&slug)?,
        name: parsed.name,
        description: parsed.description,
        tags: bisa_core::Tags::new(parsed.tags)?,
        base_url: parsed.base_url,
        hosts: parsed.hosts,
        insecure_tls: parsed.insecure_tls,
        auth: parsed.auth,
        params: parsed.params,
        operations: parsed.operations,
        check: parsed.check,
    })
}

/// What the node's `POST /connectors` takes, from what the store takes.
fn definition_json(new: &NewConnector) -> Value {
    json!({
        "id": new.id,
        "name": new.name,
        "description": new.description,
        "tags": new.tags.as_slice(),
        "base_url": new.base_url,
        "hosts": new.hosts,
        "insecure_tls": new.insecure_tls,
        "auth": new.auth,
        "params": new.params,
        "operations": new.operations,
        "check": new.check,
    })
}

/// `k=v` pairs; a value is JSON when it parses and text otherwise.
fn parse_params(pairs: &[String]) -> Result<BTreeMap<String, Value>> {
    let mut out = BTreeMap::new();
    for pair in pairs {
        let (k, v) = pair.split_once('=').ok_or_else(|| {
            anyhow!(bisa_core::text!(
                "cli-connector-param-wants-k-v-got",
                pair = format!("{pair:?}")
            ))
        })?;
        let value = serde_json::from_str(v).unwrap_or_else(|_| Value::String(v.to_string()));
        out.insert(k.trim().to_string(), value);
    }
    Ok(out)
}

/// `field=@stdin`, `field=@path` or `field=VALUE`; stdin may be read once.
/// A file is how a PEM key arrives — `private_key=@AuthKey.p8` — and a value
/// that only *looks* like a path is given through stdin.
fn parse_secrets(pairs: &[String]) -> Result<BTreeMap<SecretField, String>> {
    let mut out = BTreeMap::new();
    let mut stdin_used = false;
    for pair in pairs {
        let (k, v) = pair.split_once('=').ok_or_else(|| {
            anyhow!(bisa_core::text!(
                "cli-connector-secret-wants-field-stdin-field-path",
                pair = format!("{pair:?}")
            ))
        })?;
        let field = SecretField::parse(k.trim()).ok_or_else(|| {
            anyhow!(bisa_core::text!(
                "cli-connector-not-secret-field",
                k = format!("{k:?}"),
                a0 = (field_words()).to_string()
            ))
        })?;
        let value = if v == "@stdin" {
            if stdin_used {
                bail!(bisa_core::text!(
                    "cli-connector-only-one-secret-can-be-read"
                ));
            }
            stdin_used = true;
            std::io::read_to_string(std::io::stdin())
                .context(bisa_core::text!("cli-connector-reading-secret-from-stdin"))?
                .trim()
                .to_string()
        } else if let Some(path) = v.strip_prefix('@') {
            std::fs::read_to_string(path)
                .with_context(|| {
                    bisa_core::text!(
                        "cli-connector-reading-secret-from",
                        k = format!("{k:?}"),
                        path = path.to_string()
                    )
                })?
                .trim()
                .to_string()
        } else {
            v.to_string()
        };
        if value.is_empty() {
            bail!(bisa_core::text!(
                "cli-connector-secret-empty",
                k = format!("{k:?}")
            ));
        }
        out.insert(field, value);
    }
    Ok(out)
}

fn field_words() -> String {
    SecretField::ALL
        .iter()
        .map(|f| f.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

fn connector_line(c: &Connector, accounts: usize) -> String {
    bisa_i18n::say(&bisa_core::text!(
        "cli-connector-op-s-account-s",
        a0 = format!("{:<18}", c.id),
        a1 = format!("{:<8}", c.auth.word()),
        a2 = format!("{:>2}", c.operations.len()),
        a3 = (accounts).to_string(),
        a4 = (match &c.origin {
            bisa_core::Origin::Local => "yours".to_string(),
            bisa_core::Origin::Catalog { slug } => format!("catalog:{slug}"),
        })
        .to_string(),
        a5 = (c.description).to_string()
    ))
}

/// One account row: the label, the id, the default mark, which secret fields
/// are set — and never a value.
fn account_line(a: &bisa_core::ConnectorAccount) -> String {
    format!(
        "{}{:<20} {}  secrets: {}{}",
        if a.default { "★ " } else { "  " },
        a.label,
        a.id,
        if a.auth.fields_set.is_empty() {
            "none".to_string()
        } else {
            a.auth
                .fields_set
                .iter()
                .map(|f| f.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        },
        match a.auth.expires_at {
            Some(at) => bisa_i18n::say(&bisa_core::text!(
                "cli-connector-token-expires",
                at = at.to_string()
            )),
            None => String::new(),
        }
    )
}

fn account_json(a: &bisa_core::ConnectorAccount) -> Value {
    json!({
        "id": a.id,
        "connector": a.connector,
        "label": a.label,
        "params": a.params,
        "default": a.default,
        "secrets_set": a.auth.fields_set,
        "expires_at": a.auth.expires_at,
        "scope": a.auth.scope,
    })
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

fn list(ctx: &Ctx, out: &Out) -> Result<()> {
    let ws = ctx.workspace()?;
    let connectors = ws.list_connectors()?;
    let mut rows = Vec::new();
    for c in &connectors {
        let accounts = ws.list_connector_accounts(&c.id)?;
        out.human(&connector_line(c, accounts.len()));
        rows.push(json!({ "connector": c, "accounts": accounts.len() }));
    }
    if connectors.is_empty() {
        out.say(&bisa_core::text!(
            "cli-connector-no-connectors-yet-bisa-catalog-install"
        ));
    }
    out.json_value(json!({ "connectors": rows }));
    Ok(())
}

fn show(ctx: &Ctx, out: &Out, id: &str) -> Result<()> {
    let ws = ctx.workspace()?;
    let cid = connector_id(id)?;
    let c = ws.get_connector(&cid)?;
    let accounts = ws.list_connector_accounts(&cid)?;
    out.human(&format!("{} — {}", c.id, c.name));
    out.human(&format!("  {}", c.description));
    out.say(&bisa_core::text!(
        "cli-connector-auth-base-hosts",
        a0 = (c.auth.word()).to_string(),
        a1 = (c.base_url).to_string(),
        a2 = (c.hosts.join(", ")).to_string()
    ));
    if !c.params.is_empty() {
        out.say(&bisa_core::text!(
            "cli-connector-account-params",
            a0 = (c
                .params
                .iter()
                .map(|p| format!("{}{}", p.name, if p.required { "*" } else { "" }))
                .collect::<Vec<_>>()
                .join(", "))
            .to_string()
        ));
    }
    out.say(&bisa_core::text!("cli-connector-operations"));
    for op in &c.operations {
        let mut facts = Vec::new();
        if op.writes {
            facts.push("writes".to_string());
        }
        if let Some(secs) = op.timeout_secs {
            facts.push(bisa_i18n::say(&bisa_core::text!(
                "cli-connector-timeout-s",
                secs = secs.to_string()
            )));
        }
        if let Some(idem) = &op.idempotency {
            facts.push(format!("idempotency {}", idem.header));
        }
        if let Some(page) = &op.page {
            facts.push(bisa_i18n::say(&bisa_core::text!(
                "cli-connector-pages",
                a0 = (page.cursor_param).to_string(),
                a1 = (page.next_cursor).to_string(),
                a2 = (page.max_pages).to_string()
            )));
        }
        out.human(&format!(
            "    {:<18} {:<6} {}{}  {}",
            op.id,
            op.method.as_str().to_uppercase(),
            op.path,
            if facts.is_empty() {
                String::new()
            } else {
                format!("  ({})", facts.join("; "))
            },
            op.description
        ));
        for p in &op.params {
            out.human(&format!(
                "      {}{} ({}): {}",
                p.name,
                if p.required { "*" } else { "" },
                serde_json::to_value(p.kind)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_string))
                    .unwrap_or_default(),
                p.doc
            ));
        }
    }
    out.say(&bisa_core::text!(
        "cli-connector-accounts",
        a0 = (accounts.len()).to_string()
    ));
    for a in &accounts {
        out.human(&format!("  {}", account_line(a)));
    }
    out.json_value(json!({
        "connector": c,
        "accounts": accounts.iter().map(account_json).collect::<Vec<_>>(),
    }));
    Ok(())
}

async fn new(ctx: &Ctx, out: &Out, from: &str, id: Option<&str>) -> Result<()> {
    let new = read_definition(from, id)?;
    if let Some(client) = ctx.node_client().await {
        let v = client.post("/connectors", definition_json(&new)).await?;
        out.say(&bisa_core::text!(
            "cli-connector-connector-recorded",
            a0 = (v["connector"]["id"]).to_string()
        ));
        out.json_value(v);
        return Ok(());
    }
    let (engine, ()) = ctx.engine().await?;
    let created = bisa_engine::connectors::create_connector(engine.inner(), new)?;
    out.say(&bisa_core::text!(
        "cli-connector-connector-recorded-operation-s",
        a0 = (created.id).to_string(),
        a1 = (created.operations.len()).to_string()
    ));
    out.json_value(json!({ "connector": created }));
    engine.shutdown().await;
    Ok(())
}

async fn rm(ctx: &Ctx, out: &Out, id: &str) -> Result<()> {
    let cid = connector_id(id)?;
    if let Some(client) = ctx.node_client().await {
        let v = client.delete(&format!("/connectors/{cid}")).await?;
        out.say(&bisa_core::text!(
            "cli-connector-connector-removed",
            cid = cid.to_string()
        ));
        out.json_value(v);
        return Ok(());
    }
    let (engine, ()) = ctx.engine().await?;
    bisa_engine::connectors::remove_connector(engine.inner(), &cid)?;
    out.say(&bisa_core::text!(
        "cli-connector-connector-removed",
        cid = cid.to_string()
    ));
    out.json_value(json!({ "connector": cid, "removed": true }));
    engine.shutdown().await;
    Ok(())
}

fn accounts(ctx: &Ctx, out: &Out, id: &str) -> Result<()> {
    let ws = ctx.workspace()?;
    let cid = connector_id(id)?;
    ws.get_connector(&cid)?;
    let accounts = ws.list_connector_accounts(&cid)?;
    for a in &accounts {
        out.human(&account_line(a));
    }
    if accounts.is_empty() {
        out.say(&bisa_core::text!(
            "cli-connector-no-accounts-machine-bisa-connector-account",
            cid = cid.to_string()
        ));
    }
    out.json_value(json!({ "accounts": accounts.iter().map(account_json).collect::<Vec<_>>() }));
    Ok(())
}

async fn add_account(
    ctx: &Ctx,
    out: &Out,
    connector: &str,
    label: String,
    params: &[String],
    secrets: &[String],
    default: bool,
) -> Result<()> {
    let cid = connector_id(connector)?;
    let params = parse_params(params)?;
    let secrets = parse_secrets(secrets)?;
    let fields: Vec<&str> = secrets.keys().map(|f| f.as_str()).collect();
    if let Some(client) = ctx.node_client().await {
        let secret_map: BTreeMap<&str, &str> = secrets
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        let v = client
            .put(
                &format!("/connectors/{cid}/accounts"),
                json!({ "label": label, "params": params, "secrets": secret_map, "default": default }),
            )
            .await?;
        out.say(&bisa_core::text!(
            "cli-connector-account-added-secrets-set",
            a0 = (v["id"].as_str().unwrap_or("?")).to_string(),
            a1 = (v["label"].as_str().unwrap_or("")).to_string(),
            cid = cid.to_string(),
            a2 = (if fields.is_empty() {
                "none".to_string()
            } else {
                fields.join(", ")
            })
            .to_string()
        ));
        out.json_value(v);
        return Ok(());
    }
    let (engine, ()) = ctx.engine().await?;
    let account = bisa_engine::connectors::put_account(
        engine.inner(),
        NewConnectorAccount {
            connector: cid.clone(),
            label,
            params,
            default,
        },
        secrets,
    )?;
    out.say(&bisa_core::text!(
        "cli-connector-account-added-secrets-set",
        a0 = (account.id).to_string(),
        a1 = (account.label).to_string(),
        cid = cid.to_string(),
        a2 = (if fields.is_empty() {
            "none".to_string()
        } else {
            fields.join(", ")
        })
        .to_string()
    ));
    out.json_value(account_json(&account));
    engine.shutdown().await;
    Ok(())
}

async fn check(ctx: &Ctx, out: &Out, connector: &str, account: &str) -> Result<()> {
    let cid = connector_id(connector)?;
    let aid = account_id(account)?;
    let v = if let Some(client) = ctx.node_client().await {
        client
            .post(
                &format!("/connectors/{cid}/accounts/{aid}/check"),
                json!({}),
            )
            .await?
    } else {
        let (engine, ()) = ctx.engine().await?;
        let check = bisa_engine::connectors::check_account(engine.inner(), &cid, aid).await?;
        engine.shutdown().await;
        serde_json::to_value(check)?
    };
    out.human(&format!(
        "{}{}{}",
        v["state"].as_str().unwrap_or("?"),
        v["status"]
            .as_u64()
            .map(|s| format!(" ({s})"))
            .unwrap_or_default(),
        v["reason"]
            .as_str()
            .map(|r| format!(": {r}"))
            .unwrap_or_default()
    ));
    out.json_value(v);
    Ok(())
}

async fn make_default(ctx: &Ctx, out: &Out, connector: &str, account: &str) -> Result<()> {
    let cid = connector_id(connector)?;
    let aid = account_id(account)?;
    if let Some(client) = ctx.node_client().await {
        let v = client
            .put(
                &format!("/connectors/{cid}/accounts/{aid}/default"),
                json!({}),
            )
            .await?;
        out.say(&bisa_core::text!(
            "cli-connector-now-default-account",
            aid = aid.to_string(),
            cid = cid.to_string()
        ));
        out.json_value(v);
        return Ok(());
    }
    let (engine, ()) = ctx.engine().await?;
    bisa_engine::connectors::set_default_account(engine.inner(), &cid, aid)?;
    out.say(&bisa_core::text!(
        "cli-connector-now-default-account",
        aid = aid.to_string(),
        cid = cid.to_string()
    ));
    out.json_value(json!({ "default": aid }));
    engine.shutdown().await;
    Ok(())
}

async fn rm_account(ctx: &Ctx, out: &Out, connector: &str, account: &str) -> Result<()> {
    let cid = connector_id(connector)?;
    let aid = account_id(account)?;
    if let Some(client) = ctx.node_client().await {
        let v = client
            .delete(&format!("/connectors/{cid}/accounts/{aid}"))
            .await?;
        out.say(&bisa_core::text!(
            "cli-connector-account-forgotten-with-secrets",
            aid = aid.to_string()
        ));
        out.json_value(v);
        return Ok(());
    }
    let (engine, ()) = ctx.engine().await?;
    bisa_engine::connectors::delete_account(engine.inner(), &cid, aid)?;
    out.say(&bisa_core::text!(
        "cli-connector-account-forgotten-with-secrets",
        aid = aid.to_string()
    ));
    out.json_value(json!({ "account": aid, "forgotten": true }));
    engine.shutdown().await;
    Ok(())
}

/// The OAuth consent flow from a terminal: start it, print the URL, and
/// finish it with the code the platform shows — the browser's redirect lands
/// on the node's own listener when the node runs, so the paste is only
/// needed when it does not.
async fn connect(ctx: &Ctx, out: &Out, connector: &str, account: &str) -> Result<()> {
    let cid = connector_id(connector)?;
    let aid = account_id(account)?;
    let Some(client) = ctx.node_client().await else {
        bail!(bisa_core::text!(
            "cli-connector-connecting-oauth-account-needs-node-hosts"
        ));
    };
    let started = client
        .post(
            &format!("/connectors/{cid}/accounts/{aid}/oauth/start"),
            json!({}),
        )
        .await?;
    let url = started["url"].as_str().unwrap_or_default().to_string();
    out.say(&bisa_core::text!(
        "cli-connector-open-url-your-browser-approve-connection"
    ));
    out.human(&format!("  {url}"));
    out.human("");
    out.say(&bisa_core::text!(
        "cli-connector-when-browser-lands-node-s-page"
    ));
    out.say(&bisa_core::text!(
        "cli-connector-shows-you-code-instead-paste-here"
    ));
    let code = std::io::read_to_string(std::io::stdin())
        .context(bisa_core::text!("cli-connector-reading-code"))?
        .trim()
        .to_string();
    if code.is_empty() {
        out.say(&bisa_core::text!(
            "cli-connector-nothing-pasted-flow-stays-open-until"
        ));
        out.json_value(started);
        return Ok(());
    }
    let done = client
        .post(
            &format!("/connectors/{cid}/accounts/{aid}/oauth/complete"),
            json!({ "code": code }),
        )
        .await?;
    out.say(&bisa_core::text!(
        "cli-connector-account-connected",
        aid = aid.to_string(),
        cid = cid.to_string()
    ));
    out.json_value(done);
    Ok(())
}
