import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { IN_PROGRESS_LABEL, PULL_LABEL, PULL_MEANING, PULL_MODES, afterPull, forcePushRule, pullBannerWords, pullChoice, syncControls, syncLine, syncMenu, syncState } from "./syncModel.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const SETTINGS = readFileSync(join(HERE, "../../../../crates/bisa-core/src/settings.rs"), "utf8");
const INTERACTIVE = readFileSync(join(HERE, "../../../../crates/bisa-vcs/src/interactive.rs"), "utf8");

const status = (extra = {}) => ({
  git: true,
  exists: true,
  branch: "main",
  detached: false,
  head: "abc",
  upstream: "origin/main",
  remote: "git@github.com:o/r.git",
  ahead: 0,
  behind: 0,
  staged: 0,
  unstaged: 0,
  untracked: 0,
  conflicted: 0,
  clean: true,
  in_progress: null,
  error: null,
  ...extra,
});

test("the pull modes are the registry's words for git.pull, each with a label and a meaning", () => {
  const m = SETTINGS.match(/"git\.pull",\s*Choice\(&\[([^\]]+)\]\)/);
  assert.ok(m, "git.pull is a Choice in the registry");
  const words = [...m[1].matchAll(/"([a-z_]+)"/g)].map((x) => x[1]);
  assert.deepEqual([...PULL_MODES], words);
  for (const mode of PULL_MODES) {
    assert.ok(PULL_LABEL[mode] && PULL_MEANING[mode].length > 20, mode);
  }
  assert.equal(pullChoice("rebase"), "rebase");
  assert.equal(pullChoice("nonsense"), "ff_only");
  assert.equal(pullChoice(undefined), "ff_only");
});

test("every in-progress operation the vcs crate knows has a word", () => {
  const body = INTERACTIVE.match(/pub enum InProgress \{([\s\S]*?)\n\}/)[1];
  const ops = [...body.matchAll(/^\s{4}([A-Z][A-Za-z]*),/gm)].map((x) => x[1].replace(/([a-z])([A-Z])/g, "$1_$2").toLowerCase());
  assert.ok(ops.length >= 4, `${ops}`);
  for (const op of ops) assert.ok(IN_PROGRESS_LABEL[op], `${op} has a label`);
});

test("the state follows the status: no repository, no remote, no upstream, half-done, or ready", () => {
  assert.deepEqual(syncState(null), { kind: "not_git" });
  assert.deepEqual(syncState(status({ git: false })), { kind: "not_git" });
  assert.deepEqual(syncState(status({ exists: false })), { kind: "not_git" });
  assert.deepEqual(syncState(status({ in_progress: "merge" })), { kind: "in_progress", op: "merge" });
  assert.deepEqual(syncState(status({ remote: null })), { kind: "no_remote" });
  assert.deepEqual(syncState(status({ branch: null, detached: true })), { kind: "detached" });
  assert.deepEqual(syncState(status({ upstream: null })), { kind: "no_upstream", branch: "main" });
  assert.deepEqual(syncState(status({ ahead: 2, behind: 1 })), { kind: "ready", branch: "main", upstream: "origin/main", ahead: 2, behind: 1 });
});

test("the line and the controls say what can be done, and nothing more", () => {
  assert.equal(syncLine(syncState(status())), "Up to date with origin/main.");
  assert.equal(syncLine(syncState(status({ ahead: 2, behind: 1 }))), "origin/main · ↑2 ↓1");
  assert.match(syncLine(syncState(status({ upstream: null }))), /no upstream/);
  assert.match(syncLine(syncState(status({ remote: null }))), /No remote/);
  assert.match(syncLine(syncState(status({ in_progress: "cherry_pick" }))), /cherry-pick is in progress — the Resolve card above/);
  assert.deepEqual(syncControls({ kind: "ready", branch: "m", upstream: "o/m", ahead: 0, behind: 0 }), { fetch: true, pull: true, push: true, setOrigin: false });
  assert.deepEqual(syncControls({ kind: "no_upstream", branch: "m" }), { fetch: true, pull: false, push: true, setOrigin: false });
  assert.deepEqual(syncControls({ kind: "in_progress", op: "merge" }), { fetch: false, pull: false, push: false, setOrigin: false }, "the Operation card holds the verbs while something is half-done");
  assert.deepEqual(syncControls({ kind: "no_remote" }), { fetch: false, pull: false, push: false, setOrigin: true });
  assert.deepEqual(syncControls({ kind: "not_git" }), { fetch: false, pull: false, push: false, setOrigin: false });
});

