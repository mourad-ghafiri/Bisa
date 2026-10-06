//! One Bisa at a time: the hand-off a second launch makes to the first
//! (02 §I64).
//!
//! The app is single-instance: `tauri-plugin-single-instance`, registered
//! first in `main.rs`, has a second process of this app hand its arguments
//! and working directory to the running one and exit inside `build()` —
//! before any other plugin's setup, before `setup` starts the node, before
//! a log file is attached — so it leaves no orphan node and no run marker.
//! The first process hears it here.
//!
//! On macOS the OS already keeps a bundle to one process: a relaunch from
//! Finder or the Dock is `RunEvent::Reopen` (`main.rs`), and a `bisa://`
//! URL to the running app is `RunEvent::Opened` (the deep-link plugin's).
//! This is the door for what LaunchServices does not catch — `open -n`, the
//! binary run directly, a dev run beside the installed bundle. On Windows
//! and Linux every `bisa://join/…` link starts a new process with the link
//! as its one argument, so this is the only door: the plugin's `deep-link`
//! feature hands the arguments to the deep-link plugin first, which emits
//! `deep-link://new-url` for `App.tsx` to land on Settings › People, and
//! then calls [`hand_off`], which brings the window back.

use tauri::AppHandle;

/// The scheme an invitation link wears (`tauri.conf.json`, 14-collaboration).
const LINK_SCHEME: &str = "bisa://";

/// A second launch handed off: say so once, and bring the window back — on
/// the main thread, whichever thread the plugin calls from (a tokio task on
/// macOS, the hidden window's procedure on Windows, the bus's thread on
/// Linux). The link, when there was one, is already the deep-link plugin's;
/// what it carries — an invitation code — is not for the log, nor is the
/// other process's working directory.
pub fn hand_off(app: &AppHandle, argv: Vec<String>, _cwd: String) {
    tracing::info!(
        target: "bisa_desktop",
        args = argv.len(),
        link = link_in(&argv).is_some(),
        "a second launch handed off to this one"
    );
    let handle = app.clone();
    if let Err(e) = app.run_on_main_thread(move || crate::tray::show_window(&handle)) {
        tracing::warn!(
            target: "bisa_desktop",
            "the second launch's window did not reach the main thread: {e}"
        );
    }
}

/// The first `bisa://` argument, when a launch carried one. The first
/// element of `std::env::args` is the executable and is never a link.
fn link_in(args: &[String]) -> Option<&str> {
    args.iter()
        .skip(1)
        .map(String::as_str)
        .find(|a| a.starts_with(LINK_SCHEME))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_plain_second_launch_carries_no_link_and_a_join_link_is_found() {
        assert_eq!(
            link_in(&args(&[
                "/Applications/Bisa.app/Contents/MacOS/bisa-desktop"
            ])),
            None
        );
        assert_eq!(link_in(&[]), None, "no arguments at all");
        assert_eq!(
            link_in(&args(&["bisa-desktop", "bisa://join/ABCD-EFGH"])),
            Some("bisa://join/ABCD-EFGH")
        );
        assert_eq!(
            link_in(&args(&[
                "bisa-desktop",
                "--flag",
                "bisa://join/x",
                "bisa://join/y"
            ])),
            Some("bisa://join/x"),
            "the first link, past any other argument"
        );
    }

    #[test]
    fn the_executable_is_never_taken_for_a_link() {
        assert_eq!(
            link_in(&args(&["bisa://join/this-is-the-binary-s-name"])),
            None,
            "argv[0] is the program, whatever it is called"
        );
        assert_eq!(
            link_in(&args(&["bisa-desktop", "https://bisa.dev/join/x"])),
            None,
            "only the app's own scheme"
        );
    }
}
