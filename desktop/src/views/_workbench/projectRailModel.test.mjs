import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { foldGroups } from "../../ui/tabsFitModel.mjs";
import { ARCHIVED_NO_WORKSTREAM, RAIL_TABS, RAIL_TAB_FOLD, RAIL_TAB_ICON, RAIL_TAB_LABEL, collapseKey, groupNames, isRailTab, newWorkstreamOffer, projectOf, railCounts, railRows, tabOf, importLanding, revealPlan, rowIndexOfProject, rowIndexOfWorkstream, sectionOf, treeRowsOf } from "./projectRailModel.mjs";

const P = (id, name, extra = {}) => ({
  project: { id, slug: name.toLowerCase(), name, tags: [], root: { type: "managed" }, vcs: { type: "git", default_branch: "main" }, publish: "gated", revision: 1, created_at: 0, origin: { origin: "workspace" }, ...extra },
  path: `/ws/${id}`,
  exists: true,
  workstreams: 1,
  goals: extra.goals ?? [],
});
const W = (id, project, kind, created_at = 1, name = null) => ({
  project_name: null,
  path: `/ws/${id}`,
  exists: true,
  workstream: { id, project, name, note: null, pinned: false, kind, goal: null, work_item: null, agent: null, state: { state: "open" }, created_at },
});
const primary = (pid, at = 0) => W(pid, pid, { kind: "primary" }, at);
const worktree = (id, pid, branch, at) => W(id, pid, { kind: "worktree", branch, base: "main" }, at);
const T = (key, id, harness = "claude-code", status = "live") => ({ key, scope: "workstream", id, harness, label: null, resume: false, generation: 0, liveness: { status, code: 0 }, restoring: false });
const S = (id, workstream, word = "running", extra = {}) => ({
  id, kind: "worker", state: word === "running" ? { state: "running", tool: "Read", args: "" } : word === "failed" ? { state: "failed", reason: "boom" } : { state: word },
  since: 1, harness: "claude-code", work_item: null, workstream, project: null, goal: null,
  cost: { input_tokens: 0, output_tokens: 0, usd_cents: 0 }, children: [], last_activity: 1, ...extra,
});

test("workspace view: projects by name, each primary first, then its branches, then their sessions", () => {
  const rows = railRows({
    projects: [P("p2", "Zeta"), P("p1", "Alpha")],
    workstreams: [worktree("w2", "p1", "feature/late", 9), primary("p1", 0), worktree("w1", "p1", "feature/early", 5), primary("p2")],
    terminals: [T("t1", "w1")],
    sessions: [S("s1", "p1")],
  });
  assert.deepEqual(
    rows.map((r) => `${r.kind}:${r.id}`),
    ["project:ungrouped:p1", "workstream:p1", "agent:s1", "workstream:w1", "terminal:t1", "workstream:w2", "project:ungrouped:p2", "workstream:p2"],
  );
  assert.equal(rows[1].primary, true);
  assert.equal(rows[1].label, "primary");
  assert.equal(rows[3].label, "feature/early");
  assert.equal(rows[0].depth, 0, "no group headers when nobody made a group");
  assert.equal(rows[1].depth, 1);
  assert.equal(rows[2].depth, 2);
});

test("groups sit by name with the ungrouped last, and a collapsed thing hides its subtree but keeps its count", () => {
  const rows = railRows({
    projects: [P("p1", "Alpha", { group: "Shop" }), P("p2", "Beta"), P("p3", "Gamma", { group: "Admin" })],
    workstreams: [primary("p1"), primary("p2"), primary("p3")],
    collapsed: new Set(["rail.group.Shop", "rail.project.p2.ungrouped"]),
  });
  assert.deepEqual(
    rows.map((r) => `${r.kind}:${r.id}`),
    ["group:Admin", "project:group:Admin:p3", "workstream:p3", "group:Shop", "group:", "project:ungrouped:p2"],
  );
  assert.equal(rows[3].collapsed, true);
  assert.equal(rows[3].count, 1);
  assert.equal(rows[4].label, "Other projects");
  assert.deepEqual([rows[0].under, rows[3].under, rows[4].under], ["group:Admin", "group:Shop", "ungrouped"], "a heading says what it folds");
  assert.equal(rows[5].collapsed, true);
  assert.equal(rows[5].workstreams, 1, "the count survives the collapse");
  assert.deepEqual(groupNames([P("a", "A", { group: "Shop" }), P("b", "B", { group: " Admin " }), P("c", "C")]), ["Admin", "Shop"]);
});

