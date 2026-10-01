# 19 — Mobile Development: Flutter on the devices beside the code

The Project IDE develops Flutter apps for iOS and Android the way it develops for the web
([18](18-browser-and-servers.md)): the app runs on a **device** this machine can reach — the iOS
Simulator, an Android emulator, a phone plugged in — and the device's screen is **mirrored beside
the code**, in a document of its own; the person marks a spot on that screen and sends the capture
to an agent; the agent lists the devices, boots one, runs the app in a terminal and reads the
screen back through tools of its own. **Settings › Capabilities › Mobile Development** is the door
to all of it — off by default: on, it says what is installed, how to install the rest with the
official instructions, which platforms this machine develops for, and lets the person boot, shut
down and make simulators. A `mobile-developer` agent knows Flutter, the devices and the two stores;
the Workflow Agent reads one fact about this machine and stays universal.

The rules that decide the shape are the IDE's own ([01](01-trust-boundary.md)): the webview never
names a program or a path — the node probes the toolchain, lists the devices, captures a screen
and composes the one `flutter run` line; the Tauri shell runs that line through the login shell in
the checkout, as it runs the project's run command; nothing reaches an agent that is not a chip or
a tool's answer ([09](09-agents-in-the-ide.md)); and a tool list is a menu, not a permission — who
may ask is checked in the engine. Nothing here crosses a node: a toolchain, a simulator and a device
are facts about one machine, so no GEP kind is added ([09 — GEP](../09-protocol-gep.md)) — a
capture is an attachment, and it rides a message's context as a chip.

---

## What is on this machine

`crates/bisa-mobile-development` is the leaf that knows the programs — `flutter`, `xcrun simctl`, `xcodebuild`,
`adb`, `emulator`, `pod`, `java` — behind one port, `MobileDevelopmentTools`, with a `Real` implementation
and a `FakeMobileDevelopment` every test hands in. `EngineConfig.mobile_development` holds it, **`None` by default** like
`ssh` and `cli`: an engine nobody configured spawns nothing and answers *the mobile tools are not
available*; the node's CLI hands in `Real::default()`. Every program is run by absolute path, argv
only, with a null stdin, a hardened environment and a budget after which the child is terminated;
a program that wants a window — `emulator -avd`, `open -a Simulator` — is started detached and
never waited on. `flutter run` is never spawned here: the app runs where a person watches it.

Where the programs are is resolved in one order (`resolve.rs`): the workspace's word
(`mobile_development.flutter.path`, `mobile_development.android.sdk`), then `PATH`, then the folders the official
installers use — `~/development/flutter`, `~/flutter`, `~/fvm/default`, Homebrew's `bin`; the
Android SDK from `ANDROID_HOME` or `ANDROID_SDK_ROOT`, read by name, else Android Studio's
`~/Library/Android/sdk`. Nothing else of the environment is read.

