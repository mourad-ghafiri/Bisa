/**
 * What reaches a person outside the window, over an afternoon: an agent
 * that waits, a gate it already announced, a session that fails, one that
 * finishes, a proposal, a listener that could not start its run — each
 * named by the moment (`shell/notificationsModel.mjs`), then passed or kept
 * back by the person's six switches through the one filter, `allowed`;
 * while the menu bar icon and the Dock badge go on saying what is owed
 * (`shell/trayModel.mjs`), switches or no switches. No notification is sent
 * and no window opened: the scenario steps the models the door steps.
 *
 * Run with `node --test --import ./src/i18n/preload.mjs desktop/src/scenarios/notifications.test.mjs`.
 */

import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import { CATEGORIES, NOTIFY_DEFAULTS, NOTIFY_KEYS, allowed, emptyMemory, namesNotifyKey, onFrame, onTransition, readNotifyPrefs } from "../shell/notificationsModel.mjs";
import { inboxBadge } from "../shell/sidebarModel.mjs";
import { trayReport } from "../shell/trayModel.mjs";
import { CATEGORY_KEYS, MASTER_KEY, askVerb, permissionOf, permissionWords } from "../views/_settings/notificationsSettingsModel.mjs";

const src = (name) => readFileSync(new URL(name, import.meta.url), "utf8");
const session = (state, extra = {}) => ({ id: "S1", kind: "worker", state, since: 1, harness: "claude-code", agent: "developer", cost: { input_tokens: 0, output_tokens: 0, usd_cents: 0 }, children: [], last_activity: 1, ...extra });

/** The door: every notice the models name, with whether this person's switches let it through. */
function door(prefs) {
  let memory = emptyMemory();
  const sent = [];
  const kept = [];
  const pass = (r) => {
    memory = r.memory;
    if (!r.notice) return;
    (allowed(prefs(), r.notice.category) ? sent : kept).push(r.notice);
  };
  return {
    sent,
    kept,
    moved: (prev, row) => pass(onTransition(prev, row, memory)),
    heard: (payload) => pass(onFrame(payload, memory)),
  };
}

/** The afternoon, the same for every person. */
function afternoon(d) {
  const waiting = { state: "waiting", on: { on: "permission", tool: "Bash", gate_id: "g1" } };
  d.moved({ state: "running", tool: "Edit" }, session(waiting));
  // The engine opens the gate the session already announced.
  d.heard({ type: "gate_opened", gate_id: "g1", gate: "escalation", question: "Allow Bash?" });
  // A step's approval, with no session behind it.
  d.heard({ type: "gate_opened", gate_id: "g2", gate: "approval", question: "Ship it?" });
  d.moved(waiting, session({ state: "failed", reason: "the model refused" }));
  d.moved({ state: "running", tool: "Edit" }, session({ state: "done" }, { id: "S2", agent: "reviewer" }));
  d.heard({ type: "workflow_proposed", workflow: "W1", revision: 1, gate_id: "g3" });
  d.heard({ type: "listener_failed", listener: "workspace:W1/ticket", signal: "s1", error: "the hook body was not JSON" });
  d.heard({ type: "listener_failed", listener: "workspace:W1/ticket", signal: "s1", error: "the hook body was not JSON" });
  // What is no moment at all.
  d.heard({ type: "step_changed", run: "R1", step: "build", state: "running" });
  d.heard({ type: "agent_streamed", scope: "G1", agent: "developer" });
}

test("with the switches as they ship, an afternoon is five notices: what waits, what failed, what is proposed — finished work kept back, nothing said twice", () => {
  const prefs = readNotifyPrefs([]);
  assert.deepEqual(prefs, { ...NOTIFY_DEFAULTS });
  const d = door(() => prefs);
  afternoon(d);
  assert.deepEqual(
    d.sent.map((n) => [n.category, n.title, n.body]),
    [
      ["asks", "Bisa — waiting on you", "developer: permission: Bash"],
      ["asks", "Bisa — waiting on you", "Ship it?"],
      ["failures", "Bisa — a session failed", "developer: the model refused"],
      ["workflows", "Bisa — a workflow to adopt", "The Workflow Agent proposed a workflow."],
      ["failures", "Bisa — a start event failed", "A workflow's start event could not start a run: the hook body was not JSON"],
    ],
  );
  assert.deepEqual(d.kept.map((n) => [n.category, n.body]), [["done", "reviewer finished"]], "finished work is not an interruption until asked for");
});

