import test from "node:test";
import assert from "node:assert/strict";

import { caretLabel, countWords, cpuLabel, diskLabel, gpuLabel, languageLabel, memLabel, portOwnerWord } from "./statusBarModel.mjs";
import { readFileSync } from "node:fs";

test("cpu reads as a whole percent, a missing one as a dash", () => {
  assert.equal(cpuLabel(34.6), "35%");
  assert.equal(cpuLabel(0), "0%");
  assert.equal(cpuLabel(null), "—");
  assert.equal(cpuLabel(Number.NaN), "—");
});

test("memory reads used over total in one unit", () => {
  const GB = 1024 ** 3;
  assert.equal(memLabel(9.4 * GB, 16 * GB), "9.4 / 16 GB");
  assert.equal(memLabel(2 * GB, 16 * GB), "2.0 / 16 GB");
  assert.equal(memLabel(null, 16 * GB), "—");
  assert.equal(memLabel(1, 0), "—");
});

test("gpu reads as a whole percent or a dash without a reader; disk is a size or a dash", () => {
  assert.equal(gpuLabel({ util_percent: 29.6 }), "30%");
  assert.equal(gpuLabel(null), "—", "no reader on this machine is a dash, not a zero");
  assert.equal(diskLabel(1.2 * 1024 ** 3), "1.2 GB");
  assert.equal(diskLabel(0), "0 B");
  assert.equal(diskLabel(null), "—");
});

test("the caret reads as line and column, the language by name or as plain text, and no editor as nothing", () => {
  assert.equal(caretLabel({ line: 12, column: 4 }), "Ln 12, Col 4");
  assert.equal(caretLabel(null), "");
  assert.equal(languageLabel({ language: "rust" }), "rust");
  assert.equal(languageLabel({ language: null }), "plain text");
  assert.equal(languageLabel(null), "");
});

test("a count trigger says its number in its name — the popover's name stands in for what it draws — one and many in their own words", () => {
  assert.equal(countWords("terminals", 1), "1 open terminal");
  assert.equal(countWords("terminals", 3), "3 open terminals");
  assert.equal(countWords("harnesses", 0), "0 running harnesses");
  assert.equal(countWords("harnesses", 1), "1 running harness");
  assert.equal(countWords("ports", 2), "2 open ports");
  assert.equal(countWords("ports", Number.NaN), "0 open ports", "a count that is no count is none");
  const bar = readFileSync(new URL("./StatusBar.tsx", import.meta.url), "utf8");
  for (const what of ["terminals", "harnesses", "ports"]) assert.ok(bar.includes(`countWords("${what}"`), `the ${what} trigger is named by its count`);
});

test("a port group is headed by its owner's kind in words, never the wire's enum", () => {
  assert.deepEqual(["goal", "project", "workstream", "work_item", "shell", "harness"].map(portOwnerWord), ["Goal", "Project", "Workstream", "Work item", "Shell", "Harness"]);
  assert.equal(portOwnerWord("future_kind"), "future kind", "a kind this build does not know is said as spelled");
  assert.equal(portOwnerWord(null), "");
  const bar = readFileSync(new URL("./StatusBar.tsx", import.meta.url), "utf8");
  assert.ok(bar.includes("portOwnerWord(g.kind)") && !/>\{g\.kind\}</.test(bar), "the footer draws the word");
  assert.ok(!/"a session"|: "shell"|: "session"/.test(bar), "no English word is spelt in the footer");
});