const step = (workflow, goal = "g1") => ({ origin: "step", goal, run: "r1", step: "implement", workflow });
const fromGoal = (goal) => ({ origin: "goal", goal });

/** One of each origin, plus attachments that must not turn into tabs. */
const MIXED = {
  projects: [
    P("p1", "Alpha", { goals: ["g1", "g2"] }),
    P("p2", "Beta", { origin: fromGoal("g2"), goals: ["g1"] }),
    P("p3", "Gamma", { origin: fromGoal("g1") }),
    P("p4", "Delta", { origin: step("wf1") }),
    P("p5", "Epsilon", { origin: step("wf2", "g2") }),
  ],
  workstreams: [primary("p1"), primary("p2"), primary("p3"), primary("p4"), primary("p5")],
  goals: [{ id: "g1", label: "Dark mode" }, { id: "g2", label: "Analytics" }],
  workflows: [{ id: "wf1", label: "Bug fix" }, { id: "wf2", label: "Audit" }],
};

test("the tabs are the three origins: one rule says where a project sits, and the strip is built from it", () => {
  assert.deepEqual([...RAIL_TABS], ["workspace", "goals", "workflows"]);
  for (const t of RAIL_TABS) assert.ok(RAIL_TAB_LABEL[t], `${t} has a label`);
  const icons = readFileSync(new URL("../../ui/icons.ts", import.meta.url), "utf8");
  for (const t of RAIL_TABS) assert.match(icons, new RegExp(`^  ${RAIL_TAB_ICON[t]}: `, "m"), `${t} wears a glyph that exists`);
  assert.equal(new Set(Object.values(RAIL_TAB_ICON)).size, RAIL_TABS.length, "each tab its own glyph");
  assert.equal(RAIL_TAB_ICON.workspace, "members", "the app's Workspace mark, the Pulse's");
  for (const t of RAIL_TABS) assert.equal(typeof RAIL_TAB_FOLD[t], "number", `${t} knows its turn to yield its word`);
  assert.equal(new Set(Object.values(RAIL_TAB_FOLD)).size, RAIL_TABS.length, "one tab at a time, never two together");
  assert.deepEqual(
    foldGroups(RAIL_TABS.map((id) => ({ id, icon: true, fold: RAIL_TAB_FOLD[id] }))),
    [["workflows"], ["goals"], ["workspace"]],
    "the strip's order reversed: Workflows folds first, Workspace keeps its word longest",
  );
  assert.equal(tabOf({ origin: { origin: "workspace" } }), "workspace");
  assert.equal(tabOf({ origin: fromGoal("g1") }), "goals");
  assert.equal(tabOf({ origin: step("wf1") }), "workflows");
  assert.equal(tabOf(null), "workspace", "no origin at all is a person's");
  assert.ok(isRailTab("goals") && !isRailTab("all") && !isRailTab(null));
});

test("each tab holds only its origin's projects: a hand-made project is in Workspace alone, whatever it is attached to", () => {
  const ids = (tab) => railRows({ ...MIXED, tab }).filter((r) => r.kind === "project").map((r) => r.project.id);
  assert.deepEqual(ids("workspace"), ["p1"], "attached to two goals, still made by hand");
  assert.deepEqual(ids("goals"), ["p2", "p3"]);
  assert.deepEqual(ids("workflows"), ["p5", "p4"], "sections are workflows by name: Audit (wf2) before Bug fix (wf1)");
  for (const tab of RAIL_TABS) {
    const rows = railRows({ ...MIXED, tab });
    assert.ok(!rows.some((r) => /unattached|by_hand|from_goal/i.test(String(r.id)) || /^(Unattached|Made by hand|Made from a goal)$/.test(r.label ?? "")), `${tab} has no catch-all section`);
  }
});

test("goals tab: a project sits under the goal that made it — once — and the sections are goals by name", () => {
  const rows = railRows({ ...MIXED, tab: "goals" });
  assert.deepEqual(
    rows.filter((r) => r.kind === "goal" || r.kind === "project").map((r) => `${r.kind}:${r.id}`),
    ["goal:g2", "project:goal:g2:p2", "goal:g1", "project:goal:g1:p3"],
  );
  assert.equal(rows[0].label, "Analytics", "goals by name");
  assert.deepEqual(rows[0].projects, ["p2"], "a header knows its members");
  assert.equal(collapseKey(rows[0]), "rail.goal.g2");
});

