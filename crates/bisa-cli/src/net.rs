//! Collaboration from the terminal (14-collaboration): relays, invitations,
//! people, hosted memberships — and the pump the `node` command runs.
//!
//! Every verb goes through a running node when there is one (the pump and
//! the guest sessions live in that process) and falls back to the engine
//! or the store alone for what needs no wire: relays are a setting,
//! invitations and people are the store's. A `join` with no node running
//! opens the pool for the one claim and closes it.

use crate::ctx::Ctx;
use crate::output::Out;
use anyhow::{bail, Context as _, Result};
use bisa_collab::{InviteCode, Relays};
use bisa_collab::{CLIENT_CLI, CLIENT_DESKTOP};
use bisa_core::sync::Locked;
use bisa_core::{is_relay_url, ManualPeer, MemberRole, SyncSettings};
use bisa_guest::{FaceStore, Guests, HostedState, OwnerProfile};
use bisa_net::{Host, NetConfig};
use bisa_node::collab::{CollabDoors, CollabRefusal};
use bisa_node::dto::{HostedChannelRow, HostedPostBody, HostedPosted, SyncReport};
use bisa_store::Workspace;
use futures::future::BoxFuture;
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;

const RELAY_CHECK_BUDGET: Duration = Duration::from_secs(5);

/// The guest sessions' faces and profile, over this node's own workspace
/// (`bisa_guest::FaceStore`): a face a host sends lands in the attachment
/// store, where `GET /attachments/{sha}` serves it to the desktop; the
/// owner's row is what every host is told.
struct WorkspaceFaces(Arc<Workspace>);

impl FaceStore for WorkspaceFaces {
    fn bytes(&self, sha256: &str) -> Option<Vec<u8>> {
        self.0.attachment_bytes(sha256).ok().flatten()
    }

    fn hold(&self, sha256: &str, bytes: &[u8]) -> Result<(), String> {
        self.0
            .accept_attachment(sha256, bytes)
            .map_err(|e| e.to_string())
    }

    fn profile(&self) -> OwnerProfile {
        let owner = self.0.owner_principal();
        match self.0.member(&owner) {
            Ok(Some(row)) => OwnerProfile {
                label: row.label,
                photo: row.photo,
            },
            _ => OwnerProfile::default(),
        }
    }
}

fn sync_settings(ws: &Workspace) -> Result<SyncSettings> {
    Ok(SyncSettings::from_resolved(&ws.settings(None)?))
}

/// Write `sync.relays` at the machine scope — through the node when one
/// runs, so the pump hears it live.
async fn write_relays(ctx: &Ctx, relays: &[String]) -> Result<()> {
    let value = json!(relays);
    if let Some(node) = ctx.node_client().await {
        node.put(
            "/settings/machine",
            json!({"values": {bisa_core::collab_settings::keys::SYNC_RELAYS: value}}),
        )
        .await?;
        return Ok(());
    }
    let (engine, _) = ctx.engine().await?;
    engine.set_setting(
        bisa_core::SettingScope::Machine,
        None,
        bisa_core::collab_settings::keys::SYNC_RELAYS,
        value,
    )?;
    engine.shutdown().await;
    Ok(())
}

pub async fn relay_add(ctx: &Ctx, out: &Out, url: &str) -> Result<()> {
    let url = url.trim().to_string();
    if !is_relay_url(&url) {
        bail!(bisa_core::text!("cli-net-relay-ws-wss-url"));
    }
    let mut relays = sync_settings(&ctx.workspace()?)?.relays;
    let added = !relays.contains(&url);
    if added {
        relays.push(url.clone());
        write_relays(ctx, &relays).await?;
    }
    out.say(&if added {
        bisa_core::text!("cli-net-relay-added")
    } else {
        bisa_core::text!("cli-net-relay-already-configured")
    });
    out.json_value(json!({"added": added, "relays": relays}));
    Ok(())
}

pub async fn relay_remove(ctx: &Ctx, out: &Out, url: &str) -> Result<()> {
    let mut relays = sync_settings(&ctx.workspace()?)?.relays;
    let before = relays.len();
    relays.retain(|r| r != url.trim());
    let removed = relays.len() != before;
    if removed {
        write_relays(ctx, &relays).await?;
    }
    out.say(&if removed {
        bisa_core::text!("cli-net-relay-removed")
    } else {
        bisa_core::text!("cli-net-relay-was-not-configured")
    });
    out.json_value(json!({"removed": removed, "relays": relays}));
    Ok(())
}

pub async fn relay_list(ctx: &Ctx, out: &Out) -> Result<()> {
    if let Some(node) = ctx.node_client().await {
        let v = node.get("/sync").await?;
        let relays = v["relays"].as_array().cloned().unwrap_or_default();
        if relays.is_empty() {
            out.say(&bisa_core::text!(
                "cli-net-no-relays-configured-add-one-with"
            ));
        }
        for r in &relays {
            for line in relay_row_lines(r) {
                out.human(&line);
            }
        }
        out.json_value(v);
        return Ok(());
    }
    // No node: the settings alone — each relay with `off` while the switch
    // is, so the list reads the same as the wire's report would.
    let settings = sync_settings(&ctx.workspace()?)?;
    if settings.relays.is_empty() {
        out.say(&bisa_core::text!(
            "cli-net-no-relays-configured-add-one-with"
        ));
    }
    for r in &settings.relays {
        if settings.enabled {
            out.human(r);
        } else {
            out.human(&format!("{r:<40} off"));
        }
    }
    out.json_value(json!({"enabled": settings.enabled, "relays": settings.relays}));
    Ok(())
}

