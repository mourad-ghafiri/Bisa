//! What the operating system lets the desktop app do (ide/01): two grants a
//! person gives in macOS's System Settings — **Full Disk Access** and the
//! **Microphone** — read and asked for from the shell, never from the node. A
//! grant is a fact about this machine, the shell is the process macOS grants
//! it to, and the node never needs to know; Settings › Capabilities › System
//! draws what this module answers.
//!
//! Two rules. **Nothing here prompts unless asked**: reading a status never
//! raises a dialog (Full Disk Access is probed by opening a TCC-protected file
//! read-only and dropping the handle unread; the microphone by
//! `AVCaptureDevice.authorizationStatus`), and only `system_permission_request`
//! opens System Settings or lets macOS ask. **A prompt needs its usage
//! string**: a process that asks for the microphone without
//! `NSMicrophoneUsageDescription` is terminated by macOS, so the request
//! refuses — `unsupported`, with the reason — when `Info.plist` lacks it,
//! rather than ever calling `requestAccess`.
//!
//! This is the first `target_os` cfg in the crate: every other platform
//! answers `unsupported` with the reason, so `cargo check` passes everywhere
//! and the panel says *not macOS* instead of guessing.

use serde::{Deserialize, Serialize};

/// The grants the panel knows. `snake_case` on the wire and in the settings
/// keys (`system.full_disk_access`, `system.microphone`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionKind {
    FullDiskAccess,
    Microphone,
}

/// What macOS says, in four words.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionStatus {
    Granted,
    Denied,
    /// macOS has not been asked yet, or the probe could not tell.
    NotDetermined,
    /// Not this platform, or this build cannot ask — `detail` says which.
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PermissionReport {
    pub kind: PermissionKind,
    pub status: PermissionStatus,
    /// The reason behind `unsupported` or `not_determined`, for the panel to show.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl PermissionReport {
    fn new(kind: PermissionKind, status: PermissionStatus) -> Self {
        Self {
            kind,
            status,
            detail: None,
        }
    }

    fn unsupported(kind: PermissionKind, why: impl Into<String>) -> Self {
        Self {
            kind,
            status: PermissionStatus::Unsupported,
            detail: Some(why.into()),
        }
    }
}

impl PermissionKind {
    /// The System Settings pane that holds this grant, as `open` takes it.
    pub fn settings_url(self) -> &'static str {
        match self {
            PermissionKind::FullDiskAccess => {
                "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles"
            }
            PermissionKind::Microphone => {
                "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone"
            }
        }
    }
}

/// An io error from the TCC probe, read as a status: a refusal is the grant
/// missing; anything else says nothing about the grant.
fn probe_status(result: Result<(), std::io::ErrorKind>) -> (PermissionStatus, Option<String>) {
    match result {
        Ok(()) => (PermissionStatus::Granted, None),
        Err(std::io::ErrorKind::PermissionDenied) => (PermissionStatus::Denied, None),
        Err(other) => (
            PermissionStatus::NotDetermined,
            Some(format!("the probe could not tell: {other:?}")),
        ),
    }
}

/// Read a grant's status. Never prompts.
#[tauri::command]
pub async fn system_permission_status(kind: PermissionKind) -> Result<PermissionReport, String> {
    tauri::async_runtime::spawn_blocking(move || platform::status(kind))
        .await
        .map_err(|e| format!("permission status: {e}"))
}

/// Ask for a grant: the microphone through macOS's own prompt (once; a denial
/// afterwards opens System Settings), Full Disk Access by opening the pane a
/// person adds the app in — macOS has no prompt for it. Answers the status
/// once the person has answered or the pane is open.
#[tauri::command]
pub async fn system_permission_request(kind: PermissionKind) -> Result<PermissionReport, String> {
    tauri::async_runtime::spawn_blocking(move || platform::request(kind))
        .await
        .map_err(|e| format!("permission request: {e}"))?
}

