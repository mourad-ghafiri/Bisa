//! Each program's output read once into a typed value: the crate's prose
//! boundary, on fixtures. Nothing here runs anything.

use crate::{
    Device, DeviceKind, DeviceState, DeviceType, DoctorLine, DoctorState, IosRuntime, Platform,
};
use serde::Deserialize;

/// `flutter --version --machine`: the framework's version and channel, and
/// the Dart it ships.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct FlutterVersion {
    pub version: Option<String>,
    pub channel: Option<String>,
    pub dart: Option<String>,
    pub root: Option<String>,
}

pub fn flutter_version(json: &str) -> FlutterVersion {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Raw {
        framework_version: Option<String>,
        flutter_version: Option<String>,
        channel: Option<String>,
        dart_sdk_version: Option<String>,
        flutter_root: Option<String>,
    }
    // The tool may print a line or two before the object (a first-run
    // notice); the object starts at the first brace.
    let start = json.find('{').unwrap_or(json.len());
    let Ok(raw) = serde_json::from_str::<Raw>(&json[start..]) else {
        return FlutterVersion::default();
    };
    FlutterVersion {
        version: raw.flutter_version.or(raw.framework_version),
        channel: raw.channel,
        // `3.5.3 (build 3.5.3-...)` — the number is what a person reads.
        dart: raw
            .dart_sdk_version
            .map(|d| d.split_whitespace().next().unwrap_or(&d).to_string()),
        root: raw.flutter_root,
    }
}

/// `flutter doctor -v`: the category lines — `[✓] Flutter (Channel stable, …)`,
/// `[✗] Android toolchain - develop for Android devices`, `[!] Xcode - …` —
/// each with what the parentheses say. The indented lines under a category
/// are Flutter's detail and are left to the person's terminal.
pub fn doctor_lines(text: &str) -> Vec<DoctorLine> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim_end();
            let rest = line.strip_prefix('[')?;
            let (mark, rest) = rest.split_once("] ")?;
            let state = match mark {
                "✓" | "√" => DoctorState::Ok,
                "✗" | "X" => DoctorState::Missing,
                "!" => DoctorState::Warning,
                _ => return None,
            };
            let (name, detail) = match rest.find(" (") {
                Some(at) if rest.ends_with(')') => (
                    rest[..at].to_string(),
                    Some(rest[at + 2..rest.len() - 1].to_string()),
                ),
                _ => (rest.to_string(), None),
            };
            // `Android toolchain - develop for Android devices`: the name is
            // before the dash; what follows is Flutter's own blurb.
            let name = name.split(" - ").next().unwrap_or(&name).trim().to_string();
            (!name.is_empty()).then_some(DoctorLine {
                state,
                name,
                detail,
            })
        })
        .collect()
}

/// `xcrun simctl list -j devices available`: every iOS simulator, its OS read
/// off the runtime it belongs to.
pub fn simctl_devices(json: &str) -> Vec<Device> {
    #[derive(Deserialize)]
    struct Raw {
        devices: std::collections::BTreeMap<String, Vec<RawDevice>>,
    }
    #[derive(Deserialize)]
    struct RawDevice {
        udid: String,
        name: String,
        state: String,
        #[serde(rename = "isAvailable", default)]
        is_available: Option<bool>,
    }
    let Ok(raw) = serde_json::from_str::<Raw>(json) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (runtime, devices) in raw.devices {
        let Some(os) = runtime_words(&runtime) else {
            continue;
        };
        for d in devices {
            if d.is_available == Some(false) {
                continue;
            }
            out.push(Device {
                id: d.udid,
                name: d.name,
                platform: Platform::Ios,
                kind: DeviceKind::Simulator,
                state: if d.state.eq_ignore_ascii_case("booted") {
                    DeviceState::Booted
                } else {
                    DeviceState::Shutdown
                },
                os: Some(os.clone()),
            });
        }
    }
    out
}

/// `com.apple.CoreSimulator.SimRuntime.iOS-18-2` → `iOS 18.2`; a runtime of
/// another platform (tvOS, watchOS, visionOS) is `None`.
fn runtime_words(identifier: &str) -> Option<String> {
    let tail = identifier.rsplit('.').next()?;
    let version = tail.strip_prefix("iOS-")?;
    Some(format!("iOS {}", version.replace('-', ".")))
}

