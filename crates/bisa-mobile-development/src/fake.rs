//! A scripted [`MobileDevelopmentTools`] for tests: a toolchain and devices handed in,
//! a screenshot's bytes handed in, any call made to fail by name, every call
//! recorded. No program is spawned and nothing of the person's is read.

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
    }
}

/// An emulator as a test scripts one.
pub fn emulator(id: &str, name: &str, state: DeviceState) -> Device {
    Device {
        id: id.into(),
        name: name.into(),
        platform: Platform::Android,
        kind: DeviceKind::Emulator,
        state,
        os: None,
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

    fn set_state(&self, id: &str, state: DeviceState) -> MobileDevelopmentResult<()> {
        let mut devices = self.devices.lock().unwrap_or_else(|e| e.into_inner());
        let d = devices
            .iter_mut()
            .find(|d| d.id == id)
            .ok_or_else(|| MobileDevelopmentError::NoSuchDevice(id.to_string()))?;
        d.state = state;
        Ok(())
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
        Ok(self
            .devices
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone())
    }

    fn boot(&self, _hints: &Hints, device: &Device) -> MobileDevelopmentResult<()> {
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
        self.set_state(&device.id, up)
    }

    fn shutdown(&self, _hints: &Hints, device: &Device) -> MobileDevelopmentResult<()> {
        self.note(format!("shutdown {}", device.id));
        self.fail_if("shutdown")?;
        self.set_state(&device.id, DeviceState::Shutdown)
    }

    fn show(&self, _hints: &Hints, device: &Device) -> MobileDevelopmentResult<()> {
        self.note(format!("show {}", device.id));
        self.fail_if("show")
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
        fake.boot(&hints, &a).unwrap();
        assert_eq!(fake.devices(&hints).unwrap()[0].state, DeviceState::Booted);
        assert_eq!(fake.calls(), vec!["devices", "boot A", "devices"]);
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
}