test("the bar's menu is Refresh first, then under a rule the default pull and the other two, then under a rule Fetch, then under a rule the force push last — each off with a reason when the state says so", () => {
  const ready = syncMenu({ kind: "ready", branch: "topic", upstream: "origin/topic", ahead: 0, behind: 0 }, "rebase", "main");
  assert.deepEqual(
    ready.map((i) => [i.id, i.label, i.disabled, i.separatorBefore ?? false]),
    [
      ["refresh", "Refresh", false, false],
      ["pull:rebase", "Pull with rebase", false, true],
      ["pull:ff_only", "Pull", false, false],
      ["pull:merge", "Pull and merge", false, false],
      ["fetch", "Fetch", false, true],
      ["force_push", "Force push with lease…", false, true],
    ],
  );
  assert.equal(ready[0].id, "refresh", "the read opens the menu");
  assert.equal(ready[4].id, "fetch", "the network read comes after the pulls");
  assert.equal(ready.at(-1).id, "force_push", "the one dangerous verb closes it, under its own rule");
  assert.ok(ready.at(-1).danger, "and wears the danger tone");
  assert.ok(ready.slice(0, -1).every((i) => !i.danger));
  assert.equal(ready[1].icon, "install", "the default pull wears the glyph");
  assert.equal(ready[2].icon, null);
  assert.ok(ready.every((i) => i.hint && i.hint.length > 10));
  const none = syncMenu({ kind: "no_upstream", branch: "x" }, "ff_only");
  assert.equal(none[4].disabled, false, "fetch works without an upstream");
  assert.equal(none[1].disabled, true);
  assert.match(none[1].reason, /no upstream/);
  const half = syncMenu({ kind: "in_progress", op: "rebase" }, "ff_only");
  assert.ok(half[4].disabled && half[1].disabled && half[5].disabled, "nothing but Abort while half-done");
  assert.match(half[4].reason, /half-done/);
  assert.equal(half[0].disabled, false, "refresh is always a read");
});

