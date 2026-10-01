/**
 * The footer's Browser read-out: one number for every tab, the bar, the
 * three dimensions' rows and doors, the footnote. Run with
 * `node --test desktop/src/shell/browserStatModel.test.mjs`.
 */
import { readFileSync } from "node:fs";
import { test } from "node:test";
import assert from "node:assert/strict";

import { browserPlaces } from "./browserPlacesModel.mjs";
import { DIMENSIONS, chosenDimension, dimensionIcon, dimensionLabel, emptyWords, footnote, overlayRows, statWords, visibilityBar } from "./browserStatModel.mjs";

const places = browserPlaces(
  {
    goals: [{ id: "g1", title: "Ship the storefront", statement: "x" }],
    projects: [{ project: { id: "p1", slug: "bisa", name: "Bisa" } }],
    workstreams: [{ workstream: { id: "w1", project: "p1", name: null, kind: { kind: "worktree", branch: "feat/a" } }, project_name: "Bisa" }],
    channels: [{ channel: { id: "c1", name: "general" } }],
    dms: [],
    inbox: [],
  },
  [],
);
const tab = (key, home, over = {}) => ({ key, home, url: `https://example.com/${key}`, title: "", loading: false, canBack: false, canForward: false, headless: false, pane: null, ...over });
const pricing = tab("b1", { scope: "goal", id: "g1" }, { title: "Pricing" });
const docs = tab("b2", { scope: "workstream", id: "w1" }, { title: "Docs" });
const scout = tab("b3", { scope: "goal", id: "g1" }, { title: "Scout", headless: true });
const home = tab("b4", null, { title: "Home" });
const tabs = [pricing, docs, scout, home];

test("every dimension has a word and a glyph, and each glyph is an entry of the icon registry", () => {
  const icons = readFileSync(new URL("../ui/icons.ts", import.meta.url), "utf8");
  assert.deepEqual([...DIMENSIONS], ["tabs", "origins", "unseen"]);
  const seen = new Set();
  for (const d of DIMENSIONS) {
    assert.notEqual(dimensionLabel(d), d, `${d} has a word`);
    const key = dimensionIcon(d);
    assert.match(icons, new RegExp(`^  ${key}: `, "m"), `${d} draws from ICON.${key}`);
    seen.add(key);
  }
  assert.equal(seen.size, 3, "three dimensions, three glyphs");
  assert.equal(dimensionIcon("unseen"), "hidden", "the eye glyph names the dimension, and nothing on the bar");
  assert.equal(dimensionIcon("tabs"), "page");
});

test("every dimension's word is one short word, the row the overlay is sized for", () => {
  for (const d of DIMENSIONS) {
    const word = dimensionLabel(d);
    assert.doesNotMatch(word, /\s/, `${d} is one word`);
    assert.ok(word.length <= 9, `${d}'s word "${word}" fits a segment`);
  }
  assert.equal(dimensionLabel("sideways"), "sideways", "a word off the list reads as itself");
});

test("the chosen dimension is the remembered one while the control offers it, else Tabs", () => {
  assert.equal(chosenDimension("origins"), "origins");
  assert.equal(chosenDimension("unseen"), "unseen");
  assert.equal(chosenDimension(null), "tabs");
  assert.equal(chosenDimension("platform"), "tabs", "a resource's dimension is not one of ours");
});

test("the read-out counts every tab, in sight or not; its title is the footer's sentence; the dot's words while an agent browses; pressed while a tab is on screen", () => {
  assert.deepEqual(statWords({ sessions: [], busy: [], shown: null }), { value: "0", title: "No browser tab is open — New tab here, or ⌘⇧L", live: null, pressed: false });
  const words = statWords({ sessions: tabs, busy: ["b3"], shown: "b1" });
  assert.equal(words.value, "4", "four tabs, one of them out of sight — one number");
  assert.equal(words.title, "4 browser tabs — 1 out of sight, an agent browsing in 1; showing Pricing");
  assert.equal(words.live, "An agent is browsing in 1 tab");
  assert.equal(words.pressed, true);
  const quiet = statWords({ sessions: tabs, busy: [], shown: "b9" });
  assert.equal(quiet.title, "4 browser tabs — 1 out of sight", "a shown key the store lacks shows nothing");
  assert.equal(quiet.live, null);
  assert.equal(quiet.pressed, false);
});

test("the bar is the tabs in sight then the ones out of sight over every tab, an empty part left out, and nothing for no tab", () => {
  assert.deepEqual(visibilityBar([]), []);
  assert.deepEqual(visibilityBar(tabs), [
    { key: "seen", label: "in sight", percent: 75, tone: "accent" },
    { key: "unseen", label: "out of sight", percent: 25, tone: "quiet" },
  ]);
  assert.deepEqual(
    visibilityBar([pricing, docs]).map((s) => [s.key, s.percent]),
    [["seen", 100]],
    "every tab in sight: one part",
  );
  assert.deepEqual(
    visibilityBar([scout]).map((s) => [s.key, s.percent]),
    [["unseen", 100]],
  );
});