test("each switch keeps back its own category and no other; the master keeps back everything", () => {
  const counts = (resolved) => {
    const prefs = readNotifyPrefs(resolved);
    const d = door(() => prefs);
    afternoon(d);
    return Object.fromEntries(["asks", "failures", "done", "workflows"].map((c) => [c, d.sent.filter((n) => n.category === c).length]));
  };
  assert.deepEqual(counts([]), { asks: 2, failures: 2, done: 0, workflows: 1 });
  assert.deepEqual(counts([{ key: NOTIFY_KEYS.asks, value: false }]), { asks: 0, failures: 2, done: 0, workflows: 1 });
  assert.deepEqual(counts([{ key: NOTIFY_KEYS.failures, value: false }]), { asks: 2, failures: 0, done: 0, workflows: 1 });
  assert.deepEqual(counts([{ key: NOTIFY_KEYS.done, value: true }]), { asks: 2, failures: 2, done: 1, workflows: 1 });
  assert.deepEqual(counts([{ key: NOTIFY_KEYS.workflows, value: false }]), { asks: 2, failures: 2, done: 0, workflows: 0 });
  assert.deepEqual(counts([{ key: NOTIFY_KEYS.enabled, value: false }, { key: NOTIFY_KEYS.done, value: true }]), { asks: 0, failures: 0, done: 0, workflows: 0 });
  // The two notices no session and no frame make: an addon's, and the app's own word.
  const muted = readNotifyPrefs([{ key: NOTIFY_KEYS.enabled, value: false }]);
  assert.deepEqual([allowed(muted, "addons"), allowed(muted, "app")], [false, false]);
  const noAddons = readNotifyPrefs([{ key: NOTIFY_KEYS.addons, value: false }]);
  assert.deepEqual([allowed(noAddons, "addons"), allowed(noAddons, "app")], [false, true], "the app's own word has no switch but the master");
});

test("a switch flipped while the app runs takes the next notice: the door reads the switches as they stand", () => {
  let prefs = readNotifyPrefs([]);
  const d = door(() => prefs);
  d.heard({ type: "gate_opened", gate_id: "g1", gate: "approval", question: "Ship it?" });
  assert.equal(d.sent.length, 1);
  // Settings › System › Notifications: *Asks* off. The frame names the key, and the follower reads again.
  assert.ok(namesNotifyKey([NOTIFY_KEYS.asks]) && !namesNotifyKey(["desktop.dock_icon"]));
  prefs = readNotifyPrefs([{ key: NOTIFY_KEYS.asks, value: false }]);
  d.heard({ type: "gate_opened", gate_id: "g2", gate: "approval", question: "And this?" });
  assert.equal(d.sent.length, 1);
  assert.deepEqual(d.kept.map((n) => n.body), ["And this?"]);
});

test("switched off, nothing is lost: the menu bar icon, the Dock badge and the Inbox go on saying what needs you", () => {
  const rows = [
    { key: "G1", kind: "goal", title: "ship", read: false, unread_count: 0, needs_action: [{ gate_id: "g2", gate_kind: "approval", question: "Ship it?" }], notices: [], unread_notices: 0 },
    { key: "C1", kind: "channel", title: "lounge", read: false, unread_count: 4, needs_action: [], notices: [], unread_notices: 0 },
  ];
  const report = trayReport({ conn: "open", everOpen: true, inbox: rows, sessions: [session({ state: "running", tool: "Edit" })] });
  assert.deepEqual([report.state, report.needs, report.needs_words], ["waiting", 1, "1 needs you"], "the count is what is owed — never the unread");
  assert.deepEqual([inboxBadge(rows).needs, inboxBadge(rows).unread], [1, 1], "the sidebar counts both — what is owed and what is unread, side by side, never one sum");
  const tray = src("../shell/useTray.ts");
  assert.ok(!tray.includes("notifyPrefs") && !tray.includes("allowed("), "no switch of the notifications' reaches the icon");
});

test("the card draws the master, then one switch per category, beside what macOS says", () => {
  assert.equal(MASTER_KEY, NOTIFY_KEYS.enabled);
  assert.deepEqual([...CATEGORY_KEYS], CATEGORIES.map((c) => NOTIFY_KEYS[c]));
  assert.equal(Object.keys(NOTIFY_KEYS).length, 6, "six keys");
  // Never asked: *Allow* asks once. Refused: only System Settings can change the answer — also when the panel is opened again.
  assert.equal(askVerb(permissionOf(false, "default")), "Allow");
  const refused = permissionOf(false, "denied");
  assert.equal(askVerb(refused), null);
  assert.equal(permissionWords(refused, true).label, "not allowed");
  // Granted by macOS and off here reads as both.
  assert.equal(permissionWords(permissionOf(true, "granted"), false).label, "granted · off here");
});

test("one door: every notice passes the one filter, and a node away at boot never leaves the defaults standing", () => {
  const notifications = src("../shell/notifications.ts");
  assert.ok(notifications.includes("if (!allowed(notifyPrefs(), notice.category))"), "the filter stands at the door");
  assert.equal(notifications.split("sendNotification(").length, 2, "and the door is the one place a notification is sent");
  for (const word of ['deliver({ category: "addons", title, body })', 'deliver({ category: "app", title, body })']) assert.ok(notifications.includes(word), word);
  const everywhere = ["../addons/addonBridge.ts", "../shell/useCloseGuard.ts"].map(src).join("\n");
  assert.ok(!everywhere.includes("sendNotification("), "an addon's notice and the app's own word go through the door");
  const follower = src("../shell/notifySettings.ts");
  assert.ok(follower.includes("namesNotifyKey(e.payload.keys)") && follower.includes("useReloadOnReconnect(() => void load())"));
});
