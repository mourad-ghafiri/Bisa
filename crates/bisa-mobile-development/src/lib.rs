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
//!    spawns every program by absolute path with a null stdin, a hardened
//!    environment and a group of its own; every child is time-boxed, and on
//!    expiry it and what it started are terminated; what it writes is read
//!    up to a cap. A program that keeps a window (`emulator`) is spawned
//!    detached, watched until its device is up, and reaped when it ends.
//! 2. **Nothing of the person's is read but the paths that name a tool.**
//!    The Flutter home and the Android SDK are the workspace's settings, two
//!    environment variables read by name, or the well-known folders under
//!    the home directory ([`resolve`]) — never the environment as a whole.
//! 3. **Every caller's test runs against [`fake::FakeMobileDevelopment`]**:
//!    scripted answers, recorded calls, no program spawned. The real tools
//!    ([`real::Real`]) are held by this crate's own tests against scripted
//!    stand-ins of each program, in sealed places ([`resolve::Places::sealed`]).
//!
//! The parsers ([`parse`]) are the crate's prose boundary: each program's
//! output is read once into a typed value, on fixtures.

/// Variables of the node's own environment no child of this crate may
/// inherit: the control-plane token. The one list `bisa-harness` keeps
/// (`proc::SCRUBBED_ENV`), spelled here because this crate stands below it;
/// held equal by the workspace's layering test.
pub const SCRUBBED_ENV: &[&str] = &["BISA_API_TOKEN"];

pub mod exec;
pub mod fake;
pub mod parse;
pub mod real;
pub mod resolve;

use serde::{Deserialize, Serialize};
use std::path::Path;

pub use exec::{Detached, Exec, Limits, Output, Timeouts};
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

/// Where a device stands: a simulator is `booted`, `shutdown`, or `booting`
/// on its way up; an emulator or a phone `adb` sees is `running`, or
/// `offline` when it is there but not answering (unauthorized, asleep, still
/// starting); an emulator image nobody started is `shutdown`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DeviceState {
    Booted,
    Shutdown,
    Running,
    Offline,
    Booting,
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
            DeviceState::Booting => "booting",
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
    /// The OS the device runs, when the program said (`iOS 18.2`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os: Option<String>,
    /// The image an emulator runs, by its name (`Pixel_8`). A shut-down
    /// emulator's id is this name; a running one's id is its `adb` serial —
    /// the image is what says the two are one device.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
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

/// One of the two paths the workspace may name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(rename = "MobileDevelopmentHint")]
pub enum Hint {
    FlutterPath,
    AndroidSdk,
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
    /// The paths the workspace names that name nothing on this machine: a
    /// setting that is said and wrong is never passed over for another place.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unusable_hints: Vec<Hint>,
}

/// What a listing found, and what it could not ask: one platform's tool
/// failing never hides the other platform's devices, and is never read as
/// "no devices".
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Listing {
    pub devices: Vec<Device>,
    pub problems: Vec<ListingProblem>,
}

/// One platform's listing that failed, in the tool's own error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListingProblem {
    pub platform: Platform,
    pub error: MobileDevelopmentError,
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
    /// The devices this machine can reach: the ones that are up first, then
    /// by platform, then by name.
    fn devices(&self, hints: &Hints) -> MobileDevelopmentResult<Vec<Device>>;
    /// The devices of one platform or of both, with what could not be
    /// asked. A port that knows no better answers [`Self::devices`] and no
    /// problem.
    fn listing(&self, hints: &Hints, only: Option<Platform>) -> Listing {
        match self.devices(hints) {
            Ok(devices) => Listing {
                devices: devices
                    .into_iter()
                    .filter(|d| only.is_none_or(|p| d.platform == p))
                    .collect(),
                problems: Vec::new(),
            },
            Err(error) => Listing {
                devices: Vec::new(),
                problems: [Platform::Ios, Platform::Android]
                    .into_iter()
                    .filter(|p| only.is_none_or(|o| o == *p))
                    .map(|platform| ListingProblem {
                        platform,
                        error: error.clone(),
                    })
                    .collect(),
            },
        }
    }
    /// Boot a simulator, or start an emulator's image; answers the device
    /// once it is up — an emulator under the id it runs as.
    fn boot(&self, hints: &Hints, device: &Device) -> MobileDevelopmentResult<Device>;
    /// Shut a simulator or an emulator down; answers the device as it
    /// stands — an emulator under its image's name again.
    fn shutdown(&self, hints: &Hints, device: &Device) -> MobileDevelopmentResult<Device>;
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

/// The order a listing is answered in: what is up first, then by platform,
/// then by name — what a picker wants.
pub fn sort_for_a_picker(devices: &mut [Device]) {
    devices.sort_by(|a, b| {
        b.state
            .is_up()
            .cmp(&a.state.is_up())
            .then(a.platform.as_str().cmp(b.platform.as_str()))
            .then(a.name.cmp(&b.name))
    });
}

