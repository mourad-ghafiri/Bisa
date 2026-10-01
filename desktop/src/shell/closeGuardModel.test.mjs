/**
 * Whether a close asks, and what it asks. Run with `npm test` from `desktop/`.
 */
import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import { SETTINGS_TABS, settingsPath } from "../views/_settings/settingsLink.mjs";
import { CONFIRM_DEFAULTS, CONFIRM_DOORS, CONFIRM_KEYS, closeQuestion, confirmsClose, namesConfirmKey, othersQuestion, quitQuestion, readConfirmPrefs } from "./closeGuardModel.mjs";

const live = (harness = null) => ({ liveness: { status: "live" }, harness });
const exited = (harness = null) => ({ liveness: { status: "exited", code: 0 }, harness });
const ON = { shell: true, harness: true };

test("the three switches are read from the resolved settings, and anything unusable is the default — on", () => {
  assert.deepEqual(readConfirmPrefs(null), CONFIRM_DEFAULTS);
  assert.deepEqual(readConfirmPrefs([]), { shell: true, harness: true, quit: true });
  assert.deepEqual(readConfirmPrefs([{ key: CONFIRM_KEYS.shell, value: false }]), { shell: false, harness: true, quit: true });
  assert.deepEqual(readConfirmPrefs([{ key: CONFIRM_KEYS.harness, value: "no" }, { key: CONFIRM_KEYS.quit, value: false }]), { shell: true, harness: true, quit: false }, "a non-boolean is the default");
  assert.ok(namesConfirmKey([CONFIRM_KEYS.quit]));
  assert.ok(namesConfirmKey(["editor.tab_size", CONFIRM_KEYS.shell]));
  assert.ok(!namesConfirmKey(["editor.tab_size"]) && !namesConfirmKey(null));
});

test("only a live session asks, and only when its kind's switch is on", () => {
  assert.ok(confirmsClose(live(), ON), "a live shell, switched on");
  assert.ok(confirmsClose(live("claude"), ON), "a running harness, switched on");
  assert.ok(!confirmsClose(exited(), ON), "an exited tab never asks");
  assert.ok(!confirmsClose(exited("claude"), ON));
  assert.ok(!confirmsClose(live(), { shell: false, harness: true }), "the shell switch off: a shell closes at once");
  assert.ok(confirmsClose(live("claude"), { shell: false, harness: true }), "…and a harness still asks");
  assert.ok(!confirmsClose(live("claude"), { shell: true, harness: false }), "the harness switch off: a harness ends at once");
  assert.ok(confirmsClose(live(), { shell: true, harness: false }), "…and a shell still asks");
  assert.ok(!confirmsClose(null, ON));
  // A harness typed into a shell is a harness here: the harness switch decides, and the question says Terminate.
  const typed = { ...live(), running: "claude-code" };
  assert.ok(confirmsClose(typed, { shell: false, harness: true }));
  assert.ok(!confirmsClose(typed, { shell: true, harness: false }));
  assert.equal(closeQuestion(typed).confirmLabel, "Terminate");
});

test("a harness is terminated and a shell is closed, in the words the dialog says", () => {
  assert.deepEqual(closeQuestion(live("claude")), { title: "Terminate this harness?", body: "Closing a running harness ends its process — anything running in it stops.", confirmLabel: "Terminate", door: CONFIRM_DOORS.harness });
  assert.deepEqual(closeQuestion(live()), { title: "Close this terminal?", body: "Closing a live shell ends its process — anything running in it stops.", confirmLabel: "Close it", door: CONFIRM_DOORS.shell });
  assert.equal(othersQuestion(1).body, "1 is still running; closing ends its process.");
  assert.equal(othersQuestion(3).body, "3 are still running; closing ends their processes.");
  assert.equal(othersQuestion(2).door, CONFIRM_DOORS.others);
});

test("every question's door is a Settings tab that exists and names the switch as the catalog labels it", () => {
  const registry = readFileSync(new URL("../../../crates/bisa-core/src/settings.rs", import.meta.url), "utf8");
  const catalog = readFileSync(new URL("../../../locales/en/settings.ftl", import.meta.url), "utf8");
  for (const kind of ["shell", "harness", "quit"]) {
    const door = CONFIRM_DOORS[kind];
    const at = registry.indexOf(`"${CONFIRM_KEYS[kind]}",`);
    assert.ok(at >= 0, `${CONFIRM_KEYS[kind]} is registered`);
    const def = registry.slice(at, at + 400);
    assert.ok(/S::M\s*\)/.test(def), `${kind}'s switch is machine scope`);
    assert.ok(catalog.includes(`\nsetting-${CONFIRM_KEYS[kind].replaceAll(".", "-")} = ${door.switch}\n`), `${kind}'s door names the catalog's label: ${door.switch}`);
  }
  for (const [kind, door] of Object.entries(CONFIRM_DOORS)) {
    assert.ok(SETTINGS_TABS.includes(door.tab), `${kind} opens a Settings tab that exists: ${door.tab}`);
    assert.equal(door.path, settingsPath(door.tab), `${kind} says the path as the rail shows it — the model's own words for the tab it opens`);
    assert.ok(door.lead.endsWith(" under"), `${kind}'s lead-in ends where the path begins`);
  }
  assert.equal(CONFIRM_DOORS.others.switch, null, "the others question covers both kinds, so it names no one switch");
  assert.equal(CONFIRM_DOORS.others.tab, "terminal");
  assert.equal(CONFIRM_DOORS.quit.tab, "desktop");
  assert.equal(CONFIRM_DOORS.quit.path, "Settings › Capabilities › Desktop");
  assert.equal(CONFIRM_DOORS.shell.path, "Settings › Project IDE › Terminal");
});

test("the quit question — one question, whichever door — names what is running and what is unsaved, and says the documents are saved first", () => {
  assert.deepEqual(quitQuestion({ shells: 0, harnesses: 0, dirty: 0 }), { title: "Quit Bisa?", body: "Nothing is running and nothing is unsaved.", confirmLabel: "Quit", door: CONFIRM_DOORS.quit });
  assert.equal(quitQuestion({ shells: 2, harnesses: 1, dirty: 0 }).body, "1 harness and 2 shells are running here; quitting ends them.");
  assert.equal(quitQuestion({ shells: 1, harnesses: 0, dirty: 0 }).body, "1 shell is running here; quitting ends it.");
  assert.equal(quitQuestion({ shells: 0, harnesses: 0, dirty: 1 }).body, "1 document has unsaved changes, saved first.");
  assert.equal(quitQuestion({ shells: 0, harnesses: 1, dirty: 3 }).body, "1 harness is running here; quitting ends it. 3 documents have unsaved changes, saved first.");
});