test("workflows tab: a project sits under the workflow whose step made it; the sections are workflows by name", () => {
  const rows = railRows({ ...MIXED, tab: "workflows" });
  assert.deepEqual(
    rows.filter((r) => r.kind === "group" || r.kind === "project").map((r) => `${r.kind}:${r.id}`),
    ["group:workflow:wf2", "project:workflow:wf2:p5", "group:workflow:wf1", "project:workflow:wf1:p4"],
  );
  assert.equal(rows[0].label, "Audit", "workflows by name, not by id");
  assert.deepEqual(rows[0].projects, ["p5"]);
  assert.equal(collapseKey(rows[0]), "rail.group.workflow:wf2");
});

test("a goal or a workflow that no longer exists still heads its section, and says so", () => {
  const goals = railRows({ tab: "goals", projects: [P("p1", "Alpha", { origin: fromGoal("01HGONE") })], workstreams: [primary("p1")], goals: [] });
  assert.equal(goals[0].kind, "goal");
  assert.equal(goals[0].label, "goal ‹1HGONE› (removed)", "the id's tail, and the fact");
  assert.equal(goals[1].id, "goal:01HGONE:p1", "the project is still in its tab");
  const wfs = railRows({ tab: "workflows", projects: [P("p1", "Alpha", { origin: step("gone") })], workstreams: [primary("p1")], workflows: [] });
  assert.deepEqual(wfs.map((r) => `${r.kind}:${r.id}`), ["group:workflow:gone", "project:workflow:gone:p1", "workstream:p1"]);
  assert.equal(wfs[0].label, "workflow ‹gone› (removed)");
});

test("the strip's one badge: projects per tab, and nothing else", () => {
  assert.deepEqual(railCounts({ projects: MIXED.projects }), { workspace: 1, goals: 2, workflows: 2 });
  assert.deepEqual(railCounts({ projects: [] }), { workspace: 0, goals: 0, workflows: 0 });
});

test("the filter keeps a match's ancestors and drops everything else", () => {
  const rows = railRows({
    projects: [P("p1", "Alpha", { group: "Shop" }), P("p2", "Beta", { group: "Shop" })],
    workstreams: [primary("p1"), worktree("w1", "p1", "feature/cart", 1), primary("p2")],
    filter: "cart",
  });
  assert.deepEqual(rows.map((r) => `${r.kind}:${r.id}`), ["group:Shop", "project:group:Shop:p1", "workstream:w1"]);
  const byName = railRows({ projects: [P("p1", "Alpha"), P("p2", "Beta")], workstreams: [primary("p1"), primary("p2")], filter: "bet" });
  assert.deepEqual(byName.map((r) => r.id), ["ungrouped:p2", "p2"], "a project match keeps all its workstreams");
});

test("the current root is marked on its workstream and rolled up to its project; order ignores attention", () => {
  const rows = railRows({
    projects: [P("p1", "Alpha"), P("p2", "Beta")],
    workstreams: [primary("p1"), primary("p2")],
    sessions: [S("s1", "p2", "failed")],
    current: { scope: "workstream", id: "p2" },
  });
  assert.equal(rows[0].id, "ungrouped:p1", "Alpha first even though Beta needs you");
  assert.equal(rows[2].current, true);
  assert.equal(rows[3].current, true);
  assert.equal(rows[2].activity.state, "failed");
  // A project row folds its workstreams' activity *with the counts*: the rail
  // reads `activity.counts.needsYou` on every row and crashed on a project
  // row that carried a bare word instead.
  assert.deepEqual(rows[2].activity.counts, { needsYou: 1, working: 0, done: 0, live: 0, agents: 1 });
  assert.deepEqual(rows[0].activity.counts, { needsYou: 0, working: 0, done: 0, live: 0, agents: 0 });
  assert.equal(collapseKey(rows[0]), "rail.project.p1.ungrouped");
  assert.equal(collapseKey(rows[1]), "rail.workstream.p1");
  assert.equal(collapseKey(rows[4] ?? { kind: "agent" }), null);
});

