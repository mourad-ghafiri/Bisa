/**
 * Mobile development in the Project IDE, as the desktop draws it (ide/19)
 * — source-text facts that hold the shape across the files: the device
 * document carries its context marker, the one capture hook and the
 * capture tray; the Devices button follows the Browser one; the mirror
 * reads its cadence and pauses when the window is hidden; the webview
 * names a device and never a program — the shell reads the run line from
 * the node and keeps the folder under the checkout; a device tab is saved
 * with the layout; Settings lists the panel; the icons exist. Run with
 * `node --test desktop/src/scenarios/mobileDevelopment.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { emptyWorkbench, openTab, openTabBeside, panesFor, rootKey, tabId } from "../views/_workbench/workbenchModel.mjs";
import { leaves } from "../shell/paneTreeModel.mjs";

const src = join(dirname(fileURLToPath(import.meta.url)), "..");
const read = (rel) => readFileSync(join(src, rel), "utf8");

test("the device document carries its context marker, the one capture hook, the capture tray and the note box", () => {
  const doc = read("views/_workbench/DeviceDoc.tsx");
  assert.ok(doc.includes("data-device-doc"), "the body carries data-device-doc");
  assert.ok(doc.includes("useDeviceCapture({"), "captures go through the one hook");
  assert.ok(doc.includes("<CaptureTray") && doc.includes("<CaptureNoteBox"), "the tray and the note box");
  for (const verb of ['aria-label={t("workbench-device-doc-hot-reload")}', 'aria-label={t("workbench-device-doc-hot-restart")}', 'aria-label={t("workbench-device-doc-stop-app")}', 'aria-label={t("workbench-device-doc-run-app-device")}'] ) {
    assert.ok(doc.includes(verb), `the bar has ${verb}`);
  }
  assert.ok(doc.includes('write("r")') && doc.includes('write("R")') && doc.includes('write("q")'), "reload, restart and stop are keys to the run terminal");
  assert.ok(doc.includes("mobileDevelopmentRunSession(sessions, wid, deviceId)"), "the run terminal is found by workstream and device");
  assert.ok(!doc.includes("flutter run"), "the document never spells the run line: the node composes it");
});

test("the mirror reads its cadence from the model and pauses when the window is hidden or in the background", () => {
  const doc = read("views/_workbench/DeviceDoc.tsx");
  assert.ok(doc.includes("mirrorCadence({"), "the cadence is the model's");
  assert.ok(doc.includes("useVisible()"), "hidden: no frame is asked");
  assert.ok(doc.includes('window.addEventListener("blur"'), "in the background: no frame is asked");
  assert.ok(doc.includes("api.mobileDevelopmentFrameUrl("), "frames come from the node, never bytes through the page");
  const model = read("views/_workbench/deviceMirrorModel.mjs");
  assert.ok(model.includes("export const FRAME_MS = 500"), "a few frames a second, never a video");
});

test("the Devices button follows the Browser one in the IDE's header, on a checkout alone, and opens a device beside the code", () => {
  const screen = read("views/Workbench.tsx");
  assert.ok(screen.indexOf("<BrowserLauncher") < screen.indexOf("<DeviceLauncher"), "Browser, then Devices");
  assert.ok(screen.includes('scope === "workstream" && (\n          <DeviceLauncher'), "a checkout's alone");
  assert.ok(screen.includes("openDocBeside(key, tab)"), "a device opens beside the code");
  assert.ok(screen.includes('tab.kind === "device" && scope === "workstream"'), "the document is a checkout's");
  const launcher = read("shell/DeviceLauncher.tsx");
  assert.ok(launcher.includes("deviceMenu({"), "the menu is the model's");
  assert.ok(launcher.includes("mobileDevelopment: { device: device.id }"), "a run names the device, and nothing else");
  assert.ok(launcher.includes('e.payload.type === "mobile_development_changed"'), "the list follows the bus");
});

test("the webview names a device and never a program: the shell reads the run line from the node and keeps the folder under the checkout", () => {
  const session = read("terminal/session.ts");
  assert.ok(session.includes("mobileDevelopment,\n    onOutput: channel,"), "terminal_open takes the device");
  assert.ok(session.includes("export async function writeTerminal("), "the one door to type into a terminal");
  const api = read("api.ts");
  assert.ok(!api.includes("mobile-development/run-command"), "the page never reads the run line — the shell's route");
  const shell = read("../src-tauri/src/terminal.rs");
  assert.ok(shell.includes("fn mobile_development_launch("), "the shell fetches the line");
  assert.ok(shell.includes("/mobile-development/run-command?device="), "from the node's route");
  assert.ok(shell.includes("fn mobile_development_cwd(") && shell.includes("starts_with(root)"), "the folder stays under the checkout");
  assert.ok(shell.includes('program: "sh".to_string()'), "run through the login shell like the project's run command");
  const model = read("shell/terminalsModel.mjs");
  assert.ok(model.includes("export function mobileDevelopmentRunSession(") && model.includes("export function mobileDevelopmentOf("), "the model knows a mobile run");
});

test("a device tab is a stored document: saved with the layout, split beside the code, named from the devices store", () => {
  const layout = read("views/_workbench/ideLayoutModel.mjs");
  assert.ok(layout.includes('t.kind === "device" ? { kind: "device", id: t.id }'), "saved with its id");
  const centre = read("views/_workbench/CenterDocuments.tsx");
  assert.ok(centre.includes("useDevices(tabs.some((t) => t.kind === \"device\"))"), "the devices are read only while a device tab is open");
  assert.ok(centre.includes('if (t.kind === "device") {'), "the strip names it from the store");
  const key = rootKey("workstream", "01JW");
  const state = openTabBeside(openTab(emptyWorkbench(), key, { kind: "file", path: "lib/main.dart" }), key, { kind: "device", id: "AAAA-1" });
  const panes = leaves(panesFor(state, key));
  assert.equal(panes.length, 2);
  assert.equal(panes[1].active, tabId({ kind: "device", id: "AAAA-1" }));
});

test("the feature is named Mobile Development everywhere a name is needed, and `mobile` alone means a phone, an app or a platform", () => {
  // The crate, the modules, the keys, the routes, the tools, the event: the one word, in every shape it takes.
  const api = read("api.ts");
  assert.ok(api.includes("`/mobile-development/status`") && api.includes("`/mobile-development/devices`") && api.includes("/mobile-development/simulators`"), "the routes");
  assert.ok(!/\/mobile[/`"]/.test(api), "nothing is reached under the old prefix any more");
  assert.ok(api.includes("mobileDevelopmentStatus:") && api.includes("workstreamMobileDevelopment:") && api.includes("mobileDevelopmentFrameUrl:"), "the client's verbs");
  const model = read("views/_settings/mobileDevelopmentSettingsModel.mjs");
  for (const key of ['ENABLED_KEY = "mobile_development.enabled"', 'PLATFORMS_KEY = "mobile_development.platforms"', 'AGENTS_KEY = "mobile_development.agents"']) {
    assert.ok(model.includes(key), `the settings key: ${key}`);
  }
  assert.ok(!/"mobile\.[a-z]/.test(model) && !/"mobile\.[a-z]/.test(read("views/_settings/MobileDevelopmentPanel.tsx")), "no key of the feature says mobile alone");
  assert.ok(read("activity.ts").includes("mobile_development_changed: true") && read("views/_pulse/pulseModel.mjs").includes('"mobile_development_changed"'), "the event");
  assert.ok(read("types.hand.ts").includes('type: "mobile_development_changed"'), "the frame's kind");
  const terminals = read("shell/terminalsModel.mjs");
  assert.ok(terminals.includes("mobileDevelopment: mobileDevelopmentOf(") && !/\bmobile:/.test(terminals), "a terminal's device runs under the feature's name");
  const screens = read("../../locales/en/desktop/screens.ftl");
  assert.ok(screens.includes("screens-settings-mobile-development = Mobile Development"), "the person reads Mobile Development, title case as Decision Making");
  // What means a phone, an app or a platform keeps its word.
  assert.ok(read("ui/Tags.tsx").includes('"mobile",'), "the project tag");
  assert.ok(read("generatedNames.test.mjs").includes('"MobilePlatform"'), "the target platform's schema name");
  assert.ok(api.includes("MobileDevice"), "a device is a device");
});

test("Settings lists the Mobile Development panel under Capabilities, and the icons the feature wears exist", () => {
  const link = read("views/_settings/settingsLink.mjs");
  assert.ok(link.includes('"mobile-development",') && !link.includes('"mobile",'), "the tab id, the feature's word");
  const settings = read("views/Settings.tsx");
  assert.ok(link.includes('panel("mobile-development", t("screens-settings-mobile-development"),'), "the rail's panel, under Capabilities");
  assert.ok(link.indexOf('group("capabilities"') < link.indexOf('panel("mobile-development"') && link.indexOf('panel("mobile-development"') < link.indexOf('group("security"'), "in the Capabilities group");
  assert.ok(settings.includes('panel.id === "mobile-development" &&') && settings.includes("<MobileDevelopmentPanel />"), "the panel");
  assert.ok(settings.includes('<RegistryPanel group="mobile_development"'), "the paths are the registry panel's");
  const panel = read("views/_settings/MobileDevelopmentPanel.tsx");
  assert.ok(panel.includes("componentRows(") && panel.includes("api.mobileDevelopmentCheck()"), "the setup rows and Check again");
  assert.ok(panel.includes("platformSegments(isMac)"), "iOS is held off macOS");
  assert.ok(panel.includes("api.createSimulator("), "a simulator is made from here");
  assert.ok(!panel.includes("xcrun") && !panel.includes("adb "), "the panel names no program: the node probes");
  const icons = read("ui/icons.ts");
  for (const key of ["device: Smartphone", "simulator: MonitorSmartphone", "hotReload: Flame", "hotRestart: RotateCcw", "boot: Power"]) {
    assert.ok(icons.includes(key), `icons.ts maps ${key}`);
  }
});

test("a capture is one chip kind across the wire, the tray and the framing", () => {
  const chips = read("views/_workbench/contextChips.mjs");
  assert.ok(chips.includes('case "capture":'), "the chip's label and identity");
  const studio = read("views/_studio/ContextChips.tsx");
  assert.ok(studio.includes('case "capture":'), "drawn like any chip");
  const tray = read("views/_workbench/CaptureTray.tsx");
  assert.ok(tray.includes("<ContextTray"), "the same doors as an annotation tray");
  const annotation = read("views/_workbench/AnnotationTray.tsx");
  assert.ok(annotation.includes("<ContextTray"), "one tray of chips, two wrappers");
  const rust = readFileSync(join(src, "../../crates/bisa-core/src/message.rs"), "utf8");
  assert.ok(rust.includes("Capture {"), "the wire's variant");
  const framing = readFileSync(join(src, "../../crates/bisa-engine/src/framing.rs"), "utf8");
  assert.ok(framing.includes("[capture]"), "framed for the agent");
});
