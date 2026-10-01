//! The mobile toolchain and the devices on this machine (ide/19), as typed,
//! time-boxed subprocess calls — `flutter`, `xcrun simctl`, `xcodebuild`,
//! `adb`, `emulator`, `pod`, `java` — the way `bisa-ssh` treats the OpenSSH
//! programs and `bisa-vcs` treats `git`.
//!
//! What this crate does: says what is installed for Flutter development on
//! iOS and Android and what Flutter's own doctor says ([`Toolchain`]); lists
//! the simulators, emulators and phones this machine can reach ([`Device`]);
//! boots and shuts a simulator or an emulator down, brings the Simulator
//! window forward, creates a simulator, and captures a device's screen as
//! PNG or JPEG bytes. It composes the one `flutter run` line a terminal
//! runs ([`run_command`]) and never runs it: the app runs where a person
//! watches it.
//!
//! Three properties are load-bearing and held by tests:
//!
//! 1. **argv only, never a shell, never a prompt.** One runner ([`exec::Exec`])
//!    spawns every program by absolute path with a null stdin and a hardened
//!    environment, every child time-boxed and terminated on expiry; a program
//!    that wants a window (`emulator`, `open -a Simulator`) is spawned
//!    detached and never waited on.
//! 2. **Nothing of the person's is read but the paths that name a tool.**
//!    The Flutter home and the Android SDK are the workspace's settings, two
//!    environment variables read by name, or the well-known folders under
//!    the home directory ([`resolve`]) — never the environment as a whole.
//! 3. **Every test runs against [`fake::FakeMobileDevelopment`]**: scripted answers,
//!    recorded calls, no program spawned.
//!
//! The parsers ([`parse`]) are the crate's prose boundary: each program's
//! output is read once into a typed value, on fixtures.

pub mod exec;
pub mod fake;
pub mod parse;
pub mod real;
pub mod resolve;

use serde::{Deserialize, Serialize};
use std::path::Path;

pub use exec::{Exec, Output, Timeouts};
pub use fake::FakeMobileDevelopment;
pub use real::Real;

/// A mobile platform this machine develops for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(rename = "MobilePlatform")]
pub enum Platform {
    Ios,
    Android,
}

impl Platform {
    pub fn as_str(self) -> &'static str {
        match self {
            Platform::Ios => "ios",
            Platform::Android => "android",
        }
    }

    /// The platform's name as a sentence says it.
    pub fn words(self) -> &'static str {
        match self {
            Platform::Ios => "iOS",
            Platform::Android => "Android",
        }
    }
}

/// What a device is: Apple's simulator, Android's emulator, or a phone or
/// tablet plugged in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DeviceKind {
    Simulator,
    Emulator,
    Physical,
}

impl DeviceKind {
    pub fn words(self) -> &'static str {
        match self {
            DeviceKind::Simulator => "simulator",
            DeviceKind::Emulator => "emulator",
            DeviceKind::Physical => "device",
        }
    }
}

/// Where a device stands: a simulator is `booted` or `shutdown`; an emulator
/// or a phone `adb` sees is `running`, or `offline` when it is plugged in but
/// not answering (unauthorized, asleep); an emulator image nobody started is
/// `shutdown`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DeviceState {
    Booted,
    Shutdown,
    Running,
    Offline,
}

impl DeviceState {
    /// Whether an app can run on it now.
    pub fn is_up(self) -> bool {
        matches!(self, DeviceState::Booted | DeviceState::Running)
    }

    pub fn words(self) -> &'static str {
        match self {
            DeviceState::Booted => "booted",
            DeviceState::Shutdown => "shut down",
            DeviceState::Running => "running",
            DeviceState::Offline => "offline",
        }
    }
}

/// The encoding of a captured screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ImageFormat {
    Png,
    Jpeg,
}

