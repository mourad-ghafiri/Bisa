import test from "node:test";
import assert from "node:assert/strict";
import { checksSummary, controlsFor, defaultStrategy, mergeStrategies, prActionLabel, prRequest, splitHandles } from "./prFormModel.mjs";

test("the primary button says when the branch goes out first: only pushed work reads plain Open pull request", () => {
  for (const s of ["open", "dirty", "committed"]) assert.equal(prActionLabel(s), "Push and open pull request", s);
  assert.equal(prActionLabel("pushed"), "Open pull request");
  assert.equal(prActionLabel("pr_open"), "Open pull request");
  assert.equal(prActionLabel("pushed", "merge request"), "Open merge request", "GitLab's noun");
  assert.equal(prActionLabel("committed", "merge request"), "Push and open merge request");
});

const NONE = { draft_prs: false, reviewers: false, labels: false, merge_strategies: [], check_runs: false, review_comments: false };
const GITHUB = { draft_prs: true, reviewers: true, labels: true, merge_strategies: ["merge", "squash", "rebase"], check_runs: true, review_comments: true };

test("a code host with every capability off renders title and body and nothing else — absent, not disabled", () => {
  assert.deepEqual(controlsFor(NONE), ["title", "body"]);
  assert.deepEqual(controlsFor(null), ["title", "body"], "no code host yet: the same two");
  assert.deepEqual(mergeStrategies(NONE), []);
  assert.equal(defaultStrategy(NONE), null, "no merge control at all");
});

test("GitHub's capabilities render the whole form; the merge control starts on the setting, else the code host's first", () => {
  assert.deepEqual(controlsFor(GITHUB), ["title", "body", "draft", "reviewers", "labels"]);
  assert.equal(defaultStrategy(GITHUB), "merge", "no preference: the code host's first, never a squash of our own");
  assert.equal(defaultStrategy(GITHUB, "merge"), "merge");
  assert.equal(defaultStrategy(GITHUB, "squash"), "squash", "the setting wins when the code host offers it");
  assert.equal(defaultStrategy({ ...GITHUB, merge_strategies: ["rebase", "merge"] }, "squash"), "rebase", "a strategy the code host lacks falls to its first");
  assert.equal(defaultStrategy({ ...GITHUB, merge_strategies: ["rebase", "merge"] }), "rebase");
  assert.equal(defaultStrategy(NONE, "merge"), null, "no strategies, no control, whatever the setting");
});

test("the request carries only what the form showed", () => {
  const form = { title: " Dark mode ", body: "  ", draft: true, reviewers: "@ada, grace  ada", labels: "ui, ui" };
  assert.deepEqual(prRequest(NONE, form), { title: "Dark mode" }, "draft, reviewers and labels were never shown");
  assert.deepEqual(prRequest(GITHUB, form), {
    title: "Dark mode",
    draft: true,
    reviewers: ["ada", "grace"],
    labels: ["ui"],
  });
  assert.deepEqual(prRequest(GITHUB, { ...form, draft: false, reviewers: "", labels: "", body: "why" }), { title: "Dark mode", body: "why" });
  assert.deepEqual(splitHandles(" @a,  b\nc "), ["a", "b", "c"]);
});

test("checks summarise to one line with a tone", () => {
  assert.deepEqual(checksSummary([]), { text: "no checks", tone: "dim" });
  const runs = [
    { name: "build", status: "completed", conclusion: "success" },
    { name: "test", status: "completed", conclusion: "failure" },
    { name: "lint", status: "in_progress", conclusion: null },
  ];
  assert.deepEqual(checksSummary(runs), { text: "1 passed · 1 failed · 1 running", tone: "danger" });
  assert.equal(checksSummary([runs[0]]).tone, "ok");
  assert.equal(checksSummary([runs[2]]).tone, "dim");
});
