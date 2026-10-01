import test from "node:test";
import assert from "node:assert/strict";
import { barTitle, grantToggle, isGranted, offerRows, originWords, overlayRows, permissionWords, problemLines, pruneHidden, reviewWords, rowStateWords, sameAddon, slotOf, sortAddons, sortedPermissions, stateWords, switchWords, titleWords, visibleAddons, withEnabled } from "./addonsModel.mjs";

const addon = (id, over = {}) => ({ id, manifest: { name: id, version: "1.0.0", license: "MIT", permissions: [], window: { width: 1, height: 1 } }, origin: "local", enabled: true, granted: [], installed_at: 0, files_present: true, active: true, ...over });

test("the words say how many show, and what state each addon is in", () => {
  assert.equal(titleWords(0, 0), "No addons");
  assert.equal(titleWords(2, 3), "2 of 3 addons showing");
  assert.equal(switchWords(true).label, "Show addons");
  assert.notEqual(switchWords(true).hint, switchWords(false).hint);
  assert.equal(stateWords({ active: true, files_present: true }), "running");
  assert.equal(stateWords({ active: false, files_present: true }), "off");
  assert.equal(stateWords({ active: false, files_present: false }), "files not on this machine");
  assert.equal(originWords({ catalog: { slug: "clock" } }), "built in");
  assert.equal(originWords("local"), "imported");
});

test("every permission has plain words, and network names its hosts", () => {
  const net = { network: { hosts: ["api.example.com", "*.example.org"] } };
  assert.match(permissionWords(net), /api\.example\.com, \*\.example\.org/);
  for (const word of ["platform_info", "theme", "system_load", "workspace_summary", "notify", "clipboard_write", "storage", "open_url", "navigate"]) {
    const words = permissionWords(word);
    assert.ok(words.length > 8 && !words.includes("addons-"), `${word}: ${words}`);
  }
  assert.equal(permissionWords("bogus"), permissionWords(42));
  assert.deepEqual(sortedPermissions(["navigate", net, "theme"]), ["theme", net, "navigate"]);
});

test("a grant toggles exactly the declared permission, never a widened one", () => {
  const net = { network: { hosts: ["api.example.com"] } };
  const declared = ["storage", net, "notify"];
  assert.deepEqual(grantToggle([], declared, "storage", true), ["storage"]);
  assert.deepEqual(grantToggle(["storage"], declared, { network: { hosts: ["evil.example"] } }, true), ["storage", net], "the declaration is what is granted");
  assert.deepEqual(grantToggle(["storage", net], declared, net, false), ["storage"]);
  assert.deepEqual(grantToggle(["storage"], declared, "clipboard_write", true), ["storage"], "undeclared: nothing added");
  assert.equal(isGranted(["storage", net], { network: { hosts: ["other"] } }), true, "granted by word");
  assert.equal(isGranted(["storage"], "notify"), false);
});

test("the review names the addon, its version and licence, and one line per permission", () => {
  const r = reviewWords({ name: "Weather", version: "1.0.0", license: "MIT", permissions: [{ network: { hosts: ["api.open-meteo.com"] } }, "storage"] });
  assert.match(r.title, /Weather/);
  assert.match(r.version, /1\.0\.0/);
  assert.match(r.version, /MIT/);
  assert.equal(r.lines.length, 2);
  assert.match(r.lead, /2/);
  const none = reviewWords({ name: "Calculator", version: "1.0.0", license: "MIT", permissions: [] });
  assert.equal(none.lines.length, 0);
  assert.match(none.lead, /nothing/);
});

test("problems read as field: sentence; lists sort running first, then by name", () => {
  const tx = (text) => `<${text.id}>`;
  assert.deepEqual(problemLines([{ field: "name", text: { id: "problem-addon-needs-name" } }, { text: { id: "x" } }], tx), ["name: <problem-addon-needs-name>", "<x>"]);
  const sorted = sortAddons([addon("zeta"), addon("alpha", { active: false }), addon("beta")]);
  assert.deepEqual(sorted.map((a) => a.id), ["beta", "zeta", "alpha"]);
  const rows = offerRows([
    { slug: "b", manifest: { name: "B" }, installed: true },
    { slug: "z", manifest: { name: "Zeta" }, installed: false },
    { slug: "a", manifest: { name: "Alpha" }, installed: false },
  ]);
  assert.deepEqual(rows.map((r) => r.slug), ["a", "z"], "installed ones are left out; the rest by name");
});

test("the layer draws the active windows not put away, and nothing when switched off — the one rule the footer counts by", () => {
  const addons = [addon("a"), addon("b", { active: false }), addon("c")];
  assert.deepEqual(visibleAddons(addons, ["c"], true, true).map((a) => a.id), ["a"]);
  assert.deepEqual(visibleAddons(addons, [], false, true), []);
  assert.deepEqual(visibleAddons(addons, [], true, false), []);
  assert.equal(titleWords(visibleAddons(addons, ["c"], true, true).length, addons.length), "1 of 3 addons showing", "installed-but-off addons still count as installed");
  assert.equal(barTitle(addon("clock"), null), "clock");
  assert.equal(barTitle(addon("clock"), "  "), "clock");
  assert.equal(barTitle(addon("clock"), "Lisbon"), "Lisbon");
});