impl ImageFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            ImageFormat::Png => "png",
            ImageFormat::Jpeg => "jpeg",
        }
    }

    pub fn mime(self) -> &'static str {
        match self {
            ImageFormat::Png => "image/png",
            ImageFormat::Jpeg => "image/jpeg",
        }
    }
}

/// One device this machine can reach: a simulator's UDID, an emulator's AVD
/// name or `adb` serial, a phone's serial.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub platform: Platform,
    pub kind: DeviceKind,
    pub state: DeviceState,
    /// The OS the device runs, when the program said (`iOS 18.2`, `Android 14`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os: Option<String>,
}

impl Device {
    /// The device as one line names it: `iPhone 16 · iOS 18.2 · simulator · booted`.
    pub fn words(&self) -> String {
        let mut out = self.name.clone();
        if let Some(os) = &self.os {
            out.push_str(" · ");
            out.push_str(os);
        }
        format!("{out} · {} · {}", self.kind.words(), self.state.words())
    }
}

/// Flutter as found: where, which version and channel, which Dart.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct FlutterInfo {
    pub installed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dart: Option<String>,
}

/// Xcode as found: the developer directory `xcode-select` names and the
/// version `xcodebuild` says.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct XcodeInfo {
    pub installed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// A tool that is there or not, with the version it says.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ToolInfo {
    pub installed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// One iOS runtime the simulator can boot (`com.apple.CoreSimulator.SimRuntime.iOS-18-2`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct IosRuntime {
    pub identifier: String,
    pub name: String,
    pub version: String,
    pub available: bool,
}

/// One device type a simulator can be made as (`com.apple.CoreSimulator.SimDeviceType.iPhone-16`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DeviceType {
    pub identifier: String,
    pub name: String,
}

/// The Android SDK as found: its folder, the two programs under it, and the
/// virtual devices made in it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AndroidSdk {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adb: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emulator: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub avds: Vec<String>,
}

/// One line of `flutter doctor`: a category and Flutter's own verdict on it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DoctorState {
    Ok,
    Missing,
    Warning,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DoctorLine {
    pub state: DoctorState,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// What is installed here for mobile development, every part probed on its
/// own so one missing tool never hides the others.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Toolchain {
    pub flutter: FlutterInfo,
    pub xcode: XcodeInfo,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ios_runtimes: Vec<IosRuntime>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ios_devicetypes: Vec<DeviceType>,
    pub cocoapods: ToolInfo,
    pub android: AndroidSdk,
    pub java: ToolInfo,
    /// Flutter's own word, line by line, when Flutter is there.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub doctor: Vec<DoctorLine>,
}

/// The two paths the workspace may name (`mobile_development.flutter.path`,
/// `mobile_development.android.sdk`), passed to every call so the runner stays stateless.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Hints {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flutter_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub android_sdk: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MobileDevelopmentError {
    /// The program the call needs is not on this machine.
    #[error("{0} is not installed on this machine")]
    NotInstalled(String),
    /// The child exceeded its budget and was terminated.
    #[error("{what} timed out after {secs}s")]
    Timeout { what: String, secs: u64 },
    /// The program answered with a failure, in its own words.
    #[error("{what}: {detail}")]
    Failed { what: String, detail: String },
    /// No device answers to that id.
    #[error("no device {0}")]
    NoSuchDevice(String),
    /// The call makes no sense for that device (a phone cannot be booted).
    #[error("{0}")]
    Unsupported(String),
}

pub type MobileDevelopmentResult<T> = Result<T, MobileDevelopmentError>;

