//! Where the programs are: the workspace's word first (`mobile_development.flutter.path`,
//! `mobile_development.android.sdk`), then `PATH`, then the folders the official
//! installers use. Pure over injected probes, so a test hands in a temp
//! directory and a scripted `PATH` and never reads the developer's machine.

use std::path::{Path, PathBuf};

/// The folders under the home directory Flutter's installer and the common
/// version managers use, relative to it.
pub const FLUTTER_HOMES: &[&str] = &[
    "development/flutter/bin/flutter",
    "flutter/bin/flutter",
    "fvm/default/bin/flutter",
    ".pub-cache/bin/flutter",
];

/// Where Homebrew links `flutter` on Apple silicon and on Intel.
pub const FLUTTER_SYSTEM: &[&str] = &["/opt/homebrew/bin/flutter", "/usr/local/bin/flutter"];

/// The folder Android Studio installs the SDK in, relative to the home.
pub const ANDROID_HOME: &str = "Library/Android/sdk";

/// The two variables the Android tools read, in the order Google documents.
pub const ANDROID_ENV: &[&str] = &["ANDROID_HOME", "ANDROID_SDK_ROOT"];

/// Where the well-known folders are: the home directory `FLUTTER_HOMES`
/// are under, and the system places (Homebrew's) looked in last. The node
/// hands in the machine's ([`Places::machine`]); a test hands in a temp
/// directory and no system place, so it reads nothing of the developer's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Places {
    pub home: PathBuf,
    pub system: Vec<PathBuf>,
}

impl Places {
    /// This machine: the home directory and Homebrew's two folders.
    pub fn machine(home: PathBuf) -> Self {
        Self {
            home,
            system: FLUTTER_SYSTEM.iter().map(PathBuf::from).collect(),
        }
    }

    /// A home alone, with no system place — what a test or a sealed
    /// environment uses.
    pub fn under(home: impl Into<PathBuf>) -> Self {
        Self {
            home: home.into(),
            system: Vec::new(),
        }
    }

    /// Every place a `flutter` program is looked for, in order.
    fn flutter_candidates(&self) -> impl Iterator<Item = PathBuf> + '_ {
        FLUTTER_HOMES
            .iter()
            .map(|rel| self.home.join(rel))
            .chain(self.system.iter().cloned())
    }
}

/// The `flutter` program: the hint when it names a file, else the one on
/// `PATH`, else the first well-known place that holds one.
pub fn flutter_path(
    hint: Option<&str>,
    places: &Places,
    on_path: impl Fn(&str) -> Option<PathBuf>,
) -> Option<PathBuf> {
    if let Some(hint) = hint.map(str::trim).filter(|h| !h.is_empty()) {
        let hinted = PathBuf::from(hint);
        // A folder is taken as the Flutter home; a file as the program.
        let candidate = if hinted.is_dir() {
            hinted.join("bin").join("flutter")
        } else {
            hinted
        };
        return candidate.is_file().then_some(candidate);
    }
    if let Some(found) = on_path("flutter") {
        return Some(found);
    }
    places.flutter_candidates().find(|p| p.is_file())
}

/// The Android SDK folder: the hint, else the environment's word (read by
/// name, never the environment as a whole), else Android Studio's folder.
pub fn android_sdk(
    hint: Option<&str>,
    env: impl Fn(&str) -> Option<String>,
    home: &Path,
) -> Option<PathBuf> {
    if let Some(hint) = hint.map(str::trim).filter(|h| !h.is_empty()) {
        let hinted = PathBuf::from(hint);
        return hinted.is_dir().then_some(hinted);
    }
    for name in ANDROID_ENV {
        if let Some(value) = env(name)
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
        {
            let dir = PathBuf::from(value);
            if dir.is_dir() {
                return Some(dir);
            }
        }
    }
    let studio = home.join(ANDROID_HOME);
    studio.is_dir().then_some(studio)
}

/// `adb`, under the SDK's platform-tools.
pub fn adb_of(sdk: &Path) -> Option<PathBuf> {
    let p = sdk.join("platform-tools").join("adb");
    p.is_file().then_some(p)
}