/// `xcrun simctl list -j runtimes`: the iOS runtimes, available or not.
pub fn simctl_runtimes(json: &str) -> Vec<IosRuntime> {
    #[derive(Deserialize)]
    struct Raw {
        runtimes: Vec<RawRuntime>,
    }
    #[derive(Deserialize)]
    struct RawRuntime {
        identifier: String,
        name: String,
        version: String,
        #[serde(default)]
        platform: Option<String>,
        #[serde(rename = "isAvailable", default)]
        is_available: Option<bool>,
    }
    let Ok(raw) = serde_json::from_str::<Raw>(json) else {
        return Vec::new();
    };
    raw.runtimes
        .into_iter()
        .filter(|r| {
            r.platform.as_deref().is_none_or(|p| p == "iOS") && r.identifier.contains(".iOS-")
        })
        .map(|r| IosRuntime {
            identifier: r.identifier,
            name: r.name,
            version: r.version,
            available: r.is_available.unwrap_or(true),
        })
        .collect()
}

/// `xcrun simctl list -j devicetypes`: the iPhones and iPads a simulator can
/// be made as.
pub fn simctl_devicetypes(json: &str) -> Vec<DeviceType> {
    #[derive(Deserialize)]
    struct Raw {
        devicetypes: Vec<RawType>,
    }
    #[derive(Deserialize)]
    struct RawType {
        identifier: String,
        name: String,
        #[serde(rename = "productFamily", default)]
        product_family: Option<String>,
    }
    let Ok(raw) = serde_json::from_str::<Raw>(json) else {
        return Vec::new();
    };
    raw.devicetypes
        .into_iter()
        .filter(|t| {
            t.product_family
                .as_deref()
                .is_none_or(|f| f == "iPhone" || f == "iPad")
        })
        .map(|t| DeviceType {
            identifier: t.identifier,
            name: t.name,
        })
        .collect()
}

/// `adb devices -l`: the serial, the state word, and the `model:` column
/// as the name. `emulator-NNNN` is a running emulator, anything else a
/// phone; `offline` and `unauthorized` are both offline to us.
pub fn adb_devices(text: &str) -> Vec<Device> {
    text.lines()
        .skip_while(|l| !l.starts_with("List of devices"))
        .skip(1)
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            let serial = words.next()?;
            let state = words.next()?;
            if serial.starts_with('*') {
                return None;
            }
            let model = line
                .split_whitespace()
                .find_map(|w| w.strip_prefix("model:"))
                .map(|m| m.replace('_', " "));
            let emulator = serial.starts_with("emulator-");
            Some(Device {
                id: serial.to_string(),
                name: model.unwrap_or_else(|| serial.to_string()),
                platform: Platform::Android,
                kind: if emulator {
                    DeviceKind::Emulator
                } else {
                    DeviceKind::Physical
                },
                state: if state == "device" {
                    DeviceState::Running
                } else {
                    DeviceState::Offline
                },
                os: None,
            })
        })
        .collect()
}

/// `emulator -list-avds`: one name per line; the tool's own `INFO |` lines
/// are not names.
pub fn avd_list(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| {
            !l.is_empty()
                && !l.starts_with("INFO")
                && !l.starts_with("WARNING")
                && !l.contains(" | ")
        })
        .map(str::to_string)
        .collect()
}

/// `adb -s emulator-5554 emu avd name`: the AVD's name, then `OK`.
pub fn avd_name(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && *l != "OK")
        .map(str::to_string)
}

/// `xcodebuild -version`: `Xcode 16.0` on the first line.
pub fn xcodebuild_version(text: &str) -> Option<String> {
    text.lines()
        .find_map(|l| l.trim().strip_prefix("Xcode "))
        .map(|v| v.trim().to_string())
}

/// `xcode-select -p`: the developer directory.
pub fn xcode_select(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|l| l.starts_with('/'))
        .map(str::to_string)
}

/// `pod --version`: the version alone.
pub fn pod_version(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|l| l.chars().next().is_some_and(|c| c.is_ascii_digit()))
        .map(str::to_string)
}