/// The port: what the engine asks of this machine's mobile tools. Synchronous
/// — the engine calls it from `spawn_blocking`.
pub trait MobileDevelopmentTools: Send + Sync + std::fmt::Debug {
    /// What is installed, every part probed on its own; never an error.
    fn toolchain(&self, hints: &Hints) -> Toolchain;
    /// Where `flutter` is, without running it: the hint, `PATH`, the
    /// well-known folders ([`resolve::flutter_path`]).
    fn flutter_path(&self, hints: &Hints) -> Option<std::path::PathBuf>;
    /// The devices this machine can reach, simulators and emulators first.
    fn devices(&self, hints: &Hints) -> MobileDevelopmentResult<Vec<Device>>;
    /// Boot a simulator, or start an emulator's image; answers once it is up.
    fn boot(&self, hints: &Hints, device: &Device) -> MobileDevelopmentResult<()>;
    /// Shut a simulator or an emulator down.
    fn shutdown(&self, hints: &Hints, device: &Device) -> MobileDevelopmentResult<()>;
    /// Bring the device's window forward: the Simulator app for iOS; an
    /// emulator has its own window already.
    fn show(&self, hints: &Hints, device: &Device) -> MobileDevelopmentResult<()>;
    /// Make a simulator; answers its UDID.
    fn create_simulator(
        &self,
        hints: &Hints,
        name: &str,
        devicetype: &str,
        runtime: &str,
    ) -> MobileDevelopmentResult<String>;
    /// The device's screen as image bytes. Android answers PNG whatever was
    /// asked: `screencap` writes nothing else.
    fn screenshot(
        &self,
        hints: &Hints,
        device: &Device,
        format: ImageFormat,
    ) -> MobileDevelopmentResult<Vec<u8>>;
}

/// The one line a terminal runs to put the app on a device: the resolved
/// `flutter`, quoted, and the device's id. Composed here so the shell that
/// launches it never names a program of its own.
pub fn run_command(flutter: &Path, device: &str) -> String {
    let program = flutter.to_string_lossy();
    format!(
        "{} run -d {}",
        shlex::try_quote(&program)
            .map(|s| s.into_owned())
            .unwrap_or_else(|_| program.into_owned()),
        shlex::try_quote(device)
            .map(|s| s.into_owned())
            .unwrap_or_else(|_| device.to_string())
    )
}

/// A PNG's width and height from its header, when the bytes are a PNG.
pub fn png_size(bytes: &[u8]) -> Option<(u32, u32)> {
    const SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
    if bytes.len() < 24 || !bytes.starts_with(SIGNATURE) || &bytes[12..16] != b"IHDR" {
        return None;
    }
    let width = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
    let height = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
    Some((width, height))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_command_quotes_a_path_with_spaces_and_the_device_id() {
        let line = run_command(
            Path::new("/Users/me/dev tools/flutter/bin/flutter"),
            "1A2B-3C4D",
        );
        assert_eq!(
            line,
            "'/Users/me/dev tools/flutter/bin/flutter' run -d 1A2B-3C4D"
        );
        assert_eq!(
            run_command(Path::new("/opt/homebrew/bin/flutter"), "emulator-5554"),
            "/opt/homebrew/bin/flutter run -d emulator-5554"
        );
    }

    #[test]
    fn png_size_reads_the_ihdr_and_refuses_anything_else() {
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        png.extend_from_slice(&13u32.to_be_bytes());
        png.extend_from_slice(b"IHDR");
        png.extend_from_slice(&1170u32.to_be_bytes());
        png.extend_from_slice(&2532u32.to_be_bytes());
        png.extend_from_slice(&[8, 6, 0, 0, 0]);
        assert_eq!(png_size(&png), Some((1170, 2532)));
        assert_eq!(png_size(b"\xFF\xD8\xFF not a png"), None);
        assert_eq!(png_size(&png[..20]), None);
    }

    #[test]
    fn a_device_says_itself_in_one_line() {
        let d = Device {
            id: "X".into(),
            name: "iPhone 16".into(),
            platform: Platform::Ios,
            kind: DeviceKind::Simulator,
            state: DeviceState::Booted,
            os: Some("iOS 18.2".into()),
        };
        assert_eq!(d.words(), "iPhone 16 · iOS 18.2 · simulator · booted");
        assert!(DeviceState::Running.is_up() && !DeviceState::Offline.is_up());
        assert_eq!(ImageFormat::Jpeg.mime(), "image/jpeg");
    }
}
