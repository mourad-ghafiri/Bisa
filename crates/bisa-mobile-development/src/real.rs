//! The programs on this machine, asked through [`Exec`]. Every probe is on
//! its own: a missing Xcode says nothing about the Android SDK, and a tool
//! that times out is reported as absent with a log line, never as a failure
//! of the whole examination. A listing is the same: one platform's tool
//! failing is said beside the other platform's devices.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::exec::Exec;
use crate::{
    parse, resolve, AndroidSdk, Device, DeviceKind, DeviceState, FlutterInfo, Hint, Hints,
    ImageFormat, Listing, ListingProblem, MobileDevelopmentError, MobileDevelopmentResult,
    MobileDevelopmentTools, Platform, ToolInfo, Toolchain, XcodeInfo,
};

/// Flutter's first invocation in a while can take a moment (it checks its
/// own cache); the version question gets more than a local budget.
const FLUTTER_QUESTION: Duration = Duration::from_secs(45);
/// How often a stopping emulator is looked for, at most.
const SHUTDOWN_POLL: Duration = Duration::from_millis(500);
/// Where Android Studio keeps the JDK it bundles, under `/Applications`.
const STUDIO_JAVA: &str = "/Applications/Android Studio.app/Contents/jbr/Contents/Home/bin/java";
/// Where macOS keeps the two programs every Mac has.
const XCRUN: &str = "/usr/bin/xcrun";
const OPEN: &str = "/usr/bin/open";
/// The exit `simctl boot` answers for a simulator that is already up.
const ALREADY_BOOTED: i32 = 149;

#[derive(Debug, Clone)]
pub struct Real {
    pub exec: Exec,
    /// Where the well-known folders are and where a program is looked for:
    /// this machine's home, Homebrew's folders and `PATH`.
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

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

fn failed(what: impl Into<String>, out: &crate::Output) -> MobileDevelopmentError {
    MobileDevelopmentError::Failed {
        what: what.into(),
        detail: out.complaint(),
    }
}

impl Real {
    fn flutter(&self, hints: &Hints) -> Option<PathBuf> {
        resolve::flutter_path(hints.flutter_path.as_deref(), &self.places, |name| {
            self.places.program(name)
        })
    }

    fn sdk(&self, hints: &Hints) -> Option<PathBuf> {
        resolve::android_sdk(
            hints.android_sdk.as_deref(),
            |name| {
                (!self.places.is_sealed())
                    .then(|| std::env::var(name).ok())
                    .flatten()
            },
            &self.places.home,
        )
    }

    /// Whether a developer directory is selected. Asked before anything
    /// goes through `xcrun` or `xcodebuild`: on a Mac with no developer
    /// tools those two are stand-ins that offer to install them, and this
    /// crate never raises a window of its own. Where no `xcode-select`
    /// exists there is nothing to ask.
    fn developer_dir(&self) -> Option<String> {
        let select = self.places.program("xcode-select")?;
        let out = self.local(&select, &["-p"], &[]).ok()?;
        out.success()
            .then(|| parse::xcode_select(&out.text()))
            .flatten()
    }

    /// `xcrun`, which every simulator question goes through; absent off
    /// macOS and on a Mac with no developer directory selected.
    fn xcrun(&self) -> Option<PathBuf> {
        let xcrun = self
            .places
            .program("xcrun")
            .or_else(|| self.places.fixed(XCRUN))?;
        match self.places.program("xcode-select") {
            Some(_) => self.developer_dir().map(|_| xcrun),
            None => Some(xcrun),
        }
    }

    fn need_xcrun(&self) -> MobileDevelopmentResult<PathBuf> {
        self.xcrun()
            .ok_or_else(|| MobileDevelopmentError::NotInstalled("xcrun".into()))
    }

    /// What the Android tools and Flutter read to find the SDK.
    fn env_for(sdk: Option<&Path>) -> Vec<(String, String)> {
        sdk.map(|s| vec![("ANDROID_HOME".to_string(), s.display().to_string())])
            .unwrap_or_default()
    }

    /// `adb`: the SDK's own, else the one a program is found by name.
    fn adb_program(&self, sdk: Option<&Path>) -> Option<PathBuf> {
        sdk.and_then(resolve::adb_of)
            .or_else(|| self.places.program("adb"))
    }

