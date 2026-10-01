//! Mobile development on this machine (ide/19): the workspace's word on
//! whether this node develops for phones and for which platforms
//! ([`Access`], from the `mobile_development.*` settings), the toolchain probed through
//! the tools the node was started with (`EngineConfig::mobile_development`, `None` in
//! every test fixture), the devices listed, booted and captured, and the
//! one `flutter run` line a terminal runs.
//!
//! The shape is the browser bridge's without the bridge: nothing waits on a
//! desktop, because a simulator and `adb` are on this machine and the node
//! asks them itself — so an agent's `mobile_development_screenshot` works with no window
//! open, as a headless tab does. A capture is stored the way a browser
//! screenshot is: an attachment, answered as the path of its named copy.
//!
//! Who may ask is checked at the op: the tool list is a menu, not a permission.

use crate::events::{EngineEvent, EnginePayload};
use crate::{EngineError, Inner};
use bisa_core::{AgentId, AttachmentRef, ProjectId, WorkstreamId};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

pub use bisa_mobile_development::{
    Device, DeviceKind, DeviceState, DoctorLine, DoctorState, Hints, ImageFormat,
    MobileDevelopmentError, MobileDevelopmentTools, Platform, Toolchain,
};

/// The catalog skill whose presence on an agent is the assignment
/// (`mobile_development.agents = assigned`).
pub const MOBILE_DEVELOPMENT_SKILL: &str = "flutter-development";
/// How long a toolchain probe stands before the next read runs it again;
/// *Check again* and any `mobile_development.*` write clear it at once.
pub const TOOLCHAIN_TTL: Duration = Duration::from_secs(600);
/// The sentences the op refuses with, by policy.
pub const OFF: &str =
    "mobile development is turned off in Settings › Capabilities › Mobile Development";
pub const NOBODY_MAY: &str =
    "mobile tools are set to nobody in Settings › Capabilities › Mobile Development";
pub const NOT_ASSIGNED: &str = "this agent does not carry the Flutter Development skill — attach it in Agents, or set mobile_development.agents to everyone in Settings › Capabilities › Mobile Development";
pub const NO_TOOLS: &str =
    "the mobile tools are not available — this node was started without them";
pub const PLATFORM_OFF: &str =
    "that device's platform is off in Settings › Capabilities › Mobile Development (mobile_development.platforms)";

/// `mobile_development.platforms`: which platforms this machine develops for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Platforms {
    Both,
    Ios,
    Android,
}

impl Platforms {
    pub fn allows(self, platform: Platform) -> bool {
        match self {
            Platforms::Both => true,
            Platforms::Ios => platform == Platform::Ios,
            Platforms::Android => platform == Platform::Android,
        }
    }

    /// The platforms as a sentence says them.
    pub fn words(self) -> &'static str {
        match self {
            Platforms::Both => "iOS and Android",
            Platforms::Ios => "iOS only",
            Platforms::Android => "Android only",
        }
    }
}

/// The workspace's word on mobile development, resolved for a project when
/// the asking session has one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Access {
    pub enabled: bool,
    pub platforms: Platforms,
    pub agents: crate::browser::AgentsPolicy,
}

/// The three settings, read once per op; a value the registry cannot give
/// falls to the default the registry declares.
pub fn access(inner: &Inner, project: Option<ProjectId>) -> Access {
    let word = |key: &str| {
        inner
            .ws
            .setting(key, project)
            .ok()
            .and_then(|r| r.value.as_str().map(str::to_string))
    };
    let enabled = inner
        .ws
        .setting("mobile_development.enabled", project)
        .ok()
        .and_then(|r| r.value.as_bool())
        .unwrap_or(false);
    let platforms = match word("mobile_development.platforms").as_deref() {
        Some("ios") => Platforms::Ios,
        Some("android") => Platforms::Android,
        _ => Platforms::Both,
    };
    let agents = match word("mobile_development.agents").as_deref() {
        Some("assigned") => crate::browser::AgentsPolicy::Assigned,
        Some("nobody") => crate::browser::AgentsPolicy::Nobody,
        _ => crate::browser::AgentsPolicy::Everyone,
    };
    Access {
        enabled,
        platforms,
        agents,
    }
}

