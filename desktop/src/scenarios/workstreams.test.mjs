/**
 * A workstream as a person works it, from the project it is born in to the
 * card it becomes (ide/07, ide/16): the project dialog sends the kind's own
 * keys and says the node's refusal where it can be read; the workstream
 * dialog sends what it shows; the rail's row, the panel's header and the
 * Board's card wear one title; a commit — whichever door made it — moves the
 * record, and the statuses, the workspace's index, the card's column and the
 * lifecycle's spine all follow from that record; a rename and a placement are
 * one body each; the centre's three modes go round; removing the project
 * takes its roots with it, and the IDE leaves a project archived or removed
 * under the person for the next home or the landing. Stepped through the
 * models the way the components do; no DOM.
 *
 * Run with `node --test --import ./src/i18n/preload.mjs src/scenarios/workstreams.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { reloadsWorkspace } from "../shell/workspaceLoadModel.mjs";
import { movesStatuses, movesWorkstream } from "../shell/workstreamFramesModel.mjs";
import { boardRows, canMoveTo, columnShown, optimisticMove, placeBody, placeIndex, statusIndex } from "../views/_board/boardModel.mjs";
import { terminationConsent, terminationCounts } from "../views/_work/closeWorkstreamModel.mjs";
import { STEPS, lifecycle } from "../views/_work/prLifecycleModel.mjs";
import { creationBody, publishCaveat, refusalPlace, slugFromName } from "../views/_work/projectForm.mjs";
import { attachChoice, attachableGoals, attachedWords } from "../views/_work/attachGoalModel.mjs";
import { projectRoots, rootsGo, standsOn } from "../views/_work/removeProjectModel.mjs";
import { reviewFacts } from "../views/_work/reviewStepModel.mjs";
import { cardChips, cardTitle, renameBody, renames } from "../views/_work/workstreamCardModel.mjs";
import { canOpenWorkstream, openBody, previewBranch } from "../views/_work/workstreamCreation.mjs";
import { homeAfterLeaving, homeRoot, homeRoute, leavesRoot } from "../views/_workbench/ideHomeModel.mjs";
import { availableModes, centreOf, modeFor, nextMode, rememberMode } from "../views/_workbench/ideModeModel.mjs";
import { railRows } from "../views/_workbench/projectRailModel.mjs";
import { boardScope, selectionOf } from "../views/_workbench/railSelectionModel.mjs";
import { misfits } from "./schemaFit.mjs";

const SCHEMA = JSON.parse(readFileSync(new URL("../../api-schema.json", import.meta.url), "utf8"));
const fits = (name, body) => misfits(SCHEMA, name, body);

const project = (id, name, extra = {}) => ({
  project: { id, slug: slugFromName(name), name, tags: [], root: { type: "managed" }, vcs: { type: "git", default_branch: "main" }, publish: "auto", revision: 1, created_at: 0, origin: { origin: "workspace" }, ...extra },
  path: `/ws/projects/${slugFromName(name)}/tree`,
  exists: true,
  workstreams: 1,
  goals: [],
});
const ref = (workstream, project_name) => ({ workstream, project_name, path: `/ws/${workstream.id}`, exists: true });
const record = (id, pid, kind, state = "open", extra = {}) => ({ id, project: pid, name: null, note: null, pinned: false, kind, goal: null, work_item: null, agent: null, state: { state }, created_at: 100, board: { column: null, rank: null, due: null }, ...extra });
const status = (wid, pid, over = {}) => ({ workstream: wid, project: pid, name: null, exists: true, git: true, branch: "work/dark-mode-a3f9k2", base: "main", ahead_of_base: 0, behind_base: 0, upstream: null, ahead: 0, behind: 0, staged: 0, unstaged: 0, untracked: 0, conflicted: 0, clean: true, in_progress: null, pr: null, running_agents: 0, state: { state: "open" }, ...over });
const GITHUB = { draft_prs: true, reviewers: true, labels: true, merge_strategies: ["merge", "squash"], check_runs: true, review_comments: true, review_threads: true, delete_branch: true };
const spine = (w, s) => lifecycle({ isGit: true, state: w.state.state, aheadOfBase: s.ahead_of_base, upstream: s.upstream, ahead: s.ahead, base: s.base, pr: null, checks: null, review: reviewFacts({ reviews: [], viewer: "you", comments: [], caps: GITHUB }), caps: GITHUB });

test("a project is born: the body is the kind's own keys, and what the node refuses is said where the form can show it", () => {
  const body = creationBody({ provenance: "new", placement: "copy", slug: slugFromName("Web app"), name: " Web app ", publish: "auto", tags: ["frontend"], gitConfig: null, url: "typed and left behind", path: "/typed/and/left" });
  assert.deepEqual(body, { slug: "web-app", name: "Web app", publish: "auto", tags: ["frontend"], kind: "new" }, "a field of another way in does not ride along");
  assert.deepEqual(fits("NewProjectBody", body), []);
  // The node checks who a project is given to where it is made and where it is edited: its sentence, whole, on the form.
  assert.deepEqual(refusalPlace({ message: "agent not found: ghost", refusal: "error-engine-invalid-agent-not-found", status: 400 }, "new"), { form: "agent not found: ghost" });
  // A slug already taken is said beside the slug, by the refusal's id — whatever language its sentence is in.
  assert.deepEqual(refusalPlace({ message: "un projet nommé web-app existe déjà", refusal: "error-store-invalid-project-named-already-exists", status: 400 }, "new"), { slug: "un projet nommé web-app existe déjà" });
  // A refusal about a folder reaching a form that shows no folder field: said on the form, never lost.
  assert.deepEqual(Object.keys(refusalPlace({ message: "/ws/projects/web-app/tree already exists", refusal: "error-engine-invalid-already-exists-refusing-import-into-rather-than", status: 400 }, "new")), ["form"]);
});

test("a project is born with no goal asked: the body carries none, and attaching it afterwards is the one dialog's model — from About or the rail", () => {
  const body = creationBody({ provenance: "new", placement: "copy", slug: "web-app", name: "Web app", publish: "gated", tags: [], gitConfig: null, url: "", path: "" });
  assert.ok(!("goal" in body), "the route decides: POST /goals/{id}/projects when a door fixed a goal, POST /projects otherwise");
  assert.deepEqual(fits("NewProjectBody", body), []);
  // Gated with no goal: the one thing the form says about it, and where to go.
  assert.match(publishCaveat("gated", false), /About tab/);
  // Later, from About › Goals or the rail's menu: the goals the project is not on, the only one chosen for you, the toast naming both ends.
  const goals = [{ id: "g1", label: "Dark mode" }, { id: "g2", label: "Checkout" }];
  assert.deepEqual(attachableGoals(goals, ["g1"]).map((g) => g.id), ["g2"]);
  assert.equal(attachChoice(attachableGoals(goals, ["g1"])), "g2");
  assert.equal(attachChoice(attachableGoals(goals, [])), "", "two to choose from: the person picks");
  assert.equal(attachedWords("Web app", "Checkout"), "Web app is attached to Checkout.");
});

test("a workstream is opened: the guard, the branch previewed, and a body of what the dialog shows that the node takes", () => {
  assert.equal(canOpenWorkstream({ exists: true }, { git: true, exists: true, head: null }).reason, "unborn", "no commit yet: the primary is where work runs");
  assert.deepEqual(canOpenWorkstream({ exists: true }, { git: true, exists: true, head: "abc1234" }), { ok: true, mode: "branch" });
  assert.equal(previewBranch({ label: "Dark mode", project: "web-app" }).branch.startsWith("work/dark-mode-"), true);
  const asked = [
    openBody({ git: true, label: "Dark mode", fields: { kind: "new", name: "", start: "" }, base: "main", defaultBranch: "main" }),
    openBody({ git: true, label: "left from before", fields: { kind: "branch", branch: "topic" }, base: "release", defaultBranch: "main" }),
    openBody({ git: true, label: "", fields: { kind: "remote", remote: "origin", branch: "feature/x" }, base: "main", defaultBranch: "main" }),
    openBody({ git: true, label: "", fields: { kind: "tag", tag: "v1.2.0", branch: "" }, base: "", defaultBranch: "main" }),
    openBody({ git: true, label: "", fields: { kind: "tag", newTag: true, tagName: "v2", tagAt: "main" }, base: "main", defaultBranch: "main" }),
    openBody({ git: true, label: "", fields: { kind: "pr", pr: 12 }, base: "release", defaultBranch: "main" }),
    openBody({ git: false, label: "cart-total", fields: { kind: "new" }, base: "", defaultBranch: null }),
  ];
  for (const { body, problem } of asked) {
    assert.equal(problem, null);
    assert.deepEqual(fits("NewWorkstreamBody", body), [], JSON.stringify(body));
  }
  assert.deepEqual(asked[1].body, { source: { source: "local_branch", name: "topic" }, base: "release" }, "the label of a field no longer shown stays behind");
  assert.equal("base" in asked[5].body, false, "a pull request brings its own base");
});

test("one title everywhere: the rail's row, the panel's header and the Board's card read the same rule", () => {
  const shop = project("p1", "Shop");
  const primary = record("p1", "p1", { kind: "primary" });
  const branch = record("w1", "p1", { kind: "worktree", branch: "work/dark-mode-a3f9k2", base: "main" });
  const named = record("w2", "p1", { kind: "worktree", branch: "work/cart-7bq2xx", base: "main" }, "open", { name: "Cart total", created_at: 200 });
  const refs = [ref(primary, "Shop"), ref(branch, "Shop"), ref(named, "Shop")];
  const statuses = [status("p1", "p1", { branch: "main", base: null }), status("w1", "p1"), status("w2", "p1", { branch: "work/cart-7bq2xx" })];
  const rail = Object.fromEntries(railRows({ projects: [shop], workstreams: refs, statuses }).filter((r) => r.kind === "workstream").map((r) => [r.id, r.label]));
  const board = Object.fromEntries(Object.values(boardRows({ refs, statuses: statusIndex(statuses) }).columns).flat().map((r) => [r.id, r.title]));
  const panel = Object.fromEntries(refs.map((r, i) => [r.workstream.id, cardTitle(r.workstream, statuses[i])]));
  assert.deepEqual(rail, { p1: "main", w1: "work/dark-mode-a3f9k2", w2: "Cart total" });
  assert.deepEqual(board, rail);
  assert.deepEqual(panel, rail);
});

test("a commit moves the record whichever door made it, and the statuses, the index, the card and the lifecycle follow the record", () => {
  const before = record("w1", "p1", { kind: "worktree", branch: "work/dark-mode-a3f9k2", base: "main" });
  const refs = (w) => [ref(record("p1", "p1", { kind: "primary" }), "Shop"), ref(w, "Shop")];
  // Opened, nothing committed: Backlog, and the commit is the step in hand.
  assert.equal(columnShown(before), "backlog");
  assert.equal(spine(before, status("w1", "p1")).current, "commit");
  assert.deepEqual(cardChips(before, status("w1", "p1")).map((c) => c.id), ["no_upstream"]);

  // A commit from Git › Changes: the node says the two facts it says for the workstream's own commit.
  const facts = [
    { type: "workstream_changed", workstream: "w1", state: { state: "committed" } },
    { type: "workstream_committed", workstream: "w1", branch: "work/dark-mode-a3f9k2", commit: "abc1234def" },
  ];
  for (const fact of facts) {
    assert.equal(movesStatuses(fact.type), true, `${fact.type} re-reads every status`);
    assert.equal(movesWorkstream(fact, "w1", "p1"), true, `${fact.type} re-reads the checkout's panel`);
    assert.equal(movesWorkstream(fact, "w9", "p1"), false, "and no other checkout's");
  }
  assert.equal(reloadsWorkspace("workstream_changed"), true, "the index the rail and the Board draw from reads the record again");

  // The record as the node now holds it, and the status beside it.
  const after = { ...before, state: { state: "committed" } };
  const live = status("w1", "p1", { ahead_of_base: 1, state: { state: "committed" } });
  const board = boardRows({ refs: refs(after), statuses: statusIndex([live]) });
  assert.deepEqual(board.columns.doing.map((r) => r.id), ["w1"], "the card moved to Doing because the record did");
  assert.deepEqual(board.columns.backlog.map((r) => r.id), ["p1"]);
  assert.equal(cardChips(after, live)[0].text, "committed");
  const life = spine(after, live);
  assert.equal(Object.fromEntries(life.steps.map((s) => [s.id, s.status])).commit, "done");
  assert.equal(life.current, "push");
  assert.equal(life.steps.length, STEPS.length, "the spine never changes length");
  // The record alone is enough: a status not yet re-read still leaves the commit done.
  assert.equal(Object.fromEntries(spine(after, status("w1", "p1")).steps.map((s) => [s.id, s.status])).commit, "done");

  // A card a person placed stays where they put it: a commit does not pull it back.
  const placed = { ...after, board: { column: "todo", rank: 1024, due: null } };
  assert.equal(columnShown(placed), "todo");
  // A closed workstream is Archived whatever was chosen.
  assert.equal(columnShown({ ...placed, state: { state: "closed" } }), "archived");
});

test("a rename and a placement are one body each, and the node takes both", () => {
  assert.equal(renames(null, "  "), false, "nothing typed over no name writes nothing");
  assert.deepEqual(fits("PatchWorkstreamBody", renameBody(" Dark mode ")), []);
  assert.deepEqual(renameBody(""), { name: null });
  assert.deepEqual(fits("PatchWorkstreamBody", renameBody("")), []);
  assert.deepEqual(fits("PatchWorkstreamBody", { due: "2026-10-02" }), []);
  assert.deepEqual(fits("PatchWorkstreamBody", { pinned: true }), []);

  const refs = [
    ref(record("a", "p1", { kind: "worktree", branch: "a", base: "main" }, "committed", { board: { column: "doing", rank: 1024, due: null } }), "Shop"),
    ref(record("b", "p1", { kind: "worktree", branch: "b", base: "main" }, "open"), "Shop"),
    ref(record("c", "p2", { kind: "worktree", branch: "c", base: "main" }, "pushed", { board: { column: "doing", rank: 2048, due: null } }), "Blog"),
  ];
  const shown = boardRows({ refs, statuses: {}, projects: new Set(["p1"]) }).columns;
  // *Move to Doing* from the card's menu: last among what the column shows.
  const moved = optimisticMove(shown, "b", "doing", shown.doing.length);
  assert.deepEqual(moved.doing.map((r) => r.id), ["a", "b"]);
  const body = placeBody("doing", placeIndex({ refs, shown, id: "b", column: "doing", index: shown.doing.length }));
  assert.deepEqual(body, { column: "doing", index: 1 }, "right after a — before c, which the narrowed Board does not show");
  assert.deepEqual(fits("PlaceWorkstreamBody", body), []);
  assert.notDeepEqual(fits("PlaceWorkstreamBody", { column: "doing", index: Number.MAX_SAFE_INTEGER }), [], "the index the menu once sent is past what the wire takes");
  assert.equal(canMoveTo({ column: "doing", workstream: refs[0].workstream }, "doing"), false);
});

test("the rail's selection is the Board's scope, and the centre's three modes go round", () => {
  const shop = project("p1", "Shop");
  const rows = railRows({ projects: [shop], workstreams: [ref(record("p1", "p1", { kind: "primary" }), "Shop")], statuses: [] });
  const picked = selectionOf(rows.find((r) => r.kind === "workstream"));
  const scope = boardScope(picked, () => "Shop");
  assert.equal(scope.all, false);
  assert.deepEqual([...scope.projects], ["p1"], "a workstream's row selects the project it stands in");
  assert.equal(boardScope(null, () => undefined).all, true, "nothing picked: every workstream");

  const root = { scope: "workstream", hasProject: true };
  let memory = {};
  let mode = modeFor(memory["workstream:w1"], "project");
  const seen = [centreOf(mode, root)];
  for (let i = 0; i < 3; i += 1) {
    mode = nextMode(mode);
    memory = rememberMode(memory, "workstream:w1", mode);
    seen.push(centreOf(mode, root));
  }
  assert.deepEqual(seen, ["documents", "conversation", "board", "documents"]);
  // The Board switched off in Settings: two modes, and a root that remembered it opens in the default.
  assert.deepEqual(availableModes(false), ["project", "agent"]);
  assert.equal(modeFor("board", "agent", false), "agent");
  assert.equal(centreOf("board", { scope: "goal", hasProject: false }), "documents", "a goal's root has no Board");
});

test("a project removed takes its roots with it, and a close says what ends — after the node has answered", () => {
  const refs = [ref(record("p1", "p1", { kind: "primary" }), "Shop"), ref(record("w1", "p1", { kind: "worktree", branch: "x", base: "main" }), "Shop"), ref(record("p2", "p2", { kind: "primary" }), "Blog")];
  const roots = projectRoots(refs, "p1");
  assert.deepEqual(roots, ["p1", "w1"]);
  assert.equal(rootsGo("delete") && rootsGo("forget") && !rootsGo("archive"), true);
  assert.equal(standsOn({ scope: "workstream", id: "w1" }, roots), true, "standing in a checkout of it");
  assert.equal(standsOn({ scope: "workstream", id: "p2" }, roots), false);
  const tab = (id, harness) => ({ scope: "workstream", id, harness, liveness: { status: "live", code: null } });
  const counts = terminationCounts([{ workstream: "w1", kind: "worker", state: { state: "running" } }], [tab("w1", "claude-code"), tab("w1", null)], "w1");
  assert.deepEqual(counts, { harnesses: 1, shells: 1, agents: 1 });
  assert.equal(terminationConsent(counts), "Closing it ends what stands in it: 1 harness terminated, 1 shell closed and 1 agent session aborted.");
});

test("the IDE leaves a project archived or removed under the person: the first project's primary, else the landing — the same moment, whoever did it", () => {
  const read = (rel) => readFileSync(new URL(rel, import.meta.url), "utf8");
  const shop = project("p1", "Shop");
  const blog = project("p2", "Blog");
  const refs = [ref(record("p1", "p1", { kind: "primary" }), "Shop"), ref(record("w1", "p1", { kind: "worktree", branch: "x", base: "main" }), "Shop"), ref(record("p2", "p2", { kind: "primary" }), "Blog")];

  // Standing on Shop's checkout, the rail archives Shop. The lists in hand
  // still list Shop and its checkout, the remembered root is that checkout:
  // the home is Blog's primary all the same — a route the rail takes by replace.
  const roots = projectRoots(refs, "p1");
  const home = homeAfterLeaving("p1", "w1", [shop, blog], refs);
  assert.deepEqual(home, { scope: "workstream", id: "p2" });
  assert.equal(standsOn({ scope: "workstream", id: "w1" }, roots), true);
  assert.deepEqual(homeRoute(home), { name: "workbench", scope: "workstream", id: "p2" });
  // Standing on Blog while Shop goes: nobody moves, and the remembered root stands.
  assert.equal(standsOn({ scope: "workstream", id: "p2" }, roots), false);
  assert.deepEqual(homeAfterLeaving("p1", "p2", [shop, blog], refs), { scope: "workstream", id: "p2" });
  // Blog, the last project, deleted from About: no home — the landing with its one door.
  assert.equal(homeAfterLeaving("p2", "p2", [blog], [refs[2]]), null);
  assert.deepEqual(homeRoute(null), { name: "projects" });
  // `#/projects` afterwards, with the lists read again: an archived Shop's checkouts are listed and are nobody's home.
  assert.equal(homeRoot("w1", [], refs), null, "only an archived project left: the landing, never its hidden root");
  assert.deepEqual(homeRoot("w1", [blog], refs), { scope: "workstream", id: "p2" }, "a live project left: its primary");

  // The same act from the command line or another window: the workbench
  // hears the fact and leaves for the same home; an unarchive leaves nobody.
  assert.equal(leavesRoot({ type: "project_archived", project: "p1", archived: true }, { scope: "workstream", id: "w1" }, refs), "p1");
  assert.equal(leavesRoot({ type: "project_deleted", project: "p1" }, { scope: "workstream", id: "p1" }, refs), "p1");
  assert.equal(leavesRoot({ type: "project_archived", project: "p1", archived: false }, { scope: "workstream", id: "w1" }, refs), null);
  assert.equal(leavesRoot({ type: "project_deleted", project: "p1" }, { scope: "workstream", id: "p2" }, refs), null);
  assert.ok(reloadsWorkspace("project_archived") && reloadsWorkspace("project_deleted"), "and the lists the rail and the home read follow the same facts");

  // The wiring: the workbench hears through the one rule and reads a missing
  // root as every detail screen does; About leaves on an archive too.
  const workbench = read("../views/Workbench.tsx");
  assert.ok(workbench.includes("const leaving = leavesRoot(e.payload, { scope, id }, ws.workstreams);"), "the workbench hears the bus");
  assert.ok(workbench.includes("if (leaving) replace(homeRoute(homeAfterLeaving(leaving, lastRoot(), ws.projects, ws.workstreams)));"), "and leaves by replace, from the facts in hand");
  assert.ok(workbench.includes('useGonePlace(missing, { name: "workbench", scope, id });'), "a checkout that answers not-found is left like any place that is gone");
  assert.ok(workbench.includes("const home = homeRoot(lastRoot(), ws.projects, ws.workstreams);"), "`#/projects` picks its home by the one rule");
  const detail = read("../views/_work/ProjectDetail.tsx");
  assert.ok(detail.includes("onLeft?.(home);") && !detail.includes("onDeleted"), "About hands the home up for every act");
  const panel = read("../views/_workbench/RightPanel.tsx");
  assert.ok(panel.includes("<ProjectDetail pid={pid} goals={goals} onLeft={onLeft} />"), "through the panel");
});