test("Tabs: one row per tab in the order opened, at home by name, the one on screen current, one out of sight dim and worded unseen, a busy one marked, every row a door and closable", () => {
  const rows = overlayRows("tabs", { sessions: tabs, places, busy: ["b3"], shown: "b1" });
  assert.deepEqual(
    rows.map((r) => [r.key, r.label, r.sub, r.value]),
    [
      ["b1", "Pricing", "Goal · Ship the storefront", "on screen"],
      ["b2", "Docs", "Project IDE · Bisa › feat/a", ""],
      ["b3", "Scout", "Goal · Ship the storefront — an agent is browsing", "unseen"],
      ["b4", "Home", "Workspace", ""],
    ],
  );
  assert.deepEqual(rows.map((r) => r.current), [true, false, false, false]);
  assert.deepEqual(rows.map((r) => r.dim), [false, false, true, false]);
  assert.deepEqual(rows.map((r) => r.busy), [false, false, true, false]);
  assert.ok(rows.every((r) => r.percent === null), "a tab is no share of anything: no bar");
  assert.ok(rows.every((r) => r.door && r.door.kind === "tab" && r.door.key === r.key), "every row is a door to the pane on its tab");
  assert.ok(rows.every((r) => r.close === r.key), "every row closes its own tab — the one place a tab out of sight can be closed without showing it");
  assert.match(rows[2].hint, /^unseen — an agent browses here out of sight/, "the visibility words ride as the row's title");
  assert.equal(rows[0].hint, null);
});

test("a number that surprises says whose it is: the sentence counts the agents' tabs, and a row says who opened it — the agent by name, a page, nothing for the person's own", () => {
  const mine = { ...pricing, by: { kind: "person" } };
  const agents = { ...scout, by: { kind: "agent", agent: "reviewer" } };
  const nameless = { ...docs, by: { kind: "agent", agent: null } };
  const popup = { ...home, by: { kind: "page", from: "b1" } };
  const all = [mine, nameless, agents, popup];
  const words = statWords({ sessions: all, busy: [], shown: null });
  assert.equal(words.value, "4", "the number stays every open tab");
  assert.equal(words.title, "4 browser tabs — 2 opened by agents, 1 out of sight");
  assert.equal(statWords({ sessions: [mine, popup], busy: [], shown: null }).title, "2 browser tabs", "no agent's tab: nothing said of agents");
  const names = (id) => (id === "reviewer" ? "Reviewer" : null);
  const rows = overlayRows("tabs", { sessions: all, places, busy: ["b3"], shown: null, agentName: names });
  assert.deepEqual(
    rows.map((r) => [r.key, r.sub]),
    [
      ["b1", "Goal · Ship the storefront"],
      ["b2", "Project IDE · Bisa › feat/a · opened by an agent"],
      ["b3", "Goal · Ship the storefront — an agent is browsing · opened by Reviewer"],
      ["b4", "Workspace · opened by a page"],
    ],
  );
  // Without a roster to name it, the agent is said by its id — never by nothing.
  assert.equal(overlayRows("unseen", { sessions: all, places, busy: [], shown: null })[0].sub, "Goal · Ship the storefront · opened by reviewer");
});

test("Origins: one row per origin in the list's order, empty ones left out, the count and the asides in the sub, the bar its share of the largest, the door the first tab in sight and none when every tab is out of sight", () => {
  const rows = overlayRows("origins", { sessions: tabs, places, busy: ["b3"], shown: "b1" });
  assert.deepEqual(
    rows.map((r) => [r.key, r.label, r.sub, r.value, r.percent]),
    [
      ["origin:ide", "Project IDE", "1 tab", "1", 50],
      ["origin:goal", "Goal", "2 tabs · 1 out of sight · an agent browsing in 1", "2", 100],
      ["origin:workspace", "Workspace", "1 tab", "1", 50],
    ],
  );
  assert.deepEqual(rows.map((r) => r.door), [{ kind: "tab", key: "b2" }, { kind: "tab", key: "b1" }, { kind: "tab", key: "b4" }]);
  assert.ok(rows.every((r) => r.close === null && r.hint === null), "an origin closes nothing");
  assert.deepEqual(rows.map((r) => r.current), [false, true, false], "the origin holding the tab on screen is current");
  assert.deepEqual(rows.map((r) => r.busy), [false, true, false]);
  const dark = overlayRows("origins", { sessions: [scout], places, busy: [], shown: null });
  assert.equal(dark.length, 1);
  assert.equal(dark[0].door, null, "no tab in sight: nowhere to go without showing one");
  assert.equal(dark[0].dim, true);
  assert.equal(dark[0].sub, "1 tab · 1 out of sight");
  assert.deepEqual(overlayRows("origins", { sessions: [], places, busy: [], shown: null }), []);
});

test("Unseen: the tabs kept out of sight alone, each with the visibility words as its hint", () => {
  const rows = overlayRows("unseen", { sessions: tabs, places, busy: [], shown: null });
  assert.deepEqual(
    rows.map((r) => [r.key, r.label, r.value, r.dim]),
    [["b3", "Scout", "unseen", true]],
  );
  assert.deepEqual(rows[0].door, { kind: "tab", key: "b3" }, "its door shows it");
  assert.match(rows[0].hint, /click to show it$/);
  assert.deepEqual(overlayRows("unseen", { sessions: [pricing, docs], places, busy: [], shown: null }), []);
});

test("the empty words say no tab at all, every tab in sight, or nothing yet", () => {
  assert.equal(emptyWords("tabs", 0), "No browser tab is open — New tab here, or ⌘⇧L.");
  assert.equal(emptyWords("unseen", 0), "No browser tab is open — New tab here, or ⌘⇧L.");
  assert.equal(emptyWords("unseen", 3), "Every tab is in sight.");
  assert.equal(emptyWords("origins", 3), "Nothing to show yet.");
});

test("the footnote says the out-of-sight policy in one sentence", () => {
  assert.equal(footnote("always"), "Every tab an agent opens is kept out of sight.");
  assert.equal(footnote("never"), "Every tab an agent opens is shown beside you.");
  assert.match(footnote("unattended"), /^An agent's tab is kept out of sight in a goal in auto mode/);
  assert.equal(footnote("sideways"), footnote("unattended"), "a word off the list reads as the default");
});