impl Access {
    /// Why an agent's request is refused, or `None` when it may go: the
    /// switch, the policy against the asking agent, then the platform of the
    /// device it names. `has_skill` is whether the agent carries
    /// [`MOBILE_DEVELOPMENT_SKILL`] — looked up by the caller, since a core agent needs
    /// no lookup at all.
    pub fn refusal(
        &self,
        agent: &AgentId,
        has_skill: impl FnOnce() -> bool,
        platform: Option<Platform>,
    ) -> Option<&'static str> {
        if !self.enabled {
            return Some(OFF);
        }
        match self.agents {
            crate::browser::AgentsPolicy::Nobody => return Some(NOBODY_MAY),
            crate::browser::AgentsPolicy::Assigned if !agent.is_core_id() && !has_skill() => {
                return Some(NOT_ASSIGNED)
            }
            _ => {}
        }
        if platform.is_some_and(|p| !self.platforms.allows(p)) {
            return Some(PLATFORM_OFF);
        }
        None
    }
}

/// One toolchain examination, and when it ran.
#[derive(Clone, Debug)]
struct Probe {
    toolchain: Toolchain,
    checked_at: u64,
}

/// What the engine keeps between reads: the last examination, under a TTL.
pub struct MobileDevelopmentState {
    toolchain: bisa_cache::TtlCell<Probe>,
}

impl std::fmt::Debug for MobileDevelopmentState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MobileDevelopmentState")
            .field("examined", &self.toolchain.get(TOOLCHAIN_TTL).is_some())
            .finish()
    }
}

impl Default for MobileDevelopmentState {
    fn default() -> Self {
        Self {
            toolchain: bisa_cache::TtlCell::new("mobile_development.toolchain"),
        }
    }
}

/// The setup as Settings reads it: the switch, the platforms, what was
/// found, the paths the workspace named, and when the examination ran.
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MobileDevelopmentStatus {
    pub enabled: bool,
    pub platforms: Platforms,
    pub toolchain: Toolchain,
    pub hints: Hints,
    pub checked_at: u64,
}

/// A device's screen, stored: the attachment, its named copy's path, and
/// the PNG's size when the bytes said.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MobileDevelopmentShot {
    pub attachment: AttachmentRef,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
}

/// What a checkout holds: a Flutter app, and the platform folders it has.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MobileDevelopmentProject {
    pub flutter: bool,
    pub ios: bool,
    pub android: bool,
}

/// The line a terminal runs to put the app on a device, and where.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MobileDevelopmentRunCommand {
    pub command: String,
    pub cwd: String,
    pub device: String,
}

/// What changed, for the desktop to read again.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MobileDevelopmentChange {
    Toolchain,
    Devices,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// The two paths the machine named, empty strings read as nothing.
pub fn hints(inner: &Inner) -> Hints {
    let text = |key: &str| {
        inner
            .ws
            .setting(key, None)
            .ok()
            .and_then(|r| r.value.as_str().map(str::trim).map(str::to_string))
            .filter(|s| !s.is_empty())
    };
    Hints {
        flutter_path: text("mobile_development.flutter.path"),
        android_sdk: text("mobile_development.android.sdk"),
    }
}

fn tools(inner: &Inner) -> Result<Arc<dyn MobileDevelopmentTools>, EngineError> {
    inner.config.mobile_development.clone().ok_or_else(|| {
        EngineError::MobileDevelopmentUnavailable(bisa_core::text!(
            "error-engine-mobile-development-unavailable-no-tools"
        ))
    })
}

fn ensure_on(inner: &Inner, project: Option<ProjectId>) -> Result<Access, EngineError> {
    let access = access(inner, project);
    if !access.enabled {
        return Err(EngineError::Conflict(bisa_core::text!(
            "error-engine-conflict-mobile-development-off"
        )));
    }
    Ok(access)
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, MobileDevelopmentError> + Send + 'static,
) -> Result<T, EngineError> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| {
            EngineError::Invalid(bisa_core::text!(
                "error-engine-invalid-mobile-development-task",
                e = e.to_string()
            ))
        })?
        .map_err(EngineError::from)
}