test("the force push is open only on a ready branch that is not the project's default, and says why not otherwise", () => {
  const ready = (branch) => ({ kind: "ready", branch, upstream: `origin/${branch}`, ahead: 1, behind: 0 });
  assert.deepEqual(forcePushRule(ready("topic"), "main"), { on: true, reason: null });
  assert.deepEqual(forcePushRule(ready("topic"), null), { on: true, reason: null }, "no default branch known: the node still refuses one");
  assert.match(forcePushRule(ready("main"), "main").reason, /Never the project's default branch/);
  assert.match(forcePushRule({ kind: "no_upstream", branch: "topic" }, "main").reason, /no upstream yet — Push publishes it/);
  assert.match(forcePushRule({ kind: "no_remote" }, "main").reason, /No remote/);
  assert.match(forcePushRule({ kind: "detached" }, "main").reason, /No branch is checked out/);
  assert.match(forcePushRule({ kind: "in_progress", op: "merge" }, "main").reason, /half-done/);
  assert.equal(forcePushRule({ kind: "not_git" }, "main").on, false);
  const onMain = syncMenu(ready("main"), "ff_only", "main").at(-1);
  assert.equal(onMain.id, "force_push");
  assert.ok(onMain.disabled);
  assert.match(onMain.reason, /default branch/);
  assert.match(onMain.hint, /still points where it did when you last fetched/, "the lease, in the person's words");
});

test("a pull's answer or refusal becomes one banner, by the node's code", () => {
  const ok = { mode: "ff_only", upstream: "origin/main", from: "a", to: "b", moved: true };
  assert.deepEqual(afterPull({ ok }), { kind: "moved", upstream: "origin/main", from: "a", to: "b", mode: "ff_only" });
  assert.deepEqual(afterPull({ ok: { ...ok, moved: false, to: "a" } }), { kind: "current", upstream: "origin/main" });
  assert.deepEqual(afterPull({ err: { status: 409, code: "not_fast_forward", message: "m", detail: { ahead: 1, behind: 2 } } }), { kind: "not_fast_forward", ahead: 1, behind: 2 });
  assert.deepEqual(afterPull({ err: { status: 409, code: "conflict", message: "conflict: x", detail: { paths: ["README.md", "a/b.rs"], in_progress: "merge" } } }), {
    kind: "conflict",
    paths: ["README.md", "a/b.rs"],
    in_progress: "merge",
    detail: "conflict: x",
  });
  assert.deepEqual(afterPull({ err: { status: 409, code: "in_progress", message: "m", detail: { in_progress: "rebase" } } }), { kind: "in_progress", op: "rebase" });
  assert.deepEqual(afterPull({ err: { status: 409, code: "conflict", message: "m", detail: null } }), { kind: "conflict", paths: [], in_progress: null, detail: "m" }, "no detail: an empty conflict, still a conflict");
  assert.deepEqual(afterPull({ err: { status: 409, message: "cannot lock ref" } }), { kind: "error", detail: "cannot lock ref" }, "a 409 with no code is an error, never a guess");
  assert.deepEqual(afterPull({ err: { status: 502, code: null, message: "gateway" } }), { kind: "error", detail: "gateway" });
});

test("a pull's banner is one or two sentences, counted by the catalog, whatever the outcome", () => {
  assert.deepEqual(pullBannerWords(afterPull({ ok: { mode: "ff_only", upstream: "origin/main", from: "abcdef1234567", to: "1234567abcdef", moved: true } })), { title: "Pulled from origin/main: abcdef1 → 1234567.", body: null });
  assert.deepEqual(pullBannerWords(afterPull({ ok: { mode: "ff_only", upstream: "origin/main", from: "a", to: "a", moved: false } })), { title: "Already up to date with origin/main.", body: null });
  const ff = pullBannerWords(afterPull({ err: { status: 409, code: "not_fast_forward", message: "x", detail: { ahead: 1, behind: 3 } } }));
  assert.equal(ff.title, "Not a fast-forward.");
  assert.equal(ff.body, "This branch has 1 commit of its own and the upstream has 3. Nothing moved. Choose how to bring them together:");
  assert.match(pullBannerWords(afterPull({ err: { status: 409, code: "not_fast_forward", message: "x", detail: { ahead: 2, behind: 0 } } })).body, /^This branch has 2 commits of its own/);
  const stopped = pullBannerWords(afterPull({ err: { status: 409, code: "conflict", message: "x", detail: { paths: ["a.rs"], in_progress: "rebase" } } }));
  assert.equal(stopped.title, "The rebase stopped on 1 file.");
  assert.match(stopped.body, /continues or aborts the rebase when you are done\.$/);
  const plain = pullBannerWords(afterPull({ err: { status: 409, code: "conflict", message: "x", detail: { paths: ["a.rs", "b.rs"] } } }));
  assert.equal(plain.title, "The pull stopped on 2 files.", "a pull that left no operation half-done is named as a pull");
  assert.deepEqual(pullBannerWords(afterPull({ err: { status: 409, code: "in_progress", message: "x", detail: { in_progress: "merge" } } })), { title: "Something is already in progress here: a merge.", body: "Settle the files below, then continue or abort it from the Resolve card above, before pulling again." });
  assert.equal(pullBannerWords(afterPull({ err: { status: 409, code: "in_progress", message: "x", detail: {} } })).title, "Something is already in progress here.");
  assert.deepEqual(pullBannerWords(afterPull({ err: { status: 500, message: "the node fell over" } })), { title: "the node fell over", body: null });
  // The bar draws the model's sentences and spells none of its own.
  const bar = readFileSync(new URL("./SyncBar.tsx", import.meta.url), "utf8").replace(/\/\*[\s\S]*?\*\//g, "").replace(/^\s*\/\/.*$/gm, "");
  assert.ok(bar.includes("const words = pullBannerWords(banner);"));
  assert.ok(!/stopped on|of its own|already in progress/.test(bar), "the banners' sentences left the component");
});
