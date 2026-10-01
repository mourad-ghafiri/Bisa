import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { CHIP_IDS, PLACE, SAFETY_LINE, SAFETY_SENTENCE, VERB, bulkLabel, confirmLabel, STANDING_TONE, fileSummary, kindWord, sideWords, standingChips, standingHint } from "./gitWords.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));

test("one act, one label: every verb is one capitalised word — a force push the one two-word act, as terminology names it — and the four words for getting rid of something are distinct", () => {
  for (const [k, w] of Object.entries(VERB)) assert.match(w, /^[A-Z][a-z]+$/.test(w) || k === "forcePush" ? /^[A-Z][a-z]+( [a-z]+)?$/ : /^[A-Z][a-z]+$/, `${k} is one word`);
  assert.equal(VERB.forcePush, "Force push");
  assert.equal(new Set(Object.values(VERB)).size, Object.keys(VERB).length, "no two acts share a word");
  assert.deepEqual([VERB.discard, VERB.delete, VERB.drop, VERB.abort], ["Discard", "Delete", "Drop", "Abort"]);
  assert.equal(VERB.switch, "Switch");
  assert.equal(VERB.detach, "Detach");
  // *Continue* is the operation's own verb (ide/04 §Conflicts, continued), not a generic yes.
  for (const [k, w] of Object.entries(VERB)) if (k !== "continue") assert.ok(!/^(Go|Continue|OK|Yes)$/.test(w), "a confirmation is never a generic word");
});

test("every place is spelled with › and starts at its occupant", () => {
  for (const [k, p] of Object.entries(PLACE)) {
    if (k === "inbox") continue;
    assert.ok(p.includes(" › "), `${k}: ${p}`);
    assert.ok(!p.includes("→"), "never an arrow");
  }
  assert.equal(PLACE.settings, "About › Settings");
  assert.equal(PLACE.checkout, "About › Checkout", "the checkout's connection and config are their own view");
  assert.equal(PLACE.safety, "Git › Branches › Safety");
  assert.ok(SAFETY_SENTENCE.includes("refs/bisa/safety/") && SAFETY_SENTENCE.includes(PLACE.safety));
  assert.equal(SAFETY_LINE.split(" ").length, 4);
});

test("a confirmation's word is the verb, with the count past one", () => {
  assert.equal(confirmLabel("discard"), "Discard");
  assert.equal(confirmLabel("delete", { count: 3 }), "Delete 3 files");
  assert.equal(confirmLabel("drop", { count: 2, noun: "entries" }), "Drop 2 entries");
  assert.equal(confirmLabel("switch", { count: 1 }), "Switch");
});

test("a row's word is the letter of the side it is shown for, and a file's two sides read differently", () => {
  const both = { index: "A", worktree: "M" };
  assert.equal(kindWord(both, "staged"), "added");
  assert.equal(kindWord(both, "unstaged"), "modified");
  assert.equal(kindWord({ index: "D", worktree: "." }, "staged"), "deleted");
  assert.equal(kindWord({ index: ".", worktree: "R" }, "unstaged"), "renamed");
  assert.equal(kindWord({ index: ".", worktree: "T" }, "unstaged"), "type changed");
  assert.equal(kindWord({ index: ".", worktree: "C" }, "unstaged"), "copied");
  assert.equal(kindWord({ untracked: true }, "unstaged"), "untracked");
  assert.equal(kindWord({ untracked: true, index: "?", worktree: "?" }, "staged"), "untracked", "an untracked file has its own word on either side");
  assert.equal(kindWord({ conflicted: true, index: "U", worktree: "U" }, "unstaged"), "conflict");
  assert.equal(kindWord({ index: ".", worktree: "." }, "staged"), "changed", "an unknown letter still has a word");
  assert.equal(kindWord(null, "staged"), "changed");
});

test("a row wears one chip per side it has — staged in the accent, unstaged quiet, untracked quiet, a conflict a warning — each the door to its patch", () => {
  const strip = (chips) => chips.map((c) => [c.id, c.side, c.word, c.tone]);
  assert.deepEqual(strip(standingChips({ index: "A", worktree: "M", staged: true, unstaged: true })), [
    ["staged", "staged", "added", "accent"],
    ["unstaged", "unstaged", "modified", "quiet"],
  ]);
  assert.deepEqual(strip(standingChips({ index: "M", worktree: ".", staged: true, unstaged: false })), [["staged", "staged", "modified", "accent"]]);
  assert.deepEqual(strip(standingChips({ index: ".", worktree: "D", staged: false, unstaged: true })), [["unstaged", "unstaged", "deleted", "quiet"]]);
  assert.deepEqual(strip(standingChips({ index: "?", worktree: "?", untracked: true, unstaged: true })), [["untracked", "unstaged", "untracked", "quiet"]], "an untracked file's patch is the working-tree side");
  assert.deepEqual(strip(standingChips({ index: "U", worktree: "U", conflicted: true })), [["conflicted", "unstaged", "conflict", "warn"]]);
  assert.deepEqual(standingChips(null), []);
  assert.deepEqual(STANDING_TONE, { staged: "accent", unstaged: "quiet", untracked: "quiet", conflicted: "warn" });
  for (const chip of standingChips({ index: "A", worktree: "M", staged: true, unstaged: true })) {
    assert.equal(chip.hint, standingHint(chip.id, chip.word));
    assert.ok(chip.hint.includes(chip.word), "the tooltip repeats the word it explains");
  }
  assert.match(standingHint("staged", "added"), /^Staged: added — the index against HEAD/);
  assert.match(standingHint("unstaged", "modified"), /^Unstaged: modified — the working tree against the index/);
  assert.match(standingHint("untracked", "untracked"), /never seen/);
  assert.match(standingHint("conflicted", "conflict"), /unmerged/);
});

test("the chip legend knows every chip id the card model emits, and no other", () => {
  const src = readFileSync(join(HERE, "workstreamCardModel.mjs"), "utf8");
  const emitted = new Set([...src.matchAll(/id: "([a-z_]+)"/g)].map((m) => m[1]));
  assert.deepEqual([...CHIP_IDS].sort(), [...emitted].sort());
});

test("the summary counts a file once however many lists it is in, and is null when nothing changed", () => {
  const groups = {
    staged: [{ path: "a.rs" }, { path: "b.rs" }],
    unstaged: [{ path: "a.rs" }, { path: "c.rs" }],
    untracked: [{ path: "d.rs" }],
    conflicted: [],
  };
  assert.equal(fileSummary(groups), "4 files · 2 staged · 2 unstaged · 1 untracked");
  assert.equal(fileSummary({ staged: [], unstaged: [], untracked: [], conflicted: [{ path: "x" }] }), "1 file · 1 conflict");
  assert.equal(fileSummary({ staged: [], unstaged: [], untracked: [], conflicted: [] }), null);
  assert.equal(fileSummary(null), null);
});

test("the side words and the abort toast use the verbs", () => {
  assert.equal(sideWords(true), "index against HEAD");
  assert.equal(sideWords(false), "worktree against the index");
});

test("a bulk verb says what it reaches: the count on a section's row, all on the toolbar", () => {
  assert.equal(bulkLabel("stage", 3), "Stage 3");
  assert.equal(bulkLabel("unstage", 4), "Unstage 4");
  assert.equal(bulkLabel("stage"), "Stage all");
  assert.equal(bulkLabel("stage", 0), "Stage 0", "a section at zero still says its word; the button is off");
});
