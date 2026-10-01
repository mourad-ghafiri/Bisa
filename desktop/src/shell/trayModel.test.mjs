/**
 * The menu bar icon's facts: the state by rank, the count as rows and never
 * twice, the words, what a close means, the two switches.
 * Run with `node --test desktop/src/shell/trayModel.test.mjs`.
 */
import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import { TRAY_DEFAULTS, TRAY_EVENTS, TRAY_KEYS, TRAY_STATES, closeVerb, namesTrayKey, readTrayPrefs, statusWords, trayReport, trayState, workingCount } from "./trayModel.mjs";

const ask = { kind: "ask", gate_kind: "ask", id: "a" };
const row = (over = {}) => ({ id: "r", kind: "goal", title: "t", needs_action: [], notices: [], unread_notices: 0, read: true, mentioned: false, join: null, waiting: null, ...over });
const session = (state) => ({ id: state, state, harness: "h", scope: "g:1", children: [] });

test("the state is ranked: trouble, then owed, then work, then stillness", () => {
  assert.equal(trayState({ conn: "closed" }), "connecting", "before the node has ever answered, not open is connecting");
  assert.equal(trayState({ conn: "connecting" }), "connecting");
  assert.equal(trayState({ conn: "closed", everOpen: true, needs: 3, working: 2 }), "trouble", "a node that was there and is not outranks everything");
  assert.equal(trayState({ conn: "connecting", everOpen: true }), "trouble", "a reconnect is still trouble until it opens");
  assert.equal(trayState({ conn: "open", needs: 1, working: 5, paused: true }), "waiting", "what is owed outranks work");
  assert.equal(trayState({ conn: "open", working: 1, paused: true }), "working");
  assert.equal(trayState({ conn: "open", paused: true }), "paused");
  assert.equal(trayState({ conn: "open" }), "quiet");
  // A lagged pulse is an open stream that dropped events: the node is there, and what is owed is still owed.
  assert.equal(trayState({ conn: "lagged", everOpen: true }), "quiet", "never the red dot over a node that answered");
  assert.equal(trayState({ conn: "lagged", everOpen: true, needs: 2 }), "waiting");
  for (const s of ["trouble", "connecting", "waiting", "working", "paused", "quiet"]) assert.ok(TRAY_STATES.includes(s), s);
  assert.deepEqual([...TRAY_STATES], ["trouble", "connecting", "waiting", "working", "paused", "quiet"], "in rank order");
  // The list is the rank: whatever holds at once, the state is the first of them in it.
  const facts = [
    { conn: "closed", everOpen: true, needs: 1, working: 1, paused: true },
    { conn: "closed", needs: 1, working: 1, paused: true },
    { conn: "open", needs: 1, working: 1, paused: true },
    { conn: "open", working: 1, paused: true },
    { conn: "open", paused: true },
    { conn: "open" },
  ];
  assert.deepEqual(facts.map(trayState), [...TRAY_STATES]);
  assert.equal(trayState({ conn: "open", paused: "yes", needs: -1, working: Number.NaN }), "quiet", "what is no fact holds nothing");
});

test("the count is the Inbox's rows that need you — a waiting harness once, never the unread", () => {
  const inbox = [row({ needs_action: [ask, ask] }), row({ join: { id: "j" } }), row({ waiting: { session: "s" } }), row({ read: false }), row({ read: false, notices: [{}], unread_notices: 1 })];
  const sessions = [session("waiting"), session("running"), session("thinking"), session("done")];
  const r = trayReport({ conn: "open", inbox, sessions, working: { "g:1": ["planner"], "g:2": [] } });
  assert.equal(r.needs, 3, "two asks on one row are one row; the join and the waiting harness are one each; unread is not owed");
  assert.equal(r.state, "waiting");
  assert.equal(r.needs_words, "3 need you");
  assert.equal(r.status, "Working — 3 agents", "two running sessions and one scope mid-turn; the waiting session is not work");
  assert.equal(workingCount(sessions, {}), 2);
  assert.equal(workingCount(null, null), 0);
});