test("the popover lists every installed addon — running first — with what shows, what is put away and what may be switched on", () => {
  const addons = [addon("zeta"), addon("alpha", { enabled: false, active: false }), addon("beta"), addon("gone", { enabled: false, active: false, files_present: false })];
  const rows = overlayRows(addons, ["beta"], true, true);
  assert.deepEqual(rows.map((r) => r.addon.id), ["beta", "zeta", "alpha", "gone"], "every installed addon, running first, then by name");
  assert.deepEqual(rows.map((r) => [r.running, r.shown, r.putAway, r.canEnable]), [
    [true, false, true, true],
    [true, true, false, true],
    [false, false, false, true],
    [false, false, false, false],
  ]);
  assert.deepEqual(rows.map(rowStateWords), ["put away", "running", "off", "files not on this machine"]);
  assert.ok(overlayRows(addons, [], false, true).every((r) => !r.shown), "the layer off: nothing shows, every row stays");
  assert.ok(overlayRows(addons, [], true, false).every((r) => !r.shown), "the machine's switch off: the same");
});

test("a switch reads as the node will read it, an unknown id changes nothing, and the put-away list forgets what left", () => {
  const addons = [addon("a"), addon("b", { enabled: false, active: false }), addon("c", { enabled: false, active: false, files_present: false })];
  const off = withEnabled(addons, "a", false);
  assert.deepEqual(off.map((a) => [a.id, a.enabled, a.active]), [["a", false, false], ["b", false, false], ["c", false, false]]);
  const on = withEnabled(addons, "b", true);
  assert.deepEqual(on.map((a) => [a.id, a.enabled, a.active]), [["a", true, true], ["b", true, true], ["c", false, false]]);
  assert.equal(withEnabled(addons, "c", true)[2].active, false, "enabled without its files here is not active — as the node computes it");
  assert.equal(withEnabled(addons, "nope", true), addons, "an unknown id: the same list, by reference");
  assert.equal(addons[1].enabled, false, "the list given is never changed in place");
  const hidden = ["a", "zombie", "c"];
  assert.deepEqual(pruneHidden(hidden, addons), ["a", "c"]);
  const same = ["a"];
  assert.equal(pruneHidden(same, addons), same, "nothing to prune: the same list, by reference");
});

test("two readings of one addon are the same by their facts, and a window's slot is its place among every installed addon by id", () => {
  const a = addon("a", { granted: ["storage"] });
  assert.equal(sameAddon(a, { ...a }), true);
  assert.equal(sameAddon(a, { ...a, enabled: false }), false);
  assert.equal(sameAddon(a, { ...a, granted: [] }), false);
  assert.equal(sameAddon(a, { ...a, installed_at: 9 }), false);
  assert.equal(sameAddon(a, { ...a, manifest: { ...a.manifest, name: "renamed" } }), true, "the manifest's words are not what a window holds");
  const addons = [addon("clock"), addon("cpu"), addon("calculator")];
  assert.equal(slotOf(addons, "calculator"), 0);
  assert.equal(slotOf(addons, "clock"), 1);
  assert.equal(slotOf(addons, "cpu"), 2);
  assert.equal(slotOf(addons.filter((x) => x.id !== "clock"), "cpu"), 1, "another addon gone moves the slot; showing or hiding one never does");
  assert.equal(slotOf(addons, "nope"), 0);
});

test("a switch the node refused goes back alone, on the list as it stands — and the list's reads land in the order they were asked", async () => {
  const { readFileSync } = await import("node:fs");
  // Clock was switched off; while the PATCH was out, a re-read landed: the weather addon is off now too.
  const before = [addon("clock"), addon("weather")];
  const optimistic = withEnabled(before, "clock", false);
  const reread = withEnabled(optimistic, "weather", false);
  // The node refuses: the clock's switch goes back; the weather's stays as the node said.
  const back = withEnabled(reread, "clock", true);
  assert.deepEqual(back.map((a) => [a.id, a.enabled, a.active]), [["clock", true, true], ["weather", false, false]], "the list from before the switch would have put the weather back on");
  const store = readFileSync(new URL("./addonsStore.ts", import.meta.url), "utf8");
  assert.ok(store.includes("if (was !== undefined) set({ ...state, addons: withEnabled(state.addons, id, was) });"));
  assert.ok(!store.includes("set({ ...state, addons: before });"), "never the whole list as it was");
  // Of two reads out, the newest asked for lands; one that refused is said and leaves `loaded` as it was.
  const refresh = store.slice(store.indexOf("export async function refreshAddons"), store.indexOf("/** Show or hide every window at once"));
  assert.ok(refresh.includes("const ticket = reads.begin();") && refresh.split("reads.lands(ticket)").length - 1 === 2, "the answer and the refusal are both held to the ticket");
  assert.ok(refresh.includes("set({ ...state, failed: e instanceof Error ? e.message : String(e) });") && !refresh.includes("set({ ...state, loaded: true });"), "a read that refused is not a list that was read");
  for (const surface of ["../views/_settings/AddonsPanel.tsx", "../shell/AddonsOverlay.tsx"]) assert.ok(readFileSync(new URL(surface, import.meta.url), "utf8").includes("{failed && "), `${surface} says the refusal`);
});
