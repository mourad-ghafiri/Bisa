import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { branchActions, branchFilter, branchOrder, consentWords, doneWords, halfDoneReason, localNameFor, menuActions, remoteBranchActions, standingWords } from "./branchActionsModel.mjs";
import { VERB } from "./gitWords.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const ICONS = readFileSync(join(HERE, "../../ui/icons.ts"), "utf8");
const ctx = { current: "main", defaultBranch: "main" };

test("another branch offers Switch on hover, then merge, rebase, cherry-pick, the upstream, a workstream, rename and delete; the current one offers an interactive rebase and push with lease; the default is never renamed or deleted", () => {
  const other = branchActions({ name: "topic", current: false }, ctx);
  assert.deepEqual(
    other.map((a) => a.id),
    ["switch", "merge", "rebase", "cherry_pick", "upstream", "workstream", "rename", "delete"],
  );
  assert.deepEqual(
    menuActions({ name: "topic", current: false }, ctx).map((a) => a.id),
    ["merge", "rebase", "cherry_pick", "upstream", "workstream", "rename", "delete"],
  );
  assert.equal(other[0].label, "Switch to topic");
  assert.equal(other[1].label, "Merge topic into main…");
  assert.equal(other[2].label, "Rebase main onto topic…");
  assert.equal(other[3].label, "Cherry-pick from topic…");
  assert.equal(other[4].label, "Set upstream…");
  assert.equal(branchActions({ name: "topic", current: false, upstream: "origin/topic" }, ctx)[4].label, "Upstream: origin/topic…");
  assert.ok(other[5].separatorBefore && !other[5].consented, "a door to the dialog, in its own group");
  assert.ok(other[7].danger && other[7].consented, "delete is red and asks");
  const current = branchActions({ name: "topic", current: true }, ctx);
  assert.deepEqual(
    current.map((a) => a.id),
    ["rebase_plan", "push_lease", "upstream", "workstream", "rename"],
  );
  const def = branchActions({ name: "main", current: true }, ctx);
  assert.deepEqual(def.map((a) => a.id), ["upstream", "workstream"], "the default branch, checked out: its upstream and a workstream — never rewritten, renamed or deleted from here");
  const defOther = branchActions({ name: "main", current: false }, { current: "topic", defaultBranch: "main" });
  assert.deepEqual(
    defOther.map((a) => a.id),
    ["switch", "merge", "rebase", "cherry_pick", "upstream", "workstream"],
    "the default branch from elsewhere: switch, merge, rebase, pick, upstream, a workstream — never rename or delete",
  );
  for (const a of [...other, ...current]) assert.match(ICONS, new RegExp(`^  ${a.icon}: `, "m"), `${a.id} draws with ICON.${a.icon}`);
});

test("every consented act is off with a reason while another runs or an operation is half-done; merge, rebase and pick are off with no branch checked out", () => {
  for (const a of branchActions({ name: "topic", current: false }, { ...ctx, busy: true })) {
    if (a.id === "workstream") {
      assert.ok(!a.disabled, "the door to the dialog moves nothing here, so it is never off");
      continue;
    }
    assert.ok(a.disabled && /running/.test(a.reason), a.id);
  }
  for (const a of branchActions({ name: "topic", current: false }, { ...ctx, inProgress: "rebase" })) {
    if (a.id === "workstream") continue;
    assert.equal(a.reason, "A rebase is half-done — the Resolve card above finishes or aborts it.", a.id);
  }
  assert.equal(halfDoneReason("cherry_pick"), "A cherry-pick is half-done — the Resolve card above finishes or aborts it.");
  const detached = branchActions({ name: "topic", current: false }, { current: null, defaultBranch: "main" });
  assert.ok(!detached.find((a) => a.id === "switch").disabled);
  for (const id of ["merge", "rebase", "cherry_pick"]) assert.match(detached.find((a) => a.id === id).reason, /No branch is checked out/);
});

test("a remote branch offers Check out on hover, the acts against it, its fetch, a workstream, the delete on the remote — never the default — and the copies", () => {
  const acts = remoteBranchActions({ remote: "origin", name: "feature/x" }, ctx);
  assert.deepEqual(
    acts.map((a) => a.id),
    ["checkout", "merge", "rebase", "cherry_pick", "fetch", "workstream", "delete_remote", "copy_name", "copy_full"],
  );
  assert.equal(acts[0].label, "Check out origin/feature/x");
  assert.ok(acts[0].hover && acts[0].consented);
  assert.equal(acts[1].label, "Merge origin/feature/x into main…");
  assert.equal(acts[6].label, "Delete on origin…");
  assert.ok(acts[6].danger && acts[6].consented && acts[6].separatorBefore);
  assert.equal(acts[8].label, "Copy origin/feature/x");
  const def = remoteBranchActions({ remote: "origin", name: "main" }, ctx);
  assert.ok(def.find((a) => a.id === "delete_remote").disabled);
  assert.match(def.find((a) => a.id === "delete_remote").reason, /default branch/);
  const held = remoteBranchActions({ remote: "origin", name: "feature/x" }, { ...ctx, inProgress: "merge" });
  assert.ok(held.find((a) => a.id === "checkout").disabled && !held.find((a) => a.id === "copy_name").disabled);
  for (const a of acts) assert.match(ICONS, new RegExp(`^  ${a.icon}: `, "m"), `${a.id} draws with ICON.${a.icon}`);
});