test("the child rows carry their timing anchors", () => {
  const term = { ...T("t1", "p1", null, "live"), openedAt: 1000, exitedAt: null };
  const rows = railRows({
    projects: [P("p1", "Alpha")],
    workstreams: [primary("p1")],
    terminals: [term],
    sessions: [S("s1", "p1", "running", { since: 1200, started: 900, children: [{ id: "c1", name: "explore", description: "map", state: { state: "thinking" }, since: 1210, started: 1205 }] })],
  });
  const t = rows.find((r) => r.kind === "terminal");
  assert.equal(t.openedAt, 1000);
  assert.equal(t.exitedAt, null);
  const agent = rows.find((r) => r.kind === "agent" && r.parent === null);
  assert.equal(agent.started, 900, "the session's registration instant");
  const sub = rows.find((r) => r.kind === "agent" && r.parent === "s1");
  assert.equal(sub.started, 1205, "a sub-agent counts from its spawn instant, never its state's");
});

test("a workstream row names the harnesses here and its shells", () => {
  const rows = railRows({
    projects: [P("p1", "Alpha")],
    workstreams: [primary("p1")],
    terminals: [T("t1", "p1", "claude-code", "live"), T("t2", "p1", null, "exited")],
    sessions: [S("s1", "p1", "running", { harness: "claude-code" }), S("s2", "p1", "thinking", { harness: "codex" })],
    collapsed: new Set(["rail.workstream.p1"]),
  });
  const ws = rows.find((r) => r.kind === "workstream");
  assert.deepEqual(ws.harnesses, ["claude-code", "codex"], "distinct, first seen first");
  assert.deepEqual(ws.shellGlyphs, [
    { harness: "claude-code", live: true },
    { harness: null, live: false },
  ]);
});

test("a closed workstream is not a row; a workstream row collapses its sessions", () => {
  const closed = worktree("w9", "p1", "old", 1);
  closed.workstream.state = { state: "closed" };
  const rows = railRows({
    projects: [P("p1", "Alpha")],
    workstreams: [primary("p1"), closed],
    terminals: [T("t1", "p1")],
    collapsed: new Set(["rail.workstream.p1"]),
  });
  assert.deepEqual(rows.map((r) => `${r.kind}:${r.id}`), ["project:ungrouped:p1", "workstream:p1"]);
  assert.equal(rows[1].sessions, 1);
});

test("an agent row carries the vocabulary's state and its sub-agents nest one level in", () => {
  const rows = railRows({
    projects: [P("p1", "Alpha")],
    workstreams: [primary("p1")],
    sessions: [
      S("s1", "p1", "running", {
        agent: "dev",
        children: [{ id: "t1", name: "explore", description: "map the crate", state: { state: "thinking" }, since: 2 }],
      }),
    ],
  });
  const agents = rows.filter((r) => r.kind === "agent");
  assert.equal(agents.length, 2);
  assert.equal(agents[0].label, "dev");
  assert.equal(agents[0].activity, "running Read");
  assert.equal(agents[0].parent, null);
  assert.equal(agents[1].label, "explore");
  assert.equal(agents[1].parent, "s1");
  assert.equal(agents[1].depth, agents[0].depth + 1);
  assert.equal(agents[1].activity, "map the crate");
});

test("tree rows are unique across kinds, labelled, and open when their key is not collapsed", () => {
  const rows = [
    { kind: "group", id: "", label: "Other projects", depth: 0, collapsed: false, count: 1, projects: ["p1"] },
    { kind: "project", id: "p1", project: { id: "p1", name: "Shop" }, under: "", depth: 1, collapsed: true, sessions: 0 },
    { kind: "workstream", id: "p1", workstream: { id: "p1" }, label: "main", depth: 2, collapsed: false, sessions: 2 },
    { kind: "workstream", id: "w2", workstream: { id: "w2" }, label: "feature", depth: 2, collapsed: false, sessions: 0 },
    { kind: "terminal", id: "t1", workstream: "w2", label: "shell", depth: 3 },
    { kind: "agent", id: "s1", workstream: "w2", label: "dev", parent: null, childCount: 1, depth: 3 },
    { kind: "agent", id: "s1:sub", workstream: "w2", label: "explore", parent: "s1", depth: 4 },
  ];
  const collapsed = new Set([collapseKey(rows[1]), collapseKey(rows[5])].filter(Boolean));
  const tree = treeRowsOf(rows, (k) => collapsed.has(k));
  assert.deepEqual(
    tree.map((r) => r.id),
    ["group:", "project:p1", "workstream:p1", "workstream:w2", "terminal:t1", "agent:s1", "agent:s1:sub"],
    "a project and its primary workstream share a record id but not a row id",
  );
  assert.deepEqual(
    tree.map((r) => r.label),
    ["Other projects", "Shop", "main", "feature", "shell", "dev", "explore"],
  );
  assert.deepEqual(
    tree.map((r) => [r.expandable, r.expanded]),
    [
      [true, true],
      [true, false],
      [true, true],
      [false, false],
      [false, false],
      [true, false],
      [false, false],
    ],
    "groups and projects always open; a workstream only with sessions; a harness only with sub-agents",
  );
  assert.equal(tree[1].row, rows[1], "the rail row rides along untouched");
  assert.deepEqual(
    tree.map((r) => r.depth),
    rows.map((r) => r.depth),
  );
});