fn announce(inner: &Inner, what: MobileDevelopmentChange) {
    inner.emit(EngineEvent::global(
        EnginePayload::MobileDevelopmentChanged { what },
    ));
}

/// The setup, examined now when nothing stands or `fresh` says so, else the
/// last examination. Read whether or not mobile development is on: the
/// person reads what to install before they switch it on.
pub async fn status(
    inner: &Arc<Inner>,
    fresh: bool,
) -> Result<MobileDevelopmentStatus, EngineError> {
    let access = access(inner, None);
    let hints = hints(inner);
    if fresh {
        inner.mobile_development.toolchain.clear();
    }
    let probe = match inner.mobile_development.toolchain.get(TOOLCHAIN_TTL) {
        Some(probe) => probe,
        None => {
            let tools = tools(inner)?;
            let asked = hints.clone();
            let toolchain = blocking(move || Ok(tools.toolchain(&asked))).await?;
            let probe = Probe {
                toolchain,
                checked_at: now_secs(),
            };
            inner.mobile_development.toolchain.set(probe.clone());
            announce(inner, MobileDevelopmentChange::Toolchain);
            probe
        }
    };
    Ok(MobileDevelopmentStatus {
        enabled: access.enabled,
        platforms: access.platforms,
        toolchain: probe.toolchain,
        hints,
        checked_at: probe.checked_at,
    })
}

/// Every device this machine can reach, without the platforms' filter.
async fn every_device(inner: &Arc<Inner>) -> Result<Vec<Device>, EngineError> {
    let tools = tools(inner)?;
    let hints = hints(inner);
    blocking(move || tools.devices(&hints)).await
}

/// The devices of the platforms this machine develops for.
pub async fn devices(
    inner: &Arc<Inner>,
    project: Option<ProjectId>,
) -> Result<Vec<Device>, EngineError> {
    let access = ensure_on(inner, project)?;
    Ok(every_device(inner)
        .await?
        .into_iter()
        .filter(|d| access.platforms.allows(d.platform))
        .collect())
}

/// One device by id: absent from the machine, or of a platform that is off.
async fn find(
    inner: &Arc<Inner>,
    project: Option<ProjectId>,
    id: &str,
) -> Result<Device, EngineError> {
    let access = ensure_on(inner, project)?;
    let device = every_device(inner)
        .await?
        .into_iter()
        .find(|d| d.id == id)
        .ok_or_else(|| {
            EngineError::MobileDevelopment(MobileDevelopmentError::NoSuchDevice(id.to_string()))
        })?;
    if !access.platforms.allows(device.platform) {
        return Err(EngineError::Conflict(bisa_core::text!(
            "error-engine-conflict-mobile-development-platform-off"
        )));
    }
    Ok(device)
}

/// The device after the call, read again from the machine.
async fn after(inner: &Arc<Inner>, id: &str) -> Result<Device, EngineError> {
    announce(inner, MobileDevelopmentChange::Devices);
    every_device(inner)
        .await?
        .into_iter()
        .find(|d| d.id == id)
        .ok_or_else(|| {
            EngineError::MobileDevelopment(MobileDevelopmentError::NoSuchDevice(id.to_string()))
        })
}

pub async fn boot(
    inner: &Arc<Inner>,
    project: Option<ProjectId>,
    id: &str,
) -> Result<Device, EngineError> {
    let device = find(inner, project, id).await?;
    let tools = tools(inner)?;
    let hints = hints(inner);
    blocking(move || tools.boot(&hints, &device)).await?;
    after(inner, id).await
}

pub async fn shutdown(
    inner: &Arc<Inner>,
    project: Option<ProjectId>,
    id: &str,
) -> Result<Device, EngineError> {
    let device = find(inner, project, id).await?;
    let tools = tools(inner)?;
    let hints = hints(inner);
    blocking(move || tools.shutdown(&hints, &device)).await?;
    after(inner, id).await
}

