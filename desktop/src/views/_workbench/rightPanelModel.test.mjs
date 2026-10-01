import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import {
  ABOUT_VIEW_LABEL,
  ABOUT_VIEWS,
  aboutBody,
  availableOccupants,
  CHANGES_LAYOUT_LABEL,
  CHANGES_LAYOUTS,
  DEFAULT_VIEWS,
  GIT_VIEW_LABEL,
  GIT_VIEWS,
  isOccupant,
  isViewOf,
  OCCUPANT_COMMAND,
  OCCUPANT_LABEL,
  OCCUPANTS,
  parseRememberedTabs,
  parseViews,
  pressOccupant,
  RAIL_GROUPS,
  railGroups,
  OCCUPANT_SCROLL,
  occupantScroll,
  recallFor,
  REMOTE_LAYOUT_LABEL,
  REMOTE_LAYOUTS,
  resolveOccupant,
  CHANGE_FILTERS,
  CHANGE_FILTER_LABEL,
  } from "./rightPanelModel.mjs";
import { COMMANDS } from "../../shell/keymapModel.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));

test("the rail is five occupants in two groups — Files · Git | Workstreams · Agent · About — each with a label and a chord command; no Pull request occupant: the lifecycle is the Workstreams panel's", () => {
  assert.deepEqual(RAIL_GROUPS.map((g) => [...g]), [["files", "git"], ["workstreams", "agents", "about"]]);
  assert.deepEqual([...OCCUPANTS], ["files", "git", "workstreams", "agents", "about"], "the rail's order is the occupants' order");
  assert.ok(!isOccupant("triggers") && !("triggers" in OCCUPANT_LABEL) && !("triggers" in OCCUPANT_COMMAND), "no Triggers occupant");
  assert.equal(OCCUPANT_LABEL.workstreams, "Workstreams");
  assert.equal(OCCUPANT_LABEL.agents, "Agent", "the tab is the conversation with the agent; one word");
  assert.ok(!("pr" in OCCUPANT_LABEL) && !isOccupant("pr"), "the pull request is not an occupant");
  assert.equal(new Set(OCCUPANTS).size, OCCUPANTS.length, "disjoint");
  for (const o of OCCUPANTS) assert.ok(OCCUPANT_LABEL[o], `${o} has a label`);
  assert.deepEqual(Object.keys(OCCUPANT_LABEL).sort(), [...OCCUPANTS].sort());
  const ids = new Set(COMMANDS.map((c) => c.id));
  for (const o of OCCUPANTS) assert.ok(ids.has(OCCUPANT_COMMAND[o]), `${o}'s command ${OCCUPANT_COMMAND[o]} is a keymap command`);
});

test("the old chrome is gone: no header buttons, no bar occupants, no strip tabs, no third closable tab, no list body for Workstreams", () => {
  const src = readFileSync(join(HERE, "rightPanelModel.mjs"), "utf8");
  for (const word of ["STRIP_TABS", "HEADER_OCCUPANTS", "stripTabs", "dismissOccupant", "chromeFor", "isStripTab", "workstreamsBody", "branchBound", "panel_pr", "\"pr\"", "BAR_OCCUPANTS", "barOccupants", "isRailOccupant", "PanelDoors", "BoardDoor", "boardWords"]) {
    assert.ok(!src.includes(word), `${word} left the vocabulary`);
  }
});

test("availability: a goal or a work item shows Files and About; a checkout with a project shows all five, the primary included — whether it has a branch to take to a base is the panel's to say", () => {
  assert.deepEqual(availableOccupants({ scope: "goal", hasProject: false }), ["files", "about"]);
  assert.deepEqual(availableOccupants({ scope: "work_item", hasProject: false }), ["files", "about"]);
  assert.deepEqual(availableOccupants({ scope: "workstream", hasProject: false }), ["files", "about"]);
  assert.deepEqual(availableOccupants({ scope: "workstream", hasProject: true }), [...OCCUPANTS], "a checkout with a project: all five");
  assert.deepEqual(
    availableOccupants({ scope: "workstream", hasProject: true, centre: "conversation" }),
    OCCUPANTS.filter((o) => o !== "agents"),
    "the centre already the conversation: everything but Agent",
  );
  assert.equal(resolveOccupant("agents", availableOccupants({ scope: "workstream", hasProject: true, centre: "conversation" })), "files", "a remembered Agent tab falls to Files there");
});