test("an import never asks for a goal: nothing is guessed from the tab, a goal heading's door names its goal, and every sentence says where the project lands and where attaching lives", () => {
  for (const tab of ["workspace", "goals", "workflows"]) {
    const said = importLanding(tab, null);
    assert.equal(typeof said, "string", `${tab}: one sentence and nothing else — no goal to guess, no picker to change it in`);
    assert.match(said, /Workspace tab/, `${tab} says where a project with no goal sits`);
    assert.match(said, /About tab/, `${tab} says where attaching lives`);
  }
  assert.match(importLanding("workflows", null), /made by its steps/, "the tab says why an imported project never sits here");
  assert.match(importLanding("goals", "Dark mode"), /^It lands under Dark mode in Goals/, "a goal heading's door: the project lands under it, attached as it is made");
  assert.match(importLanding("workspace", "Dark mode"), /Dark mode/, "whatever tab the heading was on");
  assert.match(importLanding("goals", "Dark mode"), /undone from its About tab/, "and says the record is one to undo");
});

test("a workstream row carries its pulse, and a collapsed project the loudest of its workstreams'", () => {
  const now = 10_000_000;
  const rows = railRows({
    projects: [P("p1", "Alpha")],
    workstreams: [primary("p1"), worktree("w2", "p1", "work/cart", 2)],
    sessions: [S("s1", "p1", "running", { since: now / 1000 - 30 }), S("s2", "w2", "failed", { since: now / 1000 - 60 })],
    now,
  });
  const ws = rows.filter((r) => r.kind === "workstream");
  assert.equal(ws[0].pulse.headline, "running Read");
  assert.equal(ws[0].pulse.elapsed, "30s", "the rail's clock is milliseconds; the roster's is seconds");
  assert.equal(ws[1].pulse.headline, "failed: boom");
  assert.equal(rows[0].pulse, null, "an open project shows its workstreams' lines, not one of its own");
  const folded = railRows({ projects: [P("p1", "Alpha")], workstreams: [primary("p1"), worktree("w2", "p1", "work/cart", 2)], sessions: [S("s1", "p1", "running", { since: now / 1000 - 30 }), S("s2", "w2", "failed", { since: now / 1000 - 60 })], collapsed: new Set(["rail.project.p1.ungrouped"]), now });
  assert.equal(folded.length, 1);
  assert.equal(folded[0].pulse.headline, "failed: boom", "the loudest workstream speaks for the folded project");
  const quiet = railRows({ projects: [P("p1", "Alpha")], workstreams: [primary("p1")], sessions: [], now });
  assert.equal(quiet.find((r) => r.kind === "workstream").pulse, null);
});