/// A relay's row as the node reports it: where, its state and its round
/// trip, and under it — indented — why it is not connected, when it says.
fn relay_row_lines(row: &serde_json::Value) -> Vec<String> {
    let mut lines = vec![format!(
        "{:<40} {:<12} {}",
        row["url"].as_str().unwrap_or(""),
        row["status"].as_str().unwrap_or(""),
        row["latency_ms"]
            .as_u64()
            .map(|ms| format!("{ms} ms"))
            .unwrap_or_default()
    )
    .trim_end()
    .to_string()];
    if let Some(problem) = row["problem"].as_str().filter(|p| !p.is_empty()) {
        lines.push(format!("    {problem}"));
    }
    lines
}

/// What stands between this node and its relays, before anything is dialled:
/// the switch, how many relays there are to reach, that TLS can be set up in
/// this process at all, and the one thing about a network a person can act
/// on — relays are dialled directly, never through the proxy HTTP uses.
fn doctor_preamble(enabled: bool, relays: usize, proxy_mode: &str, tls_ready: bool) -> Vec<String> {
    let mut lines = vec![if enabled {
        bisa_i18n::say(&bisa_core::text!("cli-net-sync-over-relays"))
    } else {
        bisa_i18n::say(&bisa_core::text!(
            "cli-net-sync-over-relays-off",
            off_words = off_words()
        ))
    }];
    lines.push(match relays {
        0 => bisa_i18n::say(&bisa_core::text!(
            "cli-net-relays-none-configured-add-one-with"
        )),
        1 => bisa_i18n::say(&bisa_core::text!("cli-net-relays-1-configured")),
        n => bisa_i18n::say(&bisa_core::text!(
            "cli-net-relays-configured",
            n = n.to_string()
        )),
    });
    lines.push(if tls_ready {
        bisa_i18n::say(&bisa_core::text!(
            "cli-net-tls-ready-wss-relay-can-be",
            a0 = (bisa_collab::tls::PROVIDER).to_string()
        ))
    } else {
        bisa_i18n::say(&bisa_core::text!(
            "cli-net-tls-no-crypto-provider-process-no"
        ))
    });
    if proxy_mode != "none" {
        lines.push(bisa_i18n::say(&bisa_core::text!(
            "cli-net-proxy-network-proxy-mode-http-relays",
            proxy_mode = proxy_mode.to_string()
        )));
    }
    lines
}

/// Say what stands between this node and its relays, then try each one: the
/// questions a person asks when a relay stays *connecting*, answered in order
/// without guessing. Dials only the relays already configured.
pub async fn relay_doctor(ctx: &Ctx, out: &Out) -> Result<()> {
    let ws = ctx.workspace()?;
    let resolved = ws.settings(None)?;
    let settings = SyncSettings::from_resolved(&resolved);
    let proxy_mode = resolved
        .iter()
        .find(|r| r.key == "network.proxy.mode")
        .and_then(|r| r.value.as_str().map(str::to_string))
        .unwrap_or_else(|| "environment".to_string());
    bisa_collab::ensure_crypto_provider();
    let tls_ready = bisa_collab::crypto_provider_ready();
    for line in doctor_preamble(
        settings.enabled,
        settings.relays.len(),
        &proxy_mode,
        tls_ready,
    ) {
        out.human(&line);
    }
    let checks = run_checks(ctx, out, &settings.relays).await?;
    out.json_value(json!({
        "enabled": settings.enabled,
        "relays": settings.relays,
        "tls_ready": tls_ready,
        "tls_provider": bisa_collab::tls::PROVIDER,
        "proxy_mode": proxy_mode,
        "reachable": checks.iter().filter(|c| c.ok).count(),
        "checks": checks,
    }));
    Ok(())
}

/// Try a relay once — the one named, or every configured one — whether or
/// not the wire is on: the check is a throwaway connection.
pub async fn relay_check(ctx: &Ctx, out: &Out, url: Option<&str>) -> Result<()> {
    let urls: Vec<String> = match url {
        Some(u) => {
            if !is_relay_url(u) {
                bail!(bisa_core::text!("cli-net-relay-ws-wss-url"));
            }
            vec![u.trim().to_string()]
        }
        None => sync_settings(&ctx.workspace()?)?.relays,
    };
    if urls.is_empty() {
        out.say(&bisa_core::text!(
            "cli-net-no-relays-configured-add-one-with"
        ));
        out.json_value(json!({"checks": []}));
        return Ok(());
    }
    let checks = run_checks(ctx, out, &urls).await?;
    let reachable = checks.iter().filter(|c| c.ok).count();
    out.json_value(json!({"checks": checks, "reachable": reachable}));
    Ok(())
}

