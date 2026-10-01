//! The programs on this machine, asked through [`Exec`]. Every probe is on
//! its own: a missing Xcode says nothing about the Android SDK, and a tool
//! that times out is reported as absent with a log line, never as a failure
//! of the whole examination.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::exec::Exec;
use crate::{
    parse, resolve, AndroidSdk, Device, DeviceKind, DeviceState, FlutterInfo, Hints, ImageFormat,
    MobileDevelopmentError, MobileDevelopmentResult, MobileDevelopmentTools, Platform, ToolInfo,
    Toolchain, XcodeInfo,
};

/// Flutter's first invocation in a while can take a moment (it checks its
/// own cache); the version question gets more than a local budget.
const FLUTTER_QUESTION: Duration = Duration::from_secs(45);
/// How often a starting emulator is looked for.
const BOOT_POLL: Duration = Duration::from_secs(2);
/// Where Android Studio keeps the JDK it bundles, under `/Applications`.
const STUDIO_JAVA: &str = "/Applications/Android Studio.app/Contents/jbr/Contents/Home/bin/java";
/// The exit `simctl boot` answers for a simulator that is already up.
const ALREADY_BOOTED: i32 = 149;

#[derive(Debug, Clone)]
pub struct Real {
    pub exec: Exec,
    /// Where the well-known folders are: this machine's home and Homebrew's.
    pub places: resolve::Places,
}

impl Default for Real {
    fn default() -> Self {
        Self {
            exec: Exec::default(),
            places: resolve::Places::machine(
                dirs::home_dir().unwrap_or_else(|| PathBuf::from("/")),
            ),
        }
    }
}

fn on_path(name: &str) -> Option<PathBuf> {
    which::which(name).ok()
}

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

impl Real {
    fn flutter(&self, hints: &Hints) -> Option<PathBuf> {
        resolve::flutter_path(hints.flutter_path.as_deref(), &self.places, on_path)
    }

    fn sdk(&self, hints: &Hints) -> Option<PathBuf> {
        resolve::android_sdk(
            hints.android_sdk.as_deref(),
            |name| std::env::var(name).ok(),
            &self.places.home,
        )
    }

    /// `xcrun`, which every simulator question goes through; absent off macOS.
    fn xcrun(&self) -> Option<PathBuf> {
        on_path("xcrun").or_else(|| {
            let p = PathBuf::from("/usr/bin/xcrun");
            p.is_file().then_some(p)
        })
    }

    /// What the Android tools and Flutter read to find the SDK.
    fn env_for(sdk: Option<&Path>) -> Vec<(String, String)> {
        sdk.map(|s| vec![("ANDROID_HOME".to_string(), s.display().to_string())])
            .unwrap_or_default()
    }

    fn local(
        &self,
        program: &Path,
        list: &[&str],
        env: &[(String, String)],
    ) -> MobileDevelopmentResult<crate::Output> {
        self.exec
            .run(program, &args(list), env, self.exec.timeouts.local)
    }

    /// One probe's text, or nothing — a missing or failing tool is absent.
    fn probe_text(
        &self,
        program: Option<&Path>,
        list: &[&str],
        timeout: Duration,
    ) -> Option<crate::Output> {
        let program = program?;
        match self.exec.run(program, &args(list), &[], timeout) {
            Ok(out) => Some(out),
            Err(e) => {
                tracing::debug!(program = %program.display(), "mobile probe: {e}");
                None
            }
        }
    }

    fn simctl(
        &self,
        xcrun: &Path,
        list: &[&str],
        timeout: Duration,
    ) -> MobileDevelopmentResult<crate::Output> {
        let mut full = vec!["simctl".to_string()];
        full.extend(list.iter().map(|s| s.to_string()));
        let out = self.exec.run(xcrun, &full, &[], timeout)?;
        Ok(out)
    }

    fn simulators(&self, xcrun: &Path) -> MobileDevelopmentResult<Vec<Device>> {
        let out = self.simctl(
            xcrun,
            &["list", "-j", "devices", "available"],
            self.exec.timeouts.local,
        )?;
        Ok(parse::simctl_devices(&out.text()))
    }