test("the words for every state, and the tooltip's sentence has a count line of its own", () => {
  assert.equal(statusWords("connecting"), "Connecting to the node…");
  assert.equal(statusWords("trouble"), "The node is unreachable");
  assert.equal(statusWords("waiting", 0), "Waiting on you");
  assert.equal(statusWords("waiting", 2), "Working — 2 agents", "owed and at work: the work is the sentence, the count its own line");
  assert.equal(statusWords("working", 1), "Working — 1 agent");
  assert.equal(statusWords("paused"), "Paused — nothing starts");
  assert.equal(statusWords("quiet"), "Quiet — nothing running");
  const calm = trayReport({ conn: "open" });
  assert.deepEqual(calm, { state: "quiet", needs: 0, status: "Quiet — nothing running", needs_words: "Nothing needs you" });
  const first = trayReport({ conn: "closed" });
  assert.equal(first.state, "connecting");
  const lost = trayReport({ conn: "closed", everOpen: true, inbox: [row({ needs_action: [ask] })] });
  assert.equal(lost.state, "trouble");
  assert.equal(lost.needs, 1, "the count is still said while the node is away; the dot says the node is");
});

test("closing the window hides while the switch is on and quits when it is off; the switches read with their defaults", () => {
  assert.equal(closeVerb(TRAY_DEFAULTS), "hide");
  assert.equal(closeVerb({ closeKeepsRunning: false }), "quit");
  assert.equal(closeVerb(null), "quit", "no answer at all is the safe one: nothing hides unasked");
  assert.deepEqual(readTrayPrefs(null), { closeKeepsRunning: true, dockIcon: true });
  assert.deepEqual(readTrayPrefs([{ key: TRAY_KEYS.closeKeepsRunning, value: false }, { key: TRAY_KEYS.dockIcon, value: "yes" }]), { closeKeepsRunning: false, dockIcon: true }, "a non-boolean is the default");
  assert.ok(namesTrayKey([TRAY_KEYS.dockIcon]));
  assert.ok(!namesTrayKey(["desktop.confirm_quit"]));
  assert.ok(!namesTrayKey(undefined));
});

test("the two keys are registered, machine scope, under the labels the guide uses", () => {
  const registry = readFileSync(new URL("../../../crates/bisa-core/src/settings.rs", import.meta.url), "utf8");
  for (const [name, key] of Object.entries(TRAY_KEYS)) {
    const at = registry.indexOf(`"${key}",`);
    assert.ok(at >= 0, `${key} is registered`);
    const def = registry.slice(at, at + 400);
    assert.ok(/S::M\s*\)/.test(def), `${name} is machine scope`);
    assert.ok(def.includes("json!(true)"), `${name} defaults on`);
  }
  const catalog = readFileSync(new URL("../../../locales/en/settings.ftl", import.meta.url), "utf8");
  assert.ok(catalog.includes("\nsetting-desktop-close_keeps_running = Closing the window keeps Bisa running\n"), "the label is the catalog's");
  assert.ok(catalog.includes("\nsetting-desktop-dock_icon = Show in the Dock\n"));
});

test("the four events are spelt as the shell spells them", () => {
  const main = readFileSync(new URL("../../src-tauri/src/main.rs", import.meta.url), "utf8");
  const tray = readFileSync(new URL("../../src-tauri/src/tray/mod.rs", import.meta.url), "utf8");
  assert.ok(main.includes(`const CLOSE_REQUESTED: &str = "${TRAY_EVENTS.close}";`));
  assert.ok(main.includes(`const QUIT_REQUESTED: &str = "${TRAY_EVENTS.quit}";`));
  assert.ok(tray.includes(`pub const GO_EVENT: &str = "${TRAY_EVENTS.go}";`));
  assert.ok(tray.includes(`pub const DOCK_EVENT: &str = "${TRAY_EVENTS.dock}";`));
});