/// Try each relay once and say what it answered, a line each and the sum when
/// there are several — through the node when one runs, else on a throwaway
/// connection of this process.
async fn run_checks(ctx: &Ctx, out: &Out, urls: &[String]) -> Result<Vec<bisa_collab::RelayCheck>> {
    let node = ctx.node_client().await;
    let mut checks: Vec<bisa_collab::RelayCheck> = Vec::with_capacity(urls.len());
    for u in urls {
        let check: bisa_collab::RelayCheck = match &node {
            Some(node) => {
                serde_json::from_value(node.post("/sync/relays/check", json!({"url": u})).await?)?
            }
            None => Relays::check(u, RELAY_CHECK_BUDGET).await,
        };
        out.human(&check_words(&check));
        checks.push(check);
    }
    if urls.len() > 1 {
        let reachable = checks.iter().filter(|c| c.ok).count();
        out.say(&bisa_core::text!(
            "cli-net-reachable",
            reachable = reachable.to_string(),
            a0 = (urls.len()).to_string()
        ));
    }
    Ok(checks)
}

/// One line for what a check answered.
fn check_words(check: &bisa_collab::RelayCheck) -> String {
    match (check.ok, check.latency_ms, &check.error) {
        (true, Some(ms), _) => bisa_i18n::say(&bisa_core::text!(
            "cli-net-relay-reachable-ms",
            url = check.url.clone(),
            ms = ms.to_string()
        )),
        (true, None, _) => bisa_i18n::say(&bisa_core::text!(
            "cli-net-relay-reachable",
            url = check.url.clone()
        )),
        (false, _, Some(e)) => bisa_i18n::say(&bisa_core::text!(
            "cli-net-relay-not-reachable-error",
            url = check.url.clone(),
            e = e.to_string()
        )),
        (false, _, None) => bisa_i18n::say(&bisa_core::text!(
            "cli-net-relay-not-reachable",
            url = check.url.clone()
        )),
    }
}

/// A direct peer entered by hand, under `sync.iroh.peers`.
pub async fn peer_add(
    ctx: &Ctx,
    out: &Out,
    member_pubkey: &str,
    node_id: &str,
    addr: &str,
) -> Result<()> {
    bisa_core::PrincipalId::new(member_pubkey.to_string()).map_err(|e| anyhow::anyhow!("{e}"))?;
    let ws = ctx.workspace()?;
    let mut peers = sync_settings(&ws)?.iroh_peers;
    peers.retain(|p| p.member_pubkey != member_pubkey);
    peers.push(ManualPeer {
        member_pubkey: member_pubkey.to_string(),
        node_id: node_id.to_string(),
        addrs: vec![addr.to_string()],
    });
    let value = serde_json::to_value(&peers)?;
    let key = bisa_core::collab_settings::keys::SYNC_IROH_PEERS;
    if let Some(node) = ctx.node_client().await {
        node.put("/settings/machine", json!({"values": {key: value}}))
            .await?;
    } else {
        let (engine, _) = ctx.engine().await?;
        engine.set_setting(bisa_core::SettingScope::Machine, None, key, value)?;
        engine.shutdown().await;
    }
    out.say(&bisa_core::text!(
        "cli-net-peer-added",
        member_pubkey = member_pubkey.to_string(),
        addr = addr.to_string()
    ));
    out.json_value(json!({"member": member_pubkey, "node_id": node_id, "addr": addr}));
    Ok(())
}

// -- people -------------------------------------------------------------------

fn role_words(role: MemberRole) -> String {
    role.as_str().to_string()
}

pub async fn people(ctx: &Ctx, out: &Out) -> Result<()> {
    let ws = ctx.workspace()?;
    let members = ws.members()?;
    for m in &members {
        out.human(&format!(
            "{}  {:8}  {}",
            m.pubkey,
            role_words(m.role),
            m.label.as_deref().unwrap_or("")
        ));
    }
    // A named object, as every list of the command line answers.
    out.json_value(json!({"people": members}));
    Ok(())
}

fn hosted_role(word: &str) -> Result<MemberRole> {
    let role: MemberRole = word
        .parse()
        .map_err(|e: bisa_core::CoreError| anyhow::anyhow!("{e}"))?;
    if !role.is_hosted() {
        bail!(bisa_core::text!(
            "cli-net-person-another-node-admin-member-guest"
        ));
    }
    Ok(role)
}

pub async fn set_role(ctx: &Ctx, out: &Out, pubkey: &str, role: &str) -> Result<()> {
    let role = hosted_role(role)?;
    let pk = bisa_core::PrincipalId::new(pubkey.to_string()).map_err(|e| anyhow::anyhow!("{e}"))?;
    let person = if let Some(node) = ctx.node_client().await {
        let v = node
            .put(
                &format!("/workspace/people/{pubkey}/role"),
                json!({"role": role.as_str()}),
            )
            .await?;
        v["person"].clone()
    } else {
        let (engine, _) = ctx.engine().await?;
        let p = bisa_engine::collab::set_role(engine.inner(), &pk, role)?;
        engine.shutdown().await;
        serde_json::to_value(p)?
    };
    out.say(&bisa_core::text!(
        "cli-net-now",
        pubkey = pubkey.to_string(),
        a0 = (role.as_str()).to_string()
    ));
    out.json_value(json!({"person": person}));
    Ok(())
}