test("a project's section and reveal plan follow its origin and group, in the words the rows use", () => {
  const byHand = P("p1", "Alpha").project;
  assert.deepEqual(sectionOf(byHand), { tab: "workspace", kind: "group", id: "", under: "ungrouped" });
  assert.deepEqual(revealPlan(byHand), { tab: "workspace", expand: ["rail.group.", "rail.project.p1.ungrouped"] });
  const grouped = P("p2", "Beta", { group: "Shop" }).project;
  assert.deepEqual(revealPlan(grouped), { tab: "workspace", expand: ["rail.group.Shop", "rail.project.p2.group:Shop"] });
  const ofGoal = P("p3", "Gamma", { origin: fromGoal("g1") }).project;
  assert.deepEqual(revealPlan(ofGoal), { tab: "goals", expand: ["rail.goal.g1", "rail.project.p3.goal:g1"] });
  const designed = P("p4", "Delta", { origin: { origin: "goal", goal: "g1", step: { run: "r1", step: "build", workflow: "d1" } } }).project;
  assert.equal(revealPlan(designed).tab, "goals", "a design's step makes the goal's project: the Goals tab");
  const ofStep = P("p5", "Epsilon", { origin: step("w1") }).project;
  assert.deepEqual(revealPlan(ofStep), { tab: "workflows", expand: ["rail.group.workflow:w1", "rail.project.p5.workflow:w1"] });
  // The plan's keys are the rows' own collapse keys.
  const rows = railRows({ tab: "goals", projects: [P("p3", "Gamma", { origin: fromGoal("g1") })], workstreams: [primary("p3"), worktree("w9", "p3", "work/x", 2)], goals: [{ id: "g1", label: "Goal one" }] });
  assert.equal(collapseKey(rows[0]), revealPlan(ofGoal).expand[0]);
  assert.equal(collapseKey(rows[1]), revealPlan(ofGoal).expand[1]);
  assert.equal(rowIndexOfProject(rows, "p3"), 1);
  assert.equal(rowIndexOfWorkstream(rows, "w9"), 3);
  assert.equal(rowIndexOfProject(rows, "nope"), -1);
});

test("a harness with sub-agents carries its child count and its own collapse key", () => {
  const withKids = {
    projects: [P("p1", "Alpha")],
    workstreams: [primary("p1")],
    sessions: [
      S("s1", "p1", "running", {
        agent: "dev",
        children: [
          { id: "c1", name: "explore", description: "map", state: { state: "thinking" }, since: 2 },
          { id: "c2", name: "review", description: "read", state: { state: "running" }, since: 2 },
        ],
      }),
    ],
  };
  const rows = railRows(withKids);
  const parent = rows.find((r) => r.kind === "agent" && r.parent === null);
  assert.equal(parent.childCount, 2, "the harness row knows how many sub-agents it holds");
  assert.equal(collapseKey(parent), "rail.agent.s1", "a harness with sub-agents folds on its own key");
  const sub = rows.find((r) => r.kind === "agent" && r.parent === "s1");
  assert.equal(collapseKey(sub), null, "a sub-agent has no key of its own");

  // Folding the harness hides its sub-agents; the harness row and its
  // workstream stay, and the fold does not touch other rows.
  const folded = railRows({ ...withKids, collapsed: new Set(["rail.agent.s1"]) });
  assert.deepEqual(
    folded.map((r) => `${r.kind}:${r.id}`),
    ["project:ungrouped:p1", "workstream:p1", "agent:s1"],
    "the two sub-agents are gone; nothing else is",
  );
  assert.equal(folded.find((r) => r.id === "s1").childCount, 2, "the count still shows while folded");
});

test("a childless harness never gets a fold key", () => {
  const rows = railRows({ projects: [P("p1", "Alpha")], workstreams: [primary("p1")], sessions: [S("s1", "p1", "running")] });
  assert.equal(collapseKey(rows.find((r) => r.kind === "agent")), null);
});

test("a harness opened in a terminal is its session's row, with its sub-agents folding on its key", () => {
  const rows = railRows({
    projects: [P("p1", "Alpha")],
    workstreams: [primary("p1")],
    terminals: [{ ...T("t1", "p1"), sessionId: "i1" }],
    sessions: [S("i1", "p1", "running", { kind: "terminal", children: [{ id: "a1", name: "explore", description: "map", state: { state: "thinking" }, since: 2 }] })],
  });
  assert.deepEqual(
    rows.map((r) => `${r.kind}:${r.id}`),
    ["project:ungrouped:p1", "workstream:p1", "agent:i1", "agent:i1/a1"],
    "no terminal row doubles the session; the sub-agent nests under it",
  );
  const row = rows.find((r) => r.id === "i1");
  assert.equal(row.terminalKey, "t1");
  assert.equal(collapseKey(row), "rail.agent.i1");
  const workstream = rows.find((r) => r.kind === "workstream");
  assert.deepEqual(workstream.shellGlyphs, [], "the reported tab is drawn once, by its session");
  assert.equal(workstream.activity.state, "running");
});