test("About is the project's identity on every checkout; a checkout's own facts are the Workstreams occupant's", () => {
  // The rule does not ask whether the checkout is the primary: any checkout
  // of a project shows that project — and Workstreams, likewise, shows the
  // checkout you stand in on every root, so it needs no body rule at all.
  assert.equal(aboutBody({ scope: "workstream", hasProject: true }), "project");
  assert.equal(aboutBody({ scope: "workstream", hasProject: false }), "none", "a checkout still resolving its project shows nothing yet");
  assert.equal(aboutBody({ scope: "work_item", hasProject: false }), "work_item");
  assert.equal(aboutBody({ scope: "goal", hasProject: false }), "goal");
});

test("the rail is the groups whole, in the model's order, on every root — availability mutes a tab, never drops it", () => {
  assert.deepEqual(railGroups(), [["files", "git"], ["workstreams", "agents", "about"]]);
  assert.notEqual(railGroups()[0], RAIL_GROUPS[0], "a copy the caller may hold");
  assert.deepEqual(railGroups().flat(), [...OCCUPANTS]);
});

test("an unavailable or unknown occupant resolves to Files", () => {
  const filesAbout = ["files", "about"];
  assert.equal(resolveOccupant("agents", filesAbout), "files");
  assert.equal(resolveOccupant("about", filesAbout), "about");
  assert.equal(resolveOccupant("nonsense", [...OCCUPANTS]), "files");
  assert.equal(resolveOccupant(undefined, [...OCCUPANTS]), "files");
  assert.equal(resolveOccupant("workstreams", [...OCCUPANTS]), "workstreams");
  assert.equal(resolveOccupant("triggers", [...OCCUPANTS]), "files", "a panel stored open on the retired Triggers occupant opens on Files");
});

test("a rail press opens, switches, or closes the column — for every occupant, never a fourth state", () => {
  for (const o of OCCUPANTS) {
    assert.deepEqual(pressOccupant({ open: false, tab: "files" }, o), { open: true, tab: o });
    assert.deepEqual(pressOccupant({ open: true, tab: o === "files" ? "git" : "files" }, o), { open: true, tab: o });
    assert.deepEqual(pressOccupant({ open: true, tab: o }, o), { open: false, tab: o });
  }
});

test("remembered tabs keep any occupant and drop anything else; nothing is mapped", () => {
  assert.deepEqual(parseRememberedTabs({ a: "about", b: "nonsense", c: 7, d: "workstreams", e: "git", f: "files", g: "pr" }), {
    a: "about",
    d: "workstreams",
    e: "git",
    f: "files",
  });
  assert.ok(!("g" in parseRememberedTabs({ g: "pr" })), "a root that remembered the retired Pull request occupant opens on Files, like any unknown word");
  assert.deepEqual(parseRememberedTabs({ "workstream:a": "triggers", "workstream:b": "agents" }), { "workstream:b": "agents" }, "a root that remembered the retired Triggers occupant is dropped, and opens on Files");
  assert.deepEqual(parseRememberedTabs([]), {});
  assert.deepEqual(parseRememberedTabs(null), {});
  assert.deepEqual(parseRememberedTabs("git"), {});
});

test("recalling a root: the same root is left alone; another root gets what it last had, or Files", () => {
  const byRoot = { "workstream:a": "agents" };
  assert.deepEqual(recallFor({ tab: "git", root: "workstream:a", byRoot }, "workstream:a"), { tab: "git", root: "workstream:a" });
  assert.deepEqual(recallFor({ tab: "git", root: "workstream:a", byRoot }, "workstream:b"), { tab: "files", root: "workstream:b" });
  assert.deepEqual(recallFor({ tab: "files", root: "workstream:b", byRoot }, "workstream:a"), { tab: "agents", root: "workstream:a" });
  assert.deepEqual(recallFor({ tab: "files", root: null, byRoot }, "workstream:a"), { tab: "agents", root: "workstream:a" });
});

