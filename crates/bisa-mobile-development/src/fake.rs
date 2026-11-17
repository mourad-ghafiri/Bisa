//! A scripted [`MobileDevelopmentTools`] for tests: a toolchain and devices handed in,
//! a screenshot's bytes handed in, any call made to fail by name, every call
//! recorded. No program is spawned and nothing of the person's is read.
//!
//! It behaves as the real tools do where a caller could tell the difference:
//! an emulator that boots answers under an `adb` serial and goes back to its
//! image's name when it is shut down; a device that is not up has no screen
//! to capture; a phone is neither booted nor shut down from here; only iOS
//! has a window to bring forward; a listing is in a picker's order.

use std::collections::BTreeMap;
use std::sync::Mutex;

use crate::{
    Device, DeviceKind, DeviceState, Hints, ImageFormat, MobileDevelopmentError,
    MobileDevelopmentResult, MobileDevelopmentTools, Platform, Toolchain,
};

#[derive(Debug, Default)]
pub struct FakeMobileDevelopment {
    toolchain: Mutex<Toolchain>,
    devices: Mutex<Vec<Device>>,
    screenshot: Mutex<Vec<u8>>,
    failing: Mutex<BTreeMap<String, MobileDevelopmentError>>,
    calls: Mutex<Vec<String>>,
}

/// A PNG of the given size — the signature and an IHDR, which is all a
/// reader of dimensions or a sniffer of types looks at.
pub fn png_fixture(width: u32, height: u32) -> Vec<u8> {
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    png.extend_from_slice(&13u32.to_be_bytes());
    png.extend_from_slice(b"IHDR");
    png.extend_from_slice(&width.to_be_bytes());
    png.extend_from_slice(&height.to_be_bytes());
    png.extend_from_slice(&[8, 6, 0, 0, 0]);
    png.extend_from_slice(&[0, 0, 0, 0]);
    png
}

/// A simulator as a test scripts one.
pub fn simulator(id: &str, name: &str, state: DeviceState) -> Device {
    Device {
        id: id.into(),
        name: name.into(),
        platform: Platform::Ios,
        kind: DeviceKind::Simulator,
        state,
        os: Some("iOS 18.2".into()),
        image: None,
    }
}

/// An emulator as a test scripts one: its image is its name as `adb`
/// spells one (`Pixel 8` runs the image `Pixel_8`).
pub fn emulator(id: &str, name: &str, state: DeviceState) -> Device {
    Device {
        id: id.into(),
        name: name.into(),
        platform: Platform::Android,
        kind: DeviceKind::Emulator,
        state,
        os: None,
        image: Some(name.replace(' ', "_")),
    }
}

/// A phone plugged in, as a test scripts one.
pub fn phone(id: &str, name: &str, state: DeviceState) -> Device {
    Device {
        id: id.into(),
        name: name.into(),
        platform: Platform::Android,
        kind: DeviceKind::Physical,
        state,
        os: None,
        image: None,
    }
}

impl FakeMobileDevelopment {
    pub fn new() -> Self {
        Self {
            screenshot: Mutex::new(png_fixture(1170, 2532)),
            ..Self::default()
        }
    }

    pub fn with_toolchain(self, toolchain: Toolchain) -> Self {
        *self.toolchain.lock().unwrap_or_else(|e| e.into_inner()) = toolchain;
        self
    }

    pub fn with_devices(self, devices: Vec<Device>) -> Self {
        *self.devices.lock().unwrap_or_else(|e| e.into_inner()) = devices;
        self
    }

    pub fn with_screenshot(self, bytes: Vec<u8>) -> Self {
        *self.screenshot.lock().unwrap_or_else(|e| e.into_inner()) = bytes;
        self
    }

    /// Make one call — `devices`, `boot`, `shutdown`, `show`,
    /// `create_simulator`, `screenshot` — answer this error.
    pub fn failing(self, op: &str, error: MobileDevelopmentError) -> Self {
        self.failing
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(op.to_string(), error);
        self
    }

    /// Every call made, as `op` or `op <device id>`, in order.
    pub fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn note(&self, call: String) {
        self.calls
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(call);
    }

    fn fail_if(&self, op: &str) -> MobileDevelopmentResult<()> {
        match self
            .failing
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(op)
        {
            Some(e) => Err(e.clone()),
            None => Ok(()),
        }
    }