    fn adb(&self, hints: &Hints) -> MobileDevelopmentResult<(PathBuf, Vec<(String, String)>)> {
        let sdk = self.sdk(hints);
        let adb = self
            .adb_program(sdk.as_deref())
            .ok_or_else(|| MobileDevelopmentError::NotInstalled("adb".into()))?;
        Ok((adb, Self::env_for(sdk.as_deref())))
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

    /// One probe's answer, or nothing — a missing or failing tool is absent.
    fn probe(
        &self,
        program: Option<&Path>,
        list: &[&str],
        env: &[(String, String)],
        timeout: Duration,
    ) -> Option<crate::Output> {
        let program = program?;
        match self.exec.run(program, &args(list), env, timeout) {
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
        self.exec.run(xcrun, &full, &[], timeout)
    }

    /// `simctl` asked for something it must do: a refusal is a failure in
    /// its own words.
    fn simctl_does(
        &self,
        xcrun: &Path,
        list: &[&str],
        timeout: Duration,
    ) -> MobileDevelopmentResult<crate::Output> {
        let out = self.simctl(xcrun, list, timeout)?;
        if !out.success() {
            return Err(failed(format!("simctl {}", list.join(" ")), &out));
        }
        Ok(out)
    }

    fn simulators(&self, xcrun: &Path) -> MobileDevelopmentResult<Vec<Device>> {
        let out = self.simctl_does(
            xcrun,
            &["list", "-j", "devices", "available"],
            self.exec.timeouts.local,
        )?;
        Ok(parse::simctl_devices(&out.text()))
    }

    /// What `adb` lists, as it lists it.
    fn adb_listed(
        &self,
        adb: &Path,
        env: &[(String, String)],
    ) -> MobileDevelopmentResult<Vec<Device>> {
        let out = self.local(adb, &["devices", "-l"], env)?;
        if !out.success() {
            return Err(failed("adb devices", &out));
        }
        Ok(parse::adb_devices(&out.text()))
    }

    /// What `adb` lists, each emulator named after the image it runs. The
    /// emulator's console answers whether or not the device does, so one
    /// that is still starting is named too; one whose console does not
    /// answer keeps the name `adb` gave it.
    fn adb_devices(
        &self,
        adb: &Path,
        env: &[(String, String)],
    ) -> MobileDevelopmentResult<Vec<Device>> {
        let mut listed = self.adb_listed(adb, env)?;
        for d in listed.iter_mut().filter(|d| d.kind == DeviceKind::Emulator) {
            match self.local(adb, &["-s", &d.id, "emu", "avd", "name"], env) {
                Ok(named) if named.success() => {
                    if let Some(image) = parse::avd_name(&named.text()) {
                        d.name = image.replace('_', " ");
                        d.image = Some(image);
                    }
                }
                unanswered => tracing::debug!(serial = %d.id, "adb emu avd name: {unanswered:?}"),
            }
        }
        Ok(listed)
    }

    /// The Android side: what `adb` sees, and the images nobody started as
    /// shut-down emulators — with what either tool could not answer.
    fn android(&self, sdk: Option<&Path>) -> (Vec<Device>, Vec<MobileDevelopmentError>) {
        let env = Self::env_for(sdk);
        let (mut devices, mut problems) = (Vec::new(), Vec::new());
        if let Some(adb) = self.adb_program(sdk) {
            match self.adb_devices(&adb, &env) {
                Ok(found) => devices.extend(found),
                Err(e) => problems.push(e),
            }
        }
        if let Some(emulator) = sdk.and_then(resolve::emulator_of) {
            match self.local(&emulator, &["-list-avds"], &env) {
                Ok(listed) if listed.success() => {
                    for image in parse::avd_list(&listed.text()) {
                        if devices
                            .iter()
                            .any(|d| d.image.as_deref() == Some(image.as_str()))
                        {
                            continue;
                        }
                        devices.push(Device {
                            id: image.clone(),
                            name: image.replace('_', " "),
                            platform: Platform::Android,
                            kind: DeviceKind::Emulator,
                            state: DeviceState::Shutdown,
                            os: None,
                            image: Some(image),
                        });
                    }
                }
                Ok(listed) => problems.push(failed("emulator -list-avds", &listed)),
                Err(e) => problems.push(e),
            }
        }
        (devices, problems)
    }

    /// The emulator's device once `adb` lists it running; an emulator that
    /// ended with a failure before that is said so, not waited out.
    fn await_emulator(
        &self,
        image: &str,
        window: &mut crate::Detached,
        adb: &Path,
        env: &[(String, String)],
    ) -> MobileDevelopmentResult<Device> {
        let what = format!("emulator -avd {image}");
        let deadline = Instant::now() + self.exec.timeouts.boot;
        loop {
            // A listing that fails while the emulator starts is asked again.
            if let Ok(found) = self.adb_devices(adb, env) {
                if let Some(up) = found.into_iter().find(|d| {
                    d.kind == DeviceKind::Emulator
                        && d.state.is_up()
                        && d.image.as_deref() == Some(image)
                }) {
                    return Ok(up);
                }
            }
            // A launcher that hands over to the emulator proper ends with 0;
            // anything else is the emulator refusing to start.
            if let Some(code) = window.exited().filter(|code| *code != 0) {
                return Err(MobileDevelopmentError::Failed {
                    what,
                    detail: format!(
                        "the emulator ended before its device came up (exit status {code})"
                    ),
                });
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(MobileDevelopmentError::Timeout {
                    what,
                    secs: self.exec.timeouts.boot.as_secs().max(1),
                });
            }
            std::thread::sleep(self.exec.timeouts.poll.min(remaining));
        }
    }

    /// Wait until `adb` no longer lists a serial: an emulator told to end
    /// takes a moment to go.
    fn await_gone(
        &self,
        serial: &str,
        adb: &Path,
        env: &[(String, String)],
    ) -> MobileDevelopmentResult<()> {
        let deadline = Instant::now() + self.exec.timeouts.shutdown;
        loop {
            if let Ok(listed) = self.adb_listed(adb, env) {
                if !listed.iter().any(|d| d.id == serial) {
                    return Ok(());
                }
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(MobileDevelopmentError::Timeout {
                    what: format!("adb -s {serial} emu kill"),
                    secs: self.exec.timeouts.shutdown.as_secs().max(1),
                });
            }
            std::thread::sleep(self.exec.timeouts.poll.min(SHUTDOWN_POLL).min(remaining));
        }
    }
}

impl MobileDevelopmentTools for Real {
    fn flutter_path(&self, hints: &Hints) -> Option<PathBuf> {
        self.flutter(hints)
    }

    fn toolchain(&self, hints: &Hints) -> Toolchain {
        let mut t = Toolchain::default();
        let sdk = self.sdk(hints);
        // Flutter is told where the SDK is, as the Android tools are: its
        // doctor then speaks of the same SDK this examination does.
        let env = Self::env_for(sdk.as_deref());
        let flutter = self.flutter(hints);
        if resolve::said(hints.flutter_path.as_deref()).is_some() && flutter.is_none() {
            t.unusable_hints.push(Hint::FlutterPath);
        }
        if resolve::said(hints.android_sdk.as_deref()).is_some() && sdk.is_none() {
            t.unusable_hints.push(Hint::AndroidSdk);
        }
        if let Some(flutter) = flutter {
            t.flutter = FlutterInfo {
                installed: true,
                path: Some(flutter.display().to_string()),
                ..FlutterInfo::default()
            };
            if let Some(out) = self.probe(
                Some(&flutter),
                &["--version", "--machine"],
                &env,
                FLUTTER_QUESTION,
            ) {
                let v = parse::flutter_version(&out.text());
                t.flutter.version = v.version;
                t.flutter.channel = v.channel;
                t.flutter.dart = v.dart;
            }
            if let Some(out) =
                self.probe(Some(&flutter), &["doctor"], &env, self.exec.timeouts.doctor)
            {
                t.doctor = parse::doctor_lines(&out.text());
            }
        }
        t.xcode.path = self.developer_dir();
        if t.xcode.path.is_some() {
            t.xcode.version = self
                .probe(
                    self.places.program("xcodebuild").as_deref(),
                    &["-version"],
                    &[],
                    self.exec.timeouts.local,
                )
                .filter(crate::Output::success)
                .and_then(|o| parse::xcodebuild_version(&o.text()));
        }
        t.xcode = XcodeInfo {
            installed: t.xcode.version.is_some(),
            ..t.xcode
        };
        if let Some(xcrun) = self.xcrun() {
            if let Ok(out) = self.simctl_does(
                &xcrun,
                &["list", "-j", "runtimes"],
                self.exec.timeouts.local,
            ) {
                t.ios_runtimes = parse::simctl_runtimes(&out.text());
            }
            if let Ok(out) = self.simctl_does(
                &xcrun,
                &["list", "-j", "devicetypes"],
                self.exec.timeouts.local,
            ) {
                t.ios_devicetypes = parse::simctl_devicetypes(&out.text());
            }
        }
        if let Some(pod) = self.places.program("pod") {
            t.cocoapods = ToolInfo {
                installed: true,
                version: self
                    .probe(Some(&pod), &["--version"], &[], self.exec.timeouts.local)
                    .and_then(|o| parse::pod_version(&o.text())),
            };
        }
        let emulator = sdk.as_deref().and_then(resolve::emulator_of);
        t.android = AndroidSdk {
            path: sdk.as_ref().map(|p| p.display().to_string()),
            // The `adb` a listing would use: the SDK's, else the one found
            // by name — what the status says is what the devices are read with.
            adb: self
                .adb_program(sdk.as_deref())
                .map(|p| p.display().to_string()),
            emulator: emulator.as_ref().map(|p| p.display().to_string()),
            avds: self
                .probe(
                    emulator.as_deref(),
                    &["-list-avds"],
                    &env,
                    self.exec.timeouts.local,
                )
                .map(|o| parse::avd_list(&o.text()))
                .unwrap_or_default(),
        };
        let java = self
            .places
            .program("java")
            .or_else(|| self.places.fixed(STUDIO_JAVA));
        if let Some(java) = java {
            t.java = ToolInfo {
                installed: true,
                version: self
                    .probe(Some(&java), &["-version"], &[], self.exec.timeouts.local)
                    .and_then(|o| parse::java_version(&o.stderr)),
            };
        }
        t
    }

    fn devices(&self, hints: &Hints) -> MobileDevelopmentResult<Vec<Device>> {
        Ok(self.listing(hints, None).devices)
    }

    fn listing(&self, hints: &Hints, only: Option<Platform>) -> Listing {
        let mut listing = Listing::default();
        if only != Some(Platform::Android) {
            if let Some(xcrun) = self.xcrun() {
                match self.simulators(&xcrun) {
                    Ok(simulators) => listing.devices.extend(simulators),
                    Err(error) => listing.problems.push(ListingProblem {
                        platform: Platform::Ios,
                        error,
                    }),
                }
            }
        }
        if only != Some(Platform::Ios) {
            let (devices, problems) = self.android(self.sdk(hints).as_deref());
            listing.devices.extend(devices);
            listing
                .problems
                .extend(problems.into_iter().map(|error| ListingProblem {
                    platform: Platform::Android,
                    error,
                }));
        }
        crate::sort_for_a_picker(&mut listing.devices);
        listing
    }

    fn boot(&self, hints: &Hints, device: &Device) -> MobileDevelopmentResult<Device> {
        match (device.platform, device.kind) {
            (Platform::Ios, DeviceKind::Simulator) => {
                if device.state.is_up() {
                    return Ok(device.clone());
                }
                let xcrun = self.need_xcrun()?;
                // One already on its way up is waited for, not asked twice.
                if device.state != DeviceState::Booting {
                    let out =
                        self.simctl(&xcrun, &["boot", &device.id], self.exec.timeouts.local)?;
                    if !out.success() && out.code != ALREADY_BOOTED {
                        return Err(failed(format!("simctl boot {}", device.id), &out));
                    }
                }
                self.simctl_does(
                    &xcrun,
                    &["bootstatus", &device.id, "-b"],
                    self.exec.timeouts.boot,
                )?;
                Ok(Device {
                    state: DeviceState::Booted,
                    ..device.clone()
                })
            }
            (Platform::Android, DeviceKind::Emulator) => {
                if device.state.is_up() {
                    return Ok(device.clone());
                }
                // Listed by `adb` and not answering: it is running already,
                // and its id is a serial, not an image to start.
                if device.state != DeviceState::Shutdown {
                    return Err(MobileDevelopmentError::Unsupported(format!(
                        "{} is {}: it is started already and not answering yet",
                        device.name,
                        device.state.words()
                    )));
                }
                let sdk = self.sdk(hints);
                let emulator = sdk
                    .as_deref()
                    .and_then(resolve::emulator_of)
                    .ok_or_else(|| {
                        MobileDevelopmentError::NotInstalled("the Android emulator".into())
                    })?;
                // Without `adb` nothing would ever say the device is up.
                let adb = self
                    .adb_program(sdk.as_deref())
                    .ok_or_else(|| MobileDevelopmentError::NotInstalled("adb".into()))?;
                let env = Self::env_for(sdk.as_deref());
                let image = device.image.clone().unwrap_or_else(|| device.id.clone());
                let mut window =
                    self.exec
                        .spawn_detached(&emulator, &args(&["-avd", &image]), &env)?;
                self.await_emulator(&image, &mut window, &adb, &env)
            }
            _ => Err(MobileDevelopmentError::Unsupported(format!(
                "{} is a {}: it is not booted from here",
                device.name,
                device.kind.words()
            ))),
        }
    }

    fn shutdown(&self, hints: &Hints, device: &Device) -> MobileDevelopmentResult<Device> {
        match (device.platform, device.kind) {
            (Platform::Ios, DeviceKind::Simulator) => {
                let xcrun = self.need_xcrun()?;
                let out =
                    self.simctl(&xcrun, &["shutdown", &device.id], self.exec.timeouts.local)?;
                if !out.success() && !out.stderr.contains("current state: Shutdown") {
                    return Err(failed(format!("simctl shutdown {}", device.id), &out));
                }
                Ok(Device {
                    state: DeviceState::Shutdown,
                    ..device.clone()
                })
            }
            // Nobody started this image: it is down already.
            (Platform::Android, DeviceKind::Emulator) if device.state == DeviceState::Shutdown => {
                Ok(device.clone())
            }
            // Running, or there and not answering: the emulator's console
            // ends it either way.
            (Platform::Android, DeviceKind::Emulator) => {
                let (adb, env) = self.adb(hints)?;
                let out = self.local(&adb, &["-s", &device.id, "emu", "kill"], &env)?;
                if !out.success() {
                    return Err(failed(format!("adb -s {} emu kill", device.id), &out));
                }
                self.await_gone(&device.id, &adb, &env)?;
                Ok(match &device.image {
                    // Down, it is its image again, under the image's name.
                    Some(image) => Device {
                        id: image.clone(),
                        name: image.replace('_', " "),
                        state: DeviceState::Shutdown,
                        ..device.clone()
                    },
                    None => Device {
                        state: DeviceState::Shutdown,
                        ..device.clone()
                    },
                })
            }
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
                // macOS's own `open`, at its fixed place; a sealed test's
                // stand-in by name.
                let open = self
                    .places
                    .fixed(OPEN)
                    .or_else(|| {
                        self.places
                            .program("open")
                            .filter(|_| self.places.is_sealed())
                    })
                    .ok_or_else(|| {
                        MobileDevelopmentError::Unsupported(
                            "the Simulator's window is brought forward on a Mac alone".into(),
                        )
                    })?;
                // `open` answers at once, and says when there is no Simulator.
                let out = self.local(&open, &["-a", "Simulator"], &[])?;
                if !out.success() {
                    return Err(failed("open -a Simulator", &out));
                }
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
        let xcrun = self.need_xcrun()?;
        let out = self.simctl(
            &xcrun,
            &["create", name, devicetype, runtime],
            self.exec.timeouts.local,
        )?;
        if !out.success() {
            return Err(failed(format!("simctl create {name}"), &out));
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
        // Refused in words before any tool is asked: a device that is not
        // up has nothing on its screen.
        if !device.state.is_up() {
            return Err(MobileDevelopmentError::Unsupported(format!(
                "{} is {}: nothing is on its screen",
                device.name,
                device.state.words()
            )));
        }
        match (device.platform, device.kind) {
            (Platform::Ios, DeviceKind::Simulator) => {
                let xcrun = self.need_xcrun()?;
                // A file this call makes and removes: `simctl` writes to a
                // path, never to a pipe.
                let file = tempfile::Builder::new()
                    .prefix("bisa-shot-")
                    .suffix(&format!(".{}", format.as_str()))
                    .tempfile()
                    // LCOV_EXCL_START no temporary file to be had; the disk's arm
                    .map_err(|e| MobileDevelopmentError::Failed {
                        what: "screenshot".into(),
                        detail: format!("temp file: {e}"),
                    })?;
                // LCOV_EXCL_STOP
                let path = file.path().display().to_string();
                let kind = format!("--type={}", format.as_str());
                let out = self.simctl(
                    &xcrun,
                    &["io", &device.id, "screenshot", &kind, &path],
                    self.exec.timeouts.local,
                )?;
                if !out.success() {
                    return Err(failed(format!("simctl io {} screenshot", device.id), &out));
                }
                // LCOV_EXCL_START the file this call made a moment ago unreadable; the disk's arm
                std::fs::read(file.path()).map_err(|e| MobileDevelopmentError::Failed {
                    what: "screenshot".into(),
                    detail: format!("reading the capture: {e}"),
                })
                // LCOV_EXCL_STOP
            }
            (Platform::Android, _) => {
                let (adb, env) = self.adb(hints)?;
                let what = format!("adb -s {} exec-out screencap", device.id);
                let out = self.local(
                    &adb,
                    &["-s", &device.id, "exec-out", "screencap", "-p"],
                    &env,
                )?;
                if !out.success() || out.stdout.is_empty() {
                    return Err(MobileDevelopmentError::Failed {
                        what,
                        detail: out.stderr.trim().to_string(),
                    });
                }
                if out.clipped {
                    return Err(MobileDevelopmentError::Failed {
                        what,
                        detail: format!(
                            "the capture is larger than {} bytes",
                            self.exec.limits.stdout
                        ),
                    });
                }
                Ok(out.stdout)
            }
            _ => Err(MobileDevelopmentError::Unsupported(format!(
                "{} is a {}: its screen is not captured from here",
                device.name,
                device.kind.words()
            ))),
        }
    }
}

/// The real tools against scripted stand-ins of each program, in sealed
/// places: every script says what the vendor's tool says and records what it
/// was asked. Nothing of this machine's is run or read.
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::exec::{Limits, Timeouts};
    use std::os::unix::fs::PermissionsExt;

    const DEVELOPER_DIR: &str = "printf '/Applications/Xcode.app/Contents/Developer\\n'";

    struct Stage {
        dir: tempfile::TempDir,
        real: Real,
    }

    impl Stage {
        fn new() -> Self {
            Self::with(Timeouts {
                local: Duration::from_secs(10),
                doctor: Duration::from_secs(10),
                boot: Duration::from_secs(10),
                shutdown: Duration::from_secs(10),
                poll: Duration::from_millis(40),
                settle: Duration::from_millis(300),
            })
        }

        fn with(timeouts: Timeouts) -> Self {
            let dir = tempfile::tempdir().unwrap();
            for folder in ["bin", "home", "sdk/platform-tools", "sdk/emulator"] {
                std::fs::create_dir_all(dir.path().join(folder)).unwrap();
            }
            let real = Real {
                exec: Exec {
                    timeouts,
                    limits: Limits::default(),
                },
                places: resolve::Places::sealed(
                    dir.path().join("home"),
                    vec![dir.path().join("bin")],
                ),
            };
            Self { dir, real }
        }

        fn path(&self, rel: &str) -> PathBuf {
            self.dir.path().join(rel)
        }

        /// A stand-in program: it records `name args…`, then does `body`.
        /// `$DIR` is the stage's folder.
        fn script(&self, rel: &str, name: &str, body: &str) {
            let path = self.path(rel);
            let text = format!(
                "#!/bin/sh\nDIR='{dir}'\nprintf '%s\\n' \"{name} $*\" >> \"$DIR/calls.log\"\n{body}\n",
                dir = self.dir.path().display(),
            );
            std::fs::write(&path, text).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        fn bin(&self, name: &str, body: &str) {
            self.script(&format!("bin/{name}"), name, body);
        }

        fn adb(&self, body: &str) {
            self.script("sdk/platform-tools/adb", "adb", body);
        }

        fn emulator(&self, body: &str) {
            self.script("sdk/emulator/emulator", "emulator", body);
        }

        /// A Mac with a developer directory selected and this `xcrun`.
        fn xcode(&self, xcrun: &str) {
            self.bin("xcode-select", DEVELOPER_DIR);
            self.bin("xcrun", xcrun);
        }

        fn hints(&self) -> Hints {
            Hints {
                flutter_path: None,
                android_sdk: Some(self.path("sdk").display().to_string()),
            }
        }

        /// Every call a stand-in recorded, in order, `xcode-select` aside.
        fn calls(&self) -> Vec<String> {
            std::fs::read_to_string(self.path("calls.log"))
                .unwrap_or_default()
                .lines()
                .filter(|l| !l.starts_with("xcode-select"))
                .map(str::to_string)
                .collect()
        }

        fn forget(&self) {
            std::fs::remove_file(self.path("calls.log")).ok();
        }
    }

    fn sim(id: &str, state: DeviceState) -> Device {
        crate::fake::simulator(id, "iPhone 16", state)
    }

    /// An emulator image nobody started, as a listing answers one.
    fn image(name: &str) -> Device {
        Device {
            id: name.into(),
            name: name.replace('_', " "),
            platform: Platform::Android,
            kind: DeviceKind::Emulator,
            state: DeviceState::Shutdown,
            os: None,
            image: Some(name.into()),
        }
    }

    /// An emulator `adb` lists under a serial.
    fn serial(id: &str, image: Option<&str>, state: DeviceState) -> Device {
        Device {
            id: id.into(),
            name: image.map_or("sdk gphone64".into(), |i| i.replace('_', " ")),
            platform: Platform::Android,
            kind: DeviceKind::Emulator,
            state,
            os: None,
            image: image.map(str::to_string),
        }
    }

    /// `adb` as it answers while the emulator the stage started runs: the
    /// marker file `started` is the emulator's life.
    const ADB_FOLLOWS_THE_EMULATOR: &str = r#"case "$*" in
  "devices -l")
    # Busy for one question whenever the emulator has just changed.
    if [ -f "$DIR/busy" ]; then rm -f "$DIR/busy"; echo "adb: daemon busy" >&2; exit 1; fi
    echo "List of devices attached"
    [ -f "$DIR/started" ] && echo "emulator-5554          device product:sdk_gphone64 model:sdk_gphone64 transport_id:1"
    exit 0 ;;
  "-s emulator-5554 emu avd name") printf 'Pixel_8\nOK\n' ;;
  "-s emulator-5554 emu kill") : > "$DIR/busy"; rm -f "$DIR/started"; echo "OK: killing emulator, bye bye" ;;
  *) exit 64 ;;
esac"#;

