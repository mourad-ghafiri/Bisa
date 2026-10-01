/**
 * The devices of the Project IDE (ide/19): a device's words and glyph, and
 * the Devices button's menu. Run with `node --test desktop/src/views/_workbench/devicesModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { deviceById, deviceIcon, deviceMenu, deviceNote, deviceTitle, isUp, platformAllows } from "./devicesModel.mjs";

const sim = (id, name, state, os = "iOS 18.2") => ({ id, name, platform: "ios", kind: "simulator", state, os });
const emu = (id, name, state) => ({ id, name, platform: "android", kind: "emulator", state, os: null });
const phone = (id, name, state) => ({ id, name, platform: "android", kind: "physical", state, os: null });

test("a device says itself in one line, wears a glyph by its kind, and is up when booted or running", () => {
  assert.equal(deviceTitle(sim("A", "iPhone 16", "booted")), "iPhone 16 · iOS 18.2 · simulator · booted");
  assert.equal(deviceTitle(emu("P", "Pixel 8", "shutdown")), "Pixel 8 · emulator · shut down");
  assert.equal(deviceIcon(sim("A", "iPhone 16", "booted")), "simulator");
  assert.equal(deviceIcon(phone("R", "SM G991B", "running")), "device");
  assert.ok(isUp(sim("A", "x", "booted")) && isUp(phone("R", "x", "running")));
  assert.ok(!isUp(emu("P", "x", "shutdown")) && !isUp(phone("R", "x", "offline")));
  assert.equal(deviceNote(sim("A", "x", "booted")), null);
  assert.equal(deviceNote(sim("A", "x", "shutdown")), "shut down");
  assert.equal(deviceById([sim("A", "x", "booted")], "A").name, "x");
  assert.equal(deviceById([], "A"), null);
  assert.ok(platformAllows("both", "ios") && platformAllows("android", "android") && !platformAllows("ios", "android"));
});

test("the main click shows the running device, else runs on the one that is up, else boots one, else opens the setup", () => {
  const facts = { enabled: true, flutter: true, platforms: "both", tabs: [], running: [], busy: false };
  const off = deviceMenu({ ...facts, enabled: false, devices: [] });
  assert.equal(off.main.id, "check");
  assert.ok(off.main.label.startsWith("Set up"));
  assert.deepEqual(off.items.map((i) => i.id), ["check"]);
  const noApp = deviceMenu({ ...facts, flutter: false, devices: [sim("A", "iPhone 16", "booted")] });
  assert.equal(noApp.main.disabled, true, "no Flutter app: the main click is held");
  const none = deviceMenu({ ...facts, devices: [] });
  assert.equal(none.main.id, "check");
  const cold = deviceMenu({ ...facts, devices: [sim("A", "iPhone 16", "shutdown"), phone("R", "SM G991B", "offline")] });
  assert.equal(cold.main.id, "boot:A", "a simulator to boot beats a phone that is offline");
  const up = deviceMenu({ ...facts, devices: [sim("A", "iPhone 16", "booted"), emu("P", "Pixel 8", "shutdown")] });
  assert.equal(up.main.id, "run:A");
  const running = deviceMenu({ ...facts, devices: [sim("A", "iPhone 16", "booted"), emu("P", "Pixel 8", "running")], running: ["P"] });
  assert.equal(running.main.id, "view:P", "the device the app runs on");
  assert.equal(running.main.label, "Show Pixel 8");
  const busy = deviceMenu({ ...facts, devices: [sim("A", "iPhone 16", "booted")], busy: true });
  assert.equal(busy.main.disabled, true);
});

test("the caret lists each device's verbs by state and platform — up first — and the setup last", () => {
  const facts = { enabled: true, flutter: true, platforms: "both", tabs: ["A"], running: ["A"], busy: false };
  const menu = deviceMenu({ ...facts, devices: [emu("P", "Pixel 8", "shutdown"), sim("A", "iPhone 16", "booted"), phone("R", "SM G991B", "offline"), emu("Q", "Pixel Fold", "running")] });
  const ids = menu.items.map((i) => i.id);
  assert.deepEqual(ids, ["view:A", "stop:A", "shutdown:A", "run:Q", "view:Q", "shutdown:Q", "boot:P", "offline:R", "check"]);
  assert.equal(menu.items[0].label, "Show iPhone 16", "a device with a document open is shown");
  assert.equal(menu.items[1].danger, true);
  assert.equal(menu.items[2].disabled, true, "not shut down under the app");
  assert.equal(menu.items[4].label, "Open Pixel Fold beside the code");
  assert.equal(menu.items[3].separatorBefore, true, "one group per device");
  assert.equal(menu.items[7].disabled, true, "an offline phone is said, not offered");
  assert.equal(menu.items[8].separatorBefore, true);
  // A platform that is off hides its devices, and says so when nothing is left.
  const ios = deviceMenu({ ...facts, platforms: "ios", tabs: [], running: [], devices: [emu("P", "Pixel 8", "running")] });
  assert.deepEqual(ios.items.map((i) => i.id), ["check"]);
  assert.ok(ios.main.hint.includes("platform that is off"), ios.main.hint);
});