    /// Move one device and answer it as it now stands. An emulator changes
    /// its id with its state, as `adb` and the emulator's own listing name
    /// it: a serial while it runs, its image's name once it is down.
    fn moved(&self, id: &str, state: DeviceState) -> MobileDevelopmentResult<Device> {
        let mut devices = self.devices.lock().unwrap_or_else(|e| e.into_inner());
        let serial = format!(
            "emulator-{}",
            5554 + 2 * devices
                .iter()
                .filter(|d| d.id.starts_with("emulator-"))
                .count()
        );
        let d = devices
            .iter_mut()
            .find(|d| d.id == id)
            .ok_or_else(|| MobileDevelopmentError::NoSuchDevice(id.to_string()))?;
        if d.kind == DeviceKind::Emulator && d.state.is_up() != state.is_up() {
            d.id = match (&d.image, state.is_up()) {
                (_, true) => serial,
                (Some(image), false) => image.clone(),
                (None, false) => d.id.clone(),
            };
        }
        d.state = state;
        Ok(d.clone())
    }
}

impl MobileDevelopmentTools for FakeMobileDevelopment {
    fn flutter_path(&self, _hints: &Hints) -> Option<std::path::PathBuf> {
        self.note("flutter_path".into());
        self.toolchain
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .flutter
            .path
            .as_deref()
            .map(std::path::PathBuf::from)
    }