test("a project is found by id whichever list holds it — on screen or put away — and a door to a missing one gets null, never a silence", () => {
  const live = [P("p1", "Alpha"), P("p2", "Beta")];
  const away = [P("p3", "Gamma", { archived: true })];
  assert.equal(projectOf(live, away, "p2"), live[1]);
  assert.equal(projectOf(live, away, "p3"), away[0], "an archived project's row is a row: its id resolves");
  assert.equal(projectOf(live, away, "p9"), null);
  assert.equal(projectOf(live, away, null), null);
  assert.equal(projectOf([], [], "p1"), null);
});

test("a put-away project offers New workstream disabled with the reason; a live one offers it plainly", () => {
  assert.deepEqual(newWorkstreamOffer({ name: "Alpha", archived: false }), { label: "New workstream on Alpha…", disabled: false, reason: null });
  const away = newWorkstreamOffer({ name: "Gamma", archived: true });
  assert.equal(away.disabled, true);
  assert.equal(away.reason, ARCHIVED_NO_WORKSTREAM);
  assert.match(away.reason, /unarchive/);
  assert.equal(newWorkstreamOffer(null).disabled, false, "no project named: nothing to refuse here");
});

test("the project row's name is the row, never its slack", () => {
  // The model pins the name (a folded project's tree label is still its
  // name) and the pulse (a folded project's, its loudest workstream's). The
  // row draws both on one line, and the name is cut only when it alone would
  // not fit: it carries no `flex-1`; the pulse's holder does.
  const row = readFileSync(new URL("./rail/RailProjectRow.tsx", import.meta.url), "utf8").split("\n");
  const name = row.findIndex((l) => l.includes("{p.name}") && !l.includes("label=") && !l.includes("title=") && !l.includes("name="));
  const nameSpan = row.slice(Math.max(0, name - 1), name + 1).join(" ");
  assert.ok(name >= 0 && nameSpan.includes("truncate"), "the name is drawn on one line");
  assert.ok(!nameSpan.includes("flex-1"), "the name keeps its content width; it is not the row's slack");
  const pulse = row.findIndex((l) => l.includes("<PulseLine"));
  assert.ok(pulse >= 0, "a folded project's line is drawn");
  assert.ok(row.slice(Math.max(0, pulse - 2), pulse).join(" ").includes("flex-1"), "the pulse's holder is the row's slack");
});

test("the rail's strip is one row at every width: it folds in the rail's order, never wraps", () => {
  const rail = readFileSync(new URL("./ProjectRail.tsx", import.meta.url), "utf8");
  const strip = rail.slice(rail.indexOf("<Tabs"), rail.indexOf("active={tab}"));
  assert.ok(strip.length > 0, "the rail draws its tabs with the kit's strip");
  assert.ok(!strip.includes("fit="), "the strip has one behaviour: it folds");
  assert.ok(strip.includes("fold: RAIL_TAB_FOLD[id]"), "each tab knows its turn to yield its word");
  const kit = readFileSync(new URL("../../ui/Tabs.tsx", import.meta.url), "utf8");
  assert.ok(!kit.includes("flex-wrap"), "a strip never takes a second line");
});

test("the rail's prefs: a remembered tab off the list is the workspace tab, and the current project is found through the workstream the workbench is rooted in", async () => {
  const { RAIL_ARCHIVED_KEY, RAIL_TAB_KEY, currentProjectOf, railTabOf } = await import("./projectRailModel.mjs");
  assert.equal(railTabOf("goals"), "goals");
  assert.equal(railTabOf("nonsense"), "workspace");
  assert.equal(railTabOf(null), "workspace");
  assert.notEqual(RAIL_TAB_KEY, RAIL_ARCHIVED_KEY);
  const projects = [{ project: { id: "p1", name: "Alpha" } }, { project: { id: "p2", name: "Beta" } }];
  const workstreams = [{ workstream: { id: "w1", project: "p1" } }, { workstream: { id: "w2", project: "gone" } }];
  assert.equal(currentProjectOf({ scope: "workstream", id: "w1" }, workstreams, projects)?.project.id, "p1");
  assert.equal(currentProjectOf({ scope: "workstream", id: "w2" }, workstreams, projects), null, "a workstream whose project is gone names none");
  assert.equal(currentProjectOf({ scope: "workstream", id: "w9" }, workstreams, projects), null);
  assert.equal(currentProjectOf({ scope: "goal", id: "g1" }, workstreams, projects), null, "a goal root has no project");
  assert.equal(currentProjectOf(null, workstreams, projects), null);
});