pub async fn show(
    inner: &Arc<Inner>,
    project: Option<ProjectId>,
    id: &str,
) -> Result<Device, EngineError> {
    let device = find(inner, project, id).await?;
    let tools = tools(inner)?;
    let hints = hints(inner);
    let shown = device.clone();
    blocking(move || tools.show(&hints, &shown)).await?;
    Ok(device)
}

/// Make a simulator and answer it as the machine now lists it.
pub async fn create_simulator(
    inner: &Arc<Inner>,
    name: &str,
    devicetype: &str,
    runtime: &str,
) -> Result<Device, EngineError> {
    let access = ensure_on(inner, None)?;
    if !access.platforms.allows(Platform::Ios) {
        return Err(EngineError::Conflict(bisa_core::text!(
            "error-engine-conflict-mobile-development-platform-off"
        )));
    }
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-simulator-needs-name"
        )));
    }
    let tools = tools(inner)?;
    let hints = hints(inner);
    let (devicetype, runtime) = (devicetype.to_string(), runtime.to_string());
    let made = name.clone();
    let udid =
        blocking(move || tools.create_simulator(&hints, &made, &devicetype, &runtime)).await?;
    after(inner, &udid).await
}

/// The most a frame or a screenshot weighs: 32 MiB. A device tool that
/// answers more is answering something other than a screen, and nothing that
/// size is handed to the desktop or kept as an attachment.
pub const MAX_FRAME_BYTES: usize = 32 * 1024 * 1024;

/// The device's screen now, as bytes for a mirror — never stored. An
/// Android screen is PNG whatever was asked.
pub async fn frame(
    inner: &Arc<Inner>,
    project: Option<ProjectId>,
    id: &str,
    format: ImageFormat,
) -> Result<(Vec<u8>, ImageFormat), EngineError> {
    let device = find(inner, project, id).await?;
    let format = match device.platform {
        Platform::Android => ImageFormat::Png,
        Platform::Ios => format,
    };
    let tools = tools(inner)?;
    let hints = hints(inner);
    let bytes = blocking(move || tools.screenshot(&hints, &device, format)).await?;
    if bytes.len() > MAX_FRAME_BYTES {
        tracing::warn!(target: "bisa_engine::mobile_development", device = %id, bytes = bytes.len(), "a device tool answered more than a screen; the frame is refused");
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-device-answered-bytes-frame-most",
            a0 = (bytes.len()).to_string(),
            max_frame_bytes = (MAX_FRAME_BYTES).to_string()
        )));
    }
    Ok((bytes, format))
}

/// The file name a capture of a device is kept under: `mobile-<id>-<ulid>.png`.
pub fn screenshot_name(device: &str) -> String {
    let device: String = device
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(16)
        .collect();
    format!(
        "mobile-{}-{}.png",
        if device.is_empty() { "device" } else { &device },
        ulid::Ulid::from_datetime(SystemTime::now())
    )
}

/// The device's screen, stored as an attachment and answered as the path of
/// its named copy — what an agent reads with its own tools, and what a
/// capture the person marks carries.
pub async fn screenshot(
    inner: &Arc<Inner>,
    project: Option<ProjectId>,
    id: &str,
) -> Result<MobileDevelopmentShot, EngineError> {
    let (bytes, _) = frame(inner, project, id, ImageFormat::Png).await?;
    let (width, height) =
        bisa_mobile_development::png_size(&bytes).map_or((None, None), |(w, h)| (Some(w), Some(h)));
    let attachment = inner
        .ws
        .put_attachment(&bytes, &screenshot_name(id), "image/png")?;
    let path = inner
        .ws
        .put_attachment_named(&attachment.sha256, &attachment.name)?;
    Ok(MobileDevelopmentShot {
        attachment,
        path: path.display().to_string(),
        width,
        height,
    })
}

