/**
 * Settings › Capabilities › Mobile Development, as facts (ide/19). Run with
 * `node --test desktop/src/views/_settings/mobileDevelopmentSettingsModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { INSTALL, checkedWords, componentRows, deviceRows, doctorRows, platformSegments, platformShown, platformWords, policySegments, policyWords, simulatorProblem, statusWords } from "./mobileDevelopmentSettingsModel.mjs";

const toolchain = (over = {}) => ({
  flutter: { installed: true, path: "/opt/flutter/bin/flutter", version: "3.24.3", channel: "stable", dart: "3.5.3" },
  xcode: { installed: false },
  ios_runtimes: [],
  ios_devicetypes: [],
  cocoapods: { installed: false },
  android: { path: "/Users/me/Library/Android/sdk", adb: "/Users/me/Library/Android/sdk/platform-tools/adb", emulator: null, avds: [] },
  java: { installed: true, version: "17.0.10" },
  doctor: [],
  ...over,
});

test("the platforms control holds iOS off macOS with the reason, and shows Android there whatever was written", () => {
  const mac = platformSegments(true);
  assert.deepEqual(mac.map((s) => s.id), ["both", "ios", "android"]);
  assert.ok(mac.every((s) => !s.disabled));
  const linux = platformSegments(false);
  assert.equal(linux[0].disabled, true);
  assert.equal(linux[1].disabled, true);
  assert.equal(linux[1].hint, "iOS needs Xcode, which runs on macOS only");
  assert.equal(linux[2].disabled, false);
  assert.equal(platformShown("both", false), "android");
  assert.equal(platformShown("both", true), "both");
  assert.equal(platformShown("android", false), "android");
  assert.equal(platformWords("ios"), "iOS only");
  assert.deepEqual(policySegments().map((s) => s.id), ["everyone", "assigned", "nobody"]);
  assert.ok(policyWords("assigned").includes("Flutter Development"));
});

test("the status card says off, on, or on without Flutter", () => {
  assert.equal(statusWords({ enabled: false, platforms: "both", flutter: true }).label, "Off");
  assert.equal(statusWords({ enabled: false, platforms: "both", flutter: true }).tone, "quiet");
  const on = statusWords({ enabled: true, platforms: "android", flutter: true });
  assert.equal(on.tone, "ok");
  assert.ok(on.sentence.includes("Android only"));
  const missing = statusWords({ enabled: true, platforms: "both", flutter: false });
  assert.equal(missing.tone, "warn");
  assert.equal(statusWords({ enabled: true, platforms: "both", flutter: null }).tone, "ok", "unread is not missing");
});

test("the setup rows say what was found, the official way in for what was not, and hold a side that is off", () => {
  const rows = componentRows(toolchain(), { platforms: "both", mac: true });
  assert.deepEqual(rows.map((r) => r.id), ["flutter", "xcode", "runtime", "cocoapods", "android", "avd", "java"]);
  assert.equal(rows[0].tone, "ok");
  assert.equal(rows[0].found, "3.24.3 (stable) · Dart 3.5.3 · /opt/flutter/bin/flutter");
  assert.equal(rows[0].install, null, "found: nothing to install");
  assert.equal(rows[1].tone, "warn");
  assert.equal(rows[1].install, INSTALL.xcode);
  assert.ok(rows[1].install.command.startsWith("xcode-select --install"));
  assert.equal(rows[4].tone, "ok");
  assert.ok(rows[4].found.includes("no emulator"));
  assert.equal(rows[5].tone, "warn");
  assert.equal(rows[5].install.command, null, "a virtual device is made in Android Studio");
  // Android off: its rows are held, not missing.
  const ios = componentRows(toolchain(), { platforms: "ios", mac: true });
  assert.equal(ios[4].tone, "quiet");
  assert.equal(ios[4].held, "Android is off in the platforms above");
  assert.equal(ios[4].install, null);
  // Off macOS: the iOS rows are held with the reason.
  const linux = componentRows(toolchain(), { platforms: "both", mac: false });
  assert.equal(linux[1].held, "iOS needs Xcode, which runs on macOS only");
  assert.equal(linux[2].tone, "quiet");
  assert.deepEqual(componentRows(null, { platforms: "both", mac: true }), [], "unread: no rows");
  // A runtime found names itself.
  const withRuntime = componentRows(toolchain({ ios_runtimes: [{ identifier: "x", name: "iOS 18.2", version: "18.2", available: true }, { identifier: "y", name: "iOS 17.5", version: "17.5", available: false }] }), { platforms: "both", mac: true });
  assert.equal(withRuntime[2].tone, "ok");
  assert.equal(withRuntime[2].found, "iOS 18.2");
  assert.deepEqual(doctorRows([{ state: "ok", name: "Flutter", detail: "3.24.3" }, { state: "missing", name: "Xcode" }, { state: "warning", name: "Android toolchain" }]).map((r) => r.tone), ["ok", "warn", "neutral"]);
  assert.equal(checkedWords(0), "not checked yet");
  assert.equal(checkedWords(1000, 1010), "checked just now");
  assert.equal(checkedWords(1000, 1000 + 5 * 60), "checked 5 m ago");
  assert.equal(checkedWords(1000, 1000 + 3 * 3600), "checked 3 h ago");
});

test("the devices' verbs follow their kind and state, and a simulator draft needs its three words", () => {
  const rows = deviceRows(
    [
      { id: "A", name: "iPhone 16", platform: "ios", kind: "simulator", state: "booted", os: "iOS 18.2" },
      { id: "B", name: "iPad", platform: "ios", kind: "simulator", state: "shutdown", os: "iOS 18.2" },
      { id: "P", name: "Pixel 8", platform: "android", kind: "emulator", state: "running" },
      { id: "R", name: "SM G991B", platform: "android", kind: "physical", state: "offline" },
    ],
    "both",
  );
  assert.deepEqual(rows.map((r) => r.verbs), [["shutdown", "show"], ["boot"], ["shutdown"], []]);
  assert.equal(rows[0].words, "iOS 18.2 · simulator · booted");
  assert.equal(rows[1].words, "iOS 18.2 · simulator · shut down");
  assert.equal(rows[3].words, "device · offline");
  assert.deepEqual(deviceRows(rows.length ? [{ id: "P", name: "Pixel 8", platform: "android", kind: "emulator", state: "running" }] : [], "ios"), [], "a platform that is off hides its devices");
  assert.equal(simulatorProblem({ name: " ", devicetype: "x", runtime: "y" }), "Name the simulator");
  assert.equal(simulatorProblem({ name: "Test", devicetype: "", runtime: "y" }), "Choose a device type");
  assert.equal(simulatorProblem({ name: "Test", devicetype: "x", runtime: "" }), "Choose a runtime");
  assert.equal(simulatorProblem({ name: "Test", devicetype: "x", runtime: "y" }), null);
});