    /// The Android side: what `adb` sees, each running emulator named after
    /// its image, and the images nobody started as shut-down emulators.
    fn android_devices(&self, sdk: Option<&Path>) -> Vec<Device> {
        let env = Self::env_for(sdk);
        let mut out = Vec::new();
        let mut running_images = Vec::new();
        if let Some(adb) = sdk.and_then(resolve::adb_of).or_else(|| on_path("adb")) {
            if let Ok(listed) = self.local(&adb, &["devices", "-l"], &env) {
                for mut d in parse::adb_devices(&listed.text()) {
                    if d.kind == DeviceKind::Emulator {
                        if let Ok(named) =
                            self.local(&adb, &["-s", &d.id, "emu", "avd", "name"], &env)
                        {
                            if let Some(name) = parse::avd_name(&named.text()) {
                                running_images.push(name.clone());
                                d.name = name.replace('_', " ");
                            }
                        }
                    }
                    out.push(d);
                }
            }
        }
        if let Some(emulator) = sdk.and_then(resolve::emulator_of) {
            if let Ok(listed) = self.local(&emulator, &["-list-avds"], &env) {
                for avd in parse::avd_list(&listed.text()) {
                    if running_images.contains(&avd) {
                        continue;
                    }
                    out.push(Device {
                        id: avd.clone(),
                        name: avd.replace('_', " "),
                        platform: Platform::Android,
                        kind: DeviceKind::Emulator,
                        state: DeviceState::Shutdown,
                        os: None,
                    });
                }
            }
        }
        out
    }

    fn adb(&self, hints: &Hints) -> MobileDevelopmentResult<(PathBuf, Vec<(String, String)>)> {
        let sdk = self.sdk(hints);
        let adb = sdk
            .as_deref()
            .and_then(resolve::adb_of)
            .or_else(|| on_path("adb"))
            .ok_or_else(|| MobileDevelopmentError::NotInstalled("adb".into()))?;
        Ok((adb, Self::env_for(sdk.as_deref())))
    }
}

impl MobileDevelopmentTools for Real {
    fn flutter_path(&self, hints: &Hints) -> Option<PathBuf> {
        self.flutter(hints)
    }

    fn toolchain(&self, hints: &Hints) -> Toolchain {
        let mut t = Toolchain::default();
        if let Some(flutter) = self.flutter(hints) {
            t.flutter = FlutterInfo {
                installed: true,
                path: Some(flutter.display().to_string()),
                ..FlutterInfo::default()
            };
            if let Some(out) = self.probe_text(
                Some(&flutter),
                &["--version", "--machine"],
                FLUTTER_QUESTION,
            ) {
                let v = parse::flutter_version(&out.text());
                t.flutter.version = v.version;
                t.flutter.channel = v.channel;
                t.flutter.dart = v.dart;
            }
            if let Some(out) =
                self.probe_text(Some(&flutter), &["doctor"], self.exec.timeouts.doctor)
            {
                t.doctor = parse::doctor_lines(&out.text());
            }
        }
        if let Some(select) = on_path("xcode-select") {
            t.xcode.path = self
                .probe_text(Some(&select), &["-p"], self.exec.timeouts.local)
                .and_then(|o| parse::xcode_select(&o.text()));
        }
        if let Some(build) = on_path("xcodebuild") {
            t.xcode.version = self
                .probe_text(Some(&build), &["-version"], self.exec.timeouts.local)
                .and_then(|o| parse::xcodebuild_version(&o.text()));
        }
        t.xcode = XcodeInfo {
            installed: t.xcode.version.is_some(),
            ..t.xcode
        };
        if let Some(xcrun) = self.xcrun() {
            if let Ok(out) = self.simctl(
                &xcrun,
                &["list", "-j", "runtimes"],
                self.exec.timeouts.local,
            ) {
                t.ios_runtimes = parse::simctl_runtimes(&out.text());
            }
            if let Ok(out) = self.simctl(
                &xcrun,
                &["list", "-j", "devicetypes"],
                self.exec.timeouts.local,
            ) {
                t.ios_devicetypes = parse::simctl_devicetypes(&out.text());
            }
        }
        if let Some(pod) = on_path("pod") {
            t.cocoapods = ToolInfo {
                installed: true,
                version: self
                    .probe_text(Some(&pod), &["--version"], self.exec.timeouts.local)
                    .and_then(|o| parse::pod_version(&o.text())),
            };
        }
        let sdk = self.sdk(hints);
        t.android = AndroidSdk {
            path: sdk.as_ref().map(|p| p.display().to_string()),
            adb: sdk
                .as_deref()
                .and_then(resolve::adb_of)
                .map(|p| p.display().to_string()),
            emulator: sdk
                .as_deref()
                .and_then(resolve::emulator_of)
                .map(|p| p.display().to_string()),
            avds: sdk
                .as_deref()
                .and_then(resolve::emulator_of)
                .and_then(|e| self.probe_text(Some(&e), &["-list-avds"], self.exec.timeouts.local))
                .map(|o| parse::avd_list(&o.text()))
                .unwrap_or_default(),
        };
        let java = on_path("java").or_else(|| {
            let p = PathBuf::from(STUDIO_JAVA);
            p.is_file().then_some(p)
        });
        if let Some(java) = java {
            t.java = ToolInfo {
                installed: true,
                version: self
                    .probe_text(Some(&java), &["-version"], self.exec.timeouts.local)
                    .and_then(|o| parse::java_version(&o.stderr)),
            };
        }
        t
    }

