import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import * as model from "./commitActionsModel.mjs";
import {
  COMMIT_ACTIONS,
  MENU_SECTIONS,
  commitActions,
  conflictWords,
  consentWords,
  disabledReason,
  doneWords,
  headOf,
  menuActions,
  refActions,
  refNameProblem,
  shortRecovery,
} from "./commitActionsModel.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const ICONS = readFileSync(join(HERE, "../../ui/icons.ts"), "utf8");

const row = { id: "abc123def456", short: "abc123d", refs: [] };

test("the actions are one list with one label, one glyph the kit has, one hint and one consent rule each; the menu sections name every action once", () => {
  const ids = COMMIT_ACTIONS.map((a) => a.id);
  assert.deepEqual(ids, ["cherry_pick", "revert", "checkout", "branch", "tag", "inspect", "copy_sha", "copy_short", "attach"]);
  assert.equal(new Set(ids).size, ids.length);
  for (const a of COMMIT_ACTIONS) {
    assert.ok(a.label && a.menuLabel && a.hint.length > 15, `${a.id} has its words`);
    assert.match(ICONS, new RegExp(`^  ${a.icon}: `, "m"), `${a.id} draws with ICON.${a.icon}`);
    assert.equal(a.consented, ["cherry_pick", "revert", "checkout"].includes(a.id), "only what moves the tree is consented");
    if (a.consented || a.id === "branch" || a.id === "tag") assert.ok(a.menuLabel.endsWith("…"), `${a.id} asks first, so its menu item says so`);
  }
  for (const id of MENU_SECTIONS.flat()) assert.ok(ids.includes(id), `${id} is an action`);
  assert.deepEqual(MENU_SECTIONS.flat().sort(), [...ids].sort(), "the menu offers every action once");
});

test("a consented action is off while another runs, while an operation is half-done, and where it makes no sense — each with its reason; the rest are always on", () => {
  assert.equal(disabledReason("cherry_pick", row), null);
  assert.match(disabledReason("cherry_pick", row, { busy: true }), /running/);
  assert.match(disabledReason("revert", row, { inProgress: "rebase" }), /a rebase is half-done — the Resolve card above finishes or aborts it/);
  assert.match(disabledReason("checkout", row, { inProgress: "cherry_pick" }), /a cherry-pick is half-done/);
  assert.match(disabledReason("checkout", row, { headId: row.id }), /already here/);
  assert.match(disabledReason("cherry_pick", row, { headId: row.id }), /already the tip/);
  assert.equal(disabledReason("revert", row, { headId: row.id }), null, "reverting HEAD is an ordinary revert");
  for (const id of ["branch", "tag", "inspect", "copy_sha", "copy_short", "attach"]) {
    assert.equal(disabledReason(id, row, { busy: true, inProgress: "merge", headId: row.id }), null, `${id} never moves the tree, so it is never off`);
  }
  assert.match(disabledReason("nonsense", row), /Not an action/);
  const all = commitActions(row, { busy: true });
  assert.equal(all.length, COMMIT_ACTIONS.length);
  assert.ok(all.find((a) => a.id === "revert").disabled && all.find((a) => a.id === "tag").disabled === false);
});

test("the menu is the sections with a rule before each section but the first, offers every action once, and there is no hover subset", () => {
  const menu = menuActions(row, { headId: row.id });
  assert.equal(new Set(menu.map((a) => a.id)).size, COMMIT_ACTIONS.length, "every action, once");
  // The row's verbs are one menu — its `⋮` and its right-click — so a hover
  // strip that squeezed the subject out of the row cannot come back unnoticed.
  assert.equal("hoverActions" in model, false);
  assert.equal("HOVER_ACTIONS" in model, false);
  assert.deepEqual(
    menu.map((a) => a.id),
    ["copy_sha", "copy_short", "checkout", "cherry_pick", "revert", "branch", "tag", "inspect", "attach"],
  );
  assert.deepEqual(
    menu.filter((a) => a.separatorBefore).map((a) => a.id),
    ["checkout", "branch", "inspect"],
  );
  assert.equal(menu.find((a) => a.id === "checkout").label, "Checkout (detached)…", "the menu says the long form");
  assert.ok(menu.find((a) => a.id === "checkout").disabled && menu.find((a) => a.id === "revert").disabled === false);
});

test("a ref chip's menu: a branch is switched to, opened as a workstream or copied; a remote copied; a tag opened, copied or deleted; HEAD nothing", () => {
  const ids = (ref, ctx) => refActions(ref, ctx).map((a) => a.id);
  assert.deepEqual(ids({ name: "main", kind: "branch" }), ["switch", "open_workstream", "copy_ref"]);
  assert.deepEqual(ids({ name: "origin/main", kind: "remote" }), ["copy_ref"]);
  assert.deepEqual(ids({ name: "v1.2.0", kind: "tag" }), ["open_workstream", "copy_ref", "delete_tag"]);
  assert.deepEqual(ids({ name: "HEAD", kind: "head" }), []);
  // The workstream door is never off: it opens a dialog, and moves nothing here.
  const door = refActions({ name: "main", kind: "branch" }, { busy: true, inProgress: "merge", currentBranch: "main" })[1];
  assert.equal(door.label, "Open a workstream on main…");
  assert.ok(!door.disabled && !door.consented);
  assert.equal(refActions({ name: "v1", kind: "tag" }, { busy: true })[0].label, "Open a workstream at v1…");
  const sw = refActions({ name: "main", kind: "branch" })[0];
  assert.equal(sw.label, "Switch to main…");
  assert.ok(sw.consented && !sw.danger && !sw.disabled);
  assert.match(refActions({ name: "main", kind: "branch" }, { currentBranch: "main" })[0].reason, /on it/);
  assert.match(refActions({ name: "main", kind: "branch" }, { busy: true })[0].reason, /running/);
  const del = refActions({ name: "v1", kind: "tag" }, { inProgress: "merge" })[2];
  assert.ok(del.consented && del.danger && del.disabled);
  assert.match(del.reason, /a merge is half-done/);
  assert.equal(del.label, "Delete tag v1…");
});