test("every confirmation is a question, one or two sentences, the verb as its button, never a generic word, and names its recovery kind", () => {
  const kinds = ["switch", "checkout_remote", "delete_branch", "delete_remote", "delete_tag", "push_lease"];
  for (const kind of kinds) {
    const c = consentWords(kind, { name: "topic", from: "origin/topic", remote: "origin", current: "main" });
    assert.ok(c.title.endsWith("?"), `${kind}: ${c.title}`);
    assert.ok(c.body.split(". ").length <= 3, `${kind}: short`);
    assert.ok(!c.body.includes("refs/bisa/safety/"), `${kind}: the promise is SafetyNote's`);
    assert.ok(!/^(Go ahead|Continue|OK)$/.test(c.confirm), `${kind}: ${c.confirm}`);
    assert.ok(["commit", "tree", "stash"].includes(c.kind));
  }
  assert.equal(consentWords("switch", { name: "topic" }).confirm, VERB.switch);
  assert.equal(consentWords("checkout_remote", { name: "topic", from: "origin/topic" }).title, "Check out origin/topic as topic?");
  assert.match(consentWords("checkout_remote", { name: "topic", from: "origin/topic" }).body, /tracking it/);
  assert.equal(consentWords("delete_branch", { name: "topic" }).confirm, VERB.delete);
  assert.match(consentWords("delete_remote", { name: "topic", remote: "origin" }).body, /publishing policy/);
  assert.ok(consentWords("delete_branch", { name: "topic" }).danger && consentWords("push_lease", { name: "topic" }).danger);
  assert.equal(consentWords("push_lease", { name: "topic" }).title, "Force push topic with a lease?", "the act is named for what it is, as the Changes view names it");
  assert.equal(consentWords("push_lease", { name: "topic" }).confirm, "Force push with lease");
  assert.ok(!consentWords("switch", { name: "topic" }).danger, "a switch is recoverable, so it is not red");
  assert.match(consentWords("restore", { rec: { ref_name: "refs/bisa/safety/1-checkout.wip", kind: "tree", branch: "main" } }).body, /HEAD to main/);
  assert.equal(consentWords("restore", {}).confirm, VERB.restore);
});

test("the toasts use the button's verb — Aborted, never Abandoned", () => {
  assert.equal(doneWords("switch", { name: "topic" }), "On topic now.");
  assert.equal(doneWords("checkout_remote", { name: "topic", to: "origin/topic" }), "On topic now, tracking origin/topic.");
  assert.equal(doneWords("delete_branch", { name: "topic" }), "Deleted the branch topic.");
  assert.equal(doneWords("delete_remote", { name: "topic", to: "origin" }), "Deleted topic on origin.");
  assert.equal(doneWords("rename", { name: "a", to: "b" }), "Renamed a to b.");
  assert.equal(doneWords("create", { name: "x" }), "Created x. Nothing moved.");
  assert.equal(doneWords("create_switch", { name: "x" }), "Created x and switched to it.");
  assert.equal(doneWords("cherry_pick", { count: 3 }), "Cherry-picked 3 commits.");
  assert.equal(doneWords("revert", { count: 1 }), "Reverted 1 commit.");
  assert.equal(doneWords("rebase_plan", { name: "topic", to: "5 commits → 3: two squashed" }), "Rebased topic — 5 commits → 3: two squashed.");
  assert.equal(doneWords("upstream", { name: "topic", to: "origin/topic" }), "topic now follows origin/topic.");
  assert.equal(doneWords("upstream", { name: "topic" }), "topic follows no upstream now.");
  assert.equal(doneWords("abort", { op: "cherry_pick" }), "Aborted the cherry-pick.");
  assert.equal(doneWords("push_lease", { name: "topic" }), "Pushed topic with lease.");
});

test("the list is filtered by name or upstream, ordered current first then the default, and a remote branch takes its own name when free", () => {
  const rows = [
    { name: "topic", current: false, upstream: "origin/topic", ahead: 2, behind: 1 },
    { name: "main", current: false, upstream: "origin/main", ahead: 0, behind: 0 },
    { name: "fix/one", current: true, upstream: null, ahead: 0, behind: 0 },
  ];
  assert.deepEqual(
    branchFilter(rows, "ORIGIN/t").map((b) => b.name),
    ["topic"],
  );
  assert.deepEqual(branchFilter(rows, "  ").map((b) => b.name), ["topic", "main", "fix/one"]);
  assert.deepEqual(
    branchOrder(rows, "main").map((b) => b.name),
    ["fix/one", "main", "topic"],
  );
  assert.equal(localNameFor({ remote: "origin", name: "feature/x" }, rows), "feature/x");
  assert.equal(localNameFor({ remote: "origin", name: "topic" }, rows), "origin-topic", "the name is taken: prefixed with the remote's");
  // Words, not typed arrows: the row draws the kit's glyphs, and this is what a screen reader and the tooltip say.
  assert.equal(standingWords(rows[0]), "2 ahead, 1 behind");
  assert.equal(standingWords(rows[1]), "");
  assert.equal(standingWords({ ahead: 0, behind: 3 }), "3 behind");
  assert.equal(standingWords({ ahead: 4, behind: 0 }), "4 ahead");
});
