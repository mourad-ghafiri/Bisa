//! What every agent is told about mobile development (ide/19), in one
//! place, the way [`crate::browser`] holds the browser's sentence: the note a
//! work item's first prompt and a conversation's framing carry when the
//! machine develops for mobile, and the one sentence the `bisa` MCP server's
//! instructions carry always, so the three never drift. The tools are the
//! MCP server's; the toolchain is the node's; the words are here because the
//! engine and the MCP server both depend on this crate and on nothing of
//! each other.

/// The sentence a session reads when mobile development is on here: the
/// mobile tools, how the app is run, what the guard asks before, and the one
/// licence to stop.
pub const MOBILE_DEVELOPMENT_NOTE: &str =
    "Mobile apps: this machine develops Flutter apps for phones, and the \
mobile tools are yours — mobile_development_status says what is installed (Flutter, Xcode, the Android SDK) \
and what Flutter's doctor found; mobile_development_devices lists the simulators, emulators and phones here \
with their state; mobile_development_boot boots a simulator or starts an emulator; mobile_development_screenshot captures \
a device's screen as a PNG and answers the file's path — read that file to see the app. Run the \
app yourself: `flutter run -d <device id>` in the checkout, in a terminal, on an id mobile_development_devices \
listed — never `-d chrome`, which the guard refuses; a Flutter web build goes through `-d \
web-server` and the browser tools. Hot reload is `r` and hot restart `R` on that terminal. The \
guard asks the person before a store submission (altool, notarytool, fastlane, a Gradle publish) \
and before every simulator is erased; never submit an app to a store unasked. When a mobile tool \
refuses you or answers that the mobile tools are not available, say so to the person and stop — \
never work around it.";

/// The sentence the MCP server's instructions carry whatever the machine
/// has: the tools exist; the workspace says whether they answer.
pub const MOBILE_DEVELOPMENT_HINT: &str = "Mobile apps: mobile_development_status, mobile_development_devices, mobile_development_boot and \
mobile_development_screenshot are the mobile tools — Flutter on simulators, emulators and phones — and they \
answer only where Settings › Capabilities › Mobile Development is on; run the app with `flutter run -d <id>` in \
a terminal, never on the machine's browser.";

/// The note as a prompt carries it: the sentence, then which platforms this
/// machine develops for, in the workspace's words (`iOS and Android`, `iOS
/// only`, `Android only`).
pub fn mobile_development_note(platforms: &str) -> String {
    format!("{MOBILE_DEVELOPMENT_NOTE} Platforms here: {platforms}.")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_note_names_every_tool_the_run_line_and_the_one_licence_to_stop() {
        for tool in [
            "mobile_development_status",
            "mobile_development_devices",
            "mobile_development_boot",
            "mobile_development_screenshot",
        ] {
            assert!(MOBILE_DEVELOPMENT_NOTE.contains(tool), "{tool}");
            assert!(MOBILE_DEVELOPMENT_HINT.contains(tool), "{tool}");
        }
        assert!(MOBILE_DEVELOPMENT_NOTE.contains("`flutter run -d <device id>`"));
        assert!(MOBILE_DEVELOPMENT_NOTE.contains("never `-d chrome`"));
        assert!(MOBILE_DEVELOPMENT_NOTE.contains("never submit an app to a store unasked"));
        assert!(MOBILE_DEVELOPMENT_NOTE.contains("never work around it"));
        assert!(
            !MOBILE_DEVELOPMENT_NOTE.contains("unless"),
            "no licence to run the app anywhere but a device"
        );
    }

    #[test]
    fn the_note_says_which_platforms_this_machine_develops_for() {
        let note = mobile_development_note("iOS and Android");
        assert!(note.starts_with(MOBILE_DEVELOPMENT_NOTE));
        assert!(note.ends_with("Platforms here: iOS and Android."));
        assert!(mobile_development_note("Android only").ends_with("Platforms here: Android only."));
    }
}