pub async fn remove_person(ctx: &Ctx, out: &Out, pubkey: &str) -> Result<()> {
    let pk = bisa_core::PrincipalId::new(pubkey.to_string()).map_err(|e| anyhow::anyhow!("{e}"))?;
    if let Some(node) = ctx.node_client().await {
        node.delete(&format!("/workspace/people/{pubkey}")).await?;
    } else {
        let (engine, _) = ctx.engine().await?;
        bisa_engine::collab::remove_person(engine.inner(), &pk)?;
        engine.shutdown().await;
    }
    out.say(&bisa_core::text!(
        "cli-net-removed-what-they-already-received-cannot",
        pubkey = pubkey.to_string()
    ));
    out.json_value(json!({"removed": pubkey}));
    Ok(())
}

// -- invitations ----------------------------------------------------------------

pub async fn invite(
    ctx: &Ctx,
    out: &Out,
    role: &str,
    channels: &[String],
    label: Option<String>,
) -> Result<()> {
    let role = hosted_role(role)?;
    let (invite, link, code) = if let Some(node) = ctx.node_client().await {
        let v = node
            .post(
                "/workspace/invites",
                json!({"role": role.as_str(), "channels": channels, "label": label}),
            )
            .await?;
        (
            v["invite"].clone(),
            v["link"].as_str().unwrap_or_default().to_string(),
            v["code"].as_str().unwrap_or_default().to_string(),
        )
    } else {
        let (engine, _) = ctx.engine().await?;
        let channels = channels
            .iter()
            .map(|c| bisa_core::ChannelId::new(c.as_str()))
            .collect::<Result<Vec<_>, _>>()?;
        let (invite, code) =
            bisa_engine::collab::create_invite(engine.inner(), role, channels, label)?;
        engine.shutdown().await;
        (serde_json::to_value(&invite)?, code.link()?, code.text()?)
    };
    out.say(&bisa_core::text!(
        "cli-net-invitation-made-expires-share-one-these",
        a0 = (role.as_str()).to_string(),
        a1 = (invite["expires_at"]).to_string()
    ));
    out.human(&format!("\n  {link}\n  {code}\n"));
    out.say(&bisa_core::text!(
        "cli-net-person-pastes-into-settings-people-join"
    ));
    out.json_value(json!({"invite": invite, "link": link, "code": code}));
    Ok(())
}

pub async fn invites(ctx: &Ctx, out: &Out) -> Result<()> {
    let ws = ctx.workspace()?;
    let invites = ws.invites()?;
    if invites.is_empty() {
        out.say(&bisa_core::text!("cli-net-no-invitations"));
    }
    for i in &invites {
        out.human(&format!(
            "{}  {:10}  {:8}  {}",
            i.id,
            i.state.as_str(),
            i.role.as_str(),
            i.label.as_deref().unwrap_or("")
        ));
    }
    out.json_value(json!({"invites": invites}));
    Ok(())
}

pub async fn revoke(ctx: &Ctx, out: &Out, id: &str) -> Result<()> {
    let id = id.parse::<bisa_core::InviteId>().map_err(|e| {
        anyhow::anyhow!(bisa_core::text!("cli-net-bad-invite-id", e = e.to_string()))
    })?;
    let invite = if let Some(node) = ctx.node_client().await {
        node.delete(&format!("/workspace/invites/{id}")).await?["invite"].clone()
    } else {
        let (engine, _) = ctx.engine().await?;
        let i = bisa_engine::collab::revoke_invite(engine.inner(), id)?;
        engine.shutdown().await;
        serde_json::to_value(i)?
    };
    out.say(&bisa_core::text!("cli-net-invitation-withdrawn"));
    out.json_value(json!({"invite": invite}));
    Ok(())
}

// -- hosted memberships ----------------------------------------------------------

fn hosted_words(h: &bisa_guest::Hosted) -> String {
    let state = h.state.words();
    format!(
        "{}  {:8}  {}{}",
        h.host.pubkey,
        h.role.as_str(),
        h.host.name,
        if state.is_empty() {
            String::new()
        } else {
            format!("  ({state})")
        }
    )
}

pub async fn join(ctx: &Ctx, out: &Out, code: &str, label: Option<String>) -> Result<()> {
    let parsed = InviteCode::parse(code).map_err(|e| anyhow::anyhow!("{e}"))?;
    let hosted: bisa_guest::Hosted = if let Some(node) = ctx.node_client().await {
        let v = node
            .post("/hosts/join", json!({"code": code, "label": label}))
            .await?;
        serde_json::from_value(v["host"].clone())?
    } else {
        // No node: open the pool for the one claim. The code's relays are
        // kept as a setting so the node reads them next time.
        let ws = Arc::new(ctx.workspace()?);
        let settings = sync_settings(&ws)?;
        if !settings.enabled {
            bail!("{}", off_words());
        }
        let mut relays = settings.relays;
        for r in &parsed.relays {
            if is_relay_url(r) && !relays.contains(r) {
                relays.push(r.clone());
            }
        }
        if relays.is_empty() {
            bail!(bisa_core::text!("cli-net-no-relay-reach-host-through-code"));
        }
        write_relays(ctx, &relays).await?;
        let pool = Relays::start(ws.owner_keys().clone(), &relays).await;
        let guests = Guests::open(
            ws.owner_keys().clone(),
            &ctx.data_dir,
            Arc::clone(&pool),
            Arc::new(WorkspaceFaces(Arc::clone(&ws))),
        )?;
        let router = guests.spawn();
        out.say(&bisa_core::text!("cli-net-sending-claim"));
        let (session, _) = guests.join(&parsed, label, CLIENT_CLI).await?;
        let hosted = session
            .hosted()?
            .context(bisa_core::text!("cli-net-membership-was-not-recorded"))?;
        router.abort();
        pool.stop().await;
        hosted
    };
    out.human(&match &hosted.state {
        HostedState::Member => bisa_i18n::say(&bisa_core::text!(
            "cli-net-welcomed-as",
            a0 = (hosted.host.name).to_string(),
            a1 = (hosted.role.as_str()).to_string()
        )),
        HostedState::Requested => bisa_i18n::say(&bisa_core::text!(
            "cli-net-host-has-not-answered-yet-welcome"
        )),
        other => other.words(),
    });
    out.json_value(json!({"host": hosted}));
    Ok(())
}