    fn toolchain(&self, _hints: &Hints) -> Toolchain {
        self.note("toolchain".into());
        self.toolchain
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    fn devices(&self, _hints: &Hints) -> MobileDevelopmentResult<Vec<Device>> {
        self.note("devices".into());
        self.fail_if("devices")?;
        let mut listed = self
            .devices
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        crate::sort_for_a_picker(&mut listed);
        Ok(listed)
    }

    fn boot(&self, _hints: &Hints, device: &Device) -> MobileDevelopmentResult<Device> {
        self.note(format!("boot {}", device.id));
        self.fail_if("boot")?;
        let up = match device.kind {
            DeviceKind::Simulator => DeviceState::Booted,
            DeviceKind::Emulator => DeviceState::Running,
            DeviceKind::Physical => {
                return Err(MobileDevelopmentError::Unsupported(format!(
                    "{} is a device: it is not booted from here",
                    device.name
                )))
            }
        };
        if device.kind == DeviceKind::Emulator && device.state == DeviceState::Offline {
            return Err(MobileDevelopmentError::Unsupported(format!(
                "{} is offline: it is started already and not answering yet",
                device.name
            )));
        }
        self.moved(&device.id, up)
    }

    fn shutdown(&self, _hints: &Hints, device: &Device) -> MobileDevelopmentResult<Device> {
        self.note(format!("shutdown {}", device.id));
        self.fail_if("shutdown")?;
        if device.kind == DeviceKind::Physical {
            return Err(MobileDevelopmentError::Unsupported(format!(
                "{} is a device: it is not shut down from here",
                device.name
            )));
        }
        self.moved(&device.id, DeviceState::Shutdown)
    }

    fn show(&self, _hints: &Hints, device: &Device) -> MobileDevelopmentResult<()> {
        self.note(format!("show {}", device.id));
        self.fail_if("show")?;
        match device.platform {
            Platform::Ios => Ok(()),
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
        self.note(format!("create_simulator {name} {devicetype} {runtime}"));
        self.fail_if("create_simulator")?;
        let mut devices = self.devices.lock().unwrap_or_else(|e| e.into_inner());
        let id = format!("SIM-{}", devices.len() + 1);
        devices.push(Device {
            id: id.clone(),
            name: name.to_string(),
            platform: Platform::Ios,
            kind: DeviceKind::Simulator,
            state: DeviceState::Shutdown,
            os: Some(
                runtime
                    .rsplit('.')
                    .next()
                    .unwrap_or(runtime)
                    .replace("iOS-", "iOS ")
                    .replace('-', "."),
            ),
            image: None,
        });
        Ok(id)
    }

    fn screenshot(
        &self,
        _hints: &Hints,
        device: &Device,
        format: ImageFormat,
    ) -> MobileDevelopmentResult<Vec<u8>> {
        self.note(format!("screenshot {} {}", device.id, format.as_str()));
        self.fail_if("screenshot")?;
        if !device.state.is_up() {
            return Err(MobileDevelopmentError::Unsupported(format!(
                "{} is {}: nothing is on its screen",
                device.name,
                device.state.words()
            )));
        }
        Ok(self
            .screenshot
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fake_records_every_call_and_moves_a_device_it_boots() {
        let fake = FakeMobileDevelopment::new().with_devices(vec![simulator(
            "A",
            "iPhone 16",
            DeviceState::Shutdown,
        )]);
        let hints = Hints::default();
        let a = fake.devices(&hints).unwrap().remove(0);
        assert!(
            matches!(
                fake.screenshot(&hints, &a, ImageFormat::Png),
                Err(MobileDevelopmentError::Unsupported(ref why)) if why.contains("shut down")
            ),
            "a device that is not up has no screen"
        );
        let a = fake.boot(&hints, &a).unwrap();
        assert_eq!((a.id.as_str(), a.state), ("A", DeviceState::Booted));
        assert_eq!(fake.devices(&hints).unwrap()[0].state, DeviceState::Booted);
        assert_eq!(
            fake.calls(),
            vec!["devices", "screenshot A png", "boot A", "devices"]
        );
        assert_eq!(
            fake.boot(&hints, &simulator("Z", "Ghost", DeviceState::Shutdown)),
            Err(MobileDevelopmentError::NoSuchDevice("Z".into()))
        );
        let failing = FakeMobileDevelopment::new().failing(
            "screenshot",
            MobileDevelopmentError::NotInstalled("xcrun".into()),
        );
        assert_eq!(
            failing.screenshot(&hints, &a, ImageFormat::Png),
            Err(MobileDevelopmentError::NotInstalled("xcrun".into()))
        );
        assert_eq!(crate::png_size(&png_fixture(3, 4)), Some((3, 4)));
    }

    #[test]
    fn an_emulator_boots_to_running_a_phone_is_not_booted_from_here_and_a_shutdown_lands() {
        let fake = FakeMobileDevelopment::new().with_devices(vec![
            emulator("Pixel_8", "Pixel 8", DeviceState::Shutdown),
            phone("serial-1", "Phone", DeviceState::Running),
            emulator("emulator-5554", "Pixel 7", DeviceState::Offline),
        ]);
        let hints = Hints::default();
        let listed = fake.devices(&hints).unwrap();
        assert_eq!(
            listed.iter().map(|d| d.id.as_str()).collect::<Vec<_>>(),
            ["serial-1", "emulator-5554", "Pixel_8"],
            "what is up first, then by name"
        );
        let (phone, offline, image) = (&listed[0], &listed[1], &listed[2]);
        // Up, an emulator is its serial; its image says which one it is.
        let up = fake.boot(&hints, image).unwrap();
        assert_eq!(
            (up.id.as_str(), up.state, up.image.as_deref()),
            ("emulator-5556", DeviceState::Running, Some("Pixel_8"))
        );
        assert!(fake
            .devices(&hints)
            .unwrap()
            .iter()
            .all(|d| d.id != "Pixel_8"));
        assert!(matches!(
            fake.boot(&hints, phone),
            Err(MobileDevelopmentError::Unsupported(ref why)) if why.contains("Phone")
        ));
        assert!(matches!(
            fake.boot(&hints, offline),
            Err(MobileDevelopmentError::Unsupported(ref why)) if why.contains("offline")
        ));
        assert!(matches!(
            fake.shutdown(&hints, phone),
            Err(MobileDevelopmentError::Unsupported(ref why)) if why.contains("not shut down")
        ));
        // Down, it is its image again.
        let down = fake.shutdown(&hints, &up).unwrap();
        assert_eq!(
            (down.id.as_str(), down.state),
            ("Pixel_8", DeviceState::Shutdown)
        );
        assert!(matches!(
            fake.show(&hints, &down),
            Err(MobileDevelopmentError::Unsupported(_))
        ));
        fake.show(&hints, &simulator("A", "iPhone 16", DeviceState::Booted))
            .unwrap();
        // An emulator nobody knows the image of keeps its serial when down.
        let unnamed = FakeMobileDevelopment::new().with_devices(vec![Device {
            image: None,
            ..emulator("emulator-5554", "sdk gphone64", DeviceState::Running)
        }]);
        let listed = unnamed.devices(&hints).unwrap();
        assert_eq!(
            unnamed
                .shutdown(&hints, &listed[0])
                .map(|d| (d.id, d.state)),
            Ok(("emulator-5554".to_string(), DeviceState::Shutdown))
        );
        assert_eq!(fake.calls().last().map(String::as_str), Some("show A"));
    }
}