/// Where `flutter` is, for the run line: the last examination's answer when
/// one stands, else the tools' lookup — a path search, never a run.
fn flutter_program(inner: &Inner) -> Result<Option<PathBuf>, EngineError> {
    if let Some(probe) = inner.mobile_development.toolchain.get(TOOLCHAIN_TTL) {
        if let Some(path) = probe.toolchain.flutter.path {
            return Ok(Some(PathBuf::from(path)));
        }
    }
    Ok(tools(inner)?.flutter_path(&hints(inner)))
}

/// The line a terminal runs to put this checkout's app on a device — the
/// resolved `flutter`, the device's id — and the checkout it runs in. The
/// shell that launches it names nothing of its own (ide/01).
pub fn run_command(
    inner: &Inner,
    wid: WorkstreamId,
    device: &str,
) -> Result<MobileDevelopmentRunCommand, EngineError> {
    let (project, checkout) = crate::projects::checkout_tree_pub(inner, wid)?;
    ensure_on(inner, Some(project.id))?;
    let device = device.trim();
    if device.is_empty() {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-run-names-device"
        )));
    }
    let flutter = flutter_program(inner)?.ok_or_else(|| {
        EngineError::MobileDevelopment(MobileDevelopmentError::NotInstalled("flutter".into()))
    })?;
    Ok(MobileDevelopmentRunCommand {
        command: bisa_mobile_development::run_command(&flutter, device),
        cwd: checkout.display().to_string(),
        device: device.to_string(),
    })
}

/// What a checkout holds, read from its tree: a `pubspec.yaml` naming
/// Flutter, and the platform folders beside it.
pub fn project_facts(
    inner: &Inner,
    wid: WorkstreamId,
) -> Result<MobileDevelopmentProject, EngineError> {
    let (_, checkout) = crate::projects::checkout_tree_pub(inner, wid)?;
    Ok(facts_of_tree(&checkout))
}

/// The facts of one tree — pure over a path, so a test hands in a folder.
pub fn facts_of_tree(root: &Path) -> MobileDevelopmentProject {
    let pubspec = std::fs::read_to_string(root.join("pubspec.yaml")).unwrap_or_default();
    let flutter = pubspec
        .lines()
        .any(|l| l.trim_end() == "flutter:" || l.trim_start().starts_with("sdk: flutter"));
    MobileDevelopmentProject {
        flutter,
        ios: flutter && root.join("ios").is_dir(),
        android: flutter && root.join("android").is_dir(),
    }
}

/// The named copy of a capture's attachment, made on demand — the path an
/// agent reads; `None` when the bytes are not on this machine.
pub fn capture_path(inner: &Inner, shot: &AttachmentRef) -> Option<PathBuf> {
    inner.ws.put_attachment_named(&shot.sha256, &shot.name).ok()
}

/// A `mobile_development.*` write: the examination is stale — a path changed — so the
/// next read runs it again.
pub fn refresh_for(inner: &Inner, key: &str) {
    if key.starts_with("mobile_development.") {
        inner.mobile_development.toolchain.clear();
    }
}