test("HEAD's commit is read from the rows, and unknown until its row is in", () => {
  assert.equal(headOf([]), null);
  assert.equal(headOf([undefined, { id: "a", refs: [{ kind: "branch", name: "main" }] }]), null);
  assert.equal(headOf([undefined, { id: "a", refs: [] }, { id: "b", refs: [{ kind: "head", name: "HEAD" }, { kind: "branch", name: "main" }] }]), "b");
});

test("a ref name is refused for each rule git has, with a sentence, and taken otherwise", () => {
  for (const ok of ["feature/x", "v1.2.0", "release-1", "a.b", "fix_it", "topic/sub/deep", "v2"]) assert.equal(refNameProblem(ok), null, ok);
  const bad = {
    "": /needed/,
    "  ": /needed/,
    "a b": /spaces/,
    "a~b": /git refuses them in a ref name/,
    "a^b": /refuses/,
    "a:b": /refuses/,
    "a?b": /refuses/,
    "a*b": /refuses/,
    "a[b": /refuses/,
    "a\\b": /refuses/,
    "-x": /dash/,
    "/x": /slash/,
    "x/": /slash/,
    "a//b": /empty segment/,
    "a..b": /two dots/,
    "a@{1}": /@\{/,
    "@": /HEAD/,
    "x.": /end with a dot/,
    "x.lock": /\.lock/,
    "a/b.lock": /\.lock/,
    ".hidden": /start with a dot/,
    "a/.b": /start with a dot/,
  };
  for (const [name, re] of Object.entries(bad)) assert.match(refNameProblem(name) ?? "", re, JSON.stringify(name));
});

test("the confirmation, the toast and the recovery ref read one way per action, and a tag delete is the one red confirmation", () => {
  for (const kind of ["checkout", "cherry_pick", "revert", "switch"]) {
    const c = consentWords(kind, { short: "abc123d", name: "main" });
    assert.ok(c.title.endsWith("?") && !c.body.includes("refs/bisa/safety/") && c.confirm, `${kind}: a question, the promise left to SafetyNote`);
    assert.equal(c.danger, false, `${kind} is recoverable, so it is not red`);
  }
  assert.match(consentWords("cherry_pick", { short: "abc123d" }).body, /leaves the cherry-pick in progress/);
  assert.match(consentWords("revert", { short: "abc123d", parents: 2 }).body, /merge commit: it is read against its first parent/);
  assert.ok(!consentWords("revert", { short: "abc123d" }).body.includes("merge commit"));
  assert.match(consentWords("switch", { short: "abc123d", name: "main" }).title, /Switch to main/);
  const del = consentWords("delete_tag", { short: "abc123d", name: "v1" });
  assert.ok(del.danger && del.body.includes("pinned in Safety") && del.body.includes("stays on the remote"));
  assert.equal(shortRecovery("refs/bisa/safety/x_1"), "x_1");
  assert.equal(doneWords("cherry_pick", { short: "abc123d", recovery: "refs/bisa/safety/wip_3" }), "Cherry-picked abc123d. What was here is saved as wip_3.");
  assert.equal(doneWords("branch", { short: "abc123d", name: "topic" }), "Branch topic created at abc123d.");
  assert.equal(doneWords("tag", { short: "abc123d", name: "v1" }), "Tagged abc123d as v1.");
  assert.equal(doneWords("switch", { short: "abc123d", name: "main" }), "On main now.");
  assert.equal(doneWords("delete_tag", { short: "abc123d", name: "v1" }), "Deleted the tag v1.");
  assert.equal(doneWords("checkout", { short: "abc123d" }), "HEAD is now detached at abc123d.");
});

test("a conflict is read by the node's code, never the status alone", () => {
  assert.equal(conflictWords(null, "fell back"), "fell back");
  assert.equal(conflictWords({ status: 500, code: "conflict" }, "fell back"), "fell back");
  assert.equal(conflictWords({ status: 409, code: "conflict", detail: { paths: ["a.rs", "b.rs"], in_progress: "cherry_pick" } }, "x"), "Stopped on 2 conflicted files: a.rs, b.rs. The Resolve card under Git walks you through them, then continues or aborts the cherry-pick.");
  assert.equal(conflictWords({ status: 409, code: "conflict", detail: { paths: ["a.rs"] } }, "x"), "Stopped on 1 conflicted file: a.rs. The Resolve card under Git walks you through it.");
  assert.equal(conflictWords({ status: 409, code: "in_progress", detail: {} }, "A rebase is in progress"), "A rebase is in progress — the Resolve card under Git finishes or aborts it.");
});