    const EMULATOR_STARTS: &str = r#"case "$*" in
  "-list-avds") printf 'INFO    | Storing crashdata in: /tmp/x\nPixel_8\nMedium_Phone\n' ;;
  "-avd Pixel_8") : > "$DIR/busy"; : > "$DIR/started"; exec /bin/sleep 2 ;;
  *) exit 64 ;;
esac"#;

    #[test]
    fn the_machines_own_places_are_not_sealed() {
        let real = Real::default();
        assert!(!real.places.is_sealed());
        assert_eq!(real.exec.timeouts, Timeouts::default());
    }

    /// The defect this pass began with: a simulator that is shut down is
    /// the one a boot is for. It is booted and waited for; one that is up
    /// is answered as it stands, with nothing asked; one on its way up is
    /// waited for and not asked twice.
    #[test]
    fn a_shut_down_simulator_is_booted_and_waited_for() {
        let stage = Stage::new();
        stage.xcode("exit 0");
        let hints = stage.hints();
        let up = stage
            .real
            .boot(&hints, &sim("AAAA-1", DeviceState::Shutdown));
        assert_eq!(
            up.as_ref().map(|d| (d.id.as_str(), d.state)),
            Ok(("AAAA-1", DeviceState::Booted)),
            "{up:?}"
        );
        assert_eq!(
            stage.calls(),
            [
                "xcrun simctl boot AAAA-1",
                "xcrun simctl bootstatus AAAA-1 -b"
            ]
        );
        stage.forget();
        let already = stage
            .real
            .boot(&hints, &sim("AAAA-1", DeviceState::Booted))
            .unwrap();
        assert_eq!(already.state, DeviceState::Booted);
        assert!(
            stage.calls().is_empty(),
            "nothing asked: {:?}",
            stage.calls()
        );
        let rising = stage
            .real
            .boot(&hints, &sim("AAAA-1", DeviceState::Booting))
            .unwrap();
        assert_eq!(rising.state, DeviceState::Booted);
        assert_eq!(stage.calls(), ["xcrun simctl bootstatus AAAA-1 -b"]);
    }

    /// `simctl boot` answering that the simulator is already booted is no
    /// failure; any other refusal is one, in its words; and a boot that
    /// never completes is a failure too.
    #[test]
    fn a_boot_simctl_refuses_is_a_failure_in_its_words() {
        let stage = Stage::new();
        stage.xcode(
            r#"case "$2" in boot) echo "Unable to boot device in current state: Booted" >&2; exit 149 ;; esac"#,
        );
        let hints = stage.hints();
        assert_eq!(
            stage
                .real
                .boot(&hints, &sim("A", DeviceState::Shutdown))
                .map(|d| d.state),
            Ok(DeviceState::Booted)
        );
        stage.xcode(r#"case "$2" in boot) echo "Invalid device: A" >&2; exit 148 ;; esac"#);
        assert_eq!(
            stage.real.boot(&hints, &sim("A", DeviceState::Shutdown)),
            Err(MobileDevelopmentError::Failed {
                what: "simctl boot A".into(),
                detail: "Invalid device: A".into()
            })
        );
        stage.xcode(
            r#"case "$2" in bootstatus) echo "the device did not finish" >&2; exit 1 ;; esac"#,
        );
        assert_eq!(
            stage.real.boot(&hints, &sim("A", DeviceState::Shutdown)),
            Err(MobileDevelopmentError::Failed {
                what: "simctl bootstatus A -b".into(),
                detail: "the device did not finish".into()
            })
        );
        // No developer directory: `xcrun` is never called at all.
        stage.forget();
        stage.bin(
            "xcode-select",
            "echo 'xcode-select: error: unable to get active developer directory' >&2; exit 2",
        );
        assert_eq!(
            stage.real.boot(&hints, &sim("A", DeviceState::Shutdown)),
            Err(MobileDevelopmentError::NotInstalled("xcrun".into()))
        );
        assert!(stage.calls().is_empty(), "{:?}", stage.calls());
    }

    /// An emulator's image is started and answered as the device it became:
    /// under its `adb` serial, still knowing its image. The listing then
    /// shows the one device, not the image beside it.
    #[test]
    fn an_emulator_boots_into_the_device_adb_lists_and_keeps_its_image() {
        let stage = Stage::new();
        stage.adb(ADB_FOLLOWS_THE_EMULATOR);
        stage.emulator(EMULATOR_STARTS);
        let hints = stage.hints();
        let before = stage.real.listing(&hints, Some(Platform::Android));
        assert!(before.problems.is_empty(), "{:?}", before.problems);
        assert_eq!(
            before
                .devices
                .iter()
                .map(|d| (d.id.as_str(), d.state, d.image.as_deref()))
                .collect::<Vec<_>>(),
            [
                ("Medium_Phone", DeviceState::Shutdown, Some("Medium_Phone")),
                ("Pixel_8", DeviceState::Shutdown, Some("Pixel_8")),
            ]
        );
        let up = stage.real.boot(&hints, &before.devices[1]).unwrap();
        assert_eq!(
            (
                up.id.as_str(),
                up.name.as_str(),
                up.state,
                up.image.as_deref()
            ),
            (
                "emulator-5554",
                "Pixel 8",
                DeviceState::Running,
                Some("Pixel_8")
            )
        );
        assert!(
            stage.calls().iter().any(|c| c == "emulator -avd Pixel_8"),
            "{:?}",
            stage.calls()
        );
        let after = stage.real.listing(&hints, Some(Platform::Android)).devices;
        assert_eq!(
            after
                .iter()
                .map(|d| (d.id.as_str(), d.state))
                .collect::<Vec<_>>(),
            [
                ("emulator-5554", DeviceState::Running),
                ("Medium_Phone", DeviceState::Shutdown),
            ],
            "the image that runs is listed once, as the device"
        );
        // Booting what is up already asks nothing and answers it.
        stage.forget();
        assert_eq!(stage.real.boot(&hints, &up), Ok(up.clone()));
        assert!(stage.calls().is_empty());
        // Shut down, it is its image again, once `adb` no longer lists it.
        let down = stage.real.shutdown(&hints, &up).unwrap();
        assert_eq!(
            (down.id.as_str(), down.name.as_str(), down.state),
            ("Pixel_8", "Pixel 8", DeviceState::Shutdown)
        );
        assert_eq!(
            stage.calls(),
            [
                "adb -s emulator-5554 emu kill",
                "adb devices -l",
                "adb devices -l"
            ],
            "a listing that fails while it goes is asked again"
        );
        // An image nobody started is down already: nothing is asked.
        stage.forget();
        assert_eq!(stage.real.shutdown(&hints, &down), Ok(down.clone()));
        assert!(stage.calls().is_empty());
    }

    /// An emulator that refuses to start is said so with its exit status,
    /// at once — not waited out for the whole boot budget.
    #[test]
    fn an_emulator_that_ends_before_its_device_is_up_is_a_failure_at_once() {
        let stage = Stage::new();
        stage.adb(ADB_FOLLOWS_THE_EMULATOR);
        stage.emulator(r#"case "$*" in "-avd Broken") exit 1 ;; esac"#);
        let began = Instant::now();
        let refused = stage.real.boot(&stage.hints(), &image("Broken"));
        assert!(
            matches!(refused, Err(MobileDevelopmentError::Failed { ref what, ref detail })
                if what == "emulator -avd Broken" && detail.contains("exit status 1")),
            "{refused:?}"
        );
        assert!(
            began.elapsed() < Duration::from_secs(5),
            "{:?}",
            began.elapsed()
        );
    }

    /// A launcher that ends well and a device that never appears is a
    /// timeout after the boot budget, said in whole seconds.
    #[test]
    fn an_emulator_whose_device_never_appears_times_out() {
        let stage = Stage::with(Timeouts {
            boot: Duration::from_millis(300),
            poll: Duration::from_millis(40),
            ..Timeouts::default()
        });
        stage.adb(ADB_FOLLOWS_THE_EMULATOR);
        stage.emulator("exit 0");
        assert_eq!(
            stage.real.boot(&stage.hints(), &image("Pixel_8")),
            Err(MobileDevelopmentError::Timeout {
                what: "emulator -avd Pixel_8".into(),
                secs: 1
            })
        );
    }

    /// What cannot be booted is refused before anything is started: an
    /// emulator that is there and not answering (its id is a serial, not an
    /// image), a phone, and an image with no emulator program or no `adb`
    /// to say when it is up.
    #[test]
    fn what_cannot_be_booted_is_refused_before_anything_is_started() {
        let stage = Stage::new();
        stage.adb(ADB_FOLLOWS_THE_EMULATOR);
        stage.emulator(EMULATOR_STARTS);
        let hints = stage.hints();
        let offline = stage.real.boot(
            &hints,
            &serial("emulator-5554", Some("Pixel_8"), DeviceState::Offline),
        );
        assert!(
            matches!(offline, Err(MobileDevelopmentError::Unsupported(ref why)) if why.contains("offline")),
            "{offline:?}"
        );
        let phone = crate::fake::phone("R58M", "SM G991B", DeviceState::Running);
        for refused in [
            stage.real.boot(&hints, &phone).map(|_| ()),
            stage.real.shutdown(&hints, &phone).map(|_| ()),
        ] {
            assert!(
                matches!(refused, Err(MobileDevelopmentError::Unsupported(ref why)) if why.contains("SM G991B is a device")),
                "{refused:?}"
            );
        }
        assert!(
            stage.calls().is_empty(),
            "nothing started: {:?}",
            stage.calls()
        );
        std::fs::remove_file(stage.path("sdk/platform-tools/adb")).unwrap();
        assert_eq!(
            stage.real.boot(&hints, &image("Pixel_8")),
            Err(MobileDevelopmentError::NotInstalled("adb".into()))
        );
        assert_eq!(
            stage
                .real
                .shutdown(&hints, &serial("emulator-5554", None, DeviceState::Running)),
            Err(MobileDevelopmentError::NotInstalled("adb".into()))
        );
        std::fs::remove_file(stage.path("sdk/emulator/emulator")).unwrap();
        assert_eq!(
            stage.real.boot(&hints, &image("Pixel_8")),
            Err(MobileDevelopmentError::NotInstalled(
                "the Android emulator".into()
            ))
        );
        assert!(stage.calls().is_empty(), "{:?}", stage.calls());
    }

    /// A simulator is shut down, one already down is no failure, and a
    /// refusal is `simctl`'s words. An emulator `adb` could not end is a
    /// failure, not a success; one that never goes is a timeout; one whose
    /// image nobody knows stays under its serial.
    #[test]
    fn a_shutdown_reads_the_tools_answer() {
        let stage = Stage::with(Timeouts {
            shutdown: Duration::from_millis(400),
            poll: Duration::from_millis(40),
            ..Timeouts::default()
        });
        let hints = stage.hints();
        stage.xcode("exit 0");
        assert_eq!(
            stage
                .real
                .shutdown(&hints, &sim("A", DeviceState::Booted))
                .map(|d| d.state),
            Ok(DeviceState::Shutdown)
        );
        assert_eq!(stage.calls(), ["xcrun simctl shutdown A"]);
        stage.xcode(r#"echo "Unable to shutdown device in current state: Shutdown" >&2; exit 149"#);
        assert_eq!(
            stage
                .real
                .shutdown(&hints, &sim("A", DeviceState::Shutdown))
                .map(|d| d.state),
            Ok(DeviceState::Shutdown)
        );
        stage.xcode(r#"echo "Invalid device: A" >&2; exit 148"#);
        assert_eq!(
            stage.real.shutdown(&hints, &sim("A", DeviceState::Booted)),
            Err(MobileDevelopmentError::Failed {
                what: "simctl shutdown A".into(),
                detail: "Invalid device: A".into()
            })
        );
        stage.adb(r#"case "$*" in *"emu kill") echo "error: device offline" >&2; exit 1 ;; esac"#);
        let running = serial("emulator-5554", Some("Pixel_8"), DeviceState::Running);
        assert_eq!(
            stage.real.shutdown(&hints, &running),
            Err(MobileDevelopmentError::Failed {
                what: "adb -s emulator-5554 emu kill".into(),
                detail: "error: device offline".into()
            })
        );
        // Told to end and still listed when the budget is spent.
        stage.adb(
            r#"case "$*" in "devices -l") printf 'List of devices attached\nemulator-5554 device\n' ;; esac"#,
        );
        assert_eq!(
            stage.real.shutdown(&hints, &running),
            Err(MobileDevelopmentError::Timeout {
                what: "adb -s emulator-5554 emu kill".into(),
                secs: 1
            })
        );
        // Gone, and nobody knew its image: it is answered under its serial.
        stage.adb(r#"case "$*" in "devices -l") echo "List of devices attached" ;; esac"#);
        let unnamed = serial("emulator-5554", None, DeviceState::Offline);
        assert_eq!(
            stage
                .real
                .shutdown(&hints, &unnamed)
                .map(|d| (d.id, d.state)),
            Ok(("emulator-5554".to_string(), DeviceState::Shutdown))
        );
    }

    const SIMULATORS: &str = r#"{"devices":{"com.apple.CoreSimulator.SimRuntime.iOS-18-2":[
  {"udid":"AAAA-1","state":"Shutdown","name":"iPhone 16"},
  {"udid":"AAAA-2","state":"Booted","name":"iPad Pro"}]}}"#;

    /// One platform's tool failing is said beside the other platform's
    /// devices — never read as "no devices" — and a platform that was not
    /// asked for is not asked at all.
    #[test]
    fn a_listing_says_what_it_could_not_ask_and_asks_only_what_it_was_asked() {
        let stage = Stage::new();
        let hints = stage.hints();
        stage.xcode(&format!(
            r#"case "$*" in "simctl list -j devices available") cat <<'JSON'
{SIMULATORS}
JSON
;; esac"#
        ));
        stage.adb(r#"echo "adb: failed to start daemon" >&2; exit 1"#);
        stage.emulator(r#"case "$*" in "-list-avds") echo Pixel_8 ;; esac"#);
        let both = stage.real.listing(&hints, None);
        assert_eq!(
            both.devices
                .iter()
                .map(|d| d.id.as_str())
                .collect::<Vec<_>>(),
            ["AAAA-2", "Pixel_8", "AAAA-1"],
            "up first, then Android before iOS, then by name"
        );
        assert_eq!(
            both.problems,
            [ListingProblem {
                platform: Platform::Android,
                error: MobileDevelopmentError::Failed {
                    what: "adb devices".into(),
                    detail: "adb: failed to start daemon".into()
                }
            }]
        );
        assert_eq!(
            stage.real.devices(&hints).map(|d| d.len()),
            Ok(3),
            "the plain list is the listing's devices"
        );
        stage.forget();
        let ios = stage.real.listing(&hints, Some(Platform::Ios));
        assert_eq!((ios.devices.len(), ios.problems.len()), (2, 0));
        assert!(
            stage.calls().iter().all(|c| c.starts_with("xcrun")),
            "adb is never started for iOS alone: {:?}",
            stage.calls()
        );
        stage.forget();
        // The other way round: `simctl` fails, the images still list.
        stage.xcode(r#"echo 'xcrun: error: unable to find utility "simctl"' >&2; exit 72"#);
        stage.adb(r#"case "$*" in "devices -l") echo "List of devices attached" ;; esac"#);
        stage.emulator(r#"echo "PANIC: Cannot find AVD system path" >&2; exit 1"#);
        let broken = stage.real.listing(&hints, None);
        assert!(broken.devices.is_empty());
        assert_eq!(
            broken
                .problems
                .iter()
                .map(|p| (p.platform, p.error.to_string()))
                .collect::<Vec<_>>(),
            [
                (
                    Platform::Ios,
                    "simctl list -j devices available: xcrun: error: unable to find utility \"simctl\""
                        .to_string()
                ),
                (
                    Platform::Android,
                    "emulator -list-avds: PANIC: Cannot find AVD system path".to_string()
                ),
            ]
        );
        stage.forget();
        let android = stage.real.listing(&hints, Some(Platform::Android));
        assert_eq!(android.problems.len(), 1);
        assert!(
            stage.calls().iter().all(|c| !c.starts_with("xcrun")),
            "{:?}",
            stage.calls()
        );
        // A program that is there and cannot be run is a problem too.
        std::fs::set_permissions(
            stage.path("sdk/emulator/emulator"),
            std::fs::Permissions::from_mode(0o644),
        )
        .unwrap();
        let unrunnable = stage.real.listing(&hints, Some(Platform::Android));
        assert!(
            matches!(&unrunnable.problems[..], [ListingProblem { error: MobileDevelopmentError::Failed { detail, .. }, .. }] if detail.starts_with("spawn: ")),
            "{:?}",
            unrunnable.problems
        );
    }

    /// A running emulator whose console does not answer keeps the name
    /// `adb` gave it and has no image; a phone and an offline one are
    /// listed as they are. With no developer directory, iOS is simply empty.
    #[test]
    fn what_adb_lists_is_named_after_its_image_when_the_console_answers() {
        let stage = Stage::new();
        stage.bin("xcode-select", "exit 2");
        stage.bin("xcrun", "exit 1");
        stage.adb(
            r#"case "$*" in
  "devices -l") printf 'List of devices attached\nemulator-5554 device model:sdk_gphone64\nemulator-5556 offline\nR58M device model:SM_G991B\n' ;;
  "-s emulator-5554 emu avd name") printf 'Pixel_8\nOK\n' ;;
  "-s emulator-5556 emu avd name") echo "error: could not connect to TCP port 5557" >&2; exit 1 ;;
esac"#,
        );
        let listing = stage.real.listing(&stage.hints(), None);
        assert!(listing.problems.is_empty(), "{:?}", listing.problems);
        assert_eq!(
            listing
                .devices
                .iter()
                .map(|d| (d.id.as_str(), d.name.as_str(), d.state, d.image.as_deref()))
                .collect::<Vec<_>>(),
            [
                (
                    "emulator-5554",
                    "Pixel 8",
                    DeviceState::Running,
                    Some("Pixel_8")
                ),
                ("R58M", "SM G991B", DeviceState::Running, None),
                ("emulator-5556", "emulator-5556", DeviceState::Offline, None),
            ]
        );
        assert!(
            stage.calls().iter().all(|c| !c.starts_with("xcrun")),
            "no developer directory, no xcrun: {:?}",
            stage.calls()
        );
    }

    /// A capture is the device's screen as bytes: `simctl` writes a file
    /// this call reads and removes, `adb` writes to its pipe. A device that
    /// is not up is refused in words before any tool is asked.
    #[test]
    fn a_screen_is_captured_from_a_device_that_is_up_and_refused_from_one_that_is_not() {
        let stage = Stage::new();
        let hints = stage.hints();
        stage.xcode(r#"for last; do :; done; printf 'PNG-BYTES' > "$last""#);
        let shot = stage
            .real
            .screenshot(&hints, &sim("A", DeviceState::Booted), ImageFormat::Jpeg)
            .unwrap();
        assert_eq!(shot, b"PNG-BYTES");
        let call = stage.calls().remove(0);
        assert!(
            call.starts_with("xcrun simctl io A screenshot --type=jpeg ")
                && call.ends_with(".jpeg"),
            "{call}"
        );
        let written = call.rsplit(' ').next().unwrap().to_string();
        assert!(!Path::new(&written).exists(), "the file is removed");
        stage.forget();
        for down in [DeviceState::Shutdown, DeviceState::Booting] {
            let refused = stage
                .real
                .screenshot(&hints, &sim("A", down), ImageFormat::Png);
            assert!(
                matches!(refused, Err(MobileDevelopmentError::Unsupported(ref why)) if why.contains("nothing is on its screen")),
                "{refused:?}"
            );
        }
        assert!(
            stage.calls().is_empty(),
            "nothing asked: {:?}",
            stage.calls()
        );
        stage.xcode(r#"echo "No devices are booted." >&2; exit 1"#);
        assert_eq!(
            stage
                .real
                .screenshot(&hints, &sim("A", DeviceState::Booted), ImageFormat::Png),
            Err(MobileDevelopmentError::Failed {
                what: "simctl io A screenshot".into(),
                detail: "No devices are booted.".into()
            })
        );
        // Android: the bytes on the pipe; nothing is a failure; an offline
        // phone has no screen to read.
        stage.adb(r#"case "$*" in "-s R58M exec-out screencap -p") printf 'ANDROID-PNG' ;; esac"#);
        let phone = crate::fake::phone("R58M", "SM G991B", DeviceState::Running);
        assert_eq!(
            stage.real.screenshot(&hints, &phone, ImageFormat::Jpeg),
            Ok(b"ANDROID-PNG".to_vec())
        );
        stage.adb(r#"echo "error: closed" >&2"#);
        assert_eq!(
            stage.real.screenshot(&hints, &phone, ImageFormat::Png),
            Err(MobileDevelopmentError::Failed {
                what: "adb -s R58M exec-out screencap".into(),
                detail: "error: closed".into()
            })
        );
        let asleep = crate::fake::phone("R58M", "SM G991B", DeviceState::Offline);
        assert!(matches!(
            stage.real.screenshot(&hints, &asleep, ImageFormat::Png),
            Err(MobileDevelopmentError::Unsupported(_))
        ));
        // A device of a kind no tool here reads.
        let iphone = Device {
            kind: DeviceKind::Physical,
            ..sim("PHONE", DeviceState::Booted)
        };
        let refused = stage.real.screenshot(&hints, &iphone, ImageFormat::Png);
        assert!(
            matches!(refused, Err(MobileDevelopmentError::Unsupported(ref why)) if why.contains("not captured from here")),
            "{refused:?}"
        );
    }

    /// A capture larger than the cap is a failure that says so, never a
    /// clipped picture handed on as whole.
    #[test]
    fn a_capture_past_the_cap_is_refused() {
        let mut stage = Stage::new();
        stage.real.exec.limits.stdout = 8;
        stage.adb("printf 'far-more-than-eight-bytes'");
        let phone = crate::fake::phone("R58M", "SM G991B", DeviceState::Running);
        let refused = stage
            .real
            .screenshot(&stage.hints(), &phone, ImageFormat::Png);
        assert!(
            matches!(refused, Err(MobileDevelopmentError::Failed { ref detail, .. }) if detail == "the capture is larger than 8 bytes"),
            "{refused:?}"
        );
    }

    /// The Simulator's window is brought forward by `open`, whose refusal
    /// is read: no Simulator is a failure, not a success. An emulator has
    /// its own window; off a Mac there is nothing to open.
    #[test]
    fn the_simulators_window_is_shown_and_opens_refusal_is_read() {
        let stage = Stage::new();
        let hints = stage.hints();
        let device = sim("A", DeviceState::Booted);
        assert!(
            matches!(stage.real.show(&hints, &device), Err(MobileDevelopmentError::Unsupported(ref why)) if why.contains("on a Mac alone")),
            "no `open`, no window"
        );
        stage.bin("open", "exit 0");
        assert_eq!(stage.real.show(&hints, &device), Ok(()));
        assert_eq!(stage.calls(), ["open -a Simulator"]);
        stage.bin(
            "open",
            r#"echo "Unable to find application named 'Simulator'" >&2; exit 1"#,
        );
        assert_eq!(
            stage.real.show(&hints, &device),
            Err(MobileDevelopmentError::Failed {
                what: "open -a Simulator".into(),
                detail: "Unable to find application named 'Simulator'".into()
            })
        );
        assert!(matches!(
            stage.real.show(&hints, &image("Pixel_8")),
            Err(MobileDevelopmentError::Unsupported(_))
        ));
    }

    /// Off a Mac's usual setup — an `xcrun` and no `xcode-select` to ask —
    /// `xcrun` is used as it is. With no `adb` at all, the images still
    /// list. And a tool that is there and cannot be run fails each call in
    /// the system's words.
    #[test]
    fn a_tool_that_cannot_be_asked_fails_its_call_and_no_other() {
        let stage = Stage::new();
        let hints = stage.hints();
        stage.bin("xcrun", "exit 0");
        assert_eq!(
            stage
                .real
                .boot(&hints, &sim("A", DeviceState::Shutdown))
                .map(|d| d.state),
            Ok(DeviceState::Booted),
            "no xcode-select to ask: xcrun as it is"
        );
        stage.emulator(r#"case "$*" in "-list-avds") echo Pixel_8 ;; esac"#);
        let listed = stage.real.listing(&hints, Some(Platform::Android));
        assert_eq!((listed.devices.len(), listed.problems.len()), (1, 0));
        let unrunnable = |rel: &str| {
            std::fs::write(stage.path(rel), "not a program").unwrap();
            std::fs::set_permissions(stage.path(rel), std::fs::Permissions::from_mode(0o644))
                .unwrap();
        };
        unrunnable("bin/xcrun");
        unrunnable("sdk/platform-tools/adb");
        let spawn_failed = |r: MobileDevelopmentResult<()>| {
            assert!(
                matches!(r, Err(MobileDevelopmentError::Failed { ref detail, .. }) if detail.starts_with("spawn: ")),
                "{r:?}"
            )
        };
        spawn_failed(
            stage
                .real
                .create_simulator(&hints, "T", "iPhone-16", "iOS-18-2")
                .map(|_| ()),
        );
        spawn_failed(
            stage
                .real
                .screenshot(&hints, &sim("A", DeviceState::Booted), ImageFormat::Png)
                .map(|_| ()),
        );
        let phone = crate::fake::phone("R58M", "SM G991B", DeviceState::Running);
        spawn_failed(
            stage
                .real
                .screenshot(&hints, &phone, ImageFormat::Png)
                .map(|_| ()),
        );
    }

    #[test]
    fn a_simulator_is_made_and_its_udid_answered() {
        let stage = Stage::new();
        let hints = stage.hints();
        stage.xcode("echo 7E2B-NEW");
        assert_eq!(
            stage
                .real
                .create_simulator(&hints, "Test phone", "iPhone-16", "iOS-18-2"),
            Ok("7E2B-NEW".to_string())
        );
        assert_eq!(
            stage.calls(),
            ["xcrun simctl create Test phone iPhone-16 iOS-18-2"]
        );
        stage.xcode(r#"echo "Invalid runtime: iOS-99" >&2; exit 162"#);
        assert_eq!(
            stage
                .real
                .create_simulator(&hints, "T", "iPhone-16", "iOS-99"),
            Err(MobileDevelopmentError::Failed {
                what: "simctl create T".into(),
                detail: "Invalid runtime: iOS-99".into()
            })
        );
        stage.xcode("exit 0");
        assert_eq!(
            stage
                .real
                .create_simulator(&hints, "T", "iPhone-16", "iOS-18-2"),
            Err(MobileDevelopmentError::Failed {
                what: "simctl create T".into(),
                detail: "no UDID answered".into()
            })
        );
    }

    /// The whole examination, every part on its own: Flutter where the
    /// setting says, told where the SDK is; Xcode behind its developer
    /// directory; the runtimes and device types; CocoaPods; the SDK's two
    /// programs and its images; Java.
    #[test]
    fn the_setup_is_examined_part_by_part() {
        let stage = Stage::new();
        std::fs::create_dir_all(stage.path("home/flutter/bin")).unwrap();
        stage.script(
            "home/flutter/bin/flutter",
            "flutter",
            r#"case "$*" in
  "--version --machine") printf '{"frameworkVersion":"3.24.3","channel":"stable","dartSdkVersion":"3.5.3 (build 3.5.3)"}' ;;
  "doctor") printf '[✓] Flutter (Channel stable, 3.24.3)\n[✗] Android toolchain - develop for Android devices\n'; printf '%s' "$ANDROID_HOME" > "$DIR/flutter-saw-sdk" ;;
esac"#,
        );
        stage.xcode(
            r#"case "$*" in
  "simctl list -j runtimes") printf '{"runtimes":[{"identifier":"com.apple.CoreSimulator.SimRuntime.iOS-18-2","name":"iOS 18.2","version":"18.2","platform":"iOS","isAvailable":true}]}' ;;
  "simctl list -j devicetypes") printf '{"devicetypes":[{"productFamily":"iPhone","identifier":"com.apple.CoreSimulator.SimDeviceType.iPhone-16","name":"iPhone 16"}]}' ;;
esac"#,
        );
        stage.bin(
            "xcodebuild",
            "printf 'Xcode 16.0\\nBuild version 16A242d\\n'",
        );
        stage.bin("pod", "echo 1.15.2");
        stage.bin("java", r#"echo 'openjdk version "17.0.10" 2024-01-16' >&2"#);
        stage.adb("exit 0");
        stage.emulator(r#"case "$*" in "-list-avds") printf 'Pixel_8\n' ;; esac"#);
        let hints = Hints {
            flutter_path: Some("~/flutter".into()),
            ..stage.hints()
        };
        let t = stage.real.toolchain(&hints);
        let flutter = stage.path("home/flutter/bin/flutter");
        assert_eq!(stage.real.flutter_path(&hints), Some(flutter.clone()));
        assert_eq!(
            t.flutter,
            FlutterInfo {
                installed: true,
                path: Some(flutter.display().to_string()),
                version: Some("3.24.3".into()),
                channel: Some("stable".into()),
                dart: Some("3.5.3".into()),
            }
        );
        assert_eq!(t.doctor.len(), 2);
        assert_eq!(
            std::fs::read_to_string(stage.path("flutter-saw-sdk")).unwrap(),
            stage.path("sdk").display().to_string(),
            "Flutter's doctor is told where the SDK is"
        );
        assert_eq!(
            t.xcode,
            XcodeInfo {
                installed: true,
                path: Some("/Applications/Xcode.app/Contents/Developer".into()),
                version: Some("16.0".into()),
            }
        );
        assert_eq!(t.ios_runtimes.len(), 1);
        assert_eq!(t.ios_devicetypes[0].name, "iPhone 16");
        assert_eq!(
            t.cocoapods,
            ToolInfo {
                installed: true,
                version: Some("1.15.2".into())
            }
        );
        assert_eq!(t.java.version.as_deref(), Some("17.0.10"));
        assert_eq!(
            t.android,
            AndroidSdk {
                path: Some(stage.path("sdk").display().to_string()),
                adb: Some(stage.path("sdk/platform-tools/adb").display().to_string()),
                emulator: Some(stage.path("sdk/emulator/emulator").display().to_string()),
                avds: vec!["Pixel_8".into()],
            }
        );
        assert!(t.unusable_hints.is_empty());
    }

    /// What is absent is absent on its own, and a setting that names
    /// nothing is said: it is not passed over for another place, and the
    /// examination names it. With no SDK folder, the `adb` found by name is
    /// the one the status shows — the one a listing would use.
    #[test]
    fn a_setting_that_names_nothing_is_said_and_what_is_absent_is_absent_alone() {
        let stage = Stage::with(Timeouts {
            doctor: Duration::from_millis(300),
            ..Timeouts::default()
        });
        stage.bin("adb", r#"case "$*" in "devices -l") printf 'List of devices attached\nR58M device model:SM_G991B\n' ;; esac"#);
        stage.bin("flutter", "exit 0");
        let wrong = Hints {
            flutter_path: Some("/nonexistent/flutter".into()),
            android_sdk: Some("~/no-such-sdk".into()),
        };
        let t = stage.real.toolchain(&wrong);
        assert_eq!(t.unusable_hints, [Hint::FlutterPath, Hint::AndroidSdk]);
        assert!(
            !t.flutter.installed,
            "the setting is not passed over for PATH"
        );
        assert_eq!(stage.real.flutter_path(&wrong), None);
        assert_eq!(t.xcode, XcodeInfo::default());
        assert!(t.ios_runtimes.is_empty() && !t.cocoapods.installed && !t.java.installed);
        assert_eq!(
            t.android,
            AndroidSdk {
                adb: Some(stage.path("bin/adb").display().to_string()),
                ..AndroidSdk::default()
            }
        );
        // The same `adb` lists the phone.
        let listed = stage.real.listing(&wrong, None);
        assert_eq!(listed.devices.len(), 1);
        assert_eq!(listed.devices[0].id, "R58M");
        // No setting: Flutter by name; a doctor past its budget is absent,
        // the version still read; a developer directory with a broken
        // `xcodebuild` is no Xcode.
        stage.bin(
            "flutter",
            r#"case "$*" in "doctor") exec /bin/sleep 5 ;; *) printf '{"flutterVersion":"3.27.0"}' ;; esac"#,
        );
        stage.bin("xcode-select", DEVELOPER_DIR);
        stage.bin(
            "xcodebuild",
            "echo 'xcodebuild: error: tool requires Xcode' >&2; exit 1",
        );
        stage.bin("xcrun", "exit 1");
        let t = stage.real.toolchain(&Hints::default());
        assert_eq!(t.flutter.version.as_deref(), Some("3.27.0"));
        assert!(t.doctor.is_empty(), "a doctor that timed out says nothing");
        assert!(!t.xcode.installed);
        assert_eq!(
            t.xcode.path.as_deref(),
            Some("/Applications/Xcode.app/Contents/Developer")
        );
        assert!(t.ios_runtimes.is_empty() && t.ios_devicetypes.is_empty());
        assert!(t.unusable_hints.is_empty());
    }
}