/// One line for the Workflow Agent's GOAL block when this machine develops
/// for mobile — a fact beside the mode, never a rule — from the last
/// examination alone: a wake never probes the machine.
pub fn brief_line(inner: &Inner) -> Option<String> {
    let access = access(inner, None);
    if !access.enabled {
        return None;
    }
    let platforms = access.platforms.words();
    match inner.mobile_development.toolchain.get(TOOLCHAIN_TTL) {
        Some(probe) => {
            let flutter = match (
                probe.toolchain.flutter.installed,
                probe.toolchain.flutter.version.as_deref(),
            ) {
                (true, Some(v)) => format!("Flutter {v}"),
                (true, None) => "Flutter".to_string(),
                (false, _) => "Flutter not installed".to_string(),
            };
            Some(format!(
                "Mobile: {platforms} · {flutter} — mobile_development_devices lists the simulators, emulators and phones here."
            ))
        }
        None => Some(format!(
            "Mobile: on ({platforms}) — mobile_development_status probes the toolchain; mobile_development_devices lists the devices."
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The agent hears the sentence in English (`refusal`); the person hears the
    /// catalog's, in their language — the two are one sentence.
    #[test]
    fn the_person_s_refusal_is_the_agent_s_sentence() {
        assert_eq!(
            bisa_core::text!("error-engine-conflict-mobile-development-off").to_string(),
            OFF
        );
        assert_eq!(
            bisa_core::text!("error-engine-conflict-mobile-development-platform-off").to_string(),
            PLATFORM_OFF
        );
        assert_eq!(
            bisa_core::text!("error-engine-mobile-development-unavailable-no-tools").to_string(),
            NO_TOOLS
        );
    }
    use crate::browser::AgentsPolicy;

    fn access_of(enabled: bool, platforms: Platforms, agents: AgentsPolicy) -> Access {
        Access {
            enabled,
            platforms,
            agents,
        }
    }

    #[test]
    fn access_refuses_in_order_the_switch_the_policy_then_the_platform() {
        let dev = AgentId::new("developer").unwrap();
        let off = access_of(false, Platforms::Both, AgentsPolicy::Everyone);
        assert_eq!(off.refusal(&dev, || true, None), Some(OFF));
        let nobody = access_of(true, Platforms::Both, AgentsPolicy::Nobody);
        assert_eq!(
            nobody.refusal(&AgentId::general(), || true, None),
            Some(NOBODY_MAY)
        );
        let assigned = access_of(true, Platforms::Both, AgentsPolicy::Assigned);
        assert_eq!(assigned.refusal(&dev, || false, None), Some(NOT_ASSIGNED));
        assert_eq!(assigned.refusal(&dev, || true, None), None);
        assert_eq!(
            assigned.refusal(&AgentId::general(), || false, None),
            None,
            "a core agent needs no skill"
        );
        let ios = access_of(true, Platforms::Ios, AgentsPolicy::Everyone);
        assert_eq!(
            ios.refusal(&dev, || true, Some(Platform::Android)),
            Some(PLATFORM_OFF)
        );
        assert_eq!(ios.refusal(&dev, || true, Some(Platform::Ios)), None);
    }

    #[test]
    fn platforms_filter_devices_and_say_themselves() {
        assert!(Platforms::Both.allows(Platform::Ios) && Platforms::Both.allows(Platform::Android));
        assert!(
            Platforms::Android.allows(Platform::Android)
                && !Platforms::Android.allows(Platform::Ios)
        );
        assert_eq!(Platforms::Ios.words(), "iOS only");
        assert_eq!(
            serde_json::to_value(Platforms::Both).unwrap(),
            serde_json::json!("both")
        );
    }

    #[test]
    fn a_screenshot_name_is_a_png_named_after_the_device() {
        let name = screenshot_name("1A2B-3C4D");
        assert!(name.starts_with("mobile-1A2B3C4D-"), "{name}");
        assert!(name.ends_with(".png"));
        assert!(screenshot_name("").starts_with("mobile-device-"));
    }

    #[test]
    fn a_tree_is_a_flutter_app_by_its_pubspec_and_its_platform_folders() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            facts_of_tree(dir.path()),
            MobileDevelopmentProject {
                flutter: false,
                ios: false,
                android: false
            }
        );
        std::fs::write(
            dir.path().join("pubspec.yaml"),
            "name: shop\nenvironment:\n  sdk: ^3.5.0\ndependencies:\n  flutter:\n    sdk: flutter\n",
        )
        .unwrap();
        std::fs::create_dir_all(dir.path().join("ios")).unwrap();
        assert_eq!(
            facts_of_tree(dir.path()),
            MobileDevelopmentProject {
                flutter: true,
                ios: true,
                android: false
            }
        );
        std::fs::write(
            dir.path().join("pubspec.yaml"),
            "name: tool\ndependencies:\n  http: ^1.0.0\n",
        )
        .unwrap();
        assert!(
            !facts_of_tree(dir.path()).flutter,
            "a Dart package is not a Flutter app"
        );
    }
}