    fn devices(&self, hints: &Hints) -> MobileDevelopmentResult<Vec<Device>> {
        let mut out = Vec::new();
        if let Some(xcrun) = self.xcrun() {
            match self.simulators(&xcrun) {
                Ok(sims) => out.extend(sims),
                Err(e) => tracing::debug!("mobile devices: simctl: {e}"),
            }
        }
        out.extend(self.android_devices(self.sdk(hints).as_deref()));
        // Up first, then by platform, then by name: what a picker wants.
        out.sort_by(|a, b| {
            b.state
                .is_up()
                .cmp(&a.state.is_up())
                .then(a.platform.as_str().cmp(b.platform.as_str()))
                .then(a.name.cmp(&b.name))
        });
        Ok(out)
    }

    fn boot(&self, hints: &Hints, device: &Device) -> MobileDevelopmentResult<()> {
        match (device.platform, device.kind) {
            (Platform::Ios, DeviceKind::Simulator) if device.state.is_up() => {
                let xcrun = self
                    .xcrun()
                    .ok_or_else(|| MobileDevelopmentError::NotInstalled("xcrun".into()))?;
                let out = self.simctl(&xcrun, &["boot", &device.id], self.exec.timeouts.local)?;
                if !out.success() && out.code != ALREADY_BOOTED {
                    return Err(MobileDevelopmentError::Failed {
                        what: format!("simctl boot {}", device.id),
                        detail: out.stderr.trim().to_string(),
                    });
                }
                self.simctl(
                    &xcrun,
                    &["bootstatus", &device.id, "-b"],
                    self.exec.timeouts.boot,
                )?;
                Ok(())
            }
            (Platform::Android, DeviceKind::Emulator) => {
                if device.state.is_up() {
                    return Ok(());
                }
                let sdk = self.sdk(hints);
                let emulator = sdk
                    .as_deref()
                    .and_then(resolve::emulator_of)
                    .ok_or_else(|| {
                        MobileDevelopmentError::NotInstalled("the Android emulator".into())
                    })?;
                let env = Self::env_for(sdk.as_deref());
                self.exec
                    .spawn_detached(&emulator, &args(&["-avd", &device.id]), &env)?;
                // Up when adb lists a running emulator whose image is this one.
                let deadline = Instant::now() + self.exec.timeouts.boot;
                loop {
                    let running = self.android_devices(sdk.as_deref()).into_iter().any(|d| {
                        d.kind == DeviceKind::Emulator
                            && d.state.is_up()
                            && d.name == device.id.replace('_', " ")
                    });
                    if running {
                        return Ok(());
                    }
                    if Instant::now() >= deadline {
                        return Err(MobileDevelopmentError::Timeout {
                            what: format!("emulator -avd {}", device.id),
                            secs: self.exec.timeouts.boot.as_secs(),
                        });
                    }
                    std::thread::sleep(BOOT_POLL);
                }
            }
            _ => Err(MobileDevelopmentError::Unsupported(format!(
                "{} is a {}: it is not booted from here",
                device.name,
                device.kind.words()
            ))),
        }
    }