/// The emulator program, under the SDK.
pub fn emulator_of(sdk: &Path) -> Option<PathBuf> {
    let p = sdk.join("emulator").join("emulator");
    p.is_file().then_some(p)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn touch(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"").unwrap();
    }

    #[test]
    fn the_setting_wins_over_path_and_the_homes() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let places = Places::under(&home);
        let hinted = dir.path().join("elsewhere").join("bin").join("flutter");
        touch(&hinted);
        touch(&home.join("flutter/bin/flutter"));
        let on_path = |_: &str| Some(PathBuf::from("/usr/bin/flutter-on-path"));
        assert_eq!(
            flutter_path(Some(hinted.to_str().unwrap()), &places, on_path),
            Some(hinted.clone())
        );
        // A folder as the hint is the Flutter home.
        let flutter_home = dir.path().join("elsewhere");
        assert_eq!(
            flutter_path(Some(flutter_home.to_str().unwrap()), &places, on_path),
            Some(hinted.clone())
        );
        // A hint that names nothing is no answer, not a fallthrough: the
        // person said where it is, and it is not there.
        assert_eq!(
            flutter_path(Some("/nonexistent/flutter"), &places, on_path),
            None
        );
        // No hint: PATH first.
        assert_eq!(
            flutter_path(None, &places, on_path),
            Some(PathBuf::from("/usr/bin/flutter-on-path"))
        );
        assert_eq!(
            flutter_path(Some("  "), &places, on_path),
            Some(PathBuf::from("/usr/bin/flutter-on-path"))
        );
    }

    #[test]
    fn a_missing_home_is_skipped_and_the_first_present_one_answers() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let places = Places::under(&home);
        let fvm = home.join("fvm/default/bin/flutter");
        touch(&fvm);
        assert_eq!(flutter_path(None, &places, |_| None), Some(fvm.clone()));
        touch(&home.join("development/flutter/bin/flutter"));
        assert_eq!(
            flutter_path(None, &places, |_| None),
            Some(home.join("development/flutter/bin/flutter"))
        );
        let empty = Places::under(dir.path().join("nobody"));
        assert_eq!(flutter_path(None, &empty, |_| None), None);
    }

    #[test]
    fn the_system_places_come_last_and_are_the_machines_only_when_asked_for() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let brew = dir.path().join("brew/bin/flutter");
        touch(&brew);
        let places = Places {
            home: home.clone(),
            system: vec![brew.clone()],
        };
        assert_eq!(flutter_path(None, &places, |_| None), Some(brew.clone()));
        touch(&home.join("flutter/bin/flutter"));
        assert_eq!(
            flutter_path(None, &places, |_| None),
            Some(home.join("flutter/bin/flutter"))
        );
        let machine = Places::machine(home.clone());
        assert_eq!(machine.home, home);
        assert_eq!(
            machine.system,
            FLUTTER_SYSTEM.iter().map(PathBuf::from).collect::<Vec<_>>()
        );
        assert!(Places::under(&home).system.is_empty());
    }

    #[test]
    fn the_android_sdk_is_the_setting_the_named_variables_or_android_studios_folder() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let studio = home.join(ANDROID_HOME);
        fs::create_dir_all(&studio).unwrap();
        let by_env = dir.path().join("sdk-by-env");
        fs::create_dir_all(&by_env).unwrap();
        let env =
            |name: &str| (name == "ANDROID_SDK_ROOT").then(|| by_env.to_str().unwrap().to_string());
        assert_eq!(android_sdk(None, env, &home), Some(by_env.clone()));
        assert_eq!(android_sdk(None, |_| None, &home), Some(studio.clone()));
        let hinted = dir.path().join("sdk-hinted");
        fs::create_dir_all(&hinted).unwrap();
        assert_eq!(
            android_sdk(Some(hinted.to_str().unwrap()), env, &home),
            Some(hinted.clone())
        );
        assert_eq!(android_sdk(Some("/nonexistent/sdk"), env, &home), None);
        // The programs under it.
        assert_eq!(adb_of(&hinted), None);
        touch(&hinted.join("platform-tools/adb"));
        touch(&hinted.join("emulator/emulator"));
        assert_eq!(adb_of(&hinted), Some(hinted.join("platform-tools/adb")));
        assert_eq!(emulator_of(&hinted), Some(hinted.join("emulator/emulator")));
    }
}