/// The one line a terminal runs to put the app on a device: the resolved
/// `flutter`, quoted, and the device's id, quoted. Composed here so the
/// shell that launches it never names a program of its own. A path or an id
/// no quoting can carry is refused: nothing unquoted ever reaches a shell.
pub fn run_command(flutter: &Path, device: &str) -> MobileDevelopmentResult<String> {
    let program = flutter.to_string_lossy();
    let unwritable = |what: &str| {
        MobileDevelopmentError::Unsupported(format!("{what} cannot be written on a command line"))
    };
    Ok(format!(
        "{} run -d {}",
        shlex::try_quote(&program).map_err(|_| unwritable("the path to flutter"))?,
        shlex::try_quote(device).map_err(|_| unwritable("the device's id"))?
    ))
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
        )
        .unwrap();
        assert_eq!(
            line,
            "'/Users/me/dev tools/flutter/bin/flutter' run -d 1A2B-3C4D"
        );
        assert_eq!(
            run_command(Path::new("/opt/homebrew/bin/flutter"), "emulator-5554").unwrap(),
            "/opt/homebrew/bin/flutter run -d emulator-5554"
        );
        // An id that would end the command is carried as one word.
        assert_eq!(
            run_command(Path::new("/f"), "x; touch y").unwrap(),
            "/f run -d 'x; touch y'"
        );
    }

    /// A NUL cannot be quoted: the id is refused, never written bare.
    #[test]
    fn an_id_or_a_path_no_quoting_carries_is_refused() {
        let refused = run_command(Path::new("/f"), "a\0;touch x");
        assert!(
            matches!(refused, Err(MobileDevelopmentError::Unsupported(ref why)) if why.contains("device")),
            "{refused:?}"
        );
        let refused = run_command(Path::new("/f\0x"), "A");
        assert!(
            matches!(refused, Err(MobileDevelopmentError::Unsupported(ref why)) if why.contains("flutter")),
            "{refused:?}"
        );
    }

    /// A port that only lists answers a listing from its list: filtered to
    /// the platform asked, and a failure said for each platform asked.
    #[test]
    fn a_listing_is_the_list_filtered_or_its_failure_per_platform() {
        let fake = FakeMobileDevelopment::new().with_devices(vec![
            fake::simulator("A", "iPhone 16", DeviceState::Booted),
            fake::emulator("Pixel_8", "Pixel 8", DeviceState::Shutdown),
        ]);
        let hints = Hints::default();
        assert_eq!(fake.listing(&hints, None).devices.len(), 2);
        let ios = fake.listing(&hints, Some(Platform::Ios));
        assert_eq!(
            (
                ios.devices.len(),
                ios.devices[0].id.as_str(),
                ios.problems.len()
            ),
            (1, "A", 0)
        );
        let broken = FakeMobileDevelopment::new().failing(
            "devices",
            MobileDevelopmentError::NotInstalled("adb".into()),
        );
        assert_eq!(broken.listing(&hints, None).problems.len(), 2);
        let one = broken.listing(&hints, Some(Platform::Android));
        assert_eq!(one.problems.len(), 1);
        assert_eq!(one.problems[0].platform, Platform::Android);
        assert!(one.devices.is_empty());
    }

    #[test]
    fn a_picker_reads_what_is_up_first_then_platform_then_name() {
        let mut devices = vec![
            fake::emulator("Pixel_8", "Pixel 8", DeviceState::Shutdown),
            fake::simulator("B", "iPhone 16", DeviceState::Shutdown),
            fake::emulator("emulator-5554", "Pixel 7", DeviceState::Running),
            fake::simulator("A", "iPad", DeviceState::Booting),
            fake::simulator("C", "iPhone 15", DeviceState::Booted),
        ];
        sort_for_a_picker(&mut devices);
        assert_eq!(
            devices.iter().map(|d| d.id.as_str()).collect::<Vec<_>>(),
            ["emulator-5554", "C", "Pixel_8", "A", "B"]
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
    fn every_word_of_the_vocabulary_is_said() {
        assert_eq!(
            (Platform::Ios.as_str(), Platform::Android.as_str()),
            ("ios", "android")
        );
        assert_eq!(
            (Platform::Ios.words(), Platform::Android.words()),
            ("iOS", "Android")
        );
        assert_eq!(
            [
                DeviceKind::Simulator,
                DeviceKind::Emulator,
                DeviceKind::Physical
            ]
            .map(DeviceKind::words),
            ["simulator", "emulator", "device"]
        );
        assert_eq!(
            [
                DeviceState::Booted,
                DeviceState::Shutdown,
                DeviceState::Running,
                DeviceState::Offline,
                DeviceState::Booting
            ]
            .map(DeviceState::words),
            ["booted", "shut down", "running", "offline", "booting"]
        );
        assert_eq!(
            (ImageFormat::Png.as_str(), ImageFormat::Jpeg.as_str()),
            ("png", "jpeg")
        );
        assert_eq!(
            (ImageFormat::Png.mime(), ImageFormat::Jpeg.mime()),
            ("image/png", "image/jpeg")
        );
        let plain = Device {
            id: "P".into(),
            name: "Pixel".into(),
            platform: Platform::Android,
            kind: DeviceKind::Physical,
            state: DeviceState::Offline,
            os: None,
            image: None,
        };
        assert_eq!(
            plain.words(),
            "Pixel · device · offline",
            "no OS, no OS words"
        );
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
            image: None,
        };
        assert_eq!(d.words(), "iPhone 16 · iOS 18.2 · simulator · booted");
        assert!(DeviceState::Running.is_up() && !DeviceState::Offline.is_up());
        assert!(!DeviceState::Booting.is_up(), "an app cannot run on it yet");
        assert_eq!(ImageFormat::Jpeg.mime(), "image/jpeg");
    }
}