    fn shutdown(&self, hints: &Hints, device: &Device) -> MobileDevelopmentResult<()> {
        match (device.platform, device.kind) {
            (Platform::Ios, DeviceKind::Simulator) => {
                let xcrun = self
                    .xcrun()
                    .ok_or_else(|| MobileDevelopmentError::NotInstalled("xcrun".into()))?;
                let out =
                    self.simctl(&xcrun, &["shutdown", &device.id], self.exec.timeouts.local)?;
                if !out.success() && !out.stderr.contains("current state: Shutdown") {
                    return Err(MobileDevelopmentError::Failed {
                        what: format!("simctl shutdown {}", device.id),
                        detail: out.stderr.trim().to_string(),
                    });
                }
                Ok(())
            }
            (Platform::Android, DeviceKind::Emulator) if device.state.is_up() => {
                let (adb, env) = self.adb(hints)?;
                self.local(&adb, &["-s", &device.id, "emu", "kill"], &env)?;
                Ok(())
            }
            (Platform::Android, DeviceKind::Emulator) => Ok(()),
            _ => Err(MobileDevelopmentError::Unsupported(format!(
                "{} is a {}: it is not shut down from here",
                device.name,
                device.kind.words()
            ))),
        }
    }

    fn show(&self, _hints: &Hints, device: &Device) -> MobileDevelopmentResult<()> {
        match device.platform {
            Platform::Ios => {
                let open = PathBuf::from("/usr/bin/open");
                self.exec
                    .spawn_detached(&open, &args(&["-a", "Simulator"]), &[])?;
                Ok(())
            }
            Platform::Android => Err(MobileDevelopmentError::Unsupported(
                "an emulator has a window of its own; a phone has a screen".into(),
            )),
        }
    }

    fn create_simulator(
        &self,
        _hints: &Hints,
        name: &str,
        devicetype: &str,
        runtime: &str,
    ) -> MobileDevelopmentResult<String> {
        let xcrun = self
            .xcrun()
            .ok_or_else(|| MobileDevelopmentError::NotInstalled("xcrun".into()))?;
        let out = self.simctl(
            &xcrun,
            &["create", name, devicetype, runtime],
            self.exec.timeouts.local,
        )?;
        if !out.success() {
            return Err(MobileDevelopmentError::Failed {
                what: format!("simctl create {name}"),
                detail: out.stderr.trim().to_string(),
            });
        }
        let udid = out.text().trim().to_string();
        if udid.is_empty() {
            return Err(MobileDevelopmentError::Failed {
                what: format!("simctl create {name}"),
                detail: "no UDID answered".into(),
            });
        }
        Ok(udid)
    }

    fn screenshot(
        &self,
        hints: &Hints,
        device: &Device,
        format: ImageFormat,
    ) -> MobileDevelopmentResult<Vec<u8>> {
        match (device.platform, device.kind) {
            (Platform::Ios, DeviceKind::Simulator) => {
                let xcrun = self
                    .xcrun()
                    .ok_or_else(|| MobileDevelopmentError::NotInstalled("xcrun".into()))?;
                // A file this call makes and removes: `simctl` writes to a
                // path, never to a pipe.
                let file = tempfile::Builder::new()
                    .prefix("bisa-shot-")
                    .suffix(&format!(".{}", format.as_str()))
                    .tempfile()
                    .map_err(|e| MobileDevelopmentError::Failed {
                        what: "screenshot".into(),
                        detail: format!("temp file: {e}"),
                    })?;
                let path = file.path().display().to_string();
                let kind = format!("--type={}", format.as_str());
                let out = self.simctl(
                    &xcrun,
                    &["io", &device.id, "screenshot", &kind, &path],
                    self.exec.timeouts.local,
                )?;
                if !out.success() {
                    return Err(MobileDevelopmentError::Failed {
                        what: format!("simctl io {} screenshot", device.id),
                        detail: out.stderr.trim().to_string(),
                    });
                }
                std::fs::read(file.path()).map_err(|e| MobileDevelopmentError::Failed {
                    what: "screenshot".into(),
                    detail: format!("reading the capture: {e}"),
                })
            }
            (Platform::Android, _) if device.state.is_up() => {
                let (adb, env) = self.adb(hints)?;
                let out = self.local(
                    &adb,
                    &["-s", &device.id, "exec-out", "screencap", "-p"],
                    &env,
                )?;
                if !out.success() || out.stdout.is_empty() {
                    return Err(MobileDevelopmentError::Failed {
                        what: format!("adb -s {} exec-out screencap", device.id),
                        detail: out.stderr.trim().to_string(),
                    });
                }
                Ok(out.stdout)
            }
            _ => Err(MobileDevelopmentError::Unsupported(format!(
                "{} is {}: nothing is on its screen",
                device.name,
                device.state.words()
            ))),
        }
    }
}