The **toolchain** (`Toolchain`) is one examination with every part probed on its own, so a missing
Xcode says nothing about the Android SDK: Flutter's version and channel (`flutter --version
--machine`), Flutter's own doctor lines (`flutter doctor`, the category lines read into a state
each), Xcode's developer directory and version, the iOS simulator runtimes and device types
(`simctl list -j`), CocoaPods, the Android SDK folder with `adb`, the emulator and its images
(`emulator -list-avds`), Java. The engine keeps the last examination under a ten-minute TTL
(`mobile_development::MobileDevelopmentState`), clears it on *Check again* and on any `mobile_development.*` write, and announces
`mobile_development_changed { what: toolchain }` when it runs one. The **devices** (`Device { id, name,
platform, kind, state, os }`) are the simulators `simctl` lists as available, what `adb devices -l`
sees — a running emulator named after its image, a phone by its model — and every emulator image
nobody started as a shut-down emulator, the ones that are up first. A boot is `simctl boot` and
`bootstatus -b`, or the emulator started detached and looked for until `adb` sees it; a screen is
`simctl io <udid> screenshot` into a file this call makes and removes, or `adb exec-out screencap
-p` — PNG for Android whatever was asked.

## The workspace's word

Five settings, all in the `mobile_development.*` group ([13](13-settings.md)): `mobile_development.enabled` (machine,
**off**) — the switch; `mobile_development.platforms` (machine, `both` · `ios` · `android`) — a device of a
platform that is off is not listed, not booted and not captured, and iOS is held off macOS with
the reason; `mobile_development.agents` (workspace or a project, `everyone` · `assigned` · `nobody`) — which
agents the mobile tools answer, *assigned* meaning the agents that carry the `flutter-development`
skill, the General Agent and the Workflow Agent always; and the two paths. The switch and the platforms are the machine's —
facts about this Mac that never sync.

`mobile_development::Access` reads them once per op, and `Access::refusal` says why an agent's request is
refused before anything runs: the switch (`OFF`), the policy against the agent (`NOBODY_MAY`,
`NOT_ASSIGNED`), the platform of the device it names (`PLATFORM_OFF`); no tools at all is
`NO_TOOLS`. The routes are the person's: while the switch is off they answer 409 with the
sentence — except the status, read whether or not it is on, since what to install comes before
turning it on.

## The routes and the tools

The node (`crates/bisa-node/src/mobile_development.rs`) answers `GET /mobile-development/status` (and `POST
/mobile-development/status/check` to examine again), `GET /mobile-development/devices`, `POST /mobile-development/devices/{id}/boot`
· `shutdown` · `show`, `POST /mobile-development/simulators {name, devicetype, runtime}`, `GET
/mobile-development/devices/{id}/frame?format=` — the screen as image bytes for the mirror, `no-store`, never
kept — and `POST /mobile-development/devices/{id}/screenshot` — the screen stored as an attachment, answered
with the path of its named copy (`MobileDevelopmentShot`), the way a browser screenshot is. A checkout has
two more: `GET /workstreams/{wid}/mobile-development` says whether it holds a Flutter app (a `pubspec.yaml`
naming Flutter, and the `ios/` and `android/` folders beside it), and `GET
/workstreams/{wid}/mobile-development/run-command?device=` composes the one line a terminal runs —
`<flutter> run -d <id>`, the program quoted, in the checkout — for the **Tauri shell alone**: the
desktop's page never reads it and has no method for it.

Agents get four tools on every menu (`crates/bisa-mcp/src/server.rs`, `Op::MobileDevelopment` in the intake):
`mobile_development_status`, `mobile_development_devices`, `mobile_development_boot`, `mobile_development_screenshot` — the last answering a PNG's
absolute path the agent reads with its own tools, with no window open, in a goal that runs
unattended as much as in a chat. The app itself is the agent's to run: `flutter run -d <id>` in a
terminal, `r` to hot reload, never `-d chrome` — the guard refuses the machine's browser
([11](../11-security.md)). What every session is told is one text, `bisa_core::mobile_development::MOBILE_DEVELOPMENT_NOTE`,
carried by a work item's first prompt and a conversation's framing **only where mobile development
is on** (`framing::mobile_development_note`), and by the MCP server's instructions as one sentence always
(`MOBILE_DEVELOPMENT_HINT`), since the server cannot read the workspace's word. The guard **asks** the person
before a store submission — `altool`, `notarytool`, `fastlane deliver` and its kin, a Gradle
publish — and before every simulator is erased (`store_submission`, `simulator_wipe`); the
everyday lines — `flutter run`, `flutter test`, `flutter build ipa`, `simctl boot`, `adb devices`,
`pod install`, `open -a Simulator` — fall through.

## The Devices button

**Devices** sits beside **Browser** in the IDE's header on a checkout that holds a Flutter app,
while mobile development is on here (`shell/DeviceLauncher.tsx`; the words and the order are
`views/_workbench/devicesModel.mjs`'s). The main click is the one thing most worth doing: show the
device the app is running on; else run on the device that is up; else boot a simulator or an
emulator; else the setup. The caret lists every device of the platforms that are on, up first,
with its verbs by state — *Run on*, *Show*, *Stop the app on*, *Boot*, *Shut down*; an offline
phone said and not offered — then *Check the setup…*, which opens the Settings panel. *Run on*
opens a terminal with the device named (`TerminalTarget.mobile`): the shell asks the node for the
run line, refuses a folder the node names outside the checkout, and runs `sh -c <line>` through
the login shell; the tab is labelled *flutter · <device>* and remembered as one across a restart,
and the store finds it again by workstream and device (`mobileDevelopmentRunSession`). The count on the
button is the devices that are up.

## The device document

A device is a **document** of the workbench (`{ kind: "device", id }`), stored, saved with the
layout, and opened **beside the code**: a root with one pane and something in it is split and the
device takes the new half (`workbenchModel.openTabBeside`); a root already split, or with nothing
open, takes it like any document. Its strip tab wears the device's name, glyph and state from the
devices store, which is read only while a device tab is open.

`views/_workbench/DeviceDoc.tsx` draws the bar — the device and its state; **Run**; **Reload**,
**Restart** and **Stop**, which type `r`, `R` and `q` into the run terminal found by workstream and
device (`writeToTerminalTab`, the one door to type into a terminal), held with *Run on this device
first* until there is one; **Window**, which brings the Simulator forward; the camera's *Copy the
screen*, *Save the screen…*, *Attach the screen to the Agent panel*; the wand — over the
**mirror**: an `<img>` of the node's frame, asked again a few times a second (`FRAME_MS`, 500 ms)
while the device is up and the window is awake and in front, slower after a failure and stopped
with a word after five (`deviceMirrorModel.mirrorCadence`); the last frame stays while it pauses.
The mirror is **watch-only**: a touch is made in the Simulator or emulator window, which *Window*
brings forward — a simulator cannot be told to tap, and one behaviour on both platforms beats a
tap on one. A device that is shut down shows a boot door instead; one that left, a word and *Look
again*.

## Captures

With the wand on, a drag over the mirror is a rectangle in device pixels — mapped through the
picture's drawn box under `object-fit: contain`, clamped to the screen (`markFromDrag`) — and a
click is the whole screen. The screen is **captured at that moment** (`POST …/screenshot`), since
it changes under a running app, and the note box asks what should change there; each capture
becomes a numbered badge over the mirror and a line in the tray (`captureModel.mjs`,
`useDeviceCapture.ts`, `CaptureTray.tsx`). The tray's doors are the annotation tray's, shared
(`ContextTray.tsx`: **Send** to the agent the chip names, as an edit in the checkout's
conversation; **Attach** to the Agent panel; *Clear*). A capture is one chip kind across the wire:
`ContextRef::Capture { device, label, shot: AttachmentRef, mark?, note }` — framed for the agent
as `[capture] <label> · <device> · x 120, y 340 · 200×48`, the note and the file's name; the
transcript names the picture's path, made on demand from the attachment (`mobile_development::capture_path`),
and an image-taking harness receives the PNG itself, as it receives a photo. The same spot of the
same picture is one chip, its note the newer.

## Settings › Capabilities › Mobile Development

`views/_settings/MobileDevelopmentPanel.tsx` over `mobileDevelopmentSettingsModel.mjs`: the switch and the platforms
on one card — iOS held off macOS with the reason —; **the setup**, one row per component in the
order a person installs them — Flutter, Xcode, the iOS simulator runtime, CocoaPods, the Android
SDK with `adb` and the emulator, a virtual device, Java — each *found* with what was found, or
*missing* with the official guide's link and the command the person runs (shown with a copy
button, never run by the platform), or *held* with the reason when its side is off; then Flutter's
own doctor lines; *Check again* examines the machine now, and the window's focus reads the last
look again. **Devices** lists what the machine can reach with *Boot*, *Shut down* and *Show* by
kind and state, *Look again*, and *Create a simulator…* from a device type and a runtime the
status lists (a virtual Android device is made in Android Studio, the door says where). **Which
agents may use the devices** is the same three-way switch the browser has. The two paths are the
registry panel's under it.

## Agents

The catalog's **Mobile Developer** (`library/catalog/agents/mobile-developer.toml`) carries four
skills — `workstream-workflow`, `flutter-development` (the loop: status, devices, boot, `flutter
run` in a terminal, hot reload, screenshot, the two platforms kept in step, what the guard asks),
`app-store-publishing` and `google-play-publishing` (each a numbered procedure from the account to
the release, and an update; the submission the person's press) — and sits in *Engineering*. The
catalog cap is six skills, since every agent carries the browser's and the drawing skill beside up
to four of its own. The
`mobile-release` workflow template builds and runs on the devices here, checks, reviews, and puts
an **approval** before anything is submitted. The Workflow Agent is not told about mobile in its
prompt: its `GOAL` block gains one line — *Mobile: iOS and Android · Flutter 3.24 — mobile_development_devices
lists the simulators, emulators and phones here* — from the last examination, never a probe, and
only when the switch is on (`mobile_development::brief_line`); the staff line for the Mobile Developer names
Flutter, iOS, Android and the stores, which is how a mobile goal reaches it.

## Invariants

- The webview names a device's id and never a program or a path: the run line is the node's, run
  by the shell in the checkout — a folder the node names outside it is refused.
- Nothing runs on this machine's mobile tools unless the node was started with them; every test
  runs against `FakeMobileDevelopment`.
- The switch is off until this machine says otherwise; off, the routes refuse with the sentence,
  the launcher and the document are not drawn, and no session's prompt carries the note — the MCP
  server's one sentence (`MOBILE_DEVELOPMENT_HINT`) is said either way, since the server cannot read the switch.
- A device of a platform that is off is not listed, not booted, not captured.
- A frame is never stored; a capture always is, and reaches an agent as a path, never bytes.
- The mirror asks for nothing while the window is hidden or in the background, and a device that
  stopped answering is said, not hammered.
- No GEP kind: a toolchain, a simulator and a device are one machine's facts.

## Tests

| Invariant | Test |
|---|---|
| the parsers read each program's output on fixtures; the paths resolve in one order over a temp home; the run line is quoted; a PNG's size is read; a missing program is *not installed*; the fake records every call and moves a device it boots | `crates/bisa-mobile-development/src/*` unit tests |
| the mobile note names every tool, the run line and the one licence to stop, and says which platforms; the five keys hold their kinds and scopes; a capture round-trips as `kind: capture` with or without its mark | `crates/bisa-core/src/mobile_development.rs`, `settings.rs`, `message.rs` tests |
| a store submission and a simulator wipe are asked; the everyday Flutter, simulator and adb lines fall through; `flutter run -d chrome` is the browser refusal | `crates/bisa-security/src/builtin.rs` tests |
| no tools is said as such and off is said first; nobody and unassigned are refused and the skill is the assignment; a platform that is off hides its devices and refuses booting one, and a boot announces the devices changed; the toolchain is examined once until *Check again* or a `mobile_development.*` write; a screenshot is stored as an attachment and answered by path; the run line names the resolved Flutter and the checkout; the designer reads a `Mobile:` line only when the feature is on | `crates/bisa-engine/tests/it/mobile_development.rs`, `src/mobile_development.rs` unit tests, `src/framing.rs` tests |
| through the binary, with the switch as a first launch leaves it: every route of the person's but the status refuses with the sentence, an agent's three tools through the real MCP server refuse with it, and no session's prompt names the tools — the journey never turns the switch on and never reads the status, since the machine it runs on is not a test's to examine | the journey `crates/bisa-cli/tests/it/e2e/a_folder_served_and_a_browser_asked.rs`, its third test |
| every mobile route is 503 without the tools and 409 while off; the status, the devices, a boot, a capture and a made simulator read the fake; a frame is image bytes that are never cached; a Flutter checkout is recognised and its run line is the Flutter in the checkout | `crates/bisa-node/tests/it/mobile_development.rs`, `routes.rs` |
| every session holds the four mobile tools and the reference names them; the words an agent reads name the toolchain, the devices and the screen to read | `crates/bisa-mcp/src/server.rs` tests, `tests/it/docs.rs` |
| the Mobile Developer carries four skills the catalog ships, six principles, a home in a team; the `mobile-release` template starts at `build` and loops from the tests and the verdict; every `snake_case` word in the new prose is a tool | `crates/bisa-store/src/catalog.rs` tests, `tests/it/catalog.rs` |
| a mobile run stays under the checkout and belongs to one | `desktop/src-tauri/src/terminal.rs` tests |
| a mobile run tab carries its device, dedupes on it, says *flutter*, survives a restore and is found by workstream and device | `desktop/src/shell/terminalsModel.test.mjs` |
| a device tab round-trips and is stored like a document; `openTabBeside` splits a single pane and puts the document in the new half; a device tab is saved with the layout | `desktop/src/views/_workbench/workbenchModel.test.mjs`, `ideLayoutModel.test.mjs` |
| the main click runs, boots or opens the setup; the caret lists each device's verbs by state and platform | `desktop/src/views/_workbench/devicesModel.test.mjs` |
| the mirror polls only while up, awake and in front and backs off after an error; a drag maps to device pixels and a click is no rectangle; the boot door's words | `desktop/src/views/_workbench/deviceMirrorModel.test.mjs` |
| a capture is added with its picture and removed with the numbers kept; the chips carry the picture and the mark; the same spot twice is one chip | `desktop/src/views/_workbench/captureModel.test.mjs`, `contextChips.test.mjs` |
| the setup rows say what was found and the official way in, and hold a side that is off; the platforms control holds iOS off macOS; the devices' verbs follow kind and state | `desktop/src/views/_settings/mobileDevelopmentSettingsModel.test.mjs` |
| the device document carries its marker, hook and tray; the Devices button follows the Browser one; the mirror reads its cadence; the page never reads the run line; a device tab is saved; Settings lists the panel; a capture is one chip kind across the wire | `desktop/src/scenarios/mobileDevelopment.test.mjs` |
