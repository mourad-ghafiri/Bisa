import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { applyPrSuggestion, checksSummary, controlsFor, defaultStrategy, draftedWords, mergeStrategies, prActionLabel, prRequest, prSuggestionOutcome, splitHandles } from "./prFormModel.mjs";

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

test("a suggested pull request is a draft or a sentence — never a draft nobody wrote", () => {
  assert.deepEqual(prSuggestionOutcome({ suggested: true, title: " Fix the rounding ", body: " Rounds half up. " }), { draft: { title: "Fix the rounding", body: "Rounds half up." }, note: null });
  const refused = prSuggestionOutcome({ suggested: false, title: "", body: "", error: "no harness on this host" });
  assert.equal(refused.draft, null, "nothing drafted, nothing put in the fields");
  assert.match(String(refused.note), /no harness on this host/, "and the reason, said");
  assert.equal(prSuggestionOutcome(null).draft, null);
  assert.ok(prSuggestionOutcome({ suggested: false }).note, "a refusal with no reason still says something");
  assert.equal(prSuggestionOutcome({ suggested: true, title: "   ", body: "a body alone" }).draft, null, "no title is no draft");
  assert.deepEqual(prSuggestionOutcome({ suggested: true, title: "Fix it" }).draft, { title: "Fix it", body: "" }, "a title alone is a draft");
});

test("a draft fills what was left alone and keeps what was typed while it was asked", () => {
  const asked = { title: "add cart total", body: "" };
  const draft = { title: "Add the cart total", body: "Shows the total under the cart." };
  assert.deepEqual(applyPrSuggestion({ asked, now: asked, draft }), { title: draft.title, body: draft.body, kept: [] }, "nothing typed: the draft, whole");
  assert.deepEqual(applyPrSuggestion({ asked, now: { ...asked, title: "My own title" }, draft }), { title: "My own title", body: draft.body, kept: ["title"] });
  assert.deepEqual(applyPrSuggestion({ asked, now: { ...asked, body: "My notes" }, draft }), { title: draft.title, body: "My notes", kept: ["body"] });
  assert.deepEqual(applyPrSuggestion({ asked, now: { title: "T", body: "B" }, draft }).kept, ["title", "body"]);
  const written = { title: "x", body: "A body already written" };
  assert.equal(applyPrSuggestion({ asked: written, now: written, draft: { title: "Fix it", body: "" } }).body, "A body already written", "a draft with no body leaves the body be");
  assert.match(draftedWords([], "merge request"), /merge request/, "read it before you open it, in the code host's own word");
  assert.match(draftedWords(["title"], "pull request"), /title/);
  assert.match(draftedWords(["body"], "pull request"), /body/);
  assert.match(draftedWords(["title", "body"], "pull request"), /neither/);
});

test("the dialog asks the node for the draft, offers Undo and drops the ask when it closes", () => {
  const form = readFileSync(new URL("./PrForm.tsx", import.meta.url), "utf8");
  const lifecycle = readFileSync(new URL("./PullRequestLifecycle.tsx", import.meta.url), "utf8");
  const api = readFileSync(new URL("../../api.ts", import.meta.url), "utf8");
  assert.ok(lifecycle.includes("suggest={(signal) => api.suggestPr(wid, signal)}"), "the lifecycle hands the form its ask");
  assert.ok(api.includes("post<SuggestedPullRequest>(`/workstreams/${wid}/pr/suggest`, {}, s)"), "the one route");
  assert.ok(form.includes("prSuggestionOutcome(await suggest(controller.signal))") && form.includes("applyPrSuggestion({ asked, now, draft: outcome.draft })"), "the model decides what lands");
  assert.ok(form.includes("ask.current?.abort();") && form.includes("useEffect(() => () => ask.current?.abort(), []);"), "closing or leaving drops the ask");
  assert.ok(form.includes('t("work-pr-form-undo")') && form.includes("<ICON.agent size={12} aria-hidden />"), "Undo, and the agent's glyph on Suggest as on the commit box's");
});