/// `java -version` writes to stderr: `openjdk version "17.0.10" 2024-01-16`.
pub fn java_version(stderr: &str) -> Option<String> {
    let line = stderr.lines().find(|l| l.contains("version"))?;
    let quoted = line.split('"').nth(1)?;
    Some(quoted.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flutters_version_is_read_off_the_machine_object_even_after_a_notice() {
        let json = r#"Welcome to Flutter!
{"frameworkVersion":"3.24.3","channel":"stable","repositoryUrl":"https://github.com/flutter/flutter.git","frameworkRevision":"2663184aa7","frameworkCommitDate":"2024-09-11 16:27:48 -0500","engineRevision":"36335019a8","dartSdkVersion":"3.5.3 (build 3.5.3-x)","devToolsVersion":"2.37.3","flutterVersion":"3.24.3","flutterRoot":"/Users/me/flutter"}"#;
        let v = flutter_version(json);
        assert_eq!(v.version.as_deref(), Some("3.24.3"));
        assert_eq!(v.channel.as_deref(), Some("stable"));
        assert_eq!(v.dart.as_deref(), Some("3.5.3"));
        assert_eq!(v.root.as_deref(), Some("/Users/me/flutter"));
        assert_eq!(flutter_version("not json"), FlutterVersion::default());
    }

    #[test]
    fn the_doctors_category_lines_are_read_and_its_detail_lines_left() {
        let text = "Doctor summary (to see all details, run flutter doctor -v):
[✓] Flutter (Channel stable, 3.24.3, on macOS 15.0 24A335 darwin-arm64, locale en-US)
    • Flutter version 3.24.3 on channel stable
[✗] Android toolchain - develop for Android devices
    ✗ Unable to locate Android SDK.
[!] Xcode - develop for iOS and macOS (Xcode 16.0)
    ! CocoaPods not installed.
[√] Chrome - develop for the web
[✓] Connected device (2 available)

! Doctor found issues in 2 categories.
";
        let lines = doctor_lines(text);
        assert_eq!(lines.len(), 5);
        assert_eq!(lines[0].state, DoctorState::Ok);
        assert_eq!(lines[0].name, "Flutter");
        assert_eq!(
            lines[0].detail.as_deref(),
            Some("Channel stable, 3.24.3, on macOS 15.0 24A335 darwin-arm64, locale en-US")
        );
        assert_eq!(lines[1].state, DoctorState::Missing);
        assert_eq!(lines[1].name, "Android toolchain");
        assert_eq!(lines[1].detail, None);
        assert_eq!(lines[2].state, DoctorState::Warning);
        assert_eq!(lines[2].name, "Xcode");
        assert_eq!(lines[2].detail.as_deref(), Some("Xcode 16.0"));
        assert_eq!(lines[3].state, DoctorState::Ok, "the Windows mark");
        assert_eq!(lines[4].detail.as_deref(), Some("2 available"));
    }

    #[test]
    fn simulators_are_read_with_their_os_and_the_other_platforms_left() {
        let json = r#"{"devices":{
  "com.apple.CoreSimulator.SimRuntime.iOS-18-2":[
    {"udid":"AAAA-1","isAvailable":true,"deviceTypeIdentifier":"com.apple.CoreSimulator.SimDeviceType.iPhone-16","state":"Booted","name":"iPhone 16"},
    {"udid":"AAAA-2","isAvailable":true,"deviceTypeIdentifier":"com.apple.CoreSimulator.SimDeviceType.iPad-Pro","state":"Shutdown","name":"iPad Pro 13-inch (M4)"},
    {"udid":"AAAA-3","isAvailable":false,"state":"Shutdown","name":"Broken"}
  ],
  "com.apple.CoreSimulator.SimRuntime.watchOS-11-2":[
    {"udid":"WWWW","isAvailable":true,"state":"Shutdown","name":"Apple Watch"}
  ]}}"#;
        let devices = simctl_devices(json);
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].id, "AAAA-1");
        assert_eq!(devices[0].state, DeviceState::Booted);
        assert_eq!(devices[0].os.as_deref(), Some("iOS 18.2"));
        assert_eq!(devices[0].kind, DeviceKind::Simulator);
        assert_eq!(devices[1].state, DeviceState::Shutdown);
        assert!(simctl_devices("{}").is_empty());
    }

    #[test]
    fn runtimes_and_device_types_are_read_for_ios_alone() {
        let runtimes = r#"{"runtimes":[
  {"identifier":"com.apple.CoreSimulator.SimRuntime.iOS-18-2","name":"iOS 18.2","version":"18.2","platform":"iOS","isAvailable":true},
  {"identifier":"com.apple.CoreSimulator.SimRuntime.iOS-17-5","name":"iOS 17.5","version":"17.5","platform":"iOS","isAvailable":false},
  {"identifier":"com.apple.CoreSimulator.SimRuntime.tvOS-18-2","name":"tvOS 18.2","version":"18.2","platform":"tvOS","isAvailable":true}
]}"#;
        let r = simctl_runtimes(runtimes);
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].name, "iOS 18.2");
        assert!(r[0].available && !r[1].available);
        let types = r#"{"devicetypes":[
  {"productFamily":"iPhone","identifier":"com.apple.CoreSimulator.SimDeviceType.iPhone-16","name":"iPhone 16"},
  {"productFamily":"Apple Watch","identifier":"com.apple.CoreSimulator.SimDeviceType.Apple-Watch-Series-10","name":"Apple Watch Series 10"},
  {"productFamily":"iPad","identifier":"com.apple.CoreSimulator.SimDeviceType.iPad-Pro","name":"iPad Pro"}
]}"#;
        let t = simctl_devicetypes(types);
        assert_eq!(t.len(), 2);
        assert_eq!(t[1].name, "iPad Pro");
    }

    #[test]
    fn adb_lists_emulators_phones_and_the_offline_ones() {
        let text = "* daemon not running; starting now at tcp:5037
* daemon started successfully
List of devices attached
emulator-5554          device product:sdk_gphone64_arm64 model:sdk_gphone64_arm64 device:emu64a transport_id:1
R58M12345AB            device usb:1-1 product:o1sxx model:SM_G991B device:o1s transport_id:2
XYZ789                 unauthorized transport_id:3
ABC123                 offline

";
        let d = adb_devices(text);
        assert_eq!(d.len(), 4);
        assert_eq!(d[0].kind, DeviceKind::Emulator);
        assert_eq!(d[0].state, DeviceState::Running);
        assert_eq!(d[0].name, "sdk gphone64 arm64");
        assert_eq!(d[1].kind, DeviceKind::Physical);
        assert_eq!(d[1].name, "SM G991B");
        assert_eq!(d[2].state, DeviceState::Offline);
        assert_eq!(d[3].state, DeviceState::Offline);
        assert_eq!(d[3].name, "ABC123", "no model: the serial names it");
        assert!(adb_devices("List of devices attached\n").is_empty());
    }

    #[test]
    fn the_small_programs_answers_are_read() {
        assert_eq!(
            avd_list(
                "INFO    | Storing crashdata in: /tmp/x\nPixel_8_API_34\nMedium_Phone_API_35\n"
            ),
            vec!["Pixel_8_API_34", "Medium_Phone_API_35"]
        );
        assert_eq!(
            avd_name("Pixel_8_API_34\nOK\n").as_deref(),
            Some("Pixel_8_API_34")
        );
        assert_eq!(avd_name("OK\n"), None);
        assert_eq!(
            xcodebuild_version("Xcode 16.0\nBuild version 16A242d\n").as_deref(),
            Some("16.0")
        );
        assert_eq!(
            xcodebuild_version("xcode-select: error: tool 'xcodebuild' requires Xcode"),
            None
        );
        assert_eq!(
            xcode_select("/Applications/Xcode.app/Contents/Developer\n").as_deref(),
            Some("/Applications/Xcode.app/Contents/Developer")
        );
        assert_eq!(pod_version("1.15.2\n").as_deref(), Some("1.15.2"));
        assert_eq!(
            pod_version("WARNING: something\n1.16.0"),
            Some("1.16.0".into())
        );
        assert_eq!(
            java_version("openjdk version \"17.0.10\" 2024-01-16\nOpenJDK Runtime Environment Homebrew (build 17.0.10+0)\n")
                .as_deref(),
            Some("17.0.10")
        );
        assert_eq!(java_version("java: command not found"), None);
    }
}