pub async fn hosts(ctx: &Ctx, out: &Out) -> Result<()> {
    let hosts: Vec<bisa_guest::Hosted> = if let Some(node) = ctx.node_client().await {
        serde_json::from_value(node.get("/hosts").await?["hosts"].clone())?
    } else {
        let ws = Arc::new(ctx.workspace()?);
        let pool = Relays::start(ws.owner_keys().clone(), &[]).await;
        let guests = Guests::open(
            ws.owner_keys().clone(),
            &ctx.data_dir,
            pool,
            Arc::new(WorkspaceFaces(Arc::clone(&ws))),
        )?;
        guests.list().await
    };
    if hosts.is_empty() {
        out.say(&bisa_core::text!("cli-net-not-member-any-other-workspace"));
    }
    for h in &hosts {
        out.human(&hosted_words(h));
    }
    out.json_value(json!({"hosts": hosts}));
    Ok(())
}

pub async fn leave(ctx: &Ctx, out: &Out, host: &str) -> Result<()> {
    let host = host.trim().to_lowercase();
    if let Some(node) = ctx.node_client().await {
        node.delete(&format!("/hosts/{host}")).await?;
    } else {
        let ws = Arc::new(ctx.workspace()?);
        let relays = sync_settings(&ws)?.relays;
        let pool = Relays::start(ws.owner_keys().clone(), &relays).await;
        let guests = Guests::open(
            ws.owner_keys().clone(),
            &ctx.data_dir,
            Arc::clone(&pool),
            Arc::new(WorkspaceFaces(Arc::clone(&ws))),
        )?;
        guests.leave(&host).await?;
        pool.stop().await;
    }
    out.say(&bisa_core::text!("cli-net-left", host = host.to_string()));
    out.json_value(json!({"left": host}));
    Ok(())
}

// -- the pump, lent to the node -------------------------------------------------

/// What the `node` command runs beside the daemon: the relay pool, the
/// host's pump (when the settings give it anyone to talk through) and the
/// guest sessions — and the doors the node serves them through.
pub struct Pump {
    ws: Arc<Workspace>,
    engine: Arc<bisa_engine::Inner>,
    relays: Arc<Relays>,
    /// The `sync.*` settings as last read — the switch and the relay list
    /// every report and refusal answers from.
    cfg: tokio::sync::Mutex<NetConfig>,
    host: tokio::sync::Mutex<Option<Host>>,
    guests: Arc<Guests>,
    tasks: std::sync::Mutex<Vec<tokio::task::JoinHandle<()>>>,
}

/// The refusal while the wire is off, everywhere it is said.
fn off_words() -> String {
    bisa_i18n::say(&bisa_core::text!("cli-net-sync-off"))
}

/// The relay URLs the pool should hold for a config: the list when the
/// switch is on, none when it is off.
fn pool_relays(cfg: &NetConfig) -> &[String] {
    if cfg.enabled {
        &cfg.relays
    } else {
        &[]
    }
}

impl Pump {
    /// Open the pool on the `sync.*` settings and start what they allow.
    pub async fn start(
        engine: &bisa_engine::Engine,
        data_dir: &std::path::Path,
    ) -> Result<Arc<Self>> {
        let ws = Arc::clone(&engine.inner().ws);
        let cfg = NetConfig::from_workspace(&ws)?;
        // Off by default: the pool opens with no relay, and nothing is
        // contacted until the switch is turned on.
        let relays = Relays::start(ws.owner_keys().clone(), pool_relays(&cfg)).await;
        let host = if cfg.reaches_anyone() {
            match Host::start(
                Arc::clone(&ws),
                Arc::clone(&relays),
                cfg.clone(),
                data_dir.to_path_buf(),
            )
            .await
            {
                Ok(h) => Some(h),
                Err(e) => {
                    tracing::warn!(target: "bisa_cli", "the host pump did not start: {e}");
                    None
                }
            }
        } else {
            None
        };
        let guests = Guests::open(
            ws.owner_keys().clone(),
            data_dir,
            Arc::clone(&relays),
            Arc::new(WorkspaceFaces(Arc::clone(&ws))),
        )?;
        let pump = Arc::new(Self {
            ws,
            engine: Arc::clone(engine.inner()),
            relays,
            cfg: tokio::sync::Mutex::new(cfg),
            host: tokio::sync::Mutex::new(host),
            guests,
            tasks: std::sync::Mutex::new(Vec::new()),
        });
        let mut tasks = vec![pump.guests.spawn()];
        tasks.push(pump.hear_engine(engine.events()));
        tasks.push(pump.hear_guests());
        *pump.tasks.locked() = tasks;
        Ok(pump)
    }