#[cfg(target_os = "macos")]
mod platform {
    use super::{probe_status, PermissionKind, PermissionReport, PermissionStatus};
    use block2::RcBlock;
    use objc2::msg_send;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyClass, Bool};
    use objc2_foundation::{NSBundle, NSString};
    use std::path::PathBuf;
    use std::process::Command;
    use std::sync::mpsc;
    use std::time::Duration;

    // `AVCaptureDevice` is looked up at runtime; the framework has to be in the
    // process for the lookup to find it.
    #[link(name = "AVFoundation", kind = "framework")]
    extern "C" {}

    /// `AVMediaTypeAudio`'s value.
    const MEDIA_AUDIO: &str = "soun";
    /// How long a person gets to answer macOS's prompt before the request
    /// answers `not_determined` and the panel offers to check again.
    const PROMPT_BUDGET: Duration = Duration::from_secs(300);

    pub fn status(kind: PermissionKind) -> PermissionReport {
        match kind {
            PermissionKind::FullDiskAccess => full_disk_access(),
            PermissionKind::Microphone => microphone_status(),
        }
    }

    pub fn request(kind: PermissionKind) -> Result<PermissionReport, String> {
        match kind {
            // No prompt exists: the person adds the app in the pane.
            PermissionKind::FullDiskAccess => {
                open_settings(kind)?;
                Ok(full_disk_access())
            }
            PermissionKind::Microphone => {
                let now = microphone_status();
                match now.status {
                    PermissionStatus::Granted | PermissionStatus::Unsupported => Ok(now),
                    // macOS asks once; after a denial only the pane can change it.
                    PermissionStatus::Denied => {
                        open_settings(kind)?;
                        Ok(now)
                    }
                    PermissionStatus::NotDetermined => Ok(microphone_request()),
                }
            }
        }
    }

    /// TCC's own database is readable by exactly the processes that hold
    /// Full Disk Access: opening it read-only and dropping the handle unread
    /// is the probe, and it raises no prompt.
    fn full_disk_access() -> PermissionReport {
        let Some(home) = std::env::var_os("HOME") else {
            return PermissionReport::unsupported(
                PermissionKind::FullDiskAccess,
                "no HOME in the environment",
            );
        };
        let db = PathBuf::from(home).join("Library/Application Support/com.apple.TCC/TCC.db");
        let (status, detail) =
            probe_status(std::fs::File::open(&db).map(|_| ()).map_err(|e| e.kind()));
        PermissionReport {
            kind: PermissionKind::FullDiskAccess,
            status,
            detail,
        }
    }

    fn open_settings(kind: PermissionKind) -> Result<(), String> {
        let ok = Command::new("/usr/bin/open")
            .arg(kind.settings_url())
            .status()
            .map_err(|e| format!("could not open System Settings: {e}"))?;
        if ok.success() {
            Ok(())
        } else {
            Err(format!("System Settings did not open ({ok})"))
        }
    }

    fn capture_device() -> Option<&'static AnyClass> {
        AnyClass::get(c"AVCaptureDevice")
    }

    fn has_usage_string() -> bool {
        let key = NSString::from_str("NSMicrophoneUsageDescription");
        NSBundle::mainBundle()
            .objectForInfoDictionaryKey(&key)
            .is_some()
    }

    fn microphone_status() -> PermissionReport {
        let Some(cls) = capture_device() else {
            return PermissionReport::unsupported(
                PermissionKind::Microphone,
                "AVFoundation is not available to this process",
            );
        };
        let media: Retained<NSString> = NSString::from_str(MEDIA_AUDIO);
        // AVAuthorizationStatus: 0 notDetermined · 1 restricted · 2 denied · 3 authorized.
        let raw: isize = unsafe { msg_send![cls, authorizationStatusForMediaType: &*media] };
        let status = match raw {
            3 => PermissionStatus::Granted,
            1 | 2 => PermissionStatus::Denied,
            _ => PermissionStatus::NotDetermined,
        };
        PermissionReport::new(PermissionKind::Microphone, status)
    }

    fn microphone_request() -> PermissionReport {
        if !has_usage_string() {
            return PermissionReport::unsupported(
                PermissionKind::Microphone,
                "this build carries no microphone usage description, so macOS cannot be asked",
            );
        }
        let Some(cls) = capture_device() else {
            return PermissionReport::unsupported(
                PermissionKind::Microphone,
                "AVFoundation is not available to this process",
            );
        };
        let media: Retained<NSString> = NSString::from_str(MEDIA_AUDIO);
        let (tx, rx) = mpsc::channel::<bool>();
        let handler = RcBlock::new(move |granted: Bool| {
            if tx.send(granted.as_bool()).is_err() {
                tracing::debug!("the asker stopped waiting for the grant");
            }
        });
        let _: () = unsafe {
            msg_send![cls, requestAccessForMediaType: &*media, completionHandler: &*handler]
        };
        let status = match rx.recv_timeout(PROMPT_BUDGET) {
            Ok(true) => PermissionStatus::Granted,
            Ok(false) => PermissionStatus::Denied,
            Err(_) => PermissionStatus::NotDetermined,
        };
        PermissionReport::new(PermissionKind::Microphone, status)
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    use super::{PermissionKind, PermissionReport};

    const WHY: &str = "these grants are macOS's; this is not macOS";

    pub fn status(kind: PermissionKind) -> PermissionReport {
        PermissionReport::unsupported(kind, WHY)
    }

    pub fn request(kind: PermissionKind) -> Result<PermissionReport, String> {
        Ok(PermissionReport::unsupported(kind, WHY))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::ErrorKind;

    #[test]
    fn a_refused_probe_is_a_missing_grant_and_any_other_failure_says_nothing() {
        assert_eq!(probe_status(Ok(())), (PermissionStatus::Granted, None));
        assert_eq!(
            probe_status(Err(ErrorKind::PermissionDenied)),
            (PermissionStatus::Denied, None)
        );
        let (status, detail) = probe_status(Err(ErrorKind::NotFound));
        assert_eq!(status, PermissionStatus::NotDetermined);
        assert!(detail.as_deref().is_some_and(|d| d.contains("NotFound")));
    }

    #[test]
    fn each_grant_opens_its_own_pane_and_the_words_are_snake_case_on_the_wire() {
        assert!(PermissionKind::FullDiskAccess
            .settings_url()
            .ends_with("Privacy_AllFiles"));
        assert!(PermissionKind::Microphone
            .settings_url()
            .ends_with("Privacy_Microphone"));
        let report = PermissionReport::unsupported(PermissionKind::FullDiskAccess, "why");
        let json = serde_json::to_value(&report).unwrap();
        assert_eq!(json["kind"], "full_disk_access");
        assert_eq!(json["status"], "unsupported");
        assert_eq!(json["detail"], "why");
        let granted = serde_json::to_value(PermissionReport::new(
            PermissionKind::Microphone,
            PermissionStatus::Granted,
        ))
        .unwrap();
        assert_eq!(granted["status"], "granted");
        assert!(
            granted.get("detail").is_none(),
            "no detail when there is nothing to explain"
        );
        let kind: PermissionKind = serde_json::from_str("\"microphone\"").unwrap();
        assert_eq!(kind, PermissionKind::Microphone);
    }
}
