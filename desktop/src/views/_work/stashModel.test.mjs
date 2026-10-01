/**
 * The words around a stash in the Stashes view. Run with
 * `node --test desktop/src/views/_work/stashModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import {
  appliedWords,
  canStash,
  droppedWords,
  poppedWords,
  stashConfirm,
  stashPushCopy,
  stashRefusal,
  stashRow,
  stashablePaths,
  stashedWords,
} from "./stashModel.mjs";

const entry = { index: 0, commit: "a".repeat(40), branch: "main", message: "half a change", subject: "On main: half a change", at: 1700000000, untracked: false };
const wip = { ...entry, index: 2, message: null, subject: "WIP on feature: 1234567 baseline", branch: "feature", untracked: true };
const status = { git: true, exists: true, head: "abc", staged: 1, unstaged: 0, untracked: 0, conflicted: 0, in_progress: null, ahead: 0, behind: 0, clean: false, detached: false };

test("a row reads the message or git's own WIP, where it was made, and whether untracked files ride along", () => {
  assert.deepEqual(stashRow(entry), { id: "stash@{0}", title: "half a change", where: "on main", untracked: false, at: 1700000000 });
  const row = stashRow(wip);
  assert.equal(row.id, "stash@{2}");
  assert.equal(row.title, "WIP on feature", "git's own subject is not shown verbatim — the sha and old subject say nothing a person wants");
  assert.equal(row.untracked, true);
  assert.equal(stashRow({ ...entry, branch: null, message: null }).title, "WIP on a detached HEAD");
  assert.equal(stashRow({ ...entry, branch: null }).where, "detached");
});

test("Stash changes… is offered only when there is something git would save, and the reason says why not", () => {
  assert.deepEqual(canStash(status), { ok: true, reason: null });
  assert.match(canStash({ ...status, git: false }).reason, /Not a repository/);
  assert.match(canStash({ ...status, head: null }).reason, /No commit yet/);
  assert.match(canStash({ ...status, in_progress: "cherry_pick" }).reason, /cherry-pick is in progress/);
  assert.match(canStash({ ...status, conflicted: 2 }).reason, /Conflicts to settle first/);
  assert.match(canStash({ ...status, staged: 0, untracked: 3 }).reason, /tick Include untracked/);
  assert.equal(canStash({ ...status, staged: 0, untracked: 3 }, true).ok, true, "with untracked included, untracked files are something to save");
  assert.equal(canStash({ ...status, staged: 0 }).reason, "Nothing to stash.");
});

test("the dialog and the confirmations say what leaves, what stays, and where it is saved", () => {
  const whole = stashPushCopy([]);
  assert.equal(whole.title, "Stash the working tree's changes");
  assert.match(whole.description, /refs\/bisa\/safety\//);
  assert.match(whole.untracked.hint, /Ignored files never do/);
  assert.match(whole.keepIndex.hint, /stays staged/);
  assert.equal(stashPushCopy(["src/a.rs"]).title, "Stash src/a.rs");
  assert.equal(stashPushCopy(["a", "b"]).title, "Stash 2 files");
  assert.match(stashPushCopy(["a"]).description, /Only these paths/);

  const drop = stashConfirm("drop", entry);
  assert.equal(drop.title, "Drop stash@{0}?");
  assert.ok(!drop.body.includes("Safety"), "the promise is the dialog's SafetyNote, said once");
  assert.equal(drop.danger, true);
  const pop = stashConfirm("pop", wip);
  assert.equal(pop.title, "Pop stash@{2}?");
  assert.match(pop.body, /A conflict keeps the entry/);
  assert.equal(pop.danger, false, "a pop is recoverable twice over: the tree and the entry are both saved");
});

test("the toasts name the entry and the recovery ref", () => {
  const ref = "refs/bisa/safety/1700-stash_push.wip";
  assert.equal(stashedWords(entry, ref), "Stashed as stash@{0}: half a change. What was here is saved as 1700-stash_push.wip.");
  assert.match(appliedWords(entry, "refs/bisa/safety/1700-stash_apply"), /the entry is kept/);
  assert.match(poppedWords(entry, "refs/bisa/safety/1700-stash_pop"), /1700-stash_pop in Safety/);
  assert.match(droppedWords(entry, "refs/bisa/safety/1700-stash_drop.wip"), /pinned as 1700-stash_drop in Safety — Restore puts it back/);
});

test("a refusal is read from the node's code, and the two stash codes are the ones dto.rs declares", () => {
  const src = readFileSync(new URL("../../../../crates/bisa-node/src/dto.rs", import.meta.url), "utf8");
  const block = src.slice(src.indexOf("pub enum ErrorCode {"), src.indexOf("\n}\n", src.indexOf("pub enum ErrorCode {")));
  const codes = [...block.matchAll(/^\s+([A-Z][A-Za-z]+),$/gm)].map((m) => m[1].replace(/([a-z])([A-Z])/g, "$1_$2").toLowerCase());
  assert.ok(codes.includes("nothing_to_stash") && codes.includes("stash_moved") && codes.includes("conflict") && codes.includes("in_progress"), codes.join(","));
  for (const code of ["nothing_to_stash", "stash_moved", "conflict", "in_progress"]) {
    assert.equal(stashRefusal({ code, status: 409, message: "m", detail: null }, "apply").kind, code, `${code} is told apart`);
  }
  const conflict = stashRefusal({ code: "conflict", status: 409, message: "conflict: …", detail: { paths: ["a.txt", "b.txt"], in_progress: null } }, "pop");
  assert.equal(conflict.sentence, "Stopped on 2 conflicted files: a.txt, b.txt. Settle them under Conflicted or discard them — a stash conflict leaves nothing to abort. The stash entry is kept.");
  assert.doesNotMatch(stashRefusal({ code: "conflict", status: 409, message: "m", detail: { paths: ["a"] } }, "apply").sentence, /entry is kept/, "apply never dropped anything to keep");
  assert.equal(stashRefusal({ code: null, status: 409, message: "unmerged paths to settle first: a" }, "push").kind, "dirty", "a 409 with no code is git refusing over the tree");
  assert.deepEqual(stashRefusal({ code: null, status: 500, message: "boom" }, "push"), { kind: "error", sentence: "boom" }, "anything else is the node's own sentence");
  assert.match(stashRefusal({ code: "stash_moved", status: 409, message: "m" }, "drop").sentence, /Reloaded; pick the entry again/);
});

test("the dialog scopes to tracked, unconflicted paths, each once, in the listing's order", () => {
  const files = [
    { path: "src/b.rs", staged: true, unstaged: true },
    { path: "src/a.rs", staged: false, unstaged: true },
    { path: "new.txt", untracked: true },
    { path: "src/c.rs", conflicted: true },
    { path: "src/a.rs", staged: true, unstaged: false },
    { path: "same", staged: false, unstaged: false },
  ];
  assert.deepEqual(stashablePaths(files), ["src/b.rs", "src/a.rs"], "untracked and conflicted are out; a path staged and unstaged is listed once");
  assert.deepEqual(stashablePaths(null), []);
  assert.deepEqual(stashablePaths([{ path: "" , staged: true }]), []);
});