    /// The engine's word the pump acts on: a `sync.*` write re-reads the
    /// settings — the pool matches the relays, the pump its interval, and a
    /// pump that had nothing to talk through starts; the owner's own profile
    /// moving — their label, their face — is said to every host this node is
    /// a guest of (14-collaboration).
    fn hear_engine(
        self: &Arc<Self>,
        mut events: tokio::sync::broadcast::Receiver<bisa_engine::events::EngineEvent>,
    ) -> tokio::task::JoinHandle<()> {
        let pump = Arc::clone(self);
        tokio::spawn(async move {
            loop {
                match events.recv().await {
                    Ok(ev) => match &ev.payload {
                        bisa_engine::events::EnginePayload::SettingsChanged { keys, .. }
                            if keys.iter().any(|k| k.starts_with("sync.")) =>
                        {
                            pump.apply_settings().await;
                        }
                        bisa_engine::events::EnginePayload::PeopleChanged {
                            pubkey,
                            change: bisa_store::PeopleChange::ProfileChanged,
                            ..
                        } if *pubkey == pump.ws.owner_principal() => {
                            pump.guests.announce_profile().await;
                        }
                        _ => {}
                    },
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        })
    }

    /// A `sync.*` write: the pool matches the relays the switch allows, the
    /// host pump starts when the settings give it anyone to talk through and
    /// stops when they take that away, and the screens are told.
    async fn apply_settings(&self) {
        let cfg = match NetConfig::from_workspace(&self.ws) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(target: "bisa_cli", "re-reading sync settings: {e}");
                return;
            }
        };
        let mut host = self.host.lock().await;
        if !cfg.reaches_anyone() {
            if let Some(h) = host.take() {
                h.stop().await;
                tracing::info!(target: "bisa_cli", "the host pump stopped: the wire is off");
            }
            self.relays.set_relays(pool_relays(&cfg)).await;
        } else {
            match host.as_ref() {
                Some(h) => h.apply(&cfg).await,
                None => {
                    self.relays.set_relays(&cfg.relays).await;
                    match Host::start(
                        Arc::clone(&self.ws),
                        Arc::clone(&self.relays),
                        cfg.clone(),
                        self.ws.root().to_path_buf(),
                    )
                    .await
                    {
                        Ok(h) => *host = Some(h),
                        Err(e) => {
                            tracing::warn!(target: "bisa_cli", "the host pump did not start: {e}")
                        }
                    }
                }
            }
        }
        *self.cfg.lock().await = cfg;
        self.engine.emit(bisa_engine::events::EngineEvent::global(
            bisa_engine::events::EnginePayload::RelaysChanged,
        ));
    }

    /// What the guest sessions say, said on the engine's bus for the screens.
    fn hear_guests(self: &Arc<Self>) -> tokio::task::JoinHandle<()> {
        let pump = Arc::clone(self);
        let mut rx = pump.guests.subscribe();
        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok((host, change)) => {
                        let Ok(host) = bisa_core::PrincipalId::new(host) else {
                            continue;
                        };
                        let scope = match change {
                            bisa_guest::HostedChange::Message { scope, .. } => Some(scope),
                            _ => None,
                        };
                        pump.engine.emit(bisa_engine::events::EngineEvent::global(
                            bisa_engine::events::EnginePayload::HostedChanged { host, scope },
                        ));
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        })
    }

    /// The attachment fetch the node borrows.
    pub async fn attachment_fetcher(&self) -> Option<bisa_net::AttachmentFetch> {
        self.host
            .lock()
            .await
            .as_ref()
            .map(|h| h.attachment_fetcher())
    }

    pub async fn stop(&self) {
        for t in self.tasks.locked().drain(..) {
            t.abort();
        }
        if let Some(h) = self.host.lock().await.take() {
            h.stop().await;
        }
        self.relays.stop().await;
    }

    async fn session(&self, host: &str) -> Result<Arc<bisa_guest::GuestSession>, CollabRefusal> {
        self.guests
            .session(host)
            .await
            .ok_or(CollabRefusal::NotHosted)
    }

    fn rows(
        session: &bisa_guest::GuestSession,
        channels: Vec<bisa_core::Channel>,
    ) -> Vec<HostedChannelRow> {
        channels
            .into_iter()
            .map(|c| {
                let scope = c.id.to_string();
                HostedChannelRow {
                    unread_count: session.unread(&scope).unwrap_or(0) as u64,
                    latest_at: session.store().latest_at(&scope).ok().flatten(),
                    channel: c,
                }
            })
            .collect()
    }
}

fn refused(e: bisa_guest::GuestError) -> CollabRefusal {
    match e {
        bisa_guest::GuestError::NotHosted => CollabRefusal::NotHosted,
        bisa_guest::GuestError::Refused(words) => CollabRefusal::Refused(words),
        other => CollabRefusal::Failed(other.to_string()),
    }
}

