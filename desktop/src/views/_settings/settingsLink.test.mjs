/**
 * The Settings deep link, tested where it lives.
 *
 * Nothing here renders — there is no jsdom in this repo. What can actually be
 * wrong in a way a person notices: a link from another screen landing on the
 * wrong panel, and the catalog's `?kind=` being lost or written empty when it
 * composes with `?tab=`.
 *
 * Run with `npm test` from `desktop/`.
 */

import { strict as assert } from "node:assert";
import { test } from "node:test";

import { SETTINGS_TABS, settingsSearch, settingsTab } from "./settingsLink.mjs";

test("an unknown or absent tab lands on the first panel rather than nothing", () => {
  // The hash is user-editable and links outlive renames, so this is the same
  // fallback an unmatched route gets — never a blank screen.
  assert.equal(settingsTab(null), "identity");
  assert.equal(settingsTab(undefined), "identity");
  assert.equal(settingsTab(""), "identity");
  assert.equal(settingsTab("nonesuch"), "identity");
  for (const id of SETTINGS_TABS) assert.equal(settingsTab(id), id);
});

test("each catalog kind is its own settings panel, and the ids links were written against still resolve", () => {
  // One entry per kind now, each its own screen under Library.
  for (const id of ["catalog-agent", "catalog-skill", "catalog-team", "catalog-channel", "catalog-workflow"]) {
    assert.ok(SETTINGS_TABS.includes(id), `${id} is a Library panel`);
    assert.equal(settingsTab(id), id);
  }
  // These are quoted in hint strings and bookmarks; renaming one silently sends
  // every old link to Identity.
  // The Project IDE's own panel leads its group: the default mode is the
  // first thing to set before the editor's dials.
  assert.ok(SETTINGS_TABS.indexOf("ide") < SETTINGS_TABS.indexOf("editor"), "IDE before Editor");
  assert.ok(SETTINGS_TABS.indexOf("workstreams") < SETTINGS_TABS.indexOf("board"), "Board follows Workstreams, whose keys it shows");
  // The browser is a capability of the desktop app, read after the network it reaches through.
  assert.equal(SETTINGS_TABS.indexOf("browser"), SETTINGS_TABS.indexOf("network") + 1, "Browser follows Network");
  assert.equal(SETTINGS_TABS.indexOf("addons"), SETTINGS_TABS.indexOf("catalog-workflow") + 1, "Addons closes the Library group");
  for (const id of ["ide", "board", "skills", "mcp", "harnesses", "system", "desktop", "events", "goals", "governance", "workflow", "cache"]) {
    assert.equal(settingsTab(id), id);
  }
  // Automation reads in the order a goal moves: what wakes it, how it moves, what it runs.
  assert.equal(SETTINGS_TABS.includes("conversations"), false, "a conversation has no setting: the harness keeps its own context");
  assert.equal(SETTINGS_TABS.indexOf("goals"), SETTINGS_TABS.indexOf("events") + 1, "Goals follows Events");
  assert.equal(SETTINGS_TABS.includes("triggers"), false, "the Triggers panel is Events now");
  assert.equal(settingsTab("triggers"), SETTINGS_TABS[0], "an old link to it falls back like any unknown panel");
  assert.ok(SETTINGS_TABS.indexOf("goals") < SETTINGS_TABS.indexOf("workflow"), "Workflows follows Goals");
  // What a run may spend closes Automation: a goal's and a run of the workspace's alike.
  assert.equal(SETTINGS_TABS.indexOf("budgets"), SETTINGS_TABS.indexOf("workflow") + 1, "Budgets follows Workflows");
  assert.equal(settingsTab("budgets"), "budgets");
  // Desktop — the app's own behaviour on this machine — sits beside System, the machine's grants.
  assert.equal(SETTINGS_TABS.indexOf("desktop"), SETTINGS_TABS.indexOf("system") + 1, "Desktop follows System");
  // Network — this Mac's network read as facts, and the proxy the platform follows — closes Capabilities.
  assert.equal(SETTINGS_TABS.indexOf("network"), SETTINGS_TABS.indexOf("desktop") + 1, "Network follows Desktop");
  assert.equal(settingsTab("network"), "network");
  // The Git & code hosts group: Identity keeps the old `git` id so every link into
  // it still lands; SSH keys and GitHub are its two new panels.
  for (const id of ["git", "git-ssh", "github", "gitlab", "bitbucket"]) {
    assert.equal(settingsTab(id), id);
  }
  const order = ["git", "git-ssh", "github", "gitlab", "bitbucket"].map((id) => SETTINGS_TABS.indexOf(id));
  assert.ok(order.every((at, i) => i === 0 || order[i - 1] < at), `rail order: Identity, SSH keys, then one panel per code host — ${order}`);
});

test("a link into a panel is just its tab; an empty extra is dropped", () => {
  assert.deepEqual(settingsSearch("catalog-skill"), { tab: "catalog-skill" });
  // The compose path stays generic for any future per-panel key.
  assert.deepEqual(settingsSearch("git", { view: "changes" }), { tab: "git", view: "changes" });
  assert.deepEqual(settingsSearch("git", { view: "" }), { tab: "git" });
  assert.deepEqual(settingsSearch("git", { view: null }), { tab: "git" });
});

test("Decision Making follows the Security group, and a link to the retired tab falls back like any unknown one", () => {
  assert.equal(settingsTab("decision-making"), "decision-making");
  assert.equal(SETTINGS_TABS.indexOf("decision-making"), SETTINGS_TABS.indexOf("security-classifier") + 1);
  assert.equal(SETTINGS_TABS.includes("decisions"), false, "the panel has one id, the new one");
  assert.equal(settingsTab("decisions"), SETTINGS_TABS[0], "no such panel any more");
});

test("People replaced Members beside Relays & sync, and a link to the old id falls back", () => {
  assert.equal(settingsTab("people"), "people");
  assert.equal(SETTINGS_TABS.indexOf("sync"), SETTINGS_TABS.indexOf("people") + 1, "Relays & sync follows People");
  assert.equal(settingsTab("members"), SETTINGS_TABS[0], "no such panel any more");
  assert.deepEqual(settingsSearch("people", { join: "bisa://join/x/y" }), { tab: "people", join: "bisa://join/x/y" });
});
