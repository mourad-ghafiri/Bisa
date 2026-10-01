import test from "node:test";
import assert from "node:assert/strict";
import { boolOf, choiceOf, numberOf, settingOf, stringOf } from "./settingsModel.mjs";

const RESOLVED = [
  { key: "git.pull", value: "rebase", origin: "project" },
  { key: "git.delete_branch_after_merge", value: false, origin: "workspace" },
  { key: "workstreams.after_merge", value: "nonsense", origin: "project" },
  { key: "git.fetch_interval_secs", value: null, origin: "default" },
];

test("a resolved value is read by key, and a missing or null one is the fallback", () => {
  assert.equal(settingOf(RESOLVED, "git.pull", "ff_only"), "rebase");
  assert.equal(settingOf(RESOLVED, "git.fetch_interval_secs", 300), 300, "null is nothing");
  assert.equal(settingOf(RESOLVED, "git.merge_strategy", "merge"), "merge", "absent is nothing");
  assert.equal(settingOf(null, "git.pull", "ff_only"), "ff_only", "no list yet");
  assert.equal(settingOf(undefined, "git.pull", "ff_only"), "ff_only");
});

test("a choice is kept to its words: a value the key does not allow falls back", () => {
  const MODES = ["ff_only", "rebase", "merge"];
  assert.equal(choiceOf(RESOLVED, "git.pull", MODES, "ff_only"), "rebase");
  assert.equal(choiceOf(RESOLVED, "workstreams.after_merge", ["ask", "return_and_pull", "stay"], "ask"), "ask", "nonsense is not a policy");
  assert.equal(choiceOf(RESOLVED, "git.merge_strategy", ["merge", "squash", "rebase"], "merge"), "merge");
  assert.equal(choiceOf([{ key: "k", value: 3 }], "k", ["a"], "a"), "a", "a number is not a word");
});

test("a boolean is a boolean, and anything else is the fallback", () => {
  assert.equal(boolOf(RESOLVED, "git.delete_branch_after_merge", true), false);
  assert.equal(boolOf(RESOLVED, "git.pull", true), true, "a word is not a boolean");
  assert.equal(boolOf([], "git.delete_branch_after_merge", true), true);
});

test("a number is a finite number inside its bounds; a string that parses counts, anything else falls back", () => {
  const rows = [
    { key: "editor.font_size", value: 16 },
    { key: "editor.line_height", value: "1.6" },
    { key: "editor.minimap", value: true },
    { key: "terminal.font_size", value: 99 },
  ];
  assert.equal(numberOf(rows, "editor.font_size", 14), 16);
  assert.equal(numberOf(rows, "editor.line_height", 1.5), 1.6, "the raw panel writes what was typed");
  assert.equal(numberOf(rows, "editor.minimap", 14), 14, "a boolean is not a number");
  assert.equal(numberOf(rows, "terminal.font_size", 14, { min: 8, max: 32 }), 32, "clamped to the bound");
  assert.equal(numberOf(rows, "nothing", 7), 7);
  assert.equal(numberOf(null, "editor.font_size", 14), 14);
});

test("a text key reads trimmed, and anything that is not a string is the fallback", () => {
  assert.equal(stringOf([{ key: "k", value: " https://x " }], "k", "d"), "https://x");
  assert.equal(stringOf([{ key: "k", value: 3 }], "k", "d"), "d", "a number is not text");
  assert.equal(stringOf(null, "k", "d"), "d");
  assert.equal(stringOf([{ key: "k", value: null }], "k", "d"), "d", "null is nothing");
  assert.equal(stringOf([{ key: "k", value: "" }], "k", "d"), "", "empty is a value: it turns a read off");
});