test("the Git views are what a person does to the tree — Changes, Branches, History, Stashes — and never a setting; About's are Project, Checkout and Settings; none is an occupant", () => {
  assert.deepEqual([...GIT_VIEWS], ["changes", "branches", "history", "stashes"]);
  for (const v of GIT_VIEWS) assert.ok(GIT_VIEW_LABEL[v], `${v} has a label`);
  assert.deepEqual([...ABOUT_VIEWS], ["project", "checkout", "settings"], "the project, this checkout's repository, the project's saved settings");
  for (const v of ABOUT_VIEWS) assert.ok(ABOUT_VIEW_LABEL[v], `${v} has a label`);
  assert.ok(isOccupant("workstreams"));
  for (const v of [...GIT_VIEWS, ...ABOUT_VIEWS]) assert.ok(!isOccupant(v), `${v} is a view, not an occupant`);
  assert.ok(isViewOf("git", "history") && isViewOf("about", "project"));
  assert.ok(!isViewOf("about", "history") && !isViewOf("files", "changes"));
});

test("the remembered choices: a valid set is kept, an unknown word or a missing one falls to its default, and nothing throws", () => {
  assert.deepEqual(parseViews({ git: "stashes", about: "settings", changes: "list", changesFilter: "staged", remotes: "list", patch: "split" }), { git: "stashes", about: "settings", changes: "list", changesFilter: "staged", remotes: "list", patch: "split" });
  assert.deepEqual(parseViews({ git: "history" }), { git: "history", about: "project", changes: "tree", changesFilter: "all", remotes: "tree", patch: "hunks" });
  assert.equal(parseViews({ patch: "inline" }).patch, "inline", "a patch's view is one choice for every patch");
  assert.equal(parseViews({ patch: "monaco" }).patch, "hunks", "a view off the list opens on the hunks");
  assert.deepEqual(parseViews({ git: "settings", about: "changes", changes: "nonsense" }), DEFAULT_VIEWS, "each choice takes only its own words");
  assert.deepEqual(parseViews(null), DEFAULT_VIEWS);
  assert.deepEqual(parseViews("changes"), DEFAULT_VIEWS);
  assert.deepEqual(parseViews([]), DEFAULT_VIEWS);
  assert.deepEqual(DEFAULT_VIEWS, { git: "changes", about: "project", changes: "tree", changesFilter: "all", remotes: "tree", patch: "hunks" }, "the Changes view and a remote's branches open as a tree, the Changes view on every file, and a patch on its hunks");
  // The Changes filter: seven words, All first, remembered beside the layout and no occupant's door.
  assert.deepEqual([...CHANGE_FILTERS], ["all", "conflicted", "staged", "unstaged", "tracked", "untracked", "modified"]);
  for (const f of CHANGE_FILTERS) assert.ok(CHANGE_FILTER_LABEL[f], `${f} has a label`);
  assert.ok(isViewOf("changesFilter", "untracked") && !isViewOf("changesFilter", "tree"), "a filter word is not a layout");
  assert.equal(parseViews({ changesFilter: "everything" }).changesFilter, "all", "an unknown filter word is All");
  assert.deepEqual([...REMOTE_LAYOUTS], ["tree", "list"]);
  for (const l of REMOTE_LAYOUTS) assert.ok(REMOTE_LAYOUT_LABEL[l], `${l} has a label`);
  assert.ok(isViewOf("remotes", "list") && !isViewOf("remotes", "branches"), "a remote layout is its own choice");
  assert.deepEqual([...CHANGES_LAYOUTS], ["tree", "list"]);
  for (const l of CHANGES_LAYOUTS) assert.ok(CHANGES_LAYOUT_LABEL[l], `${l} has a label`);
  assert.ok(isViewOf("changes", "tree") && !isViewOf("changes", "changes"), "a layout is not a view and a view is not a layout");
});

test("the panel body is a scrollport only for the occupants that scroll as one; the explorer and the Agent pane scroll inside their own box", () => {
  // Every occupant has a word, so a new one cannot land on the wrong default by omission.
  for (const o of OCCUPANTS) assert.ok(["own", "panel"].includes(OCCUPANT_SCROLL[o]), `${o} says whose scrollport the body is`);
  assert.equal(occupantScroll("files"), "own", "the tree's VirtualList is the one scrollport, however deep a folder is opened");
  assert.equal(occupantScroll("agents"), "own");
  assert.equal(occupantScroll("git"), "panel", "the Changes tree is unbounded under a sticky composer: the panel scrolls as one");
  for (const o of ["about", "workstreams"]) assert.equal(occupantScroll(o), "panel", `${o} is a PanelBody, which the body scrolls`);
  assert.equal(occupantScroll("nonsense"), "panel", "an unknown occupant scrolls the safe way — nothing is ever clipped by mistake");
});