impl CollabDoors for Pump {
    fn sync(&self) -> BoxFuture<'_, SyncReport> {
        Box::pin(async {
            let hosts = self.guests.count().await;
            let cfg = self.cfg.lock().await.clone();
            if !cfg.enabled {
                // Off: every configured relay listed, none contacted.
                return SyncReport {
                    running: true,
                    enabled: false,
                    relays: cfg
                        .relays
                        .iter()
                        .map(bisa_collab::RelayHealth::off)
                        .collect(),
                    people: self.ws.people().map(|p| p.len()).unwrap_or(0),
                    hosts,
                    ..SyncReport::default()
                };
            }
            let host = self.host.lock().await;
            match host.as_ref() {
                Some(h) => {
                    let s = h.status().await;
                    SyncReport {
                        running: true,
                        enabled: true,
                        relays: s.relays,
                        connected_relays: s.connected_relays,
                        published: s.published,
                        ingested: s.ingested,
                        last_catchup: s.last_catchup,
                        people: s.people,
                        hosts,
                        iroh_node_id: s.iroh_node_id,
                        iroh_peers_connected: s.iroh_peers_connected,
                    }
                }
                None => {
                    let relays = self.relays.health().await;
                    SyncReport {
                        running: true,
                        enabled: true,
                        connected_relays: relays.iter().filter(|r| r.connected).count(),
                        relays,
                        people: self.ws.people().map(|p| p.len()).unwrap_or(0),
                        hosts,
                        ..SyncReport::default()
                    }
                }
            }
        })
    }
    fn check_relay(&self, url: String) -> BoxFuture<'_, bisa_collab::RelayCheck> {
        Box::pin(async move { Relays::check(&url, RELAY_CHECK_BUDGET).await })
    }
    fn reconnect(&self) -> BoxFuture<'_, ()> {
        Box::pin(async { self.relays.reconnect().await })
    }
    fn hosts(&self) -> BoxFuture<'_, Vec<bisa_guest::Hosted>> {
        Box::pin(async { self.guests.list().await })
    }
    fn join(
        &self,
        code: String,
        label: Option<String>,
    ) -> BoxFuture<'_, Result<bisa_guest::Hosted, CollabRefusal>> {
        Box::pin(async move {
            let parsed =
                InviteCode::parse(&code).map_err(|e| CollabRefusal::Refused(e.to_string()))?;
            // A join is talking; the switch is the person's to turn on first.
            if !self.cfg.lock().await.enabled {
                return Err(CollabRefusal::Refused(off_words()));
            }
            // The code's relays become a setting — written through the
            // engine, so it is said on the bus: this pump re-reads what it
            // holds, and a screen showing the relays shows these. The pool
            // takes them at once: the claim below goes out through them.
            let mut relays = sync_settings(&self.ws)
                .map_err(|e| CollabRefusal::Failed(e.to_string()))?
                .relays;
            let mut grew = false;
            for r in &parsed.relays {
                if is_relay_url(r) && !relays.contains(r) {
                    relays.push(r.clone());
                    grew = true;
                }
            }
            if grew {
                bisa_engine::settings::set(
                    &self.engine,
                    bisa_core::SettingScope::Machine,
                    None,
                    bisa_core::collab_settings::keys::SYNC_RELAYS,
                    json!(relays),
                )
                .map_err(|e| CollabRefusal::Failed(e.to_string()))?;
                self.relays.set_relays(&relays).await;
            }
            if self.relays.count().await == 0 {
                return Err(CollabRefusal::Refused(bisa_i18n::say(&bisa_core::text!(
                    "cli-net-no-relay-reach-host-through-code"
                ))));
            }
            let (session, _) = self
                .guests
                .join(&parsed, label, CLIENT_DESKTOP)
                .await
                .map_err(refused)?;
            session
                .hosted()
                .map_err(refused)?
                .ok_or(CollabRefusal::NotHosted)
        })
    }
    fn leave(&self, host: String) -> BoxFuture<'_, Result<(), CollabRefusal>> {
        Box::pin(async move { self.guests.leave(&host).await.map_err(refused) })
    }
    fn hosted_channels(
        &self,
        host: String,
    ) -> BoxFuture<'_, Result<Vec<HostedChannelRow>, CollabRefusal>> {
        Box::pin(async move {
            let s = self.session(&host).await?;
            Ok(Self::rows(&s, s.channels().map_err(refused)?))
        })
    }
    fn hosted_dms(
        &self,
        host: String,
    ) -> BoxFuture<'_, Result<Vec<HostedChannelRow>, CollabRefusal>> {
        Box::pin(async move {
            let s = self.session(&host).await?;
            Ok(Self::rows(&s, s.dms().map_err(refused)?))
        })
    }
    fn hosted_members(
        &self,
        host: String,
    ) -> BoxFuture<'_, Result<Vec<bisa_collab::Directory>, CollabRefusal>> {
        Box::pin(async move { self.session(&host).await?.members().map_err(refused) })
    }
    fn hosted_messages(
        &self,
        host: String,
        scope: String,
        before: Option<u64>,
        limit: usize,
    ) -> BoxFuture<'_, Result<Vec<bisa_guest::HostedMessage>, CollabRefusal>> {
        Box::pin(async move {
            self.session(&host)
                .await?
                .messages(&scope, before, limit)
                .map_err(refused)
        })
    }
    fn hosted_post(
        &self,
        host: String,
        scope: String,
        post: HostedPostBody,
    ) -> BoxFuture<'_, Result<HostedPosted, CollabRefusal>> {
        Box::pin(async move {
            let s = self.session(&host).await?;
            let mentions = post
                .mentions
                .iter()
                .map(|m| bisa_core::PrincipalId::new(m.clone()))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| CollabRefusal::Refused(e.to_string()))?;
            let posted = s
                .post(&scope, &post.content, &mentions, post.reply_to.as_deref())
                .await
                .map_err(refused)?;
            Ok(HostedPosted {
                id: posted.id,
                scope: posted.scope,
                redacted: posted.redacted,
            })
        })
    }
    fn hosted_react(
        &self,
        host: String,
        event: String,
        emoji: String,
    ) -> BoxFuture<'_, Result<HostedPosted, CollabRefusal>> {
        Box::pin(async move {
            let posted = self
                .session(&host)
                .await?
                .react(&event, &emoji)
                .await
                .map_err(refused)?;
            Ok(HostedPosted {
                id: posted.id,
                scope: posted.scope,
                redacted: posted.redacted,
            })
        })
    }
    fn hosted_retract(
        &self,
        host: String,
        event: String,
    ) -> BoxFuture<'_, Result<HostedPosted, CollabRefusal>> {
        Box::pin(async move {
            let posted = self
                .session(&host)
                .await?
                .retract(&event)
                .await
                .map_err(refused)?;
            Ok(HostedPosted {
                id: posted.id,
                scope: posted.scope,
                redacted: posted.redacted,
            })
        })
    }
    fn hosted_open_dm(
        &self,
        host: String,
        participants: Vec<String>,
    ) -> BoxFuture<'_, Result<(), CollabRefusal>> {
        Box::pin(async move {
            let people = participants
                .iter()
                .map(|p| bisa_core::PrincipalId::new(p.clone()))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| CollabRefusal::Refused(e.to_string()))?;
            self.session(&host)
                .await?
                .open_dm(&people)
                .await
                .map_err(refused)
        })
    }
    fn hosted_read(&self, host: String, scope: String) -> BoxFuture<'_, Result<(), CollabRefusal>> {
        Box::pin(async move {
            self.session(&host)
                .await?
                .mark_read(&scope)
                .map_err(refused)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_doctor_says_what_stands_in_the_way_before_anything_is_dialled() {
        let healthy = doctor_preamble(true, 4, "none", true);
        assert_eq!(
            healthy,
            vec![
                "sync over relays: on".to_string(),
                "relays: 4 configured".to_string(),
                "tls: ready (ring) — a wss:// relay can be reached".to_string(),
            ]
        );
        // Off, none configured, and a proxy the relays will not use: each is said.
        let blocked = doctor_preamble(false, 0, "manual", true);
        assert!(blocked[0].starts_with("sync over relays: off — "));
        assert!(blocked[0].contains("sync.enabled"));
        assert!(blocked[1].starts_with("relays: none configured"));
        assert!(blocked[3].contains("`manual`") && blocked[3].contains("dialled directly"));
        assert_eq!(
            doctor_preamble(true, 1, "none", true)[1],
            "relays: 1 configured"
        );
        // The fault this command exists to name.
        let no_tls = doctor_preamble(true, 1, "none", false);
        assert!(
            no_tls[2].starts_with("tls: no crypto provider"),
            "{}",
            no_tls[2]
        );
        // `environment` is the default, and a proxy may be in the environment.
        assert_eq!(doctor_preamble(true, 1, "environment", true).len(), 4);
    }

    #[test]
    fn a_relay_row_says_its_problem_under_it_and_nothing_when_there_is_none() {
        let up = json!({"url": "wss://relay.example", "status": "connected", "latency_ms": 38});
        let lines = relay_row_lines(&up);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].starts_with("wss://relay.example") && lines[0].ends_with("38 ms"));

        let down = json!({"url": "wss://relay.example", "status": "connecting",
            "problem": "never connected in 3 attempts — the address may be wrong"});
        let lines = relay_row_lines(&down);
        assert_eq!(lines.len(), 2);
        assert!(
            !lines[0].ends_with(' '),
            "no trailing pad when there is no latency"
        );
        assert_eq!(
            lines[1],
            "    never connected in 3 attempts — the address may be wrong"
        );
        assert_eq!(
            relay_row_lines(&json!({"url": "u", "status": "off", "problem": ""})).len(),
            1
        );
    }

    #[test]
    fn a_check_reads_as_one_line_with_the_relays_own_error() {
        let check = |ok, latency_ms, error: Option<&str>| bisa_collab::RelayCheck {
            url: "wss://relay.example".into(),
            ok,
            latency_ms,
            error: error.map(str::to_string),
        };
        assert_eq!(
            check_words(&check(true, Some(41), None)),
            "wss://relay.example: reachable (41 ms)"
        );
        assert_eq!(
            check_words(&check(false, None, Some("connection refused"))),
            "wss://relay.example: not reachable — connection refused"
        );
        assert_eq!(
            check_words(&check(false, None, None)),
            "wss://relay.example: not reachable"
        );
    }
}
